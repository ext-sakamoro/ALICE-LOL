//! `warp = (<a>, <b>)` in a material law: the cell of a point is looked up at the point moved by
//! two of its noises, `(u + a, v + b)`, so stones get irregular outlines while the fields still
//! read the point's own `u`, `v`
//!
//! Reading, binding and the identifier are checked here against the law text by hand; where the
//! evaluator looks the cell up is checked by the surface evaluator's own oracle. The warp's noises
//! are ordinary noises, so the tile already holds their frequencies a whole number of times (a
//! warped pattern repeats with the tile).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alice_lol::material_law::{MaterialLaw, MATERIAL_SEMANTICS_PINS};

const BASE: &str = "law warped
kind material
claim stones with irregular outlines
param size number = 0.25 range 0.1 0.5
tile = (1, 1)
cell grid = (size, size, 0.5)
noise wu = (8, 0.01, 3)
noise wv = (8, 0.01, 4)
noise grain = (16, 0.002, 7)
WARP
normal_step = 0.001
field height = min(edge / 0.004, 1) * 0.004 + grain
field albedo = (0.3, 0.28, 0.25)
field roughness = 0.6
";

fn law(warp: &str) -> Result<MaterialLaw, String> {
    MaterialLaw::from_file(&BASE.replace("WARP", warp)).map_err(|e| e.to_string())
}

#[test]
fn a_warp_names_two_declared_noises() {
    let w = law("warp = (wu, wv)").unwrap();
    assert_eq!(w.bind(&[]).unwrap().warp(), Some([0, 1]));
    // the same noise twice and either order read: the indices are the declared places
    assert_eq!(
        law("warp = (grain, wu)").unwrap().bind(&[]).unwrap().warp(),
        Some([2, 0])
    );
    assert_eq!(
        law("warp = (wv, wv)").unwrap().bind(&[]).unwrap().warp(),
        Some([1, 1])
    );
    // no warp line: the cell is looked up at the point itself
    assert_eq!(law("").unwrap().bind(&[]).unwrap().warp(), None);
    // the line may come before the noises it names
    let early = BASE
        .replace("WARP\n", "")
        .replace("tile = (1, 1)", "tile = (1, 1)\nwarp = (wu, wv)");
    assert_eq!(
        MaterialLaw::from_file(&early)
            .unwrap()
            .bind(&[])
            .unwrap()
            .warp(),
        Some([0, 1])
    );
}

#[test]
fn a_warp_that_does_not_name_two_noises_does_not_read() {
    for (bad, why) in [
        ("warp = (wu, nope)", "an undeclared name"),
        ("warp = (size, wv)", "a parameter, not a noise"),
        ("warp = (u, wv)", "a reserved input, not a noise"),
        ("warp = (wu)", "one name"),
        ("warp = (wu, wv, grain)", "three names"),
        ("warp = (wu + 1, wv)", "an expression, not a name"),
        ("warp = wu, wv", "no parentheses"),
        ("warp (wu, wv)", "no `=`"),
        ("warp = (wu, wv)\nwarp = (wu, wv)", "given twice"),
    ] {
        assert!(law(bad).is_err(), "{why}: `{bad}` read");
    }
    // a warp moves the cell lookup: without a cell line it means nothing
    let no_cell = BASE
        .replace("cell grid = (size, size, 0.5)\n", "")
        .replace("WARP", "warp = (wu, wv)");
    let no_cell = no_cell.replace("min(edge / 0.004, 1) * 0.004 + grain", "grain");
    assert!(
        MaterialLaw::from_file(&no_cell).is_err(),
        "a warp without a cell line read"
    );
}

#[test]
fn the_warp_is_in_the_identifier() {
    let plain = law("").unwrap().law_id();
    let w = law("warp = (wu, wv)").unwrap().law_id();
    let swapped = law("warp = (wv, wu)").unwrap().law_id();
    let spaced = law("warp   =  ( wu ,wv )").unwrap().law_id();
    assert_ne!(plain, w, "the warp changes where cells fall, so the law");
    assert_ne!(
        w, swapped,
        "which noise moves u and which moves v is part of the law"
    );
    assert_eq!(w, spaced, "spelling and spacing are not");
}

#[test]
fn the_semantics_pins_name_the_warp() {
    assert!(
        MATERIAL_SEMANTICS_PINS
            .iter()
            .any(|(text, _)| text.contains("warp")),
        "the meaning of a warp (where the cell is looked up) is pinned"
    );
}
