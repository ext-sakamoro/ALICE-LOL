//! Named resource limits for user-controlled parameters, and the error types
//! that report a refusal.
//!
//! Two different hazards hide behind "a user gave a huge number":
//!
//! - **Eager expansion**: a site that allocates one `SdfNode` (or clones one)
//!   per repetition up front, e.g. `Vec::with_capacity(dx * dy)` followed by a
//!   loop that pushes a cloned child into it (`stdlib::hardsurface::pattern_sdf`'s
//!   gridfinity dividers). Memory cost is `count * size_of::<SdfNode>()` plus one
//!   heap allocation per repeated child, paid at construction time, before the
//!   tree is ever evaluated. [`MAX_NODE_EXPANSION`] bounds this.
//! - **Lazy repetition**: `alice_sdf::SdfNode::RepeatFinite` stores one
//!   `Arc<SdfNode>` child and a `[u32; 3]` count; evaluating it costs O(1) at
//!   construction (no allocation per repeat, the count is read lazily when the
//!   field is sampled). A *large* count here is not itself a bug -- a 200×200
//!   hex-hole grid on a real panel is a legitimate, affordable `RepeatFinite`.
//!   The actual hazard is a *degenerate* count: dividing a panel size by a
//!   pitch of 0 (or a negative/non-finite pitch) saturates the `as u32` cast
//!   to `u32::MAX`, which is a garbage value regardless of its magnitude, not
//!   a "too big" one. [`MIN_PITCH_MM`] (combined with the existing
//!   [`MAX_STDLIB_COUNT`]-bounded count derived from it) catches the
//!   degenerate input at its source instead of guessing a count ceiling that
//!   would also refuse legitimate large-but-lazy grids.
//!
//! `MAX_STDLIB_COUNT` / `MAX_SKADIS_PANEL_MM` lived in `runtime_parser.rs`
//! (the only two limits that existed before this module); re-exported here so
//! every limit lives in one place, old import paths keep working.
//!
//! ## Two error shapes, not one
//!
//! [`ResourceLimitError`] is for *integer counts* (a product of two `u32`
//! divider counts, say) -- there is no ambiguity in representing the
//! requested value as a `u64`, since the inputs were already integers.
//!
//! [`FloatLimitError`] is for the float-valued checks
//! ([`checked_positive_finite`], [`checked_bounded`]): an earlier version of
//! this module coerced the offending `f32` into a `u64` micrometer count for
//! the error message, which collapsed `NaN` and a negative value to the same
//! `requested=0` -- indistinguishable in the message, even though they are
//! different mistakes. [`FloatLimitError`] instead keeps the literal `f32`
//! (whose `Display` already renders `NaN` / `inf` / `-5` distinctly) and
//! names the unit in the message.

/// Maximum value for a per-axis "count" argument (rows / cols / dividers /
/// etc.), checked by `runtime_parser.rs`'s `bounded_count` before an eager
/// construction site ever sees it. 1024 (pre-existing; unchanged by this
/// module).
pub use crate::runtime_parser::MAX_STDLIB_COUNT;

/// Maximum finite size (mm) for a SKADIS panel edge, checked by
/// `runtime_parser.rs`'s `skadis_size`. 2000mm (pre-existing; unchanged by
/// this module).
pub use crate::runtime_parser::MAX_SKADIS_PANEL_MM;

/// Maximum total element count for an **eager** expansion site -- one that
/// clones/allocates an `SdfNode` per repetition up front (not a lazy
/// `RepeatFinite`, see the module doc).
///
/// Measured 2026-10-10: `size_of::<alice_sdf::SdfNode>()` is 80 bytes (a
/// `RepeatFinite`'s own shape: one `Arc<Self>` pointer + a `[u32; 3]` count +
/// a `Vec3`, the largest-ish variant; other variants are smaller or equal).
/// An eager site additionally pays one heap allocation per repeated child
/// (e.g. `translate(cavity.clone(), ..)` wraps each clone in its own
/// `Arc`-holding node). 10,000 elements is roughly 10,000 * (80 bytes + a
/// small allocation) -- on the order of 6MB, not a denial of service by
/// itself.
///
/// This is also a **language semantics change**, not just a safety net: the
/// `.lol` parser previously bounded each divider axis independently to
/// [`MAX_STDLIB_COUNT`] (1024) but never their *product*, so a request like
/// `gridfinity_bin_ex(1,1,1,101,100,0,0)` (10,100 dividers) was previously
/// accepted (expensively: ~6MB, measured) and is refused from this version
/// on. The ceiling is set by the physical geometry, not by an arbitrary
/// safety margin: `dividers` subdivides a single bin cell
/// ([`GRID_UNIT`](crate::stdlib::hardsurface::pattern_sdf::gridfinity_spec::GRID_UNIT)
/// = 42mm) into compartments; a divider count anywhere near 100 per axis
/// implies a compartment narrower than the wall thickness needed to print
/// it, so it was never a producible part regardless of this limit. Every
/// current stdlib caller of an eager site (`gridfinity_bin_ex`'s dividers)
/// uses at most 2*2 = 4 in its tests/examples, so 10,000 is headroom over
/// real usage, not a tight fit. Documented in CHANGELOG as a breaking change
/// (`#### 挙動`, 0.4.0); folded into the single `LOL_SEMANTICS_ID` move once
/// the rest of this phase lands.
pub const MAX_NODE_EXPANSION: u64 = 10_000;

/// Minimum finite value (mm) for a pitch/spacing argument that a count is
/// derived from by division (e.g. `panel_size / pitch`).
///
/// Geometric justification, not an arbitrary epsilon: 0.01mm (10 micron) is
/// already far below the smallest feature a consumer FDM printer can
/// resolve (typically 100-400 micron nozzle width), so no legitimate design
/// needs a smaller pitch; a request below it is either a mistake or
/// adversarial input, not a real part. Combined with the existing
/// [`MAX_STDLIB_COUNT`] bound on the *count* the division produces, a pitch
/// at this floor against a panel at [`MAX_SKADIS_PANEL_MM`] (2000mm) still
/// yields a count of 200,000 -- the count bound is what actually constrains
/// it, this floor only rejects the *degenerate* (zero/negative/tiny-enough-
/// to-overflow) case at its source, per the module doc's "lazy repetition"
/// case.
pub const MIN_PITCH_MM: f32 = 0.01;

/// A user-controlled integer-count parameter refused a named resource limit
/// before any allocation was produced from it.
///
/// `kind` names which limit was hit (a short, stable, `snake_case` token
/// such as `"grid_expansion"`, suitable for matching in tests without
/// string-parsing a human sentence); `limit` and `requested` are the limit's
/// value and what was actually asked for, in the same (integer) units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimitError {
    /// Which limit was hit, e.g. `"grid_expansion"`.
    pub kind: &'static str,
    /// The limit's value.
    pub limit: u64,
    /// What was actually requested (saturates to `u64::MAX` if the request
    /// itself overflows representing it).
    pub requested: u64,
}

impl std::fmt::Display for ResourceLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "resource limit exceeded: kind={} limit={} requested={}",
            self.kind, self.limit, self.requested
        )
    }
}

impl std::error::Error for ResourceLimitError {}

/// A user-controlled **float** parameter refused a named resource limit.
///
/// Unlike [`ResourceLimitError`], `value` and `limit` keep the literal `f32`
/// the caller supplied instead of coercing it into an integer unit -- so the
/// message distinguishes `NaN` from `-5` from `1e30`, which a `requested=0`
/// placeholder for both `NaN` and a negative value could not. `unit` names
/// the physical unit for the message (e.g. `"mm"`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloatLimitError {
    /// Which limit was hit, e.g. `"pitch"`, `"panel_size"`.
    pub kind: &'static str,
    /// The limit's value, in `unit`.
    pub limit: f32,
    /// What was actually requested, in `unit` (kept literal: may be `NaN` or
    /// infinite).
    pub value: f32,
    /// The physical unit both `limit` and `value` are in, e.g. `"mm"`.
    pub unit: &'static str,
}

impl std::fmt::Display for FloatLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "resource limit exceeded: kind={} limit={} {} value={} {}",
            self.kind, self.limit, self.unit, self.value, self.unit
        )?;
        // rotate_q's tolerance is tight enough (ROTATION_QUAT_UNIT_TOLERANCE) that a
        // hand-typed literal with only 3-4 decimal digits is rejected far more often
        // than not: say so here, where a human or an LLM actually reads the refusal,
        // not only in the docs (LLM_REFERENCE.md / syntax.md).
        if self.kind == "rotation_quat_unit" {
            write!(
                f,
                " (rotate_q needs about 8 significant digits per component to land \
                 inside this tolerance; for a hand-written rotation, use rotate(x, y, z) \
                 in degrees instead -- rotate_q is meant for a machine-computed exact \
                 quaternion, not one typed by hand)"
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for FloatLimitError {}

/// A `Spec` struct's `validate()` refused its fields, and the matching
/// `try_*` builder refused to construct an `SdfNode` from them.
///
/// `#[non_exhaustive]`: a `Spec` may grow new validated fields with their
/// own reasons to refuse, without that being a breaking change to this enum.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum SpecError {
    /// A named integer-count resource limit was exceeded; see [`ResourceLimitError`].
    ResourceLimit(ResourceLimitError),
    /// A named float-valued resource limit was exceeded; see [`FloatLimitError`].
    FloatLimit(FloatLimitError),
}

impl From<ResourceLimitError> for SpecError {
    fn from(e: ResourceLimitError) -> Self {
        Self::ResourceLimit(e)
    }
}

impl From<FloatLimitError> for SpecError {
    fn from(e: FloatLimitError) -> Self {
        Self::FloatLimit(e)
    }
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResourceLimit(e) => write!(f, "{e}"),
            Self::FloatLimit(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SpecError {}

/// `a * b <= limit`.
///
/// Checked without ever forming the product in a width that could overflow
/// (`u32 * u32` promoted to `u64` cannot overflow: the largest possible
/// product, `u32::MAX * u32::MAX`, fits in `u64`).
///
/// # Errors
///
/// [`ResourceLimitError`] (`kind` as given) if the product exceeds `limit`.
pub fn checked_product(
    a: u32,
    b: u32,
    limit: u64,
    kind: &'static str,
) -> Result<u64, ResourceLimitError> {
    let product = u64::from(a) * u64::from(b);
    if product > limit {
        return Err(ResourceLimitError {
            kind,
            limit,
            requested: product,
        });
    }
    Ok(product)
}

/// `v.is_finite() && v >= floor`.
///
/// The degenerate-pitch check described in the module doc (a pitch that is
/// zero, negative, `NaN`, or infinite is refused at its source, before it is
/// divided into anything).
///
/// # Errors
///
/// [`FloatLimitError`] (`kind` as given, `unit = "mm"`) if `v` is not finite
/// or is below `floor`.
pub fn checked_positive_finite(
    v: f32,
    floor: f32,
    kind: &'static str,
) -> Result<f32, FloatLimitError> {
    if !v.is_finite() || v < floor {
        return Err(FloatLimitError {
            kind,
            limit: floor,
            value: v,
            unit: "mm",
        });
    }
    Ok(v)
}

/// `v.is_finite() && v > floor && v <= ceiling`.
///
/// The degenerate-*size* check: unlike [`checked_positive_finite`] (a floor
/// only, for a pitch/spacing a count is later derived from), a size that
/// itself drives a loop bounded by `pos >= size` (e.g. `skadis_panel_sdf`'s
/// connector-hole placement, which starts at `i = 1` and advances by a fixed
/// pitch until `pos >= size`) hangs for `NaN` (the comparison is always
/// `false`, so the loop never terminates) and for `+inf` or any merely huge
/// finite value (the loop runs `size / pitch` times before it terminates,
/// which is unbounded without an upper limit here) -- not only for a
/// degenerate *small* value. An upper bound is therefore load-bearing on its
/// own, not just a finite check.
///
/// # Errors
///
/// [`FloatLimitError`] (`kind` as given, `unit = "mm"`, `limit = ceiling`) if
/// `v` is not finite, is at or below `floor`, or is above `ceiling`.
pub fn checked_bounded(
    v: f32,
    floor: f32,
    ceiling: f32,
    kind: &'static str,
) -> Result<f32, FloatLimitError> {
    if !(v.is_finite() && v > floor && v <= ceiling) {
        return Err(FloatLimitError {
            kind,
            limit: ceiling,
            value: v,
            unit: "mm",
        });
    }
    Ok(v)
}

/// The TPMS scale/thickness contract shared by every way to build one of the
/// 9 `alice_sdf` TPMS surfaces.
///
/// `gyroid`/`schwarz_p`/`diamond_surface`/`neovius`/`lidinoid`/`iwp`/`frd`/
/// `fischer_koch_s`/`pmy` each divide a bounded quantity by `scale`
/// (`d.abs() / scale - thickness`), so `scale` must be finite and strictly
/// greater than `0.0` or the division is `±inf`/`NaN`; `thickness` only has
/// to be finite (it is subtracted, not divided by).
///
/// The floor is plain `scale > 0.0`, not [`checked_positive_finite`]'s usual
/// inclusive floor (this needs every positive value, including subnormals
/// down to `f32::from_bits(1)`, so no single inclusive floor constant would
/// do). An earlier version of this check used `>= 1e-6` instead, to match
/// `fmt_f32`'s then-near-zero-collapsing behavior (a scale accepted below
/// that threshold would have been silently rounded to `0.0` by the first
/// `to_lol`, making the emitted text unparseable by this very check).
/// `fmt_f32` is a faithful round-trip for every finite value now (no
/// near-zero collapsing left), so that extra margin is no longer needed: any
/// finite positive scale, however small, round-trips through emit/parse
/// unchanged.
///
/// Every entrance that builds one of these 9 node types goes through this one
/// function: the runtime `.lol` text parser (`runtime_parser.rs`, both the 9
/// direct constructors and the 3 `lattice_infill`/`diamond_infill`/
/// `schwarz_infill` wrappers, which build the same node types through a
/// different surface syntax), the `lol!` compile-time macro
/// (`alice-lol-macro`'s `codegen.rs`, which inlines this same check for a
/// literal scale/thickness known at macro-expansion time, and calls this
/// function for a captured runtime expression), and
/// `stdlib::hardsurface::reinforcement::gyroid_infill` (a plain Rust builder,
/// which panics on [`SpecError`] per this crate's existing `try_*` +
/// `# Panics` convention for an infallible public builder).
///
/// # Errors
///
/// [`SpecError`] if `scale` is non-finite or not strictly positive, or
/// `thickness` is non-finite.
pub fn checked_tpms_fields(scale: f32, thickness: f32) -> Result<(f32, f32), SpecError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(FloatLimitError {
            kind: "tpms_scale",
            limit: 0.0,
            value: scale,
            unit: "mm",
        }
        .into());
    }
    let thickness = checked_positive_finite(thickness, f32::NEG_INFINITY, "tpms_thickness")?;
    Ok((scale, thickness))
}

/// The unit-magnitude tolerance `rotate_q(x, y, z, w, child)`'s quaternion
/// must fall within.
///
/// Derived, not guessed: `crate::angle::quat_from_euler_xyz` (the function
/// `rotate(x, y, z, child)` itself builds its quaternion with) measured a
/// maximum `||q|^2 - 1|` of `4.7139432e-7` (about `3.95 * f32::EPSILON`) over
/// a dense 101^3 sweep of `[-pi, pi]^3` -- every quaternion `emit` ever
/// writes out as `rotate_q(...)`, including one built from an ordinary
/// `rotate(x, y, z, child)`, must still parse back through this same check.
/// `8.0 * f32::EPSILON` gives about 2x headroom over that measured max (a
/// finer sweep, or a future change to `quat_from_euler_xyz`'s formula, could
/// plausibly land a few ulp further out without legitimately meaning
/// "non-unit").
pub const ROTATION_QUAT_UNIT_TOLERANCE: f32 = 8.0 * f32::EPSILON;

/// The `rotate_q(x, y, z, w, child)` contract: `x`/`y`/`z`/`w` must all be
/// finite, and `x^2 + y^2 + z^2 + w^2` must be within
/// [`ROTATION_QUAT_UNIT_TOLERANCE`] of `1.0`.
///
/// Unlike [`checked_tpms_fields`], this does **not** normalise the result --
/// the literal `x`/`y`/`z`/`w` the caller gave are returned unchanged (as a
/// `glam::Quat`) when they pass. Renormalising would silently rewrite a
/// value the user wrote to a DIFFERENT value, defeating the entire point of
/// `rotate_q` (an exact, bit-preserving alternative to `rotate`'s lossy
/// Euler-degrees form); it would also make `parse(emit(rotate_q(...)))`
/// non-idempotent for any input that was not already exactly at its
/// normalised form. A near-unit-but-not-exactly-unit quaternion is accepted
/// as-is: `glam::Quat::mul_vec3` (what `eval`'s `Rotate` transform calls)
/// applies it as given, and the tolerance above is tight enough that the
/// resulting scale error is far below `eval`'s own `1e-4` parity threshold
/// used elsewhere in this crate's oracles.
///
/// A quaternion further from unit than the tolerance is refused: unlike
/// `alice_sdf::SdfNode::Scale`, which explicitly divides the evaluated
/// distance back down by its `factor`, `Rotate`'s `eval` does
/// nothing to correct for a non-unit quaternion's implicit scaling -- a
/// `rotate_q` this crate silently accepted un-checked would, for any
/// magnitude far enough from `1.0` to matter, break the distance-field
/// Lipschitz bound every other primitive and modifier in this crate is
/// expected to hold.
///
/// # Errors
///
/// [`SpecError`] if any of `x`/`y`/`z`/`w` is non-finite, or if
/// `x^2 + y^2 + z^2 + w^2` is further than [`ROTATION_QUAT_UNIT_TOLERANCE`]
/// from `1.0`.
pub fn checked_rotation_quat(x: f32, y: f32, z: f32, w: f32) -> Result<glam::Quat, SpecError> {
    if !(x.is_finite() && y.is_finite() && z.is_finite() && w.is_finite()) {
        return Err(FloatLimitError {
            kind: "rotation_quat_finite",
            limit: 0.0,
            value: if x.is_finite() {
                if y.is_finite() {
                    if z.is_finite() {
                        w
                    } else {
                        z
                    }
                } else {
                    y
                }
            } else {
                x
            },
            unit: "unitless",
        }
        .into());
    }
    let mag2 = f64::from(x).mul_add(
        f64::from(x),
        f64::from(y).mul_add(
            f64::from(y),
            f64::from(z).mul_add(f64::from(z), f64::from(w) * f64::from(w)),
        ),
    );
    let dev = (mag2 - 1.0).abs();
    if dev > f64::from(ROTATION_QUAT_UNIT_TOLERANCE) {
        #[allow(clippy::cast_possible_truncation)]
        let dev_f32 = dev as f32;
        return Err(FloatLimitError {
            kind: "rotation_quat_unit",
            limit: ROTATION_QUAT_UNIT_TOLERANCE,
            value: dev_f32,
            unit: "|q|^2-1",
        }
        .into());
    }
    Ok(glam::Quat::from_xyzw(x, y, z, w))
}

#[cfg(test)]
mod tests {
    use super::{
        checked_bounded, checked_positive_finite, checked_product, checked_rotation_quat,
        checked_tpms_fields, SpecError, MAX_NODE_EXPANSION, MIN_PITCH_MM,
    };

    #[test]
    fn checked_product_within_limit_is_ok() {
        assert_eq!(
            checked_product(2, 2, MAX_NODE_EXPANSION, "grid_expansion"),
            Ok(4)
        );
    }

    #[test]
    fn checked_product_at_the_limit_is_ok() {
        assert_eq!(
            checked_product(100, 100, 10_000, "grid_expansion"),
            Ok(10_000)
        );
    }

    #[test]
    fn checked_product_one_over_the_limit_errs() {
        let e = checked_product(100, 101, 10_000, "grid_expansion").unwrap_err();
        assert_eq!(e.kind, "grid_expansion");
        assert_eq!(e.limit, 10_000);
        assert_eq!(e.requested, 10_100);
    }

    #[test]
    fn checked_product_cannot_overflow_even_at_u32_max() {
        // the whole point of promoting to u64 before multiplying: this must
        // not wrap around to a small, falsely-passing value
        let e =
            checked_product(u32::MAX, u32::MAX, MAX_NODE_EXPANSION, "grid_expansion").unwrap_err();
        assert_eq!(e.requested, u64::from(u32::MAX) * u64::from(u32::MAX));
    }

    #[test]
    fn checked_positive_finite_above_floor_is_ok() {
        assert_eq!(
            checked_positive_finite(20.0, MIN_PITCH_MM, "pitch"),
            Ok(20.0)
        );
    }

    #[test]
    fn checked_positive_finite_exactly_at_floor_is_ok() {
        assert_eq!(
            checked_positive_finite(MIN_PITCH_MM, MIN_PITCH_MM, "pitch"),
            Ok(MIN_PITCH_MM)
        );
    }

    #[test]
    #[allow(clippy::float_cmp)] // v is set to the exact literal, no arithmetic in between
    fn checked_positive_finite_zero_errs_and_value_is_literally_zero() {
        let e = checked_positive_finite(0.0, MIN_PITCH_MM, "pitch").unwrap_err();
        assert_eq!(e.kind, "pitch");
        assert_eq!(e.value, 0.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // v is set to the exact literal, no arithmetic in between
    fn checked_positive_finite_negative_errs_and_value_is_the_literal_negative() {
        let e = checked_positive_finite(-5.0, MIN_PITCH_MM, "pitch").unwrap_err();
        assert_eq!(e.value, -5.0);
    }

    #[test]
    fn checked_positive_finite_nan_errs_and_value_is_distinguishable_from_negative() {
        let e = checked_positive_finite(f32::NAN, MIN_PITCH_MM, "pitch").unwrap_err();
        assert!(e.value.is_nan());
        // the whole point of this type: a NaN message must not read the same
        // as a negative-value message (the old u64-coerced "requested=0" did)
        assert!(e.to_string().contains("NaN"), "{}", e.to_string());
        let neg = checked_positive_finite(-5.0, MIN_PITCH_MM, "pitch").unwrap_err();
        assert_ne!(e.to_string(), neg.to_string());
    }

    #[test]
    fn checked_positive_finite_infinite_errs_and_value_is_literally_infinite() {
        let e = checked_positive_finite(f32::INFINITY, MIN_PITCH_MM, "pitch").unwrap_err();
        assert!(e.value.is_infinite() && e.value > 0.0);
        assert!(e.to_string().contains("inf"), "{}", e.to_string());
    }

    #[test]
    fn checked_positive_finite_just_below_floor_errs() {
        let below = MIN_PITCH_MM - 0.001;
        assert!(checked_positive_finite(below, MIN_PITCH_MM, "pitch").is_err());
    }

    #[test]
    fn checked_bounded_within_range_is_ok() {
        assert_eq!(
            checked_bounded(1000.0, 0.0, 2000.0, "panel_size"),
            Ok(1000.0)
        );
    }

    #[test]
    fn checked_bounded_at_the_ceiling_is_ok() {
        assert_eq!(
            checked_bounded(2000.0, 0.0, 2000.0, "panel_size"),
            Ok(2000.0)
        );
    }

    #[test]
    fn checked_bounded_at_the_floor_errs() {
        // floor is exclusive: a panel size of exactly 0 is still degenerate
        assert!(checked_bounded(0.0, 0.0, 2000.0, "panel_size").is_err());
    }

    #[test]
    fn checked_bounded_nan_errs_without_hanging() {
        let e = checked_bounded(f32::NAN, 0.0, 2000.0, "panel_size").unwrap_err();
        assert_eq!(e.kind, "panel_size");
        assert!(e.value.is_nan());
    }

    #[test]
    fn checked_bounded_infinite_errs() {
        let e = checked_bounded(f32::INFINITY, 0.0, 2000.0, "panel_size").unwrap_err();
        assert!(e.value.is_infinite() && e.value > 0.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // limit/value are set to the exact literals, no arithmetic in between
    fn checked_bounded_over_ceiling_reports_the_actual_request_not_a_placeholder() {
        let e = checked_bounded(50_000.0, 0.0, 2000.0, "panel_size").unwrap_err();
        assert_eq!(e.limit, 2000.0);
        assert_eq!(e.value, 50_000.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // value is set to the exact literal, no arithmetic in between
    fn checked_bounded_negative_errs_and_value_is_distinguishable_from_nan() {
        let e = checked_bounded(-5.0, 0.0, 2000.0, "panel_size").unwrap_err();
        assert_eq!(e.value, -5.0);
        let nan = checked_bounded(f32::NAN, 0.0, 2000.0, "panel_size").unwrap_err();
        assert_ne!(e.to_string(), nan.to_string());
    }

    #[test]
    fn spec_error_from_resource_limit_error_displays_the_same_text() {
        let e = checked_product(1000, 1000, MAX_NODE_EXPANSION, "grid_expansion").unwrap_err();
        let spec_err: SpecError = e.into();
        assert_eq!(spec_err.to_string(), e.to_string());
    }

    #[test]
    fn spec_error_from_float_limit_error_displays_the_same_text() {
        let e = checked_bounded(50_000.0, 0.0, 2000.0, "panel_size").unwrap_err();
        let spec_err: SpecError = e.into();
        assert_eq!(spec_err.to_string(), e.to_string());
    }

    #[test]
    fn resource_limit_error_display_names_all_three_fields() {
        let e = checked_product(1000, 1000, MAX_NODE_EXPANSION, "grid_expansion").unwrap_err();
        let s = e.to_string();
        assert!(s.contains("grid_expansion"), "{s}");
        assert!(s.contains(&MAX_NODE_EXPANSION.to_string()), "{s}");
        assert!(s.contains("1000000"), "{s}");
    }

    #[test]
    fn float_limit_error_display_names_kind_and_unit() {
        let e = checked_bounded(50_000.0, 0.0, 2000.0, "panel_size").unwrap_err();
        let s = e.to_string();
        assert!(s.contains("panel_size"), "{s}");
        assert!(s.contains("mm"), "{s}");
        assert!(s.contains("50000"), "{s}");
    }

    #[test]
    fn checked_tpms_fields_rejects_a_degenerate_scale() {
        for scale in [
            -0.0,
            0.0,
            -1.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -5e-7,
        ] {
            assert!(
                checked_tpms_fields(scale, 0.1).is_err(),
                "scale={scale} should be rejected"
            );
        }
    }

    #[test]
    fn checked_tpms_fields_rejects_a_non_finite_thickness() {
        for thickness in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(
                checked_tpms_fields(3.0, thickness).is_err(),
                "thickness={thickness} should be rejected"
            );
        }
        // a negative thickness is not restricted (only its finiteness is)
        assert!(checked_tpms_fields(3.0, -5.0).is_ok());
    }

    #[test]
    fn checked_tpms_fields_accepts_the_threshold_and_ordinary_values() {
        assert_eq!(checked_tpms_fields(1e-6, 0.1).unwrap(), (1e-6, 0.1));
        assert_eq!(checked_tpms_fields(3.0, 0.1).unwrap(), (3.0, 0.1));
    }

    /// Any finite positive scale is accepted, with no lower floor beyond
    /// `> 0.0` -- down to the smallest positive subnormal `f32` -- now that
    /// `fmt_f32` no longer needs a margin against its own near-zero
    /// collapsing (it doesn't collapse anything any more).
    #[test]
    fn checked_tpms_fields_accepts_any_finite_positive_scale_including_subnormals() {
        for scale in [1e-45_f32, 1e-40, 1e-10, 5e-7] {
            assert!(scale > 0.0, "test input itself must be positive: {scale}");
            assert_eq!(
                checked_tpms_fields(scale, 0.1).unwrap(),
                (scale, 0.1),
                "scale={scale} should be accepted"
            );
        }
    }

    #[test]
    fn checked_rotation_quat_rejects_non_finite_components() {
        for (x, y, z, w) in [
            (f32::NAN, 0.0, 0.0, 1.0),
            (0.0, f32::INFINITY, 0.0, 1.0),
            (0.0, 0.0, f32::NEG_INFINITY, 1.0),
            (0.0, 0.0, 0.0, f32::NAN),
        ] {
            assert!(
                checked_rotation_quat(x, y, z, w).is_err(),
                "({x},{y},{z},{w}) should be rejected (non-finite)"
            );
        }
    }

    #[test]
    fn checked_rotation_quat_accepts_the_identity_and_an_ordinary_unit_quat() {
        let q = checked_rotation_quat(0.0, 0.0, 0.0, 1.0).unwrap();
        assert_eq!((q.x, q.y, q.z, q.w), (0.0, 0.0, 0.0, 1.0));
        // (0.6, 0.8, 0.0, 0.0): 0.6^2 + 0.8^2 = 1.0 exactly in f32
        let q = checked_rotation_quat(0.6, 0.8, 0.0, 0.0).unwrap();
        assert_eq!((q.x, q.y, q.z, q.w), (0.6, 0.8, 0.0, 0.0));
    }

    #[test]
    fn checked_rotation_quat_rejects_a_clearly_non_unit_quat() {
        for (x, y, z, w) in [
            (0.0_f32, 0.0, 0.0, 0.0),
            (2.0, 0.0, 0.0, 0.0),
            (1.0, 1.0, 0.0, 0.0),
        ] {
            assert!(
                checked_rotation_quat(x, y, z, w).is_err(),
                "({x},{y},{z},{w}) is far from unit and should be rejected"
            );
        }
    }

    /// The rejection message itself says why a hand-typed `rotate_q` literal
    /// is so often refused (the tolerance needs ~8 significant digits), not
    /// just the bare numbers -- a reader (human or LLM) hitting this error
    /// should not have to go find the docs to learn that `rotate(x, y, z)`
    /// in degrees is the form meant for hand-written rotations.
    #[test]
    fn checked_rotation_quat_error_message_explains_the_digit_requirement() {
        let e = checked_rotation_quat(1.0, 1.0, 0.0, 0.0).unwrap_err();
        let s = e.to_string();
        assert!(s.contains("significant digits"), "{s}");
        assert!(s.contains("rotate(x, y, z)"), "{s}");
    }

    /// The boundary itself, at two ADJACENT `f32` bit patterns either side of
    /// [`ROTATION_QUAT_UNIT_TOLERANCE`] (found by sweeping bit patterns near
    /// `sqrt(1 + tolerance)` and computing each candidate's own `|q|^2-1` the
    /// same way `checked_rotation_quat` does, not derived analytically --
    /// `f32` rounding on the way to the boundary is exactly the thing a
    /// formula-based construction would get wrong): pins the tolerance to an
    /// exact value instead of only exercising "clearly inside" / "clearly
    /// outside" cases, so a mutant that shifts the comparison by even one bit
    /// (`>` vs `>=`, or a slightly different constant) is caught.
    #[test]
    fn checked_rotation_quat_boundary_is_exact() {
        // x=y=z=0.0, so |q|^2-1 = w*w-1 exactly (0.mul_add(0, w*w) == w*w,
        // no intermediate rounding): w_inside=1.0000004 (bits 0x3f800003)
        // measures |q|^2-1 = 7.152558652023799e-7 (<= tol); the very next
        // representable f32, w_outside=1.0000005 (bits 0x3f800004), measures
        // 9.536745437799254e-7 (> tol = 9.5367431640625e-7 by a hair).
        let w_inside = f32::from_bits(0x3f80_0003);
        let w_outside = f32::from_bits(0x3f80_0004);
        assert!(
            checked_rotation_quat(0.0, 0.0, 0.0, w_inside).is_ok(),
            "w={w_inside:?} (|q|^2-1 just inside tolerance) should be accepted"
        );
        assert!(
            checked_rotation_quat(0.0, 0.0, 0.0, w_outside).is_err(),
            "w={w_outside:?} (|q|^2-1 just outside tolerance) should be rejected"
        );
    }

    /// Accepted input is returned bit-exact, never renormalised -- the
    /// contract `rotate_q`'s emit/parse idempotence depends on.
    #[test]
    fn checked_rotation_quat_does_not_renormalise() {
        // w is the exact "just inside tolerance" boundary value from
        // checked_rotation_quat_boundary_is_exact (|q|^2-1 = 7.15e-7, not
        // 0): close enough to unit to be accepted, far enough that
        // normalising it would actually move the bits -- a quaternion
        // this close to exactly unit (e.g. (0.6, 0.8, 0.0, 1e-6)) does NOT
        // catch a `.normalize()` mutant, since the correction is far below
        // f32's precision at that magnitude and the bits come out the same
        // either way (confirmed: that weaker case passed even with
        // `.normalize()` inserted into the accept path).
        let (qx, qy, qz, qw) = (0.0_f32, 0.0, 0.0, f32::from_bits(0x3f80_0003));
        let q = checked_rotation_quat(qx, qy, qz, qw).unwrap();
        assert_eq!(q.x.to_bits(), qx.to_bits());
        assert_eq!(q.y.to_bits(), qy.to_bits());
        assert_eq!(q.z.to_bits(), qz.to_bits());
        assert_eq!(q.w.to_bits(), qw.to_bits());
    }
}
