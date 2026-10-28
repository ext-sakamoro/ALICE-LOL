//! `LOL_SEMANTICS_ID` follows the alice-det-math it is built with
//!
//! The first pin of `LOL_SEMANTICS_PINS` is `alice_det_math::SEMANTICS_ID`, so the fold of the
//! pins moves when the resolved alice-det-math moves (0.5.0 → 0.5.1 added `max` / `min` and
//! changed its identifier). A single recorded constant then fails against one of them. The
//! identifier is therefore recorded once per alice-det-math generation, the constant is the
//! row of the linked one, and an identifier of a known earlier generation stays recognised.
//!
//! oracle: each row is checked against the definition (the fold of the pins with the
//! alice-det-math pin set to that row's identifier), which needs no second build; the 0.5.0
//! row is also checked against the values recorded before this change (literal)

use alice_lol::law_id::{
    fold_semantics_pins, hex32, known_semantics_id, LOL_SEMANTICS_BY_DET_MATH, LOL_SEMANTICS_ID,
    LOL_SEMANTICS_PINS,
};

/// the pins as they are under another alice-det-math identifier
fn pins_under(det_math: [u8; 32]) -> Vec<(&'static str, [u8; 32])> {
    let pins: Vec<_> = LOL_SEMANTICS_PINS
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
        LOL_SEMANTICS_BY_DET_MATH.len() >= 2,
        "the table compares nothing"
    );
    for (det_math, lol) in LOL_SEMANTICS_BY_DET_MATH {
        assert_eq!(fold_semantics_pins(&pins_under(*det_math)), *lol);
    }
}

#[test]
fn the_constant_is_the_row_of_the_linked_det_math() {
    let row = LOL_SEMANTICS_BY_DET_MATH
        .iter()
        .find(|(d, _)| *d == alice_det_math::SEMANTICS_ID)
        .expect("the linked alice-det-math has a row");
    assert_eq!(LOL_SEMANTICS_ID, row.1);
    assert_eq!(LOL_SEMANTICS_ID, fold_semantics_pins(LOL_SEMANTICS_PINS));
}

#[test]
fn the_rows_are_distinct() {
    for (i, a) in LOL_SEMANTICS_BY_DET_MATH.iter().enumerate() {
        for b in &LOL_SEMANTICS_BY_DET_MATH[i + 1..] {
            assert_ne!(a.0, b.0, "an alice-det-math identifier has two rows");
            assert_ne!(
                a.1, b.1,
                "two alice-det-math generations give the same LOL identifier"
            );
        }
    }
}

// recorded before the table existed: alice-det-math 0.5.0 and the LOL constant under it
const DET_MATH_0_5_0: [u8; 32] =
    hex32("135c407d4249c1c200e4d1be79a12e848ce5c355fc3a918da965849e697ef000");
const LOL_UNDER_0_5_0: [u8; 32] =
    hex32("403359f60a227113988db2cc16d2c9b40a2cb3d725f06cebb6a57828ee2a8421");

#[test]
fn the_0_5_0_row_is_the_identifier_recorded_before() {
    assert!(LOL_SEMANTICS_BY_DET_MATH.contains(&(DET_MATH_0_5_0, LOL_UNDER_0_5_0)));
}

#[test]
fn an_identifier_of_a_known_generation_is_recognised() {
    for (det_math, lol) in LOL_SEMANTICS_BY_DET_MATH {
        assert_eq!(known_semantics_id(lol), Some(*det_math));
    }
    assert_eq!(known_semantics_id(&LOL_UNDER_0_5_0), Some(DET_MATH_0_5_0));
    assert_eq!(known_semantics_id(&[0_u8; 32]), None);
    assert_eq!(
        known_semantics_id(&DET_MATH_0_5_0),
        None,
        "a det-math id is not a LOL id"
    );
}

/// the previous list is exactly the rows whose alice-det-math identifier alice-det-math itself
/// lists as reproducing the same bits (both directions: a row missing from the list or an
/// entry with no such row is red)
#[test]
fn the_previous_list_is_the_rows_det_math_verifies_as_the_same_bits() {
    use alice_lol::law_id::PREVIOUS_LOL_SEMANTICS_IDS;
    assert!(
        !LOL_SEMANTICS_BY_DET_MATH.is_empty(),
        "the derivation scans no row"
    );
    let derived: Vec<[u8; 32]> = LOL_SEMANTICS_BY_DET_MATH
        .iter()
        .filter(|(d, _)| alice_det_math::PREVIOUS_SEMANTICS_IDS.contains(d))
        .map(|(_, lol)| *lol)
        .collect();
    let mut listed = PREVIOUS_LOL_SEMANTICS_IDS.to_vec();
    let mut want = derived;
    listed.sort_unstable();
    want.sort_unstable();
    assert_eq!(listed, want);
    assert!(!PREVIOUS_LOL_SEMANTICS_IDS.contains(&LOL_SEMANTICS_ID));
}

/// An identifier of an earlier alice-det-math generation (a row of another generation) is in
/// `PREVIOUS_LOL_SEMANTICS_IDS` only if a cross-version oracle covers it, and this build's own identifier never is
/// alice-det-math 0.5.2 at 2d01c63f lists no earlier generation as the same bits, so the law
/// identifier alice-lol 0.4.1 stamped (alice-det-math 0.5.0) is not a previous identifier
#[test]
fn an_earlier_generation_id_is_previous_only_with_a_cross_version_oracle() {
    use alice_lol::law_id::PREVIOUS_LOL_SEMANTICS_IDS;
    assert!(!PREVIOUS_LOL_SEMANTICS_IDS.contains(&LOL_SEMANTICS_ID));
    let earlier: Vec<[u8; 32]> = LOL_SEMANTICS_BY_DET_MATH
        .iter()
        .filter(|(d, _)| *d != alice_det_math::SEMANTICS_ID)
        .map(|(_, id)| *id)
        .collect();
    assert!(
        earlier.contains(&LOL_UNDER_0_5_0),
        "the identifier alice-lol 0.4.1 stamped is an earlier generation's"
    );
    let mut compared = 0;
    for id in &earlier {
        let covered = COVERED_BY_A_CROSS_VERSION_ORACLE.contains(id);
        assert!(
            covered || !PREVIOUS_LOL_SEMANTICS_IDS.contains(id),
            "an earlier generation's identifier is in PREVIOUS_LOL_SEMANTICS_IDS with no cross-version oracle"
        );
        compared += 1;
    }
    assert!(compared > 0, "no earlier identifier compared");
    for id in PREVIOUS_LOL_SEMANTICS_IDS {
        assert!(
            COVERED_BY_A_CROSS_VERSION_ORACLE.contains(id),
            "an identifier is in PREVIOUS_LOL_SEMANTICS_IDS with no cross-version oracle"
        );
    }
}

/// The earlier identifiers a cross-version oracle covers (an oracle that reads what that release
/// wrote and reproduces that release's own result bit for bit): none yet
const COVERED_BY_A_CROSS_VERSION_ORACLE: &[[u8; 32]] = &[];

/// the law identifier alice-lol 0.4.1 stamped (alice-det-math 0.5.0) is no longer an earlier
/// release with the same bits: it reads as a mismatch, the current one as verified
#[test]
fn the_0_4_1_identifier_is_not_previous() {
    use alice_det_math::SemanticsCheck;
    use alice_lol::law_id::check_lol_semantics;
    assert_eq!(
        check_lol_semantics(&LOL_UNDER_0_5_0),
        SemanticsCheck::Mismatch
    );
    assert_eq!(
        check_lol_semantics(&LOL_SEMANTICS_ID),
        SemanticsCheck::Verified
    );
    assert_eq!(check_lol_semantics(&[0_u8; 32]), SemanticsCheck::Mismatch);
    assert_eq!(
        check_lol_semantics(&DET_MATH_0_5_0),
        SemanticsCheck::Mismatch
    );
}
