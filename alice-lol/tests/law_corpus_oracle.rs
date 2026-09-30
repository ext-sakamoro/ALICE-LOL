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
    for (res, _, _, _, _, decided) in rows {
        assert!(
            decided > 0.5,
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
}

type ConstraintBuilder = Box<dyn Fn(&SdfNode) -> Option<Constraint>>;

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

/// 1 (constraint, resolution) 分を corpus 全件に当てて内訳を数える
fn measure_row(
    label: &str,
    build: &ConstraintBuilder,
    corpus: &[(String, SdfNode)],
    res: usize,
) -> Row {
    let config = CheckConfig {
        aabb_min: Vec3::splat(-AABB),
        aabb_max: Vec3::splat(AABB),
        resolution: res,
    };
    let (mut passed, mut violated, mut skipped) = (0usize, 0, 0);
    let mut undecided_names = Vec::new();
    for (name, node) in corpus {
        let Some(constraint) = build(node) else {
            skipped += 1;
            continue;
        };
        let laws = vec![Law::hard(label, constraint).expect("provable constraint")];
        match verdict(&check_laws(&laws, &config)) {
            "violation" => violated += 1,
            "unresolved" => {
                undecided_names.push((name.clone(), alice_sdf::interval::eval_lipschitz(node)));
            }
            _ => passed += 1,
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
    }
}

/// 決着率の下限 — 2026-09-29 の実測を pin する値 (目標値ではない)
///
/// 下回ったら区間が緩んだか探索が浅い **上限は置かない** (改善して決着率が
/// 上がるのは歓迎)
fn decided_floor(label: &str) -> f32 {
    match label {
        "NonOverlap(sphere 0.5)" => 0.85,
        "Containment(in sphere 4.0)" | "GradientBound(static+1e-3)" => 0.95,
        // 実測 87.7% (res 4) / 92.2% (res 8) と 85.7% / 86.1%
        "MinThickness(0.2)" | "Contact(0.0..0.5)" => 0.80,
        // 実測 17.4% (res 4) / 25.4% (res 8) — 未決定の 7 割強がここに集中する
        "Reachable(inside..inside)" => 0.15,
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
            assert!(
                r.decided > floor,
                "{label} res {}: 決着率が {:.1}% で下限 {:.0}% を割った",
                r.res,
                r.decided * 100.0,
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
        //   NonOverlap 核 9 件 (r=0.37/0.61 で 10、r=0.50 で 21)
        //   Contact    核 11 件 (r=0.37 で 20、r=0.50 で 34、r=0.61 で 12)
        // ⚠️ r=0.50 だけが突出するのは corpus 既定 `box3d(0.5,0.5,0.5)` の
        //    内接球半径と一致して接する配置になるため
        let (core_max, decided_floor) = match kind {
            "NonOverlap" => (9usize, 0.88f32),
            "Contact" => (11usize, 0.83f32),
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
