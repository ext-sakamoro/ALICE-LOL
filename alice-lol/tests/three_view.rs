//! `alice_lol::three_view`: a record read from a sheet, derived into the avatar laws' units.
//!
//! oracle: an ideal sheet drawn from the laws themselves. The avatar laws are bound at their
//! defaults, their points are placed on an invented sheet (500 px per eye height on the body
//! views, 100 px per chin-to-eye distance on the face view), and the derivation of that
//! record must give the defaults back: the derivation is checked against the laws it serves,
//! not against itself. Each perturbation then moves exactly one thing and the verdict has
//! to name it (a value out of range by how much, a footwear shift that must cancel, a mark
//! that is not measurable, two views that disagree, a calibrated pick rule).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
// the sheet is built line by line with the laws' own notation (x, y, S, G, C …), as in the module doc
#![allow(
    clippy::suboptimal_flops,
    clippy::many_single_char_names,
    clippy::format_push_string
)]
#![allow(clippy::needless_raw_string_hashes)]

use std::path::Path;

use alice_lol::proportion_law::ProportionLaw;
use alice_lol::three_view::{
    check, derive, record_from_json, Law, Record, Verdict, View, ARMPIT_CORRECTION,
    CHECK_JSON_VERSION, CORRECTION_UNCERTAINTY, KNEE_WAIST_CORRECTION,
};

fn law(name: &str) -> ProportionLaw {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../laws/avatar")
        .join(name);
    ProportionLaw::from_file(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn ranges(l: &ProportionLaw) -> Vec<(String, f64, f64)> {
    l.params()
        .iter()
        .map(|p| (p.name.clone(), p.lo, p.hi))
        .collect()
}

fn defaults(l: &ProportionLaw) -> Vec<(String, f64)> {
    l.params()
        .iter()
        .map(|p| (p.name.clone(), p.default))
        .collect()
}

const S: f64 = 500.0; // px per eye height (body views)
const G: f64 = 1000.0; // ground row of the body views
const C: f64 = 300.0; // centre column of the front view
const FS: f64 = 100.0; // px per chin-to-eye distance (face view)
const FC: f64 = 2000.0; // centre column of the face view
const FCHIN: f64 = 900.0; // chin row of the face view

/// The record of an ideal sheet drawn from the laws at their defaults (plus `lift` px of
/// footwear under every body landmark)
fn ideal(outsole: f64, lift: f64) -> String {
    let body = law("body.law");
    let face = law("face.law");
    let b = body.bind(&[]).unwrap();
    let f = face.bind(&[]).unwrap();
    let pt = |n: &str| b.point(n).unwrap();
    let y = |v: f64| G - v * S - outsole - lift;
    let x = |v: f64| C + v * S;
    let mut t = String::from("three_view 1\nimage 3000 1200\npicked_by synthetic\n");
    t += &format!("footwear boot outsole {outsole} ankle_lift {lift} ± 0\n");
    t += &format!("view front box 0 0 1000 1100 ground {G} centre {C}\n");
    t += &format!("view side box 1000 0 1500 1100 ground {G} centre 1250\n");
    t += &format!("view face box 1800 600 2200 1000 ground - centre {FC}\n");
    let mark = |n: &str, v: &str, x: f64, y: f64| format!("mark {n} {v} {x} {y} ± 0\n");
    t += &mark("head_top", "front", C, y(pt("top")[1]));
    t += &mark("eye", "front", C, y(pt("eye_line")[1]));
    t += &mark("chin", "front", C, y(pt("chin")[1]));
    t += &mark(
        "shoulder",
        "front",
        x(pt("shoulder_l")[0]),
        y(pt("shoulder_l")[1]),
    );
    t += &mark("elbow", "front", x(pt("elbow_l")[0]), y(pt("elbow_l")[1]));
    t += &mark("wrist", "front", x(pt("wrist_l")[0]), y(pt("wrist_l")[1]));
    t += &mark("hips", "front", C, y(pt("hips")[1]));
    t += &mark("hip_joint", "front", x(pt("hip_l")[0]), y(pt("hip_l")[1]));
    t += &mark("knee", "front", x(pt("knee_l")[0]), y(pt("knee_l")[1]));
    t += &mark("ankle", "front", x(pt("ankle_l")[0]), y(pt("ankle_l")[1]));
    t += &mark("knee", "side", 1250.0, y(pt("knee_l")[1]));
    // face: chin at FCHIN, eye line one unit above it
    let fp = |n: &str| f.point(n).unwrap();
    let fy = |v: f64| FCHIN - v * FS;
    let fx = |v: f64| FC + v * FS;
    let (ex, ew, eh) = (fp("eye_l")[0], fp("eye_size")[0], fp("eye_size")[1]);
    t += &mark("eye", "face", fx(ex), fy(1.0));
    t += &mark("chin", "face", FC, fy(0.0));
    t += &mark("eye_inner", "face", fx(ex - ew / 2.0), fy(1.0));
    t += &mark("eye_outer", "face", fx(ex + ew / 2.0), fy(1.0));
    t += &mark("eye_top", "face", fx(ex), fy(1.0 + eh / 2.0));
    t += &mark("eye_bottom", "face", fx(ex), fy(1.0 - eh / 2.0));
    t += &mark(
        "brow",
        "face",
        fx(fp("brow_inner_l")[0]),
        fy(fp("brow_inner_l")[1]),
    );
    t += &mark(
        "brow_inner",
        "face",
        fx(fp("brow_inner_l")[0]),
        fy(fp("brow_inner_l")[1]),
    );
    t += &mark(
        "brow_outer",
        "face",
        fx(fp("brow_outer_l")[0]),
        fy(fp("brow_outer_l")[1]),
    );
    t += &mark("mouth", "face", FC, fy(fp("mouth")[1]));
    let mh = f.scalar("mouth_half_width").unwrap();
    t += &mark("mouth_left", "face", fx(-mh), fy(fp("mouth")[1]));
    t += &mark("mouth_right", "face", fx(mh), fy(fp("mouth")[1]));
    let hh = f.scalar("head_half_width").unwrap();
    t += &mark("head_left", "face", fx(-hh), fy(1.0));
    t += &mark("head_right", "face", fx(hh), fy(1.0));
    t
}

fn value(d: &alice_lol::three_view::Derivation, law: Law, name: &str) -> f64 {
    d.values
        .iter()
        .find(|v| v.law == law && v.name == name)
        .unwrap_or_else(|| panic!("{name} not derived: {:?}", d.not_measurable))
        .value
}

#[test]
fn an_ideal_sheet_drawn_from_the_laws_gives_their_defaults_back() {
    let r = Record::parse(&ideal(0.0, 0.0)).unwrap();
    let d = derive(&r);
    for (l, file) in [(Law::Body, "body.law"), (Law::Face, "face.law")] {
        let derived: Vec<_> = d.values.iter().filter(|v| v.law == l).collect();
        let named = defaults(&law(file));
        let mut compared = 0;
        for (name, want) in &named {
            if let Some(v) = derived.iter().find(|v| v.name == name) {
                assert!(
                    (v.value - want).abs() < 1e-12,
                    "{} {name}: derived {} vs default {want}",
                    l.name(),
                    v.value
                );
                assert_eq!(v.u, 0.0, "{name}: exact marks give no ±");
                compared += 1;
            }
        }
        // the body: all 9 params; the face: all but eye_recess / depth (they need a side face)
        assert_eq!(
            compared,
            if l == Law::Body { 9 } else { 7 },
            "{} compared {compared}",
            l.name()
        );
    }
    let missing: Vec<_> = d
        .not_measurable
        .iter()
        .map(|(l, n, _)| (l.name(), *n))
        .collect();
    assert_eq!(
        missing,
        [("avatar_face", "eye_recess"), ("avatar_face", "depth")]
    );
    // the law's fixed values are measured and agree exactly
    for f in &d.fixed {
        assert!(
            (f.value - f.fixed).abs() < 1e-12,
            "{} {}: {} vs fixed {}",
            f.law.name(),
            f.name,
            f.value,
            f.fixed
        );
    }
    assert_eq!(d.fixed.len(), 4, "shoulder, ankle, head_width, eye_x");
    assert!(d.cross_view.is_empty());
}

#[test]
fn footwear_under_the_body_cancels_out() {
    let flat = derive(&Record::parse(&ideal(0.0, 0.0)).unwrap());
    let boot = derive(&Record::parse(&ideal(12.0, 9.0)).unwrap());
    for v in &flat.values {
        let w = boot
            .values
            .iter()
            .find(|w| w.law == v.law && w.name == v.name)
            .unwrap();
        assert!(
            (v.value - w.value).abs() < 1e-12,
            "{}: {} vs {}",
            v.name,
            v.value,
            w.value
        );
    }
    // forgetting the footwear moves every height: the record must state it
    let t = ideal(12.0, 9.0).replace(
        "footwear boot outsole 12 ankle_lift 9 ± 0",
        "footwear boot outsole 0 ankle_lift 0 ± 0",
    );
    let wrong = derive(&Record::parse(&t).unwrap());
    assert!((value(&wrong, Law::Body, "knee") - value(&flat, Law::Body, "knee")).abs() > 0.01);
}

#[test]
fn a_value_outside_its_range_is_judged_by_how_far() {
    let body = law("body.law");
    let face = law("face.law");
    // move the knee down to 0.25 eye heights (range 0.31 .. 0.4)
    let knee_y = |v: f64| G - v * S;
    let t: String = ideal(0.0, 0.0)
        .lines()
        .map(|l| {
            if l.starts_with("mark knee front") {
                format!("mark knee front {} {} ± 5", C + 0.05 * S, knee_y(0.25))
            } else if l.starts_with("mark eye front") {
                l.replace("± 0", "± 4")
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let r = Record::parse(&t).unwrap();
    let c = check(
        &r,
        (&body.law_id(), &ranges(&body)),
        (&face.law_id(), &ranges(&face)),
    )
    .unwrap();
    let knee = c.judged.iter().find(|j| j.value.name == "knee").unwrap();
    assert!((knee.value.value - 0.25).abs() < 1e-12);
    // ± = (u_knee + knee · u_eye) / e = (5 + 0.25 · 4) / 500 = 0.012
    assert!((knee.value.u - 0.012).abs() < 1e-12, "{}", knee.value.u);
    match knee.verdict {
        Verdict::Below(by) => assert!((by - (0.31 - 0.25)).abs() < 1e-12, "{by}"),
        other => panic!("{other:?}"),
    }
    // everything else is in range
    for j in c.judged.iter().filter(|j| j.value.name != "knee") {
        assert_eq!(j.verdict, Verdict::In, "{}", j.value.name);
    }
}

#[test]
fn verdicts_at_the_range_ends() {
    assert_eq!(Verdict::of(0.35, 0.01, 0.31, 0.4), Verdict::In);
    assert_eq!(Verdict::of(0.315, 0.01, 0.31, 0.4), Verdict::Edge);
    assert_eq!(Verdict::of(0.395, 0.01, 0.31, 0.4), Verdict::Edge);
    assert_eq!(Verdict::of(0.31, 0.0, 0.31, 0.4), Verdict::In);
    assert!(
        matches!(Verdict::of(0.30, 0.0, 0.31, 0.4), Verdict::Below(d) if (d - 0.01).abs() < 1e-12)
    );
    assert!(
        matches!(Verdict::of(0.43, 0.5, 0.31, 0.4), Verdict::Above(d) if (d - 0.03).abs() < 1e-12)
    );
}

#[test]
fn a_mark_that_is_not_measurable_is_listed_never_guessed() {
    let t = ideal(0.0, 0.0)
        .lines()
        .map(|l| {
            if l.starts_with("mark hips front") {
                "mark hips front not_measurable under the skirt".to_owned()
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let d = derive(&Record::parse(&t).unwrap());
    for name in ["hips_y", "hip_drop"] {
        assert!(
            !d.values.iter().any(|v| v.name == name),
            "{name} derived without the hips"
        );
        let (_, _, why) = d
            .not_measurable
            .iter()
            .find(|(_, n, _)| *n == name)
            .unwrap();
        assert!(why.contains("under the skirt"), "{why}");
    }
    // hip_x needs only the hip joint: still derived
    assert!(d.values.iter().any(|v| v.name == "hip_x"));
    // no eye on the front view: no body value at all
    let t = ideal(0.0, 0.0)
        .lines()
        .filter(|l| !l.starts_with("mark eye front"))
        .collect::<Vec<_>>()
        .join("\n");
    let d = derive(&Record::parse(&t).unwrap());
    assert!(d.values.iter().all(|v| v.law == Law::Face));
    assert_eq!(
        d.not_measurable
            .iter()
            .filter(|(l, _, _)| *l == Law::Body)
            .count(),
        9
    );
}

#[test]
fn two_views_that_disagree_are_reported_not_averaged() {
    let base = ideal(0.0, 0.0);
    let side_knee = base
        .lines()
        .find(|l| l.starts_with("mark knee side"))
        .unwrap()
        .to_owned();
    let y: f64 = side_knee
        .split_whitespace()
        .nth(4)
        .unwrap()
        .parse()
        .unwrap();
    // 3 px apart with ± 1 each: within; 3 px with ± 1 and 0: beyond
    let with = |dy: f64, ua: f64, ub: f64| {
        base.lines()
            .map(|l| {
                if l.starts_with("mark knee side") {
                    format!("mark knee side 1250 {} ± {ub}", y + dy)
                } else if l.starts_with("mark knee front") {
                    let w: Vec<&str> = l.split_whitespace().collect();
                    format!("mark knee front {} {} ± {ua}", w[3], w[4])
                } else {
                    l.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(derive(&Record::parse(&with(2.0, 1.0, 1.0)).unwrap())
        .cross_view
        .is_empty());
    let d = derive(&Record::parse(&with(3.0, 1.0, 1.0)).unwrap());
    assert_eq!(d.cross_view.len(), 1);
    let c = &d.cross_view[0];
    assert_eq!(
        (c.landmark.as_str(), c.a, c.b),
        ("knee", View::Front, View::Side)
    );
    assert!(
        (c.height_a - c.height_b - 3.0).abs() < 1e-9 && c.tolerance == 2.0,
        "{c:?}"
    );
    // the front value is the one derived (the side one does not move it)
    let front = derive(&Record::parse(&ideal(0.0, 0.0)).unwrap());
    assert_eq!(
        value(&d, Law::Body, "knee"),
        value(&front, Law::Body, "knee")
    );
}

#[test]
fn a_calibrated_pick_rule_applies_its_correction() {
    let add_rule = |name: &str, rule: &str| {
        ideal(0.0, 0.0)
            .lines()
            .map(|l| {
                if l.starts_with(&format!("mark {name} front")) {
                    format!("{l} rule {rule}")
                } else {
                    l.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let plain = derive(&Record::parse(&ideal(0.0, 0.0)).unwrap());
    let knee = derive(&Record::parse(&add_rule("knee", "knee_waist")).unwrap());
    let k = knee.values.iter().find(|v| v.name == "knee").unwrap();
    assert_eq!(k.correction, Some(KNEE_WAIST_CORRECTION));
    assert!((k.value - value(&plain, Law::Body, "knee") - KNEE_WAIST_CORRECTION).abs() < 1e-12);
    assert_eq!(k.u, CORRECTION_UNCERTAINTY);
    let arm = derive(&Record::parse(&add_rule("shoulder", "armpit")).unwrap());
    let s = arm.values.iter().find(|v| v.name == "shoulder_x").unwrap();
    assert!((s.value - value(&plain, Law::Body, "shoulder_x") - ARMPIT_CORRECTION).abs() < 1e-12);
    // without the rule, no correction
    assert!(plain.values.iter().all(|v| v.correction.is_none()));
}

#[test]
fn stature_gives_the_eye_height_in_metres() {
    let t = ideal(0.0, 0.0).replace(
        "picked_by synthetic\n",
        "picked_by synthetic\nstature 1.5 stated\n",
    );
    let d = derive(&Record::parse(&t).unwrap());
    let top = law("body.law").bind(&[]).unwrap().point("top").unwrap()[1];
    let (m, u) = d.eye_height_m.unwrap();
    assert!((m - 1.5 / top).abs() < 1e-12, "{m}");
    assert_eq!(u, 0.0);
    assert!(derive(&Record::parse(&ideal(0.0, 0.0)).unwrap())
        .eye_height_m
        .is_none());
}

#[test]
fn the_reader_refuses_what_it_does_not_know() {
    let ok = ideal(0.0, 0.0);
    let cases: &[(&str, String)] = &[
        ("header", ok.replace("three_view 1", "three_view 2")),
        ("unknown line", format!("{ok}colour red\n")),
        ("unknown view", ok.replace("view side box", "view top box")),
        (
            "landmark of another view",
            format!("{ok}mark mouth front 1 1 ± 0\n"),
        ),
        (
            "mark on an unframed view",
            format!("{ok}mark knee back 1 1 ± 0\n"),
        ),
        ("twice", format!("{ok}mark knee side 1 1 ± 0\n")),
        (
            "not a number",
            ok.replace("ground 1000 centre 300", "ground NaN centre 300"),
        ),
        ("negative ±", format!("{ok}mark heel side 1 1 ± -1\n")),
        (
            "unknown rule",
            format!("{ok}mark toe_sole side 1 1 ± 0 rule guess\n"),
        ),
        ("reasonless", format!("{ok}mark heel side not_measurable\n")),
        (
            "inverted box",
            ok.replace(
                "view side box 1000 0 1500 1100",
                "view side box 1500 0 1000 1100",
            ),
        ),
        ("no image", ok.replace("image 3000 1200\n", "")),
    ];
    for (what, text) in cases {
        assert!(Record::parse(text).is_err(), "{what} was accepted");
    }
    assert!(Record::parse(&ok).is_ok());
    assert_eq!(cases.len(), 12);
}

#[test]
fn the_canonical_text_reads_back_to_the_same_record_and_id() {
    let r = Record::parse(&ideal(3.0, 4.0)).unwrap();
    let again = Record::parse(&r.to_text()).unwrap();
    assert_eq!(r, again);
    assert_eq!(r.id(), again.id());
    // order of lines in the input does not matter; content does
    let text = ideal(3.0, 4.0);
    let mut lines: Vec<&str> = text.lines().collect();
    lines[1..].reverse();
    let shuffled = Record::parse(&lines.join("\n")).unwrap();
    assert_eq!(shuffled.id(), r.id());
    let moved = Record::parse(&ideal(3.0, 5.0)).unwrap();
    assert_ne!(moved.id(), r.id());
}

#[test]
fn landmarks_as_json_make_the_same_record() {
    let json = r#"{"image": [3000, 1200], "picked_by": "synthetic",
      "footwear": {"kind": "boot", "outsole": 3, "ankle_lift": 4, "u": 0},
      "views": {"front": {"box": [0, 0, 1000, 1100], "ground": 1000, "centre": 300},
                "face": {"box": [1800, 600, 2200, 1000], "ground": null, "centre": 2000}},
      "marks": [{"name": "eye", "view": "front", "x": 300, "y": 400, "u": 2},
                {"name": "knee", "view": "front", "x": 320, "y": 800, "u": 5, "rule": "knee_waist"},
                {"name": "hips", "view": "front", "not_measurable": "under the skirt"},
                {"name": "chin", "view": "face", "x": 2000, "y": 900, "u": 1}]}"#;
    let from_json = record_from_json(json).unwrap();
    let text = "three_view 1\nimage 3000 1200\npicked_by synthetic\nfootwear boot outsole 3 ankle_lift 4 ± 0\n\
        view front box 0 0 1000 1100 ground 1000 centre 300\nview face box 1800 600 2200 1000 ground - centre 2000\n\
        mark eye front 300 400 ± 2\nmark knee front 320 800 ± 5 rule knee_waist\nmark hips front not_measurable under the skirt\n\
        mark chin face 2000 900 ± 1\n";
    assert_eq!(from_json, Record::parse(text).unwrap());
    for bad in [
        r#"{"image": [1, 1], "views": {}, "marks": [], "colour": "red"}"#,
        r#"{"image": [1, 1], "views": {}, "marks": [{"name": "eye", "view": "front", "x": 1}]}"#,
        r#"{"image": [1, 1], "views": {}, "marks": []} x"#,
        r#"{"image": [1, 1], "image": [2, 2], "views": {}, "marks": []}"#,
        r#"[1, 2]"#,
    ] {
        assert!(record_from_json(bad).is_err(), "{bad}");
    }
}

/// The JSON a reader relies on: its shape and its numbers, pinned byte for byte (a change of
/// shape bumps `CHECK_JSON_VERSION` and this golden together)
#[test]
fn the_check_json_is_pinned() {
    let body = law("body.law");
    let face = law("face.law");
    let t = ideal(0.0, 0.0)
        .lines()
        .map(|l| {
            if l.starts_with("mark knee front") {
                format!(
                    "mark knee front {} {} ± 5 rule knee_waist",
                    C + 0.05 * S,
                    G - 0.25 * S
                )
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        .replace(
            "picked_by synthetic\n",
            "picked_by synthetic\nstature 1.5 stated\n",
        );
    let r = Record::parse(&t).unwrap();
    let c = check(
        &r,
        (&body.law_id(), &ranges(&body)),
        (&face.law_id(), &ranges(&face)),
    )
    .unwrap();
    let json = c.to_json();
    assert!(json.starts_with(&format!("{{\"three_view_check\":{CHECK_JSON_VERSION},")));
    let golden =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/three_view_check_v1.json");
    if std::env::var_os("THREE_VIEW_BLESS").is_some() {
        std::fs::write(&golden, format!("{json}\n")).unwrap();
    }
    let want = std::fs::read_to_string(&golden).unwrap();
    assert_eq!(
        json,
        want.trim_end(),
        "regenerate with THREE_VIEW_BLESS=1 only for an intended change"
    );
}
