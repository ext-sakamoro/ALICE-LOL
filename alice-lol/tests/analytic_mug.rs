//! `mug_sdf` against its own specification: a 3 mm side wall, a 3 mm floor and an open top
//!
//! `SdfNode::Cylinder` stands along Y, so the mug's axis is Y (floor at y = -height/2, rim at
//! y = +height/2). Each probe is a point whose inside / outside follows from the specification
//! alone: inside the side wall in every horizontal direction, inside the floor, and empty in the
//! cavity right above the floor, right inside the wall on both sides, and just below the rim.
// the probes spell the specification's closed forms (a * b + c) as written; these tests are not under the bit rule of src/
#![allow(clippy::suboptimal_flops)]

use alice_lol::stdlib::products::mug_sdf;
use alice_sdf::eval::eval;
use glam::Vec3;

const DIA: f32 = 50.0;
const HEIGHT: f32 = 100.0;
const WALL: f32 = 3.0;
const FLOOR: f32 = 3.0;

fn probe(p: Vec3) -> f32 {
    eval(&mug_sdf(DIA, HEIGHT), p)
}

/// The side wall is 3 mm thick in every horizontal direction (the cavity is centred on the axis)
#[test]
fn the_side_wall_is_three_millimetres_all_round() {
    let r = DIA * 0.5;
    let mut compared = 0;
    // the handle sits on +X; probe the wall at -X, +Z and -Z, and the cavity in all four directions
    for dir in [Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
        let in_wall = dir * (r - WALL * 0.5);
        assert!(
            probe(in_wall) < 0.0,
            "the middle of the wall at {in_wall} is not material"
        );
        compared += 1;
    }
    for dir in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
        let in_cavity = dir * (r - WALL - 0.5);
        assert!(
            probe(in_cavity) > 0.0,
            "0.5 mm inside the wall at {in_cavity} is not empty"
        );
        compared += 1;
    }
    assert_eq!(compared, 7);
}

/// The floor is 3 mm thick at the bottom, and the cavity starts right above it
#[test]
fn the_floor_is_three_millimetres() {
    let bottom = -HEIGHT * 0.5;
    assert!(
        probe(Vec3::new(0.0, bottom + FLOOR * 0.5, 0.0)) < 0.0,
        "the middle of the floor is not material"
    );
    assert!(
        probe(Vec3::new(0.0, bottom + FLOOR + 0.5, 0.0)) > 0.0,
        "0.5 mm above the floor is not empty"
    );
}

/// The top is open: there is no lid below the rim
#[test]
fn the_top_is_open() {
    let rim = HEIGHT * 0.5;
    assert!(
        probe(Vec3::new(0.0, rim - 0.5, 0.0)) > 0.0,
        "0.5 mm below the rim on the axis is not empty (a lid)"
    );
    assert!(
        probe(Vec3::new(0.0, rim - 0.01, 0.0)) > 0.0,
        "the rim plane on the axis is not empty (a lid)"
    );
}
