//! `roblox_export` の閉形式突合 oracle (feature `roblox`、`required-features` で有効時のみ build)
//!
//! `roblox_export` はこれまで test が 1 本も無く、CI の test matrix にも feature `roblox` を
//! 有効にする entry が無かった (clippy / msrv の `--all-features` は compile だけ)
//!
//! 独立の参照は次のとおり
//! - 検証 (`validate_for_roblox`): 手で組んだ mesh での上限の境界 (三角形数 / 各軸のサイズは `<=`)、
//!   面積ゼロの三角形の判定
//! - 生成 (`node_to_mesh_roblox`): 球 `sphere(r)` の体積 4/3 π (r·s)³ と bbox 2·r·s、
//!   水密 (全エッジがちょうど 2 枚に共有される)、外向き (符号付き体積が正)、スケールの線形性
//! - 出力 (`node_to_obj_roblox` / `node_to_fbx_roblox`): OBJ を **独立に読み戻した**頂点数・面数・座標が
//!   stats と一致する
//! - 異常系: 空 mesh は `EmptyMesh`、パース失敗は `Parse`、書き込み失敗は `Io`、
//!   退化した設定で panic しない

use alice_lol::print_export::{ExportError, Mesh, Vertex};
use alice_lol::roblox_export::{
    lol_to_fbx_roblox, lol_to_obj_roblox, node_to_fbx_roblox, node_to_mesh_roblox,
    node_to_obj_roblox, validate_for_roblox, RobloxConfig,
};
use alice_lol::runtime_parser::parse_lol;
use alice_lol::SdfNode;
use glam::Vec3;
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};

// ── 手で組む mesh / 独立の参照計算 ──

/// 標準ライブラリだけの一時ディレクトリ (drop で消す、`tempfile` を dev-dependency に足さない)
struct Tmp(std::path::PathBuf);

impl Tmp {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "alice_lol_roblox_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn node(lol: &str) -> SdfNode {
    parse_lol(lol).unwrap_or_else(|e| panic!("parse {lol}: {e}"))
}

fn mesh_of(triangles: &[[Vec3; 3]]) -> Mesh {
    let mut m = Mesh::new();
    for t in triangles {
        let base = u32::try_from(m.vertices.len()).expect("small mesh");
        for &p in t {
            m.vertices.push(Vertex::new(p, Vec3::Z));
        }
        m.indices.extend([base, base + 1, base + 2]);
    }
    m
}

/// 面積 0.5 の直角三角形 (退化していない) を `n` 個、互いに離して置く
fn healthy_triangles(n: usize) -> Mesh {
    let tris: Vec<[Vec3; 3]> = (0..n)
        .map(|i| {
            let o = Vec3::new(f32::from(u16::try_from(i).expect("small")) * 3.0, 0.0, 0.0);
            [o, o + Vec3::X, o + Vec3::Y]
        })
        .collect();
    mesh_of(&tris)
}

/// 発散定理による符号付き体積 (外向き CCW なら正)
fn signed_volume(m: &Mesh) -> f64 {
    m.indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| {
            let p = |k: usize| m.vertices[t[k] as usize].position.as_dvec3();
            p(0).dot(p(1).cross(p(2))) / 6.0
        })
        .sum()
}

/// (境界エッジ数, 非多様体エッジ数)  閉じた多様体では全エッジがちょうど 2 枚に共有される
fn edge_defects(m: &Mesh) -> (usize, usize) {
    let mut e: HashMap<(u32, u32), u32> = HashMap::new();
    for t in m.indices.as_chunks::<3>().0 {
        for (u, v) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *e.entry((u.min(v), u.max(v))).or_insert(0) += 1;
        }
    }
    (
        e.values().filter(|&&n| n == 1).count(),
        e.values().filter(|&&n| n > 2).count(),
    )
}

fn extent(m: &Mesh) -> Vec3 {
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in &m.vertices {
        lo = lo.min(v.position);
        hi = hi.max(v.position);
    }
    hi - lo
}

// ── 検証 (validate_for_roblox) ──

#[test]
fn triangle_limit_is_inclusive() {
    let cfg = RobloxConfig::accessory().with_max_triangles(3);
    let at = validate_for_roblox(&healthy_triangles(3), &cfg);
    assert!(at.is_within_triangle_limit, "3 枚 / 上限 3 は範囲内 (`<=`)");
    assert_eq!(at.triangle_count, 3);
    let over = validate_for_roblox(&healthy_triangles(4), &cfg);
    assert!(!over.is_within_triangle_limit, "4 枚 / 上限 3 は超過");
    assert!(!over.is_valid());
}

#[test]
fn size_limit_is_checked_per_axis_and_is_inclusive() {
    let cfg = RobloxConfig::accessory().with_max_size_studs(Vec3::new(10.0, 8.0, 6.0));
    let boxed = |s: Vec3| {
        mesh_of(&[[
            Vec3::ZERO,
            Vec3::new(s.x, 0.0, 0.0),
            Vec3::new(0.0, s.y, s.z),
        ]])
    };
    // ちょうど上限 (各軸 `<=`)
    let at = validate_for_roblox(&boxed(Vec3::new(10.0, 8.0, 6.0)), &cfg);
    assert!(at.is_within_size_limit, "{at}");
    assert_eq!(at.bounds_studs, Vec3::new(10.0, 8.0, 6.0));
    // 1 軸だけ超えても超過 (x / y / z それぞれ)
    for (axis, over) in [
        ("x", Vec3::new(10.5, 8.0, 6.0)),
        ("y", Vec3::new(10.0, 8.5, 6.0)),
        ("z", Vec3::new(10.0, 8.0, 6.5)),
    ] {
        let v = validate_for_roblox(&boxed(over), &cfg);
        assert!(
            !v.is_within_size_limit,
            "{axis} だけ超過でも不合格のはず: {v}"
        );
    }
}

#[test]
fn degenerate_faces_are_zero_area_triangles_only() {
    let cfg = RobloxConfig::accessory();
    let flagged = |tri: [Vec3; 3]| validate_for_roblox(&mesh_of(&[tri]), &cfg).has_degenerate_faces;
    // 面積 0: 共線 / 3 頂点が一致
    assert!(
        flagged([Vec3::ZERO, Vec3::X, Vec3::new(2.0, 0.0, 0.0)]),
        "共線"
    );
    assert!(flagged([Vec3::ONE, Vec3::ONE, Vec3::ONE]), "3 頂点が一致");
    // 面積 0.5 の直角三角形は退化していない
    assert!(!flagged([Vec3::ZERO, Vec3::X, Vec3::Y]));
    // 細いが面積のある三角形 (面積 1e-3) は退化ではない (許容が大きすぎる変異の検出)
    assert!(!flagged([
        Vec3::ZERO,
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 2e-3, 0.0)
    ]));
    // 1 枚でも退化していれば全体が退化あり
    let mut m = healthy_triangles(2);
    m.vertices
        .extend([Vec3::ZERO; 3].map(|p| Vertex::new(p, Vec3::Z)));
    m.indices.extend([6, 7, 8]);
    assert!(validate_for_roblox(&m, &cfg).has_degenerate_faces);
}

#[test]
fn is_valid_requires_every_check_and_display_reports_each_failure() {
    let cfg = RobloxConfig::accessory().with_max_triangles(2);
    let ok = validate_for_roblox(&healthy_triangles(2), &cfg);
    assert!(ok.is_valid());
    let shown = ok.to_string();
    assert!(shown.starts_with("[PASS]"), "{shown}");
    assert!(
        !shown.contains("OVER") && !shown.contains("DEGENERATE"),
        "{shown}"
    );

    let over_tri = validate_for_roblox(&healthy_triangles(3), &cfg);
    assert!(!over_tri.is_valid());
    assert!(over_tri.to_string().contains("[OVER TRI LIMIT]"));

    let big = RobloxConfig::accessory().with_max_size_studs(Vec3::splat(1.0));
    let over_size = validate_for_roblox(&healthy_triangles(2), &big);
    assert!(!over_size.is_valid());
    assert!(over_size.to_string().contains("[OVER SIZE LIMIT]"));

    let degenerate = validate_for_roblox(&mesh_of(&[[Vec3::ONE; 3]]), &RobloxConfig::accessory());
    assert!(!degenerate.is_valid());
    let shown = degenerate.to_string();
    assert!(
        shown.starts_with("[FAIL]") && shown.contains("[DEGENERATE FACES]"),
        "{shown}"
    );
}

#[test]
fn an_empty_mesh_validates_as_empty_and_valid() {
    let v = validate_for_roblox(&Mesh::new(), &RobloxConfig::accessory());
    assert_eq!((v.triangle_count, v.vertex_count), (0, 0));
    assert_eq!(v.bounds_studs, Vec3::ZERO);
    assert!(v.is_valid());
}

// ── 生成 (node_to_mesh_roblox): 球の閉形式 ──

#[test]
fn sphere_mesh_matches_the_closed_form_for_every_preset() {
    let sphere = node("sphere(1.0)");
    for (name, cfg) in [
        ("accessory", RobloxConfig::accessory()),
        ("meshpart", RobloxConfig::meshpart()),
        ("preview", RobloxConfig::preview()),
    ] {
        let mesh = node_to_mesh_roblox(&sphere, &cfg);
        // 半径 1 (SDF) x scale_studs 2 = 半径 2 stud
        let want_vol = 4.0 / 3.0 * std::f64::consts::PI * 8.0;
        let vol = signed_volume(&mesh);
        assert!(vol > 0.0, "{name}: 符号付き体積が負 = 内向き ({vol})");
        assert!(
            (vol - want_vol).abs() / want_vol < 0.05,
            "{name}: 体積 {vol:.3} が 4/3π(2)³ = {want_vol:.3} から外れた"
        );
        let e = extent(&mesh);
        assert!(
            (e - Vec3::splat(4.0)).abs().max_element() < 0.2,
            "{name}: bbox {e:?} が直径 4 stud から外れた"
        );
    }
}

#[test]
fn sphere_mesh_is_watertight_for_every_preset() {
    // `repair_all` の頂点マージ許容量が cell 幅に対して大きいと、水密な mesh に穴を開ける
    // (print_export は cell 幅比 0.04 に直した 2026-09-30、roblox_export は固定 5e-3 のまま)
    let sphere = node("sphere(1.0)");
    let mut failures = Vec::new();
    for (name, cfg) in [
        ("accessory", RobloxConfig::accessory()),
        ("meshpart", RobloxConfig::meshpart()),
        ("preview", RobloxConfig::preview()),
    ] {
        let mesh = node_to_mesh_roblox(&sphere, &cfg);
        let (boundary, non_manifold) = edge_defects(&mesh);
        if (boundary, non_manifold) != (0, 0) {
            failures.push(format!(
                "{name}: 境界エッジ {boundary} 非多様体 {non_manifold} ({} 三角形)",
                mesh.indices.len() / 3
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "水密でない preset:\n{}",
        failures.join("\n")
    );
}

/// 各 preset の `max_triangles` は Roblox の UGC 制約 (accessory 4,000 / `MeshPart` 10,000)
/// `estimate_resolution` は「上限を超えないよう解像度を下げる」ためにあるので、最も単純な形状
/// (球) で自身の上限を超える preset は役目を果たしていない
#[test]
fn presets_keep_a_plain_sphere_within_their_triangle_limit() {
    let sphere = node("sphere(1.0)");
    let mut failures = Vec::new();
    for (name, cfg) in [
        ("accessory", RobloxConfig::accessory()),
        ("meshpart", RobloxConfig::meshpart()),
        ("preview", RobloxConfig::preview()),
    ] {
        let mesh = node_to_mesh_roblox(&sphere, &cfg);
        let v = validate_for_roblox(&mesh, &cfg);
        if !v.is_within_triangle_limit {
            failures.push(format!(
                "{name}: {} 三角形が上限 {} を超える",
                v.triangle_count, cfg.max_triangles
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "上限を守れない preset:\n{}",
        failures.join("\n")
    );
}

/// 三角形上限を緩めて高解像度 (128 / 192) を許すと、`repair_all` の頂点マージ許容量が
/// cell 幅に対して大きいと水密な mesh に穴が開く (固定 5e-3 は res 128 で cell の 16%、
/// 実測の境界エッジ 741)  上限で解像度が下がる既定のプリセットでは、この欠陥は表に出ない
#[test]
fn a_high_resolution_sphere_stays_watertight() {
    let sphere = node("sphere(1.0)");
    let mut failures = Vec::new();
    for resolution in [64_usize, 128, 192] {
        let cfg = RobloxConfig {
            resolution,
            ..RobloxConfig::accessory()
        }
        .with_max_triangles(10_000_000);
        let mesh = node_to_mesh_roblox(&sphere, &cfg);
        let (boundary, non_manifold) = edge_defects(&mesh);
        if (boundary, non_manifold) != (0, 0) {
            failures.push(format!(
                "res {resolution}: 境界エッジ {boundary} 非多様体 {non_manifold} ({} 三角形)",
                mesh.indices.len() / 3
            ));
        }
        // 解像度を上限としてそのまま使えている (三角形上限が緩いので下がらない)
        assert!(
            mesh.indices.len() / 3 > 4000 * (resolution / 64),
            "res {resolution} が使われていない"
        );
    }
    assert!(
        failures.is_empty(),
        "高解像度で水密でない:\n{}",
        failures.join("\n")
    );
}

/// 球以外 (トーラス / 箱 / smooth union) でも、上限を守り水密であること
/// 上限は解像度から経験則で決めるのではなく、実測の三角形数で守る
#[test]
fn presets_hold_the_limit_and_stay_watertight_for_several_shapes() {
    let shapes = [
        ("torus", "torus(1.0, 0.3)"),
        ("box", "box3d(0.8, 0.8, 0.8)"),
        (
            "smooth_union",
            "smooth_union(0.3, sphere(0.8), translate(0.0, 0.9, 0.0, sphere(0.5)))",
        ),
    ];
    let mut failures = Vec::new();
    for (shape, lol) in shapes {
        let n = node(lol);
        for (name, cfg) in [
            ("accessory", RobloxConfig::accessory()),
            ("meshpart", RobloxConfig::meshpart()),
            ("preview", RobloxConfig::preview()),
        ] {
            let mesh = node_to_mesh_roblox(&n, &cfg);
            let tris = mesh.indices.len() / 3;
            let (boundary, non_manifold) = edge_defects(&mesh);
            if tris > cfg.max_triangles || tris == 0 || (boundary, non_manifold) != (0, 0) {
                failures.push(format!(
                    "{shape}/{name}: {tris} 三角形 (上限 {}) 境界エッジ {boundary} 非多様体 {non_manifold}",
                    cfg.max_triangles
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "上限 / 水密を守れない:\n{}",
        failures.join("\n")
    );
}

/// 細かい特徴 (半径 0.08 の球を 5 x 5 の格子に並べる) は、粗い試し (cell 0.125) ではほとんど見えず、
/// 解像度を上げると急に三角形が増える (解像度の 2 乗比例の見積もりが外れる)  上限は見積もりではなく
/// **実測の三角形数**で守らなければならない
#[test]
fn the_limit_is_enforced_by_measurement_when_fine_features_defeat_the_estimate() {
    let mut parts = Vec::new();
    for ix in 0..5_u8 {
        for iz in 0..5_u8 {
            let (x, z) = (
                0.6_f32.mul_add(f32::from(ix), -1.2),
                0.6_f32.mul_add(f32::from(iz), -1.2),
            );
            parts.push(format!("translate({x:.2}, 0.0, {z:.2}, sphere(0.08))"));
        }
    }
    let lol = parts
        .into_iter()
        .reduce(|a, b| format!("union({a}, {b})"))
        .expect("25 parts");
    let n = node(&lol);
    let mut failures = Vec::new();
    for (name, cfg) in [
        ("accessory", RobloxConfig::accessory()),
        ("meshpart", RobloxConfig::meshpart()),
        ("preview", RobloxConfig::preview()),
    ] {
        let tris = node_to_mesh_roblox(&n, &cfg).indices.len() / 3;
        if tris > cfg.max_triangles {
            failures.push(format!(
                "{name}: {tris} 三角形が上限 {} を超える",
                cfg.max_triangles
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "見積もりが外れたとき上限を守れない:\n{}",
        failures.join("\n")
    );
}

/// `preview` は「高速プレビュー向け」(doc)  accessory より粗い (三角形が少ない) mesh でなければならない
#[test]
fn the_preview_preset_is_coarser_than_accessory() {
    let sphere = node("sphere(1.0)");
    let tris = |cfg: &RobloxConfig| node_to_mesh_roblox(&sphere, cfg).indices.len() / 3;
    let (preview, accessory) = (
        tris(&RobloxConfig::preview()),
        tris(&RobloxConfig::accessory()),
    );
    assert!(
        preview < accessory,
        "preview {preview} 三角形 >= accessory {accessory} 三角形 (preview が速くなっていない)"
    );
}

#[test]
fn stud_scaling_is_linear_in_scale_studs() {
    let sphere = node("sphere(1.0)");
    let base = node_to_mesh_roblox(&sphere, &RobloxConfig::accessory().with_scale_studs(1.0));
    for s in [0.5_f32, 2.0, 3.0] {
        let scaled = node_to_mesh_roblox(&sphere, &RobloxConfig::accessory().with_scale_studs(s));
        assert_eq!(scaled.vertices.len(), base.vertices.len(), "scale {s}");
        assert_eq!(scaled.indices, base.indices, "scale {s}: 位相は不変のはず");
        for (a, b) in scaled.vertices.iter().zip(&base.vertices) {
            let want = b.position * s;
            assert!(
                (a.position - want).abs().max_element() <= 1e-5 * s.max(1.0),
                "scale {s}: {:?} != {s} * {:?}",
                a.position,
                b.position
            );
        }
    }
}

// ── 出力 (OBJ / FBX): 独立パーサで読み戻す ──

/// OBJ を独立に読み戻す (頂点位置, 面の頂点 index (0 始まり))
fn read_obj(path: &std::path::Path) -> (Vec<Vec3>, Vec<[usize; 3]>) {
    let text = std::fs::read_to_string(path).expect("read obj");
    let (mut verts, mut faces) = (Vec::new(), Vec::new());
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("v ") {
            let f: Vec<f32> = rest
                .split_whitespace()
                .map(|x| x.parse().expect("f32"))
                .collect();
            verts.push(Vec3::new(f[0], f[1], f[2]));
        } else if let Some(rest) = line.strip_prefix("f ") {
            let idx: Vec<usize> = rest
                .split_whitespace()
                .map(|tok| {
                    tok.split('/')
                        .next()
                        .expect("index")
                        .parse::<usize>()
                        .expect("usize")
                        - 1
                })
                .collect();
            assert_eq!(idx.len(), 3, "三角形のみのはず: {line}");
            faces.push([idx[0], idx[1], idx[2]]);
        }
    }
    (verts, faces)
}

#[test]
fn obj_export_round_trips_to_the_stats_and_the_scaled_geometry() {
    let dir = Tmp::new();
    let path = dir.path().join("ball.obj");
    let cfg = RobloxConfig::accessory();
    let stats = lol_to_obj_roblox("sphere(1.0)", &path, &cfg).expect("export");
    let (verts, faces) = read_obj(&path);
    assert_eq!(verts.len(), stats.vertex_count);
    assert_eq!(faces.len(), stats.triangle_count);
    assert!(
        faces.iter().flatten().all(|&i| i < verts.len()),
        "index が範囲外"
    );
    // 読み戻した座標の bbox == stats.bounds_studs (OBJ の text 表記の丸めのみ許容)
    let (lo, hi) = verts
        .iter()
        .fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |a, v| {
            (a.0.min(*v), a.1.max(*v))
        });
    assert!(((hi - lo) - stats.bounds_studs).abs().max_element() < 1e-4);
    // 直接生成した mesh と一致する (決定論)
    let direct = node_to_mesh_roblox(&node("sphere(1.0)"), &cfg);
    assert_eq!(direct.vertices.len(), stats.vertex_count);
    assert_eq!(direct.indices.len() / 3, stats.triangle_count);
    assert_eq!(stats.path, path.display().to_string());
    assert_eq!(stats.validation.triangle_count, stats.triangle_count);
}

#[test]
fn fbx_export_writes_a_file_with_the_same_stats_as_obj() {
    let dir = Tmp::new();
    let cfg = RobloxConfig::accessory();
    let obj =
        node_to_obj_roblox(&node("sphere(1.0)"), dir.path().join("a.obj"), &cfg).expect("obj");
    let fbx_path = dir.path().join("a.fbx");
    let fbx = node_to_fbx_roblox(&node("sphere(1.0)"), &fbx_path, &cfg).expect("fbx");
    assert_eq!(
        (fbx.vertex_count, fbx.triangle_count),
        (obj.vertex_count, obj.triangle_count),
        "同じ mesh を別形式で出しているだけ"
    );
    let bytes = std::fs::read(&fbx_path).expect("read fbx");
    assert!(bytes.len() > 100, "FBX が空同然: {} bytes", bytes.len());
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]).to_string();
    assert!(
        head.contains("FBX"),
        "FBX の magic / ヘッダが無い: {head:?}"
    );
    let via_lol =
        lol_to_fbx_roblox("sphere(1.0)", dir.path().join("b.fbx"), &cfg).expect("lol fbx");
    assert_eq!(via_lol.triangle_count, fbx.triangle_count);
}

// ── 制約違反は警告 (エラーにしない)、異常系 ──

#[test]
fn a_constraint_violation_is_reported_in_the_stats_not_raised_as_an_error() {
    let dir = Tmp::new();
    let cfg = RobloxConfig::accessory().with_max_triangles(10);
    let stats = node_to_obj_roblox(&node("sphere(1.0)"), dir.path().join("s.obj"), &cfg)
        .expect("上限超過でも出力は成功する (doc: エラーにはしない)");
    assert!(stats.triangle_count > 10);
    assert!(!stats.validation.is_within_triangle_limit);
    assert!(!stats.validation.is_valid());
}

#[test]
fn error_paths_are_typed() {
    let dir = Tmp::new();
    let cfg = RobloxConfig::accessory();
    // bounds (±2) の外にある形状は空 mesh
    let outside = node("translate(10.0, 0.0, 0.0, sphere(1.0))");
    assert!(matches!(
        node_to_obj_roblox(&outside, dir.path().join("e.obj"), &cfg),
        Err(ExportError::EmptyMesh)
    ));
    assert!(matches!(
        node_to_fbx_roblox(&outside, dir.path().join("e.fbx"), &cfg),
        Err(ExportError::EmptyMesh)
    ));
    // 空 mesh のときファイルを作らない
    assert!(!dir.path().join("e.obj").exists());
    // パース失敗
    assert!(matches!(
        lol_to_obj_roblox("sphere(", dir.path().join("p.obj"), &cfg),
        Err(ExportError::Parse(_))
    ));
    // 書き込み失敗
    let missing = dir.path().join("no_such_dir").join("x.obj");
    assert!(matches!(
        node_to_obj_roblox(&node("sphere(1.0)"), missing, &cfg),
        Err(ExportError::Io(_))
    ));
}

#[test]
fn degenerate_configs_do_not_panic() {
    let sphere = node("sphere(1.0)");
    let configs: Vec<(&str, RobloxConfig)> = vec![
        (
            "resolution 0",
            RobloxConfig {
                resolution: 0,
                ..RobloxConfig::accessory()
            },
        ),
        (
            "max_triangles 0",
            RobloxConfig::accessory().with_max_triangles(0),
        ),
        (
            "bounds 反転",
            RobloxConfig::accessory().with_bounds(Vec3::splat(2.0), Vec3::splat(-2.0)),
        ),
        (
            "bounds 厚み 0",
            RobloxConfig::accessory().with_bounds(Vec3::ZERO, Vec3::ZERO),
        ),
        (
            "bounds NaN",
            RobloxConfig::accessory().with_bounds(Vec3::splat(f32::NAN), Vec3::splat(f32::NAN)),
        ),
        ("scale 0", RobloxConfig::accessory().with_scale_studs(0.0)),
        ("scale 負", RobloxConfig::accessory().with_scale_studs(-2.0)),
        (
            "scale 巨大",
            RobloxConfig::accessory().with_scale_studs(1.0e30),
        ),
        (
            "size 上限 NaN",
            RobloxConfig::accessory().with_max_size_studs(Vec3::splat(f32::NAN)),
        ),
    ];
    for (name, cfg) in configs {
        let r = catch_unwind(AssertUnwindSafe(|| {
            let mesh = node_to_mesh_roblox(&sphere, &cfg);
            validate_for_roblox(&mesh, &cfg)
        }));
        assert!(r.is_ok(), "{name}: panic した");
    }
    // scale 0 は全頂点が原点に潰れるので、退化面ありと判定されなければならない
    let mesh = node_to_mesh_roblox(&sphere, &RobloxConfig::accessory().with_scale_studs(0.0));
    let v = validate_for_roblox(&mesh, &RobloxConfig::accessory());
    assert!(
        v.has_degenerate_faces && !v.is_valid(),
        "scale 0 の mesh は不合格のはず: {v}"
    );
}
