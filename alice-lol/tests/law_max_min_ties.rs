//! Bit-level tie tests of the Law max / min through the public API (ALICE-Law G2)
//!
//! Each test drives one class of library site with an equal pair of zeros of
//! different sign and with a NaN operand, and reads the resulting value back
//! bit for bit. The Law helpers (`det_math::max` / `min`, `det_vec`) return
//! +0 from max and -0 from min for an equal pair of zeros, in either order,
//! and ignore a NaN operand; the platform `f32::max` / `min` and glam's lane
//! forms leave both unspecified.
//!
//! Equivalent mutant (recorded, not tested here): removing the zero-sign rule of
//! `det_math::max` changes only `max(+0, -0)`. The stdlib floors take the literal
//! `+0.0` as their second operand, so that pair cannot occur there, and the export
//! bounds are `max - min`, which erases the sign unless max gives -0 and min gives +0
//! at the same time (the platform forms can do that on `x86_64`; the Law helpers
//! never do). On an `aarch64` host the mutant therefore stays green.

use alice_lol::roblox_export::{validate_for_roblox, RobloxConfig};
use alice_lol::stdlib::hardsurface::mount::skadis_peg_compat;
use alice_lol::stdlib::hardsurface::reinforcement::honeycomb_infill;
use alice_lol::SdfNode;
use alice_sdf::mesh::{Mesh, Vertex};
use glam::Vec3;

/// The `(field, value)` pairs printed for `field` in the node's debug form, as bits
fn field_bits(node: &SdfNode, field: &str) -> Vec<u32> {
    let text = format!("{node:?}");
    let key = format!("{field}: ");
    let mut out = Vec::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find(&key) {
        rest = &rest[at + key.len()..];
        let end = rest.find([',', ' ', '}']).unwrap_or(rest.len());
        let value: f32 = rest[..end]
            .parse()
            .unwrap_or_else(|_| panic!("{field}: not a float: {}", &rest[..end]));
        out.push(value.to_bits());
    }
    assert!(!out.is_empty(), "no `{field}` in {text}");
    out
}

const POS_ZERO: u32 = 0x0000_0000;

/// Hex-cell radius floor (`reinforcement::honeycomb_infill`, `max(r - wall / 2, 0)`):
/// a -0 radius against the +0 floor is +0, and a NaN cell size falls to the floor
#[test]
fn honeycomb_cell_radius_floor_of_negative_zero_and_nan() {
    let container = SdfNode::sphere(10.0);
    // cell_size -0 and wall 0: the fused -0.5 * 0 + (-0) is -0, then max(-0, +0)
    let zero = honeycomb_infill(container.clone(), -0.0, 0.0, 1);
    assert_eq!(field_bits(&zero, "hex_radius"), vec![POS_ZERO]);
    // a NaN cell size: max(NaN, +0) is the floor, not NaN
    let nan = honeycomb_infill(container, f32::NAN, 1.0, 1);
    assert_eq!(field_bits(&nan, "hex_radius"), vec![POS_ZERO]);
}

/// Peg round radius (`mount::skadis_peg_compat`, `max(min(R, min_element(half)), 0)`):
/// a -0 board thickness gives a -0 half extent; `min_element` and min carry -0
/// and the floor max(-0, +0) gives +0; a NaN thickness is skipped by the fold
#[test]
fn peg_round_radius_with_negative_zero_and_nan_thickness() {
    let zero = skadis_peg_compat(-0.0);
    assert_eq!(field_bits(&zero, "round_radius"), vec![POS_ZERO]);
    let positive = skadis_peg_compat(0.0);
    assert_eq!(field_bits(&positive, "round_radius"), vec![POS_ZERO]);
    // NaN thickness: min_element(x, y, NaN) is min(x, y), so the radius is the
    // one the two finite half extents give, the same as for a thick board
    let nan = skadis_peg_compat(f32::NAN);
    let thick = skadis_peg_compat(100.0);
    assert_eq!(
        field_bits(&nan, "round_radius"),
        field_bits(&thick, "round_radius")
    );
}

fn mesh_of(points: &[Vec3]) -> Mesh {
    Mesh {
        vertices: points.iter().map(|&p| Vertex::new(p, Vec3::Y)).collect(),
        indices: Vec::new(),
    }
}

/// Bounding size of an export (`roblox_export::validate_for_roblox`, lane-wise
/// `det_vec` min / max): a +0 / -0 pair of vertices in either order gives a +0
/// extent, and a NaN coordinate is skipped by the lane fold
#[test]
fn roblox_bounds_of_mixed_zeros_in_both_orders_and_a_nan_vertex() {
    let config = RobloxConfig::default();
    let a = validate_for_roblox(
        &mesh_of(&[Vec3::new(0.0, 0.0, 0.0), Vec3::new(-0.0, -0.0, -0.0)]),
        &config,
    );
    let b = validate_for_roblox(
        &mesh_of(&[Vec3::new(-0.0, -0.0, -0.0), Vec3::new(0.0, 0.0, 0.0)]),
        &config,
    );
    for v in [a.bounds_studs, b.bounds_studs] {
        assert_eq!([v.x.to_bits(), v.y.to_bits(), v.z.to_bits()], [POS_ZERO; 3]);
    }
    let with_nan = validate_for_roblox(
        &mesh_of(&[
            Vec3::new(f32::NAN, 1.0, 1.0),
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(3.0, 1.0, 1.0),
        ]),
        &config,
    );
    let finite = validate_for_roblox(
        &mesh_of(&[Vec3::new(1.0, 2.0, 3.0), Vec3::new(3.0, 1.0, 1.0)]),
        &config,
    );
    let (n, f) = (with_nan.bounds_studs, finite.bounds_studs);
    assert_eq!(
        [n.x.to_bits(), n.y.to_bits(), n.z.to_bits()],
        [f.x.to_bits(), f.y.to_bits(), f.z.to_bits()]
    );
}

/// Runtime parser count (`runtime_parser.rs` `count`, `max(v.round(), 1.0)` then a cast
/// to `u32`): the floor is the literal 1.0, so no equal pair of zeros can reach the max,
/// and a non-finite count is refused before it (so no NaN operand either); a -0 count
/// becomes the floor 1, the same node as an explicit 1
#[test]
fn parser_count_of_negative_zero_is_the_floor_and_non_finite_is_refused() {
    let zero =
        alice_lol::runtime_parser::parse_lol("token_well(10, 5, -0)").expect("-0 count parses");
    let one = alice_lol::runtime_parser::parse_lol("token_well(10, 5, 1)").expect("1 count parses");
    assert_eq!(format!("{zero:?}"), format!("{one:?}"));
    assert!(
        alice_lol::runtime_parser::parse_lol("token_well(10, 5, 1e39)").is_err(),
        "an infinite count is refused"
    );
}
