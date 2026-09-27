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
#![allow(clippy::cast_precision_loss)]

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
    vec![Law::hard("no_overlap", Constraint::NonOverlap { a, b })]
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
            )];
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
