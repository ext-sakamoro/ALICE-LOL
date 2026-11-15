//! `esp32_enclosure`: a USB opening wider than its wall face is refused (typed error), not cut
//!
//! The opening sits in the +X wall; the face between the two side walls is the cavity width
//! `pcb_depth + 2 × pcb_clearance`. An opening up to that width is cut (and notched through the rim
//! when the lip above it would be thinner than the wall); a wider one would remove the whole wall,
//! so `try_esp32_enclosure` (and the LOL construct) return `SpecError::FloatLimit` with the
//! requested and the allowed width instead of building a shape.

use alice_lol::limits::SpecError;
use alice_lol::runtime_parser::parse_lol;
use alice_lol::stdlib::hardsurface::pattern_sdf::{try_esp32_enclosure, Esp32EnclosureSpec};

const fn spec_with_opening(width: f32) -> Esp32EnclosureSpec {
    Esp32EnclosureSpec {
        usb_opening_width: width,
        ..Esp32EnclosureSpec::esp32_devkit_v1()
    }
}

fn face(s: &Esp32EnclosureSpec) -> f32 {
    s.pcb_depth + s.pcb_clearance + s.pcb_clearance
}

/// Just inside the boundary (face - 0.01 mm) builds; just outside (face + 0.01 mm) is refused with
/// the requested and the allowed width
#[test]
fn the_opening_is_cut_up_to_the_face_width_and_refused_beyond_it() {
    let base = Esp32EnclosureSpec::esp32_devkit_v1();
    let f = face(&base);
    assert!(try_esp32_enclosure(&spec_with_opening(f - 0.01)).is_ok());
    match try_esp32_enclosure(&spec_with_opening(f + 0.01)) {
        Err(SpecError::FloatLimit(e)) => {
            assert_eq!(e.kind, "usb_opening_width");
            assert!((e.limit - f).abs() < 1e-5, "limit {} (face {f})", e.limit);
            assert!((e.value - (f + 0.01)).abs() < 1e-5, "value {}", e.value);
        }
        other => panic!("an opening wider than the face built or failed otherwise: {other:?}"),
    }
    assert!(try_esp32_enclosure(&spec_with_opening(f32::NAN)).is_err());
}

/// The LOL construct goes through the guard: a board narrower than the default 9 mm opening (the
/// grammar corpus's `esp32_enclosure(3, 2, 1)`: face 3 mm) is a parse error naming the limit, a real
/// board (the bundled template's 45 x 18) parses
#[test]
fn the_lol_construct_refuses_an_opening_wider_than_its_face() {
    let e = parse_lol("esp32_enclosure(3, 2, 1)").expect_err("a 9 mm opening on a 3 mm face built");
    assert!(e.to_string().contains("usb_opening_width"), "{e}");
    assert!(parse_lol("esp32_enclosure(45, 18, 12)").is_ok());
}
