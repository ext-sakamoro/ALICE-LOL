//! `point <name> = <expr>` in a material law: a named value at the point, read by later point
//! lines and by the fields. It reads what a field reads (`u`, `v`, `edge`, `cell0` .. `cell2`,
//! the noises, the parameters and lets) and the point lines before it.
//!
//! The meaning is exact substitution: a law with point lines samples bit for bit as the same law
//! with each point's expression written out where it is read (the expected values below come from
//! that written-out law, not from the point lines), so laws without point lines evaluate as before.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use alice_lol::material_law::{MaterialError, MaterialLaw, MaterialPoint};

const NAMED: &str = "law steps
kind material
param jw number = 0.01 range 0.001 0.1
tile = (1, 1)
cell grid = (0.25, 0.25, 0.5)
noise grain = (16, 0.002, 7)
point s = min(max(edge / jw, 0), 1)
point t = s * s * (3 - 2 * s)
normal_step = 0.001
field height = 0.004 * t + grain
field albedo = (0.2 + 0.1 * t, 0.3 * t + 0.1 * cell1, 0.25)
field roughness = 0.8 - 0.3 * t
";

const WRITTEN_OUT: &str = "law steps
kind material
param jw number = 0.01 range 0.001 0.1
tile = (1, 1)
cell grid = (0.25, 0.25, 0.5)
noise grain = (16, 0.002, 7)
normal_step = 0.001
field height = 0.004 * (min(max(edge / jw, 0), 1) * min(max(edge / jw, 0), 1) * (3 - 2 * min(max(edge / jw, 0), 1))) + grain
field albedo = (0.2 + 0.1 * (min(max(edge / jw, 0), 1) * min(max(edge / jw, 0), 1) * (3 - 2 * min(max(edge / jw, 0), 1))), 0.3 * (min(max(edge / jw, 0), 1) * min(max(edge / jw, 0), 1) * (3 - 2 * min(max(edge / jw, 0), 1))) + 0.1 * cell1, 0.25)
field roughness = 0.8 - 0.3 * (min(max(edge / jw, 0), 1) * min(max(edge / jw, 0), 1) * (3 - 2 * min(max(edge / jw, 0), 1)))
";

fn point(edge: f64, c1: f64, grain: f64) -> MaterialPoint {
    MaterialPoint {
        u: 0.3,
        v: 0.7,
        edge,
        cell: [0.1, c1, 0.9],
        noise: vec![grain],
    }
}

#[test]
fn point_lines_sample_as_the_written_out_law() {
    let (named, written) = (
        MaterialLaw::from_file(NAMED).unwrap(),
        MaterialLaw::from_file(WRITTEN_OUT).unwrap(),
    );
    for jw in [0.001, 0.0137, 0.1] {
        let (a, b) = (
            named.bind(&[("jw", jw)]).unwrap(),
            written.bind(&[("jw", jw)]).unwrap(),
        );
        for k in 0..40 {
            let e = f64::from(k) * 0.0007;
            let p = point(e, f64::from(k % 7) / 7.0, (f64::from(k) - 20.0) * 1e-4);
            assert_eq!(
                a.sample(&p).unwrap(),
                b.sample(&p).unwrap(),
                "jw {jw} edge {e}"
            );
        }
        // and on the surface
        for k in 0..16 {
            let (u, v) = (f64::from(k) / 16.0 + 0.01, 0.37 + f64::from(k) / 40.0);
            assert_eq!(
                a.at(u, v).unwrap(),
                b.at(u, v).unwrap(),
                "jw {jw} at ({u}, {v})"
            );
        }
    }
}

#[test]
fn a_point_reads_the_point_and_the_points_before_it() {
    let base = |points: &str, height: &str| {
        NAMED
            .replace(
                "point s = min(max(edge / jw, 0), 1)\npoint t = s * s * (3 - 2 * s)\n",
                points,
            )
            .replace("field height = 0.004 * t + grain", height)
    };
    // every point input, the noise, a parameter, an earlier point
    let ok = base(
        "point a = u + v + edge + cell0 + cell1 + cell2 + grain + jw\npoint b = a * 2\npoint t = 0.5\n",
        "field height = b * 0.001",
    );
    let law = MaterialLaw::from_file(&ok).unwrap();
    let got = law
        .bind(&[])
        .unwrap()
        .sample(&point(0.002, 0.5, 0.001))
        .unwrap();
    // a = 0.3 + 0.7 + 0.002 + 0.1 + 0.5 + 0.9 + 0.001 + 0.01, written out in that order
    let a = 0.3 + 0.7 + 0.002 + 0.1 + 0.5 + 0.9 + 0.001 + 0.01;
    assert_eq!(got.height, a * 2.0 * 0.001);
    for (bad, why) in [
        ("point a = b\npoint b = 1\npoint t = 0.5\n", "a later point"),
        ("point a = a + 1\npoint t = 0.5\n", "itself"),
        ("point jw = 1\npoint t = 0.5\n", "the name of a parameter"),
        ("point grain = 1\npoint t = 0.5\n", "the name of a noise"),
        ("point edge = 1\npoint t = 0.5\n", "a reserved name"),
        ("point t = 1\npoint t = 0.5\n", "declared twice"),
        ("point t = height\n", "a field, not a value"),
        ("point t =\n", "no expression"),
        ("point = 1\npoint t = 0.5\n", "no name"),
    ] {
        assert!(
            MaterialLaw::from_file(&base(bad, "field height = 0.004 * t + grain")).is_err(),
            "{why}: `{}` read",
            bad.trim()
        );
    }
    // a noise declared after the point line may not take its name either
    let late = NAMED.replace(
        "field height = 0.004 * t + grain",
        "noise s = (4, 0.1, 2)\nfield height = 0.004 * t + grain",
    );
    assert!(
        MaterialLaw::from_file(&late).is_err(),
        "a later noise named as a point"
    );
}

#[test]
fn a_point_that_is_not_finite_names_itself() {
    let law = MaterialLaw::from_file(&NAMED.replace(
        "point t = s * s * (3 - 2 * s)",
        "point t = s * s * (3 - 2 * s) + sqrt(0.1 - edge)",
    ))
    .unwrap_or_else(|e| panic!("{e}"));
    let b = law.bind(&[]).unwrap();
    // finite at the defaults' zero point (edge 0), not at edge 0.2
    match b.sample(&point(0.2, 0.5, 0.0)) {
        Err(MaterialError::NonFinite { field }) => assert_eq!(field, "t"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn point_lines_are_in_the_identifier() {
    let named = MaterialLaw::from_file(NAMED).unwrap().law_id();
    let written = MaterialLaw::from_file(WRITTEN_OUT).unwrap().law_id();
    let renamed = MaterialLaw::from_file(
        &NAMED
            .replace("point t =", "point w =")
            .replace("* t", "* w")
            .replace("0.004 * w + grain", "0.004 * w + grain"),
    )
    .unwrap()
    .law_id();
    let spaced = MaterialLaw::from_file(&NAMED.replace("point s = min(", "point   s  =  min("))
        .unwrap()
        .law_id();
    assert_ne!(
        named, written,
        "the text of the law differs, so its identifier"
    );
    assert_ne!(named, renamed, "a point's name is part of the law");
    assert_eq!(named, spaced, "spacing is not");
    // the fields unchanged, only what a point stands for: a different law
    let other_step =
        MaterialLaw::from_file(&NAMED.replace("point t = s * s * (3 - 2 * s)", "point t = s"))
            .unwrap()
            .law_id();
    assert_ne!(named, other_step, "a point's expression is part of the law");
}
