//! The platform `f32::max`/`f32::min`, generated into each module allowed to call them.
//!
//! `f32::max`/`f32::min` do not specify which of two equal-magnitude,
//! opposite-signed zeros they return, and `aarch64`/`x86_64` disagree on it —
//! [`crate::det_math::max`]/[`min`] (the Law form) is what production code
//! calls. The handful of modules below are provably safe despite calling the
//! platform function directly (a constant nonzero operand, both operands
//! already `.abs()`-derived, or the result cast to an integer immediately
//! after, each erasing the sign distinction before it could matter) — see
//! `scripts/raw_minmax_manifest.toml` for the exact modules and their
//! individual justification.
//!
//! ## Why a macro that generates private items, not shared `pub(crate)` fns
//!
//! An earlier version of this file was three `pub(crate) fn`s, with
//! `scripts/raw_minmax_manifest.py` counting call sites by scanning source
//! text for the bare function name. That is defeatable without changing
//! what runs: `use raw_max_f32 as x;` then calling `x(a, b)` hides under a
//! different name, and `.fold(0.0, raw_max_f32)` passes the function itself
//! as a value, never writing a textually-matchable call at all. Requiring a
//! [`Reason`] argument closed the second hole (a bare function reference no
//! longer type-checks against a 2-argument fold/map), but the first — an
//! alias simply isn't the text the manifest looks for — survives any
//! text-based scheme, no matter how the function signature is shaped.
//!
//! `raw_minmax_here!()` closes it with ordinary Rust item privacy instead
//! of a scan: invoking it defines `raw_max_f32`/`raw_min_f32`/`raw_max_f64`
//! as **private items of the invoking module** — not re-exported,
//! forwardable, or nameable from anywhere else, so there is no shared item
//! anywhere in the crate for an alias to rename. "Which modules may call
//! the raw platform max/min" is instead enforced by
//! `clippy::disallowed_macros` (`disallowed-macros` in `clippy.toml`,
//! naming this macro's own definition path): invoking the macro from a
//! module that does not carry a reasoned
//! `#![allow(clippy::disallowed_macros, reason = "...")]` is a clippy
//! error. Clippy resolves `disallowed_macros` by the macro's **definition**
//! path, not the spelling used to invoke it, so `use raw_minmax_here as x;
//! x!();` is caught exactly the same as calling it by its own name —
//! verified against a minimal standalone crate before relying on it here
//! (clippy reported `use of a disallowed macro
//! "alice_lol::raw_minmax::raw_minmax_here"` for the aliased invocation).
//!
//! `scripts/raw_minmax_manifest.py` now tracks the list of modules carrying
//! that allow (not a per-call count): a module invoking the macro without
//! the allow fails clippy directly (no separate script needed to catch
//! that); the script's job is the opposite direction — a module listed as
//! allowed that no longer actually invokes the macro is a stale entry to
//! remove, and a module invoking the macro that is not yet listed is a new
//! site to review and add.

/// The specific, named safety class a caller is asserting for one call to
/// the generated `raw_max_f32`/`raw_min_f32`/`raw_max_f64` — required as an
/// argument (not load-bearing to the computed value) so the call cannot be
/// spelled without naming it. Variant names match
/// `scripts/disallowed_methods_allow_reasons.py`'s `nonzero-const:`/
/// `both-abs:`/`int-cast:` prefixes one-for-one.
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

// The ONE real implementation of each operation, each with its own
// disallowed_methods exemption attribute — kept OUTSIDE the macro body
// below deliberately: putting that same exemption inside a macro's
// expansion was found (empirically, against a standalone repro) to make
// clippy stop honouring an INVOKING module's own disallowed_macros
// exemption, flagging every invocation of raw_minmax_here!() as disallowed
// regardless of that allow. Moving the disallowed_methods exemption onto a
// plain (non-macro-generated) function here, with the macro only
// generating a thin wrapper that calls through, avoids the interaction:
// the macro's own expansion carries no lint-level attribute of its own to
// conflict with the invoking module's.
#[allow(
    clippy::disallowed_methods,
    reason = "raw-helper: the one place f32::max is named and allowed, see alice-lol/src/raw_minmax.rs's module doc"
)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "see alice-lol/src/det_math.rs's own max() for why"
)]
pub(crate) const fn impl_max_f32(a: f32, b: f32) -> f32 {
    a.max(b)
}

#[allow(
    clippy::disallowed_methods,
    reason = "raw-helper: the one place f32::min is named and allowed, see alice-lol/src/raw_minmax.rs's module doc"
)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "see alice-lol/src/det_math.rs's own max() for why"
)]
pub(crate) const fn impl_min_f32(a: f32, b: f32) -> f32 {
    a.min(b)
}

#[allow(
    clippy::disallowed_methods,
    reason = "raw-helper: the one place f64::max is named and allowed, see alice-lol/src/raw_minmax.rs's module doc"
)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "see alice-lol/src/det_math.rs's own max() for why"
)]
pub(crate) const fn impl_max_f64(a: f64, b: f64) -> f64 {
    a.max(b)
}

/// Defines the named subset of `raw_max_f32`/`raw_min_f32`/`raw_max_f64` as
/// private items of the invoking module — see the module doc above for why
/// this is a macro rather than shared `pub(crate)` functions. Invoke once
/// per module, at module scope (not inside a function), naming only the
/// helper(s) that module actually calls (`raw_minmax_here!(max_f32);` or
/// `raw_minmax_here!(max_f32, min_f32);`), alongside a reasoned
/// `#![allow(clippy::disallowed_macros, reason = "...")]` at the top of
/// that same module — `clippy.toml`'s `disallowed-macros` entry for this
/// macro's path is what enforces that every OTHER module is refused.
///
/// Naming only what is used, rather than always generating all three and
/// relying on `#[allow(dead_code)]` for the unused ones, is deliberate, not
/// just tidiness: an `#[allow(...)]` of ANY kind on the macro-generated
/// item (`dead_code` included, tried first) was found — empirically, against
/// a standalone repro — to make clippy stop honouring the invoking
/// module's own `#![allow(clippy::disallowed_macros, ...)]`, flagging every
/// invocation as disallowed regardless of that allow. A module that never
/// has an unused generated helper never needs that attribute, sidestepping
/// the interaction entirely.
///
/// Each generated function is a thin wrapper over the real `impl_*`
/// function above (see the comment there for the matching reason the
/// `disallowed_methods` allow lives there and not in this macro body).
macro_rules! raw_minmax_here {
    ($($which:ident),+ $(,)?) => {
        $( $crate::raw_minmax::raw_minmax_here!(@one $which); )+
    };
    (@one max_f32) => {
        const fn raw_max_f32(a: f32, b: f32, _reason: $crate::raw_minmax::Reason) -> f32 {
            $crate::raw_minmax::impl_max_f32(a, b)
        }
    };
    (@one min_f32) => {
        const fn raw_min_f32(a: f32, b: f32, _reason: $crate::raw_minmax::Reason) -> f32 {
            $crate::raw_minmax::impl_min_f32(a, b)
        }
    };
    (@one max_f64) => {
        const fn raw_max_f64(a: f64, b: f64, _reason: $crate::raw_minmax::Reason) -> f64 {
            $crate::raw_minmax::impl_max_f64(a, b)
        }
    };
}
pub(crate) use raw_minmax_here;

#[cfg(test)]
mod tests {
    /// The generated functions are plain (private) items of `generated`,
    /// same as in any real caller module — so the tests that exercise them
    /// live inside this module too, rather than trying to reach in from
    /// `super`, which the same privacy applies against.
    mod generated {
        #![allow(
            clippy::disallowed_macros,
            reason = "this test module is what exercises the macro's own generated code, not a production caller claiming a nonzero-const:/both-abs:/int-cast: proof"
        )]
        use super::super::Reason;

        crate::raw_minmax::raw_minmax_here!(max_f32, min_f32, max_f64);

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
    }

    /// Mutation teeth (2026-10-13): the `Reason` argument exists to make a
    /// bare function reference (`.fold(0.0, raw_max_f64)`) stop
    /// type-checking, forcing an explicit closure that still names both the
    /// function and its `Reason::` variant in source text. Verified by
    /// hand against a standalone repro: `.fold(0.0_f64, raw_max_f64)` (2
    /// args expected by `fold`, 3 required here) fails to compile with
    /// E0593 ("function is expected to take 2 arguments, but it takes 3
    /// arguments") — reverting to a 2-argument signature (dropping
    /// `_reason: Reason`) makes it compile again, which is exactly the
    /// hole this closes.
    ///
    /// The macro-generated privacy itself (the other half of this file's
    /// design) is not demonstrated by an in-tree test either, for the same
    /// reason: `cargo clippy` cannot show "an outside module is refused"
    /// without an actual second, unauthorized module committed to the
    /// tree, which would then itself need removing before this gate could
    /// ever be green again. Verified by hand twice instead: against a
    /// standalone crate (see the module doc's "verified against a minimal
    /// standalone crate" paragraph), and once more directly against this
    /// crate (a temporary unauthorized invocation appended to
    /// `alice-lol/src/limits.rs`, confirmed to fail
    /// `cargo clippy -p alice-lol --lib -- -D clippy::disallowed_macros`
    /// with the expected diagnostic, then reverted).
    /// `scripts/raw_minmax_manifest.py`'s own self-tests are the in-tree
    /// regression coverage for the manifest-side half.
    #[test]
    fn mutation_teeth_marker_reason_argument_defeats_bare_function_reference() {
        // signpost only; see the doc comment above.
    }
}
