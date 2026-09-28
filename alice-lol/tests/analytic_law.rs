//! 法則検証器の解析解突合テスト (analytic oracle)
//!
//! `law::check_laws` の距離依存 5 variant (`MinThickness` / `NonOverlap` /
//! `Containment` / `Contact` / `Stress`) を、**幾何が閉じた式で分かる scene**
//! に対して突き合わせる `golden` (実装の出力を pin する test) ではなく、
//! 真の距離を手計算した値との比較なので、検証器が場の値を距離と取り違えて
//! いれば落ちる
//!
//! 起票: セルフレビュー 2026-09-16 § 4「検証器が嘘をつく」 —
//! `sdf_eval` の返り値をそのまま距離として使うと
//! (a) TPMS の場は距離を最大 √3〜7 倍に **過大** 申告する (薄壁が合格)
//! (b) union の内部は距離を **過小** 申告する (厚い壁が不合格)
//! の両方向で判定が壊れる

use alice_lol::law::{check_laws, CheckConfig, Constraint, Law};
use alice_sdf::SdfNode;
use glam::Vec3;

/// 1 セルだけの検査範囲 (中心 `p`、半幅 `h`) — セル中心 = `p` になる
fn single_cell(p: Vec3, h: f32) -> CheckConfig {
    CheckConfig {
        aabb_min: p - Vec3::splat(h),
        aabb_max: p + Vec3::splat(h),
        resolution: 1,
    }
}

fn min_thickness(node: SdfNode, t: f32) -> Vec<Law> {
    vec![Law::hard(
        "thick",
        Constraint::MinThickness {
            node,
            min_thickness: t,
        },
    )
    .expect("provable constraint")]
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// MinThickness
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// gyroid 板 (`|g|/k - τ`、k = 1, τ = 0.1) の原点: 場の値は −τ = −0.1 だが、
/// 原点は gyroid の平坦点で g ≈ √3·s (s = 平面 x+y+z=0 からの距離、2 階微分は
/// 全て 0 なので誤差は O(s³)) → 表面 `|g| = τ` までの真の距離は
/// ε: 1.5·sin(2ε/√3) = 0.1 ⇒ ε = 0.05778
///
/// 閾値 t = 0.08 に対し、場の値 0.1 ≥ 0.08 で「合格」とするのが誤り、
/// 真の距離 0.058 < 0.08 なので肉厚不足
#[test]
fn gyroid_sheet_is_thinner_than_its_field_claims() {
    let node = SdfNode::gyroid(1.0, 0.1);
    let true_dist = 0.057_78_f32;
    let t = 0.08_f32;

    let report = check_laws(&min_thickness(node, t), &single_cell(Vec3::ZERO, 0.01));

    assert!(
        report.has_hard_violations(),
        "gyroid 板の真の半厚 {true_dist:.4} < {t} なのに合格した (場の値 0.1 を距離と誤認)\n{}",
        alice_lol::law::format_report(&report)
    );
    let v = &report.violations[0];
    // residual = (表面までの距離の上界) − t  ∈ [true − t, 0)
    let expect = true_dist - t;
    assert!(
        v.residual < 0.0 && (v.residual - expect).abs() < 0.006,
        "residual {:.4} が解析解 {expect:.4} (±0.006) から外れた",
        v.residual
    );
}

/// 半径 1 の 2 球 (中心 ±0.5 on x) の union、原点: 場の値は −0.5 だが
/// union 表面 (各球のうち相手の外側にある部分) までの真の距離は
/// min |q| s.t. q ∈ ∂A, |q − `c_B`| ≥ 1 ⇒ |q|² = 1.25 + `u_x` ≥ 0.75 ⇒ 0.8660
///
/// 閾値 t = 0.6 に対し、場の値 0.5 < 0.6 で「肉厚不足」とするのが誤り
#[test]
fn union_interior_is_thicker_than_its_field_claims() {
    let a = SdfNode::sphere(1.0).translate(0.5, 0.0, 0.0);
    let b = SdfNode::sphere(1.0).translate(-0.5, 0.0, 0.0);
    let node = a.union(b);

    let report = check_laws(&min_thickness(node, 0.6), &single_cell(Vec3::ZERO, 0.01));

    assert!(
        report.all_passed(),
        "union 内部の真の距離 0.866 ≥ 0.6 なのに違反 / 未決定になった (min(fa,fb) を距離と誤認)\n{}",
        alice_lol::law::format_report(&report)
    );
}

/// 球殻 (R = 1.0 から r = 0.7 を引く、肉厚 0.3) の中央半径 0.85 の点:
/// 内外どちらの表面までも真の距離 0.15 (subtract の内部は exact なので
/// 場の値も −0.15、こちらは検証器の新旧で一致すべき境界 case)
#[test]
fn sphere_shell_thickness_boundary_is_exact() {
    let shell = || SdfNode::sphere(1.0).subtract(SdfNode::sphere(0.7));
    let p = Vec3::new(0.85, 0.0, 0.0);

    let pass = check_laws(&min_thickness(shell(), 0.14), &single_cell(p, 0.01));
    assert!(
        pass.all_passed(),
        "真の距離 0.15 ≥ 0.14 は合格\n{}",
        alice_lol::law::format_report(&pass)
    );

    let fail = check_laws(&min_thickness(shell(), 0.16), &single_cell(p, 0.01));
    assert!(fail.has_hard_violations(), "真の距離 0.15 < 0.16 は違反");
    let r = fail.violations[0].residual;
    assert!(
        (r - (0.15 - 0.16)).abs() < 0.004,
        "residual {r:.4} が解析解 −0.01 (±0.004) から外れた"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// NonOverlap / Containment
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

const fn grid(min: Vec3, max: Vec3, n: usize) -> CheckConfig {
    CheckConfig {
        aabb_min: min,
        aabb_max: max,
        resolution: n,
    }
}

/// 半径 1 の 2 球、中心間 2.2 (隙間 0.2) → 重なりなし、中心間 1.9 (侵入 0.1)
/// → 重なりあり、最深点でも侵入深さは片側 0.05 なので residual ∈ [−0.05, 0)
#[test]
fn non_overlap_two_spheres_analytic() {
    let law = |dx: f32| {
        vec![Law::hard(
            "apart",
            Constraint::NonOverlap {
                a: SdfNode::sphere(1.0),
                b: SdfNode::sphere(1.0).translate(dx, 0.0, 0.0),
            },
        )
        .expect("provable constraint")]
    };
    let cfg = grid(Vec3::new(-1.2, -1.2, -1.2), Vec3::new(3.4, 1.2, 1.2), 8);

    let apart = check_laws(&law(2.2), &cfg);
    assert!(
        apart.all_passed(),
        "隙間 0.2 は重なりなし (grid 中心が両方外側でも「証明」まで要求)\n{}",
        alice_lol::law::format_report(&apart)
    );

    let overlap = check_laws(&law(1.9), &cfg);
    assert!(overlap.has_hard_violations(), "侵入 0.1 は重なり");
    let r = overlap.violations[0].residual;
    assert!(
        (-0.05 - 1e-3..=-1e-3).contains(&r),
        "residual {r:.4} は片側侵入深さの上界 −0.05 以上、かつ実際の侵入量のはず"
    );
}

/// 内球 r = 0.5 が外球 R = 1 に収まる: 中心 (0.4,0,0) は最遠点 0.9 < 1 で pass、
/// 中心 (0.6,0,0) は最遠点 1.1 > 1 で はみ出し量 0.1 (residual ∈ [−0.1, 0))
#[test]
fn containment_sphere_in_sphere_analytic() {
    let law = |cx: f32| {
        vec![Law::hard(
            "inside",
            Constraint::Containment {
                inner: SdfNode::sphere(0.5).translate(cx, 0.0, 0.0),
                outer: SdfNode::sphere(1.0),
            },
        )
        .expect("provable constraint")]
    };
    let cfg = grid(Vec3::splat(-1.5), Vec3::splat(1.5), 8);

    let inside = check_laws(&law(0.4), &cfg);
    assert!(
        inside.all_passed(),
        "最遠点 0.9 < 1 は収まる\n{}",
        alice_lol::law::format_report(&inside)
    );

    let outside = check_laws(&law(0.6), &cfg);
    assert!(outside.has_hard_violations(), "最遠点 1.1 > 1 ははみ出し");
    let r = outside.violations[0].residual;
    assert!(
        (-0.1 - 1e-3..=-1e-3).contains(&r),
        "residual {r:.4} ははみ出し量 0.1 の範囲内、かつ実際のはみ出し量のはず"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Contact
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 半径 1 の 2 球、中心間 2.5 → 表面間距離 (gap) は正確に 0.5
///
/// [0.3, 1.0] → pass / [0.6, 1.0] → 近すぎ (residual ≈ 0.5 − 0.6) /
/// [0.1, 0.4] → 遠すぎ (residual ≈ 0.4 − 0.5)
#[test]
fn contact_gap_between_spheres_analytic() {
    let law = |lo: f32, hi: f32| {
        vec![Law::hard(
            "mate",
            Constraint::Contact {
                a: SdfNode::sphere(1.0),
                b: SdfNode::sphere(1.0).translate(2.5, 0.0, 0.0),
                min_distance: lo,
                max_distance: hi,
            },
        )
        .expect("provable constraint")]
    };
    let cfg = grid(Vec3::new(-1.2, -1.2, -1.2), Vec3::new(3.7, 1.2, 1.2), 10);

    let ok = check_laws(&law(0.3, 1.0), &cfg);
    assert!(
        ok.all_passed(),
        "gap 0.5 ∈ [0.3, 1.0]\n{}",
        alice_lol::law::format_report(&ok)
    );

    let close = check_laws(&law(0.6, 1.0), &cfg);
    assert!(close.has_hard_violations(), "gap 0.5 < 0.6 は近すぎ");
    let r = close.violations[0].residual;
    assert!(
        (-0.1 - 0.01..0.0).contains(&r),
        "residual {r:.4} は 0.5 − 0.6 = −0.1 の近傍 (上界側) のはず"
    );

    let far = check_laws(&law(0.1, 0.4), &cfg);
    assert!(far.has_hard_violations(), "gap 0.5 > 0.4 は遠すぎ");
    let r = far.violations[0].residual;
    // residual = max − UB、UB ≥ gap なので residual ≤ 0.4 − 0.5 = −0.1
    // (UB は標本点経由の三角不等式なので gap より少し大きい、cell 0.49 で +0.05 まで)
    assert!(
        (-0.1 - 0.05..=-0.1 + 1e-3).contains(&r),
        "residual {r:.4} は 0.4 − 0.5 = −0.1 (上界側 −0.05 まで) のはず"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Stress
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 板 2 × 2 × 0.2 (z 半厚 0.1) の中央面 z = 0 の点: 表面まで 0.1
/// 荷重 1.0 × 係数 0.15 = 必要 0.15 > 0.1 → 違反、係数 0.05 → pass
///
/// 0.5.0 から `Stress` は `Priority::Hard` を名乗れない (要求値が無次元の
/// heuristic = `Evidence::Modelled`) ので `soft` で積む 測っている距離が
/// 解析解と合うことは変わらない
#[test]
fn stress_plate_thickness_analytic() {
    let law = |factor: f32| {
        vec![Law::soft(
            "load",
            1.0,
            Constraint::Stress {
                node: SdfNode::box3d(2.0, 2.0, 0.2),
                load_points: vec![(Vec3::new(0.0, 0.0, 0.1), 1.0)],
                min_thickness_factor: factor,
            },
        )]
    };
    let cfg = single_cell(Vec3::ZERO, 0.01);

    let ok = check_laws(&law(0.05), &cfg);
    assert!(
        ok.all_passed(),
        "必要 0.05 ≤ 真の距離 0.1\n{}",
        alice_lol::law::format_report(&ok)
    );

    let bad = check_laws(&law(0.15), &cfg);
    assert_eq!(bad.violations.len(), 1, "必要 0.15 > 真の距離 0.1");
    let r = bad.violations[0].residual;
    assert!(
        (r - (0.1 - 0.15)).abs() < 0.004,
        "residual {r:.4} が解析解 −0.05 (±0.004) から外れた"
    );
}

/// `Stress` は距離を反例で押さえても、要求値が無次元の heuristic なので
/// 結論はモデルに依る — `Priority::Hard` を名乗らせない
#[test]
fn stress_cannot_claim_hard_priority() {
    let c = Constraint::Stress {
        node: SdfNode::box3d(2.0, 2.0, 0.2),
        load_points: vec![(Vec3::new(0.0, 0.0, 0.1), 1.0)],
        min_thickness_factor: 0.15,
    };
    let err = Law::hard("load", c).expect_err("Stress が Hard を名乗れてしまった");
    assert_eq!(err.constraint, "Stress");

    let report = check_laws(&law_soft_stress(0.15), &single_cell(Vec3::ZERO, 0.01));
    assert!(
        !report.has_hard_violations(),
        "soft で積んだ Stress が Hard 違反として出た"
    );
    assert!(
        report.violations[0].evidence.model().is_some(),
        "Stress の違反が Modelled として報告されていない"
    );
}

fn law_soft_stress(factor: f32) -> Vec<Law> {
    vec![Law::soft(
        "load",
        1.0,
        Constraint::Stress {
            node: SdfNode::box3d(2.0, 2.0, 0.2),
            load_points: vec![(Vec3::new(0.0, 0.0, 0.1), 1.0)],
            min_thickness_factor: factor,
        },
    )]
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 判定不能は合格ではない
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `InfiniteCone` の区間評価は `Interval::EVERYTHING` (包含を諦める node) なので
/// 深い内部点の球探索は証明も証拠も得られない → unresolved に載り、
/// `all_passed` は false (旧実装は場の値 |f| ≥ t で silent に合格していた)
#[test]
fn undecidable_node_is_reported_not_passed() {
    use alice_lol::law::UnresolvedReason;
    // 半角 45° の無限円錐 (頂点原点、−y 方向に開く)、軸上の深い内部点
    let node = SdfNode::infinite_cone(std::f32::consts::FRAC_PI_4);
    let p = Vec3::new(0.0, -10.0, 0.0);
    assert!(alice_sdf::eval(&node, p) < 0.0, "標本点は内部のはず");

    let report = check_laws(&min_thickness(node, 0.5), &single_cell(p, 0.01));

    assert!(!report.all_passed(), "判定不能を合格にしてはいけない");
    assert!(report.violations.is_empty(), "違反の証拠は無い");
    assert!(report.has_unresolved());
    assert_eq!(report.passed, 0);
    match &report.unresolved[0].reason {
        UnresolvedReason::SurfaceProximity { radius } => assert!((radius - 0.5).abs() < 1e-6),
        other => panic!("unexpected reason {other:?}"),
    }
    let text = alice_lol::law::format_report(&report);
    assert!(text.contains("UNDECIDED"), "{text}");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 精度 parameter (resolution) 独立性
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 検査範囲を固定して `resolution` を 1 / 2 / 4 / 8 と振っても、解析解で決まる
/// verdict (違反 / 証明付き合格) が変わらないこと 変わったら「解像度が判定
/// を担っている」= 標本点の場の値に依存した実装に退行した証拠
#[test]
fn verdict_is_independent_of_resolution() {
    let cases: Vec<(&str, Vec<Law>, bool)> = vec![
        // 原点半幅 0.03 の範囲: 全標本点が gyroid 板の中央付近 (真の半厚 0.058 < 0.08)
        (
            "gyroid_thin",
            min_thickness(SdfNode::gyroid(1.0, 0.1), 0.08),
            true,
        ),
        // union 内部 (真の距離 ≥ 0.83 > 0.6、範囲内の全点で)
        (
            "union_thick",
            min_thickness(
                SdfNode::sphere(1.0)
                    .translate(0.5, 0.0, 0.0)
                    .union(SdfNode::sphere(1.0).translate(-0.5, 0.0, 0.0)),
                0.6,
            ),
            false,
        ),
    ];
    for (name, laws, expect_violation) in &cases {
        for res in [1usize, 2, 4, 8] {
            let cfg = CheckConfig {
                aabb_min: Vec3::splat(-0.03),
                aabb_max: Vec3::splat(0.03),
                resolution: res,
            };
            let report = check_laws(laws, &cfg);
            assert_eq!(
                report.has_hard_violations(),
                *expect_violation,
                "{name} @ resolution {res}: verdict changed\n{}",
                alice_lol::law::format_report(&report)
            );
            assert!(
                !report.has_unresolved(),
                "{name} @ resolution {res}: unresolved\n{}",
                alice_lol::law::format_report(&report)
            );
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 解像度以下の薄い隙間 / はみ出しは「判定不能」であって「合格」でも「違反」でもない
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 2 球の隙間 0.02 (cell 1.15 → 深さ 4 の葉 0.072 より薄い): 両表面を跨ぐ葉の中心は
/// 隙間の中 (両方外側) なので witness にならず、区間も決まらない → unresolved
/// 旧実装は「重なりなし」で silent 合格、witness 条件を裏返した実装は偽の違反
#[test]
fn gap_thinner_than_probe_resolution_is_unresolved() {
    use alice_lol::law::UnresolvedReason;
    let laws = vec![Law::hard(
        "apart",
        Constraint::NonOverlap {
            a: SdfNode::sphere(1.0),
            b: SdfNode::sphere(1.0).translate(2.02, 0.0, 0.0),
        },
    )
    .expect("provable constraint")];
    // grid 境界が球面 (x = 1.0 / 1.02) に乗らないよう −1.25 始点 (cell 1.15)
    let report = check_laws(
        &laws,
        &grid(Vec3::splat(-1.25), Vec3::new(3.35, 1.25, 1.25), 4),
    );
    assert!(
        report.violations.is_empty(),
        "隙間があるので違反ではない\n{}",
        alice_lol::law::format_report(&report)
    );
    assert!(
        report.has_unresolved(),
        "解像度以下の隙間を合格にしてはいけない"
    );
    assert!(!report.all_passed());
    assert_eq!(report.unresolved[0].reason, UnresolvedReason::SignUndecided);
    // 未決定の葉は隙間の中 (x ≈ 1.0..1.02) を含む
    let r = &report.unresolved[0].region;
    assert!(
        r.x.lo <= 1.02 && r.x.hi >= 1.0,
        "region {r:?} が隙間を含まない"
    );
}

/// 内球 0.99 in 外球 1.0 (殻 0.01 < 葉 0.045): はみ出してはいないが証明もできない
#[test]
fn containment_margin_thinner_than_probe_resolution_is_unresolved() {
    let laws = vec![Law::hard(
        "inside",
        Constraint::Containment {
            inner: SdfNode::sphere(0.99),
            outer: SdfNode::sphere(1.0),
        },
    )
    .expect("provable constraint")];
    let report = check_laws(&laws, &grid(Vec3::splat(-1.2), Vec3::splat(1.2), 4));
    assert!(
        report.violations.is_empty(),
        "はみ出してはいない\n{}",
        alice_lol::law::format_report(&report)
    );
    assert!(report.has_unresolved() && !report.all_passed());
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 複数の違反箇所から「最悪」を選ぶ (標本順に依存しない)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 厚さ 0.2 の板 (中央面まで 0.1) と厚さ 0.1 の板 (0.05) を x で並べ、t = 0.3:
/// 最悪 residual は薄い板の 0.05 − 0.3 = −0.25 板の並び順を両方試して、
/// 「最初に見つけた」でも「最後に見つけた」でもなく最悪が返ることを固定する
#[test]
fn min_thickness_reports_the_thinnest_region_regardless_of_sample_order() {
    for thin_first in [true, false] {
        let (xa, xb) = if thin_first { (-1.0, 1.0) } else { (1.0, -1.0) };
        let thin = SdfNode::box3d(1.0, 1.0, 0.1).translate(xa, 0.0, 0.0);
        let thick = SdfNode::box3d(1.0, 1.0, 0.2).translate(xb, 0.0, 0.0);
        let node = thin.union(thick);
        // z 中央面だけを標本にする (cell 中心 z = 0)
        let cfg = CheckConfig {
            aabb_min: Vec3::new(-1.5, -0.1, -0.01),
            aabb_max: Vec3::new(1.5, 0.1, 0.01),
            resolution: 6,
        };
        let report = check_laws(&min_thickness(node, 0.3), &cfg);
        assert!(report.has_hard_violations());
        let r = report.violations[0].residual;
        assert!(
            (r - (0.05 - 0.3)).abs() < 0.004,
            "thin_first={thin_first}: residual {r:.4} は薄い板の −0.25 のはず"
        );
    }
}

/// 2 組の重なり (侵入 0.1 と 0.3) を x で並べる: 最悪は深い方 (片側 −0.15)
#[test]
fn non_overlap_reports_the_deepest_overlap_regardless_of_sample_order() {
    for deep_first in [true, false] {
        let (xd, xs) = if deep_first { (-2.0, 2.0) } else { (2.0, -2.0) };
        let a = SdfNode::sphere(0.5)
            .translate(xd - 0.35, 0.0, 0.0)
            .union(SdfNode::sphere(0.5).translate(xs - 0.45, 0.0, 0.0));
        let b = SdfNode::sphere(0.5)
            .translate(xd + 0.35, 0.0, 0.0)
            .union(SdfNode::sphere(0.5).translate(xs + 0.45, 0.0, 0.0));
        let laws =
            vec![Law::hard("apart", Constraint::NonOverlap { a, b }).expect("provable constraint")];
        let cfg = CheckConfig {
            aabb_min: Vec3::new(-3.0, -0.05, -0.05),
            aabb_max: Vec3::new(3.0, 0.05, 0.05),
            resolution: 12,
        };
        let report = check_laws(&laws, &cfg);
        assert!(report.has_hard_violations());
        let r = report.violations[0].residual;
        // 深い方: 中心間 0.7、半径 0.5 → 侵入 0.3、中点で片側 −0.15
        assert!(
            (-0.15 - 0.02..=-0.15 + 0.03).contains(&r),
            "deep_first={deep_first}: residual {r:.4} は深い方 (≈ −0.15) のはず"
        );
    }
}

/// 内側の 2 球が外球からそれぞれ 0.05 / 0.2 はみ出す: 最悪は 0.2 側
#[test]
fn containment_reports_the_largest_overflow_regardless_of_sample_order() {
    for big_first in [true, false] {
        let (xb, xs) = if big_first { (-2.0, 2.0) } else { (2.0, -2.0) };
        let outer = SdfNode::sphere(1.0)
            .translate(xb, 0.0, 0.0)
            .union(SdfNode::sphere(1.0).translate(xs, 0.0, 0.0));
        let inner = SdfNode::sphere(0.5)
            .translate(xb + 0.7, 0.0, 0.0)
            .union(SdfNode::sphere(0.5).translate(xs + 0.55, 0.0, 0.0));
        let laws = vec![
            Law::hard("inside", Constraint::Containment { inner, outer })
                .expect("provable constraint"),
        ];
        let cfg = CheckConfig {
            aabb_min: Vec3::new(-3.5, -0.05, -0.05),
            aabb_max: Vec3::new(3.5, 0.05, 0.05),
            resolution: 14,
        };
        let report = check_laws(&laws, &cfg);
        assert!(report.has_hard_violations());
        let r = report.violations[0].residual;
        assert!(
            (-0.2 - 0.02..=-0.2 + 0.05).contains(&r),
            "big_first={big_first}: residual {r:.4} は大きい方 (≈ −0.2) のはず"
        );
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Contact の端: 上界なし / 閾値が解像度以下
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// gap 3.0 ≫ max 0.4: 遠すぎは区間で証明できるが、探索半径 (0.4 + cell 対角) では
/// 上界が取れない → residual は −∞、region は検査 AABB 全体
#[test]
fn contact_too_far_without_upper_bound_reports_neg_infinity() {
    let laws = vec![Law::hard(
        "mate",
        Constraint::Contact {
            a: SdfNode::sphere(1.0),
            b: SdfNode::sphere(1.0).translate(5.0, 0.0, 0.0),
            min_distance: 0.1,
            max_distance: 0.4,
        },
    )
    .expect("provable constraint")];
    let cfg = grid(Vec3::new(-1.2, -1.2, -1.2), Vec3::new(6.2, 1.2, 1.2), 10);
    let report = check_laws(&laws, &cfg);
    assert!(
        report.has_hard_violations(),
        "{}",
        alice_lol::law::format_report(&report)
    );
    let v = &report.violations[0];
    assert!(
        v.residual.is_infinite() && v.residual < 0.0,
        "residual {}",
        v.residual
    );
    assert!(
        v.region.x.lo <= -1.2 && v.region.x.hi >= 6.2,
        "region は AABB 全体"
    );
}

/// gap 0.5 に対し min 0.51: 真は違反 (近すぎ) だが、上界 (標本点経由の三角不等式、
/// ≈ 0.52) が min を超えるので違反を証明できず、gap > 0.51 も偽なので合格も
/// 証明できない → unresolved (`ub ≤ max` だけで合格にしてはいけない)
/// (gap > m の証明は区間演算で厳密なので、min 0.499 なら合格が証明できる)
#[test]
fn contact_margin_thinner_than_probe_resolution_is_unresolved() {
    use alice_lol::law::UnresolvedReason;
    let laws = vec![Law::hard(
        "mate",
        Constraint::Contact {
            a: SdfNode::sphere(1.0),
            b: SdfNode::sphere(1.0).translate(2.5, 0.0, 0.0),
            min_distance: 0.51,
            max_distance: 1.0,
        },
    )
    .expect("provable constraint")];
    let cfg = grid(Vec3::new(-1.2, -1.2, -1.2), Vec3::new(3.7, 1.2, 1.2), 10);
    let report = check_laws(&laws, &cfg);
    assert!(
        report.violations.is_empty(),
        "{}",
        alice_lol::law::format_report(&report)
    );
    assert!(report.has_unresolved());
    match &report.unresolved[0].reason {
        UnresolvedReason::GapUnbracketed { upper } => {
            assert!(
                (0.5..0.6).contains(upper),
                "upper {upper} は gap 0.5 の少し上のはず"
            );
        }
        other => panic!("unexpected reason {other:?}"),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Thermal / Continuity / VolumeConservation (0.5.0 で三値化する 3 法則)
//
// この 3 つは 0.4.0 まで oracle が 1 本も無く、`law_tests.rs` 側にあるのは
// **実装の出力を pin した変化検出器** だった (格子 count の proxy がどんな値を
// 返すかを固定しているだけで、幾何として正しいかは誰も見ていない)
//
// ここでは「幾何が閉じた式で分かる / 独立に数えられる scene」を使い、
// 検証器が **偽陽性 (真では満たしているのに違反と断言する)** を出さないことを
// 見る 違反の検出が健全でも、合格の側で嘘をつけば gate としては同じく壊れる
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 格子を歩き (表面を確実に含むセル数, 内部セル数) を **実装と独立に** 数える
///
/// 表面の判定は 8 角の符号反転 = 中間値定理 場の値を距離として使わないので、
/// 場が真の距離を過大 / 過小申告する node でも正しい
fn true_surface_and_interior(
    node: &SdfNode,
    src: Vec3,
    radius: f32,
    cfg: &CheckConfig,
) -> (usize, usize) {
    let n = cfg.resolution;
    #[allow(clippy::cast_precision_loss)]
    let step = (cfg.aabb_max - cfg.aabb_min) / n as f32;
    let (mut surface, mut interior) = (0usize, 0usize);
    for iz in 0..n {
        for iy in 0..n {
            for ix in 0..n {
                #[allow(clippy::cast_precision_loss)]
                let lo = cfg.aabb_min + step * Vec3::new(ix as f32, iy as f32, iz as f32);
                let center = lo + step * 0.5;
                if center.distance(src) > radius {
                    continue;
                }
                if alice_sdf::eval(node, center) < 0.0 {
                    interior += 1;
                }
                let (mut pos, mut neg) = (false, false);
                for c in 0..8u32 {
                    let corner = lo
                        + Vec3::new(
                            if c & 1 == 0 { 0.0 } else { step.x },
                            if c & 2 == 0 { 0.0 } else { step.y },
                            if c & 4 == 0 { 0.0 } else { step.z },
                        );
                    if alice_sdf::eval(node, corner) >= 0.0 {
                        pos = true;
                    } else {
                        neg = true;
                    }
                }
                if pos && neg {
                    surface += 1;
                }
            }
        }
    }
    (surface, interior)
}

/// Thermal の表面帯は **場のスケールで痩せてはいけない**
///
/// 実装は表面近傍を `|f(center)| < step` で取るが、gyroid の場は真の距離を
/// 約 1.7 倍に過大申告する (同 file `gyroid_sheet_is_thinner_than_its_field_claims`)
/// ので、この帯は真の距離でいう `step / 1.7` 相当まで痩せ、**表面セルを
/// 取りこぼして表面比を過小評価する**
///
/// 真の表面比は角の符号反転で独立に数え、その 95% を閾値にする
/// 真の比が閾値を上回っている以上、**違反を出したらそれは偽陽性**
#[test]
fn thermal_surface_ratio_must_not_shrink_with_the_field_scale() {
    let cfg = grid(Vec3::splat(-2.0), Vec3::splat(2.0), 20);
    let (src, radius) = (Vec3::ZERO, 1.5_f32);

    // 対照: 場 = 真の距離の球 これは現実装でも通る (原因の切り分け)
    let ball = SdfNode::sphere(1.5);
    let (s, i) = true_surface_and_interior(&ball, src, radius, &cfg);
    #[allow(clippy::cast_precision_loss)]
    let ball_thr = (s as f32 / i as f32) * 0.95;
    let ok = check_laws(
        &[Law::soft(
            "ball",
            1.0,
            Constraint::Thermal {
                node: ball,
                heat_sources: vec![src],
                search_radius: radius,
                min_surface_ratio: ball_thr,
            },
        )],
        &cfg,
    );
    assert!(
        ok.violations.is_empty(),
        "場 = 真の距離の球で偽陽性が出た (格子 count 自体の問題)\n{}",
        alice_lol::law::format_report(&ok)
    );

    // 本題: 場が過大申告する gyroid
    let gyroid = SdfNode::gyroid(1.0, 0.3);
    let (s, i) = true_surface_and_interior(&gyroid, src, radius, &cfg);
    assert!(i > 0 && s > 0, "検査対象に内部 / 表面セルが無い");
    #[allow(clippy::cast_precision_loss)]
    let true_ratio = s as f32 / i as f32;
    let report = check_laws(
        &[Law::soft(
            "gyroid",
            1.0,
            Constraint::Thermal {
                node: gyroid,
                heat_sources: vec![src],
                search_radius: radius,
                min_surface_ratio: true_ratio * 0.95,
            },
        )],
        &cfg,
    );
    assert!(
        report.violations.is_empty(),
        "真の表面比 {true_ratio:.4} は閾値 {:.4} を上回るのに違反と断言した \
         (|f| < step が場のスケールで痩せ、表面セルを取りこぼしている)\n{}",
        true_ratio * 0.95,
        alice_lol::law::format_report(&report)
    );
}

/// Continuity は **格子より細い接続を「分離」と断言してはいけない**
///
/// 半径 0.1 の首で繋いだ 2 球は連結 実装はセル中心の点標本だけで
/// 内部 mask を作るので、首が 1 セルより細いと中心が 1 つも内部に落ちず、
/// 首が存在しないかのように flood fill が途切れて **分離と断言する**
///
/// 正しくは「この解像度では決められない」= 違反ではない
#[test]
fn continuity_must_not_call_a_thin_neck_disconnected() {
    let neck = SdfNode::sphere(0.8)
        .translate(-1.0, 0.0, 0.0)
        .union(SdfNode::sphere(0.8).translate(1.0, 0.0, 0.0))
        .union(SdfNode::capsule(
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            0.1,
        ));
    // 首の中心 (原点) が内部であることを確認 = 幾何として確かに連結
    assert!(
        alice_sdf::eval(&neck, Vec3::ZERO) < 0.0,
        "首の中心が内部でない — scene の作り方が誤り"
    );

    for res in [8_usize, 16, 24] {
        let cfg = grid(Vec3::splat(-2.5), Vec3::splat(2.5), res);
        let report = check_laws(
            &[Law::soft(
                "connected",
                1.0,
                Constraint::Continuity {
                    node: neck.clone(),
                    seed_point: Vec3::new(-1.0, 0.0, 0.0),
                },
            )],
            &cfg,
        );
        assert!(
            report.violations.is_empty(),
            "res={res}: 半径 0.1 の首で繋がった 2 球を分離と断言した\n{}",
            alice_lol::law::format_report(&report)
        );
    }
}

/// `VolumeConservation` は **平行移動を体積変化と申告してはいけない**
///
/// 平行移動は体積を厳密に保存する (相対差 0) 実装はセル中心が内側に落ちるか
/// だけを数えるので、半セルずらすとどの中心が拾われるかが変わり、
/// **変化していない体積に対して数 % 〜 12% の差を申告する**
#[test]
fn volume_conservation_must_not_flag_a_translation() {
    for (res, shift) in [(8_usize, 0.25_f32), (16, 0.125), (24, 0.25)] {
        let cfg = grid(Vec3::splat(-2.0), Vec3::splat(2.0), res);
        let report = check_laws(
            &[Law::soft(
                "moved",
                1.0,
                Constraint::VolumeConservation {
                    before: SdfNode::sphere(1.0),
                    after: SdfNode::sphere(1.0).translate(shift, 0.0, 0.0),
                    relative_tolerance: 0.02,
                },
            )],
            &cfg,
        );
        assert!(
            report.violations.is_empty(),
            "res={res} shift={shift}: 平行移動 (真の体積差 0) を体積変化と申告した\n{}",
            alice_lol::law::format_report(&report)
        );
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 残差が距離であること (0.5.0)
//
// `NonOverlap` / `Containment` の判定 (符号だけを見る) は 0.4.0 で健全になったが、
// **報告される残差は場の値のまま**だった 場の値は距離ではないので、
// `top_violations` の順位と「侵入深さ」「はみ出し量」の数値が信用できない
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `NonOverlap` の残差は **侵入深さ** であって場の値ではない
///
/// A = 半径 1 の 2 球 (中心 ±0.5 on x) の union、原点の場は `min(fa, fb) = −0.5`
/// だが union 表面までの真の距離は 0.866 (同 file
/// `union_interior_is_thicker_than_its_field_claims` と同じ幾何)
///
/// B は原点から遠い大きな箱なので、侵入深さは A 側で決まって 0.866
/// 場の値 0.5 を残差にすると **侵入を 42% 浅く報告する**
#[test]
fn non_overlap_residual_is_a_distance_not_a_field_value() {
    let a = SdfNode::sphere(1.0)
        .translate(0.5, 0.0, 0.0)
        .union(SdfNode::sphere(1.0).translate(-0.5, 0.0, 0.0));
    let b = SdfNode::box3d(10.0, 10.0, 10.0);

    // 前提の確認: 原点は両方の内部、A の場は −0.5
    assert!((alice_sdf::eval(&a, Vec3::ZERO) - (-0.5)).abs() < 1e-5);
    assert!(alice_sdf::eval(&b, Vec3::ZERO) < -4.9);

    let laws = vec![Law::hard("overlap", Constraint::NonOverlap { a, b })
        .expect("provable constraint")];
    let report = check_laws(&laws, &single_cell(Vec3::ZERO, 0.01));
    assert!(report.has_hard_violations(), "原点は両方の内部なので重なり");

    let r = report.violations[0].residual;
    assert!(
        (-1.05..=-0.80).contains(&r),
        "residual {r:.4} が真の侵入深さ 0.866 の近傍にない \
         (場の値 −0.5 をそのまま報告していないか)"
    );
}

/// `Containment` の残差は **はみ出し量** であって場の値ではない
///
/// outer = 半径 1 の 2 球 (中心 0 と 1.8) の intersection = 薄いレンズ
/// (x 半幅 0.1、y/z 半径 0.436) intersection の外側の場 `max(f1, f2)` は
/// 真の距離を **過小** 申告する
///
/// p = (0.9, 0, 2.0) で場は 1.193 だが、レンズ上端 (0.9, 0, 0.436) までの
/// 真の距離は 1.564 場の値を残差にすると **はみ出しを 24% 小さく報告する**
#[test]
fn containment_residual_is_a_distance_not_a_field_value() {
    let outer = SdfNode::sphere(1.0).intersection(SdfNode::sphere(1.0).translate(1.8, 0.0, 0.0));
    let p = Vec3::new(0.9, 0.0, 2.0);
    let inner = SdfNode::sphere(0.2).translate(p.x, p.y, p.z);

    // 前提の確認: p は inner の内部かつ outer の外部、outer の場は約 1.193
    assert!(alice_sdf::eval(&inner, p) < 0.0);
    let field = alice_sdf::eval(&outer, p);
    assert!(
        (1.15..1.25).contains(&field),
        "outer の場 {field:.4} が想定 1.193 から外れた"
    );

    let laws = vec![Law::hard("inside", Constraint::Containment { inner, outer })
        .expect("provable constraint")];
    let report = check_laws(&laws, &single_cell(p, 0.01));
    assert!(report.has_hard_violations(), "p は outer の外なのではみ出し");

    let r = report.violations[0].residual;
    assert!(
        (-1.85..=-1.45).contains(&r),
        "residual {r:.4} が真のはみ出し量 1.564 の近傍にない \
         (場の値 −1.193 をそのまま報告していないか)"
    );
}
