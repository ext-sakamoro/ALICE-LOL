//! Proportion laws (`kind proportion`): reading, binding, outputs and the identifier
//!
//! The expected outputs below are worked out by hand from the law text, spelled as the same f64
//! operations in the same order (no fused multiply-add), not computed by the code under test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
#![allow(clippy::suboptimal_flops)]

use alice_lol::law_id::{fold_semantics_pins, LOL_SEMANTICS_ID};
use alice_lol::motion_law::{SampleError, MOTION_SEMANTICS_ID};
use alice_lol::proportion_law::{
    measured_proportion_semantics_id, proportion_semantics_fingerprints, Output, ProportionLaw,
    PROPORTION_PROBE_LAW, PROPORTION_SEMANTICS_ID, PROPORTION_SEMANTICS_PINS,
};

const LIMB: &str = "law limb
kind proportion
claim a limb of two segments
param length number = 0.3 range 0.2 0.4
param bend number = 0 range -1 1
let upper = 0.55 * length
out lower = length - upper
point joint = (bend * upper, 0 - upper, 0)
point tip = (max(0.25, length), 0 - length, 1 / 4)
";

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

#[test]
fn outputs_follow_the_expressions_in_file_order() {
    let law = ProportionLaw::from_file(LIMB).unwrap();
    assert_eq!(law.name(), "limb");
    assert_eq!(
        law.output_names().collect::<Vec<_>>(),
        ["lower", "joint", "tip"]
    );
    // the defaults: length 0.3, bend 0
    let b = law.bind(&[]).unwrap();
    let upper = 0.55 * 0.3;
    assert_eq!(b.values(), [0.3, 0.0]);
    assert_eq!(b.scalar("lower"), Some(0.3 - upper));
    assert_eq!(b.point("joint"), Some([0.0 * upper, 0.0 - upper, 0.0]));
    assert_eq!(b.point("tip"), Some([0.3, 0.0 - 0.3, 0.25]));
    let all: Vec<(&str, Output)> = b.outputs().collect();
    assert_eq!(all[0], ("lower", Output::Scalar(0.3 - upper)));
    // a scalar is not a point and the other way round
    assert_eq!(b.point("lower"), None);
    assert_eq!(b.scalar("tip"), None);
    assert_eq!(b.scalar("upper"), None, "a let is not an output");
    // other values: length 0.22 (max picks 0.25), bend -1
    let b = law.bind(&[("length", 0.22), ("bend", -1.0)]).unwrap();
    let upper = 0.55 * 0.22;
    assert_eq!(b.scalar("lower"), Some(0.22 - upper));
    assert_eq!(b.point("joint"), Some([-upper, 0.0 - upper, 0.0]));
    assert_eq!(b.point("tip"), Some([0.25, 0.0 - 0.22, 0.25]));
}

#[test]
fn parameters_are_named_declared_and_in_range() {
    let law = ProportionLaw::from_file(LIMB).unwrap();
    let p = &law.params()[0];
    assert_eq!(
        (p.name.as_str(), p.default, p.lo, p.hi),
        ("length", 0.3, 0.2, 0.4)
    );
    assert_eq!(
        law.bind(&[("width", 1.0)]).unwrap_err(),
        SampleError::UnknownParam("width".into())
    );
    assert_eq!(
        law.bind(&[("length", 0.3), ("length", 0.25)]).unwrap_err(),
        SampleError::RepeatedParam("length".into())
    );
    for v in [0.19, 0.41, f64::NAN, f64::INFINITY] {
        assert_eq!(
            law.bind(&[("length", v)]).unwrap_err(),
            SampleError::ParamOutOfRange {
                name: "length".into()
            },
            "{v}"
        );
    }
    // the ends of the range are in it
    assert!(law.bind(&[("length", 0.2)]).is_ok());
    assert!(law.bind(&[("length", 0.4)]).is_ok());
}

#[test]
fn an_output_that_is_not_finite_names_itself() {
    let law = ProportionLaw::from_file(
        "law gap
kind proportion
param a number = 0.4 range 0.4 0.6
let inv = 1 / (a - 0.5)
out g = inv * 2
",
    )
    .unwrap();
    assert!(law.bind(&[("a", 0.45)]).is_ok());
    assert_eq!(
        law.bind(&[("a", 0.5)]).unwrap_err(),
        SampleError::NonFinite { name: "inv".into() }
    );
    let law = ProportionLaw::from_file(
        "law root
kind proportion
param a number = 1 range -1 1
point p = (0, sqrt(a), 0)
",
    )
    .unwrap();
    assert_eq!(
        law.bind(&[("a", -1.0)]).unwrap_err(),
        SampleError::NonFinite { name: "p".into() }
    );
}

#[test]
fn files_that_do_not_read() {
    let cases: [(&str, usize); 14] = [
        ("", 0),
        ("law x\nkind motion\nout a = 1\n", 2),
        ("kind proportion\n", 1),
        ("law x\nkind proportion\nparam a number = 1 range 0 2\n", 0),
        ("law x\nkind proportion\nwidth 3\nout a = 1\n", 3),
        ("law x\nkind proportion\nparam a number = 3 range 0 2\nout b = a\n", 3),
        ("law x\nkind proportion\nparam a number = 1 range 0 2\nparam a number = 1 range 0 2\nout b = a\n", 4),
        ("law x\nkind proportion\nparam pi number = 1 range 0 2\nout b = pi\n", 3),
        ("law x\nkind proportion\nout a = 1\nlet b = 2\nout c = b\n", 4),
        ("law x\nkind proportion\nout a = 1\nout b = a\n", 4),
        ("law x\nkind proportion\npoint p = (1, 2)\n", 3),
        ("law x\nkind proportion\npoint p = 1, 2, 3\n", 3),
        ("law x\nkind proportion\nout a = b + 1\n", 3),
        ("law x\nkind proportion\nparam a number = 0 range 0 1\nout b = 1 / a\n", 0),
    ];
    for (text, line) in cases {
        let e = ProportionLaw::from_file(text).unwrap_err();
        assert_eq!(e.line, line, "{text:?}: {e}");
    }
    let e =
        ProportionLaw::from_file("law x\nkind proportion\npoint p = (1, max(2, 3))\n").unwrap_err();
    assert!(e.message.contains("3 coordinates, not 2"), "{e}");
    assert!(e.to_string().starts_with("proportion law, line 3: "), "{e}");
}

#[test]
fn the_identifier_names_the_law_not_its_spelling() {
    let a = ProportionLaw::from_file(LIMB).unwrap().law_id();
    let spaced = LIMB.replace("0.55 * length", "0.55*length")
        + "# a comment\nclaim a note\nsource a source\n";
    assert_eq!(ProportionLaw::from_file(&spaced).unwrap().law_id(), a);
    for changed in [
        LIMB.replace("= 0.3 range", "= 0.25 range"),
        LIMB.replace("range 0.2 0.4", "range 0.2 0.5"),
        LIMB.replace("0.55 * length", "0.56 * length"),
        LIMB.replace("law limb", "law limbs"),
        LIMB.replace("out lower", "out lower2"),
        LIMB.replace("1 / 4)", "1 / 5)"),
        LIMB.replace("point tip = (max(0.25, length), 0 - length, 1 / 4)\n", ""),
        // the same three expressions as three scalars are not a point
        LIMB.replace(
            "point tip = (max(0.25, length), 0 - length, 1 / 4)\n",
            "out tip = max(0.25, length)\nout tip_y = 0 - length\nout tip_z = 1 / 4\n",
        ),
    ] {
        assert_ne!(
            ProportionLaw::from_file(&changed).unwrap().law_id(),
            a,
            "{changed}"
        );
    }
    // pinned: a literal for a fixed declaration
    assert_eq!(
        hex(&a),
        "58a2117ff27c03e4bd53abd5d69d047e314b4513071863575cb6e4bcd84f7146"
    );
}

#[test]
fn the_proportion_semantics_are_their_own_and_measured() {
    assert_ne!(PROPORTION_SEMANTICS_ID, LOL_SEMANTICS_ID);
    assert_ne!(PROPORTION_SEMANTICS_ID, MOTION_SEMANTICS_ID);
    let measured = proportion_semantics_fingerprints();
    assert_eq!(measured.len(), PROPORTION_SEMANTICS_PINS.len());
    for ((name, pinned), (m_name, m)) in PROPORTION_SEMANTICS_PINS.iter().zip(&measured) {
        assert_eq!(name, m_name);
        assert_eq!(
            hex(m),
            hex(pinned),
            "pin `{name}` is not the measured behaviour"
        );
    }
    assert_eq!(
        fold_semantics_pins(PROPORTION_SEMANTICS_PINS),
        PROPORTION_SEMANTICS_ID
    );
    assert_eq!(measured_proportion_semantics_id(), PROPORTION_SEMANTICS_ID);
    assert!(ProportionLaw::from_file(PROPORTION_PROBE_LAW).is_ok());
}
