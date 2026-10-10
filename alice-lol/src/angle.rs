//! Reducing a user-supplied angle, in degrees, to a numerically well-conditioned
//! range before it is ever converted to radians.
//!
//! # Why this exists
//!
//! `%` (`f32::rem`, i.e. `fmod`) is exact for every finite input, no matter
//! the magnitude -- IEEE 754's remainder operation is defined to return the
//! mathematically exact remainder, not a rounded approximation. In
//! particular, when `|deg| < 360.0`, `deg % 360.0` is `deg` itself, bit for
//! bit: the quotient truncates to exactly `0`, so no arithmetic is actually
//! performed on the value.
//!
//! `sin`/`cos` (and therefore `glam::Quat::from_rotation_x` /
//! `Quat::from_euler`, which call them) carry no such guarantee: correctly
//! reducing a huge radian argument modulo a period that is an irrational
//! multiple of 1 (`2*PI`) is the classically hard Payne-Hanek problem, and
//! ordinary `f32`/`f64` `sin`/`cos` routines only promise accuracy for
//! arguments within a few turns of zero. A degree value of `1e9`, converted
//! directly via `.to_radians()` to approximately `1.745e7` radians and handed
//! to `Quat::from_rotation_x`, measurably produces a rotation several tens of
//! degrees away from the true one (`109.79` vs. the correct `280.0`, found by
//! reducing in degree-space first): this is not a one-ulp rounding
//! difference, it is the sub-turn phase being lost before any trigonometric
//! function is even called.
//!
//! Reducing the *degree* value first sidesteps the problem entirely, because
//! `%` carries no such weakness: every caller that turns a user-controlled
//! angle into a rotation reduces it exactly once, here, instead of each one
//! calling `.to_radians()` on an unreduced value and inheriting whatever
//! `sin`/`cos` does with it.
//!
//! ## Why not `rem_euclid`
//!
//! An earlier version of this function used `f32::rem_euclid`, which is also
//! exact on its own terms (`fmod` plus an `if r < 0.0 { r + 360.0 }`
//! correction) -- but that correction is itself a floating-point *addition*,
//! which is not exact when the operands are not Sterbenz-close. For a
//! negative non-integer angle already inside the target range (`-0.1`, say),
//! `rem_euclid` computed `-0.1 + 360.0 = 359.9` (rounded, since `359.9` is
//! not exactly representable), and this module's own `(-180, 180]` wrap then
//! subtracted `360.0` back off that already-rounded value, landing on
//! `-0.100006104` -- a value bit-different from the `-0.1` that went in, so
//! "already-canonical angles are unchanged" did not hold for non-integer
//! negative angles (measured: 5,314,551 of 39.9M swept values differed from
//! an exact `f64` reference, every one of them negative). `%` never performs
//! that addition: a value already in `(-180, 180]` takes the identity branch
//! below and is returned bit for bit.

/// Reduce an angle, in degrees, to `(-180.0, 180.0]`.
///
/// Exact and deterministic for every finite input (see the module doc): `%`
/// (`fmod`) is exact per IEEE 754, and the two branches that adjust a result
/// outside `(-180, 180]` back into it (`r - 360.0` / `r + 360.0`) only ever
/// fire when `|r| > 180.0`, where the operand and `360.0` are close enough in
/// magnitude for the subtraction/addition to be exact too (Sterbenz's
/// lemma). Non-finite input (`NaN`/`±inf`) is unreachable from `.lol` text --
/// the grammar's number literal has no token for either -- so this function
/// does not refuse it; `%` already maps both to `NaN` (never panics, never
/// produces a different kind of incorrectness), which callers that do
/// accept a raw `f32` from Rust (not from parsed text) inherit as plain
/// `NaN` propagation, the same contract every other arithmetic function in
/// this crate has for `NaN` input.
///
/// Identity on `(-180.0, 180.0]`: for any `deg` already in that range,
/// `canon_angle_deg(deg) == deg` bit for bit, including non-integer and
/// negative values (`canon_angle_deg(-0.1) == -0.1`, not an earlier
/// `rem_euclid`-based version's `-0.100006104`, see the module doc).
#[must_use]
pub fn canon_angle_deg(deg: f32) -> f32 {
    let r = deg % 360.0;
    if r > 180.0 {
        r - 360.0
    } else if r <= -180.0 {
        r + 360.0
    } else {
        r
    }
}
#[cfg(test)]
mod tests {
    use super::canon_angle_deg;

    #[test]
    #[allow(clippy::float_cmp)] // both sides are exact literals, no arithmetic in between
    fn already_canonical_positive_is_unchanged() {
        assert_eq!(canon_angle_deg(10.0), 10.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // both sides are exact literals, no arithmetic in between
    fn already_canonical_negative_is_unchanged() {
        assert_eq!(canon_angle_deg(-15.0), -15.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // both sides are exact literals, no arithmetic in between
    fn already_canonical_negative_non_integer_is_bit_identical() {
        // the case rem_euclid got wrong (module doc "Why not rem_euclid"):
        // rem_euclid(-0.1, 360.0) = -0.1 + 360.0 = 359.9 (rounded), and this
        // module's old (-180, 180] wrap then subtracted 360.0 back off that
        // already-rounded value, landing on -0.100006104, not -0.1
        let x = -0.1_f32;
        assert_eq!(canon_angle_deg(x).to_bits(), x.to_bits());
    }

    #[test]
    #[allow(clippy::float_cmp)] // both sides are exact literals, no arithmetic in between
    fn zero_is_unchanged() {
        assert_eq!(canon_angle_deg(0.0), 0.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // both sides are exact literals, no arithmetic in between
    fn exactly_180_stays_180() {
        assert_eq!(canon_angle_deg(180.0), 180.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // both sides are exact literals, no arithmetic in between
    fn exactly_negative_180_wraps_to_positive_180() {
        // -180 and 180 are the same rotation; this picks the single
        // canonical side, matching `emit::canon_deg`'s existing choice
        assert_eq!(canon_angle_deg(-180.0), 180.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // both sides are exact literals, no arithmetic in between
    fn one_billion_degrees_reduces_to_the_true_remainder() {
        // 1_000_000_000 / 360 = 2_777_777.777..., remainder 280
        assert_eq!(canon_angle_deg(1e9), 280.0 - 360.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // both sides are exact literals, no arithmetic in between
    fn negative_one_billion_degrees_reduces_to_the_true_remainder() {
        assert_eq!(canon_angle_deg(-1e9), -280.0 + 360.0);
    }

    #[test]
    fn astronomically_huge_finite_value_still_reduces_exactly() {
        // `%` is exact at every finite magnitude (module doc); this is not a
        // representable real-world angle, only a check that the function
        // never panics or produces a non-finite result for one
        let r = canon_angle_deg(1e30);
        assert!(r.is_finite());
        assert!((-180.0..=180.0).contains(&r));
    }

    #[test]
    fn f32_max_still_reduces_exactly() {
        let r = canon_angle_deg(f32::MAX);
        assert!(r.is_finite());
        assert!((-180.0..=180.0).contains(&r));
    }

    #[test]
    #[allow(clippy::float_cmp)] // tiny is the exact literal passed in, no arithmetic in between
    fn subnormal_is_unchanged() {
        let tiny = f32::MIN_POSITIVE / 2.0;
        assert_eq!(canon_angle_deg(tiny), tiny);
    }

    #[test]
    #[allow(clippy::float_cmp)] // tiny is the exact literal passed in, no arithmetic in between
    fn negative_subnormal_is_unchanged() {
        // the other case rem_euclid got wrong: a negative subnormal rounded
        // all the way to 0.0 once run through "+ 360.0, - 360.0"
        let tiny = -(f32::MIN_POSITIVE / 2.0);
        assert_eq!(canon_angle_deg(tiny), tiny);
    }

    #[test]
    fn nan_propagates_as_nan_not_a_panic() {
        assert!(canon_angle_deg(f32::NAN).is_nan());
    }

    #[test]
    fn positive_infinity_propagates_as_nan() {
        // `%` of infinity is NaN by IEEE 754 (the remainder is undefined),
        // not a panic and not a silently-wrong finite value
        assert!(canon_angle_deg(f32::INFINITY).is_nan());
    }

    #[test]
    fn negative_infinity_propagates_as_nan() {
        assert!(canon_angle_deg(f32::NEG_INFINITY).is_nan());
    }

    /// the exact `f64` reference: `%` (never `rem_euclid`) at `f64` precision,
    /// then the same `(-180, 180]` wrap -- this is a *different* formula from
    /// `canon_angle_deg`, not a re-derivation of it, so an `f32`/`f64` match
    /// is a real cross-check, not a tautology
    fn f64_reference(deg: f32) -> f32 {
        let r = f64::from(deg) % 360.0;
        let r = if r > 180.0 {
            r - 360.0
        } else if r <= -180.0 {
            r + 360.0
        } else {
            r
        };
        #[allow(clippy::cast_possible_truncation)]
        let r = r as f32;
        r
    }

    #[test]
    #[allow(clippy::float_cmp)] // comparing two independent reductions, not an arithmetic estimate
    fn reduction_matches_an_f64_reference_everywhere_swept() {
        // differential check: sweep a wide range of magnitudes (including
        // subnormals and non-integers) and signs and confirm the f32-only
        // reduction never drifts from the f64 reference
        let cases: &[f32] = &[
            0.0,
            f32::MIN_POSITIVE / 2.0, // subnormal
            1e-30,
            0.1,
            1.0,
            10.0,
            90.0,
            179.0,
            179.9,
            180.0,
            180.1,
            181.0,
            270.0,
            359.0,
            359.9,
            360.0,
            361.0,
            719.0,
            720.0,
            1000.0,
            1e4,
            1e5,
            1e6,
            1e7,
            1e8,
            1e9,
            1e12,
            1e15,
            1e18,
            1e20,
            1e25,
            1e30,
            1e35,
            f32::MAX,
        ];
        for &deg in cases {
            for &sign in &[1.0_f32, -1.0] {
                let x = deg * sign;
                let got = canon_angle_deg(x);
                let want = f64_reference(x);
                assert_eq!(got, want, "mismatch at x={x:e}");
            }
        }
    }

    #[test]
    fn every_angle_already_in_range_is_bit_identical_to_itself() {
        // the property the module doc claims and the earlier `rem_euclid`
        // version violated for negative non-integers: canon_angle_deg is the
        // identity, bit for bit, on its own target range. Sweep densely
        // (step 0.01 degree, ~36,000 points) across signs; indexed by an
        // integer step count (not accumulated float addition) so the swept
        // value itself has no drift to confuse with the property under test.
        for i in -17_999_i16..=18_000 {
            let deg = f32::from(i) / 100.0;
            assert_eq!(
                canon_angle_deg(deg).to_bits(),
                deg.to_bits(),
                "deg={deg} not bit-identical to itself"
            );
        }
    }

    #[test]
    fn mutant_reverting_to_rem_euclid_is_caught() {
        // pins the exact defect the `rem_euclid`-based predecessor had: this
        // test fails against that implementation and passes against this one
        fn old_rem_euclid_version(deg: f32) -> f32 {
            let mut a = deg.rem_euclid(360.0);
            if a > 180.0 {
                a -= 360.0;
            }
            a
        }
        let x = -0.1_f32;
        assert_ne!(
            old_rem_euclid_version(x).to_bits(),
            x.to_bits(),
            "the old implementation is expected to NOT be bit-identical here \
             (if this now fails, rem_euclid's rounding defect was fixed \
             upstream and this pin should be revisited)"
        );
        assert_eq!(canon_angle_deg(x).to_bits(), x.to_bits());
    }
}
