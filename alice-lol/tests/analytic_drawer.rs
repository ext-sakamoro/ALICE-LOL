//! `drawer_organizer` against its own specification (the default chopsticks set: 250 x 200 x 40,
//! walls 1.5, floor 1.5, dividers 1.2)
//!
//! The tray's outer size, the floor and every wall across the slots follow from the spec alone.
//! The walls are found by scanning the field across the tray at mid-depth and mid-height: every
//! material run (the two outer walls and each divider) must be at least as thick as the spec says.
// the probes spell the specification's closed forms (a * b + c) as written; these tests are not under the bit rule of src/; the scan casts a positive span to a step count
#![allow(
    clippy::suboptimal_flops,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use alice_lol::stdlib::hardsurface::pattern_sdf::{drawer_organizer, DrawerSpec};
use alice_sdf::eval::eval;
use glam::Vec3;

fn spec() -> DrawerSpec {
    DrawerSpec::default_chopsticks_set()
}

/// The outer size is the spec's width / depth / height (not grown by the fillet radius)
#[test]
fn the_tray_has_the_specified_outer_size() {
    let s = spec();
    let tray = drawer_organizer(&s);
    let (hx, hy, hz) = (s.width * 0.5, s.depth * 0.5, s.height * 0.5);
    // inside the outer wall, half a wall in from each face, and just outside each face
    assert!(
        eval(&tray, Vec3::new(hx - s.wall_thickness * 0.5, 0.0, 0.0)) < 0.0,
        "x wall"
    );
    assert!(
        eval(&tray, Vec3::new(hx + 0.2, 0.0, 0.0)) > 0.0,
        "the tray reaches past x = {hx}"
    );
    assert!(
        eval(&tray, Vec3::new(0.0, hy + 0.2, 0.0)) > 0.0,
        "the tray reaches past y = {hy}"
    );
    assert!(
        eval(&tray, Vec3::new(0.0, 0.0, -hz - 0.2)) > 0.0,
        "the tray reaches below z = -{hz}"
    );
}

/// The floor is the spec's floor thickness, and the slots are empty right above it
#[test]
fn the_floor_has_the_specified_thickness() {
    let s = spec();
    let tray = drawer_organizer(&s);
    let bottom = -s.height * 0.5;
    // at the centre of the tray (inside a slot or a divider): the floor's middle is material
    assert!(
        eval(&tray, Vec3::new(0.0, 0.0, bottom + s.floor_thickness * 0.5)) < 0.0,
        "floor"
    );
    // somewhere above the floor across the slots there is empty space 0.3 mm above the floor
    let z = bottom + s.floor_thickness + 0.3;
    let empty = (0..2500).any(|i| {
        #[allow(clippy::cast_precision_loss)]
        let x = -s.width * 0.5 + 0.1 * i as f32;
        eval(&tray, Vec3::new(x, 0.0, z)) > 0.0
    });
    assert!(
        empty,
        "no slot is open 0.3 mm above the floor (the floor is thicker than specified)"
    );
}

/// Every wall across the slots (outer walls and dividers) is at least as thick as specified
#[test]
fn every_wall_across_the_slots_has_the_specified_thickness() {
    let s = spec();
    let tray = drawer_organizer(&s);
    let step = 0.01_f32;
    let n = (s.width / step) as usize + 40;
    let mut runs: Vec<f32> = Vec::new();
    let mut run = 0.0_f32;
    for i in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let x = -s.width * 0.5 - 0.2 + step * i as f32;
        if eval(&tray, Vec3::new(x, 0.0, 0.0)) < 0.0 {
            run += step;
        } else if run > 0.0 {
            runs.push(run);
            run = 0.0;
        }
    }
    let slots: u32 = s.slots.iter().map(|sl| sl.count).sum();
    assert!(
        runs.len() >= 2,
        "fewer than two walls found (compared nothing): {runs:?}"
    );
    assert_eq!(
        u32::try_from(runs.len()).expect("wall count fits u32"),
        slots + 1,
        "walls {runs:?}"
    );
    let thinnest = runs.iter().copied().fold(f32::INFINITY, f32::min);
    let want = s.divider_thickness.min(s.wall_thickness) - 0.05;
    assert!(
        thinnest >= want,
        "the thinnest wall is {thinnest} mm (spec {want}+): {runs:?}"
    );
}
