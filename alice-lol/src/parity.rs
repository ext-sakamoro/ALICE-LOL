//! When two evaluations of a field agree
//!
//! Evaluation is IEEE-total: an overflow is ±∞ and an invalid operation is NaN, and both are
//! results, not errors. Two results agree when they are the same value (bit for bit: two
//! infinities of the same sign agree), when both are NaN, or when both are finite and within
//! a relative tolerance. A difference of two infinities is NaN, so a plain `|a - b| <= tol`
//! says two equal infinities disagree (a fuzz input `capped_torus(5.1e23, 1, 748)` evaluates
//! to +∞ on both sides of a round trip, and that comparison failed).
//!
//! This is the evaluation of an SDF field (`eval`). A quantitative law (`research_law`) is
//! stricter: a non-finite intermediate value is an error there (`conformance/TASK.md`). SDF
//! evaluation has no reference evaluator of its own in `conformance/`, which covers law files.

/// `a` and `b` agree: the same bits, both NaN, or both finite and
/// `|a - b| <= rel_tol * max(|a|, |b|, 1)` (symmetric: the order of `a` and `b` does not matter)
///
/// An infinity agrees only with the same infinity; NaN agrees only with NaN; `-0.0` and
/// `0.0` agree (their difference is 0).
#[must_use]
pub fn agrees(a: f32, b: f32, rel_tol: f32) -> bool {
    a.to_bits() == b.to_bits()
        || (a.is_nan() && b.is_nan())
        // the tolerance is only for two finite values: with an infinite `a` it is itself ∞,
        // and `inf` would agree with `-inf` and with every finite number
        || (a.is_finite() && b.is_finite() && (a - b).abs() <= rel_tol * a.abs().max(b.abs()).max(1.0))
}

#[cfg(test)]
mod tests {
    use super::agrees;

    #[test]
    fn the_table() {
        let (inf, nan) = (f32::INFINITY, f32::NAN);
        for (a, b, want) in [
            (inf, inf, true),
            (-inf, -inf, true),
            (inf, -inf, false),
            (-inf, inf, false),
            (inf, f32::MAX, false),
            (f32::MAX, inf, false),
            (nan, nan, true),
            (nan, -nan, true),
            (nan, 1.0, false),
            (1.0, nan, false),
            (nan, inf, false),
            (-0.0, 0.0, true),
            (0.0, -0.0, true),
            (1.0, 1.0 + 1e-5, true),
            (1.0, 1.001, false),
            (1e6, 1e6 + 50.0, true),
            (1e6, 1e6 + 200.0, false),
            // within the tolerance either way round
            (1.0, 1.000_05, true),
            // the scale is the larger of the two: the difference (0.013 610 84) is within
            // 1e-4 of the larger value but not of the smaller one (exact f32 values), so
            // scaling by the first argument would make the answer depend on the order
            (
                f32::from_bits(0x4308_1874),
                f32::from_bits(0x4308_1bf0),
                true,
            ), // 136.095 52 and 136.109 13
            (0.5, 2.0, false),
        ] {
            assert_eq!(agrees(a, b, 1e-4), want, "{a} vs {b}");
            assert_eq!(
                agrees(b, a, 1e-4),
                want,
                "{b} vs {a} (the relation is symmetric)"
            );
        }
    }
}
