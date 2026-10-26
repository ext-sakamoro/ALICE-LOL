//! The avatar proportion laws of `laws/avatar`: they read, their outputs are the hand-computed
//! values of their text, every corner of their parameter ranges keeps the body and the face in
//! order, and together they hold at most 300 numbers.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
#![allow(clippy::suboptimal_flops)]

use std::path::Path;

use alice_lol::proportion_law::ProportionLaw;

fn law(name: &str) -> ProportionLaw {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../laws/avatar")
        .join(name);
    ProportionLaw::from_file(&std::fs::read_to_string(path).unwrap()).unwrap()
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
    assert_eq!(b.point("hips"), Some([0.0, 0.656, 0.0]));
    assert_eq!(b.point("knee_l"), Some([0.054, 0.35, 0.0]));
    assert_eq!(b.point("ankle_l"), Some([0.054, 0.055, 0.0]));
    assert_eq!(b.point("elbow_l"), Some([0.064 + 0.295 * 0.56, 0.896, 0.0]));
    assert_eq!(b.point("wrist_l"), Some([0.064 + 0.295, 0.896, 0.0]));
    assert_eq!(b.point("hip_l"), Some([0.054, 0.656 - 0.044, 0.0]));
    // the unit: the eyes are at 1, the chin one face length below them, the top a parameter
    assert_eq!(b.point("eye_line"), Some([0.0, 1.0, 0.0]));
    assert_eq!(b.point("chin"), Some([0.0, 1.0 - 0.055, 0.0]));
    assert_eq!(b.point("top"), Some([0.0, 1.09, 0.0]));
    assert_eq!(b.scalar("chin_to_eye"), Some(0.055));
    assert_eq!(
        hex(&law.law_id()),
        "978eeabfc4fb1c95768e52e79802ea9f33e2ff1e859bbb0800262d7a49be803c"
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
        "f13643d4aaf95441feb9bc8927365688e6e85593b765df49f06f4649439596b4"
    );
}

#[test]
fn every_corner_of_the_body_keeps_the_body_in_order() {
    let law = law("body.law");
    let cs = corners(&law);
    assert_eq!(cs.len(), 1 << law.params().len());
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
    let law = law("face.law");
    let cs = corners(&law);
    assert_eq!(cs.len(), 1 << law.params().len());
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
