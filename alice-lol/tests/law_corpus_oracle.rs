//! 判定器 (`law::check_laws`) を **総当たり反証器** と突き合わせる corpus oracle
//!
//! `tests/analytic_law.rs` は解析解が手計算できる 16 scene を見る 本 test が
//! 足すのは **grammar corpus 全 construct** (LOL の入口 `parse_lol` から到達
//! できる形すべて) に対する 2 方向の検査:
//!
//! - **false green**: 総当たり grid が「両方の内部にある点」を実際に見つけた
//!   のに、判定器が `all_passed()`(= 違反なし かつ 未決定なし = 証明付き合格)
//!   を返したら誤り 未決定 (`Unresolved`) は合格ではないので許容する
//! - **false red**: 判定器が hard violation を報告したのに、報告された点の
//!   近傍を総当たりしても重なりの証拠が無ければ誤り
//!
//! どちらも「判定器の出力を pin する」golden ではなく、**独立実装 (区間演算を
//! 使わない素の点評価) との突合**なので、区間演算側の緩み / 締めすぎの両方が
//! 露出する 併せて未決定率 (判定器がどれだけ「証明できていない」か) を
//! 実測して print する — 未決定は不合格側に倒れるので安全だが、多ければ
//! 判定器が実用になっていない
//!
//! 起票: 2026-09-27 LOL 品質 gate 棚卸し (区間包含の健全性は alice-sdf 側
//! `tests/test_interval_soundness.rs` が corpus 全 variant で見る、本 test は
//! その上に載る law 判定の健全性を見る)

// grid 生成の index → 座標変換 (値域は高々 32、f32 の仮数に収まる)
// grid の index ⇄ 座標変換に限った許容: 値域は高々 32 で f32 の仮数に収まり、
// `rel` は AABB 内に clamp した後なので負にも上限超えにもならない 単文字 binding
// (n / x / y / z / i) は grid 座標の慣習表記
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]

mod common;

use alice_lol::law::{check_laws, CheckConfig, Constraint, HardVerdict, Law, LawReport};
use alice_lol::runtime_parser::parse_lol;
use alice_lol::{eval, SdfNode};
use common::corpus::{fixtures, grammar_corpus};
use glam::Vec3;

/// 検査範囲 (判定器と総当たりで共通)
const AABB: f32 = 2.0;
/// 総当たり grid の 1 軸分割数 (16³ = 4,096 点 / construct)
///
/// CI は debug build で 3 matrix 走るので、corpus 全 construct を見る幅を
/// 保ったまま 1 construct あたりの仕事量を削る側で調整している
const GRID: usize = 16;
/// 「決定的な重なり」とみなす場の深さ — cell 解像度の言い訳が効かない深さに
/// 限って false green を主張する
const DECISIVE: f32 = 0.05;

/// 判定器の解像度 (cell 辺 1.0) — 未決定率は
/// [`unresolved_rate_is_measured_and_bounded`] が 4 / 8 / 16 で別途測る
const RESOLUTION: usize = 4;

fn cfg() -> CheckConfig {
    CheckConfig {
        aabb_min: Vec3::splat(-AABB),
        aabb_max: Vec3::splat(AABB),
        resolution: RESOLUTION,
    }
}

fn grid_points() -> Vec<Vec3> {
    let mut pts = Vec::with_capacity(GRID * GRID * GRID);
    let step = 2.0 * AABB / (GRID - 1) as f32;
    for i in 0..GRID {
        for j in 0..GRID {
            for k in 0..GRID {
                pts.push(Vec3::new(
                    (i as f32).mul_add(step, -AABB),
                    (j as f32).mul_add(step, -AABB),
                    (k as f32).mul_add(step, -AABB),
                ));
            }
        }
    }
    pts
}

/// 両方の内部にある最も深い点 (`max(f_a, f_b)` が最小) を総当たりで探す
fn overlap_witness(a: &SdfNode, b: &SdfNode, pts: &[Vec3]) -> Option<(Vec3, f32)> {
    let mut best: Option<(Vec3, f32)> = None;
    for p in pts {
        let (fa, fb) = (eval(a, *p), eval(b, *p));
        if !fa.is_finite() || !fb.is_finite() {
            continue;
        }
        let depth = fa.max(fb);
        if depth < 0.0 && best.is_none_or(|(_, d)| depth < d) {
            best = Some((*p, depth));
        }
    }
    best
}

/// `centre` 周り半径 `r` の局所総当たり (false red の裏取り)
fn local_overlap(a: &SdfNode, b: &SdfNode, centre: Vec3, r: f32) -> Option<(Vec3, f32)> {
    let n: u8 = 12;
    let step = 2.0 * r / f32::from(n - 1);
    let mut pts = Vec::with_capacity(usize::from(n) * usize::from(n) * usize::from(n));
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                pts.push(
                    centre
                        + Vec3::new(
                            f32::from(i).mul_add(step, -r),
                            f32::from(j).mul_add(step, -r),
                            f32::from(k).mul_add(step, -r),
                        ),
                );
            }
        }
    }
    overlap_witness(a, b, &pts)
}

/// grammar corpus + 深い合成 fixture を `SdfNode` に
fn nodes() -> Vec<(String, SdfNode)> {
    let mut out = Vec::new();
    for (name, snippet) in grammar_corpus() {
        if let Ok(node) = parse_lol(&snippet) {
            out.push((name, node));
        }
    }
    for (name, snippet) in fixtures() {
        if let Ok(node) = parse_lol(snippet) {
            out.push((format!("fixture:{name}"), node));
        }
    }
    out
}

fn non_overlap(a: SdfNode, b: SdfNode) -> Vec<Law> {
    vec![Law::hard("no_overlap", Constraint::NonOverlap { a, b }).expect("provable constraint")]
}

/// 3 値の導出を `LawReport::hard_verdict` に単一源化する
///
/// 本 file の law は全て `Law::hard` なので、旧実装 (`has_hard_violations` →
/// `has_unresolved` の 2 段) と結果は一致する 差が出るのは Soft を混ぜた時
/// だけで、その時は **Soft の未決定で "unresolved" と報告していた旧実装が
/// Hard の主張としては過剰**だった
fn verdict(report: &LawReport) -> &'static str {
    match report.hard_verdict() {
        HardVerdict::Violated => "violation",
        HardVerdict::Undecided => "unresolved",
        HardVerdict::Proven => "passed",
    }
}

/// 総当たりが重なりを見つけた scene を「証明付き合格」と言ってはいけない
#[test]
fn judge_never_proves_a_pass_the_brute_force_refutes() {
    let pts = grid_points();
    let mut failures = Vec::new();
    let (mut checked, mut skipped) = (0usize, 0usize);
    let (mut violation, mut unresolved) = (0usize, 0usize);

    for (name, node) in nodes() {
        // 自分自身を少しずらすと内部は必ず重なる (feature size ≫ 0.05 の形)
        let shifted = node.clone().translate(0.05, 0.03, 0.04);
        let Some((w, depth)) = overlap_witness(&node, &shifted, &pts) else {
            skipped += 1; // 検査範囲に内部が無い (2D / 空 / 遠方の形)
            continue;
        };
        if depth > -DECISIVE {
            skipped += 1; // 浅すぎる重なりは cell 解像度の範囲、主張しない
            continue;
        }
        checked += 1;
        let report = check_laws(&non_overlap(node, shifted), &cfg());
        match verdict(&report) {
            "violation" => violation += 1,
            "unresolved" => unresolved += 1,
            _ => failures.push(format!(
                "{name}: {w:?} で両方の内部 (深さ {depth:.4}) なのに証明付き合格"
            )),
        }
    }

    eprintln!(
        "false-green probe: {checked} scene (violation {violation} / unresolved {unresolved}), skip {skipped}"
    );
    assert!(checked > 100, "corpus が小さすぎる: {checked}");
    assert!(
        failures.is_empty(),
        "{} 件の証明付き合格が総当たりで反証された:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// hard violation の報告には、その点の近傍に実際の重なりが無ければならない
#[test]
fn reported_violations_have_a_brute_force_witness() {
    let mut failures = Vec::new();
    let (mut checked, mut reported) = (0usize, 0usize);
    // 検査 AABB の外まで離す = 範囲内に重なりは無い
    for (name, node) in nodes() {
        let far = node.clone().translate(6.0, 0.0, 0.0);
        checked += 1;
        let report = check_laws(&non_overlap(node.clone(), far.clone()), &cfg());
        for v in &report.violations {
            reported += 1;
            // 違反の定義は「その点で両方が負」 まず報告点そのものを確かめ、
            // 落ちた時だけ近傍を総当たりして「近くに重なりがあるのに点だけ
            // ずれている」のか「そもそも重なりが無い」のかを分ける
            let (fa, fb) = (eval(&node, v.point), eval(&far, v.point));
            if fa < 0.0 && fb < 0.0 {
                continue;
            }
            // cell 対角 + 余裕
            #[allow(clippy::cast_precision_loss)]
            let r = 2.0 * AABB / RESOLUTION as f32 * 1.74;
            let near = local_overlap(&node, &far, v.point, r);
            failures.push(format!(
                "{name}: {:?} で違反を報告 (残差 {}) したが、その点の値は a={fa} b={fb} — 近傍総当たり: {near:?}",
                v.point, v.residual
            ));
        }
    }
    eprintln!("false-red probe: {checked} scene, 報告された違反 {reported} 件");
    assert!(
        failures.is_empty(),
        "{} 件の違反報告に証拠が無い:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// 判定器が実際に何割「証明できる」のかを実測する (未決定は不合格側なので
/// 安全だが、多ければ判定器が実用になっていない)
#[test]
fn unresolved_rate_is_measured_and_bounded() {
    let mut rows = Vec::new();
    // resolution 16 は cell 4,096 個 × 八分木なので corpus の先頭 40 construct
    // だけで測る (決着率の傾きが見えれば十分、CI 時間を食わせない)
    for (resolution, limit) in [(4usize, usize::MAX), (8, usize::MAX), (16, 40)] {
        let config = CheckConfig {
            aabb_min: Vec3::splat(-AABB),
            aabb_max: Vec3::splat(AABB),
            resolution,
        };
        let (mut passed, mut violated, mut undecided) = (0usize, 0usize, 0usize);
        for (_name, node) in nodes().into_iter().take(limit) {
            let laws = vec![Law::hard(
                "thickness",
                Constraint::MinThickness {
                    node,
                    min_thickness: 0.2,
                },
            )
            .expect("provable constraint")];
            let report = check_laws(&laws, &config);
            match verdict(&report) {
                "violation" => violated += 1,
                "unresolved" => undecided += 1,
                _ => passed += 1,
            }
        }
        let total = passed + violated + undecided;
        #[allow(clippy::cast_precision_loss)]
        let decided = (total - undecided) as f32 / total as f32;
        rows.push((resolution, total, passed, violated, undecided, decided));
    }

    eprintln!("MinThickness(0.2) 判定内訳 — resolution ごと");
    for (res, total, passed, violated, undecided, decided) in &rows {
        eprintln!(
            "  resolution {res:>2}: {total} scene → 合格 {passed} / 違反 {violated} / 未決定 {undecided} (決着率 {:.1}%)",
            decided * 100.0
        );
    }

    // 決着率の下限 — 下回ったら区間が緩んだか探索深さが足りない
    // 実測 87.7% / 92.2% / 100.0% — ⚠️ Lipschitz 包囲を入れても **1 pt も動かない**
    // (`decided_floor` の MinThickness の項参照、内部を問う法則なので外部限定の
    //  契約が使えない) 0.5 → 0.85 に引き上げ
    for (res, _, _, _, _, decided) in rows {
        assert!(
            decided > 0.85,
            "resolution {res} の決着率が {:.1}% しかない (未決定は不合格側なので安全だが、判定器として実用にならない)",
            decided * 100.0
        );
    }
}

/// 勾配上界の法則は、静的上界そのものを上界に渡した時に **corpus のどの
/// construct でも違反を出してはいけない** — 出たら `eval_lipschitz` の主張が
/// 破れているということ (SDF 側の property test と独立な経路での再検査)
#[test]
fn gradient_bound_never_contradicts_the_static_claim() {
    let cfg = cfg();
    let mut checked = 0usize;
    for (name, node) in nodes() {
        let claimed = alice_sdf::interval::eval_lipschitz(&node);
        if !claimed.is_finite() {
            continue; // 上界を主張していない法は対象外
        }
        checked += 1;
        // 上界ぴったりを渡すと静的証明で即合格になるので、標本経路を通すため
        // に僅かに下げた値で「証拠が出ないこと」を見る
        let laws = vec![Law::hard(
            "grad",
            Constraint::GradientBound {
                node,
                max_gradient: claimed * (1.0 + 1e-3),
                probe: 1e-3,
            },
        )
        .expect("provable constraint")];
        let report = check_laws(&laws, &cfg);
        // ⚠️ `!has_hard_violations()` でなく `is_proven()` で見る — 前者は
        // **未決定を合格側に倒す**ので「反例が見つからなかった」と「上界が
        // 証明された」を区別できない (本 test の合格が空振りかどうかが
        // 分からない状態だった)
        assert!(
            report.hard_verdict().is_proven(),
            "{name}: {:?} — sampling found a quotient above the claimed bound {claimed}, or the bound could not be proven",
            report.hard_verdict()
        );
    }
    assert!(checked > 50, "only {checked} constructs claimed a bound");
}

/// 到達不能の「証明」は、点の総当たりで経路が見つかる scene に出してはいけない
/// (false red = 実際は繋がっているのに詰みと判定する)
#[test]
fn proven_unreachability_is_never_refuted_by_a_point_path() {
    let cfg = cfg();
    let pts = grid_points();
    let mut violations = 0usize;
    for (name, node) in nodes() {
        // 内部点を 2 つ拾う (無ければ対象外)
        let inside: Vec<Vec3> = pts
            .iter()
            .copied()
            .filter(|p| eval(&node, *p) < 0.0)
            .collect();
        if inside.len() < 2 {
            continue;
        }
        let (from, to) = (inside[0], inside[inside.len() - 1]);
        let laws = vec![Law::hard(
            "reach",
            Constraint::Reachable {
                node: node.clone(),
                from,
                to,
            },
        )
        .expect("provable constraint")];
        let report = check_laws(&laws, &cfg);
        if !report.has_hard_violations() {
            continue;
        }
        violations += 1;
        // 違反と言った以上、点の総当たり (判定器と独立な素の点評価) でも
        // 6-connected の経路が見つかってはいけない
        let n = cfg.resolution;
        let step = (cfg.aabb_max - cfg.aabb_min) / n as f32;
        let idx = |p: Vec3| -> (usize, usize, usize) {
            let rel = (p - cfg.aabb_min) / step;
            (
                (rel.x as usize).min(n - 1),
                (rel.y as usize).min(n - 1),
                (rel.z as usize).min(n - 1),
            )
        };
        let interior: Vec<bool> = (0..n * n * n)
            .map(|i| {
                let (x, y, z) = (i % n, (i / n) % n, i / (n * n));
                let c = cfg.aabb_min + step * Vec3::new(x as f32, y as f32, z as f32) + step * 0.5;
                eval(&node, c) < 0.0
            })
            .collect();
        let (sx, sy, sz) = idx(from);
        let (tx, ty, tz) = idx(to);
        let mut seen = vec![false; n * n * n];
        let mut q = std::collections::VecDeque::new();
        let s = sx + sy * n + sz * n * n;
        if interior[s] {
            seen[s] = true;
            q.push_back((sx, sy, sz));
        }
        let mut found = false;
        while let Some((x, y, z)) = q.pop_front() {
            if (x, y, z) == (tx, ty, tz) {
                found = true;
                break;
            }
            for (nx, ny, nz) in [
                x.checked_sub(1).map(|v| (v, y, z)),
                (x + 1 < n).then_some((x + 1, y, z)),
                y.checked_sub(1).map(|v| (x, v, z)),
                (y + 1 < n).then_some((x, y + 1, z)),
                z.checked_sub(1).map(|v| (x, y, v)),
                (z + 1 < n).then_some((x, y, z + 1)),
            ]
            .into_iter()
            .flatten()
            {
                let i = nx + ny * n + nz * n * n;
                if !seen[i] && interior[i] {
                    seen[i] = true;
                    q.push_back((nx, ny, nz));
                }
            }
        }
        assert!(
            !found,
            "{name}: judge proved unreachability but a point path exists"
        );
    }
    println!("reachability: {violations} constructs judged unreachable");
}

/// [`unresolved_rate_by_constraint`] の 1 行分 (constraint × resolution)
struct Row {
    res: usize,
    total: usize,
    skipped: usize,
    passed: usize,
    violated: usize,
    undecided: usize,
    decided: f32,
    /// 未決定になった construct の (名前, 静的 Lipschitz 上界)
    ///
    /// 解像度を上げても減らない構造を特定するために名前まで残す
    /// (WM-19b、`NonOverlap` の 21 件が res 4 / 8 で 1 件も動かない事実の追跡)
    undecided_names: Vec<(String, f32)>,
    /// 到達性の解像度が上限に当たった construct の (名前, 測った最小肉厚)
    capped: Vec<(String, f32)>,
    /// 到達性で判定の対象にならなかった construct (外接箱が無い / 内部の点が 2 つ未満)
    skipped_names: Vec<String>,
}

type ConstraintBuilder = Box<dyn Fn(&SdfNode) -> Option<Constraint> + Sync>;

/// `Law::hard` を名乗れる 6 constraint を corpus の各 node に当てる builder 群
///
/// `Stress` / `Thermal` / `Continuity` / `VolumeConservation` / `ThermalField`
/// は `evidence_class()` が `Modelled` を返すので `Law::hard` が構造的に弾く
/// (推定は Hard を名乗れない) ので対象外
///
/// 第 2 node を要するもの (`NonOverlap` / `Containment` / `Contact`) は固定の
/// probe と突き合わせる 率は probe の選び方に依存するので、**絶対値でなく
/// resolution 間の差**を読むこと
fn constraint_builders() -> Vec<(&'static str, ConstraintBuilder)> {
    let probe = parse_lol("sphere(0.5)").expect("probe snippet parses");
    let outer = parse_lol("sphere(4.0)").expect("outer snippet parses");
    let pts = grid_points();
    let probe_for_contact = probe.clone();

    vec![
        (
            "MinThickness(0.2)",
            Box::new(|n: &SdfNode| {
                Some(Constraint::MinThickness {
                    node: n.clone(),
                    min_thickness: 0.2,
                })
            }),
        ),
        (
            "NonOverlap(sphere 0.5)",
            Box::new(move |n: &SdfNode| {
                Some(Constraint::NonOverlap {
                    a: n.clone(),
                    b: probe.clone(),
                })
            }),
        ),
        (
            "Containment(in sphere 4.0)",
            Box::new(move |n: &SdfNode| {
                Some(Constraint::Containment {
                    inner: n.clone(),
                    outer: outer.clone(),
                })
            }),
        ),
        (
            "Contact(0.0..0.5)",
            Box::new(move |n: &SdfNode| {
                Some(Constraint::Contact {
                    a: n.clone(),
                    b: probe_for_contact.clone(),
                    min_distance: 0.0,
                    max_distance: 0.5,
                })
            }),
        ),
        (
            "GradientBound(static+1e-3)",
            Box::new(|n: &SdfNode| {
                let claimed = alice_sdf::interval::eval_lipschitz(n);
                claimed.is_finite().then(|| Constraint::GradientBound {
                    node: n.clone(),
                    max_gradient: claimed * (1.0 + 1e-3),
                    probe: 1e-3,
                })
            }),
        ),
        (
            "Reachable(inside..inside)",
            Box::new(move |n: &SdfNode| {
                // 内部点を 2 つ持つ construct だけが対象
                let inside: Vec<Vec3> = pts.iter().copied().filter(|p| eval(n, *p) < 0.0).collect();
                (inside.len() >= 2).then(|| Constraint::Reachable {
                    node: n.clone(),
                    from: inside[0],
                    to: inside[inside.len() - 1],
                })
            }),
        ),
    ]
}

/// 箱 `lo..hi` の中の最小肉厚: res 96 の mesh の三角形ごとに、表面のすぐ内側で読んだ SDF の勾配の逆向きに測る
/// (面の法線は縁で信用しない) 三角形が無い時は `None`
fn min_wall_in(node: &SdfNode, lo: Vec3, hi: Vec3) -> Option<f32> {
    use alice_sdf::eval::eval_normal;
    use alice_sdf::mesh::{sdf_to_mesh, MarchingCubesConfig};
    use alice_sdf::validity::local_thickness;
    let m = sdf_to_mesh(
        node,
        lo,
        hi,
        &MarchingCubesConfig {
            resolution: 96,
            ..MarchingCubesConfig::default()
        },
    );
    let span = (hi - lo).length();
    let pv = |i: u32| m.vertices[i as usize].position;
    let mut min: Option<f32> = None;
    for t in m.indices.chunks(3) {
        let (a, b, c) = (pv(t[0]), pv(t[1]), pv(t[2]));
        if (b - a).cross(c - a).length_squared() <= 0.0 {
            continue;
        }
        let cen = (a + b + c) / 3.0;
        let inside = cen - eval_normal(node, cen) * (eval(node, cen) + 1e-3);
        let n = eval_normal(node, inside);
        let origin = inside + n * (1e-4 - eval(node, inside));
        if let Some(th) = local_thickness(node, origin, -n, span) {
            min = Some(min.map_or(th, |m: f32| m.min(th)));
        }
    }
    min
}

/// 到達性の判定の解像度の上限 (これを超える construct は [`REACHABLE_RESOLUTION_CAPPED`] に名前を出す)
const REACHABLE_MAX_RESOLUTION: usize = 64;

/// 上限 [`REACHABLE_MAX_RESOLUTION`] に当たる construct と、その最小肉厚 (mm、自身の箱での `min_wall_in` の実測)
///
/// これらは res 64 でもセルが壁の半分より大きく、到達性が決着しないまま残りうる 黙って未決定に
/// 数えないよう名前で固定する (集合が変わったら、どれが増えた / 減ったかを見てから更新する)
const REACHABLE_RESOLUTION_CAPPED: &[(&str, f32)] = &[
    ("skadis_hook_l", 2.0001),
    ("skadis_hook_j", 2.0001),
    ("skadis_container", 1.6001),
    ("skadis_clip", 2.0001),
    ("skadis_shelf", 1.6001),
    ("skadis_elastic_cord", 1.5001),
    ("drawer_organizer", 1.5001),
    ("shelf_divider", 0.0269),
    ("octahedron", 0.1037),
    ("pyramid", 0.0165),
    ("tetrahedron", 3.6438),
    ("dodecahedron", 5.5600),
    ("icosahedron", 6.0001),
    ("truncated_octahedron", 6.0001),
    ("truncated_icosahedron", 6.0001),
    ("infinite_cylinder", 6.0001),
    ("cone", 0.0811),
    ("egg", 0.1024),
    ("diamond", 0.0033),
    ("triangular_prism", 3.4952),
    ("cut_sphere", 0.0779),
    ("neovius", 0.0002),
    ("circle_2d", 4.0001),
    ("blobby_cross", 1.2427),
    ("terrain", 0.0002),
    ("t_slot_bracket_2020", 0.0013),
    ("countersink", 0.0529),
    ("curtain_rod_bracket", 0.0002),
    ("pixhawk_mount", 0.0023),
    ("capped_cone", 0.3591),
    ("death_star", 0.0145),
    ("rect_2d", 2.0001),
    ("parabola_segment", 0.0002),
    ("regular_polygon", 0.0294),
    ("gridfinity_bin", 0.2746),
    ("phone_stand", 0.4001),
    ("headphone_holder", 0.0264),
    ("under_desk_mount", 0.5019),
    ("desk_shelf", 1.0001),
    ("card_tray", 1.5001),
    ("wrench_holder", 3.0001),
    ("sbc_case", 1.0001),
    ("esp32_enclosure", 1.6001),
    ("battery_18650_holder", 0.9115),
    ("drill_bit_holder", 3.0001),
    ("spice_rack", 0.1501),
    ("egg_tray", 3.0001),
    ("build_plate_rack", 0.5783),
    ("cutlery_tray", 3.0001),
    ("pill_organizer", 1.5001),
    ("hairdryer_holder", 3.0016),
    ("hex_key_holder", 3.0195),
    ("sock_divider", 1.0001),
    ("soap_tray", 2.0001),
    ("chopstick_holder", 2.0001),
    ("swatch_holder", 2.0001),
    ("tp_holder", 0.0020),
    ("sd_card_holder", 1.5001),
    ("driver_rack", 3.0001),
    ("sink_caddy", 2.5001),
    ("clamp_rack", 0.4880),
    ("dry_box", 1.1124),
    ("outdoor_enclosure", 0.7501),
    ("phone_dock", 0.0009),
    ("cutting_board_rack", 1.0001),
    ("tape_dispenser", 1.0001),
    ("shower_caddy", 0.0147),
    ("bag_clip_org", 2.5001),
    ("led_hub_box", 0.0164),
    ("makeup_organizer", 2.0001),
    ("vesa_mount", 0.4446),
    ("flange_mount", 0.0184),
    ("dovetail", 0.0612),
    ("helix", 0.1292),
    ("box_frame", 2.0001),
    ("star_polygon", 0.0121),
    ("cross_shape", 6.8942),
    ("chamfered_cube", 0.0633),
    ("trapezoid", 0.1260),
    ("parallelogram", 0.1599),
    ("rounded_rect_2d", 4.0001),
    ("superellipsoid", 0.0010),
    ("l_bracket", 0.0017),
    ("heat_set_array", 0.6004),
    ("flange_circular", 0.0575),
    ("rack_shelf", 0.0268),
    ("gridfinity_bin_ex", 0.9977),
    ("columns_union", 0.0034),
    ("columns_intersection", 0.0135),
    ("groove", 0.0034),
    ("columns_subtraction", 0.0058),
    ("bend", 2.0001),
    ("taper", 0.0197),
    ("displacement", 0.0003),
    ("repeat", 2.0000),
    ("shear", 0.0446),
    ("noise", 0.0439),
    ("surface_roughness", 0.0037),
    ("fixture:sword", 0.0036),
    ("fixture:pen_cup_torus", 2.0001),
    ("fixture:gridfinity_bin_ex", 0.2468),
];

/// 到達性の判定の対象にならない construct (base 解像度 4): 外接箱が無い、または判定の cell の半対角より深い
/// 端の点が 2 つ取れない (2D の形・解像度が上限に当たり壁が cell より薄いもの・薄い殻など) 黙って数から落とさ
/// ないよう名前で固定する (増えても減っても red、どれが動いたかを出す)
const REACHABLE_SKIPPED_RES4: &[&str] = &[
    "skadis_shelf",
    "drawer_organizer",
    "shelf_divider",
    "tetrahedron",
    "dodecahedron",
    "icosahedron",
    "truncated_octahedron",
    "truncated_icosahedron",
    "infinite_cylinder",
    "triangular_prism",
    "gyroid",
    "schwarz_p",
    "diamond_surface",
    "neovius",
    "lidinoid",
    "iwp",
    "frd",
    "fischer_koch_s",
    "pmy",
    "circle_2d",
    "blobby_cross",
    "tube",
    "cut_hollow_sphere",
    "rect_2d",
    "annular_2d",
    "gridfinity_bin",
    "monitor_riser",
    "tissue_box_cover",
    "card_tray",
    "hex_bit_holder",
    "egg_tray",
    "cutlery_tray",
    "pill_organizer",
    "sock_divider",
    "soap_tray",
    "chopstick_holder",
    "swatch_holder",
    "sd_card_holder",
    "sink_caddy",
    "cutting_board_rack",
    "tape_dispenser",
    "shower_caddy",
    "makeup_organizer",
    "pin_hinge_knuckle",
    "box_frame",
    "cross_shape",
    "rounded_rect_2d",
    "segment_2d",
    "triangle",
    "bezier",
    "chamfer_subtraction",
    "exp_smooth_subtraction",
    "bend",
    "taper",
    "repeat",
    "elongate",
    "shear",
    "fixture:lattice_infill",
    "fixture:gridfinity_bin_ex",
];

/// [`REACHABLE_SKIPPED_RES4`] at the base resolution 8 (the endpoint depth follows the cell, so the sets differ)
const REACHABLE_SKIPPED_RES8: &[&str] = &[
    "skadis_shelf",
    "drawer_organizer",
    "shelf_divider",
    "tetrahedron",
    "dodecahedron",
    "icosahedron",
    "truncated_octahedron",
    "truncated_icosahedron",
    "infinite_cylinder",
    "triangular_prism",
    "gyroid",
    "schwarz_p",
    "diamond_surface",
    "neovius",
    "lidinoid",
    "iwp",
    "frd",
    "fischer_koch_s",
    "pmy",
    "circle_2d",
    "blobby_cross",
    "tube",
    "cut_hollow_sphere",
    "rect_2d",
    "annular_2d",
    "gridfinity_bin",
    "monitor_riser",
    "tissue_box_cover",
    "card_tray",
    "hex_bit_holder",
    "egg_tray",
    "cutlery_tray",
    "pill_organizer",
    "sock_divider",
    "soap_tray",
    "chopstick_holder",
    "swatch_holder",
    "sd_card_holder",
    "sink_caddy",
    "cutting_board_rack",
    "tape_dispenser",
    "shower_caddy",
    "makeup_organizer",
    "pin_hinge_knuckle",
    "box_frame",
    "cross_shape",
    "rounded_rect_2d",
    "segment_2d",
    "triangle",
    "bezier",
    "chamfer_subtraction",
    "exp_smooth_subtraction",
    "bend",
    "taper",
    "repeat",
    "shear",
    "fixture:lattice_infill",
    "fixture:gridfinity_bin_ex",
];

/// 上限に当たった construct の実測が [`REACHABLE_RESOLUTION_CAPPED`] と一致するか (名前の集合と肉厚)
fn assert_capped_matches(row: &Row) {
    let mut got: Vec<(&str, f32)> = row.capped.iter().map(|(n, w)| (n.as_str(), *w)).collect();
    let mut want: Vec<(&str, f32)> = REACHABLE_RESOLUTION_CAPPED.to_vec();
    got.sort_by(|a, b| a.0.cmp(b.0));
    want.sort_by(|a, b| a.0.cmp(b.0));
    let got_names: Vec<&str> = got.iter().map(|(n, _)| *n).collect();
    let want_names: Vec<&str> = want.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        got_names, want_names,
        "res {}: 上限に当たった construct の集合が固定と違う",
        row.res
    );
    for ((n, g), (_, w)) in got.iter().zip(&want) {
        assert!(
            (g - w).abs() <= 1e-4_f32.max(0.01 * w),
            "res {}: {n} の最小肉厚 {g} (固定 {w})",
            row.res
        );
    }
}

/// 到達性の判定の解像度: セルが最小肉厚の半分以下になる最小の解像度 (`base` 以上、上限つき)
///
/// 到達性は経路が通る肉の中を区間で証明するので、壁がセルより薄いと決着しない (2026-11-14 実測:
/// `rounded_box` の外寸を正しくして壁が仕様どおり薄くなり、固定の res 4 で決着率が 18.6 % → 11.6 %)
/// 返り値は (解像度, 上限に当たったか)
fn reachable_resolution(node: &SdfNode, lo: Vec3, hi: Vec3, base: usize) -> (usize, Option<f32>) {
    let Some(wall) = min_wall_in(node, lo, hi).filter(|w| *w > 0.0) else {
        return (base, None);
    };
    // cell = edge / res <= wall / 2  <=>  res >= 2 edge / wall (edge = the box's longest side)
    let edge = (hi - lo).max_element();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let need = (2.0 * edge / wall).ceil() as usize;
    let res = need.max(base);
    if res > REACHABLE_MAX_RESOLUTION {
        (REACHABLE_MAX_RESOLUTION, Some(wall))
    } else {
        (res, None)
    }
}

/// 到達性を判定する箱: construct 自身の外接箱 (alice-sdf の解析的な外接箱、扱えない node は区間演算の
/// 探索) を各辺 5 % 広げたもの 固定の ±[`AABB`] の箱では、実寸の construct は箱の中が空のくぼみだけになり
/// 判定の対象から外れていた (実寸の引数で 9 件) 外接箱が決まらない (空の形) 時は `None`
fn own_box(node: &SdfNode) -> Option<(Vec3, Vec3)> {
    use alice_sdf::tight_aabb::{
        analytic_aabb, compute_tight_aabb_with_config, AnalyticAabb, TightAabbConfig,
    };
    let b = match analytic_aabb(node) {
        AnalyticAabb::Bounded(b) => b,
        AnalyticAabb::Empty => return None,
        AnalyticAabb::Unsupported => {
            let search = TightAabbConfig::try_new(500.0, 24, 16).expect("search config");
            compute_tight_aabb_with_config(node, &search)
        }
    };
    let pad = (b.max - b.min) * 0.05 + Vec3::splat(1e-3);
    let (lo, hi) = (b.min - pad, b.max + pad);
    (lo.is_finite() && hi.is_finite() && (hi - lo).min_element() > 0.0).then_some((lo, hi))
}

/// 判定の格子 (箱 `lo..hi` を各軸 `n` 個の cell に割った cell の中心) のうち内部の点を、その場所の肉の中央 (`-∇d` に沿って深さが増えなくなる
/// 所) まで動かし、判定の cell `cell` の半対角 (`cell × √3 / 2`) 以上内側に入った点の最初と最後を結ぶ到達性
/// (そういう点が 2 つ未満なら `None`)
///
/// 端の点を含む cell が丸ごと内部に入るので端は証明できる (表面のすぐ内側の点を端に選ぶと cell が表面を跨いで
/// 決着せず、実寸の 80 x 20 x 60 mm の箱でも未決定だった) 深さの閾値は形でなく cell で決める (最も深い点の
/// 半分などにすると、大きさの違う 2 つの部品の両端が大きい方に入り、分かれていることを見落とす) 格子点を肉の
/// 中央へ動かすのは、薄い壁 (壁厚 > 2 × 半対角、最小肉厚に合わせた解像度で成り立つ) の中央に格子点が
/// 乗らなくても端が取れるようにするため 格子は判定の解像度と同じ細かさにする (固定の粗い格子では、格子の
/// 間隔より小さい部品に点が 1 つも落ちず、60 mm と 6 mm の立方体の組の両端が大きい方に入った)
fn reachable_in(node: &SdfNode, lo: Vec3, hi: Vec3, n: usize) -> Option<Constraint> {
    use alice_sdf::eval::eval_normal;
    const HALF_DIAGONAL: f32 = 0.866_025_4; // √3 / 2
    #[allow(clippy::cast_precision_loss)]
    let step = (hi - lo) / n as f32;
    let cell = step.max_element();
    // the cell centre with index `idx` (x outermost), moved to the middle of the material around it; kept
    // when it is at least the cell's half-diagonal deep
    let endpoint = |idx: usize| -> Option<Vec3> {
        let (i, j, k) = (idx / (n * n), (idx / n) % n, idx % n);
        #[allow(clippy::cast_precision_loss)]
        let mut p = lo + step * (Vec3::new(i as f32, j as f32, k as f32) + 0.5);
        let mut d = eval(node, p);
        if d >= 0.0 {
            return None;
        }
        // 肉の中央へ: -∇d に沿って深さの 1/2 ずつ進み、深くならなくなったら止める
        for _ in 0..32 {
            let q = p - eval_normal(node, p) * (-d * 0.5);
            let dq = eval(node, q);
            if dq >= d {
                break;
            }
            p = q;
            d = dq;
        }
        (d <= -cell * HALF_DIAGONAL).then_some(p)
    };
    // the first and the last qualifying cell in grid order (the same pair a full scan would keep)
    let total = n * n * n;
    let first = (0..total).find_map(|idx| endpoint(idx).map(|p| (idx, p)))?;
    let last = (0..total)
        .rev()
        .find_map(|idx| endpoint(idx).map(|p| (idx, p)))?;
    (last.0 > first.0).then(|| Constraint::Reachable {
        node: node.clone(),
        from: first.1,
        to: last.1,
    })
}

/// 1 (constraint, resolution) 分を corpus 全件に当てて内訳を数える
/// 1 construct の判定 (`measure_row` の 1 行分)
enum Outcome {
    Skipped,
    Passed,
    Violated,
    Undecided(f32),
}

/// 1 construct を判定する (到達性は自身の箱と最小肉厚に合わせた解像度) 返り値は (判定, 上限に当たった肉厚)
fn judge_one(
    label: &str,
    build: &ConstraintBuilder,
    node: &SdfNode,
    res: usize,
) -> (Outcome, Option<f32>) {
    let reach = label.starts_with("Reachable");
    let (lo, hi) = if reach {
        match own_box(node) {
            Some(b) => b,
            None => return (Outcome::Skipped, None),
        }
    } else {
        (Vec3::splat(-AABB), Vec3::splat(AABB))
    };
    // 到達性だけ、解像度を construct の最小肉厚に合わせる (上限に当たったものは肉厚を返す)
    let (resolution, cap) = if reach {
        reachable_resolution(node, lo, hi, res)
    } else {
        (res, None)
    };
    let constraint = if reach {
        reachable_in(node, lo, hi, resolution)
    } else {
        build(node)
    };
    let Some(constraint) = constraint else {
        return (Outcome::Skipped, cap);
    };
    let config = CheckConfig {
        aabb_min: lo,
        aabb_max: hi,
        resolution,
    };
    let laws = vec![Law::hard(label, constraint).expect("provable constraint")];
    let outcome = match verdict(&check_laws(&laws, &config)) {
        "violation" => Outcome::Violated,
        "unresolved" => Outcome::Undecided(alice_sdf::interval::eval_lipschitz(node)),
        _ => Outcome::Passed,
    };
    (outcome, cap)
}

/// 1 (constraint, resolution) 分を corpus 全件に当てて内訳を数える (construct ごとに独立なので thread で並べ、
/// 結果は corpus の順に集める)
fn measure_row(
    label: &str,
    build: &ConstraintBuilder,
    corpus: &[(String, SdfNode)],
    res: usize,
) -> Row {
    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    let chunk = corpus.len().div_ceil(threads).max(1);
    let outcomes: Vec<(Outcome, Option<f32>)> = std::thread::scope(|scope| {
        // every thread is spawned before the first join (collecting the handles is what runs them together)
        #[allow(clippy::needless_collect)]
        let handles: Vec<_> = corpus
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .map(|(_, node)| judge_one(label, build, node, res))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("judge thread"))
            .collect()
    });
    let (mut passed, mut violated, mut skipped) = (0usize, 0, 0);
    let mut undecided_names = Vec::new();
    let mut capped = Vec::new();
    let mut skipped_names = Vec::new();
    let reach = label.starts_with("Reachable");
    for ((name, _), (outcome, cap)) in corpus.iter().zip(outcomes) {
        if let Some(wall) = cap {
            capped.push((name.clone(), wall));
        }
        match outcome {
            Outcome::Skipped => {
                skipped += 1;
                if reach {
                    skipped_names.push(name.clone());
                }
            }
            Outcome::Passed => passed += 1,
            Outcome::Violated => violated += 1,
            Outcome::Undecided(lip) => undecided_names.push((name.clone(), lip)),
        }
    }
    let undecided = undecided_names.len();
    let total = passed + violated + undecided;
    Row {
        res,
        total,
        skipped,
        passed,
        violated,
        undecided,
        decided: (total - undecided) as f32 / total as f32,
        undecided_names,
        capped,
        skipped_names,
    }
}

/// 決着率の下限 — 2026-09-29 の実測を pin する値 (目標値ではない)
///
/// 下回ったら区間が緩んだか探索が浅い **上限は置かない** (改善して決着率が
/// 上がるのは歓迎)
fn decided_floor(label: &str) -> f32 {
    match label {
        // ⚠️ 2026-09-30 に Lipschitz 包囲 (第 2 の証明経路) を入れて上がった
        // 「経路なし」は破壊試験 (`lipschitz_enclosure` を `None` 固定) の実測
        // - `NonOverlap`  95.5% (res 4) / 95.9% (res 8) ← 経路なしは res 8 が 91.4%
        // - `Containment` / `GradientBound` は元から未決定 0 件 (100.0%)
        "NonOverlap(sphere 0.5)" | "Containment(in sphere 4.0)" | "GradientBound(static+1e-3)" => {
            0.95
        }
        // 実測 87.7% / 92.2% — ⚠️ **Lipschitz 経路の恩恵が完全にゼロ** (旧と同値)
        // 最小肉厚は**実体の内部**を問うので、箱の中心が内部になり
        // `eval_lipschitz` の外部限定 (`f ≥ 0`) 契約が使えない
        // ⇒ ここを減らすには内部でも使える別の上界が要る
        "MinThickness(0.2)" => 0.80,
        // 実測 88.1% / 88.9% ← 経路なしでは 85.7% / 86.1%
        "Contact(0.0..0.5)" => 0.88,
        // 2026-11-15: 判定の箱を construct 自身の外接箱にし (固定の ±2 の箱では実寸の容器の中の空のくぼみ
        // だけが入り 9 件が対象外、3 件は箱で切った断片が違反に数えられていた)、端の点を判定の cell の半対角
        // より深い肉の中央に取り (表面のすぐ内側の端は cell が表面を跨いで決着しない)、開いた殻の容器 18 種の
        // 見本を実寸の引数にした (`REAL_SIZE_SAMPLES`) ⚠️ 到達性の下限はこの日から **corpus の全件 244 件**
        // (対象外を含む) に対する決着 (合格 + 違反) の割合で比べる (判定した件数を分母にすると対象外が増える
        // だけで率が上がる) 実測 45.5% (res 4、合格 108 + 違反 3 = 111 / 244、対象外 59) / 49.6% (res 8、
        // 118 + 3 = 121 / 244、対象外 58) で下限を 0.45 にした (res 8 にも同じ下限) 以前の ±2 の箱を同じ
        // 分母で数えると 13.9% (res 4、34 / 244) / 20.5% (res 8、50 / 244)
        // 2026-11-14: 解像度を construct の最小肉厚に合わせた (セル <= 壁 / 2、上限 64 は
        // `REACHABLE_RESOLUTION_CAPPED` に名前で固定) 実測 15.1% (res 4) / 22.2% (res 8)
        // それ以前の固定解像度での実測 17.4% (res 4) / 25.4% (res 8) — 未決定の 7 割強がここに集中する
        // ⚠️ **Lipschitz 経路で 1 件も動かなかった唯一の constraint** (値が完全同一)
        // 到達性は「連結性」を問うので、箱ごとの符号を締めても決着しない
        // = 未決定を減らすには**別の法則**が要る (探索深さでも包囲の精度でもない)
        "Reachable(inside..inside)" => 0.45,
        other => panic!("未知の constraint ラベル {other} — 下限を決めてから足す"),
    }
}

/// 判定器が **constraint ごとに** どれだけ証明できるかを実測する
///
/// [`unresolved_rate_is_measured_and_bounded`] は `MinThickness` 1 種だけを見る
/// 本 test が足すのは `Law::hard` を名乗れる 6 constraint 全部の内訳で、狙いは
/// 「未決定を減らすのは **法則側** か **解像度側** か」の切り分け:
///
/// - resolution を上げると減る = 区間の粗さが原因 (探索深さで解決する)
/// - resolution を上げても残る = **法則そのものが足りていない** (別の法則を
///   足すか、判定の定式化を変えないと減らない)
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "judges the corpus at resolutions up to 64 in each construct's own box: runs in release (release-profile.yml, scripts/law_tests.sh --release)"
)]
// one table per constraint, printed and checked in the same pass
#[allow(clippy::too_many_lines)]
fn unresolved_rate_by_constraint() {
    let corpus = nodes();
    let builders = constraint_builders();

    eprintln!("constraint 別 未決定率 — 対象 {} construct", corpus.len());
    eprintln!("  (res 4 → 8 で減る = 解像度で解決 / 減らない = 法則側が足りない)");

    let mut table = Vec::new();
    for (label, build) in &builders {
        let rows: Vec<Row> = [4usize, 8]
            .into_iter()
            .map(|res| measure_row(label, build, &corpus, res))
            .collect();
        for r in &rows {
            eprintln!(
                "  {label:<28} res {:>2}: {:>3} 件 (対象外 {:>3}) → 合格 {:>3} / 違反 {:>3} / 未決定 {:>3} (決着率 {:.1}%)",
                r.res, r.total, r.skipped, r.passed, r.violated, r.undecided, r.decided * 100.0
            );
        }
        if label.starts_with("Reachable") {
            for r in &rows {
                eprintln!(
                    "  {label:<28} res {:>2}: 対象外 {:?}",
                    r.res, r.skipped_names
                );
                eprintln!("  {label:<28} res {:>2}: 上限 {:?}", r.res, r.capped);
            }
            for r in &rows {
                let mut got: Vec<&str> = r.skipped_names.iter().map(String::as_str).collect();
                let mut want = if r.res == 4 {
                    REACHABLE_SKIPPED_RES4.to_vec()
                } else {
                    REACHABLE_SKIPPED_RES8.to_vec()
                };
                got.sort_unstable();
                want.sort_unstable();
                assert_eq!(got, want, "res {}: 到達性の対象外の集合が固定と違う", r.res);
                assert!(
                    r.total > 100,
                    "res {}: 到達性で判定した construct が {} 件",
                    r.res,
                    r.total
                );
                assert_capped_matches(r);
            }
        }
        for r in &rows {
            for (n, w) in &r.capped {
                eprintln!("  {label:<28} res {:>2}: 上限 {REACHABLE_MAX_RESOLUTION} に当たった {n} (最小肉厚 {w:.4})", r.res);
            }
        }
        let delta = (rows[1].decided - rows[0].decided) * 100.0;
        let verdict_text = if delta >= 1.0 {
            "解像度で減る"
        } else if rows[1].undecided == 0 {
            "未決定なし"
        } else {
            "解像度では減らない = 法則側"
        };
        eprintln!("  {label:<28} → res 4→8 の決着率 {delta:+.1} pt ({verdict_text})");

        // WM-19b: 解像度に反応しない構造を名指しする
        // 「res 4 の未決定集合 ⊇ res 8 の未決定集合」かどうかで、減った分が
        // どれかも見える 集合が完全一致なら解像度が一切効いていない
        if rows[1].undecided > 0 {
            let lo: Vec<&str> = rows[0]
                .undecided_names
                .iter()
                .map(|(n, _)| n.as_str())
                .collect();
            let same = rows[1]
                .undecided_names
                .iter()
                .all(|(n, _)| lo.contains(&n.as_str()))
                && rows[0].undecided == rows[1].undecided;
            eprintln!(
                "  {label:<28}   res 8 未決定 {} 件 (res 4 と同一集合: {})",
                rows[1].undecided,
                if same { "yes" } else { "no" }
            );
            for (n, lip) in &rows[1].undecided_names {
                let lip_text = if lip.is_finite() {
                    format!("L={lip:.3}")
                } else {
                    "L=非有限".to_string()
                };
                eprintln!("  {label:<28}     - {n} ({lip_text})");
            }
        }

        let floor = decided_floor(label);
        for r in &rows {
            // 到達性の下限は corpus の全件 (対象外を含む) に対する決着の割合で比べる: 判定した件数だけを分母に
            // すると、難しい construct が対象外に移るだけで率が上がる
            let rate = if label.starts_with("Reachable") {
                #[allow(clippy::cast_precision_loss)]
                let all = (r.passed + r.violated) as f32 / (r.total + r.skipped) as f32;
                all
            } else {
                r.decided
            };
            assert!(
                rate > floor,
                "{label} res {}: 決着率が {:.1}% で下限 {:.0}% を割った",
                r.res,
                rate * 100.0,
                floor * 100.0
            );
        }
        table.extend(rows);
    }

    // 空振り (vacuous) 検査 — 表全体で合格と違反の両方が出ていなければ、
    // 判定器でなく scene 側が何も問うていない (未決定率を測る意味が消える)
    let passed_all: usize = table.iter().map(|r| r.passed).sum();
    let violated_all: usize = table.iter().map(|r| r.violated).sum();
    assert!(
        passed_all > 0 && violated_all > 0,
        "表全体で合格 {passed_all} / 違反 {violated_all} — 片側しか出ていないなら scene が何も問うていない"
    );
}

/// probe 依存 constraint の builder (半径だけを振る)
fn probe_builder(kind: &str, radius: f32) -> ConstraintBuilder {
    let probe = parse_lol(&format!("sphere({radius})")).expect("probe snippet parses");
    match kind {
        "NonOverlap" => Box::new(move |n: &SdfNode| {
            Some(Constraint::NonOverlap {
                a: n.clone(),
                b: probe.clone(),
            })
        }),
        "Contact" => Box::new(move |n: &SdfNode| {
            Some(Constraint::Contact {
                a: n.clone(),
                b: probe.clone(),
                min_distance: 0.0,
                max_distance: 0.5,
            })
        }),
        other => panic!("probe 依存でない constraint {other} が渡された"),
    }
}

/// probe を 3 種類に振り、未決定集合が probe に支配されていないかを見る
///
/// 2026-09-30 実測: probe `sphere(0.5)` は corpus 既定の `box3d(0.5, 0.5, 0.5)` の
/// **内接球半径と厳密に一致**するため、切り欠きと probe が接する配置になる
/// 接触点では距離が厳密に 0 なので外側丸めで区間が 0 をまたぎ、**細分しても
/// 接点は消えない** 結果 `NonOverlap` の未決定が 21 件になり、probe を 0.37 に
/// すると 10 件に減った `Contact` に至っては res 4→8 の傾きの符号判定が反転した
///
/// ⇒ **単一 probe の測定値は判定器の性能でなく probe と corpus の相性を測る**
///
/// 本 test は「**どの probe でも未決定**」= 真の核 を pin し、probe 間のばらつき
/// (和集合 − 核 = probe 依存分) を出力して、測定が probe に支配されている時に
/// 気付けるようにする
#[test]
fn undecided_set_is_not_probe_dominated() {
    // きりの良い値は corpus 側の定数と一致しやすいので意図的にずらした 3 点
    const RADII: [f32; 3] = [0.37, 0.5, 0.61];
    const RES: usize = 8;
    let corpus = nodes();

    for kind in ["NonOverlap", "Contact"] {
        let mut sets: Vec<(f32, f32, Vec<String>)> = Vec::new();
        for r in RADII {
            let row = measure_row(kind, &probe_builder(kind, r), &corpus, RES);
            eprintln!(
                "  {kind:<11} probe r={r:.2} res {RES}: 未決定 {:>3} 件 / 合格 {:>3} / 違反 {:>3} (決着率 {:.1}%)",
                row.undecided,
                row.passed,
                row.violated,
                row.decided * 100.0
            );
            sets.push((
                r,
                row.decided,
                row.undecided_names.iter().map(|(n, _)| n.clone()).collect(),
            ));
        }

        let core: Vec<&String> = sets[0]
            .2
            .iter()
            .filter(|n| sets[1..].iter().all(|(_, _, s)| s.iter().any(|x| x == *n)))
            .collect();
        let union: std::collections::BTreeSet<&String> =
            sets.iter().flat_map(|(_, _, s)| s.iter()).collect();
        eprintln!(
            "  {kind:<11} 真の核 (全 probe で未決定) {} 件 / 和集合 {} 件 → **probe 依存 {} 件**",
            core.len(),
            union.len(),
            union.len() - core.len()
        );
        for n in &core {
            eprintln!("  {kind:<11}   核: {n}");
        }

        // 2026-09-30 実測を pin — **核が増えたら退行** (probe 依存分は probe の
        // 選び方で動くので pin しない、出力で見る)
        //   NonOverlap 核 0 件 (未決定 r=0.37 で 1、r=0.50 で 10、r=0.61 で 0)
        //   Contact    核 1 件 (未決定 r=0.37 で 14、r=0.50 で 27、r=0.61 で 1)
        // ⚠️ r=0.50 だけが突出するのは corpus 既定 `box3d(0.5,0.5,0.5)` の
        //    内接球半径と一致して接する配置になるため
        //
        // ## 対照実験 (2026-09-30、`lipschitz_enclosure` を `None` 固定にして実測)
        //
        // Lipschitz 包囲 (第 2 の証明経路) を殺すと核は **9 / 11 件に戻る**:
        //   NonOverlap 核 9 (heart / cut_sphere / link / capped_torus / death_star /
        //                    arc_shape / segment_2d / triangle / bezier)
        //   Contact    核 11 (上記 9 + under_desk_mount + fixture:smooth_union_scale_rotate)
        // ⇒ 核が動いたのは corpus や probe の変化でなく **本経路の効果**
        // 残った `fixture:smooth_union_scale_rotate` は Lipschitz でも決着しない
        // = 別の法則が要る側 (`smooth_union` は場が距離でないので L が緩む)
        let (core_max, decided_floor) = match kind {
            "NonOverlap" => (0usize, 0.95f32),
            "Contact" => (1usize, 0.88f32),
            other => panic!("未知の constraint {other} — pin を決めてから足す"),
        };
        assert!(
            core.len() <= core_max,
            "{kind}: probe に依らない未決定 (真の核) が {} 件に増えた (pin {core_max})",
            core.len()
        );
        for (r, decided, _) in &sets {
            assert!(
                *decided > decided_floor,
                "{kind} probe r={r:.2}: 決着率 {:.1}% が下限 {:.0}% を割った",
                decided * 100.0,
                decided_floor * 100.0
            );
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 第 2 の証明経路 (Lipschitz 包囲) — doctrine §5 の中核前提の実証
//
// doctrine §5 は「法則を足せば undecided は減る」と主張していたが、**足して
// proven に動いた実例は 2026-09-30 まで 0 件**だった 本 section はその 1 件目
//
// ## 未決定の正体 (2026-09-30 実測、3 段の診断で確定)
//
// 真の核 5 件 (`heart` / `cut_sphere` / `link` / `capped_torus` / `death_star`) の
// 未決定領域を総当たりで測ると、**区間 `[-5.5, +6.53]` に対し真の範囲は
// `[+1.58, +1.62]`** だった 区間は健全 (真値を含む) だが依存性問題で
// **セル幅の約 300 倍**に膨らんでおり、⚠️ **解像度を上げても幅は縮まらない**
// (= 未決定が「解像度に無反応」だった正体)
//
// 一方 `|f(centre)| = 1.60` / `L = 1.0` / `L·ρ = 0.027` なので、Lipschitz 包囲
// `[f(c) − L·ρ, f(c) + L·ρ]` は **59 倍の余裕で符号を確定させる**
// ⇒ 未決定は「決定不能」ではなく **「区間が緩い」** だった
//
// ## ⚠️ 効く範囲は **外部 (`f ≥ 0`) を問う法則だけ**
//
// `eval_lipschitz` の契約は外部限定 (`lipschitz_enclosure` の doc 参照) なので、
// **箱の中心が実体の内部に来る判定では 1 件も動かない**
//
// | constraint | 何を問うか | 決着率 (res 4 / 8) | 経路なし | 効果 |
// |---|---|---|---|---|
// | `NonOverlap` | 箱が**外部**にあるか | 95.5% / 95.9% | - / 91.4% | **真の核 9 → 0** |
// | `Contact`    | gap が**外部**で範囲内か | 88.1% / 88.9% | 85.7% / 86.1% | **真の核 11 → 1** |
// | `MinThickness` | **内部**の肉厚 | 87.7% / 92.2% | 87.7% / 92.2% | ⚠️ **±0** |
// | `Reachable`  | **連結性** | 17.4% / 25.4% | 17.4% / 25.4% | ⚠️ **±0** |
// | `Containment` / `GradientBound` | - | 100% | 100% | 元から未決定 0 |
//
// ⚠️ **`MinThickness` と `Reachable` は 1 pt も動かない** 前者は内部を問うので
// 外部限定の契約が使えず、後者は連結性なので箱ごとの符号を締めても決着しない
// ⇒ **残りの未決定を減らすには「内部でも使える上界」と「連結性の法則」が要る**
// (どちらも解像度でも包囲の精度でも埋まらない、法則側の欠落)
//
// 対照実験は `undecided_set_is_not_probe_dominated` 内の § を参照
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 真の核のうち「薄い / 穴あき / 減算」5 件
///
/// 低次元 4 件 (`arc_shape` / `segment_2d` / `triangle` / `bezier`) は内部が
/// 測度ゼロなので「問わないのが正解」側、核から外して扱う
const TRUE_CORE_NONDEGENERATE: [&str; 5] =
    ["heart", "cut_sphere", "link", "capped_torus", "death_star"];

/// **第 2 の証明経路が真の核を決着させることの実証** (doctrine §5 の 1 件目)
///
/// 対照実験 (`alice-lol/src/law.rs` の `lipschitz_enclosure` を `None` 固定) では
/// この 5 件は **3 つの probe 半径すべてで未決定** (= 決着 0/3) だった
/// 経路を入れると **3 つすべてで `Proven`** (2026-09-30 実測、下の表が出力される)
/// ⇒ undecided → proven が文字どおり起きた 1 件目
#[test]
fn the_lipschitz_path_decides_the_former_true_core() {
    const RADII: [f32; 3] = [0.37, 0.5, 0.61];
    const RES: usize = 8;
    let config = CheckConfig {
        aabb_min: Vec3::splat(-AABB),
        aabb_max: Vec3::splat(AABB),
        resolution: RES,
    };
    let corpus = nodes();

    let mut seen = 0usize;
    for target in TRUE_CORE_NONDEGENERATE {
        let Some((_, node)) = corpus.iter().find(|(n, _)| n == target) else {
            panic!("corpus に {target} が居ない — 対象の選び方が古い");
        };
        seen += 1;
        let mut decided = 0usize;
        let mut row = Vec::new();
        for r in RADII {
            let probe = parse_lol(&format!("sphere({r})")).expect("probe parses");
            let laws = vec![Law::hard(
                "no_overlap",
                Constraint::NonOverlap {
                    a: node.clone(),
                    b: probe,
                },
            )
            .expect("provable constraint")];
            let v = check_laws(&laws, &config).hard_verdict();
            if v == HardVerdict::Proven {
                decided += 1;
            }
            row.push(format!("r={r:.2}:{v:?}"));
        }
        eprintln!("  {target:<14} {}", row.join("  "));
        assert_eq!(
            decided,
            RADII.len(),
            "{target}: {} (Lipschitz 経路が効いていれば 3 probe すべて Proven、\
             経路を殺すと 3 つすべて Undecided)",
            row.join(" ")
        );
    }
    assert_eq!(seen, TRUE_CORE_NONDEGENERATE.len(), "対象の取り落とし");
}

/// ⚠️ **区間包囲が総当たりの真値を含むか** — 健全性そのものの検査
///
/// `eval_interval` は健全 (真値を包囲する) ことが前提で、`NonOverlap` の決着は
/// `ia.lo >= 0.0` を見る ⇒ **下界が真の最小値より高いと偽の `proven` になる**
///
/// 本 test は corpus 全 construct × 24 箱 × 5³ 点で包囲の破れを探す
/// 検出経路は Lipschitz 包囲との共通部分が空になったこと (健全な包囲同士の
/// 交差は空にならないので、**空になること自体が不健全の機械的な証拠**)
///
/// # 既知の破れ (ALICE-SDF 側、未修正)
///
/// `rounded_cone` の 1 件のみ 外れ量 0.35 `interval.rs:585` が外接球半径に
/// `(2h).hypot(max(r1,r2))` を渡しているが、丸め錐のキャップは球なので真の
/// 外接半径は `h + max(r1,r2)` `hypot(2h,r) < h+r ⟺ 3h < 2r` なので
/// **`max(r1,r2) > 1.5·half_height` の丸め錐で外接球が実体より小さくなる**
/// (corpus は `rounded_cone(3.0, 2.0, 1.0)` = `r=3.0` / `h=0.5` で該当)
///
/// 判定器側は包囲の矛盾を検出した cell を **未決定**にするので、この破れが
/// `proven` に化けることはない (`law.rs` の `Refined::Contradiction`)
#[test]
fn interval_enclosure_contains_the_brute_force_truth() {
    use alice_sdf::interval::{eval_interval, Vec3Interval};

    /// ALICE-SDF 側の未修正分 ⚠️ **修正されたらこの test が red になる**
    /// (「まだ破れている」ことも pin しているので、直したらここから削る)
    const KNOWN_UNSOUND: [&str; 1] = ["rounded_cone"];
    /// 包囲の破れとみなす下限 (f32 の丸め分は見ない)
    const SLACK: f32 = 1.0e-3;
    /// 標本の箱の幅 = 判定器の八分木深さ 4 での cell 幅 (4 / 2^4)
    const W: f32 = 0.25;

    let mut seed = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        #[allow(clippy::cast_precision_loss)]
        let u = (seed >> 11) as f32 / (1u64 << 53) as f32;
        u
    };

    // 箱が検査 AABB に収まる範囲で中心を撒く
    let span = 2.0f32.mul_add(AABB, -W);
    let mut offenders: Vec<(String, f32)> = Vec::new();
    let (mut checked, mut boxes) = (0usize, 0usize);
    for (name, node) in nodes() {
        checked += 1;
        let mut worst = 0.0_f32;
        for _ in 0..24 {
            let base = Vec3::new(
                span.mul_add(next(), -AABB),
                span.mul_add(next(), -AABB),
                span.mul_add(next(), -AABB),
            );
            let bx = Vec3Interval::from_bounds(base, base + Vec3::splat(W));
            let iv = eval_interval(&node, bx);
            if !iv.lo.is_finite() || !iv.hi.is_finite() {
                continue; // 包囲を主張していない形は対象外
            }
            boxes += 1;
            for i in 0..5 {
                for j in 0..5 {
                    for k in 0..5 {
                        let t = |q: usize| (q as f32) / 4.0 * W;
                        let v = eval(&node, base + Vec3::new(t(i), t(j), t(k)));
                        if v.is_finite() {
                            worst = worst.max(iv.lo - v).max(v - iv.hi);
                        }
                    }
                }
            }
        }
        if worst > SLACK {
            offenders.push((name, worst));
        }
    }
    offenders.sort_by(|a, b| b.1.total_cmp(&a.1));

    // 空振り検査 — 箱が 1 つも finite でなければ何も測っていない
    assert!(checked > 200, "corpus が小さすぎる: {checked} construct");
    assert!(boxes > 2_000, "finite な包囲が {boxes} 箱しかない (空振り)");

    for (name, w) in &offenders {
        eprintln!("  包囲の破れ {name}: {w:.5}");
    }
    let unexpected: Vec<&(String, f32)> = offenders
        .iter()
        .filter(|(n, _)| !KNOWN_UNSOUND.contains(&n.as_str()))
        .collect();
    assert!(
        unexpected.is_empty(),
        "既知でない construct で区間包囲が真値を外した (偽の proven を生む向き): {unexpected:?}"
    );
    for known in KNOWN_UNSOUND {
        assert!(
            offenders.iter().any(|(n, _)| n == known),
            "{known} の包囲の破れが再現しない — ALICE-SDF 側が修正された可能性 \
             KNOWN_UNSOUND から削って決着率の pin を締め直す"
        );
    }
}

/// The own-box judge decides real-size shapes: a connected 80 x 20 x 60 mm box passes, two 20 mm cubes
/// 80 mm apart are a violation (the first and last interior lattice points sit in different cubes), and
/// so are a 60 mm and a 6 mm cube (both ends must not fall in the bigger part)
/// With the fixed ±2 box neither the gap nor the cubes reach the box, so the pair would be skipped
#[test]
fn own_box_judge_decides_real_size_shapes() {
    let corpus = vec![
        (
            "connected".to_string(),
            parse_lol("box3d(40, 10, 30)").expect("parses"),
        ),
        (
            "disconnected".to_string(),
            parse_lol(
                "union(translate(-50, 0, 0, box3d(10, 10, 10)), translate(50, 0, 0, box3d(10, 10, 10)))",
            )
            .expect("parses"),
        ),        // one part much bigger than the other (a 60 mm cube and a 6 mm cube): the endpoints must not both
        // fall in the bigger part (a depth threshold set by the deepest point would put them there)
        (
            "disconnected_uneven".to_string(),
            parse_lol(
                "union(translate(-60, 0, 0, box3d(30, 30, 30)), translate(60, 0, 0, box3d(3, 3, 3)))",
            )
            .expect("parses"),
        ),
    ];
    let builders = constraint_builders();
    let (label, build) = builders
        .iter()
        .find(|(l, _)| l.starts_with("Reachable"))
        .expect("Reachable builder");
    let row = measure_row(label, build, &corpus, RESOLUTION);
    assert_eq!(row.skipped, 0, "skipped {:?}", row.skipped_names);
    assert_eq!(row.total, 3, "evaluated nothing");
    assert_eq!(
        (row.passed, row.violated),
        (1, 2),
        "undecided {:?}, passed {} violated {}",
        row.undecided_names,
        row.passed,
        row.violated
    );
}
