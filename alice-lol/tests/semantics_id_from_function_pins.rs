//! `LOL_SEMANTICS_ID` and `LOL_GEOMETRY_SEMANTICS_ID` fold the alice-det-math functions this crate
//! calls, not alice-det-math's whole identifier
//!
//! `DET_MATH_FUNCTIONS` is the set of functions the crate calls (ALICE-Law's `fold_set_check.py`
//! compares it with the source), `DET_MATH_PINS` their identifiers (`alice_det_math::function_pin`,
//! checked at compile time by `assert_function_pins`), and both pin lists carry one
//! `alice-det-math::<function>` entry per pin. So an identifier moves when a called function's
//! behaviour moves, and stays when alice-det-math changes a function this crate does not call.
//!
//! oracle: the constants are the folds of their pins (recomputed here), the det-math entries are
//! exactly the called functions with their live pins, a change to one called function's pin moves
//! both folds, and the identifiers of the generations that folded the whole alice-det-math
//! identifier are not accepted (no cross-version oracle covers them)

use alice_det_math::SemanticsCheck;
use alice_lol::geometry_id::{
    check_lol_geometry_semantics, fold_geometry_pins, LOL_GEOMETRY_SEMANTICS_ID,
    LOL_GEOMETRY_SEMANTICS_PINS, PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS,
};
use alice_lol::law_id::{
    check_lol_semantics, fold_semantics_pins, hex32, DET_MATH_FUNCTIONS, DET_MATH_PINS,
    LOL_SEMANTICS_ID, LOL_SEMANTICS_PINS, PREVIOUS_LOL_SEMANTICS_IDS,
};

fn det_entries(pins: &[(&'static str, [u8; 32])]) -> Vec<(&'static str, [u8; 32])> {
    pins.iter()
        .filter_map(|(n, id)| n.strip_prefix("alice-det-math::").map(|f| (f, *id)))
        .collect()
}

#[test]
fn the_pins_are_the_called_functions_and_their_live_identifiers() {
    assert!(
        !DET_MATH_FUNCTIONS.is_empty(),
        "no called function: this compares nothing"
    );
    let names: Vec<&str> = DET_MATH_PINS.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names, DET_MATH_FUNCTIONS,
        "one pin per called function, in that order"
    );
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted, names, "sorted and without repeats");
    for (n, id) in DET_MATH_PINS {
        assert_eq!(*id, alice_det_math::function_pin(n), "{n}");
    }
}

#[test]
fn both_pin_lists_carry_exactly_the_function_pins() {
    let want: Vec<(&str, [u8; 32])> = DET_MATH_PINS.to_vec();
    assert_eq!(det_entries(LOL_SEMANTICS_PINS), want);
    assert_eq!(det_entries(LOL_GEOMETRY_SEMANTICS_PINS), want);
    for (n, id) in LOL_SEMANTICS_PINS.iter().chain(LOL_GEOMETRY_SEMANTICS_PINS) {
        assert_ne!(
            *id,
            alice_det_math::SEMANTICS_ID,
            "{n}: the whole det-math id is folded"
        );
        assert_ne!(*n, "alice-det-math", "the whole-identifier entry is gone");
    }
}

#[test]
fn the_constants_are_the_folds_of_their_pins() {
    assert_eq!(LOL_SEMANTICS_ID, fold_semantics_pins(LOL_SEMANTICS_PINS));
    assert_eq!(
        LOL_GEOMETRY_SEMANTICS_ID,
        fold_geometry_pins(LOL_GEOMETRY_SEMANTICS_PINS)
    );
    assert_ne!(LOL_SEMANTICS_ID, LOL_GEOMETRY_SEMANTICS_ID);
}

#[test]
fn a_change_to_one_called_function_moves_both_identifiers() {
    let mut moved = 0;
    for f in DET_MATH_FUNCTIONS {
        let key = format!("alice-det-math::{f}");
        let bump = |pins: &[(&'static str, [u8; 32])]| -> Vec<(&'static str, [u8; 32])> {
            pins.iter()
                .map(|&(n, mut id)| {
                    if n == key {
                        id[0] ^= 1;
                    }
                    (n, id)
                })
                .collect()
        };
        assert_ne!(
            fold_semantics_pins(&bump(LOL_SEMANTICS_PINS)),
            LOL_SEMANTICS_ID,
            "{f}"
        );
        assert_ne!(
            fold_geometry_pins(&bump(LOL_GEOMETRY_SEMANTICS_PINS)),
            LOL_GEOMETRY_SEMANTICS_ID,
            "{f}"
        );
        moved += 1;
    }
    assert_eq!(moved, DET_MATH_FUNCTIONS.len());
}

/// The identifiers of the generations that folded alice-det-math's whole identifier (0.5.0, 0.5.1,
/// 0.5.2 at 2d01c63f): no oracle reads what those builds wrote and compares it with their own
/// results, so they are not previous identifiers
#[test]
fn the_whole_identifier_generations_are_not_accepted() {
    let law = [
        "403359f60a227113988db2cc16d2c9b40a2cb3d725f06cebb6a57828ee2a8421",
        "d81c2cbbf7f010b7b3368c461d0a4ba60345fae302e9991fbd992d35dc3a7002",
        "c71fed08b4a6e4771cf5d375c5dbed1dfc8e368f034e7948a9b093eddc05cd10",
    ];
    let geometry = [
        "251e73bf84e00c4c1916184bdcbbb75e5cf3f74b62fd4adcbc3922dd18df907d",
        "d1cbba69a1b33ce7363ac178f16ad0cb062699ff8c9c76af7058c6d006bc8a24",
        "8ee0c1a391b51f78c66e4d6b9db593666a5922f4a706ffffe76e63f975ba48a7",
    ];
    for h in law {
        assert_eq!(
            check_lol_semantics(&hex32(h)),
            SemanticsCheck::Mismatch,
            "{h}"
        );
    }
    for h in geometry {
        assert_eq!(
            check_lol_geometry_semantics(&hex32(h)),
            SemanticsCheck::Mismatch,
            "{h}"
        );
    }
    assert_eq!(
        check_lol_semantics(&LOL_SEMANTICS_ID),
        SemanticsCheck::Verified
    );
    assert_eq!(
        check_lol_geometry_semantics(&LOL_GEOMETRY_SEMANTICS_ID),
        SemanticsCheck::Verified
    );
    assert!(
        PREVIOUS_LOL_SEMANTICS_IDS.is_empty() && PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS.is_empty()
    );
}
