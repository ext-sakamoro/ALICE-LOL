//! 解析解突合 oracle — `mul_add` の outer negation を operand negation に書き換える Law form
//!
//! `tools/no-outer-fneg-of-fma`'s F2 check landed 29 real findings across
//! `stdlib/hardsurface/pattern_sdf.rs` / `skadis_sdf.rs` / `alice-lol-ui`'s
//! `layout.rs` / one `tests/analytic_hardsurface.rs` site: a bound
//! `let t = a.mul_add(s, k);` whose value was later negated (`-t`), buried
//! inside a tuple/array literal, a later statement, an `if` branch, or an
//! argument to a *different* `mul_add` call. Every one of those was rewritten
//! to the Law-adopted form — negate both operands instead of the call's
//! result, `a.mul_add(-s, -k)` in place of `-(a.mul_add(s, k))` — which the
//! `no-outer-fneg-of-fma` gate itself no longer flags.
//!
//! That rewrite is mathematically the same value everywhere EXCEPT at exact
//! cancellation (`a*s + k == 0.0` precisely): IEEE 754 round-to-nearest
//! rounds the EXACT sum of two opposite-signed operands that is precisely
//! zero to `+0.0` regardless of which operand carries the overall minus sign
//! — so `a.mul_add(-s, -k)` lands on `+0.0` at that point the same way
//! `a.mul_add(s, k)` does, while the OLD form (negate the already-rounded
//! result) carries `-(+0.0) == -0.0` instead. This is the Law-adopted
//! convention already established for `mul_add` cancellation elsewhere in this
//! workspace (see `alice-lol/src/angle.rs`'s
//! `y_component_cancellation_lattice_is_debug_release_identical_and_canonical`),
//! not a new rule invented for this rewrite — the 29 sites now MATCH that
//! existing convention instead of the pre-rewrite form's inconsistent one.
//!
//! None of this workspace's existing closed-form oracle tests for the 29
//! sites' own functions (`analytic_hardsurface.rs`, `analytic_cavity_mount.rs`,
//! `analytic_skadis_sdf.rs`) specifically drive an input to exact
//! cancellation — a continuous sweep of spec dimensions essentially never
//! lands on the exact float value that cancels a particular `mul_add`, so
//! their passing is evidence of general correctness, not evidence this
//! specific zero-sign edge held. This file is the one that does: it pins the
//! identity directly (not through any one of the 29 call sites' own private
//! bindings — several share the exact same shape with different variable
//! names, so one lattice over the shape covers all of them), at the lattice
//! of `(a, s, k)` triples where `a * s + k` is exactly zero.

#![allow(
    clippy::float_cmp,
    reason = "the whole point of this file is exact (not approximate) equality to 0.0"
)]

#[test]
fn negated_operands_mul_add_lands_on_positive_zero_at_exact_cancellation_f32() {
    // each triple: a * s + k == 0.0 exactly, by the PRODUCT and the addend
    // carrying genuinely opposite signs (a real cancellation, not "both
    // terms are trivially zero" -- see the degenerate-all-zero test below
    // for why that is a different case with a different answer)
    let lattice: &[(f32, f32, f32)] = &[
        (2.0, 3.0, -6.0),
        (-2.0, 3.0, 6.0),
        (2.0, -3.0, 6.0),
        (0.5, 4.0, -2.0),
        (1.0, 1.0, -1.0),
        (-1.0, -1.0, -1.0),
    ];
    for &(a, s, k) in lattice {
        // sanity: this triple really is an exact-cancellation input for the
        // OLD form too, not just for the rewritten one
        assert_eq!(
            a.mul_add(s, k),
            0.0,
            "(a={a}, s={s}, k={k}): not actually an exact-cancellation input, lattice entry is wrong"
        );
        let old_form = -(a.mul_add(s, k));
        let new_form = a.mul_add(-s, -k);
        assert_eq!(
            old_form.to_bits(),
            (-0.0f32).to_bits(),
            "(a={a}, s={s}, k={k}): old form = {:08x}, expected the canonical -0.0 this rewrite moves away from",
            old_form.to_bits()
        );
        assert_eq!(
            new_form.to_bits(),
            0.0f32.to_bits(),
            "(a={a}, s={s}, k={k}): new form = {:08x}, expected the canonical +0.0 (0x00000000) \
             the no-outer-fneg-of-fma gate's Law form converges on",
            new_form.to_bits()
        );
    }
}

#[test]
fn negated_operands_mul_add_lands_on_positive_zero_at_exact_cancellation_f64() {
    let lattice: &[(f64, f64, f64)] = &[(2.0, 3.0, -6.0), (-2.0, 3.0, 6.0), (0.5, 4.0, -2.0)];
    for &(a, s, k) in lattice {
        assert_eq!(
            a.mul_add(s, k),
            0.0,
            "(a={a}, s={s}, k={k}): lattice entry is wrong"
        );
        let old_form = -(a.mul_add(s, k));
        let new_form = a.mul_add(-s, -k);
        assert_eq!(old_form.to_bits(), (-0.0f64).to_bits());
        assert_eq!(new_form.to_bits(), 0.0f64.to_bits());
    }
}

/// The degenerate "every operand is identically zero" input (`a=0, s=0,
/// k=0`, as opposed to the lattice above's genuine opposite-sign
/// cancellation) is NOT a case where the two forms diverge: `a * s` and `k`
/// are both `+0.0` here (the SAME sign, not opposite), and IEEE 754 only
/// defaults a same-signed `+0`/`+0` exact sum to `+0` -- the Law-form
/// rewrite negates both operands here too (`a * (-s)` and `-k`), making
/// them both `-0.0` instead (still the same sign as each other, just
/// flipped), so the sum stays `-0.0` on both sides. This was caught by this
/// test file's own first draft, which wrongly asserted `+0.0` here too --
/// recorded as its own case rather than silently dropped, since it is the
/// one lattice point that does NOT get the "Law form always normalises to
/// +0.0" property the other tests in this file pin.
#[test]
fn all_zero_operands_do_not_diverge_between_the_two_forms() {
    let old_form = -(0.0f32.mul_add(0.0, 0.0));
    let new_form = 0.0f32.mul_add(-0.0, -0.0);
    assert_eq!(old_form.to_bits(), (-0.0f32).to_bits());
    assert_eq!(
        new_form.to_bits(),
        (-0.0f32).to_bits(),
        "new form = {:08x}, expected -0.0 (same sign as the old form here, not the +0.0 the \
         genuine-cancellation lattice above pins)",
        new_form.to_bits()
    );
}

/// The two-separate-negated-terms shape (`alice-lol-ui`'s `layout.rs`:
/// `row_f.mul_add(-(max_h + gap), -(max_h * 0.5))` in place of
/// `-row_f.mul_add(max_h + gap, max_h * 0.5)`) is the same identity with the
/// slope and addend each wrapped in their own sub-expression rather than
/// being plain bindings — pinned separately since the operands here are
/// themselves computed, not bare values, so a sign error in either
/// sub-expression would not be caught by the simpler lattice above.
#[test]
fn two_negated_subexpression_operands_also_land_on_positive_zero_at_exact_cancellation() {
    // genuine (opposite-sign) cancellation lattice; see the dedicated
    // all-zero test above for why `(row_f, max_h, gap) = (0, 0, 0)` is not
    // in this set (it does not diverge between the two forms at all).
    // (row_f, max_h, gap) with s = max_h + gap, k = max_h * 0.5:
    //   (-1.0, 4.0, -2.0): s = 2.0, k = 2.0, row_f * s + k = -2.0 + 2.0 = 0.0
    //   (1.0, 4.0, -6.0):  s = -2.0, k = 2.0, row_f * s + k = -2.0 + 2.0 = 0.0
    let lattice: &[(f32, f32, f32)] = &[(-1.0, 4.0, -2.0), (1.0, 4.0, -6.0)];
    for &(row_f, max_h, gap) in lattice {
        let k = max_h * 0.5;
        let s = max_h + gap;
        assert_eq!(
            row_f.mul_add(s, k),
            0.0,
            "(row_f={row_f}, max_h={max_h}, gap={gap}): lattice entry is wrong"
        );
        let new_form = row_f.mul_add(-s, -k);
        assert_eq!(
            new_form.to_bits(),
            0.0f32.to_bits(),
            "(row_f={row_f}, max_h={max_h}, gap={gap}): new form = {:08x}, expected +0.0",
            new_form.to_bits()
        );
    }
}
