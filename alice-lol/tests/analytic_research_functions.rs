//! `research_law` の 2 引数以上の関数 (`atan2` / `min` / `max`) の解析解突合テスト
//!
//! 期待値はこの file に書いた閉じた式 (π の分数、最小値・最大値の定義) から決める
//! (実装の出力を pin する golden ではない):
//!
//! - `atan2(y, x)` は点 `(x, y)` の角度で値域は (−π, π] 4 象限と軸上の点を見る
//! - `min` / `max` は 2 個以上の引数を取り、すべて同じ次元 (結果も同じ次元)
//! - `atan2` の 2 引数は同じ次元で、結果は無次元
//! - 引数の数が合わない呼び出しは読めない

#![allow(clippy::unwrap_used)]
#![allow(
    clippy::float_cmp,
    reason = "min / max return one of their arguments unchanged, so the comparison is exact"
)]

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use alice_lol::research_law::{LawExpr, Param, ResearchLaw, ResearchLawError, Var};
use alice_zip::law::{Provenance, ValidRange};

fn constant(text: &str) -> f64 {
    LawExpr::parse(text).unwrap().evaluate_constant().unwrap()
}

/// det-math の atan2 は最終 bit まで libm と一致するとは限らないので、
/// π の分数との差を 2 ulp 以内で見る
fn close(got: f64, want: f64) {
    let tol = 2.0 * f64::EPSILON * want.abs().max(1.0);
    assert!((got - want).abs() <= tol, "got {got:e}, want {want:e}");
}

fn prov() -> Provenance {
    Provenance::new("closed form", "stated")
}

#[test]
fn atan2_gives_the_angle_of_the_point_in_every_quadrant() {
    close(constant("atan2(1, 1)"), FRAC_PI_4);
    close(constant("atan2(1, -1)"), 3.0 * FRAC_PI_4);
    close(constant("atan2(-1, -1)"), -3.0 * FRAC_PI_4);
    close(constant("atan2(-1, 1)"), -FRAC_PI_4);
    close(constant("atan2(1, 0)"), FRAC_PI_2);
    close(constant("atan2(-1, 0)"), -FRAC_PI_2);
    close(constant("atan2(0, 1)"), 0.0);
    // the negative x axis is +π (the range is (−π, π])
    close(constant("atan2(0, -1)"), PI);
    // only the ratio matters
    close(constant("atan2(2*3, 2*3)"), FRAC_PI_4);
}

#[test]
fn min_and_max_take_two_or_more_arguments() {
    assert_eq!(constant("min(3, 1, 2)"), 1.0);
    assert_eq!(constant("max(3, 1, 2)"), 3.0);
    assert_eq!(constant("min(-5, 2)"), -5.0);
    assert_eq!(constant("max(-5, -2)"), -2.0);
    assert_eq!(constant("min(4, 4)"), 4.0);
    // arguments are expressions
    assert_eq!(constant("max(1 + 1, 3 - 2, 2^2)"), 4.0);
    assert_eq!(constant("2*max(1, 3) - min(1, 3)"), 5.0);
}

#[test]
fn a_call_with_the_wrong_number_of_arguments_is_rejected() {
    for text in [
        "atan2(1)",
        "atan2(1, 2, 3)",
        "min(2)",
        "max(2)",
        "sqrt(1, 2)",
        "exp(1, 2)",
    ] {
        let r = LawExpr::parse(text);
        assert!(
            matches!(r, Err(ResearchLawError::ArgumentCount { .. })),
            "{text}: {r:?}"
        );
    }
    for text in ["min()", "atan2(1,)", "max(, 1)"] {
        assert!(LawExpr::parse(text).is_err(), "{text}");
    }
}

fn law(expr: &str, output: &str, inputs: &[(&str, &str)]) -> Result<ResearchLaw, ResearchLawError> {
    let vars: Vec<Var> = inputs.iter().map(|(n, u)| Var::new(n, u)).collect();
    let ranges: Vec<(&str, ValidRange)> = inputs
        .iter()
        .map(|(n, _)| {
            (
                *n,
                ValidRange {
                    lo: -10.0,
                    hi: 10.0,
                },
            )
        })
        .collect();
    let no_params: [Param; 0] = [];
    ResearchLaw::new(
        "f",
        expr,
        Var::new("y", output),
        &vars,
        &no_params,
        &ranges,
        prov(),
    )
}

#[test]
fn atan2_needs_two_quantities_of_one_dimension_and_is_dimensionless() {
    let l = law("atan2(b, a)", "1", &[("a", "m"), ("b", "m")]).unwrap();
    close(
        l.evaluate(&[("a", -2.0), ("b", 2.0)]).unwrap(),
        3.0 * FRAC_PI_4,
    );
    let r = law("atan2(b, a)", "1", &[("a", "m"), ("b", "s")]);
    assert!(
        matches!(r, Err(ResearchLawError::IncompatibleDimensions { .. })),
        "{r:?}"
    );
    let r = law("atan2(b, a)", "m", &[("a", "m"), ("b", "m")]);
    assert!(
        matches!(r, Err(ResearchLawError::OutputDimension { .. })),
        "{r:?}"
    );
}

#[test]
fn min_and_max_keep_the_dimension_of_their_arguments() {
    let l = law("min(a, b) + max(a, b)", "m", &[("a", "m"), ("b", "m")]).unwrap();
    assert_eq!(l.evaluate(&[("a", 3.0), ("b", -1.0)]).unwrap(), 2.0);
    let r = law("min(a, b)", "m", &[("a", "m"), ("b", "s")]);
    assert!(
        matches!(r, Err(ResearchLawError::IncompatibleDimensions { .. })),
        "{r:?}"
    );
    let r = law("max(a, b)", "s", &[("a", "m"), ("b", "m")]);
    assert!(
        matches!(r, Err(ResearchLawError::OutputDimension { .. })),
        "{r:?}"
    );
}
