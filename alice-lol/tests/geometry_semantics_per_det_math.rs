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

/// the previous list is exactly the rows whose alice-det-math identifier alice-det-math itself
/// lists as reproducing the same bits (both directions)
#[test]
fn the_previous_list_is_the_rows_det_math_verifies_as_the_same_bits() {
    use alice_lol::geometry_id::PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS;
    assert!(
        !LOL_GEOMETRY_SEMANTICS_BY_DET_MATH.is_empty(),
        "the derivation scans no row"
    );
    let derived: Vec<[u8; 32]> = LOL_GEOMETRY_SEMANTICS_BY_DET_MATH
        .iter()
        .filter(|(d, _)| alice_det_math::PREVIOUS_SEMANTICS_IDS.contains(d))
        .map(|(_, g)| *g)
        .collect();
    let mut listed = PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS.to_vec();
    let mut want = derived;
    listed.sort_unstable();
    want.sort_unstable();
    assert_eq!(listed, want);
    assert!(!PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS.contains(&LOL_GEOMETRY_SEMANTICS_ID));
}

/// An identifier of an earlier alice-det-math generation (a row of another generation) is in
/// `PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS` only if a cross-version oracle covers it, and this build's own identifier never is
/// alice-det-math 0.5.2 at 2d01c63f lists no earlier generation as the same bits, so the geometry
/// identifier alice-lol 0.4.1 stamped (alice-det-math 0.5.0) is not a previous identifier
#[test]
fn an_earlier_generation_id_is_previous_only_with_a_cross_version_oracle() {
    use alice_lol::geometry_id::PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS;
    assert!(!PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS.contains(&LOL_GEOMETRY_SEMANTICS_ID));
    let earlier: Vec<[u8; 32]> = LOL_GEOMETRY_SEMANTICS_BY_DET_MATH
        .iter()
        .filter(|(d, _)| *d != alice_det_math::SEMANTICS_ID)
        .map(|(_, id)| *id)
        .collect();
    assert!(
        earlier.contains(&GEOMETRY_UNDER_0_5_0),
        "the identifier alice-lol 0.4.1 stamped is an earlier generation's"
    );
    let mut compared = 0;
    for id in &earlier {
        let covered = COVERED_BY_A_CROSS_VERSION_ORACLE.contains(id);
        assert!(
            covered || !PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS.contains(id),
            "an earlier generation's identifier is in PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS with no cross-version oracle"
        );
        compared += 1;
    }
    assert!(compared > 0, "no earlier identifier compared");
    for id in PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS {
        assert!(
            COVERED_BY_A_CROSS_VERSION_ORACLE.contains(id),
            "an identifier is in PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS with no cross-version oracle"
        );
    }
}

/// The earlier identifiers a cross-version oracle covers (an oracle that reads what that release
/// wrote and reproduces that release's own result bit for bit): none yet
const COVERED_BY_A_CROSS_VERSION_ORACLE: &[[u8; 32]] = &[];

/// the geometry identifier alice-lol 0.4.1 stamped (alice-det-math 0.5.0) is no longer an earlier
/// release with the same bits: it reads as a mismatch, the current one as verified
#[test]
fn the_0_4_1_geometry_identifier_is_not_previous() {
    use alice_det_math::SemanticsCheck;
    use alice_lol::geometry_id::check_lol_geometry_semantics;
    assert_eq!(
        check_lol_geometry_semantics(&GEOMETRY_UNDER_0_5_0),
        SemanticsCheck::Mismatch
    );
    assert_eq!(
        check_lol_geometry_semantics(&LOL_GEOMETRY_SEMANTICS_ID),
        SemanticsCheck::Verified
    );
    assert_eq!(
        check_lol_geometry_semantics(&[0_u8; 32]),
        SemanticsCheck::Mismatch
    );
    assert_eq!(
        check_lol_geometry_semantics(&DET_MATH_0_5_0),
        SemanticsCheck::Mismatch
    );
}
