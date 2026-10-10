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
        )
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

#[cfg(test)]
mod tests {
    use super::{
        checked_bounded, checked_positive_finite, checked_product, SpecError, MAX_NODE_EXPANSION,
        MIN_PITCH_MM,
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
}
