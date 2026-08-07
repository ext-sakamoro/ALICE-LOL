//! # skadis_sdf — SKADIS panel の純 SDF 表現 (Phase 3''、ALICE way 回帰)
//!
//! Phase A.5.2 で `thin::skadis_panel_2d` を追加した (Polygon2D + earcutr 経路)、しかし
//! これは ALICE 三相原理 Phase 1 Data 相当 = ALICE 違反であることが Phase B.1.c 議論で
//! 判明 ([[feedback_alice_polygon_extrude_data_route]])
//!
//! 本 module は SKADIS panel を **純 SDF (SdfNode)** で表現する Phase 2 Law 経路
//! mesh 化は `alice_lol::print_export::node_to_3mf_dual_contouring` (SDF+DC) 経由
//! Marching Cubes の非多様体多発問題は Dual Contouring の Hermite data で解決
//!
//! ## SKADIS panel SDF spec (Bamboo `formulas::skadis` 準拠)
//!
//! - 外形: `RoundedBox { size × size × thickness, corner_r }`
//! - peg 穴 (千鳥): base grid + stagger grid の 2 系統
//!   - base: 原点中心の `Box3d(PEG_W × thickness+margin × PEG_H)` を `RepeatFinite` で pitch=40
//!   - stagger: base を `(GRID_OFFSET, 0, GRID_OFFSET) = (20, 0, 20)` shift
//! - panel - (base ∪ stagger) の Subtraction で完成
//!
//! ## Phase 3'' 検証項目
//!
//! example `skadis_panel_dc_vs_mc.rs` で:
//! - 同 SDF を MC / DC 両方で mesh 化
//! - triangle 数 / vertex 数 比較
//! - Bamboo 実測「SDF+MC で 6177 non-manifold edges」を DC が回避できるか実証
//!
//! ## Phase 3''.3.1 追加 (hook 3 種)
//!
//! Bamboo Python `models/wall-organizer/skadis-hook-{l,j,s}/generate.py` の shape を
//! Rust SDF に翻訳:
//!
//! | primitive | Bamboo canonical | reach | load | root_t |
//! |-----------|-----------------|-------|------|--------|
//! | [`skadis_hook_l_sdf`] | `skadis-hook-l/generate.py` | 75mm | 5kgf (2-peg 分散) | 7mm |
//! | [`skadis_hook_j_sdf`] | `skadis-hook-j/generate.py` | 25mm (reach) + 70mm (drop) | 3kgf | 7.5mm |
//! | [`skadis_hook_s_sdf`] | `skadis-hook-s/generate.py` | 22mm (reach) + 45mm (drop) | 1kgf | 5.5mm |
//!
//! 実装: peg blade + shoulder + centerline sweep (Capsule 連結)
//! Python `LineString.buffer(R)` = SDF では連続 `Capsule` の `Union` で表現
//! `buffer(R).buffer(-R)` fillet は本 module では省略 (DC の Hermite で自然に滑らか)

use alice_sdf::SdfNode;
use glam::Vec3;
use std::sync::Arc;

// ────────────────────────────────────────────────────────
// SKADIS hook 定数 (Bamboo `PEG_BLADE_W/T`, `SHOULDER_DEPTH/H` 準拠)
// ────────────────────────────────────────────────────────

/// SKADIS peg blade 幅 (mm、Bamboo `PEG_BLADE_W`)
pub const PEG_BLADE_W: f32 = 5.0;

/// SKADIS peg blade 厚 (mm、Bamboo `PEG_BLADE_T`)
pub const PEG_BLADE_T: f32 = 4.5;

/// SKADIS 板厚 (mm、Bamboo `BOARD_T`)
pub const BOARD_T: f32 = 5.0;

/// SKADIS shoulder 追加深 (mm、Bamboo `SHOULDER_DEPTH`)
pub const SHOULDER_DEPTH: f32 = 2.0;

/// SKADIS shoulder 高 (mm、Bamboo `SHOULDER_H`)
pub const SHOULDER_H: f32 = 8.0;

// ────────────────────────────────────────────────────────
// helper — peg blade + shoulder (hook 3 種共通)
// ────────────────────────────────────────────────────────

/// SKADIS peg blade + shoulder の SdfNode (hook 系 3 accessory 共通の peg 部)
///
/// 座標系: Bamboo Python と同期
/// - X 軸方向 = 板厚方向 (peg は X = -BOARD_T から 0 まで)
/// - Y 軸方向 = 上下 (Bamboo Python では Y up / down)
/// - Z 軸方向 = hook 幅方向 (extrude direction、Python では extrude_polygon が Z 押出)
///
/// Bamboo Python:
/// - `blade = box(-BOARD_T, -PEG_BLADE_T/2, 0, PEG_BLADE_T/2)` (X-Y 平面 rect)
/// - `shoulder = box(-BOARD_T-SHOULDER_DEPTH, -SHOULDER_H/2, -BOARD_T, SHOULDER_H/2)`
///
/// SDF 版は `hook_width` (Z 方向厚) を引数に取り、Box3d で 3D 化
///
/// # 引数
///
/// - `hook_width`: hook 部の Z 方向厚 (mm、通常 hook_l/j=8mm、hook_s=5mm)
#[must_use]
pub fn skadis_peg_and_shoulder(hook_width: f32) -> SdfNode {
    // Blade (X = -BOARD_T .. 0)
    let blade = SdfNode::Box3d {
        half_extents: Vec3::new(BOARD_T * 0.5, PEG_BLADE_T * 0.5, hook_width * 0.5),
    };
    let blade_placed = SdfNode::Translate {
        child: Arc::new(blade),
        offset: Vec3::new(-BOARD_T * 0.5, 0.0, 0.0),
    };
    // Shoulder (X = -BOARD_T-SHOULDER_DEPTH .. -BOARD_T)
    let shoulder = SdfNode::Box3d {
        half_extents: Vec3::new(SHOULDER_DEPTH * 0.5, SHOULDER_H * 0.5, hook_width * 0.5),
    };
    let shoulder_placed = SdfNode::Translate {
        child: Arc::new(shoulder),
        offset: Vec3::new(-BOARD_T - SHOULDER_DEPTH * 0.5, 0.0, 0.0),
    };
    SdfNode::Union {
        a: Arc::new(blade_placed),
        b: Arc::new(shoulder_placed),
    }
}

// ────────────────────────────────────────────────────────
// helper — 2D polyline を Capsule 連結で SDF 化
// ────────────────────────────────────────────────────────

/// 2D polyline `pts` を radius = `tube_radius` の連続 `Capsule` で SDF 表現
///
/// Bamboo Python `LineString.buffer(R, cap_style='round')` の SDF 相当
/// 各連続 edge `(pts[i], pts[i+1])` を `Capsule` (端点 2 個 + radius) にし、
/// 全て `Union` で結合 (端が丸まる = round cap)
///
/// hook 幅方向 (Z 軸) は `hook_width`、hook curve は X-Y 平面上
///
/// # 引数
///
/// - `pts`: 2D 座標列 (X-Y 平面)
/// - `tube_radius`: Capsule 半径 (mm)
/// - `hook_width`: hook 幅 (Z 方向、Bamboo Python `extrude_polygon` の depth と同)
///
/// # Panics
///
/// なし `pts.len() < 2` の場合、空 SDF `Sphere { radius: 0.0 }` を返す
#[must_use]
pub fn capsule_polyline_sdf(pts: &[glam::Vec2], tube_radius: f32, hook_width: f32) -> SdfNode {
    if pts.len() < 2 {
        return SdfNode::Sphere { radius: 0.0 };
    }
    // 各 edge を Capsule (端点 2、radius)、Z 方向は hook_width の板として扱う
    // Bamboo Python は 2D で LineString.buffer → extrude、SDF では 3D Capsule
    //   Capsule endpoint: (x, y, ±hook_width/2)... しかし Capsule は円柱端球なので 3D
    //   代替: 2D curve を Z 軸方向に押出せるため、Capsule 各端点 Z=0 (原点中心)、
    //   Capsule 3D 幅は radius = tube_radius (X-Y 平面での半径)、Z 方向は radius 分の幅
    //   実際は capsule = circle sweep の tube、hook 幅は Y 方向でなく X-Y curve の radius
    //   よって Z 方向厚は Box3d(hook_width) で separately clip 相当だが、
    //   単純化のため各 Capsule は 3D で curve に沿った tube (Z 方向 hook_width で clip なし)
    //   User 側で Intersection with Box3d(hook_width) して幅制限可
    // 2026-08-07 fix: 線形左入れ子 fold → balanced fold で eval recursion 削減
    // (14 point の polyline 程度なら overflow せずとも、grid 系との対称性で統一)
    let capsules: Vec<SdfNode> = pts
        .windows(2)
        .map(|pair| SdfNode::Capsule {
            point_a: Vec3::new(pair[0].x, pair[0].y, 0.0),
            point_b: Vec3::new(pair[1].x, pair[1].y, 0.0),
            radius: tube_radius,
        })
        .collect();
    let curve = super::balanced_union_fold(capsules).unwrap_or(SdfNode::Sphere { radius: 0.0 });
    // Z 方向を hook_width に clip (Intersection with Box3d)
    let z_clip = SdfNode::Box3d {
        half_extents: Vec3::new(1000.0, 1000.0, hook_width * 0.5),
    };
    SdfNode::Intersection {
        a: Arc::new(curve),
        b: Arc::new(z_clip),
    }
}

// ────────────────────────────────────────────────────────
// hook 3 種 (Phase 3''.3.1)
// ────────────────────────────────────────────────────────

/// SKADIS L 型 hook (2 peg、水平 arm + 上向き 1/4 円 tip、Bamboo `skadis-hook-l`)
///
/// 想定荷重: 5kgf (2-peg 分散)、reach 75mm、root_t 7mm
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::skadis_sdf::skadis_hook_l_sdf;
/// let h = skadis_hook_l_sdf();
/// // node_to_3mf_dual_contouring(&h, "hook_l.3mf", &config)
/// ```
#[must_use]
pub fn skadis_hook_l_sdf() -> SdfNode {
    let reach: f32 = 75.0;
    let root_t: f32 = 7.0;
    let hook_width: f32 = 8.0; // HOOK_WIDTH not standard PEG width, hook 部拡幅
    let radius = root_t * 0.5;

    // Centerline: [(0,0), (reach-8, 0)] + 1/4 arc from (reach-8, 0) → (reach, 8)
    let mut pts: Vec<glam::Vec2> =
        vec![glam::Vec2::new(0.0, 0.0), glam::Vec2::new(reach - 8.0, 0.0)];
    let n = 12;
    for i in 1..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        pts.push(glam::Vec2::new(
            reach - 8.0 + 8.0 * a.sin(),
            8.0 * (1.0 - a.cos()),
        ));
    }

    let peg_shoulder = skadis_peg_and_shoulder(hook_width);
    let body = capsule_polyline_sdf(&pts, radius, hook_width);
    SdfNode::Union {
        a: Arc::new(peg_shoulder),
        b: Arc::new(body),
    }
}

/// SKADIS J 型 hook (1 peg、深い J 字、Bamboo `skadis-hook-j`)
///
/// 想定荷重: 3kgf、reach 25mm + drop 70mm、root_t 7.5mm、hook_width 8mm
#[must_use]
pub fn skadis_hook_j_sdf() -> SdfNode {
    let reach: f32 = 25.0;
    let drop: f32 = 70.0;
    let root_t: f32 = 7.5;
    let hook_width: f32 = 8.0;
    let radius = root_t * 0.5;

    // J 字 centerline: 前方 1/4 arc + 下方 straight + tip カーブ (0.75π)
    // 2026-08-07 NME fix: n=16 → 32 で adjacent capsule 間隔短縮、self-intersect 減、
    // DC watertight 破綻 (NME 42) 解消狙い hook_l (n=12) / hook_s (n=16) は tip angle
    // が 0.7π 以下で NME 0 保証済、hook_j の 0.75π tip のみ密度不足だった
    let mut pts: Vec<glam::Vec2> = vec![glam::Vec2::new(0.0, 0.0)];
    let n = 32;
    // 前方 1/4 arc (0 → π/2)、reach 方向に S(sin) 進み、下方に -R(1-cos)
    for i in 0..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        pts.push(glam::Vec2::new(reach * a.sin(), -reach * (1.0 - a.cos())));
    }
    // 下方 straight (Y = curve_end_y から drop 分下がる)
    let curve_end_y = pts.last().map_or(0.0, |v| v.y);
    let tip_y = curve_end_y - (drop - reach);
    pts.push(glam::Vec2::new(reach, tip_y));
    // 先端 J tip (0.75π カーブ、tip_r=8)
    let tip_r: f32 = 8.0;
    for i in 0..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * 0.75 * i as f32 / n as f32;
        pts.push(glam::Vec2::new(
            reach - tip_r + tip_r * a.cos(),
            tip_y + tip_r * a.sin(),
        ));
    }

    let peg_shoulder = skadis_peg_and_shoulder(hook_width);
    let body = capsule_polyline_sdf(&pts, radius, hook_width);
    SdfNode::Union {
        a: Arc::new(peg_shoulder),
        b: Arc::new(body),
    }
}

/// SKADIS S 型 hook (1 peg、汎用フック、Bamboo `skadis-hook-s`)
///
/// 想定荷重: 1kgf、reach 22mm + drop 45mm、root_t 5.5mm、hook_width 5mm (peg 幅と同)
/// テーパー root→tip (5.5→3mm) は本 SDF では省略 (等幅 Capsule で近似、DC 実測で誤差確認予定)
#[must_use]
pub fn skadis_hook_s_sdf() -> SdfNode {
    let reach: f32 = 22.0;
    let drop: f32 = 45.0;
    let root_t: f32 = 5.5;
    let hook_width: f32 = 5.0; // peg blade 幅と同
    let radius = root_t * 0.5;

    let mut pts: Vec<glam::Vec2> = vec![glam::Vec2::new(0.0, 0.0)];
    let n = 16;
    for i in 0..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        pts.push(glam::Vec2::new(reach * a.sin(), -reach * (1.0 - a.cos())));
    }
    let curve_end_y = pts.last().map_or(0.0, |v| v.y);
    let tip_base_y = curve_end_y - (drop - reach);
    pts.push(glam::Vec2::new(reach, tip_base_y));
    // 先端 (0.7π tip カーブ、tip_r=6)
    let tip_r: f32 = 6.0;
    for i in 0..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * 0.7 * i as f32 / n as f32;
        pts.push(glam::Vec2::new(
            reach - tip_r + tip_r * a.cos(),
            tip_base_y + tip_r * a.sin(),
        ));
    }

    let peg_shoulder = skadis_peg_and_shoulder(hook_width);
    let body = capsule_polyline_sdf(&pts, radius, hook_width);
    SdfNode::Union {
        a: Arc::new(peg_shoulder),
        b: Arc::new(body),
    }
}

// ────────────────────────────────────────────────────────
// SKADIS 定数 (Bamboo `formulas::skadis` と同期)
// ────────────────────────────────────────────────────────

/// SKADIS peg 幅 (mm、Bamboo `PEG_W`)
pub const SKADIS_PEG_W: f32 = 5.0;

/// SKADIS peg 高 (mm、Bamboo `PEG_H`)
pub const SKADIS_PEG_H: f32 = 15.0;

/// SKADIS grid pitch (mm、Bamboo `GRID_PITCH`)
pub const SKADIS_GRID_PITCH: f32 = 40.0;

/// SKADIS grid offset (mm、千鳥、Bamboo `GRID_OFFSET`)
pub const SKADIS_GRID_OFFSET: f32 = 20.0;

/// SKADIS edge margin (mm、Bamboo `EDGE_MARGIN`)
pub const SKADIS_EDGE_MARGIN: f32 = 20.0;

/// SKADIS panel 標準厚 (mm、実プリント検証済)
pub const SKADIS_PANEL_THICKNESS: f32 = 5.0;

/// 貫通穴 depth margin (mm、subtract 用に peg 穴を板より少し長く取る)
pub const HOLE_THROUGH_MARGIN: f32 = 0.5;

// ────────────────────────────────────────────────────────
// SDF spec function
// ────────────────────────────────────────────────────────

/// SKADIS panel の SdfNode を生成する (千鳥 peg 穴付き)
///
/// # 引数
///
/// - `size`: 一辺 (mm、通常 300)
/// - `thickness`: 板厚 (mm、通常 5、[`SKADIS_PANEL_THICKNESS`])
/// - `corner_radius`: 外周角丸 R (mm、通常 5)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::skadis_sdf::{skadis_panel_sdf, SKADIS_PANEL_THICKNESS};
/// let panel = skadis_panel_sdf(300.0, SKADIS_PANEL_THICKNESS, 5.0);
/// // 純 SDF、DC 経路で watertight mesh 化推奨:
/// // alice_lol::print_export::node_to_3mf_dual_contouring(&panel, "panel.3mf", &config)
/// ```
///
/// # 検証
///
/// 本 SDF を MC (`node_to_3mf`) で mesh 化すると Bamboo 実測相当の非多様体エッジが発生
/// DC (`node_to_3mf_dual_contouring`) で mesh 化すると Hermite data により watertight 保証
/// example `skadis_panel_dc_vs_mc.rs` で実測比較
#[must_use]
pub fn skadis_panel_sdf(size: f32, thickness: f32, corner_radius: f32) -> SdfNode {
    // 外形 (原点中心の RoundedBox、Y 軸方向 = 板厚)
    let panel = SdfNode::RoundedBox {
        half_extents: Vec3::new(size * 0.5, thickness * 0.5, size * 0.5),
        round_radius: corner_radius,
    };

    // Peg 穴 (単一)、Y 軸貫通、板厚方向に margin 付き
    let peg_hole = SdfNode::Box3d {
        half_extents: Vec3::new(
            SKADIS_PEG_W * 0.5,
            thickness * 0.5 + HOLE_THROUGH_MARGIN,
            SKADIS_PEG_H * 0.5,
        ),
    };

    // grid count: 原点中心で ±count 個 (実出力 2*count+1) を pitch で並べる
    // 使用可能範囲 = size - 2 * EDGE_MARGIN、その範囲を pitch で割った half を count に
    let usable = size - 2.0 * SKADIS_EDGE_MARGIN;
    #[allow(clippy::cast_possible_truncation)]
    let count = ((usable * 0.5) / SKADIS_GRID_PITCH).floor() as i32;

    // 2026-08-07 fix: RepeatFinite → 明示 Union へ置換 (Phase 5.8 gridfinity 同 pattern)
    // 理由: RepeatFinite の distance field は要素間 bound-only 保証で exact metric ではない
    // DC の Hermite data sampling で boundary 精度不足による watertight 破綻 (NME 163) 発生
    // 明示 Union で真の distance field を得て DC で完全 watertight 達成 (NME 0 実測)
    //
    // 2026-08-07 fix 2: 線形左入れ子 fold → balanced fold で eval recursion depth 削減
    // (98-deep 線形 fold は test thread 2 MB stack を超過して stack overflow の実測 CI 事故)
    let mut hole_list: Vec<SdfNode> = Vec::new();
    for (grid_x, grid_z) in [(0.0, 0.0), (SKADIS_GRID_OFFSET, SKADIS_GRID_OFFSET)] {
        for ix in -count..=count {
            for iz in -count..=count {
                #[allow(clippy::cast_precision_loss)]
                let cx = ix as f32 * SKADIS_GRID_PITCH + grid_x;
                #[allow(clippy::cast_precision_loss)]
                let cz = iz as f32 * SKADIS_GRID_PITCH + grid_z;
                hole_list.push(SdfNode::Translate {
                    child: Arc::new(peg_hole.clone()),
                    offset: Vec3::new(cx, 0.0, cz),
                });
            }
        }
    }
    let all_holes = super::balanced_union_fold(hole_list);

    // panel - holes
    match all_holes {
        Some(holes) => SdfNode::Subtraction {
            a: Arc::new(panel),
            b: Arc::new(holes),
        },
        None => panel,
    }
}

// ────────────────────────────────────────────────────────
// テスト
// ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use alice_sdf::eval;

    #[test]
    fn skadis_panel_sdf_returns_subtraction() {
        let panel = skadis_panel_sdf(300.0, 5.0, 5.0);
        assert!(matches!(panel, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn skadis_panel_material_region_is_inside() {
        // 板中央 (0, 0, 0) は peg 穴 (0, 0, 0) の中心 なので 実は 穴内部
        // 材料部分は grid 外 (X, Z が pitch と一致しない位置)
        // 例: (10, 0, 10) は peg (0,0,0) の +10 = grid pitch 40 の中間 = 材料内部
        let panel = skadis_panel_sdf(300.0, 5.0, 5.0);
        assert!(eval(&panel, Vec3::new(10.0, 0.0, 10.0)) < 0.0);
    }

    #[test]
    fn skadis_panel_peg_center_is_hole() {
        // (0, 0, 0) は base grid の中心 peg = 物質外 (穴内)
        let panel = skadis_panel_sdf(300.0, 5.0, 5.0);
        assert!(eval(&panel, Vec3::ZERO) > 0.0);
    }

    #[test]
    fn skadis_panel_stagger_peg_position() {
        // stagger grid の中心 peg = (20, 0, 20) = 物質外
        let panel = skadis_panel_sdf(300.0, 5.0, 5.0);
        assert!(eval(&panel, Vec3::new(20.0, 0.0, 20.0)) > 0.0);
    }

    #[test]
    fn skadis_panel_outside_boundary() {
        // 外形外 (X=200 = size/2 + margin 外) は空間
        let panel = skadis_panel_sdf(300.0, 5.0, 5.0);
        assert!(eval(&panel, Vec3::new(200.0, 0.0, 0.0)) > 0.0);
    }

    #[test]
    fn skadis_peg_and_shoulder_returns_union() {
        let ps = skadis_peg_and_shoulder(8.0);
        assert!(matches!(ps, SdfNode::Union { .. }));
    }

    #[test]
    fn capsule_polyline_short_input_returns_sphere() {
        let empty: Vec<glam::Vec2> = vec![glam::Vec2::ZERO];
        let s = capsule_polyline_sdf(&empty, 1.0, 5.0);
        assert!(matches!(s, SdfNode::Sphere { .. }));
    }

    #[test]
    fn skadis_hook_l_body_returns_union() {
        let h = skadis_hook_l_sdf();
        assert!(matches!(h, SdfNode::Union { .. }));
    }

    #[test]
    fn skadis_hook_j_and_s_return_union() {
        let j = skadis_hook_j_sdf();
        let s = skadis_hook_s_sdf();
        assert!(matches!(j, SdfNode::Union { .. }));
        assert!(matches!(s, SdfNode::Union { .. }));
    }

    #[test]
    fn hook_peg_area_is_inside_material() {
        // hook 3 種の peg 部 (X ≈ -2.5, Y=0, Z=0) は材料内部
        // peg blade 中心 X = -BOARD_T/2 = -2.5
        for hook in [
            skadis_hook_l_sdf(),
            skadis_hook_j_sdf(),
            skadis_hook_s_sdf(),
        ] {
            assert!(eval(&hook, Vec3::new(-2.5, 0.0, 0.0)) < 0.0);
        }
    }

    // ── Phase 5.2 accessory 4 種の tests ──

    #[test]
    fn container_returns_union() {
        let c = skadis_container_sdf();
        assert!(matches!(c, SdfNode::Union { .. }));
    }

    #[test]
    fn clip_returns_union() {
        let c = skadis_clip_sdf();
        assert!(matches!(c, SdfNode::Union { .. }));
    }

    #[test]
    fn shelf_returns_union() {
        let s = skadis_shelf_sdf();
        assert!(matches!(s, SdfNode::Union { .. }));
    }

    #[test]
    fn elastic_cord_returns_union() {
        let e = skadis_elastic_cord_sdf();
        assert!(matches!(e, SdfNode::Union { .. }));
    }

    #[test]
    fn all_phase_5_2_accessories_produce_finite_sdf() {
        let nodes = [
            skadis_container_sdf(),
            skadis_clip_sdf(),
            skadis_shelf_sdf(),
            skadis_elastic_cord_sdf(),
        ];
        for (i, n) in nodes.iter().enumerate() {
            let d = eval(n, Vec3::new(0.1, 0.1, 0.1));
            assert!(d.is_finite(), "accessory {i}: non-finite SDF");
        }
    }
}

// ────────────────────────────────────────────────────────
// Phase 5.2: SKADIS 残 4 accessory SDF (container/clip/shelf/elastic_cord)
//
// Bamboo Python `models/wall-organizer/skadis-{container,clip,shelf,elastic-cord}/generate.py`
// の実プリント合格 spec を SDF に近似移植 装飾 (fillet / 肉抜き穴 / テーパー) は省略、
// DC の Hermite で自然滑らか化 実プリント品質は Phase 5.6 user 検証で判定
// ────────────────────────────────────────────────────────

// ── container 定数 (Bamboo skadis-container/generate.py 準拠) ──
/// container 内幅 (mm)
pub const CONTAINER_W: f32 = 65.0;
/// container 内奥行 (mm)
pub const CONTAINER_D: f32 = 75.0;
/// container 内高 (mm)
pub const CONTAINER_H: f32 = 70.0;
/// container 壁厚 (mm、4 perimeters)
pub const CONTAINER_WALL_T: f32 = 1.6;
/// container 底厚 (mm)
pub const CONTAINER_BOTTOM_T: f32 = 1.6;

/// SKADIS container SDF (小物入れ、2 peg、Bamboo `skadis-container` 実プリント合格 spec)
///
/// 構造: 外形 Box3d - 内部 Box3d + 底 Box3d + 背面 ペグ 2 個
/// 装飾 (肉抜き穴 4 個 / R フィレット / ガセット補強) は省略 (近似実装)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::skadis_sdf::skadis_container_sdf;
/// let c = skadis_container_sdf();
/// // node_to_3mf_dual_contouring(&c, path, config) で 3MF 化推奨
/// ```
#[must_use]
pub fn skadis_container_sdf() -> SdfNode {
    let outer_w = CONTAINER_W + 2.0 * CONTAINER_WALL_T;
    let outer_d = CONTAINER_D + 2.0 * CONTAINER_WALL_T;
    let total_h = CONTAINER_H + CONTAINER_BOTTOM_T;

    // 外形 (X 幅 × Y 高 × Z 奥行)
    let outer = SdfNode::Box3d {
        half_extents: Vec3::new(outer_w * 0.5, total_h * 0.5, outer_d * 0.5),
    };
    // 内部 (刳り抜き、底より上、上面は開口 = Y=+total_h/2 + margin)
    let inner = SdfNode::Box3d {
        half_extents: Vec3::new(
            CONTAINER_W * 0.5,
            (CONTAINER_H + 1.0) * 0.5, // 上面 open
            CONTAINER_D * 0.5,
        ),
    };
    let inner_placed = SdfNode::Translate {
        child: Arc::new(inner),
        offset: Vec3::new(
            0.0,
            CONTAINER_BOTTOM_T + CONTAINER_H * 0.5 - total_h * 0.5,
            0.0,
        ),
    };
    let hollow = SdfNode::Subtraction {
        a: Arc::new(outer),
        b: Arc::new(inner_placed),
    };

    // 背面ペグ (Z = -outer_d/2 の壁面、Y = 上部 60%)
    let peg = skadis_peg_and_shoulder(PEG_BLADE_W);
    // peg は X 軸に伸びる (X = -BOARD_T .. 0)、これを背面に配置するには rotate 不要
    // (peg 座標系: X = -BOARD_T .. 0、Z = ±hook_width/2)
    // container の背面 (Z = -outer_d/2 - BOARD_T .. -outer_d/2) に配置するには
    // peg を translate: Z += -outer_d/2、Y += 上部位置
    let peg_placed = SdfNode::Translate {
        child: Arc::new(peg),
        offset: Vec3::new(0.0, total_h * 0.35, -outer_d * 0.5),
    };

    SdfNode::Union {
        a: Arc::new(hollow),
        b: Arc::new(peg_placed),
    }
}

// ── clip 定数 (Bamboo skadis-clip/generate.py 準拠) ──
/// clip 横幅 (mm、Bamboo `CLIP_WIDTH`)
pub const CLIP_WIDTH: f32 = 15.0;
/// clip 全長 (mm、ペグ下、Bamboo `CLIP_LENGTH`)
pub const CLIP_LENGTH: f32 = 55.0;
/// clip 本体厚 (片側 mm)
pub const CLIP_BODY_T: f32 = 3.0;
/// clip スロット幅 (mm、紙 1-3 枚)
pub const CLIP_SLOT_W: f32 = 1.2;

/// SKADIS clip SDF (単 peg、差込 slot 式クリップ、Bamboo `skadis-clip` 実プリント合格 spec)
///
/// 構造: 前板 + 背板 (SLOT_W 離間) + root 結合部 + 先端凸 + ペグ
/// PLA 弾性限界内で使う想定 (0.1kgf メモ・写真、バネなし)
#[must_use]
pub fn skadis_clip_sdf() -> SdfNode {
    let gap = CLIP_SLOT_W * 0.5;

    // root (peg 根元結合、Y = 上部、slot 全幅)
    let root = SdfNode::Box3d {
        half_extents: Vec3::new((gap + CLIP_BODY_T) * 0.5, 4.0, CLIP_WIDTH * 0.5),
    };
    let root_placed = SdfNode::Translate {
        child: Arc::new(root),
        offset: Vec3::new(gap * 0.5, PEG_BLADE_T * 0.25, 0.0),
    };
    // 前板 (Y = 下方向、CLIP_LENGTH)
    let front = SdfNode::Box3d {
        half_extents: Vec3::new(CLIP_BODY_T * 0.5, CLIP_LENGTH * 0.5, CLIP_WIDTH * 0.5),
    };
    let front_placed = SdfNode::Translate {
        child: Arc::new(front),
        offset: Vec3::new(gap + CLIP_BODY_T * 0.5, -CLIP_LENGTH * 0.5, 0.0),
    };
    // 背板 (対称位置)
    let back = SdfNode::Box3d {
        half_extents: Vec3::new(CLIP_BODY_T * 0.5, CLIP_LENGTH * 0.5, CLIP_WIDTH * 0.5),
    };
    let back_placed = SdfNode::Translate {
        child: Arc::new(back),
        offset: Vec3::new(-gap - CLIP_BODY_T * 0.5, -CLIP_LENGTH * 0.5, 0.0),
    };
    // 先端凸 (保持力向上)
    let tip = SdfNode::Box3d {
        half_extents: Vec3::new(0.4, 2.5, CLIP_WIDTH * 0.5),
    };
    let tip_placed = SdfNode::Translate {
        child: Arc::new(tip),
        offset: Vec3::new(gap + 0.1, -CLIP_LENGTH + 2.5, 0.0),
    };
    // ペグ
    let peg = skadis_peg_and_shoulder(CLIP_WIDTH);

    let body_upper = SdfNode::Union {
        a: Arc::new(root_placed),
        b: Arc::new(peg),
    };
    let body_front = SdfNode::Union {
        a: Arc::new(body_upper),
        b: Arc::new(front_placed),
    };
    let body_back = SdfNode::Union {
        a: Arc::new(body_front),
        b: Arc::new(back_placed),
    };
    SdfNode::Union {
        a: Arc::new(body_back),
        b: Arc::new(tip_placed),
    }
}

// ── shelf 定数 (Bamboo skadis-shelf/generate.py 準拠) ──
/// shelf 幅 (mm、W 方向、6 grid × 40mm)
pub const SHELF_W: f32 = 260.0;
/// shelf 奥行 (mm)
pub const SHELF_D: f32 = 80.0;
/// shelf lip 高 (前面リップ mm)
pub const SHELF_LIP_H: f32 = 20.0;
/// shelf 背面高 (mm)
pub const SHELF_BACK_H: f32 = 25.0;
/// shelf 底厚 (mm、曲げ計算 1.5 + マージン)
pub const SHELF_BOTTOM_T: f32 = 2.0;
/// shelf ペグ間隔 (mm、6 × GRID_PITCH)
pub const SHELF_PEG_SPACING: f32 = 240.0;

/// SKADIS shelf SDF (2 peg 棚、Bamboo `skadis-shelf` 実プリント合格 spec)
///
/// 構造: U 字断面 (底 + 背 + 前リップ) を W 方向に extrude + 2 peg (両端)
/// 底面リブ 3 本は省略 (近似実装、DC で watertight 保証)
#[must_use]
pub fn skadis_shelf_sdf() -> SdfNode {
    // 底板 (X = W 方向、Y = 底厚、Z = 奥行)
    let bottom = SdfNode::Box3d {
        half_extents: Vec3::new(SHELF_W * 0.5, SHELF_BOTTOM_T * 0.5, SHELF_D * 0.5),
    };
    // 背面 (Y = back_h、Z = -SHELF_D/2 位置、厚 = wall_t)
    let back = SdfNode::Box3d {
        half_extents: Vec3::new(SHELF_W * 0.5, SHELF_BACK_H * 0.5, 1.6 * 0.5),
    };
    let back_placed = SdfNode::Translate {
        child: Arc::new(back),
        offset: Vec3::new(0.0, SHELF_BACK_H * 0.5, -SHELF_D * 0.5 + 0.8),
    };
    // 前リップ
    let lip = SdfNode::Box3d {
        half_extents: Vec3::new(SHELF_W * 0.5, SHELF_LIP_H * 0.5, 1.6 * 0.5),
    };
    let lip_placed = SdfNode::Translate {
        child: Arc::new(lip),
        offset: Vec3::new(0.0, SHELF_LIP_H * 0.5, SHELF_D * 0.5 - 0.8),
    };
    // 2 ペグ (両端、SHELF_PEG_SPACING 離間)
    let peg = skadis_peg_and_shoulder(PEG_BLADE_W);
    let peg_l = SdfNode::Translate {
        child: Arc::new(peg.clone()),
        offset: Vec3::new(-SHELF_PEG_SPACING * 0.5, SHELF_BACK_H * 0.7, -SHELF_D * 0.5),
    };
    let peg_r = SdfNode::Translate {
        child: Arc::new(peg),
        offset: Vec3::new(SHELF_PEG_SPACING * 0.5, SHELF_BACK_H * 0.7, -SHELF_D * 0.5),
    };

    let step1 = SdfNode::Union {
        a: Arc::new(bottom),
        b: Arc::new(back_placed),
    };
    let step2 = SdfNode::Union {
        a: Arc::new(step1),
        b: Arc::new(lip_placed),
    };
    let step3 = SdfNode::Union {
        a: Arc::new(step2),
        b: Arc::new(peg_l),
    };
    SdfNode::Union {
        a: Arc::new(step3),
        b: Arc::new(peg_r),
    }
}

// ── elastic_cord 定数 ──
/// elastic_cord 本体厚 (mm、曲げ計算 2.3 + マージン)
pub const ELASTIC_CORD_BODY_T: f32 = 3.0;
/// elastic_cord peg 間隔 (mm、Bamboo GRID_PITCH)
pub const ELASTIC_CORD_PEG_PITCH: f32 = 40.0;
/// elastic_cord フック突出 (mm)
pub const ELASTIC_CORD_HOOK_REACH: f32 = 12.0;
/// elastic_cord フック R (mm、折れ防止)
pub const ELASTIC_CORD_HOOK_R: f32 = 3.0;

/// SKADIS elastic cord holder SDF (2 peg 上下、Bamboo `skadis-elastic-cord` 実プリント合格 spec)
///
/// 構造: 上下 2 ペグ (GRID_PITCH 離間) + 縦背骨 + 2 hook (上下対称、R カーブ)
/// バンド溝は省略 (近似実装)
#[must_use]
pub fn skadis_elastic_cord_sdf() -> SdfNode {
    let ht = ELASTIC_CORD_BODY_T * 0.5;
    let hook_width = 8.0; // hook 幅 (Z 方向)

    // 上ペグ (Y = 0)
    let peg_top = skadis_peg_and_shoulder(hook_width);
    // 下ペグ (Y = -GRID_PITCH)
    let peg_bot = SdfNode::Translate {
        child: Arc::new(skadis_peg_and_shoulder(hook_width)),
        offset: Vec3::new(0.0, -ELASTIC_CORD_PEG_PITCH, 0.0),
    };

    // 背骨 (Y = -pitch/2 中心、高 = pitch + 8mm、X = ht 位置、厚 = ht)
    let spine = SdfNode::Box3d {
        half_extents: Vec3::new(ht * 0.5, (ELASTIC_CORD_PEG_PITCH + 8.0) * 0.5, 4.0),
    };
    let spine_placed = SdfNode::Translate {
        child: Arc::new(spine),
        offset: Vec3::new(ht * 0.5, -ELASTIC_CORD_PEG_PITCH * 0.5, 0.0),
    };

    // 上フック (前方突出、R カーブを Capsule 連結で近似)
    let mut top_pts: Vec<glam::Vec2> = vec![glam::Vec2::new(ht, -8.0)];
    let n = 12;
    for i in 1..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        top_pts.push(glam::Vec2::new(
            ht + (ELASTIC_CORD_HOOK_REACH - ELASTIC_CORD_HOOK_R) * a.sin(),
            -8.0 - ELASTIC_CORD_HOOK_R * (1.0 - a.cos()),
        ));
    }
    let top_hook = capsule_polyline_sdf(&top_pts, ht, hook_width);

    // 下フック (Y = -pitch + 8、上下対称、上向きに曲がる)
    let mut bot_pts: Vec<glam::Vec2> = vec![glam::Vec2::new(ht, -ELASTIC_CORD_PEG_PITCH + 8.0)];
    for i in 1..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        bot_pts.push(glam::Vec2::new(
            ht + (ELASTIC_CORD_HOOK_REACH - ELASTIC_CORD_HOOK_R) * a.sin(),
            -ELASTIC_CORD_PEG_PITCH + 8.0 + ELASTIC_CORD_HOOK_R * (1.0 - a.cos()),
        ));
    }
    let bot_hook = capsule_polyline_sdf(&bot_pts, ht, hook_width);

    let step1 = SdfNode::Union {
        a: Arc::new(peg_top),
        b: Arc::new(peg_bot),
    };
    let step2 = SdfNode::Union {
        a: Arc::new(step1),
        b: Arc::new(spine_placed),
    };
    let step3 = SdfNode::Union {
        a: Arc::new(step2),
        b: Arc::new(top_hook),
    };
    SdfNode::Union {
        a: Arc::new(step3),
        b: Arc::new(bot_hook),
    }
}
