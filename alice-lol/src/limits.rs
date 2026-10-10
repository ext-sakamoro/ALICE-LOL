//! Named resource limits for user-controlled parameters, and the error type
//! that reports a refusal.
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
/// small allocation) -- on the order of a few MB, not a denial of service by itself -- while
/// every current stdlib caller of an eager site (`gridfinity_bin_ex`'s
/// dividers) uses at most 2*2 = 4 in its tests/examples, so this is headroom,
/// not a tight fit.
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
// ALLOW-UNWIRED: pending the pitch-class Spec sites (hex_hole_pitch etc.), landed separately
pub const MIN_PITCH_MM: f32 = 0.01;

/// A user-controlled parameter refused a named resource limit before any
/// allocation or lazy-but-degenerate count was produced from it.
///
/// `kind` names which limit was hit (a short, stable, `snake_case` token
/// such as `"grid_expansion"` or `"pitch"`, suitable for matching in tests
/// without string-parsing a human sentence); `limit` and `requested` are the
/// limit's value and what was actually asked for, in the same units (always
/// representable as `u64`: counts and products of counts for expansion
/// limits, and pitches/sizes in micrometers -- `requested_mm * 1000.0`,
/// rounded -- for the mm-denominated ones, so a non-finite input still has a
/// meaningful value to report instead of being unrepresentable).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimitError {
    /// Which limit was hit, e.g. `"grid_expansion"`, `"pitch"`, `"count"`.
    pub kind: &'static str,
    /// The limit's value.
    pub limit: u64,
    /// What was actually requested (saturates to `u64::MAX` if the request
    /// itself overflows representing it, e.g. a non-finite pitch).
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

/// A `Spec` struct's `validate()` refused its fields, and the matching
/// `try_*` builder refused to construct an `SdfNode` from them.
///
/// `#[non_exhaustive]`: a `Spec` may grow new validated fields with their
/// own reasons to refuse, without that being a breaking change to this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpecError {
    /// A named resource limit was exceeded; see [`ResourceLimitError`].
    ResourceLimit(ResourceLimitError),
}

impl From<ResourceLimitError> for SpecError {
    fn from(e: ResourceLimitError) -> Self {
        Self::ResourceLimit(e)
    }
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResourceLimit(e) => write!(f, "{e}"),
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
/// `requested` in the returned error is `v` in micrometers, rounded and
/// saturated to `u64` (so a `NaN`/infinite `v` still has a representable
/// value: `0` for `NaN` or a negative value, `u64::MAX` for `+inf`) -- `limit`
/// is `floor` in the same units.
///
/// # Errors
///
/// [`ResourceLimitError`] (`kind` as given) if `v` is not finite or is below
/// `floor`.
// ALLOW-UNWIRED: pending the pitch-class Spec sites (hex_hole_pitch etc.), landed separately
pub fn checked_positive_finite(
    v: f32,
    floor: f32,
    kind: &'static str,
) -> Result<f32, ResourceLimitError> {
    if !v.is_finite() || v < floor {
        let requested = if v.is_nan() || v < 0.0 {
            0
        } else if v.is_infinite() {
            u64::MAX
        } else {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                (f64::from(v) * 1000.0).round() as u64
            }
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let limit = (f64::from(floor) * 1000.0).round() as u64;
        return Err(ResourceLimitError {
            kind,
            limit,
            requested,
        });
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::{
        checked_positive_finite, checked_product, SpecError, MAX_NODE_EXPANSION, MIN_PITCH_MM,
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
    fn checked_positive_finite_zero_errs() {
        let e = checked_positive_finite(0.0, MIN_PITCH_MM, "pitch").unwrap_err();
        assert_eq!(e.kind, "pitch");
        assert_eq!(e.requested, 0);
    }

    #[test]
    fn checked_positive_finite_negative_errs() {
        let e = checked_positive_finite(-5.0, MIN_PITCH_MM, "pitch").unwrap_err();
        assert_eq!(e.requested, 0);
    }

    #[test]
    fn checked_positive_finite_nan_errs() {
        let e = checked_positive_finite(f32::NAN, MIN_PITCH_MM, "pitch").unwrap_err();
        assert_eq!(e.requested, 0);
    }

    #[test]
    fn checked_positive_finite_infinite_errs() {
        let e = checked_positive_finite(f32::INFINITY, MIN_PITCH_MM, "pitch").unwrap_err();
        assert_eq!(e.requested, u64::MAX);
    }

    #[test]
    fn checked_positive_finite_just_below_floor_errs() {
        let below = MIN_PITCH_MM - 0.001;
        assert!(checked_positive_finite(below, MIN_PITCH_MM, "pitch").is_err());
    }

    #[test]
    fn spec_error_from_resource_limit_error_displays_the_same_text() {
        let e = checked_product(1000, 1000, MAX_NODE_EXPANSION, "grid_expansion").unwrap_err();
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
}
