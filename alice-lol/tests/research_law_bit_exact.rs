//! `research_law` evaluates its functions through `alice-det-math`, so a law
//! re-computed on another machine gives the same bits
//!
//! Each case compares the law's result with the `alice-det-math` function
//! called directly (`to_bits` equality), over enough inputs that a platform
//! libm with a different last-ulp behaviour would show up. Integer powers are
//! compared with repeated squaring written here, the order the law promises.

use alice_lol::research_law::{Param, ResearchLaw, Var};
use alice_zip::law::{Provenance, ValidRange};

fn law(expr: &str, lo: f64, hi: f64) -> ResearchLaw {
    ResearchLaw::new(
        expr,
        expr,
        Var::new("y", "1"),
        &[Var::new("x", "1")],
        &[] as &[Param],
        &[("x", ValidRange { lo, hi })],
        Provenance::new("bit-exact check", "direct"),
    )
    .unwrap()
}

/// Deterministic inputs spread over `[lo, hi]` (no RNG: the same list everywhere)
#[allow(clippy::suboptimal_flops)] // the inputs only need to be the same everywhere
fn inputs(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let t = (i as f64 + 0.37) / n as f64;
            lo + (hi - lo) * t
        })
        .collect()
}

fn assert_same_bits(expr: &str, lo: f64, hi: f64, reference: fn(f64) -> f64) {
    let l = law(expr, lo, hi);
    let mut compared = 0;
    for x in inputs(lo, hi, 20_000) {
        let got = l.evaluate(&[("x", x)]).unwrap();
        let want = reference(x);
        assert_eq!(
            got.to_bits(),
            want.to_bits(),
            "{expr} at x = {x:e}: {got:e} vs {want:e}"
        );
        compared += 1;
    }
    assert_eq!(compared, 20_000);
}

/// x^n by repeated squaring, from the least significant bit of n
fn pow_by_squaring(x: f64, n: u32) -> f64 {
    let (mut base, mut e, mut acc) = (x, n, 1.0_f64);
    while e > 0 {
        if e & 1 == 1 {
            acc *= base;
        }
        base *= base;
        e >>= 1;
    }
    acc
}

#[test]
fn sin_is_alice_det_math_sin64() {
    assert_same_bits("sin(x)", -1.0e6, 1.0e6, alice_det_math::sin64);
}

#[test]
fn cos_is_alice_det_math_cos64() {
    assert_same_bits("cos(x)", -1.0e6, 1.0e6, alice_det_math::cos64);
}

#[test]
fn exp_is_alice_det_math_exp64() {
    assert_same_bits("exp(x)", -700.0, 700.0, alice_det_math::exp64);
}

#[test]
fn ln_is_alice_det_math_ln64() {
    assert_same_bits("ln(x)", 1.0e-300, 1.0e300, alice_det_math::ln64);
}

#[test]
fn sqrt_is_alice_det_math_sqrt64() {
    assert_same_bits("sqrt(x)", 0.0, 1.0e300, alice_det_math::sqrt64);
}

#[test]
fn a_non_integer_power_is_alice_det_math_powf64() {
    assert_same_bits("x^2.5", 1.0e-100, 1.0e100, |x| {
        alice_det_math::powf64(x, 2.5)
    });
}

#[test]
fn an_integer_power_is_repeated_squaring() {
    assert_same_bits("x^7", -1.0e40, 1.0e40, |x| pow_by_squaring(x, 7));
    assert_same_bits("x^2", -1.0e150, 1.0e150, |x| pow_by_squaring(x, 2));
}

#[test]
fn a_negative_integer_power_is_the_reciprocal_of_repeated_squaring() {
    assert_same_bits("x^(-3)", 1.0e-50, 1.0e50, |x| 1.0 / pow_by_squaring(x, 3));
}
