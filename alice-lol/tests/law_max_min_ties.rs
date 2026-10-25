//! Bit-level tie tests of the Law max / min through the public API (ALICE-Law G2)
//!
//! Each test drives one class of library site with an equal pair of zeros of
//! different sign and with a NaN operand, and reads the resulting value back
//! bit for bit. The Law helpers (`det_math::max` / `min`, `det_vec`) return
//! +0 from max and -0 from min for an equal pair of zeros, in either order,
//! and ignore a NaN operand; the platform `f32::max` / `min` and glam's lane
//! forms leave both unspecified.
//!
//! NaN operands are driven with both sign bits (`POS_NAN` / `NEG_NAN`, built with
//! `from_bits`: the bits of `f32::NAN` and of its negation are not specified) and in the
//! first operand: without its NaN rule the max falls through to `!a.is_sign_negative()`,
//! which a positive NaN reaches, and the min to `a.is_sign_negative()`, which a negative
//! NaN reaches, so each sign catches one of the two.
//!
//! Mutants: dropping a NaN rule of `det_math::max` or of `det_math::min` (for the first or,
//! with the roblox trailing-NaN case in `law_max_min_ties_roblox.rs`, the second operand), or a mutant
//! that returns the first operand (or -0) for an equal pair of zeros, turns these tests
//! red. The one mutant they leave green is `det_math::max` returning the second operand `b`
//! on any equal pair (dropping `|| !a.is_sign_negative()`): it differs from the Law only for
//! `max(+0, -0)`, and every max site tested here gets its +0 operand second (a literal
//! `+0.0`, or `plate_thickness + c`, which is never -0), so that pair cannot occur.
//! The roblox export bounds are in `law_max_min_ties_roblox.rs` (feature `roblox`).
use alice_lol::stdlib::hardsurface::mount::skadis_peg_compat;
use alice_lol::stdlib::hardsurface::pattern_sdf::{
    drawer_organizer, l_bracket, t_slot_bracket_2020, DrawerSpec, LBracketSpec,
    TSlotBracket2020Spec,
};
use alice_lol::stdlib::hardsurface::reinforcement::honeycomb_infill;
use alice_lol::SdfNode;

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

/// A quiet NaN with the sign bit clear
const POS_NAN: f32 = f32::from_bits(0x7FC0_0000);
/// A quiet NaN with the sign bit set
const NEG_NAN: f32 = f32::from_bits(0xFFC0_0000);

/// Hex-cell radius floor (`reinforcement::honeycomb_infill`, `max(r - wall / 2, 0)`):
/// a -0 radius against the +0 floor is +0, and a NaN cell size falls to the floor
#[test]
fn honeycomb_cell_radius_floor_of_negative_zero_and_nan() {
    let container = SdfNode::sphere(10.0);
    // cell_size -0 and wall 0: the fused -0.5 * 0 + (-0) is -0, then max(-0, +0)
    let zero = honeycomb_infill(container.clone(), -0.0, 0.0, 1);
    assert_eq!(field_bits(&zero, "hex_radius"), vec![POS_ZERO]);
    // a NaN cell size: max(NaN, +0) is the floor, not NaN
    for nan in [POS_NAN, NEG_NAN] {
        let node = honeycomb_infill(container.clone(), nan, 1.0, 1);
        assert_eq!(field_bits(&node, "hex_radius"), vec![POS_ZERO]);
    }
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
    let thick = skadis_peg_compat(100.0);
    for nan in [POS_NAN, NEG_NAN] {
        let node = skadis_peg_compat(nan);
        assert_eq!(
            field_bits(&node, "round_radius"),
            field_bits(&thick, "round_radius")
        );
    }
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

fn debug(node: &SdfNode) -> String {
    format!("{node:?}")
}

/// L bracket arm floors (`pattern_sdf::l_bracket`, `max(arm, plate_thickness + 1.0)` on both
/// arms): with a plate of -1 the floor is +0, so a -0 arm is the pair (-0, +0), which gives
/// +0, the same node as a +0 arm; a NaN arm falls to the floor
#[test]
fn l_bracket_arm_floor_of_negative_zero_and_nan() {
    let spec = |arm: f32| LBracketSpec {
        arm_width: arm,
        arm_height: arm,
        plate_thickness: -1.0,
        ..LBracketSpec::m4_2holes()
    };
    let positive = debug(&l_bracket(&spec(0.0)));
    assert!(!positive.contains("-0.0"), "the +0 baseline carries no -0");
    assert_eq!(debug(&l_bracket(&spec(-0.0))), positive);
    assert_eq!(
        debug(&l_bracket(&spec(POS_NAN))),
        positive,
        "a positive NaN arm"
    );
    assert_eq!(
        debug(&l_bracket(&spec(NEG_NAN))),
        positive,
        "a negative NaN arm"
    );
}

/// T-slot bracket arm floor (`pattern_sdf::t_slot_bracket_2020`, `max(arm, plate + 5.0)`):
/// with a plate of -5 the floor is +0; a -0 arm gives the +0 node, a NaN arm the floor
#[test]
fn t_slot_bracket_arm_floor_of_negative_zero_and_nan() {
    let spec = |arm: f32| TSlotBracket2020Spec {
        arm_size: arm,
        plate_thickness: -5.0,
        ..TSlotBracket2020Spec::standard_20()
    };
    let positive = debug(&t_slot_bracket_2020(&spec(0.0)));
    assert_eq!(debug(&t_slot_bracket_2020(&spec(-0.0))), positive);
    assert_eq!(debug(&t_slot_bracket_2020(&spec(POS_NAN))), positive);
    assert_eq!(debug(&t_slot_bracket_2020(&spec(NEG_NAN))), positive);
}

/// Drawer slot depth (`pattern_sdf::drawer_organizer`, `min(min_depth, depth - 2 * wall)`):
/// with wall 0 the second operand is the drawer depth itself. (+0, -0) gives -0, the same
/// node as (-0, -0); (-0, +0) gives -0, which differs from (+0, +0); a NaN min depth falls to
/// the drawer depth, the same node as an infinite min depth
#[test]
fn drawer_slot_depth_min_of_mixed_zeros_and_nan() {
    let spec = |min_depth: f32, depth: f32| {
        let mut s = DrawerSpec::default_chopsticks_set();
        s.wall_thickness = 0.0;
        s.depth = depth;
        for slot in &mut s.slots {
            slot.min_depth = min_depth;
        }
        s
    };
    assert_eq!(
        debug(&drawer_organizer(&spec(0.0, -0.0))),
        debug(&drawer_organizer(&spec(-0.0, -0.0))),
        "min(+0, -0) is -0"
    );
    assert_ne!(
        debug(&drawer_organizer(&spec(-0.0, 0.0))),
        debug(&drawer_organizer(&spec(0.0, 0.0))),
        "min(-0, +0) is -0, not +0"
    );
    let finite = DrawerSpec::default_chopsticks_set();
    let with = |min_depth: f32| {
        let mut s = finite.clone();
        for slot in &mut s.slots {
            slot.min_depth = min_depth;
        }
        debug(&drawer_organizer(&s))
    };
    assert_eq!(
        with(POS_NAN),
        with(f32::INFINITY),
        "a positive NaN min depth (first operand) falls to the drawer depth"
    );
    assert_eq!(
        with(NEG_NAN),
        with(f32::INFINITY),
        "a negative NaN min depth (first operand) falls to the drawer depth"
    );
}

/// Density hatching (`laser_pattern::density_hatch`): the first line is at `d = proj_min`, the Law
/// min over the corner projections, and its y on the left edge is `(sin * x_min + d) / cos`
///
/// At angle 0 with `x = -1`, `y = -0` and a height of `+0`, the projections `-sin * cx + cos * cy`
/// are `+0, -0, +0, +0` (corner order `(x, y)`, `(x + w, y)`, `(x + w, y + h)`, `(x, y + h)`), so the
/// fold meets the equal pair in both orders (`min(+0, -0)`, then `min(-0, +0)`): the Law min is -0
/// either way, `d` is -0 and, with `sin * x_min = -0`, the line's y on the left edge is -0. The
/// platform min returns the second operand of an equal pair on x86_64, which gives +0 there.
/// A NaN projection cannot be the first operand (the fold starts from +inf and never holds a NaN),
/// and as the second operand the Law and the platform both return the other one; a NaN corner also
/// makes the clip find no edge, so no line is drawn: not a tie
#[test]
fn density_hatch_first_line_takes_the_law_min_of_mixed_zero_projections() {
    use alice_lol::laser_pattern::{density_hatch, Bounds, LaserElement};
    let lines = |b: Bounds| -> Vec<(u64, u64, u64, u64)> {
        density_hatch(1.0, 1.0, 0.0, &b, &|_, _| 0.5)
            .into_iter()
            .map(|e| match e {
                LaserElement::Line(x1, y1, x2, y2) => {
                    (x1.to_bits(), y1.to_bits(), x2.to_bits(), y2.to_bits())
                }
                other => panic!("not a line: {other:?}"),
            })
            .collect()
    };
    let neg_zero = (-0.0f64).to_bits();
    // the projections are +0, -0, +0, +0: the min is -0 in either order of the equal pair
    let got = lines(Bounds::new(-1.0, -0.0, 2.0, 0.0));
    assert!(!got.is_empty(), "the flat box gives its line");
    assert_eq!(
        got[0].1, neg_zero,
        "the first line's y on the left edge is the Law min: -0"
    );
    // on the right edge `sin * x_max` is +0, and +0 + -0 is +0: only the left edge shows the sign
    assert_eq!(got[0].3, 0.0f64.to_bits());
    // all projections +0: the min is +0 and so is the line's y
    let got = lines(Bounds::new(1.0, 0.0, 2.0, 0.0));
    assert_eq!(got[0].1, 0.0f64.to_bits());
}
