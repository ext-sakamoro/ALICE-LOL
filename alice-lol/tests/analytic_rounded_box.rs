//! The LOL primitive `rounded_box(hx, hy, hz, r)`: hx / hy / hz are the outer half-extents (as for
//! `box3d`), and the corners are rounded by r inside that box
//!
//! Closed form: the faces lie exactly at ±h on each axis, and the box corner (hx, hy, hz) is
//! r(√3 - 1) outside the rounded surface. Emitting the parsed node gives back the same text.

use alice_lol::emit::to_lol;
use alice_lol::runtime_parser::parse_lol;
use alice_sdf::eval::eval;
use glam::Vec3;

#[test]
fn the_faces_of_rounded_box_are_at_the_given_half_extents() {
    let (hx, hy, hz, r) = (10.0_f32, 6.0_f32, 4.0_f32, 1.5_f32);
    let node = parse_lol(&format!("rounded_box({hx}, {hy}, {hz}, {r})")).expect("parse");
    let mut compared = 0;
    for (axis, h) in [(Vec3::X, hx), (Vec3::Y, hy), (Vec3::Z, hz)] {
        for sign in [1.0_f32, -1.0] {
            let face = axis * h * sign;
            assert!(
                eval(&node, face).abs() < 1e-4,
                "face {face}: {}",
                eval(&node, face)
            );
            assert!(
                eval(&node, face + axis * sign * 0.05) > 0.0,
                "grows past {face}"
            );
            assert!(
                eval(&node, face - axis * sign * 0.05) < 0.0,
                "shrinks inside {face}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 6);
    let want = r * (3.0_f32.sqrt() - 1.0);
    let corner = eval(&node, Vec3::new(hx, hy, hz));
    assert!((corner - want).abs() < 1e-3, "corner {corner}, want {want}");
}

/// The same numbers as box3d give the same outer size (only the corners differ)
#[test]
fn rounded_box_and_box3d_with_the_same_numbers_have_the_same_faces() {
    let rb = parse_lol("rounded_box(8, 5, 3, 1)").expect("parse");
    let bx = parse_lol("box3d(8, 5, 3)").expect("parse");
    for p in [
        Vec3::new(8.0, 0.0, 0.0),
        Vec3::new(0.0, 5.0, 0.0),
        Vec3::new(0.0, 0.0, -3.0),
    ] {
        assert!((eval(&rb, p) - eval(&bx, p)).abs() < 1e-4, "{p}");
    }
}

/// Emitting the parsed primitive writes the outer half-extents back
#[test]
fn rounded_box_emits_its_outer_half_extents() {
    let text = "rounded_box(10, 6, 4, 1.5)";
    let node = parse_lol(text).expect("parse");
    let out = to_lol(&node).expect("emit");
    assert!(out.contains("rounded_box(10.0, 6.0, 4.0, 1.5)"), "{out}");
}
