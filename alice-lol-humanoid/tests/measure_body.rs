//! Measuring a body in the units of the avatar body law
//!
//! The expected values are worked out by hand from the marks, as the same f64 operations in the
//! same order, not computed by the code under test.
#![allow(clippy::unwrap_used, clippy::float_cmp, clippy::suboptimal_flops)]
// the expected values spell the mean as (a + b) / 2, the arithmetic of the hand computation
#![allow(clippy::manual_midpoint)]

use std::path::Path;

use alice_lol::proportion_law::ProportionLaw;
use alice_lol_humanoid::measure::{measure, BodyMarks, MeasureError, Part, Side};

/// Mark a left point and its mirror on the right
fn pair(m: &mut BodyMarks, part: Part, left: [f64; 3]) {
    m.set(part, Some(Side::Left), left);
    m.set(part, Some(Side::Right), [-left[0], left[1], left[2]]);
}

/// A body 2 units from sole to eyes, standing on 0.5, its right side slightly unlike its left
fn marks() -> BodyMarks {
    let mut m = BodyMarks::new(0.5, 2.5);
    m.set(Part::Hips, None, [0.0, 1.8, 0.0]);
    m.set(Part::UpperLeg, Some(Side::Left), [0.1, 1.7, 0.0]);
    m.set(Part::UpperLeg, Some(Side::Right), [-0.12, 1.66, 0.0]);
    pair(&mut m, Part::LowerLeg, [0.1, 1.2, 0.0]);
    pair(&mut m, Part::UpperArm, [0.13, 2.3, 0.0]);
    // left arm straight out, right arm hanging 30 degrees below: the same lengths
    m.set(Part::LowerArm, Some(Side::Left), [0.43, 2.3, 0.0]);
    m.set(Part::Hand, Some(Side::Left), [0.68, 2.3, 0.0]);
    m.set(
        Part::LowerArm,
        Some(Side::Right),
        [-0.13 - 0.3 * 0.75_f64.sqrt(), 2.3 - 0.15, 0.0],
    );
    m.set(
        Part::Hand,
        Some(Side::Right),
        [-0.13 - 0.55 * 0.75_f64.sqrt(), 2.3 - 0.275, 0.0],
    );
    m
}

#[test]
fn the_values_are_fractions_of_the_eye_height() {
    let got = measure(&marks()).unwrap();
    let e = 2.5 - 0.5;
    assert_eq!(got.get("hips_y"), Some((1.8 - 0.5) / e));
    assert_eq!(got.get("hip_drop"), Some((1.8 - (1.7 + 1.66) / 2.0) / e));
    assert_eq!(got.get("hip_x"), Some((0.1 + 0.12) / 2.0 / e));
    assert_eq!(got.get("knee"), Some((1.2 - 0.5) / e));
    assert_eq!(got.get("shoulder_x"), Some((0.13 + 0.13) / 2.0 / e));
    // the lengths along the bones, the same on both sides up to rounding
    let arm = got.get("arm").unwrap();
    assert!((arm - 0.55 / e).abs() < 1e-12, "{arm}");
    let split = got.get("elbow_split").unwrap();
    assert!((split - 0.3 / 0.55).abs() < 1e-12, "{split}");
    assert_eq!(got.get("face_length"), None);
    assert_eq!(
        got.values().iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        [
            "hips_y",
            "hip_drop",
            "hip_x",
            "knee",
            "shoulder_x",
            "arm",
            "elbow_split"
        ]
    );
}

#[test]
fn the_arm_is_measured_along_the_bones_in_any_direction() {
    // the right arm reaches forward and down: 0.3 and 0.25 long in all three axes
    let mut m = marks();
    let (u, v) = (0.3 / 3.0_f64.sqrt(), 0.25 / 3.0_f64.sqrt());
    m.set(Part::LowerArm, Some(Side::Right), [-0.13 - u, 2.3 - u, u]);
    m.set(
        Part::Hand,
        Some(Side::Right),
        [-0.13 - u - v, 2.3 - u - v, u + v],
    );
    let got = measure(&m).unwrap();
    let arm = got.get("arm").unwrap();
    assert!((arm - 0.55 / 2.0).abs() < 1e-12, "{arm}");
}

#[test]
fn the_chin_gives_the_face_length() {
    let mut m = marks();
    m.chin = Some(2.38);
    assert_eq!(
        measure(&m).unwrap().get("face_length"),
        Some((2.5 - 2.38) / 2.0)
    );
}

#[test]
fn the_units_cancel() {
    // the same body scaled by 3 and moved up by 7 measures the same
    let mut m = marks();
    let s = |y: f64| y * 3.0 + 7.0;
    m.sole = s(m.sole);
    m.eye = s(m.eye);
    for p in m.joints.values_mut() {
        *p = [p[0] * 3.0, s(p[1]), p[2] * 3.0];
    }
    let (a, b) = (measure(&marks()).unwrap(), measure(&m).unwrap());
    for ((n, x), (_, y)) in a.values().iter().zip(b.values()) {
        assert!((x - y).abs() < 1e-12, "{n}: {x} {y}");
    }
}

#[test]
fn a_body_that_cannot_be_measured_says_why() {
    let mut m = marks();
    m.eye = m.sole;
    assert_eq!(measure(&m).unwrap_err(), MeasureError::BadUnit);
    let mut m = marks();
    m.eye = f64::NAN;
    assert_eq!(measure(&m).unwrap_err(), MeasureError::BadUnit);
    let mut m = marks();
    m.chin = Some(f64::INFINITY);
    assert_eq!(measure(&m).unwrap_err(), MeasureError::BadUnit);
    let mut m = marks();
    m.set(Part::Hips, None, [0.0, f64::NAN, 0.0]);
    assert_eq!(measure(&m).unwrap_err(), MeasureError::BadUnit);
    for (part, side, name) in [
        (Part::Hips, None, "hips"),
        (Part::UpperLeg, Some(Side::Right), "hip joints"),
        (Part::LowerLeg, Some(Side::Left), "knees"),
        (Part::UpperArm, Some(Side::Left), "shoulder joints"),
        (Part::LowerArm, Some(Side::Right), "elbows"),
        (Part::Hand, Some(Side::Left), "wrists"),
    ] {
        let mut m = marks();
        m.joints.remove(&(part, side));
        assert_eq!(measure(&m).unwrap_err(), MeasureError::Missing(name));
    }
    let mut m = marks();
    pair(&mut m, Part::LowerArm, [0.13, 2.3, 0.0]);
    pair(&mut m, Part::Hand, [0.13, 2.3, 0.0]);
    assert_eq!(measure(&m).unwrap_err(), MeasureError::Degenerate("arm"));
}

#[test]
fn the_measured_values_bind_the_body_law() {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../laws/avatar/body.law"),
    )
    .unwrap();
    let law = ProportionLaw::from_file(&text).unwrap();
    // every measured name is a parameter of the law
    let got = measure(&marks()).unwrap();
    for (n, _) in got.values() {
        assert!(law.params().iter().any(|p| p.name == *n), "{n}");
    }
    // a body inside the ranges binds and gives its own points back
    let mut m = BodyMarks::new(0.0, 1.0);
    m.chin = Some(0.94);
    m.set(Part::Hips, None, [0.0, 0.66, 0.0]);
    pair(&mut m, Part::UpperLeg, [0.052, 0.62, 0.0]);
    pair(&mut m, Part::LowerLeg, [0.052, 0.36, 0.0]);
    pair(&mut m, Part::UpperArm, [0.065, 0.9, 0.0]);
    pair(&mut m, Part::LowerArm, [0.065 + 0.17, 0.9, 0.0]);
    pair(&mut m, Part::Hand, [0.065 + 0.3, 0.9, 0.0]);
    let got = measure(&m).unwrap();
    let b = law.bind(&got.as_params()).unwrap();
    let close = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-12);
    assert!(close(b.point("knee_l").unwrap(), [0.052, 0.36, 0.0]));
    assert!(close(b.point("hip_l").unwrap(), [0.052, 0.62, 0.0]));
    assert!(close(b.point("wrist_l").unwrap(), [0.365, 0.9, 0.0]));
    assert!(close(b.point("elbow_l").unwrap(), [0.235, 0.9, 0.0]));
    assert!(close(b.point("chin").unwrap(), [0.0, 0.94, 0.0]));
}

/// The head top, when marked: its height in eye units, and with the chin the proportion class.
/// Both ratios are read from the marks in the one space they share: heads = (top − sole) /
/// (top − chin), `eye_ratio` = (eye − sole) / (top − sole)
#[test]
fn the_head_top_and_the_proportion_class() {
    use alice_lol_humanoid::measure::ProportionClass as C;
    let class = |top: Option<f64>, chin: Option<f64>| {
        let mut m = marks();
        m.head_top = top;
        m.chin = chin;
        measure(&m).map(|g| (g.get("head_top"), g.proportion_class()))
    };
    // sole 0.5, eyes 2.5 (e = 2); no chin: the height is measured, the head length is not
    assert_eq!(
        class(Some(2.64), None),
        Ok((Some((2.64 - 0.5) / 2.0), None))
    );
    // top 2.64, chin 2.34: heads 2.14 / 0.3 = 7.13, eye_ratio 2 / 2.14 = 0.935
    assert_eq!(class(Some(2.64), Some(2.34)).unwrap().1, Some(C::Adult));
    // top 2.9, chin 2.2: heads 2.4 / 0.7 = 3.43
    assert_eq!(class(Some(2.9), Some(2.2)).unwrap().1, Some(C::Youthful));
    // top 2.68, chin 2.32: heads 2.18 / 0.36 = 6.06, eye_ratio 0.917 (between the bands)
    assert_eq!(
        class(Some(2.68), Some(2.32)).unwrap().1,
        Some(C::Indeterminate)
    );
    // decided by the head count alone (the eyes in the adult band): top 2.6, chin 2.1: heads
    // 2.1 / 0.5 = 4.2, eye_ratio 2 / 2.1 = 0.952
    assert_eq!(class(Some(2.6), Some(2.1)).unwrap().1, Some(C::Youthful));
    // no head top: neither
    assert_eq!(class(None, Some(2.34)), Ok((None, None)));
    // a head top at or below the eyes, or not finite, is a bad mark
    for top in [2.5, 2.4, f64::NAN, f64::INFINITY] {
        assert_eq!(
            class(Some(top), Some(2.34)),
            Err(MeasureError::BadUnit),
            "{top}"
        );
        // without the chin too (no class to catch it)
        assert_eq!(class(Some(top), None), Err(MeasureError::BadUnit), "{top}");
    }
    // the head top is the law's last parameter
    let mut m = marks();
    m.head_top = Some(2.64);
    assert_eq!(
        measure(&m).unwrap().values().last().map(|v| v.0),
        Some("head_top")
    );
}
