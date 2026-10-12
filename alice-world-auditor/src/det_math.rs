//! `max64` with a defined result on every target and profile.
//!
//! `f64::max` leaves the sign of an equal pair of zeros unspecified, and the
//! targets differ: `aarch64` returns +0, `x86_64` returns the argument
//! (and `x86_64`'s `f64::min(0.0, -0.0)` is -0 unoptimized and +0
//! optimized, showing the split is also profile-dependent there). The
//! function below fixes that case: a NaN operand yields the other operand
//! (as std does), an equal pair of zeros yields +0 (IEEE 754-2019
//! maximumNumber, which is what `aarch64` already returns), and every
//! other pair is ordered by value. Production code uses this; the lib
//! clippy pass forbids `f64::max` (`.github/clippy-lib/clippy.toml`).
//!
//! This body is a blob-synced copy of `alice-lol`'s own crate-internal
//! `det_math::max64` (itself a blob-synced copy of `alice-physics`'s),
//! same IEEE 754-2019 rule, independent implementations kept byte-identical
//! across all three repos — checked by
//! `scripts/det_math_minmax_blob_sync.py`.

/// `max` with a defined zero sign: NaN yields the other operand, `max(-0.0,
/// 0.0)` and `max(0.0, -0.0)` are `+0.0`
#[inline]
#[must_use]
#[allow(
    clippy::redundant_pub_crate,
    reason = "pub(crate) is intentional defense-in-depth, not an oversight: mod det_math is \
    private today, but pub(crate) keeps these crate-only even if that ever changes, unlike pub"
)]
pub(crate) fn max64(a: f64, b: f64) -> f64 {
    if a.is_nan() || b > a {
        b
    } else if b.is_nan() || a > b || !a.is_sign_negative() {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod max64_tests {
    use super::max64;

    fn expected_max(a: f64, b: f64) -> f64 {
        match (a.is_nan(), b.is_nan()) {
            (true, _) => b,
            (_, true) => a,
            _ if a > b => a,
            _ if b > a => b,
            _ => {
                if a != 0.0 {
                    a
                } else if a.is_sign_negative() && b.is_sign_negative() {
                    -0.0
                } else {
                    0.0
                }
            }
        }
    }

    const LATTICE: [f64; 7] = [
        f64::NEG_INFINITY,
        -1.0,
        -0.0,
        0.0,
        1.0,
        f64::INFINITY,
        f64::NAN,
    ];

    fn same(x: f64, y: f64) -> bool {
        (x.is_nan() && y.is_nan()) || x.to_bits() == y.to_bits()
    }

    #[test]
    fn max64_follows_the_rule_on_the_signed_zero_lattice() {
        let mut compared = 0;
        for &a in &LATTICE {
            for &b in &LATTICE {
                let (x, y) = (core::hint::black_box(a), core::hint::black_box(b));
                assert!(
                    same(max64(x, y), expected_max(a, b)),
                    "max64({a:?}, {b:?}) = {:?}",
                    max64(x, y)
                );
                compared += 1;
            }
        }
        assert_eq!(compared, 49);
    }

    #[test]
    fn the_zero_pair_has_the_documented_sign_in_both_orders() {
        let (p, n) = (
            core::hint::black_box(0.0f64),
            core::hint::black_box(-0.0f64),
        );
        assert_eq!(max64(p, n).to_bits(), 0);
        assert_eq!(max64(n, p).to_bits(), 0);
    }
}
