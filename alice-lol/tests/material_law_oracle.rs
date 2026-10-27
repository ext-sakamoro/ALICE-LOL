//! Material laws (`kind material`): reading, binding, the tiling check, the fields at a point and
//! the identifier
//!
//! The expected fields below are worked out by hand from the law text, spelled as the same f64
//! operations in the same order (no fused multiply-add), not computed by the code under test. The
//! tiling check is the closed form: a tile repeats when every cell period and every noise
//! frequency fits it a whole number of times.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
#![allow(clippy::suboptimal_flops)]

use alice_lol::law_id::{fold_semantics_pins, LOL_SEMANTICS_ID};
use alice_lol::material_law::{
    material_semantics_fingerprints, measured_material_semantics_id, BoundCell, MaterialError,
    MaterialLaw, MaterialPoint, MaterialSample, MATERIAL_PROBE_LAW, MATERIAL_SEMANTICS_ID,
    MATERIAL_SEMANTICS_PINS,
};
use alice_lol::proportion_law::PROPORTION_SEMANTICS_ID;

const PAVERS: &str = "law pavers
kind material
claim square pavers with a joint, a little per-cell colour and a fine grain
param size number = 0.25 range 0.1 0.5
param joint number = 0.008 range 0 0.02
param wear number = 0.3 range 0 1
let half = joint / 2
tile = (1, 1)
cell grid = (size, size, 0)
noise grain = (16, 0.002, 7)
noise blotch = (4, 0.5, 11)
normal_step = 0.001
field height = min(edge / half, 1) * 0.004 + grain
field albedo = (0.30 + 0.05 * cell0, 0.28 + 0.05 * cell1, 0.25 + 0.04 * cell2)
field roughness = max(0, min(1, 0.6 + wear * blotch * 0.5))
";

fn point(u: f64, v: f64, edge: f64, cell: [f64; 3], noise: &[f64]) -> MaterialPoint {
    MaterialPoint {
        u,
        v,
        edge,
        cell,
        noise: noise.to_vec(),
    }
}

#[test]
fn the_fields_follow_the_expressions() {
    let law = MaterialLaw::from_file(PAVERS).unwrap();
    assert_eq!(law.name(), "pavers");
    assert_eq!(law.noise_names().collect::<Vec<_>>(), ["grain", "blotch"]);
    let b = law.bind(&[]).unwrap();
    assert_eq!(b.values(), [0.25, 0.008, 0.3]);
    assert_eq!(b.tile(), [1.0, 1.0]);
    assert_eq!(
        b.cell(),
        Some(BoundCell::Grid {
            period: [0.25, 0.25],
            row_shift: 0.0
        })
    );
    let n: Vec<(&str, f64, f64, u32)> = b.noises().collect();
    assert_eq!(n, [("grain", 16.0, 0.002, 7), ("blotch", 4.0, 0.5, 11)]);
    assert_eq!(b.normal_step(), 0.001);

    // inside a cell: edge 0.003 m, half joint 0.004 ⇒ 0.75 of the bevel
    let s = b
        .sample(&point(0.1, 0.2, 0.003, [0.5, 0.25, 1.0], &[0.0005, -0.4]))
        .unwrap();
    let half = 0.008 / 2.0;
    assert_eq!(
        s,
        MaterialSample {
            height: (0.003_f64 / half).min(1.0) * 0.004 + 0.0005,
            albedo: [0.30 + 0.05 * 0.5, 0.28 + 0.05 * 0.25, 0.25 + 0.04 * 1.0],
            roughness: 0.0_f64.max(1.0_f64.min(0.6 + 0.3 * -0.4 * 0.5)),
        }
    );
    // far from an edge the bevel saturates
    let s = b
        .sample(&point(0.1, 0.1, 0.1, [0.0; 3], &[0.0, 0.0]))
        .unwrap();
    assert_eq!(s.height, 1.0 * 0.004 + 0.0);
    // other parameters
    let b = law.bind(&[("wear", 1.0), ("joint", 0.02)]).unwrap();
    let s = b
        .sample(&point(0.0, 0.0, 0.002, [0.0; 3], &[0.0, 0.5]))
        .unwrap();
    assert_eq!(s.height, (0.002_f64 / 0.01).min(1.0) * 0.004 + 0.0);
    assert_eq!(s.roughness, 0.0_f64.max(1.0_f64.min(0.6 + 1.0 * 0.5 * 0.5)));
}

#[test]
fn a_tile_must_hold_every_period_a_whole_number_of_times() {
    let law = MaterialLaw::from_file(PAVERS).unwrap();
    // 1 m / 0.2 = 5 cells: tiles; 1 m / 0.3 = 3.33: does not
    assert!(law.bind(&[("size", 0.2)]).is_ok());
    assert!(law.bind(&[("size", 0.125)]).is_ok());
    for size in [0.3, 0.24, 0.45] {
        assert!(
            matches!(
                law.bind(&[("size", size)]),
                Err(MaterialError::NotTiling { .. })
            ),
            "size {size}"
        );
    }
    // a noise frequency that does not fit the tile
    let off = PAVERS.replace(
        "noise grain = (16, 0.002, 7)",
        "noise grain = (16.5, 0.002, 7)",
    );
    let e = MaterialLaw::from_file(&off).unwrap_err();
    assert!(e.message.contains("tile"), "{e}");
    // a very large repeat count is refused: past 2^20 the ratio cannot tell k + 0.5 from k
    let fine = PAVERS
        .replace(
            "param wear number = 0.3 range 0 1",
            "param wear number = 0.3 range 0 1\nparam freq number = 16 range 1 10000000000000",
        )
        .replace(
            "noise grain = (16, 0.002, 7)",
            "noise grain = (freq, 0.002, 7)",
        );
    let law_f = MaterialLaw::from_file(&fine).unwrap();
    for f in [
        3.5,
        1e6 + 0.5,
        1e8 + 0.5,
        1e9 + 0.5,
        4e9 + 0.5,
        1e10 + 0.5,
        1e12 + 0.5,
        2e6,
        1e9,
    ] {
        assert!(
            matches!(
                law_f.bind(&[("freq", f)]),
                Err(MaterialError::NotTiling { .. })
            ),
            "frequency {f} must not bind"
        );
    }
    assert!(
        law_f.bind(&[("freq", 1_048_576.0)]).is_ok(),
        "2^20 cycles tile"
    );
    assert!(
        law_f.bind(&[("freq", 1_048_577.0)]).is_err(),
        "past 2^20 is refused"
    );
    // half a millionth of a cycle off still tiles, two millionths does not
    assert!(law_f.bind(&[("freq", 16.000_000_5)]).is_ok());
    assert!(law_f.bind(&[("freq", 16.000_002)]).is_err());
    // a brick row shift: 4 rows of 0.25 in 1 m with half a cell of shift ⇒ 2 cells: tiles
    let brick = PAVERS.replace(
        "cell grid = (size, size, 0)",
        "cell grid = (size, size, 0.5)",
    );
    assert!(MaterialLaw::from_file(&brick).unwrap().bind(&[]).is_ok());
    // a shift of 0.3 of a cell over 4 rows is 1.2 cells: does not tile
    let skew = PAVERS.replace(
        "cell grid = (size, size, 0)",
        "cell grid = (size, size, 0.3)",
    );
    assert!(MaterialLaw::from_file(&skew).is_err());
    // voronoi: 1 m / 0.25 = 4 cells per side
    let voro = PAVERS.replace("cell grid = (size, size, 0)", "cell voronoi = (size, 0.8)");
    let law = MaterialLaw::from_file(&voro).unwrap();
    assert_eq!(
        law.bind(&[]).unwrap().cell(),
        Some(BoundCell::Voronoi {
            period: 0.25,
            jitter: 0.8
        })
    );
    assert!(matches!(
        law.bind(&[("size", 0.3)]),
        Err(MaterialError::NotTiling { .. })
    ));
    // without a cell line there is no cell, edge and cell values are what the caller gives
    let plain = PAVERS.replace("cell grid = (size, size, 0)\n", "");
    assert_eq!(
        MaterialLaw::from_file(&plain)
            .unwrap()
            .bind(&[])
            .unwrap()
            .cell(),
        None
    );
}

#[test]
fn a_sample_outside_its_range_or_not_finite_names_the_field() {
    let law = MaterialLaw::from_file(PAVERS).unwrap();
    let b = law.bind(&[]).unwrap();
    // albedo above 1
    assert!(matches!(
        b.sample(&point(0.0, 0.0, 0.0, [20.0, 0.0, 0.0], &[0.0, 0.0])),
        Err(MaterialError::OutOfRange { field }) if field == "albedo"
    ));
    let rough = PAVERS.replace(
        "field roughness = max(0, min(1, 0.6 + wear * blotch * 0.5))",
        "field roughness = 0.6 + wear * blotch",
    );
    let b2law = MaterialLaw::from_file(&rough).unwrap();
    let b2 = b2law.bind(&[]).unwrap();
    assert!(matches!(
        b2.sample(&point(0.0, 0.0, 0.0, [0.0; 3], &[0.0, 2.0])),
        Err(MaterialError::OutOfRange { field }) if field == "roughness"
    ));
    // a non-finite input or result
    assert!(matches!(
        b.sample(&point(f64::NAN, 0.0, 0.0, [0.0; 3], &[0.0, 0.0])),
        Err(MaterialError::NonFinite { field }) if field == "u"
    ));
    assert!(matches!(
        b.sample(&point(0.0, 0.0, 0.0, [0.0; 3], &[0.0, f64::INFINITY])),
        Err(MaterialError::NonFinite { field }) if field == "blotch"
    ));
    // (the defaults are sampled at u = 0 when the law is read, so the division is by u - 1)
    let div = PAVERS.replace("+ grain", "+ grain / (u - 1)");
    let dlaw = MaterialLaw::from_file(&div).unwrap();
    assert!(matches!(
        dlaw.bind(&[]).unwrap().sample(&point(1.0, 0.0, 0.0, [0.0; 3], &[0.0, 0.0])),
        Err(MaterialError::NonFinite { field }) if field == "height"
    ));
    // the noise values must be one per declared noise
    assert!(matches!(
        b.sample(&point(0.0, 0.0, 0.0, [0.0; 3], &[0.0])),
        Err(MaterialError::NoiseCount {
            expected: 2,
            got: 1
        })
    ));
}

#[test]
fn parameters_are_named_declared_and_in_range() {
    let law = MaterialLaw::from_file(PAVERS).unwrap();
    let p = &law.params()[0];
    assert_eq!(
        (p.name.as_str(), p.default, p.lo, p.hi),
        ("size", 0.25, 0.1, 0.5)
    );
    assert!(
        matches!(law.bind(&[("width", 1.0)]), Err(MaterialError::UnknownParam(n)) if n == "width")
    );
    assert!(matches!(
        law.bind(&[("wear", 0.1), ("wear", 0.2)]),
        Err(MaterialError::RepeatedParam(n)) if n == "wear"
    ));
    for v in [-0.01, 1.01, f64::NAN, f64::INFINITY] {
        assert!(
            matches!(law.bind(&[("wear", v)]), Err(MaterialError::ParamOutOfRange { name }) if name == "wear"),
            "{v}"
        );
    }
}

#[test]
#[allow(clippy::too_many_lines)] // one row per refused file
fn files_that_do_not_read() {
    let cases: &[(&str, &str)] = &[
        ("", "empty"),
        ("law x\nkind proportion\n", "kind material"),
        ("law x\nkind material\n", "field height"),
        (
            &PAVERS.replace(
                "field albedo = (0.30 + 0.05 * cell0, 0.28 + 0.05 * cell1, 0.25 + 0.04 * cell2)\n",
                "",
            ),
            "field albedo",
        ),
        (
            &PAVERS.replace(
                "field roughness = max(0, min(1, 0.6 + wear * blotch * 0.5))\n",
                "",
            ),
            "field roughness",
        ),
        (&PAVERS.replace("tile = (1, 1)\n", ""), "tile"),
        (&PAVERS.replace("normal_step = 0.001\n", ""), "normal_step"),
        (
            &PAVERS.replace("tile = (1, 1)", "tile = (1, 1)\ntile = (2, 2)"),
            "twice",
        ),
        (
            &PAVERS.replace(
                "cell grid = (size, size, 0)",
                "cell grid = (size, size, 0)\ncell voronoi = (size, 0.5)",
            ),
            "twice",
        ),
        (
            &PAVERS.replace("cell grid = (size, size, 0)", "cell hex = (size, 0)"),
            "grid",
        ),
        (
            &PAVERS.replace("cell grid = (size, size, 0)", "cell grid = (size, size)"),
            "3",
        ),
        (
            &PAVERS.replace("noise blotch = (4, 0.5, 11)", "noise grain = (4, 0.5, 11)"),
            "declared twice",
        ),
        (
            &PAVERS.replace(
                "noise blotch = (4, 0.5, 11)",
                "noise blotch = (4, 0.5, 1.5)",
            ),
            "seed",
        ),
        (
            &PAVERS.replace("noise blotch = (4, 0.5, 11)", "noise blotch = (4, 0.5, -1)"),
            "seed",
        ),
        (
            &PAVERS.replace("noise blotch = (4, 0.5, 11)", "noise u = (4, 0.5, 11)"),
            "reserved",
        ),
        (
            &PAVERS.replace("let half = joint / 2", "let half = u / 2"),
            "u",
        ),
        (
            &PAVERS.replace("tile = (1, 1)", "tile = (0, 1)"),
            "positive",
        ),
        (
            &PAVERS.replace("tile = (1, 1)", "tile = (1, -1)"),
            "positive",
        ),
        (
            &PAVERS
                .replace("cell grid = (size, size, 0)", "cell grid = (0.5, size, 0)")
                .replace("tile = (1, 1)", "tile = (0.25, 1)"),
            "holds 0.5",
        ),
        (
            &PAVERS.replace("normal_step = 0.001", "normal_step = 0"),
            "normal_step",
        ),
        (
            &PAVERS.replace(
                "field height = min(edge / half, 1) * 0.004 + grain",
                "field height = nope + 1",
            ),
            "nope",
        ),
        (
            &PAVERS.replace(
                "param size number = 0.25 range 0.1 0.5",
                "param size number = 0.6 range 0.1 0.5",
            ),
            "range",
        ),
        (&PAVERS.replace("claim", "colour"), "unknown line"),
        ("law x\nkind material\n\u{7}\n", "control"),
    ];
    for (text, why) in cases {
        let e = MaterialLaw::from_file(text).unwrap_err();
        assert!(e.to_string().contains(why), "{why:?} not in {e}");
    }
    // the defaults must tile and give an in-range sample at the zero point
    let dark = PAVERS.replace("0.30 + 0.05 * cell0", "0 - 0.1");
    assert!(MaterialLaw::from_file(&dark)
        .unwrap_err()
        .to_string()
        .contains("default"));
    let many = format!("law x\nkind material\n{}", "# c\n".repeat(5000));
    assert!(MaterialLaw::from_file(&many)
        .unwrap_err()
        .to_string()
        .contains("lines"));
}

#[test]
fn the_identifier_names_the_law_not_its_spelling() {
    let a = MaterialLaw::from_file(PAVERS).unwrap();
    let respelled = PAVERS
        .replace("min(edge / half, 1) * 0.004", "min(edge/half,1)*0.004")
        .replace("claim square pavers", "claim other words")
        .replace("tile = (1, 1)", "tile = ( 1 , 1 )");
    let b = MaterialLaw::from_file(&format!("# a comment\n{respelled}")).unwrap();
    assert_eq!(a.law_id(), b.law_id());
    for (from, to) in [
        ("law pavers", "law pavers2"),
        ("range 0.1 0.5", "range 0.1 0.6"),
        ("tile = (1, 1)", "tile = (2, 2)"),
        (
            "cell grid = (size, size, 0)",
            "cell grid = (size, size, 0.5)",
        ),
        (
            "noise grain = (16, 0.002, 7)",
            "noise grain = (16, 0.002, 8)",
        ),
        ("normal_step = 0.001", "normal_step = 0.002"),
        ("0.6 + wear", "0.61 + wear"),
        ("0.25 + 0.04 * cell2", "0.25 + 0.04 * cell1"),
        (
            "noise blotch = (4, 0.5, 11)\n",
            "noise blotch = (4, 0.5, 11)\nnoise extra = (2, 0.1, 1)\n",
        ),
    ] {
        let c = MaterialLaw::from_file(&PAVERS.replace(from, to)).unwrap();
        assert_ne!(a.law_id(), c.law_id(), "{from} -> {to}");
    }
}

#[test]
fn the_material_semantics_are_their_own_and_measured() {
    assert_eq!(measured_material_semantics_id(), MATERIAL_SEMANTICS_ID);
    let measured = material_semantics_fingerprints();
    assert_eq!(measured.len(), MATERIAL_SEMANTICS_PINS.len());
    for ((name, got), (pin, want)) in measured.iter().zip(MATERIAL_SEMANTICS_PINS) {
        assert_eq!(name, pin);
        assert_eq!(got, want, "{name}");
    }
    assert_eq!(
        fold_semantics_pins(MATERIAL_SEMANTICS_PINS),
        MATERIAL_SEMANTICS_ID
    );
    assert_ne!(MATERIAL_SEMANTICS_ID, LOL_SEMANTICS_ID);
    assert_ne!(MATERIAL_SEMANTICS_ID, PROPORTION_SEMANTICS_ID);
    assert!(MaterialLaw::from_file(MATERIAL_PROBE_LAW).is_ok());
}
