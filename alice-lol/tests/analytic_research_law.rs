//! `research_law` の解析解突合テスト (analytic oracle)
//!
//! 期待値はすべて **この file に書いた閉じた式**から計算する (実装の出力を
//! pin する golden ではない):
//!
//! - 理想気体 `P = n R T / V` (R = 8.314462618 J/(mol K))
//! - 自由落下 `s = g t² / 2` (g = 9.80665 m/s²)
//! - 単位換算 1 kPa = 1000 Pa / 1 L = 1e-3 m³
//!
//! 判定規則 (`ingest`) は `alice_zip::law` と同じ順序: 証拠なし → 範囲外 →
//! 支持 → parameter 更新 → residual 増 → 破綻

// 厳密比較は「同じ閉じた式を同じ順序で計算した値」との比較にだけ使う
// 期待値は書いた通りの順序で計算する (fused mul_add に置き換えない)
#![allow(
    clippy::float_cmp,
    clippy::unwrap_used,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops
)]

use alice_lol::research_law::{
    compare, Bridge, LawExpr, Observation, Param, ResearchLaw, ResearchLawError, ResearchOracle,
    ResearchVerdict, Unit, Var,
};
use alice_zip::law::{IngestPolicy, Provenance, ValidRange};

const R: f64 = 8.314_462_618;
const G0: f64 = 9.806_65;

const fn range(lo: f64, hi: f64) -> ValidRange {
    ValidRange { lo, hi }
}

fn prov() -> Provenance {
    Provenance::new("closed form", "stated")
}

/// SI の理想気体 (P [Pa], n [mol], T [K], V [m^3])
fn ideal_gas_si() -> ResearchLaw {
    ResearchLaw::new(
        "ideal gas (SI)",
        "n*R*T/V",
        Var::new("P", "Pa"),
        &[
            Var::new("n", "mol"),
            Var::new("T", "K"),
            Var::new("V", "m^3"),
        ],
        &[Param::new("R", R, 0.0, "J/(mol*K)")],
        &[
            ("n", range(0.01, 100.0)),
            ("T", range(1.0, 2000.0)),
            ("V", range(1e-6, 10.0)),
        ],
        prov(),
    )
    .unwrap()
}

/// 同じ法則を kPa と L で書いたもの
fn ideal_gas_kpa_l() -> ResearchLaw {
    ResearchLaw::new(
        "ideal gas (kPa, L)",
        "n*R*T/V",
        Var::new("P", "kPa"),
        &[Var::new("n", "mol"), Var::new("T", "K"), Var::new("V", "L")],
        &[Param::new("R", R, 0.0, "J/(mol*K)")],
        &[
            ("n", range(0.01, 100.0)),
            ("T", range(1.0, 2000.0)),
            ("V", range(1e-3, 1e4)),
        ],
        prov(),
    )
    .unwrap()
}

fn free_fall(g: f64) -> ResearchLaw {
    ResearchLaw::new(
        "free fall",
        "0.5*g*t^2",
        Var::new("s", "m"),
        &[Var::new("t", "s")],
        &[Param::new("g", g, 0.0, "m/s^2")],
        &[("t", range(0.0, 10.0))],
        prov(),
    )
    .unwrap()
}

fn s_closed(g: f64, t: f64) -> f64 {
    0.5 * g * t * t
}

const fn policy() -> IngestPolicy {
    IngestPolicy {
        abs_tolerance: 0.01,
        break_factor: 4.0,
    }
}

// ── 単位 ────────────────────────────────────────────────────────────

#[test]
fn unit_parser_scales_and_dimensions() {
    let pa = Unit::parse("Pa").unwrap();
    let kpa = Unit::parse("kPa").unwrap();
    assert_eq!(pa.dimension(), kpa.dimension());
    assert_eq!(pa.scale(), 1.0);
    assert_eq!(kpa.scale(), 1000.0);
    // Pa = kg m^-1 s^-2  (L, M, T, I, Θ, N, J)
    assert_eq!(pa.dimension().exponents(), [-1, 1, -2, 0, 0, 0, 0]);
    // J/(mol*K) = kg m^2 s^-2 mol^-1 K^-1
    assert_eq!(
        Unit::parse("J/(mol*K)").unwrap().dimension().exponents(),
        [2, 1, -2, 0, -1, -1, 0]
    );
    let litre = Unit::parse("L").unwrap();
    let m3 = Unit::parse("m^3").unwrap();
    assert_eq!(litre.dimension(), m3.dimension());
    assert_eq!(litre.scale(), 1e-3);
    assert_eq!(m3.scale(), 1.0);
    assert_eq!(
        Unit::parse("m/s^2").unwrap().dimension().exponents(),
        [1, 0, -2, 0, 0, 0, 0]
    );
    assert_eq!(
        Unit::parse("N").unwrap().dimension(),
        Unit::parse("kg*m/s^2").unwrap().dimension()
    );
    assert!(Unit::parse("1").unwrap().dimension().is_dimensionless());
    for base in ["m", "kg", "s", "A", "K", "mol", "cd"] {
        let d = Unit::parse(base).unwrap().dimension().exponents();
        assert_eq!(d.iter().map(|e| e.unsigned_abs()).sum::<u8>(), 1, "{base}");
    }
}

#[test]
fn unknown_unit_is_rejected() {
    assert!(matches!(
        Unit::parse("furlong"),
        Err(ResearchLawError::UnknownUnit(u)) if u == "furlong"
    ));
    let r = ResearchLaw::new(
        "x",
        "t",
        Var::new("y", "parsec_ish"),
        &[Var::new("t", "s")],
        &[],
        &[],
        prov(),
    );
    assert!(matches!(r, Err(ResearchLawError::UnknownUnit(_))));
}

// ── 理想気体 ───────────────────────────────────────────────────────

#[test]
fn ideal_gas_matches_closed_form() {
    let law = ideal_gas_si();
    for &(n, t, v) in &[
        (1.0, 273.15, 0.022_414),
        (2.0, 300.0, 0.05),
        (0.5, 1000.0, 1.0),
        (10.0, 77.0, 0.001),
    ] {
        let expected = n * R * t / v;
        let got = law.evaluate(&[("n", n), ("T", t), ("V", v)]).unwrap();
        assert!(
            ((got - expected) / expected).abs() <= 1e-14,
            "n={n} T={t} V={v}: {got} vs {expected}"
        );
    }
}

#[test]
fn ideal_gas_in_kpa_and_litre_converts_units() {
    let law = ideal_gas_kpa_l();
    // 1 mol, 273.15 K, 22.414 L → P = R·273.15/0.022414 Pa → /1000 kPa
    let expected_kpa = R * 273.15 / 0.022_414 / 1000.0;
    let got = law
        .evaluate(&[("n", 1.0), ("T", 273.15), ("V", 22.414)])
        .unwrap();
    assert!(
        ((got - expected_kpa) / expected_kpa).abs() <= 1e-12,
        "{got}"
    );
}

#[test]
fn dimension_errors_are_rejected() {
    // 圧力 + 体積
    let r = ResearchLaw::new(
        "bad sum",
        "n*R*T/V + V",
        Var::new("P", "Pa"),
        &[
            Var::new("n", "mol"),
            Var::new("T", "K"),
            Var::new("V", "m^3"),
        ],
        &[Param::new("R", R, 0.0, "J/(mol*K)")],
        &[],
        prov(),
    );
    assert!(
        matches!(r, Err(ResearchLawError::IncompatibleDimensions { .. })),
        "{r:?}"
    );
    // exp の引数が温度
    let r = ResearchLaw::new(
        "bad exp",
        "exp(T)",
        Var::new("y", "1"),
        &[Var::new("T", "K")],
        &[],
        &[],
        prov(),
    );
    assert!(
        matches!(r, Err(ResearchLawError::DimensionfulArgument { .. })),
        "{r:?}"
    );
    // 未知の識別子
    let r = ResearchLaw::new(
        "bad ident",
        "n*R*T/W",
        Var::new("P", "Pa"),
        &[
            Var::new("n", "mol"),
            Var::new("T", "K"),
            Var::new("V", "m^3"),
        ],
        &[Param::new("R", R, 0.0, "J/(mol*K)")],
        &[],
        prov(),
    );
    assert!(
        matches!(&r, Err(ResearchLawError::UnknownIdentifier(id)) if id == "W"),
        "{r:?}"
    );
    // 式の次元が出力と違う (P を J で宣言)
    let r = ResearchLaw::new(
        "bad output",
        "n*R*T/V",
        Var::new("P", "J"),
        &[
            Var::new("n", "mol"),
            Var::new("T", "K"),
            Var::new("V", "m^3"),
        ],
        &[Param::new("R", R, 0.0, "J/(mol*K)")],
        &[],
        prov(),
    );
    assert!(
        matches!(r, Err(ResearchLawError::OutputDimension { .. })),
        "{r:?}"
    );
    // 次元の指数が整数にならない
    let r = ResearchLaw::new(
        "half power",
        "t^0.5",
        Var::new("y", "1"),
        &[Var::new("t", "s")],
        &[],
        &[],
        prov(),
    );
    assert!(
        matches!(r, Err(ResearchLawError::FractionalDimension)),
        "{r:?}"
    );
    // sqrt(m^2) = m は通る
    assert!(ResearchLaw::new(
        "sqrt",
        "sqrt(a)",
        Var::new("y", "m"),
        &[Var::new("a", "m^2")],
        &[],
        &[],
        prov(),
    )
    .is_ok());
}

// ── 自由落下 + oracle ─────────────────────────────────────────────

#[test]
fn free_fall_oracles_from_closed_form_pass_and_wrong_one_fails() {
    let law = free_fall(G0)
        .with_oracle(ResearchOracle::new(
            &[("t", 1.0)],
            s_closed(G0, 1.0),
            1e-12,
            "closed form",
        ))
        .with_oracle(ResearchOracle::new(
            &[("t", 3.0)],
            s_closed(G0, 3.0),
            1e-12,
            "closed form",
        ))
        // 故意に誤った値 (g = 9.81 で計算)
        .with_oracle(ResearchOracle::new(
            &[("t", 2.0)],
            s_closed(9.81, 2.0),
            1e-6,
            "wrong on purpose",
        ))
        .with_oracle(ResearchOracle::new(
            &[("t", 12.0)],
            s_closed(G0, 12.0),
            1e-6,
            "outside the range",
        ));
    let out = law.check_oracles();
    assert_eq!(out.len(), 4);
    assert!(out[0].passed && out[1].passed);
    assert!(!out[2].passed);
    let err = out[2].error.unwrap();
    assert!((err - 2.0 * (9.81 - G0)).abs() < 1e-12, "{err}");
    assert!(!out[3].passed);
    assert!(out[3].error.is_none());
    assert!(matches!(
        &out[3].value,
        Err(ResearchLawError::OutOfRange { variable }) if variable == "t"
    ));
}

#[test]
fn evaluate_refuses_to_extrapolate() {
    let law = free_fall(G0);
    assert!(matches!(
        law.evaluate(&[("t", 10.5)]),
        Err(ResearchLawError::OutOfRange { variable }) if variable == "t"
    ));
    assert!(matches!(
        law.evaluate(&[("t", -0.1)]),
        Err(ResearchLawError::OutOfRange { .. })
    ));
    // 境界は含む
    assert_eq!(law.evaluate(&[("t", 10.0)]).unwrap(), s_closed(G0, 10.0));
    assert!(matches!(
        law.evaluate(&[]),
        Err(ResearchLawError::MissingVariable(v)) if v == "t"
    ));
    assert!(matches!(
        law.evaluate(&[("t", 1.0), ("x", 1.0)]),
        Err(ResearchLawError::UnknownVariable(v)) if v == "x"
    ));
}

// ── compare ──────────────────────────────────────────────────────

#[test]
fn compare_si_and_kpa_litre_laws_agree_under_bridge() {
    let a = ideal_gas_si();
    let b = ideal_gas_kpa_l();
    let bridge = Bridge::new(&[("n", "n"), ("T", "T"), ("V", "V")]);
    for &(n, t, v) in &[
        (1.0, 273.15, 0.022_414),
        (3.0, 450.0, 0.2),
        (0.1, 20.0, 5.0),
    ] {
        let c = compare(&a, &b, &bridge, &[("n", n), ("T", t), ("V", v)]).unwrap();
        let expected = n * R * t / v;
        assert!(((c.a_value - expected) / expected).abs() <= 1e-14);
        assert!(((c.b_value_in_a_unit - expected) / expected).abs() <= 1e-12);
        assert!(c.relative <= 1e-9, "relative {}", c.relative);
        assert!(c.difference.abs() <= 1e-9 * expected);
    }
}

#[test]
fn compare_detects_a_real_difference() {
    // b の R を 1% 大きくすると、b/a = 1.01
    let a = ideal_gas_si();
    let b = ResearchLaw::new(
        "ideal gas (kPa, L), R +1%",
        "n*R*T/V",
        Var::new("P", "kPa"),
        &[Var::new("n", "mol"), Var::new("T", "K"), Var::new("V", "L")],
        &[Param::new("R", R * 1.01, 0.0, "J/(mol*K)")],
        &[],
        prov(),
    )
    .unwrap();
    let bridge = Bridge::new(&[("n", "n"), ("T", "T"), ("V", "V")]);
    let c = compare(&a, &b, &bridge, &[("n", 1.0), ("T", 300.0), ("V", 0.03)]).unwrap();
    let pa = R * 300.0 / 0.03;
    assert!((c.difference - (pa - 1.01 * pa)).abs() <= 1e-9 * pa);
    assert!((c.relative - 0.01 / 1.01).abs() <= 1e-9);
}

#[test]
fn bridge_between_different_dimensions_is_an_error() {
    let a = ideal_gas_si();
    let b = ideal_gas_kpa_l();
    // a の V (m^3) を b の T (K) に写す
    let bridge = Bridge::new(&[("n", "n"), ("T", "V"), ("V", "T")]);
    let r = compare(&a, &b, &bridge, &[("n", 1.0), ("T", 300.0), ("V", 0.03)]);
    assert!(
        matches!(r, Err(ResearchLawError::IncompatibleDimensions { .. })),
        "{r:?}"
    );
    // 出力の次元が違う (圧力 vs 長さ)
    let fall = free_fall(G0);
    let bridge = Bridge::new(&[("t", "T")]);
    let r = compare(&fall, &a, &bridge, &[("t", 1.0)]);
    assert!(r.is_err(), "{r:?}");
    // b の入力が bridge で覆われていない
    let bridge = Bridge::new(&[("n", "n"), ("T", "T")]);
    let r = compare(&a, &b, &bridge, &[("n", 1.0), ("T", 300.0), ("V", 0.03)]);
    assert!(
        matches!(&r, Err(ResearchLawError::MissingVariable(v)) if v == "V"),
        "{r:?}"
    );
}

// ── ingest ───────────────────────────────────────────────────────

fn obs(g: f64, ts: &[f64]) -> Vec<Observation> {
    ts.iter()
        .map(|&t| Observation::new(&[("t", t)], s_closed(g, t)))
        .collect()
}

#[test]
fn ingest_supports_exact_observations() {
    let law = free_fall(G0);
    let v = law.ingest(&obs(G0, &[1.0, 2.0, 3.5, 7.0]), &policy());
    match v {
        ResearchVerdict::Supports { rms } => assert!(rms <= 1e-12, "{rms}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn ingest_parameter_update_recovers_g() {
    let law = free_fall(9.70);
    let pol = IngestPolicy {
        abs_tolerance: 1e-6,
        break_factor: 4.0,
    };
    let data = obs(9.81, &[0.5, 1.0, 2.0, 3.0, 4.0, 5.0]);
    match law.ingest(&data, &pol) {
        ResearchVerdict::ParameterUpdate {
            previous_rms,
            updated,
        } => {
            // 旧法則の residual は閉じた式 0.5 (9.81 - 9.70) t² の RMS
            let ts = [0.5_f64, 1.0, 2.0, 3.0, 4.0, 5.0];
            let ss: f64 = ts.iter().map(|t| (0.5 * 0.11 * t * t).powi(2)).sum();
            let rms0 = (ss / ts.len() as f64).sqrt();
            assert!((previous_rms - rms0).abs() <= 1e-9 * rms0, "{previous_rms}");
            let g = updated.param("g").unwrap();
            assert!((g - 9.81).abs() <= 1e-6, "g = {g}");
            assert!(updated.residual().rms <= pol.abs_tolerance);
            assert_eq!(updated.residual().n, data.len());
            assert_eq!(updated.evidence().len(), data.len());
            // 更新後の法則は新しい g で評価する
            let s = updated.evaluate(&[("t", 2.5)]).unwrap();
            assert!((s - s_closed(9.81, 2.5)).abs() <= 1e-5);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn ingest_residual_grew_on_alternating_noise() {
    // band = 0.01、δ = 0.02 ∈ (band, 4·band]
    // t = 1..4 に ±δ を交互に足す g の再推定は Σ±t² = −10 方向しか吸収できず、
    // 残る RMS = δ·sqrt((4 − 100/354)/4) ≈ 0.964 δ > band なので更新にならない
    let delta = 0.02;
    let law = free_fall(G0);
    let data: Vec<Observation> = [1.0_f64, 2.0, 3.0, 4.0]
        .iter()
        .enumerate()
        .map(|(i, &t)| {
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            Observation::new(&[("t", t)], s_closed(G0, t) + sign * delta)
        })
        .collect();
    match law.ingest(&data, &policy()) {
        ResearchVerdict::ResidualGrew { rms } => assert!((rms - delta).abs() <= 1e-12, "{rms}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn ingest_breaks_on_cubic_evidence() {
    let law = free_fall(G0);
    let data: Vec<Observation> = [1.0_f64, 2.0, 3.0, 4.0, 5.0]
        .iter()
        .map(|&t| Observation::new(&[("t", t)], G0 * t * t * t))
        .collect();
    assert!(matches!(
        law.ingest(&data, &policy()),
        ResearchVerdict::Breaks { .. }
    ));
}

#[test]
fn ingest_out_of_range_and_no_evidence() {
    let law = free_fall(G0);
    let mut data = obs(G0, &[1.0, 2.0]);
    data.push(Observation::new(&[("t", 20.0)], s_closed(G0, 20.0)));
    data.push(Observation::new(&[("t", 3.0)], f64::NAN));
    assert_eq!(
        law.ingest(&data, &policy()),
        ResearchVerdict::OutOfRange { outside: 2 }
    );
    assert_eq!(law.ingest(&[], &policy()), ResearchVerdict::NoEvidence);
}

#[test]
fn ingest_band_uses_the_stored_residual() {
    // 証拠に ±0.05 の散らばりを持つ法則: band = max(0.01, 0.05) = 0.05
    let law = free_fall(G0)
        .with_evidence(&[
            Observation::new(&[("t", 1.0)], s_closed(G0, 1.0) + 0.05),
            Observation::new(&[("t", 2.0)], s_closed(G0, 2.0) - 0.05),
        ])
        .unwrap();
    assert!((law.residual().rms - 0.05).abs() <= 1e-12);
    assert_eq!(law.residual().n, 2);
    let data = vec![Observation::new(&[("t", 3.0)], s_closed(G0, 3.0) + 0.04)];
    assert!(matches!(
        law.ingest(&data, &policy()),
        ResearchVerdict::Supports { .. }
    ));
}

#[test]
fn law_without_parameters_cannot_be_updated() {
    // y = 2 t (parameter なし) に y = 3 t の証拠 → 更新できず、破綻
    let law = ResearchLaw::new(
        "fixed",
        "2*t",
        Var::new("y", "1"),
        &[Var::new("t", "1")],
        &[],
        &[("t", range(0.0, 10.0))],
        prov(),
    )
    .unwrap();
    let data: Vec<Observation> = [1.0_f64, 2.0, 3.0]
        .iter()
        .map(|&t| Observation::new(&[("t", t)], 3.0 * t))
        .collect();
    assert!(matches!(
        law.ingest(&data, &policy()),
        ResearchVerdict::Breaks { .. }
    ));
}

// ── 記述 ─────────────────────────────────────────────────────────

#[test]
fn describe_lists_expression_units_ranges_and_provenance() {
    let text = ideal_gas_si().describe();
    for needle in [
        "ideal gas (SI)",
        "P [Pa] = n*R*T/V",
        "n [mol]",
        "V [m^3]",
        "R = 8.314462618",
        "J/(mol*K)",
        "closed form",
    ] {
        assert!(text.contains(needle), "missing {needle:?} in\n{text}");
    }
}

// ── 退化入力 / panic ────────────────────────────────────────────

fn no_panic<T>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> T {
    std::panic::catch_unwind(f).expect("must not panic")
}

#[test]
fn parse_errors_are_reported_not_panicked() {
    assert!(matches!(
        no_panic(|| LawExpr::parse("")),
        Err(ResearchLawError::EmptyExpression)
    ));
    assert!(matches!(
        no_panic(|| LawExpr::parse("   ")),
        Err(ResearchLawError::EmptyExpression)
    ));
    assert!(matches!(
        no_panic(|| LawExpr::parse("t +")),
        Err(ResearchLawError::UnexpectedEnd)
    ));
    assert!(matches!(
        no_panic(|| LawExpr::parse("(t * 2")),
        Err(ResearchLawError::UnbalancedParenthesis { .. })
    ));
    assert!(matches!(
        no_panic(|| LawExpr::parse("t * 2)")),
        Err(ResearchLawError::UnbalancedParenthesis { .. })
    ));
    assert!(matches!(
        no_panic(|| LawExpr::parse("t ^ x")),
        Err(ResearchLawError::NonConstantExponent)
    ));
    assert!(matches!(
        no_panic(|| LawExpr::parse("foo(t)")),
        Err(ResearchLawError::UnknownFunction(f)) if f == "foo"
    ));
    assert!(no_panic(|| LawExpr::parse("t $ 2")).is_err());
    let deep = format!("{}t{}", "(".repeat(5000), ")".repeat(5000));
    assert!(matches!(
        no_panic(|| LawExpr::parse(&deep)),
        Err(ResearchLawError::NestingTooDeep)
    ));
    let long = vec!["t"; 5000].join("+");
    assert!(matches!(
        no_panic(|| LawExpr::parse(&long)),
        Err(ResearchLawError::NestingTooDeep)
    ));
    // 正しい式: 単項マイナスと関数と右結合の ^
    let e = LawExpr::parse("-2^2 + sqrt(9) * cos(0) + ln(exp(1)) + 2^3^2").unwrap();
    assert_eq!(e.evaluate_constant().unwrap(), -4.0 + 3.0 + 1.0 + 512.0);
}

fn dimensionless_law(expr: &str) -> ResearchLaw {
    ResearchLaw::new(
        "d",
        expr,
        Var::new("y", "1"),
        &[Var::new("t", "1")],
        &[Param::new("a", 1.0, 0.0, "1")],
        &[],
        prov(),
    )
    .unwrap()
}

#[test]
fn division_by_zero_is_an_error_not_nan() {
    let law = dimensionless_law("a/(t - 1)");
    let r = no_panic(|| law.evaluate(&[("t", 1.0)]));
    assert!(matches!(r, Err(ResearchLawError::NonFinite)), "{r:?}");
    let law = dimensionless_law("ln(t)");
    let r = no_panic(|| law.evaluate(&[("t", -1.0)]));
    assert!(matches!(r, Err(ResearchLawError::NonFinite)), "{r:?}");
    // 中間値が inf でも最終値が有限になる式も error にする
    let law = dimensionless_law("1/(a/(t - 1))");
    let r = no_panic(|| law.evaluate(&[("t", 1.0)]));
    assert!(matches!(r, Err(ResearchLawError::NonFinite)), "{r:?}");
}

#[test]
fn nan_and_infinite_conditions_are_out_of_range() {
    let law = dimensionless_law("a*t");
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let r = no_panic(|| law.evaluate(&[("t", bad)]));
        assert!(
            matches!(&r, Err(ResearchLawError::OutOfRange { variable }) if variable == "t"),
            "{r:?}"
        );
    }
}

#[test]
fn huge_exponent_overflow_is_an_error() {
    let law = dimensionless_law("a*t^400");
    let r = no_panic(|| law.evaluate(&[("t", 10.0)]));
    assert!(matches!(r, Err(ResearchLawError::NonFinite)), "{r:?}");
    let law = dimensionless_law("a*t^1e300");
    let r = no_panic(|| law.evaluate(&[("t", 10.0)]));
    assert!(matches!(r, Err(ResearchLawError::NonFinite)), "{r:?}");
    // 次元の指数が表現範囲を超える
    let r = no_panic(|| {
        ResearchLaw::new(
            "big dim",
            "t^200",
            Var::new("y", "1"),
            &[Var::new("t", "s")],
            &[],
            &[],
            prov(),
        )
    });
    assert!(
        matches!(r, Err(ResearchLawError::DimensionOverflow)),
        "{r:?}"
    );
}

#[test]
fn invalid_declarations_are_rejected() {
    // 入力と parameter の名前の重複
    let r = ResearchLaw::new(
        "dup",
        "t",
        Var::new("y", "s"),
        &[Var::new("t", "s")],
        &[Param::new("t", 1.0, 0.0, "s")],
        &[],
        prov(),
    );
    assert!(
        matches!(r, Err(ResearchLawError::DuplicateName(_))),
        "{r:?}"
    );
    // 成立範囲が入力でない名前を指す
    let r = ResearchLaw::new(
        "range",
        "t",
        Var::new("y", "s"),
        &[Var::new("t", "s")],
        &[],
        &[("u", range(0.0, 1.0))],
        prov(),
    );
    assert!(
        matches!(r, Err(ResearchLawError::UnknownVariable(_))),
        "{r:?}"
    );
    // lo > hi / NaN の範囲
    for bad in [range(2.0, 1.0), range(f64::NAN, 1.0)] {
        let r = ResearchLaw::new(
            "range",
            "t",
            Var::new("y", "s"),
            &[Var::new("t", "s")],
            &[],
            &[("t", bad)],
            prov(),
        );
        assert!(matches!(r, Err(ResearchLawError::InvalidRange(_))), "{r:?}");
    }
    // parameter の値が NaN
    let r = ResearchLaw::new(
        "nan param",
        "a*t",
        Var::new("y", "1"),
        &[Var::new("t", "1")],
        &[Param::new("a", f64::NAN, 0.0, "1")],
        &[],
        prov(),
    );
    assert!(matches!(r, Err(ResearchLawError::NonFinite)), "{r:?}");
}
