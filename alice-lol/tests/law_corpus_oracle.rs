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

use alice_lol::law::{check_laws, CheckConfig, Constraint, Law, LawReport};
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

fn verdict(report: &LawReport) -> &'static str {
    if report.has_hard_violations() {
        "violation"
    } else if report.has_unresolved() {
        "unresolved"
    } else {
        "passed"
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
        assert!(
            !report.has_hard_violations(),
            "{name}: sampling found a quotient above the claimed bound {claimed}"
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
