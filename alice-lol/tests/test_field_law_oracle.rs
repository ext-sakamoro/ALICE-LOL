//! `GradientBound` と `Reachable` の 3 値が、どれも「証明」になっていること
//! を確かめる oracle。
//!
//! 判定器の 3 値は「合格 / 違反 / 未定」だが、この 2 法則では 3 つとも根拠が
//! 違う:
//!
//! | 値 | 根拠 |
//! |---|---|
//! | 合格 | `GradientBound` は静的上界 (`eval_lipschitz`) が上界以下という**証明**、`Reachable` は内部と確定したセルだけを辿る**経路** |
//! | 違反 | `GradientBound` は上界を超える**標本対**、`Reachable` は 2 点を隔てる**外部確定セル** (どんな経路もそこを通る) |
//! | 未定 | 標本は「超過が無いこと」を証明できない / 判定できないセルを経由しないと繋がらない |
//!
//! 「標本で見つからなかった」を合格に繰り上げないことが、この法則が gate と
//! して意味を持つ条件。合格は必ず証明から来る。

use alice_lol::law::{check_laws, CheckConfig, Constraint, Law, UnresolvedReason};
use alice_sdf::SdfNode;
use glam::Vec3;

fn config(half: f32, resolution: usize) -> CheckConfig {
    CheckConfig {
        aabb_min: Vec3::splat(-half),
        aabb_max: Vec3::splat(half),
        resolution,
    }
}

fn run(constraint: Constraint, cfg: &CheckConfig) -> (usize, usize) {
    let laws = vec![Law::hard("probe", constraint)];
    let report = check_laws(&laws, cfg);
    (report.violations.len(), report.unresolved.len())
}

// ─────────────────────────── GradientBound ───────────────────────────

#[test]
fn an_exact_field_passes_by_proof_without_sampling() {
    // 球の静的上界は 1 なので、標本を 1 点も見ずに合格が出る
    let (v, u) = run(
        Constraint::GradientBound {
            node: SdfNode::sphere(1.0),
            max_gradient: 1.0,
            probe: 1e-3,
        },
        &config(3.0, 8),
    );
    assert_eq!(
        (v, u),
        (0, 0),
        "exact field should pass by the static bound"
    );
}

#[test]
fn an_over_reporting_field_is_caught_with_a_witness() {
    // gyroid の場は距離ではない (静的上界 √3) 標本が実際に超過を見つける
    let cfg = config(3.0, 12);
    let laws = vec![Law::hard(
        "gyroid",
        Constraint::GradientBound {
            node: SdfNode::gyroid(2.0, 0.1),
            max_gradient: 1.0,
            probe: 1e-3,
        },
    )];
    let report = check_laws(&laws, &cfg);
    assert_eq!(report.violations.len(), 1, "expected a witness");
    assert_eq!(report.unresolved.len(), 0);
    let v = &report.violations[0];
    assert!(v.residual > 0.0, "residual is the excess over the bound");
    // 証拠点は検査領域の中にある
    assert!(v.point.x >= cfg.aabb_min.x && v.point.x <= cfg.aabb_max.x);
    assert!(v.region.x.lo <= v.point.x && v.point.x <= v.region.x.hi);
}

#[test]
fn a_loose_static_bound_with_no_witness_is_undecided_not_a_pass() {
    // twist の静的上界は捻り強度から出る保守的な値で 1 を大きく超えるが、
    // 軸のごく近傍だけを見れば実際の差分商は 1 前後にしかならない。
    // 「標本で見つからなかった」を合格にしてはいけない場面。
    let twisted = SdfNode::box3d(2.0, 2.0, 2.0).twist(2.0);
    let cfg = config(0.05, 4);
    let laws = vec![Law::hard(
        "twist_near_axis",
        Constraint::GradientBound {
            node: twisted,
            max_gradient: 1.2,
            probe: 1e-3,
        },
    )];
    let report = check_laws(&laws, &cfg);
    assert_eq!(
        report.violations.len(),
        0,
        "no witness should be found here"
    );
    assert_eq!(
        report.unresolved.len(),
        1,
        "and that must not read as a pass"
    );
    match report.unresolved[0].reason {
        UnresolvedReason::GradientUnwitnessed {
            claimed,
            worst_sampled,
        } => {
            assert!(claimed > 1.2, "the static bound is what failed to prove it");
            assert!(worst_sampled <= 1.2, "sampling found nothing above it");
        }
        ref other => panic!("unexpected reason: {other:?}"),
    }
}

#[test]
fn the_three_gradient_outcomes_are_all_reachable() {
    // 同じ法則が 3 値すべてを返せることを 1 本で押さえる
    let pass = run(
        Constraint::GradientBound {
            node: SdfNode::sphere(1.0),
            max_gradient: 1.0,
            probe: 1e-3,
        },
        &config(3.0, 6),
    );
    let violated = run(
        Constraint::GradientBound {
            node: SdfNode::gyroid(2.0, 0.1),
            max_gradient: 1.0,
            probe: 1e-3,
        },
        &config(3.0, 12),
    );
    let undecided = run(
        Constraint::GradientBound {
            node: SdfNode::box3d(2.0, 2.0, 2.0).twist(2.0),
            max_gradient: 1.2,
            probe: 1e-3,
        },
        &config(0.05, 4),
    );
    assert_eq!(pass, (0, 0));
    assert_eq!(violated, (1, 0));
    assert_eq!(undecided, (0, 1));
}

// ───────────────────────────── Reachable ─────────────────────────────

#[test]
fn two_points_inside_one_solid_are_reachable_by_a_proven_path() {
    let (v, u) = run(
        Constraint::Reachable {
            node: SdfNode::sphere(2.0),
            from: Vec3::new(-1.0, 0.0, 0.0),
            to: Vec3::new(1.0, 0.0, 0.0),
        },
        &config(3.0, 12),
    );
    assert_eq!((v, u), (0, 0), "a path of proven-interior cells exists");
}

#[test]
fn two_disjoint_solids_are_proven_unreachable() {
    // 離れた 2 球 どんな経路も外部と確定したセルを通る = 到達不能の証明
    let a = SdfNode::sphere(0.8).translate(-2.0, 0.0, 0.0);
    let b = SdfNode::sphere(0.8).translate(2.0, 0.0, 0.0);
    let cfg = config(4.0, 16);
    let laws = vec![Law::hard(
        "split",
        Constraint::Reachable {
            node: a.union(b),
            from: Vec3::new(-2.0, 0.0, 0.0),
            to: Vec3::new(2.0, 0.0, 0.0),
        },
    )];
    let report = check_laws(&laws, &cfg);
    assert_eq!(report.violations.len(), 1, "separation is provable here");
    assert_eq!(report.unresolved.len(), 0);
    assert!(report.violations[0].residual < 0.0);
}

#[test]
fn an_endpoint_outside_the_solid_is_a_violation_not_a_pass() {
    let (v, u) = run(
        Constraint::Reachable {
            node: SdfNode::sphere(1.0),
            from: Vec3::ZERO,
            to: Vec3::new(2.5, 0.0, 0.0),
        },
        &config(4.0, 12),
    );
    assert_eq!(v, 1, "a point outside the solid cannot be a destination");
    assert_eq!(u, 0);
}

#[test]
fn a_passage_the_grid_cannot_resolve_is_undecided() {
    // 2 球を、セルより細い管でつなぐ 管のセルは内部とも外部とも確定しない
    // ので、内部確定だけでは届かず、外部確定だけでは隔てられもしない
    let cfg = config(4.0, 12); // cell 幅 = 8/12 ≈ 0.67
    let a = SdfNode::sphere(0.9).translate(-1.6, 0.0, 0.0);
    let b = SdfNode::sphere(0.9).translate(1.6, 0.0, 0.0);
    let bridge = SdfNode::capsule(
        Vec3::new(-1.6, 0.0, 0.0),
        Vec3::new(1.6, 0.0, 0.0),
        0.05, // cell 幅よりずっと細い
    );
    let laws = vec![Law::hard(
        "thin_bridge",
        Constraint::Reachable {
            node: a.union(b).union(bridge),
            from: Vec3::new(-1.6, 0.0, 0.0),
            to: Vec3::new(1.6, 0.0, 0.0),
        },
    )];
    let report = check_laws(&laws, &cfg);
    assert_eq!(
        report.violations.len(),
        0,
        "a bridge exists, so unreachability must not be claimed"
    );
    assert_eq!(
        report.unresolved.len(),
        1,
        "but the grid cannot prove the path either"
    );
    assert!(matches!(
        report.unresolved[0].reason,
        UnresolvedReason::ReachabilityUndecided { .. }
    ));
}

#[test]
fn raising_the_resolution_turns_undecided_into_a_pass() {
    // 未定は「検証器の解像度が足りない」であって「繋がっていない」ではない
    let a = SdfNode::sphere(0.9).translate(-1.6, 0.0, 0.0);
    let b = SdfNode::sphere(0.9).translate(1.6, 0.0, 0.0);
    let bridge = SdfNode::capsule(Vec3::new(-1.6, 0.0, 0.0), Vec3::new(1.6, 0.0, 0.0), 0.8);
    let node = a.union(b).union(bridge);
    let coarse = run(
        Constraint::Reachable {
            node: node.clone(),
            from: Vec3::new(-1.6, 0.0, 0.0),
            to: Vec3::new(1.6, 0.0, 0.0),
        },
        &config(4.0, 8),
    );
    let fine = run(
        Constraint::Reachable {
            node,
            from: Vec3::new(-1.6, 0.0, 0.0),
            to: Vec3::new(1.6, 0.0, 0.0),
        },
        &config(4.0, 32),
    );
    assert_eq!(coarse.0, 0, "a real bridge is never a violation");
    assert_eq!(fine, (0, 0), "at a fine enough grid the path is proven");
}

#[test]
fn the_three_reachability_outcomes_are_all_reachable() {
    let pass = run(
        Constraint::Reachable {
            node: SdfNode::sphere(2.0),
            from: Vec3::new(-1.0, 0.0, 0.0),
            to: Vec3::new(1.0, 0.0, 0.0),
        },
        &config(3.0, 12),
    );
    let violated = run(
        Constraint::Reachable {
            node: SdfNode::sphere(0.8)
                .translate(-2.0, 0.0, 0.0)
                .union(SdfNode::sphere(0.8).translate(2.0, 0.0, 0.0)),
            from: Vec3::new(-2.0, 0.0, 0.0),
            to: Vec3::new(2.0, 0.0, 0.0),
        },
        &config(4.0, 16),
    );
    let undecided = run(
        Constraint::Reachable {
            node: SdfNode::sphere(0.9)
                .translate(-1.6, 0.0, 0.0)
                .union(SdfNode::sphere(0.9).translate(1.6, 0.0, 0.0))
                .union(SdfNode::capsule(
                    Vec3::new(-1.6, 0.0, 0.0),
                    Vec3::new(1.6, 0.0, 0.0),
                    0.05,
                )),
            from: Vec3::new(-1.6, 0.0, 0.0),
            to: Vec3::new(1.6, 0.0, 0.0),
        },
        &config(4.0, 12),
    );
    assert_eq!(pass, (0, 0));
    assert_eq!(violated, (1, 0));
    assert_eq!(undecided, (0, 1));
}
