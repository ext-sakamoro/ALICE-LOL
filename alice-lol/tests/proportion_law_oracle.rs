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

// ── require and derive ────────────────────────────────────────────────────────────────────

/// A limb whose joint must stay below a reach, with a let that reads no name (`reach`)
const ARM: &str = "law arm
kind proportion
param length number = 0.3 range 0.2 0.4
param split number = 0.5 range 0.3 0.7
let reach = 0.35
let upper = length * split
require upper <= reach * split
require 0.1 <= length - upper
out lower = length - upper
point elbow = (upper, 0, 0)
";

#[test]
fn a_require_refuses_the_values_it_does_not_hold_for() {
    let law = ProportionLaw::from_file(ARM).unwrap();
    assert_eq!(
        law.requires().collect::<Vec<_>>(),
        ["upper <= reach * split", "0.1 <= length - upper"]
    );
    // the defaults hold: upper 0.15 <= 0.175, 0.1 <= 0.15
    assert!(law.bind(&[]).is_ok());
    // the edge holds (<=): length 0.35, split 0.5: upper 0.175 = reach * split
    assert!(law.bind(&[("length", 0.35)]).is_ok());
    // past it: length 0.4, split 0.5: upper 0.2 > 0.175, named with both values
    assert_eq!(
        law.bind(&[("length", 0.4)]).unwrap_err(),
        SampleError::Refused {
            require: "upper <= reach * split".into(),
            values: format!("{} > {}", 0.4 * 0.5, 0.35 * 0.5)
        }
    );
    // the second: length 0.2, split 0.7: 0.1 > 0.2 - 0.14 (the first holds: 0.14 <= 0.245)
    let e = law.bind(&[("length", 0.2), ("split", 0.7)]).unwrap_err();
    assert_eq!(
        e,
        SampleError::Refused {
            require: "0.1 <= length - upper".into(),
            values: format!("0.1 > {}", 0.2 - 0.2 * 0.7)
        }
    );
    assert!(e
        .to_string()
        .contains("`require 0.1 <= length - upper` does not hold"));
    // a law whose defaults break a require does not read
    let bad = ARM
        .replace("range 0.2 0.4", "range 0.2 0.5")
        .replace("= 0.3 range", "= 0.45 range");
    assert!(ProportionLaw::from_file(&bad).is_err());
}

#[test]
fn requires_that_do_not_read_are_refused() {
    for (what, line) in [
        ("no <=", "require upper < reach"),
        ("an output", "require lower <= reach"),
        ("a name declared later", "require upper <= later"),
        ("no right side", "require upper <="),
    ] {
        let text = ARM.replace("out lower", &format!("{line}\nlet later = 1\nout lower"));
        assert!(
            ProportionLaw::from_file(&text).is_err(),
            "{what} was accepted"
        );
    }
}

/// The arm, derived: a longer range, the reach as a parameter, one more require and output
const LONG_ARM: &str = "law long_arm
kind proportion
derive arm arm.law
claim the arm, longer
param length number = 0.45 range 0.3 0.6
param reach number = 0.5 range 0.4 0.6
param hand number = 0.08 range 0.05 0.1
require hand <= length - upper
out tip = length + hand
";

#[test]
fn a_derived_law_restates_promotes_and_adds() {
    let arm = ProportionLaw::from_file(ARM).unwrap();
    let long = ProportionLaw::from_file_with(LONG_ARM, &[("arm", &arm)]).unwrap();
    assert_eq!(long.derived_from(), Some("arm"));
    assert_eq!(
        ProportionLaw::derives_from(LONG_ARM),
        Some(("arm".into(), "arm.law".into()))
    );
    assert_eq!(ProportionLaw::derives_from(ARM), None);
    // the restated parameter keeps its place; the promoted let and the new one follow
    let p: Vec<(&str, f64, f64, f64)> = long
        .params()
        .iter()
        .map(|p| (p.name.as_str(), p.default, p.lo, p.hi))
        .collect();
    assert_eq!(
        p,
        [
            ("length", 0.45, 0.3, 0.6),
            ("split", 0.5, 0.3, 0.7),
            ("reach", 0.5, 0.4, 0.6),
            ("hand", 0.08, 0.05, 0.1)
        ]
    );
    // every output of the arm, then its own
    assert_eq!(
        long.output_names().collect::<Vec<_>>(),
        ["lower", "elbow", "tip"]
    );
    assert_eq!(
        long.requires().collect::<Vec<_>>(),
        [
            "upper <= reach * split",
            "0.1 <= length - upper",
            "hand <= length - upper"
        ]
    );
    // the arm's lines read the promoted reach: length 0.45, split 0.5: upper 0.225 <= 0.5 * 0.5
    let b = long.bind(&[]).unwrap();
    assert_eq!(b.scalar("lower"), Some(0.45 - 0.45 * 0.5));
    assert_eq!(b.scalar("tip"), Some(0.45 + 0.08));
    // the inherited require still refuses: reach 0.4, length 0.6, split 0.7: 0.42 > 0.28
    assert!(matches!(
        long.bind(&[("reach", 0.4), ("length", 0.6), ("split", 0.7)]),
        Err(SampleError::Refused { require, .. }) if require == "upper <= reach * split"
    ));
    // the arm's range does not apply any more, the derived one does
    // (length 0.55, reach 0.6: upper 0.275 <= 0.3)
    assert!(long.bind(&[("length", 0.55), ("reach", 0.6)]).is_ok());
    assert!(matches!(
        arm.bind(&[("length", 0.55)]),
        Err(SampleError::ParamOutOfRange { .. })
    ));
}

#[test]
fn a_derived_law_does_not_redefine_or_cut() {
    let arm = ProportionLaw::from_file(ARM).unwrap();
    let with = |text: &str| ProportionLaw::from_file_with(text, &[("arm", &arm)]);
    let cases = [
        (
            "not given",
            ProportionLaw::from_file(LONG_ARM),
            "which is not given",
        ),
        (
            "another law's name",
            ProportionLaw::from_file_with(
                LONG_ARM,
                &[("arm", &ProportionLaw::from_file(LIMB).unwrap())],
            ),
            "is named `limb`",
        ),
        (
            "itself",
            with(&LONG_ARM.replace("derive arm arm.law", "derive long_arm long_arm.law")),
            "does not derive from itself",
        ),
        (
            "not right after kind",
            with(&LONG_ARM.replace(
                "derive arm arm.law\nclaim the arm, longer",
                "claim the arm, longer\nderive arm arm.law",
            )),
            "right after `kind proportion`",
        ),
        (
            "twice",
            with(&LONG_ARM.replace("claim the arm, longer", "derive arm arm.law")),
            "right after `kind proportion`",
        ),
        (
            "a let that reads names made a parameter",
            with(&format!(
                "{LONG_ARM}param upper number = 0.2 range 0.1 0.3\n"
            )),
            "cannot become a parameter",
        ),
        (
            "an output redefined",
            with(&format!("{LONG_ARM}out lower = 1\n")),
            "`lower` is a line of the law this one derives from",
        ),
        (
            "a let redefined",
            with(&LONG_ARM.replace("param hand", "let upper = 0.1\nparam hand")),
            "`upper` is a line of the law this one derives from",
        ),
        (
            "a parameter restated twice",
            with(&format!(
                "{LONG_ARM}param length number = 0.5 range 0.3 0.6\n"
            )),
            "`length` is reserved or declared twice",
        ),
        (
            "a promoted name declared again",
            with(&format!(
                "{LONG_ARM}param reach number = 0.5 range 0.4 0.6\n"
            )),
            "`reach` is reserved or declared twice",
        ),
        (
            "an inherited require broken by the defaults",
            with(
                &LONG_ARM
                    .replace(
                        "param reach number = 0.5 range 0.4 0.6",
                        "param reach number = 0.4 range 0.4 0.6",
                    )
                    .replace("= 0.45 range 0.3 0.6", "= 0.6 range 0.3 0.6"),
            ),
            "`require upper <= reach * split` does not hold",
        ),
    ];
    for (what, r, why) in &cases {
        let e = r
            .as_ref()
            .map(ProportionLaw::name)
            .expect_err(what)
            .to_string();
        assert!(e.contains(why), "{what}: {e}");
    }
    assert_eq!(cases.len(), 11);
    assert!(with(LONG_ARM).is_ok());
}

#[test]
fn the_identifier_covers_requires_and_the_law_derived_from() {
    let arm = ProportionLaw::from_file(ARM).unwrap();
    // a law without the new lines keeps the identifier it had (the limb's pin above), and a
    // require moves it
    let without =
        ProportionLaw::from_file(&ARM.replace("require upper <= reach * split\n", "")).unwrap();
    assert_ne!(without.law_id(), arm.law_id());
    let swapped = ProportionLaw::from_file(
        &ARM.replace("upper <= reach * split", "reach * split <= upper")
            .replace("= 0.3 range 0.2 0.4", "= 0.4 range 0.2 0.4"),
    )
    .unwrap();
    assert_ne!(
        swapped.law_id(),
        arm.law_id(),
        "the sides are not interchangeable"
    );
    // the derived law moves when the law it derives from moves, even if its own text does not,
    // and even when the change is one the derived law restates (the length range here): its
    // resolved lines are then the same, only the identifier of the law it derives from differs
    let long = ProportionLaw::from_file_with(LONG_ARM, &[("arm", &arm)]).unwrap();
    let wider = ProportionLaw::from_file(&ARM.replace("range 0.2 0.4", "range 0.2 0.45")).unwrap();
    assert_ne!(wider.law_id(), arm.law_id());
    let long_wider = ProportionLaw::from_file_with(LONG_ARM, &[("arm", &wider)]).unwrap();
    assert_eq!(
        long_wider.params(),
        long.params(),
        "the restated range hides the change"
    );
    assert_ne!(
        long_wider.law_id(),
        long.law_id(),
        "the parent's identifier is in the child's"
    );
    let arm2 = ProportionLaw::from_file(&ARM.replace(
        "out lower = length - upper",
        "out lower = length - upper + 0",
    ))
    .unwrap();
    let long2 = ProportionLaw::from_file_with(LONG_ARM, &[("arm", &arm2)]).unwrap();
    assert_ne!(long.law_id(), long2.law_id());
    // and spacing does not count
    let spaced = ProportionLaw::from_file_with(
        &LONG_ARM.replace("derive arm arm.law", "derive   arm   arm.law"),
        &[("arm", &arm)],
    )
    .unwrap();
    assert_eq!(spaced.law_id(), long.law_id());
}
