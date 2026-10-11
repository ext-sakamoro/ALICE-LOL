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

/// `glam::Quat::from_euler(EulerRot::XYZ, x, y, z)`, specialised to intrinsic
/// XYZ.
///
/// Built from [`alice_det_math::sin_cos`] instead of `glam`'s own `sin`/`cos`
/// (the only order this crate's `rotate(x, y, z, child)` ever uses).
///
/// # Why this exists
///
/// `glam::Quat::from_euler` dispatches to a different SIMD backend per target
/// (NEON on `aarch64`, SSE2 on `x86_64`, a scalar fallback elsewhere), and
/// each backend's `sin_cos` is ultimately the platform's own libm -- the same
/// non-determinism this crate already avoids for every other Law-path
/// trigonometric call (`research_law.rs`, `law_id.rs`: see
/// `alice_det_math::SEMANTICS_ID`). The same source text parsed on two
/// machines must build the same quaternion bits; this function is the one
/// place `rotate(x, y, z, child)` goes through to get that. The new
/// `rotate_q` construct's unit-magnitude tolerance
/// ([`crate::limits::checked_rotation_quat`]) was derived from a one-time
/// measurement of this function's own maximum deviation from unit length
/// across a dense angle sweep, not something this function computes itself.
///
/// The formula is Ken Shoemake's (Graphics Gems IV), specialised to the
/// `XYZ` case of `glam`'s own `impl_quat_from_euler!` macro (verified against
/// `glam 0.29.3`'s source: `Order::from_euler(EulerRot::XYZ)` is
/// `(initial_axis: X, parity_even: true, initial_repeated: false,
/// frame_static: true)`, which collapses the general macro to exactly the
/// steps below) -- not a re-derivation from the paper, a transcription of the
/// one case this crate actually uses, with `alice_det_math::sin_cos` in
/// place of `glam`'s `$scalar::math::sin_cos`.
#[must_use]
pub fn quat_from_euler_xyz(x: f32, y: f32, z: f32) -> glam::Quat {
    let ti = x * 0.5;
    let tj = (-y) * 0.5; // XYZ is parity-even: angles.y negated before halving
    let th = z * 0.5;
    let (si, ci) = alice_det_math::sin_cos(ti);
    let (sj, cj) = alice_det_math::sin_cos(tj);
    let (sh, ch) = alice_det_math::sin_cos(th);
    let cc = ci * ch;
    let cs = ci * sh;
    let sc = si * ch;
    let ss = si * sh;
    // XYZ: parity_even = true -> parity = -1.0, (i, j, k) = (x, y, z)
    glam::Quat::from_xyzw(
        cj.mul_add(sc, -(sj * cs)),
        -(cj.mul_add(ss, sj * cc)),
        cj.mul_add(cs, -(sj * sc)),
        cj.mul_add(cc, sj * ss),
    )
}

/// `glam::Quat::from_rotation_x(angle)`, built from
/// [`alice_det_math::sin_cos`] instead of `glam`'s own `sin`/`cos`.
///
/// Same reason as [`quat_from_euler_xyz`]: any `SdfNode::Rotate` this
/// crate's own `stdlib` constructs directly (not through the `rotate(x, y,
/// z, child)`/`rotate_q(...)` text grammar) must build the identical
/// quaternion on every architecture too, or `emit`'s now-faithful text
/// exposes the same platform-libm drift `rotate`'s own fix closed. Formula
/// verified against `glam 0.29.3`'s own `from_rotation_x`
/// (`Quat::from_xyzw(s, 0.0, 0.0, c)` where `(s, c) = sin_cos(angle * 0.5)`).
#[must_use]
pub fn quat_from_rotation_x(angle: f32) -> glam::Quat {
    let (s, c) = alice_det_math::sin_cos(angle * 0.5);
    glam::Quat::from_xyzw(s, 0.0, 0.0, c)
}

/// `glam::Quat::from_rotation_y(angle)`, deterministic; see
/// [`quat_from_rotation_x`].
#[must_use]
pub fn quat_from_rotation_y(angle: f32) -> glam::Quat {
    let (s, c) = alice_det_math::sin_cos(angle * 0.5);
    glam::Quat::from_xyzw(0.0, s, 0.0, c)
}

/// `glam::Quat::from_rotation_z(angle)`, deterministic; see
/// [`quat_from_rotation_x`].
#[must_use]
pub fn quat_from_rotation_z(angle: f32) -> glam::Quat {
    let (s, c) = alice_det_math::sin_cos(angle * 0.5);
    glam::Quat::from_xyzw(0.0, 0.0, s, c)
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

#[cfg(test)]
mod quat_from_euler_xyz_tests {
    use super::{
        quat_from_euler_xyz, quat_from_rotation_x, quat_from_rotation_y, quat_from_rotation_z,
    };
    use glam::{EulerRot, Quat};

    /// This is a different trig backend (`alice_det_math::sin_cos`, not
    /// `glam`'s own libm-backed one) computing the SAME Shoemake XYZ formula,
    /// so an exact bit match is not expected -- only that the two stay within
    /// a few ulp of each other across a representative sweep, confirming the
    /// formula transcription itself (not the trig) is correct. Measured max
    /// over this sweep: `1.7881393e-7` (about 1 ulp at this magnitude); the
    /// bound below is `1e-5`, two orders of magnitude of headroom so this
    /// does not become a flaky pin if `alice_det_math` tightens its own
    /// accuracy later.
    #[test]
    fn matches_glam_from_euler_on_a_sweep() {
        let angles: &[f32] = &[-3.0, -1.5, -0.7, -0.1, 0.0, 0.05, 0.3, 0.9, 1.57, 2.0, 2.9];
        let mut max_diff = 0.0_f32;
        for &x in angles {
            for &y in angles {
                for &z in angles {
                    let want = Quat::from_euler(EulerRot::XYZ, x, y, z);
                    let got = quat_from_euler_xyz(x, y, z);
                    for (a, b) in [
                        (want.x, got.x),
                        (want.y, got.y),
                        (want.z, got.z),
                        (want.w, got.w),
                    ] {
                        max_diff = max_diff.max((a - b).abs());
                    }
                }
            }
        }
        assert!(
            max_diff < 1e-5,
            "quat_from_euler_xyz formula does not match glam's XYZ case: max_diff={max_diff:e}"
        );
    }

    /// `quat_from_euler_xyz` must itself be a pure function of its three
    /// inputs (no hidden platform dependence beyond `alice_det_math`'s own
    /// portability guarantee): the same angles must give bit-identical
    /// output across repeated calls -- a mutant that introduced, say, an
    /// uninitialised-memory read or a thread-local would be caught here, not
    /// by the glam-comparison test above (which only bounds a *difference*,
    /// not exact repeatability).
    #[test]
    fn is_bit_identical_across_repeated_calls() {
        for (x, y, z) in [
            (0.0_f32, 0.0, 0.0),
            (1.0, -2.0, 3.5),
            (std::f32::consts::PI, 0.1, -0.1),
        ] {
            let a = quat_from_euler_xyz(x, y, z);
            let b = quat_from_euler_xyz(x, y, z);
            assert_eq!(a.x.to_bits(), b.x.to_bits());
            assert_eq!(a.y.to_bits(), b.y.to_bits());
            assert_eq!(a.z.to_bits(), b.z.to_bits());
            assert_eq!(a.w.to_bits(), b.w.to_bits());
        }
    }

    /// Cross-architecture determinism oracle: a fixed table of
    /// `(x, y, z)` Euler-radian inputs mapped to the exact `f32` bit
    /// pattern `quat_from_euler_xyz` must produce for each, computed once
    /// (on `aarch64`) and pinned as literals here -- this test runs
    /// identically on every CI runner (`ubuntu-latest`/`windows-latest` are
    /// `x86_64`, `macos-latest` is `aarch64`), so a mismatch on any one of
    /// them is exactly the cross-architecture drift `alice_det_math::sin_cos`
    /// exists to prevent (unlike `glam::Quat::from_euler`, which dispatches
    /// to a different SIMD/libm backend per target). If `alice_det_math`
    /// ever changes its own rounding, this is the test that should be
    /// re-generated (see the module doc's "Why this exists" for how), not
    /// loosened to a tolerance.
    #[test]
    fn quat_from_euler_xyz_is_bit_identical_across_architectures() {
        type Row = ((f32, f32, f32), (u32, u32, u32, u32));
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_3, FRAC_PI_4, FRAC_PI_6, PI};
        // FRAC_PI_2/FRAC_PI_4/FRAC_PI_6/FRAC_PI_3 are bit-identical to
        // 90/45/30/60 .to_radians() (confirmed separately) -- the inputs
        // below really are those degree values, just spelled as the named
        // constant clippy wants here.
        let table: &[Row] = &[
            (
                (0.0, 0.0, 0.0),
                (0x0000_0000, 0x8000_0000, 0x0000_0000, 0x3f80_0000),
            ),
            (
                (FRAC_PI_2, 0.0, 0.0),
                (0x3f35_04f4, 0x8000_0000, 0x0000_0000, 0x3f35_04f3),
            ),
            (
                (0.0, FRAC_PI_4, 0.0),
                (0x0000_0000, 0x3ec3_ef16, 0x0000_0000, 0x3f6c_835e),
            ),
            (
                (0.0, 0.0, FRAC_PI_6),
                (0x0000_0000, 0x8000_0000, 0x3e84_83ee, 0x3f77_46ea),
            ),
            (
                (0.174_532_92, 0.349_065_84, FRAC_PI_6),
                (0x3e02_be66, 0x3e14_5aef, 0x3e89_7d87, 0x3f71_9743),
            ),
            (
                (-FRAC_PI_4, FRAC_PI_3, -2.094_395_2),
                (0xbf10_d58a, 0xbd65_8d0f, 0xbf49_e06a, 0x3e6f_f811),
            ),
            (
                (PI, 0.0, 0.0),
                (0x3f80_0000, 0x8000_0000, 0x8000_0000, 0xb33b_bd2e),
            ),
            (
                (1.0, -2.0, 3.5),
                (0xbf45_d6a5, 0xbdfc_6f29, 0x3f09_d982, 0x3e9f_f8d0),
            ),
        ];
        for &((x, y, z), (ex, ey, ez, ew)) in table {
            let q = quat_from_euler_xyz(x, y, z);
            assert_eq!(q.x.to_bits(), ex, "x component at ({x}, {y}, {z})");
            assert_eq!(q.y.to_bits(), ey, "y component at ({x}, {y}, {z})");
            assert_eq!(q.z.to_bits(), ez, "z component at ({x}, {y}, {z})");
            assert_eq!(q.w.to_bits(), ew, "w component at ({x}, {y}, {z})");
        }
    }

    /// Same cross-architecture oracle as
    /// `quat_from_euler_xyz_is_bit_identical_across_architectures`, for the
    /// single-axis builders `stdlib` uses directly (not through the
    /// `rotate`/`rotate_q` text grammar): `can_rack`'s tilt and the other
    /// `stdlib` shapes build their `SdfNode::Rotate` through these, so they
    /// need the same guarantee `rotate(...)` itself has.
    #[test]
    fn quat_from_rotation_axes_are_bit_identical_across_architectures() {
        use std::f32::consts::{FRAC_PI_2, PI};
        type Row = (
            f32,
            (u32, u32, u32, u32),
            (u32, u32, u32, u32),
            (u32, u32, u32, u32),
        );
        let table: &[Row] = &[
            (
                0.0,
                (0x0000_0000, 0x0000_0000, 0x0000_0000, 0x3f80_0000),
                (0x0000_0000, 0x0000_0000, 0x0000_0000, 0x3f80_0000),
                (0x0000_0000, 0x0000_0000, 0x0000_0000, 0x3f80_0000),
            ),
            (
                FRAC_PI_2,
                (0x3f35_04f4, 0x0000_0000, 0x0000_0000, 0x3f35_04f3),
                (0x0000_0000, 0x3f35_04f4, 0x0000_0000, 0x3f35_04f3),
                (0x0000_0000, 0x0000_0000, 0x3f35_04f4, 0x3f35_04f3),
            ),
            (
                -FRAC_PI_2,
                (0xbf35_04f4, 0x0000_0000, 0x0000_0000, 0x3f35_04f3),
                (0x0000_0000, 0xbf35_04f4, 0x0000_0000, 0x3f35_04f3),
                (0x0000_0000, 0x0000_0000, 0xbf35_04f4, 0x3f35_04f3),
            ),
            (
                PI,
                (0x3f80_0000, 0x0000_0000, 0x0000_0000, 0xb33b_bd2e),
                (0x0000_0000, 0x3f80_0000, 0x0000_0000, 0xb33b_bd2e),
                (0x0000_0000, 0x0000_0000, 0x3f80_0000, 0xb33b_bd2e),
            ),
            (
                -2.670_354, // a representative non-special angle (a can_rack-like tilt)
                (0xbf78_ed3d, 0x0000_0000, 0x0000_0000, 0x3e6f_0c49),
                (0x0000_0000, 0xbf78_ed3d, 0x0000_0000, 0x3e6f_0c49),
                (0x0000_0000, 0x0000_0000, 0xbf78_ed3d, 0x3e6f_0c49),
            ),
        ];
        for &(a, (xx, xy, xz, xw), (yx, yy, yz, yw), (zx, zy, zz, zw)) in table {
            let qx = quat_from_rotation_x(a);
            assert_eq!(
                (
                    qx.x.to_bits(),
                    qx.y.to_bits(),
                    qx.z.to_bits(),
                    qx.w.to_bits()
                ),
                (xx, xy, xz, xw),
                "quat_from_rotation_x({a})"
            );
            let qy = quat_from_rotation_y(a);
            assert_eq!(
                (
                    qy.x.to_bits(),
                    qy.y.to_bits(),
                    qy.z.to_bits(),
                    qy.w.to_bits()
                ),
                (yx, yy, yz, yw),
                "quat_from_rotation_y({a})"
            );
            let qz = quat_from_rotation_z(a);
            assert_eq!(
                (
                    qz.x.to_bits(),
                    qz.y.to_bits(),
                    qz.z.to_bits(),
                    qz.w.to_bits()
                ),
                (zx, zy, zz, zw),
                "quat_from_rotation_z({a})"
            );
        }
    }

    /// Formula cross-check against `glam`'s own `from_rotation_x/y/z`, same
    /// spirit as `matches_glam_from_euler_on_a_sweep`: a different trig
    /// backend computing the same construction, bounded to a few ulp, not
    /// bit-identical.
    #[test]
    fn quat_from_rotation_axes_match_glam_on_a_sweep() {
        let angles: &[f32] = &[-3.0, -1.5, -0.7, -0.1, 0.0, 0.05, 0.3, 0.9, 1.57, 2.0, 2.9];
        let mut max_diff = 0.0_f32;
        for &a in angles {
            for (got, want) in [
                (quat_from_rotation_x(a), glam::Quat::from_rotation_x(a)),
                (quat_from_rotation_y(a), glam::Quat::from_rotation_y(a)),
                (quat_from_rotation_z(a), glam::Quat::from_rotation_z(a)),
            ] {
                for (g, w) in [
                    (got.x, want.x),
                    (got.y, want.y),
                    (got.z, want.z),
                    (got.w, want.w),
                ] {
                    max_diff = max_diff.max((g - w).abs());
                }
            }
        }
        assert!(max_diff < 1e-5, "max_diff={max_diff:e}");
    }
}
