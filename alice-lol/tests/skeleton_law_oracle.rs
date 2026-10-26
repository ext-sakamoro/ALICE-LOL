//! Skeleton laws (`kind skeleton`): reading, the hierarchy, chains, mirrored twins, binding and
//! the identifier
//!
//! The expected places below are worked out by hand from the law text, spelled as the same f64
//! operations in the same order (no fused multiply-add), not computed by the code under test: a
//! joint's place is its parent's place plus its offset, coordinate by coordinate.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
#![allow(clippy::suboptimal_flops)]

use std::fmt::Write as _;

use alice_lol::law_id::{fold_semantics_pins, LOL_SEMANTICS_ID};
use alice_lol::motion_law::{SampleError, MOTION_SEMANTICS_ID};
use alice_lol::proportion_law::PROPORTION_SEMANTICS_ID;
use alice_lol::skeleton_law::{
    measured_skeleton_semantics_id, skeleton_semantics_fingerprints, SkeletonLaw, MAX_CHAIN,
    MAX_JOINTS, SKELETON_PROBE_LAW, SKELETON_SEMANTICS_ID, SKELETON_SEMANTICS_PINS,
};

const ARM: &str = "law arm
kind skeleton
claim an arm from a shoulder, a finger of three joints, and a head
param upper number = 0.3 range 0.2 0.4
param curl number = 0.1 range 0 0.5
let half = upper / 2
joint shoulder_l = (0.2, 1.4, 0)
joint elbow_l from shoulder_l = (upper, 0 - half, 0)
chain finger_l from elbow_l count 3 = (0.05 / i, 0 - curl * i, 0.01 * i)
joint head = (0, 1.6, 0.05)
mirror _l _r
";

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// The arm's places by hand, for `upper` and `curl`
fn arm_by_hand(upper: f64, curl: f64) -> Vec<(&'static str, Option<usize>, [f64; 3])> {
    let half = upper / 2.0;
    let off_elbow = [upper, 0.0 - half, 0.0];
    let off_finger = |i: f64| [0.05 / i, 0.0 - curl * i, 0.01 * i];
    let neg = |o: [f64; 3]| [-o[0], o[1], o[2]];
    let shoulder_l = add([0.0; 3], [0.2, 1.4, 0.0]);
    let elbow_l = add(shoulder_l, off_elbow);
    let f1 = add(elbow_l, off_finger(1.0));
    let f2 = add(f1, off_finger(2.0));
    let f3 = add(f2, off_finger(3.0));
    let head = add([0.0; 3], [0.0, 1.6, 0.05]);
    let shoulder_r = add([0.0; 3], neg([0.2, 1.4, 0.0]));
    let elbow_r = add(shoulder_r, neg(off_elbow));
    let g1 = add(elbow_r, neg(off_finger(1.0)));
    let g2 = add(g1, neg(off_finger(2.0)));
    let g3 = add(g2, neg(off_finger(3.0)));
    vec![
        ("shoulder_l", None, shoulder_l),
        ("elbow_l", Some(0), elbow_l),
        ("finger_l1", Some(1), f1),
        ("finger_l2", Some(2), f2),
        ("finger_l3", Some(3), f3),
        ("head", None, head),
        ("shoulder_r", None, shoulder_r),
        ("elbow_r", Some(6), elbow_r),
        ("finger_r1", Some(7), g1),
        ("finger_r2", Some(8), g2),
        ("finger_r3", Some(9), g3),
    ]
}

#[test]
fn joints_follow_the_hierarchy_chains_and_twins() {
    let law = SkeletonLaw::from_file(ARM).unwrap();
    assert_eq!(law.name(), "arm");
    for (upper, curl) in [(0.3, 0.1), (0.2, 0.0), (0.4, 0.5), (0.27, 0.33)] {
        let b = law.bind(&[("upper", upper), ("curl", curl)]).unwrap();
        assert_eq!(b.values(), [upper, curl]);
        let got: Vec<(&str, Option<usize>, [f64; 3])> = b.joints().collect();
        assert_eq!(got, arm_by_hand(upper, curl), "upper {upper} curl {curl}");
        assert_eq!(b.place("finger_r2"), Some(got[9].2));
        assert_eq!(b.place("finger_l"), None, "a chain's name is not a joint");
        assert_eq!(b.place("half"), None, "a let is not a joint");
    }
    // the defaults
    let d = law.bind(&[]).unwrap();
    assert_eq!(d.joints().collect::<Vec<_>>(), arm_by_hand(0.3, 0.1));
    assert_eq!(
        law.joints().collect::<Vec<_>>(),
        arm_by_hand(0.3, 0.1)
            .iter()
            .map(|x| (x.0, x.1))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_declared_twin_is_kept_and_parents_map_to_twins() {
    // elbow_r is declared, so it is not mirrored; hand_l's twin hangs under the declared elbow_r;
    // spine has no suffix, so arm_l's twin hangs under spine too
    let law = SkeletonLaw::from_file(
        "law twins
kind skeleton
joint spine = (0, 1, 0)
joint arm_l from spine = (0.1, 0.2, 0)
joint elbow_l from arm_l = (0.3, 0, 0)
joint elbow_r from spine = (0, 0, 0.5)
joint hand_l from elbow_l = (0.25, 0, 0.125)
mirror _l _r
",
    )
    .unwrap();
    let b = law.bind(&[]).unwrap();
    let names: Vec<(&str, Option<usize>)> = b.joints().map(|(n, p, _)| (n, p)).collect();
    assert_eq!(
        names,
        [
            ("spine", None),
            ("arm_l", Some(0)),
            ("elbow_l", Some(1)),
            ("elbow_r", Some(0)),
            ("hand_l", Some(2)),
            ("arm_r", Some(0)),
            ("hand_r", Some(3)),
        ]
    );
    let spine = [0.0, 1.0, 0.0];
    let elbow_r = add(spine, [0.0, 0.0, 0.5]);
    assert_eq!(b.place("arm_r"), Some(add(spine, [-0.1, 0.2, 0.0])));
    assert_eq!(b.place("hand_r"), Some(add(elbow_r, [-0.25, 0.0, 0.125])));
}

#[test]
fn parameters_are_named_declared_and_in_range() {
    let law = SkeletonLaw::from_file(ARM).unwrap();
    let p = &law.params()[0];
    assert_eq!(
        (p.name.as_str(), p.default, p.lo, p.hi),
        ("upper", 0.3, 0.2, 0.4)
    );
    assert_eq!(
        law.bind(&[("width", 1.0)]).unwrap_err(),
        SampleError::UnknownParam("width".into())
    );
    assert_eq!(
        law.bind(&[("upper", 0.3), ("upper", 0.25)]).unwrap_err(),
        SampleError::RepeatedParam("upper".into())
    );
    for v in [0.19, 0.41, f64::NAN, f64::INFINITY] {
        assert_eq!(
            law.bind(&[("upper", v)]).unwrap_err(),
            SampleError::ParamOutOfRange {
                name: "upper".into()
            },
            "{v}"
        );
    }
    assert!(law.bind(&[("upper", 0.2)]).is_ok());
    assert!(law.bind(&[("upper", 0.4)]).is_ok());
}

#[test]
fn a_place_that_is_not_finite_names_itself() {
    let law = SkeletonLaw::from_file(
        "law gap
kind skeleton
param a number = 0.4 range 0.4 0.6
let inv = 1 / (a - 0.5)
joint j = (inv, 0, 0)
",
    )
    .unwrap();
    assert!(law.bind(&[("a", 0.45)]).is_ok());
    assert_eq!(
        law.bind(&[("a", 0.5)]).unwrap_err(),
        SampleError::NonFinite { name: "inv".into() }
    );
    // a chain's joint names itself, at the place where it fails (a + 2.5 - i < 0)
    let law = SkeletonLaw::from_file(
        "law fold
kind skeleton
param a number = 1 range -1 1
joint r = (0, 0, 0)
chain c from r count 3 = (0, sqrt(a + 2.5 - i), 0)
",
    )
    .unwrap();
    assert!(law.bind(&[("a", 1.0)]).is_ok());
    assert_eq!(
        law.bind(&[("a", 0.25)]).unwrap_err(),
        SampleError::NonFinite { name: "c3".into() }
    );
    assert_eq!(
        law.bind(&[("a", -0.75)]).unwrap_err(),
        SampleError::NonFinite { name: "c2".into() }
    );
}

#[test]
fn files_that_do_not_read() {
    let cases: [(&str, usize); 22] = [
        ("", 0),
        ("law x\nkind proportion\njoint a = (0, 0, 0)\n", 2),
        ("kind skeleton\n", 1),
        ("law x\nkind skeleton\nparam a number = 1 range 0 2\n", 0),
        ("law x\nkind skeleton\nwidth 3\njoint a = (0, 0, 0)\n", 3),
        ("law x\nkind skeleton\nparam a number = 3 range 0 2\njoint b = (a, 0, 0)\n", 3),
        ("law x\nkind skeleton\nparam i number = 1 range 0 2\njoint b = (i, 0, 0)\n", 3),
        ("law x\nkind skeleton\njoint a = (0, 0, 0)\nlet b = 2\n", 4),
        ("law x\nkind skeleton\njoint a = (0, 0, 0)\njoint a = (1, 0, 0)\n", 4),
        ("law x\nkind skeleton\njoint a from b = (0, 0, 0)\n", 3),
        ("law x\nkind skeleton\njoint a = (0, 0)\n", 3),
        ("law x\nkind skeleton\njoint a = (0, i, 0)\n", 3),
        ("law x\nkind skeleton\njoint a = (0, 0, 0)\nchain c from a count 0 = (0, i, 0)\n", 4),
        ("law x\nkind skeleton\njoint a = (0, 0, 0)\nchain c from a count 65 = (0, i, 0)\n", 4),
        ("law x\nkind skeleton\njoint a = (0, 0, 0)\nchain c2 from a count 2 = (0, i, 0)\n", 4),
        ("law x\nkind skeleton\njoint a = (0, 0, 0)\njoint c1 = (0, 0, 0)\nchain c from a count 2 = (0, i, 0)\n", 5),
        ("law x\nkind skeleton\njoint a = (0, 0, 0)\nchain c from a = (0, i, 0)\n", 4),
        ("law x\nkind skeleton\njoint a = (0, 0, 0)\nmirror _l _l\n", 4),
        ("law x\nkind skeleton\njoint a = (0, 0, 0)\nmirror _l _r\nmirror _a _b\n", 5),
        ("law x\nkind skeleton\nmirror _L _r\njoint a = (0, 0, 0)\n", 3),
        ("law x\nkind skeleton\nparam p_r number = 1 range 0 2\njoint p_l = (p_r, 0, 0)\nmirror _l _r\n", 0),
        ("law x\nkind skeleton\nparam a number = 0 range 0 1\njoint b = (1 / a, 0, 0)\n", 0),
    ];
    for (text, line) in cases {
        let e = SkeletonLaw::from_file(text).unwrap_err();
        assert_eq!(e.line, line, "{text:?}: {e}");
    }
    let e = SkeletonLaw::from_file("law x\nkind skeleton\njoint p = (1, max(2, 3))\n").unwrap_err();
    assert!(e.message.contains("3 coordinates, not 2"), "{e}");
    assert!(e.to_string().starts_with("skeleton law, line 3: "), "{e}");
    // the limits: a chain of 64 reads, and the joints stop at MAX_JOINTS
    assert_eq!(MAX_CHAIN, 64);
    let ok = format!("law x\nkind skeleton\njoint a = (0, 0, 0)\nchain c from a count {MAX_CHAIN} = (0, 0.01, 0)\n");
    assert_eq!(
        SkeletonLaw::from_file(&ok).unwrap().joints().count(),
        1 + MAX_CHAIN as usize
    );
    let mut big = String::from("law x\nkind skeleton\njoint a = (0, 0, 0)\n");
    let chains = MAX_JOINTS / 64;
    for k in 0..chains {
        let _ = writeln!(big, "chain c{k}x from a count 64 = (0, 0.01, 0)");
    }
    let e = SkeletonLaw::from_file(&big).unwrap_err();
    assert!(
        e.message
            .contains(&format!("more than {MAX_JOINTS} joints")),
        "{e}"
    );
}

#[test]
fn the_identifier_names_the_law_not_its_spelling() {
    let a = SkeletonLaw::from_file(ARM).unwrap().law_id();
    let spaced =
        ARM.replace("upper / 2", "upper/2") + "# a comment\nclaim a note\nsource a source\n";
    assert_eq!(SkeletonLaw::from_file(&spaced).unwrap().law_id(), a);
    for changed in [
        ARM.replace("= 0.3 range", "= 0.25 range"),
        ARM.replace("range 0.2 0.4", "range 0.2 0.5"),
        ARM.replace("upper / 2", "upper / 3"),
        ARM.replace("law arm", "law arms"),
        ARM.replace("joint head", "joint skull"),
        ARM.replace("count 3", "count 4"),
        ARM.replace("0.05 / i", "0.05 * i"),
        ARM.replace("mirror _l _r\n", ""),
        ARM.replace("mirror _l _r", "mirror _l _s"),
        // the same place as an offset from a parent is another declaration
        ARM.replace(
            "joint head = (0, 1.6, 0.05)",
            "joint head from shoulder_l = (0, 1.6, 0.05)",
        ),
        ARM.replace("joint head = (0, 1.6, 0.05)\n", ""),
    ] {
        assert_ne!(
            SkeletonLaw::from_file(&changed).unwrap().law_id(),
            a,
            "{changed}"
        );
    }
    // pinned: a literal for a fixed declaration
    assert_eq!(
        hex(&a),
        "55f4db613cb782da3f048f2cf8dbd6017a86d1dcbaa6e819fa41ca3368961b71"
    );
}

#[test]
fn the_skeleton_semantics_are_their_own_and_measured() {
    assert_ne!(SKELETON_SEMANTICS_ID, LOL_SEMANTICS_ID);
    assert_ne!(SKELETON_SEMANTICS_ID, MOTION_SEMANTICS_ID);
    assert_ne!(SKELETON_SEMANTICS_ID, PROPORTION_SEMANTICS_ID);
    let measured = skeleton_semantics_fingerprints();
    assert_eq!(measured.len(), SKELETON_SEMANTICS_PINS.len());
    for ((name, pinned), (m_name, m)) in SKELETON_SEMANTICS_PINS.iter().zip(&measured) {
        assert_eq!(name, m_name);
        assert_eq!(
            hex(m),
            hex(pinned),
            "pin `{name}` is not the measured behaviour"
        );
    }
    assert_eq!(
        fold_semantics_pins(SKELETON_SEMANTICS_PINS),
        SKELETON_SEMANTICS_ID
    );
    assert_eq!(measured_skeleton_semantics_id(), SKELETON_SEMANTICS_ID);
    assert!(SkeletonLaw::from_file(SKELETON_PROBE_LAW).is_ok());
}
