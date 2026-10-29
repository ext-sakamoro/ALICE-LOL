//! The avatar proportion laws of `laws/avatar`: they read, their outputs are the hand-computed
//! values of their text, every corner of their parameter ranges keeps the body and the face in
//! order or is refused by a `require` (counted per line and pinned), and together they hold at
//! most 300 numbers. The stylised family derives from them: it holds the values measured on the
//! drawn sheets that the reference family refuses, and keeps every output of the reference.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
#![allow(clippy::suboptimal_flops)]

use std::path::Path;

use alice_lol::motion_law::SampleError;
use alice_lol::proportion_law::ProportionLaw;

/// A law of `laws/avatar`, with the law it derives from read first (its file is relative)
fn law(name: &str) -> ProportionLaw {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../laws/avatar")
        .join(name);
    let text = std::fs::read_to_string(path).unwrap();
    match ProportionLaw::derives_from(&text) {
        None => ProportionLaw::from_file(&text).unwrap(),
        Some((parent, file)) => {
            let p = law(&file);
            ProportionLaw::from_file_with(&text, &[(parent.as_str(), &p)]).unwrap()
        }
    }
}

/// Parameter values by name
type Values = Vec<(String, f64)>;

/// Binds every corner; returns the bound corners' values and how many each `require` refused
/// (any other error fails)
fn bind_corners(law: &ProportionLaw) -> (Vec<Values>, Vec<(String, usize)>) {
    let mut refused: Vec<(String, usize)> = law.requires().map(|r| (r.to_owned(), 0)).collect();
    let mut bound = Vec::new();
    let cs = corners(law);
    assert_eq!(cs.len(), 1 << law.params().len());
    for c in cs {
        let named: Vec<(&str, f64)> = c.iter().map(|(n, v)| (n.as_str(), *v)).collect();
        match law.bind(&named) {
            Ok(_) => bound.push(c),
            Err(SampleError::Refused { require, .. }) => {
                refused.iter_mut().find(|(r, _)| *r == require).unwrap().1 += 1;
            }
            Err(e) => panic!("{c:?}: {e}"),
        }
    }
    (bound, refused)
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

/// Every combination of the ends of every parameter range
fn corners(law: &ProportionLaw) -> Vec<Vec<(String, f64)>> {
    let mut out = vec![Vec::new()];
    for p in law.params() {
        out = out
            .into_iter()
            .flat_map(|c| {
                [p.lo, p.hi].map(|v| {
                    let mut c = c.clone();
                    c.push((p.name.clone(), v));
                    c
                })
            })
            .collect();
    }
    out
}

#[test]
fn the_body_law_reads_and_its_defaults_are_its_text() {
    let law = law("body.law");
    let b = law.bind(&[]).unwrap();
    assert_eq!(b.point("hips"), Some([0.0, 0.66, 0.0]));
    assert_eq!(b.point("knee_l"), Some([0.05, 0.35, 0.0]));
    assert_eq!(b.point("ankle_l"), Some([0.05, 0.06, 0.0]));
    assert_eq!(b.point("elbow_l"), Some([0.06 + 0.3 * 0.56, 0.9, 0.0]));
    assert_eq!(b.point("wrist_l"), Some([0.06 + 0.3, 0.9, 0.0]));
    assert_eq!(b.point("hip_l"), Some([0.05, 0.66 - 0.04, 0.0]));
    // the unit: the eyes are at 1, the chin one face length below them, the top a parameter
    assert_eq!(b.point("eye_line"), Some([0.0, 1.0, 0.0]));
    assert_eq!(b.point("chin"), Some([0.0, 1.0 - 0.06, 0.0]));
    assert_eq!(b.point("top"), Some([0.0, 1.09, 0.0]));
    assert_eq!(b.scalar("chin_to_eye"), Some(0.06));
    assert_eq!(
        hex(&law.law_id()),
        "a8a66de2beb482bd5ff9aa682da92a1e274930be148b889be3b2afaa9dd960be"
    );
}

#[test]
fn the_face_law_reads_and_its_defaults_are_its_text() {
    let law = law("face.law");
    let b = law.bind(&[]).unwrap();
    assert_eq!(b.point("eye_l"), Some([0.53, 1.0, 0.0 - 0.58]));
    assert_eq!(b.point("eye_size"), Some([0.67, 0.28, 0.0]));
    assert_eq!(b.point("brow_outer_l"), Some([0.65, 1.27, 0.0]));
    assert_eq!(b.scalar("mouth_half_width"), Some(0.47 / 2.0));
    assert_eq!(b.scalar("head_half_width"), Some(1.0));
    assert_eq!(
        hex(&law.law_id()),
        "d4fc684c6bcb6b52dea6d09bf7bdf2e249cf17a0f92e7f9851b22f1f47b504bf"
    );
}

#[test]
fn every_corner_of_the_body_keeps_the_body_in_order() {
    for (file, refused) in [("body.law", [0, 0, 0]), ("body_stylised.law", [0, 0, 0])] {
        let law = law(file);
        let (cs, r) = bind_corners(&law);
        assert_eq!(
            r,
            [
                "knee <= hip_joint",
                "hip_joint <= shoulder",
                "chin_y <= head_top"
            ]
            .iter()
            .zip(refused)
            .map(|(n, k)| ((*n).to_owned(), k))
            .collect::<Vec<_>>(),
            "{file}"
        );
        assert_eq!(
            cs.len(),
            (1 << law.params().len()) - refused.iter().sum::<usize>()
        );
        every_body_corner_in_order(&law, cs);
    }
}

fn every_body_corner_in_order(law: &ProportionLaw, cs: Vec<Vec<(String, f64)>>) {
    for c in cs {
        let named: Vec<(&str, f64)> = c.iter().map(|(n, v)| (n.as_str(), *v)).collect();
        let b = law.bind(&named).unwrap();
        let y = |n: &str| b.point(n).unwrap()[1];
        let x = |n: &str| b.point(n).unwrap()[0];
        let order = [
            y("ankle_l"),
            y("knee_l"),
            y("hip_l"),
            y("hips"),
            y("shoulder_l"),
            y("chin"),
            y("eye_line"),
            y("top"),
        ];
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{c:?}: {order:?}");
        assert!(
            0.0 < x("shoulder_l") && x("shoulder_l") < x("elbow_l") && x("elbow_l") < x("wrist_l"),
            "{c:?}"
        );
        assert!(b.scalar("chin_to_eye").unwrap() > 0.0, "{c:?}");
    }
}

#[test]
fn every_corner_of_the_face_keeps_the_face_in_order() {
    for (file, refused) in [("face.law", [0, 0]), ("face_stylised.law", [0, 0])] {
        let law = law(file);
        let (cs, r) = bind_corners(&law);
        assert_eq!(
            r,
            [
                "1 + eye_height / 2 <= brow_y",
                "mouth_y <= 1 - eye_height / 2"
            ]
            .iter()
            .zip(refused)
            .map(|(n, k)| ((*n).to_owned(), k))
            .collect::<Vec<_>>(),
            "{file}"
        );
        assert_eq!(
            cs.len(),
            (1 << law.params().len()) - refused.iter().sum::<usize>()
        );
        every_face_corner_in_order(&law, cs);
    }
}

fn every_face_corner_in_order(law: &ProportionLaw, cs: Vec<Vec<(String, f64)>>) {
    for c in cs {
        let named: Vec<(&str, f64)> = c.iter().map(|(n, v)| (n.as_str(), *v)).collect();
        let b = law.bind(&named).unwrap();
        let eye = b.point("eye_l").unwrap();
        let size = b.point("eye_size").unwrap();
        let (bi, bo) = (
            b.point("brow_inner_l").unwrap(),
            b.point("brow_outer_l").unwrap(),
        );
        let mouth = b.point("mouth").unwrap();
        let half = b.scalar("head_half_width").unwrap();
        // the eyes do not meet, stay inside the head and sit below the brows, above the mouth
        assert!(eye[0] - size[0] / 2.0 > 0.0, "{c:?}");
        assert!(eye[0] + size[0] / 2.0 < half, "{c:?}");
        assert!(eye[1] + size[1] / 2.0 < bi[1], "{c:?}");
        assert!(mouth[1] < eye[1] - size[1] / 2.0, "{c:?}");
        assert!(0.0 < bi[0] && bi[0] < bo[0] && bo[0] < half, "{c:?}");
        assert!(b.scalar("mouth_half_width").unwrap() < half, "{c:?}");
    }
}

/// The laws are a generalisation, not a copy: all of them together hold at most 300 numbers
#[test]
fn the_avatar_laws_hold_at_most_300_numbers() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../laws/avatar");
    let mut files = 0;
    let mut numbers = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "law") {
            files += 1;
            for line in std::fs::read_to_string(&p).unwrap().lines() {
                let t = line.trim();
                if t.starts_with('#') || t.starts_with("claim") || t.starts_with("source") {
                    continue;
                }
                numbers += t
                    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_'))
                    .filter(|w| w.parse::<f64>().is_ok())
                    .count();
            }
        }
    }
    assert!(files >= 2, "{files} law files read");
    assert!(numbers > 0 && numbers <= 300, "{numbers} numbers");
}

/// The values measured on the two drawn sheets (eye heights for the body, chin-to-eye
/// distances for the face; the knee with its +0.021 correction)
const SHEET_BODY: [&[(&str, f64)]; 2] = [
    &[("knee", 0.293), ("face_length", 0.068)],
    &[("knee", 0.352), ("face_length", 0.069), ("shoulder", 0.878)],
];
const SHEET_FACE: [&[(&str, f64)]; 2] = [
    &[("eye_height", 0.52), ("brow_y", 1.4), ("mouth_y", 0.42)],
    &[("mouth_y", 0.316)],
];

#[test]
fn the_stylised_family_holds_the_drawn_sheets_the_reference_refuses() {
    let (body, body_s) = (law("body.law"), law("body_stylised.law"));
    let (face, face_s) = (law("face.law"), law("face_stylised.law"));
    assert_eq!(body_s.derived_from(), Some("avatar_body"));
    assert_eq!(face_s.derived_from(), Some("avatar_face"));
    for v in SHEET_BODY {
        assert!(body_s.bind(v).is_ok(), "{v:?}");
    }
    for v in SHEET_FACE {
        assert!(face_s.bind(v).is_ok(), "{v:?}");
    }
    // the reference family refuses sheet A's knee by its range, and has no shoulder parameter
    assert!(
        matches!(body.bind(SHEET_BODY[0]), Err(SampleError::ParamOutOfRange { name }) if name == "knee")
    );
    assert!(matches!(
        body.bind(&[("shoulder", 0.878)]),
        Err(SampleError::UnknownParam(_))
    ));
    assert!(
        matches!(face.bind(SHEET_FACE[0]), Err(SampleError::ParamOutOfRange { name }) if name == "eye_height")
    );
    // sheet B's mouth is inside both
    assert!(face.bind(SHEET_FACE[1]).is_ok());
    // the shoulder of a drawn body moves the arm's points; the reference keeps 0.9
    let b = body_s.bind(&[("shoulder", 0.878)]).unwrap();
    assert_eq!(b.point("wrist_l").unwrap()[1], 0.878);
    assert_eq!(body.bind(&[]).unwrap().point("wrist_l").unwrap()[1], 0.9);
    // the parameters the sheets did not measure keep the reference's range and default
    for (r, s) in [(&body, &body_s), (&face, &face_s)] {
        for p in r.params() {
            let q = s.params().iter().find(|q| q.name == p.name).unwrap();
            let measured = ["knee", "face_length", "eye_height", "brow_y", "mouth_y"]
                .contains(&p.name.as_str());
            assert_eq!(q == p, !measured, "{}", p.name);
        }
    }
}

#[test]
#[allow(clippy::many_single_char_names)] // r reference, s / d derived, a / b bound, f face
fn a_derived_law_has_every_output_of_its_reference() {
    for (r, s) in [
        ("body.law", "body_stylised.law"),
        ("face.law", "face_stylised.law"),
    ] {
        let (r, s) = (law(r), law(s));
        let (ro, so): (Vec<&str>, Vec<&str>) =
            (r.output_names().collect(), s.output_names().collect());
        assert_eq!(so[..ro.len()], ro[..]);
        assert!(r.requires().eq(s.requires().take(r.requires().count())));
    }
    // where the ranges overlap the two bodies are the same body: the values below are inside
    // both (knee 0.345, face_length 0.065), the drawn shoulder set to the reference's 0.9
    let (r, d) = (law("body.law"), law("body_stylised.law"));
    let both = [("knee", 0.345), ("face_length", 0.065)];
    let a = r.bind(&both).unwrap();
    let b = d.bind(&[both[0], both[1], ("shoulder", 0.9)]).unwrap();
    assert!(a.outputs().eq(b.outputs()));
    // the faces do not overlap in eye_height: the drawn face's own defaults, by hand
    let face_s = law("face_stylised.law");
    let f = face_s.bind(&[]).unwrap();
    assert_eq!(f.point("eye_size"), Some([0.67, 0.52, 0.0]));
    assert_eq!(f.point("brow_outer_l"), Some([0.65, 1.4, 0.0]));
    assert_eq!(f.point("mouth"), Some([0.0, 0.37, 0.0]));
}
