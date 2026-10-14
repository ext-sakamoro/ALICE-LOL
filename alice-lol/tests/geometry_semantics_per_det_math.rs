//! `LOL_GEOMETRY_SEMANTICS_ID` follows the alice-det-math it is built with
//!
//! The same pattern as `LOL_SEMANTICS_ID` (`tests/semantics_id_per_det_math.rs`): the first
//! pin of `LOL_GEOMETRY_SEMANTICS_PINS` is `alice_det_math::SEMANTICS_ID`, so one recorded
//! constant fails under one of two alice-det-math generations. The identifier is recorded per
//! generation, the constant is the row of the linked one, and an earlier generation's
//! identifier stays recognised.
//!
//! oracle: each row is the fold of the pins with the alice-det-math pin set to that row's
//! identifier (no second build needed); the 0.5.0 row is the value recorded before (literal)

use alice_lol::geometry_id::{
    fold_geometry_pins, known_geometry_semantics_id, LOL_GEOMETRY_SEMANTICS_BY_DET_MATH,
    LOL_GEOMETRY_SEMANTICS_ID, LOL_GEOMETRY_SEMANTICS_PINS,
};
use alice_lol::law_id::hex32;

fn pins_under(det_math: [u8; 32]) -> Vec<(&'static str, [u8; 32])> {
    let pins: Vec<_> = LOL_GEOMETRY_SEMANTICS_PINS
        .iter()
        .map(|&(name, id)| {
            if name == "alice-det-math" {
                (name, det_math)
            } else {
                (name, id)
            }
        })
        .collect();
    assert_eq!(
        pins.iter().filter(|(n, _)| *n == "alice-det-math").count(),
        1,
        "the alice-det-math pin is there exactly once"
    );
    pins
}

#[test]
fn every_row_is_the_fold_of_the_pins_under_its_det_math() {
    assert!(
        LOL_GEOMETRY_SEMANTICS_BY_DET_MATH.len() >= 2,
        "the table compares nothing"
    );
    for (det_math, geometry) in LOL_GEOMETRY_SEMANTICS_BY_DET_MATH {
        assert_eq!(fold_geometry_pins(&pins_under(*det_math)), *geometry);
    }
}

#[test]
fn the_constant_is_the_row_of_the_linked_det_math() {
    let row = LOL_GEOMETRY_SEMANTICS_BY_DET_MATH
        .iter()
        .find(|(d, _)| *d == alice_det_math::SEMANTICS_ID)
        .expect("the linked alice-det-math has a row");
    assert_eq!(LOL_GEOMETRY_SEMANTICS_ID, row.1);
}

#[test]
fn the_rows_are_distinct() {
    for (i, a) in LOL_GEOMETRY_SEMANTICS_BY_DET_MATH.iter().enumerate() {
        for b in &LOL_GEOMETRY_SEMANTICS_BY_DET_MATH[i + 1..] {
            assert_ne!(a.0, b.0);
            assert_ne!(a.1, b.1);
        }
    }
}

const DET_MATH_0_5_0: [u8; 32] =
    hex32("135c407d4249c1c200e4d1be79a12e848ce5c355fc3a918da965849e697ef000");
const GEOMETRY_UNDER_0_5_0: [u8; 32] =
    hex32("251e73bf84e00c4c1916184bdcbbb75e5cf3f74b62fd4adcbc3922dd18df907d");

#[test]
fn the_0_5_0_row_is_the_identifier_recorded_before() {
    assert!(LOL_GEOMETRY_SEMANTICS_BY_DET_MATH.contains(&(DET_MATH_0_5_0, GEOMETRY_UNDER_0_5_0)));
}

#[test]
fn an_identifier_of_a_known_generation_is_recognised() {
    for (det_math, geometry) in LOL_GEOMETRY_SEMANTICS_BY_DET_MATH {
        assert_eq!(known_geometry_semantics_id(geometry), Some(*det_math));
    }
    assert_eq!(known_geometry_semantics_id(&[0_u8; 32]), None);
}
