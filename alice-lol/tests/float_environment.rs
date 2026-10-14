//! The floating-point environment the Law-bearing arithmetic relies on, asserted at run
//! time (ALICE-Law G14): IEEE 754 round-to-nearest, ties-to-even, and subnormal numbers
//! kept as they are, neither flushed to zero on output (FTZ) nor read as zero on input
//! (DAZ).
//!
//! Several modules state these assumptions in their docs (`angle.rs`, `geometry_id.rs`);
//! this test makes them checked facts on every target the test suite runs on. Each
//! operand goes through `black_box`, so the operation happens at run time on the target's
//! own floating-point unit rather than in the compiler's constant folding (which always
//! uses round-to-nearest-even and would make the test vacuous).
//!
//! oracle: the IEEE 754-2019 rounding rule and the binary32 / binary64 encodings, with
//! every expected value written as an exact bit pattern or an exact power of two.

use std::hint::black_box;

#[test]
fn f32_addition_rounds_a_tie_to_the_even_neighbour() {
    let half_ulp = black_box(f32::EPSILON / 2.0); // 2^-24, exactly half an ulp at 1.0
                                                  // 1 + 2^-24 lies exactly between 1 (even mantissa) and 1 + 2^-23 (odd): rounds down
    assert_eq!((black_box(1.0f32) + half_ulp).to_bits(), 1.0f32.to_bits());
    // (1 + 2^-23) + 2^-24 lies between 1 + 2^-23 (odd) and 1 + 2^-22 (even): rounds up
    let odd = black_box(f32::from_bits(1.0f32.to_bits() + 1));
    assert_eq!((odd + half_ulp).to_bits(), 1.0f32.to_bits() + 2);
}

#[test]
fn f64_addition_rounds_a_tie_to_the_even_neighbour() {
    let half_ulp = black_box(f64::EPSILON / 2.0); // 2^-53
    assert_eq!((black_box(1.0f64) + half_ulp).to_bits(), 1.0f64.to_bits());
    let odd = black_box(f64::from_bits(1.0f64.to_bits() + 1));
    assert_eq!((odd + half_ulp).to_bits(), 1.0f64.to_bits() + 2);
}

#[test]
fn a_value_just_off_the_tie_rounds_to_the_nearer_neighbour() {
    // not a tie: 1 + 2^-24 + 2^-30 is nearer 1 + 2^-23, so it rounds up even though the
    // lower neighbour is even (round-toward-zero or a truncating unit would round down)
    let x = black_box(f32::EPSILON / 2.0 + f32::EPSILON / 128.0);
    assert_eq!((black_box(1.0f32) + x).to_bits(), 1.0f32.to_bits() + 1);
    // and the mirror image below 1: 1 - (2^-25 + 2^-31) is nearer 1 - 2^-24
    let y = black_box(f32::EPSILON / 4.0 + f32::EPSILON / 256.0);
    assert_eq!((black_box(1.0f32) - y).to_bits(), 1.0f32.to_bits() - 1);
}

#[test]
#[allow(
    clippy::cast_possible_truncation,
    reason = "the f64 to f32 narrowing is the rounding under test"
)]
fn narrowing_f64_to_f32_rounds_a_tie_to_even() {
    // 1 + 2^-24 is exact in f64 and a tie in f32
    let tie_down = black_box(1.0f64 + 2f64.powi(-24));
    assert_eq!((tie_down as f32).to_bits(), 1.0f32.to_bits());
    let tie_up = black_box(1.0f64 + 2f64.powi(-23) + 2f64.powi(-24));
    assert_eq!((tie_up as f32).to_bits(), 1.0f32.to_bits() + 2);
}

#[test]
fn f32_subnormal_results_are_not_flushed_to_zero() {
    let min_normal = black_box(f32::MIN_POSITIVE); // 2^-126
    let half = min_normal / black_box(2.0f32); // 2^-127, subnormal
    assert!(half.is_subnormal(), "2^-127 flushed: {half:e}");
    assert_eq!(half.to_bits(), 0x0040_0000);
    // the smallest subnormal, 2^-149, survives an exact halving of 2^-148
    let smallest = f32::from_bits(black_box(2u32)) / black_box(2.0f32);
    assert_eq!(smallest.to_bits(), 1);
    // a difference of two normal numbers that is subnormal (gradual underflow)
    let a = black_box(f32::from_bits(0x0080_0001)); // 2^-126 * (1 + 2^-23)
    assert_eq!((a - min_normal).to_bits(), 1, "gradual underflow lost");
}

#[test]
fn f32_subnormal_inputs_are_not_read_as_zero() {
    let sub = black_box(f32::from_bits(1)); // 2^-149
                                            // DAZ would turn this into 0 * 2 = 0
    assert_eq!((sub * black_box(2.0f32)).to_bits(), 2);
    assert_eq!((sub + sub).to_bits(), 2);
    // a subnormal scaled back into the normal range keeps its value exactly:
    // 2^-149 * 2^126 = 2^-23 (biased exponent 104, mantissa 0)
    assert_eq!((sub * black_box(2f32.powi(126))).to_bits(), 104 << 23);
}

#[test]
fn f64_subnormal_results_and_inputs_are_kept() {
    let min_normal = black_box(f64::MIN_POSITIVE); // 2^-1022
    let half = min_normal / black_box(2.0f64);
    assert!(half.is_subnormal(), "2^-1023 flushed: {half:e}");
    assert_eq!(half.to_bits(), 0x0008_0000_0000_0000);
    let sub = black_box(f64::from_bits(1)); // 2^-1074
    assert_eq!((sub * black_box(2.0f64)).to_bits(), 2);
    assert_eq!((sub + sub).to_bits(), 2);
}

#[test]
fn rounding_a_subnormal_tie_is_also_to_even() {
    // 3 * 2^-150 = 1.5 * 2^-149, a tie between 2^-149 (odd mantissa 1) and 2^-148
    // (even mantissa 2): rounds to 2^-148
    let three = f32::from_bits(black_box(3u32)); // 3 * 2^-149
    assert_eq!((three / black_box(2.0f32)).to_bits(), 2);
    // 2^-149 / 2 = 2^-150, a tie between 0 (even) and 2^-149 (odd): rounds to +0
    let one = f32::from_bits(black_box(1u32));
    assert_eq!((one / black_box(2.0f32)).to_bits(), 0);
}
