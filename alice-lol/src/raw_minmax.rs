//! The platform `f32::max`/`f32::min`, named and gathered in one place.
//!
//! `f32::max`/`f32::min` do not specify which of two equal-magnitude,
//! opposite-signed zeros they return, and `aarch64`/`x86_64` disagree on it —
//! [`crate::det_math::max`]/[`min`] (the Law form) is what production code
//! calls. The handful of sites below are provably safe despite calling the
//! platform function directly (a constant nonzero operand, both operands
//! already `.abs()`-derived, or the result cast to an integer immediately
//! after, each erasing the sign distinction before it could matter) — see
//! `scripts/raw_minmax_manifest.toml` for the exact call sites and their
//! individual justification.
//!
//! This file is the only place the `disallowed_methods` lint is named and
//! allowed for `f32::max`/`f32::min`: a per-call exemption scattered through
//! production code silences the lint for its whole enclosing statement,
//! which could also hide an unrelated, newly-introduced libm call in the
//! same statement. Naming the exception here instead means every other call
//! site keeps the lint live.

#[allow(
    clippy::disallowed_methods,
    reason = "raw-helper: the one place f32::max is named and allowed, see scripts/raw_minmax_manifest.toml for each caller"
)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "see alice-lol/src/det_math.rs's own max() for why"
)]
pub(crate) const fn raw_max_f32(a: f32, b: f32) -> f32 {
    a.max(b)
}

#[allow(
    clippy::disallowed_methods,
    reason = "raw-helper: the one place f32::min is named and allowed, see scripts/raw_minmax_manifest.toml for each caller"
)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "see alice-lol/src/det_math.rs's own max() for why"
)]
pub(crate) const fn raw_min_f32(a: f32, b: f32) -> f32 {
    a.min(b)
}

#[allow(
    clippy::disallowed_methods,
    reason = "raw-helper: the one place f64::max is named and allowed, see scripts/raw_minmax_manifest.toml for each caller"
)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "see alice-lol/src/det_math.rs's own max() for why"
)]
pub(crate) const fn raw_max_f64(a: f64, b: f64) -> f64 {
    a.max(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_max_matches_std() {
        assert_eq!(raw_max_f32(1.0, 2.0).to_bits(), 1.0_f32.max(2.0).to_bits());
        assert_eq!(
            raw_max_f32(-1.0, -2.0).to_bits(),
            (-1.0_f32).max(-2.0).to_bits()
        );
    }

    #[test]
    fn raw_min_matches_std() {
        assert_eq!(raw_min_f32(1.0, 2.0).to_bits(), 1.0_f32.min(2.0).to_bits());
        assert_eq!(
            raw_min_f32(-1.0, -2.0).to_bits(),
            (-1.0_f32).min(-2.0).to_bits()
        );
    }

    #[test]
    fn raw_max_f64_matches_std() {
        assert_eq!(raw_max_f64(1.0, 2.0).to_bits(), 1.0_f64.max(2.0).to_bits());
        assert_eq!(
            raw_max_f64(-1.0, -2.0).to_bits(),
            (-1.0_f64).max(-2.0).to_bits()
        );
    }
}
