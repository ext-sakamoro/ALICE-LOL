//! `fastener::counterbore` keeps a floor under the head of at least `max(0.5 d, 1.2)` mm
//!
//! On a thick plate the bore is the head height + 0.5 mm deep (the head sinks 0.5 mm), exactly as
//! before. On a plate too thin for that, the bore stops where the floor begins and the head stands
//! proud by `head height - depth`.
// the probes spell the specification's closed forms (a * b + c) as written; these tests are not under the bit rule of src/; depth 0 is an exact value, compared exactly
#![allow(clippy::suboptimal_flops, clippy::manual_midpoint, clippy::float_cmp)]

use alice_lol::stdlib::hardsurface::cavity::subtract_through_counterbore;
use alice_lol::stdlib::hardsurface::fastener::{
    counterbore, counterbore_depth, counterbore_floor, screw_hole, MetricSize,
};
use alice_sdf::eval::eval;
use alice_sdf::SdfNode;
use glam::Vec3;
use std::sync::Arc;

/// An M4 counterbore in a 5 mm plate: 2.0 mm floor, 3.0 mm bore, head 1.0 mm proud
#[test]
fn a_thin_plate_keeps_the_floor_and_the_head_stands_proud() {
    let m = MetricSize::M4;
    let t = 5.0_f32;
    assert!((counterbore_floor(m) - 2.0).abs() < 1e-6);
    let depth = counterbore_depth(m, t);
    assert!((depth - 3.0).abs() < 1e-6, "depth {depth}");
    let proud = m.head_height_socket() - depth;
    assert!(
        (proud - 1.0).abs() < 1e-6,
        "the head stands {proud} mm proud (closed form 4 - 3 = 1)"
    );

    let plate = SdfNode::Box3d {
        half_extents: Vec3::new(30.0, t * 0.5, 30.0),
    };
    let part = subtract_through_counterbore(plate, m, t, 0.0, 0.0);
    // between the clearance hole (radius about 2.2 for M4) and the head's rim (radius 3.5)
    let r = 0.5 * (m.nominal_diameter() * 0.5 + m.head_diameter_socket() * 0.5) + 0.25;
    let bottom = -t * 0.5;
    assert!(
        eval(&part, Vec3::new(r, bottom + 1.0, 0.0)) < 0.0,
        "the middle of the floor is not material"
    );
    assert!(
        eval(&part, Vec3::new(r, bottom + 2.0 + 0.1, 0.0)) > 0.0,
        "0.1 mm above the floor is not the bore"
    );
}

/// On a plate thick enough for the head, the tree is exactly the former one (bore = head + 0.5)
#[test]
fn a_thick_plate_is_unchanged_bit_for_bit() {
    let m = MetricSize::M4;
    let t = 8.0_f32;
    let bore_depth = m.head_height_socket() + 0.5;
    assert_eq!(counterbore_depth(m, t).to_bits(), bore_depth.to_bits());
    let former = SdfNode::Union {
        a: Arc::new(screw_hole(m, t + 10.0)),
        b: Arc::new(SdfNode::Translate {
            child: Arc::new(SdfNode::Cylinder {
                radius: m.head_diameter_socket() * 0.5,
                half_height: bore_depth * 0.5,
            }),
            offset: Vec3::new(0.0, (t - bore_depth) * 0.5, 0.0),
        }),
    };
    assert_eq!(format!("{:?}", counterbore(m, t)), format!("{former:?}"));
}

/// A plate thinner than the floor gets no counterbore (depth 0), only the through hole
#[test]
fn a_plate_thinner_than_the_floor_gets_no_bore() {
    assert_eq!(counterbore_depth(MetricSize::M6, 2.5), 0.0);
}
