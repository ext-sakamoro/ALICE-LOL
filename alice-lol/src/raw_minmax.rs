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
//!
//! ## `Reason`: why a text manifest alone is not the enforcement
//!
//! `scripts/raw_minmax_manifest.py` counts call sites by scanning source
//! text for the bare function name — which a caller can defeat without
//! changing what runs: `use raw_max_f32 as x;` then calling `x(a, b)` hides
//! under a different name, and `.fold(0.0, raw_max_f32)` passes the function
//! itself as a value, never writing a textually-matchable call at all. Both
//! of those compiled before `Reason` was required.
//!
//! Requiring a [`Reason`] argument closes both: a function reference no
//! longer matches the signature a 2-argument fold/map expects, so passing
//! one bare (`raw_max_f32` without parens) only type-checks where a
//! 3-argument function is expected, which forces an explicit wrapping
//! closure at the call site instead — and that closure's body still spells
//! out the function name and a `Reason::` variant in source text, exactly
//! where the manifest script (and a reader) can see it. An alias still
//! works, but now also needs the `Reason::` variant named inline, which
//! does not become any harder to grep for just because the function itself
//! is renamed.
//!
//! This does not make the manifest text-scan redundant — the two check
//! different things: the manifest's `file`-to-`count` ratchet catches a
//! site being added or removed without updating
//! `scripts/raw_minmax_manifest.toml` in the same commit (a drift/review
//! discipline), while `Reason` is what actually gates the compiler against
//! the specific evasions above.

/// The specific, named safety class a caller is asserting for one call to
/// [`raw_max_f32`]/[`raw_min_f32`]/[`raw_max_f64`] — required as an
/// argument (not load-bearing to the computed value, see each function's
/// body) so the call cannot be spelled without naming it; see the module
/// doc above for why a required argument of this shape, rather than a
/// larger visibility change, is what closes the manifest's known gaps.
/// Variant names match `scripts/disallowed_methods_allow_reasons.py`'s
/// `nonzero-const:`/`both-abs:`/`int-cast:` prefixes one-for-one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "see alice-lol/src/det_math.rs's own max() for why"
)]
pub(crate) enum Reason {
    /// the other operand is a literal or expression provably never zero,
    /// so the equal-zero tie this exemption exists for cannot occur
    NonzeroConst,
    /// every operand compared is a `.abs()` result (or a quantity built
    /// only from `.abs()` results and a `+0.0`/positive seed), so it is
    /// always `+0` or positive, never `-0`
    BothAbs,
    /// the result is cast to an integer type immediately after, which
    /// erases the sign distinction before anything observes it
    IntCast,
}

#[allow(
    clippy::disallowed_methods,
    reason = "raw-helper: the one place f32::max is named and allowed, see scripts/raw_minmax_manifest.toml for each caller"
)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "see alice-lol/src/det_math.rs's own max() for why"
)]
pub(crate) const fn raw_max_f32(a: f32, b: f32, _reason: Reason) -> f32 {
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
pub(crate) const fn raw_min_f32(a: f32, b: f32, _reason: Reason) -> f32 {
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
pub(crate) const fn raw_max_f64(a: f64, b: f64, _reason: Reason) -> f64 {
    a.max(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_max_matches_std() {
        assert_eq!(
            raw_max_f32(1.0, 2.0, Reason::NonzeroConst).to_bits(),
            1.0_f32.max(2.0).to_bits()
        );
        assert_eq!(
            raw_max_f32(-1.0, -2.0, Reason::NonzeroConst).to_bits(),
            (-1.0_f32).max(-2.0).to_bits()
        );
    }

    #[test]
    fn raw_min_matches_std() {
        assert_eq!(
            raw_min_f32(1.0, 2.0, Reason::NonzeroConst).to_bits(),
            1.0_f32.min(2.0).to_bits()
        );
        assert_eq!(
            raw_min_f32(-1.0, -2.0, Reason::NonzeroConst).to_bits(),
            (-1.0_f32).min(-2.0).to_bits()
        );
    }

    #[test]
    fn raw_max_f64_matches_std() {
        assert_eq!(
            raw_max_f64(1.0, 2.0, Reason::NonzeroConst).to_bits(),
            1.0_f64.max(2.0).to_bits()
        );
        assert_eq!(
            raw_max_f64(-1.0, -2.0, Reason::NonzeroConst).to_bits(),
            (-1.0_f64).max(-2.0).to_bits()
        );
    }

    /// Mutation teeth (2026-10-13): the `Reason` argument exists to make a
    /// bare function reference (`.fold(0.0, raw_max_f64)`) stop
    /// type-checking, forcing an explicit closure that still names both the
    /// function and its `Reason::` variant in source text. Verified by
    /// hand: `.fold(0.0_f64, raw_max_f64)` (2 args expected by `fold`, 3
    /// required here) fails to compile with "closure takes 1 argument"/
    /// "function takes 3 arguments" — reverting to a 2-argument signature
    /// (dropping `_reason: Reason`) makes it compile again, which is
    /// exactly the hole this change closes.
    #[test]
    fn mutation_teeth_marker_reason_argument_defeats_bare_function_reference() {
        // signpost only; see the doc comment above and raw_minmax.rs's
        // module doc "why a text manifest alone is not the enforcement".
    }
}
