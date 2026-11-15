//! The tape roll fits the dispenser: its swept space is empty and the axle carries it clear of the base
//!
//! `tape_dispenser` holds a roll of inner diameter `inner_diameter`, outer diameter `outer_diameter` and
//! width `roll_width` on an axle along the roll's axis. The stated layout (Y-up, before the part is
//! turned onto the bed with Z up):
//! - the base plate is `wall_thickness` thick and spans `2·wall + outer_diameter/2 + roll_width` along Z;
//!   the back wall stands on its back edge (Z−)
//! - the roll's centre sits `wall + ROLL_CLEARANCE + outer_diameter/2` above the bed, so the roll's
//!   outer surface clears the base plate by `ROLL_CLEARANCE`
//! - the roll spans `roll_width` along Z, starting `ROLL_CLEARANCE` in front of the back wall's inner
//!   face; the axle (radius `inner_diameter/2 − 0.5`) runs from the back wall through the whole span
//! - the hood's underside is `ROLL_CLEARANCE` above the roll's top
//!
//! The oracle samples the roll's annulus (radius `inner/2 .. outer/2`) over its width and requires every
//! point to be empty (`d > 0`), measures the gap from the roll's lowest line to the base plate, and
//! requires the axle to be solid along the whole span and joined to the back wall. Every expected value
//! comes from the stated dimensions above, none from the generator.

use alice_lol::runtime_parser::parse_lol;
use alice_sdf::eval::eval;
use alice_sdf::SdfNode;
use glam::Vec3;

/// The free gap around the roll the layout states (mm)
const ROLL_CLEARANCE: f32 = 2.0;

/// (inner diameter, roll width, wall) as the parser takes them; the outer diameter is the parser's 150
const CASES: &[(f32, f32, f32)] = &[(76.0, 50.0, 3.0), (76.0, 48.0, 3.0), (25.0, 12.0, 4.0)];
const OUTER_DIAMETER: f32 = 150.0;

struct Layout {
    centre_y: f32,
    z0: f32,
    z1: f32,
    inner_r: f32,
    outer_r: f32,
    axle_r: f32,
    back_inner_z: f32,
    wall: f32,
}

fn layout(inner: f32, width: f32, wall: f32) -> Layout {
    let outer_r = OUTER_DIAMETER * 0.5;
    let base_hz = 0.5 * (2.0 * wall + outer_r + width);
    let back_inner_z = -base_hz + wall;
    let z0 = back_inner_z + ROLL_CLEARANCE;
    Layout {
        centre_y: wall + ROLL_CLEARANCE + outer_r,
        z0,
        z1: z0 + width,
        inner_r: inner * 0.5,
        outer_r,
        axle_r: inner * 0.5 - 0.5,
        back_inner_z,
        wall,
    }
}

/// The Y-up design point `(x, y, z)` in the part's frame (turned +90° about X: Y → Z, Z → −Y)
fn d(node: &SdfNode, x: f32, y: f32, z: f32) -> f32 {
    eval(node, Vec3::new(x, -z, y))
}

#[test]
fn the_rolls_swept_space_is_empty() {
    for &(inner, width, wall) in CASES {
        let lol = format!("tape_dispenser({inner}, {width}, {wall})");
        let node = parse_lol(&lol).unwrap_or_else(|e| panic!("{lol}: {e}"));
        let l = layout(inner, width, wall);
        let mut compared = 0usize;
        let mut worst = (f32::INFINITY, Vec3::ZERO);
        for iz in 0..=20 {
            let z = l.z0 + (l.z1 - l.z0) * iz as f32 / 20.0;
            for ir in 0..=8 {
                let r = l.inner_r + (l.outer_r - l.inner_r) * ir as f32 / 8.0;
                for ia in 0..72 {
                    let a = ia as f32 * std::f32::consts::TAU / 72.0;
                    let (s, c) = alice_det_math::sin_cos(a);
                    let (x, y) = (r * c, l.centre_y + r * s);
                    let v = d(&node, x, y, z);
                    compared += 1;
                    if v < worst.0 {
                        worst = (v, Vec3::new(x, y, z));
                    }
                }
            }
        }
        assert!(compared > 10_000, "{lol}: compared {compared}");
        assert!(
            worst.0 > 0.0,
            "{lol}: the roll's space holds material, d = {:.3} at {:?} (Y-up)",
            worst.0,
            worst.1
        );
    }
}

#[test]
fn the_roll_clears_the_base_plate_by_the_stated_gap() {
    for &(inner, width, wall) in CASES {
        let lol = format!("tape_dispenser({inner}, {width}, {wall})");
        let node = parse_lol(&lol).unwrap_or_else(|e| panic!("{lol}: {e}"));
        let l = layout(inner, width, wall);
        // the roll hangs on the axle, so its lowest line is the axle's measured centre minus the outer
        // radius: read the axle's solid run on the vertical through the middle of the span (above the
        // base plate) and the base plate's top on the same vertical
        let z = 0.5 * (l.z0 + l.z1);
        let step = 0.01_f32;
        // from just above the bed (the bottom face itself reads d = 0)
        let ys: Vec<f32> = (50..=40_000).map(|i| i as f32 * step).collect();
        let solid: Vec<bool> = ys.iter().map(|&y| d(&node, 0.0, y, z) <= 0.0).collect();
        let base_top = ys[solid.iter().position(|s| !s).expect("air above the base")];
        let axle_lo = ys
            .iter()
            .zip(&solid)
            .skip_while(|(&y, _)| y <= base_top)
            .find(|(_, &s)| s)
            .map(|(&y, _)| y)
            .unwrap_or_else(|| panic!("{lol}: no axle above the base in the middle of the span"));
        let axle_hi = ys
            .iter()
            .zip(&solid)
            .skip_while(|(&y, _)| y < axle_lo)
            .find(|(_, &s)| !s)
            .map(|(&y, _)| y)
            .expect("the axle ends");
        let centre = 0.5 * (axle_lo + axle_hi);
        let gap = centre - l.outer_r - base_top;
        assert!(
            (base_top - l.wall).abs() <= 0.02,
            "{lol}: base top at {base_top}, stated {}",
            l.wall
        );
        assert!(
            (gap - ROLL_CLEARANCE).abs() <= 0.03,
            "{lol}: the roll hung on the axle (centre {centre:.2}) clears the base by {gap:.3}, \
             stated {ROLL_CLEARANCE}"
        );
    }
}

#[test]
fn the_axle_is_solid_over_the_span_and_joins_the_back_wall() {
    for &(inner, width, wall) in CASES {
        let lol = format!("tape_dispenser({inner}, {width}, {wall})");
        let node = parse_lol(&lol).unwrap_or_else(|e| panic!("{lol}: {e}"));
        let l = layout(inner, width, wall);
        let mut compared = 0usize;
        // the axle's centre line from inside the back wall to the far end of the roll
        let from = l.back_inner_z - 0.5 * l.wall;
        for i in 0..=100 {
            let z = from + (l.z1 - from) * i as f32 / 100.0;
            let v = d(&node, 0.0, l.centre_y, z);
            compared += 1;
            assert!(v < 0.0, "{lol}: the axle line is empty at z = {z:.2} (d = {v:.3})");
        }
        // and its surface is at the stated radius in the middle of the span
        let zm = 0.5 * (l.z0 + l.z1);
        let surface = d(&node, l.axle_r, l.centre_y, zm);
        assert!(surface.abs() <= 0.02, "{lol}: axle surface d = {surface:.3}");
        assert_eq!(compared, 101);
    }
}

/// The tear edge's flat land on the hood's front face (mm): the bevel stops there instead of running to a
/// knife edge (a 0.4 mm nozzle cannot print below two perimeters, 0.8 mm, and the printability law flags
/// below three, 1.2 mm). Whether this edge tears tape well is not verified by a print
const TEAR_LAND: f32 = 1.2;
/// The parser's tear angle (degrees)
const TEAR_ANGLE_DEG: f32 = 30.0;

/// Near the front, the hood's solid run on a vertical is `min(wall, land + δ·tan θ)` at a distance `δ`
/// behind the front face: a flat land of `TEAR_LAND`, then the bevel at the tear angle up to the hood's top
#[test]
fn the_tear_edge_bevels_down_to_a_printable_land() {
    let tan_t = 30.0_f32.to_radians();
    let (s, c) = alice_det_math::sin_cos(tan_t);
    let tan_t = s / c;
    assert!((TEAR_ANGLE_DEG - 30.0).abs() < f32::EPSILON);
    for &(inner, width, wall) in CASES {
        let lol = format!("tape_dispenser({inner}, {width}, {wall})");
        let node = parse_lol(&lol).unwrap_or_else(|e| panic!("{lol}: {e}"));
        let l = layout(inner, width, wall);
        let hood_bottom = l.centre_y + l.outer_r + ROLL_CLEARANCE;
        let z_front = 0.5 * (2.0 * wall + l.outer_r + width);
        let land = TEAR_LAND.min(wall);
        let mut compared = 0usize;
        for delta in [0.05_f32, 0.5, 1.0, 1.5, 2.5, 4.0, 8.0] {
            let z = z_front - delta;
            // solid run upward from just below the hood's underside
            let start = hood_bottom - 0.5;
            let ys: Vec<f32> = (0..=1_500).map(|i| start + i as f32 * 0.005).collect();
            let first = ys
                .iter()
                .position(|&y| d(&node, 0.0, y, z) <= 0.0)
                .unwrap_or_else(|| panic!("{lol}: no hood at δ = {delta}"));
            let last = ys[first..]
                .iter()
                .position(|&y| d(&node, 0.0, y, z) > 0.0)
                .map_or(ys.len() - 1, |k| first + k - 1);
            let run = ys[last] - ys[first];
            let expected = wall.min(delta.mul_add(tan_t, land));
            compared += 1;
            assert!(
                (ys[first] - hood_bottom).abs() <= 0.02,
                "{lol}: hood underside at {} (stated {hood_bottom}) at δ = {delta}",
                ys[first]
            );
            assert!(
                (run - expected).abs() <= 0.03,
                "{lol}: hood run {run:.3} at δ = {delta}, closed form {expected:.3}"
            );
        }
        assert_eq!(compared, 7);
    }
}
