//! `max` / `min` with a defined result on every target and profile.
//!
//! `f32::max` / `f32::min` leave the sign of an equal pair of zeros
//! unspecified, and the targets differ: `aarch64` returns +0 from max and
//! -0 from min, `x86_64` returns the argument (`0.0f32.max(-0.0)` is -0
//! there). The functions below fix that case: a NaN operand yields the
//! other operand (as std does), an equal pair of zeros yields +0 from max
//! and -0 from min (IEEE 754-2019 maximumNumber / minimumNumber, which is
//! what `aarch64` already returns), and every other pair is ordered by
//! value. Production code uses these; the lib clippy pass forbids
//! `f32::max` / `f32::min` (`.github/clippy-lib/clippy.toml`).
//!
//! These bodies are a blob-synced copy of `alice-lol`'s own crate-internal
//! `det_math::max`/`min` (itself a blob-synced copy of `alice-physics`'s),
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
pub(crate) fn max(a: f32, b: f32) -> f32 {
    if a.is_nan() || b > a {
        b
    } else if b.is_nan() || a > b || !a.is_sign_negative() {
        a
    } else {
        b
    }
}

/// `min` with a defined zero sign: NaN yields the other operand, `min(-0.0,
/// 0.0)` and `min(0.0, -0.0)` are `-0.0`
#[inline]
#[must_use]
#[allow(clippy::redundant_pub_crate, reason = "see max() above")]
pub(crate) fn min(a: f32, b: f32) -> f32 {
    if a.is_nan() || b < a {
        b
    } else if b.is_nan() || a < b || a.is_sign_negative() {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod max_min_tests {
    use super::{max, min};

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

    fn expected_min(a: f64, b: f64) -> f64 {
        match (a.is_nan(), b.is_nan()) {
            (true, _) => b,
            (_, true) => a,
            _ if a < b => a,
            _ if b < a => b,
            _ => {
                if a == 0.0 && (a.is_sign_negative() || b.is_sign_negative()) {
                    -0.0
                } else {
                    a
                }
            }
        }
    }

    const LATTICE: [f32; 7] = [
        f32::NEG_INFINITY,
        -1.0,
        -0.0,
        0.0,
        1.0,
        f32::INFINITY,
        f32::NAN,
    ];

    fn same(x: f32, y: f64) -> bool {
        (x.is_nan() && y.is_nan()) || f64::from(x).to_bits() == y.to_bits()
    }

    #[test]
    fn max_and_min_follow_the_rule_on_the_signed_zero_lattice() {
        let mut compared = 0;
        for &a in &LATTICE {
            for &b in &LATTICE {
                let (x, y) = (core::hint::black_box(a), core::hint::black_box(b));
                assert!(
                    same(max(x, y), expected_max(f64::from(a), f64::from(b))),
                    "max({a:?}, {b:?}) = {:?}",
                    max(x, y)
                );
                assert!(
                    same(min(x, y), expected_min(f64::from(a), f64::from(b))),
                    "min({a:?}, {b:?}) = {:?}",
                    min(x, y)
                );
                compared += 2;
            }
        }
        assert_eq!(compared, 2 * 49);
    }

    #[test]
    fn the_zero_pair_has_the_documented_sign_in_both_orders() {
        let (p, n) = (
            core::hint::black_box(0.0f32),
            core::hint::black_box(-0.0f32),
        );
        assert_eq!(max(p, n).to_bits(), 0x0000_0000);
        assert_eq!(max(n, p).to_bits(), 0x0000_0000);
        assert_eq!(min(p, n).to_bits(), 0x8000_0000);
        assert_eq!(min(n, p).to_bits(), 0x8000_0000);
    }
}
