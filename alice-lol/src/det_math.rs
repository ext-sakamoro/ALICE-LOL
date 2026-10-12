//! `max` / `min` with a defined result on every target and profile.
//!
//! `f32::max` / `f32::min` (and `f64`) leave the sign of an equal pair of
//! zeros unspecified, and the targets differ: `aarch64` returns +0 from max and
//! -0 from min, `x86_64` returns the argument (`0.0f32.max(-0.0)` is -0 there),
//! and `x86_64`'s `f64::min(0.0, -0.0)` is -0 unoptimized and +0 optimized. The
//! functions below fix that case: a NaN operand yields the other operand (as
//! std does), an equal pair of zeros yields +0 from max and -0 from min (IEEE
//! 754-2019 maximumNumber / minimumNumber, which is what `aarch64` already
//! returns), and every other pair is ordered by value. Production code uses
//! these; the lib clippy pass forbids `f32::max` / `f32::min` / `f64::max` /
//! `f64::min` (`.github/clippy-lib/clippy.toml`).
//!
//! These bodies are a blob-synced copy of `alice-physics`'s own crate-internal
//! `det_math::max`/`min`/`max64`/`min64` (same IEEE 754-2019 rule, independent
//! implementations kept byte-identical so the two repos cannot drift on the
//! tie-break — checked by `scripts/test_det_math_minmax_blob_sync.py`).

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

/// [`max`] for `f64`
#[inline]
#[must_use]
#[allow(clippy::redundant_pub_crate, reason = "see max() above")]
pub(crate) fn max64(a: f64, b: f64) -> f64 {
    if a.is_nan() || b > a {
        b
    } else if b.is_nan() || a > b || !a.is_sign_negative() {
        a
    } else {
        b
    }
}

/// [`min`] for `f64`
#[inline]
#[must_use]
#[allow(clippy::redundant_pub_crate, reason = "see max() above")]
pub(crate) fn min64(a: f64, b: f64) -> f64 {
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
    use super::{max, max64, min, min64};

    // the expected value on {-inf, -1, -0, +0, +1, +inf, NaN}^2, written out
    // by rule rather than computed with the std functions under test
    fn expected_max(a: f64, b: f64) -> f64 {
        match (a.is_nan(), b.is_nan()) {
            (true, _) => b,
            (_, true) => a,
            _ if a > b => a,
            _ if b > a => b,
            // equal: only the zero pair can differ in bits
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
    fn max_and_min_follow_the_rule_on_the_signed_zero_lattice() {
        let mut compared = 0;
        for &a in &LATTICE {
            for &b in &LATTICE {
                // black_box keeps the comparisons at run time, so the
                // profile under test compiles the branches itself
                let (x, y) = (core::hint::black_box(a), core::hint::black_box(b));
                // the lattice values are exactly representable in f32 (±1, ±0,
                // ±inf, NaN), so this never actually truncates
                #[allow(clippy::cast_possible_truncation)]
                let (xf, yf) = (x as f32, y as f32);
                assert!(
                    same(max64(x, y), expected_max(a, b)),
                    "max64({a:?}, {b:?}) = {:?}",
                    max64(x, y)
                );
                assert!(
                    same(min64(x, y), expected_min(a, b)),
                    "min64({a:?}, {b:?}) = {:?}",
                    min64(x, y)
                );
                assert!(
                    same(f64::from(max(xf, yf)), expected_max(a, b)),
                    "max({a:?}, {b:?}) = {:?}",
                    max(xf, yf)
                );
                assert!(
                    same(f64::from(min(xf, yf)), expected_min(a, b)),
                    "min({a:?}, {b:?}) = {:?}",
                    min(xf, yf)
                );
                compared += 4;
            }
        }
        assert_eq!(compared, 4 * 49);
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
        let (p, n) = (
            core::hint::black_box(0.0f64),
            core::hint::black_box(-0.0f64),
        );
        assert_eq!(max64(p, n).to_bits(), 0);
        assert_eq!(max64(n, p).to_bits(), 0);
        assert_eq!(min64(p, n).to_bits(), 0x8000_0000_0000_0000);
        assert_eq!(min64(n, p).to_bits(), 0x8000_0000_0000_0000);
    }
}
