//! `ThermalField` (実温度場) の法則レベル oracle
//!
//! `physics` feature 専用 (`alice-physics` は AGPL-3.0-or-later、有効化すると
//! 下流に伝播する)
//!
//! solver 自体の物理 (エネルギー保存 / 境界条件の単調性と飽和 / 決定性) は
//! `law.rs` の `evidence_gate_tests` 側で白箱に固定している こちらは
//! **法則として 3 値が 3 分岐とも出るか** と、判定の向きが物理と合っているかを見る
#![cfg(feature = "physics")]

use alice_lol::law::{check_laws, CheckConfig, Constraint, Evidence, Law, UnresolvedReason};
use alice_physics::transient_thermal::{TemperatureDependence as Dep, ThermalMaterial};
use alice_sdf::SdfNode;
use glam::Vec3;

/// 温度依存のない PLA 近似 (閉形式で追えるようにする)
///
/// 熱伝導率が低いので小さな発熱でも十分な温度差が付き、両端が離れるので
/// 3 値の切り替えを見られる (アルミ相当の `k = 200` だと 0.4 K しか上がらず
/// 判定が動かない、2026-09-29 実測)
const fn pla_like() -> ThermalMaterial {
    ThermalMaterial {
        name: "oracle_pla_const",
        conductivity: Dep::Constant(0.13),
        specific_heat: Dep::Constant(1200.0),
        density: Dep::Constant(1240.0),
        reference_temperature: 293.15,
    }
}

// 較正済 scene — 半径 0.35 単位の球、1 単位 = 20 mm、周囲 20 °C
//
// ━━ 測定の出所 ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 最高温度の bracket = **[44.13, 307.76] °C**
//   測定日   2026-09-29
//   commit   cc07ac8 (= 117b089 で形状側を `Inside` に直した後)
//   機械     arm64 / macOS 26.3
//
// ⚠️ **値が動いていたら、まず実装が変わったかを見ること** (再測定か回帰かの
// 判別が付かないと直せない) `thermal_solve_is_bit_reproducible` が同一入力の
// bit 一致を固定しているので、**同じ commit なら機械が変わっても同じ値が出る
// はず** — 異機種で食い違ったらそれ自体が発見なので、出所を残しておく
//
// 履歴: `117b089` の前は `[46.08, 104.06]` だった (両 run が
// `Inside ∪ Undecided` で解いていた頃) 形状側を最小材料に直したことで
// **上界が 104 → 308 と 3 倍広がり、下界も 46.08 → 44.13 に下がった** =
// 旧実装の上界が 3 倍過小だった (偽の合格を出しえた) ことの定量
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

const SCENE_METRES_PER_UNIT: f32 = 0.02;
const SCENE_DURATION_S: f32 = 5000.0;
const SCENE_WATTS: f32 = 0.05;
/// bracket `[44.13, 307.76]` を跨ぐ閾値 (上の測定の出所を参照)
const SCENE_PROBE_C: f32 = 80.0;

/// 1 m 角の立方格子 (`transient_step_3d` が要求する立方セル)
const fn cube_grid(n: usize) -> CheckConfig {
    CheckConfig {
        aabb_min: Vec3::splat(-0.5),
        aabb_max: Vec3::splat(0.5),
        resolution: n,
    }
}

fn law(max_temperature_c: f32, duration_s: f32, watts: f32) -> Vec<Law> {
    vec![Law::soft(
        "heat",
        1.0,
        Constraint::ThermalField {
            node: SdfNode::sphere(0.35),
            sources: vec![(Vec3::ZERO, watts)],
            material: pla_like(),
            metres_per_unit: SCENE_METRES_PER_UNIT,
            ambient_c: 20.0,
            max_temperature_c,
            duration_s,
        },
    )]
}

/// 3 値が 3 分岐とも出る — 上限を動かすだけで違反 / 未定 / 合格が切り替わる
///
/// 境界条件 (対流熱伝達率 `h`) は設計時に決まらないので両端で挟む。
/// `h = ∞` (最大冷却) が下界、`h = 0` (断熱) が上界なので:
///
/// - 上限が下界より低い → **違反** (どれだけ冷やしても超える)
/// - 上限が上界より高い → **合格** (一切冷えなくても収まる)
/// - その間 → **未定** (`h` 次第で結論が変わる)
#[test]
fn verdict_is_three_valued_across_the_cooling_bracket() {
    let cfg = cube_grid(9);
    let (duration, watts) = (SCENE_DURATION_S, SCENE_WATTS);

    // まず未定を出して、その区間の両端を読む (閾値を勘で置かない)
    let probe = check_laws(&law(SCENE_PROBE_C, duration, watts), &cfg);
    let (lo_c, hi_c) = match probe.unresolved.first().map(|u| &u.reason) {
        Some(UnresolvedReason::TemperatureUnbracketed { lo_c, hi_c }) => (*lo_c, *hi_c),
        other => {
            panic!("上限 {SCENE_PROBE_C}°C は bracket を跨ぐはず (実際: {other:?} / {probe:?})")
        }
    };
    assert!(
        lo_c < hi_c,
        "下界 {lo_c:.3} が上界 {hi_c:.3} 以上 = bracket が潰れている"
    );
    assert!(
        lo_c < SCENE_PROBE_C && hi_c > SCENE_PROBE_C,
        "probe の上限 {SCENE_PROBE_C}°C が bracket [{lo_c:.3}, {hi_c:.3}] を跨いでいない"
    );
    assert!(
        lo_c > 20.0,
        "下界 {lo_c:.3} が周囲温度以下 = 発熱が効いていない"
    );

    // 下界より下の上限 → 違反
    let violated = check_laws(&law(lo_c - 1.0, duration, watts), &cfg);
    assert_eq!(
        violated.violations.len(),
        1,
        "上限が最高温度の下界 {lo_c:.3} を下回るのに違反が出ない"
    );
    let v = &violated.violations[0];
    assert!(
        matches!(v.evidence, Evidence::Modelled { .. }),
        "実温度場でも数値解なので Modelled のはず (実際 {:?})",
        v.evidence
    );
    assert!(
        (v.residual - (lo_c - 1.0 - lo_c)).abs() < 1.0e-3,
        "residual {:.4} が上限と下界の差 −1.0 K と合わない",
        v.residual
    );

    // 上界より上の上限 → 合格
    let passed = check_laws(&law(hi_c + 1.0, duration, watts), &cfg);
    assert!(
        passed.all_passed(),
        "上限が最高温度の上界 {hi_c:.3} を上回るのに合格しない: {passed:?}"
    );
}

/// 発熱が大きいほど最高温度は上がる — 判定の向きが物理と合っている
///
/// 同じ上限で発熱だけを増やせば、合格 → 未定 → 違反 の順に落ちていくはず
#[test]
fn more_power_never_makes_the_verdict_milder() {
    let cfg = cube_grid(9);
    let duration = SCENE_DURATION_S;
    // 0 = 合格 / 1 = 未定 / 2 = 違反 として単調性を見る
    let severity = |watts: f32| {
        let r = check_laws(&law(60.0, duration, watts), &cfg);
        if r.violations.is_empty() {
            u8::from(r.has_unresolved())
        } else {
            2
        }
    };
    let mut previous = 0;
    for watts in [0.0_f32, 0.005, 0.05, 0.5, 5.0] {
        let s = severity(watts);
        assert!(
            s >= previous,
            "発熱 {watts} W で判定が緩くなった ({previous} → {s}) = 向きが逆"
        );
        previous = s;
    }
    assert_eq!(previous, 2, "5 W でも違反にならない");
    assert_eq!(severity(0.0), 0, "発熱 0 W で合格にならない");
    assert_eq!(severity(SCENE_WATTS), 1, "較正済の 0.05 W で未定にならない");
}

/// `Priority::Hard` は名乗れない (数値解であってモデルなので)
#[test]
fn thermal_field_cannot_claim_hard_priority() {
    let c = Constraint::ThermalField {
        node: SdfNode::sphere(0.35),
        sources: vec![(Vec3::ZERO, 50.0)],
        material: pla_like(),
        metres_per_unit: SCENE_METRES_PER_UNIT,
        ambient_c: 20.0,
        max_temperature_c: 60.0,
        duration_s: SCENE_DURATION_S,
    };
    let err = Law::hard("heat", c).expect_err("ThermalField が Hard を名乗れてしまった");
    assert_eq!(err.constraint, "ThermalField");
    // 挟んでいる 2 軸 (境界条件 h と形状) が読み手に伝わること
    for token in ["境界条件", "形状", "断熱", "等温"] {
        assert!(
            err.model.contains(token),
            "model に {token} が無い = 何を挟んでいるか読み手に伝わらない ({})",
            err.model
        );
    }
}

/// solver の前提を満たさない格子は **黙って通さず** 理由つきで未定にする
#[test]
fn unusable_grids_are_reported_not_silently_passed() {
    let material = pla_like();
    let make = |cfg: CheckConfig, metres_per_unit: f32| {
        let laws = vec![Law::soft(
            "heat",
            1.0,
            Constraint::ThermalField {
                node: SdfNode::sphere(0.35),
                sources: vec![(Vec3::ZERO, 50.0)],
                material,
                metres_per_unit,
                ambient_c: 20.0,
                max_temperature_c: 60.0,
                duration_s: SCENE_DURATION_S,
            },
        )];
        check_laws(&laws, &cfg)
    };

    // 各軸 3 セル未満
    let tiny = make(cube_grid(2), 1.0);
    // セルが立方でない
    let anisotropic = make(
        CheckConfig {
            aabb_min: Vec3::new(-0.5, -0.5, -0.5),
            aabb_max: Vec3::new(0.5, 0.5, 2.0),
            resolution: 9,
        },
        1.0,
    );
    // 単位が不正
    let bad_units = make(cube_grid(9), 0.0);

    for (label, report) in [
        ("3 セル未満", tiny),
        ("非立方セル", anisotropic),
        ("metres_per_unit = 0", bad_units),
    ] {
        assert!(
            report.violations.is_empty(),
            "{label}: 解けていないのに違反を断言した"
        );
        assert!(
            !report.all_passed(),
            "{label}: 解けていないのに合格を名乗った"
        );
        assert!(
            matches!(
                report.unresolved[0].reason,
                UnresolvedReason::ThermalGridUnusable { .. }
            ),
            "{label}: 理由が ThermalGridUnusable でない ({:?})",
            report.unresolved[0].reason
        );
    }
}

/// 熱源が材料に乗らない scene は **黙って合格せず** 理由つきで未定にする
///
/// 0.4.0 は「形状の外の熱源は効かない」と黙って捨てていた = 熱が入らず
/// 上界も周囲温度になり **偽の合格**になる経路だった (`Inside ∪ Undecided` で
/// 解いていたので踏みにくかっただけで、`Inside` に絞ったことで顕在化した
/// 元からある穴であって、修正で新しく作った穴ではない)
#[test]
fn a_source_outside_the_material_is_not_silently_passed() {
    let cfg = cube_grid(9);
    let make = |source: Vec3| {
        check_laws(
            &[Law::soft(
                "heat",
                1.0,
                Constraint::ThermalField {
                    node: SdfNode::sphere(0.35),
                    sources: vec![(source, SCENE_WATTS)],
                    material: pla_like(),
                    metres_per_unit: SCENE_METRES_PER_UNIT,
                    ambient_c: 20.0,
                    // 周囲温度より少しでも上がれば違反になる厳しい上限
                    max_temperature_c: 20.5,
                    duration_s: SCENE_DURATION_S,
                },
            )],
            &cfg,
        )
    };

    // 球の外 (材料でない) / 検査 AABB の外
    for (label, source) in [
        ("球の外", Vec3::new(0.45, 0.0, 0.0)),
        ("AABB の外", Vec3::new(5.0, 0.0, 0.0)),
    ] {
        let report = make(source);
        assert!(
            !report.all_passed(),
            "{label}: 熱源が効いていないのに合格を名乗った"
        );
        assert!(
            matches!(
                report.unresolved[0].reason,
                UnresolvedReason::ThermalGridUnusable { .. }
            ),
            "{label}: 理由が ThermalGridUnusable でない ({:?})",
            report.unresolved[0].reason
        );
    }

    // 対照: 中心 (材料の内部) なら解けて判定が出る
    let ok = make(Vec3::ZERO);
    assert!(
        !ok.unresolved
            .iter()
            .any(|u| matches!(u.reason, UnresolvedReason::ThermalGridUnusable { .. })),
        "材料の内部に置いた熱源まで unusable にしている: {ok:?}"
    );
}
