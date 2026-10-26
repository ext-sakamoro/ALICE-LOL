//! Memory laws (conformance/TASK.md "Memory"): when a memory comes to mind, the shapes a reader
//! refuses, and what the identifier covers
//!
//! oracle: the answers are written from the contract (`recalled` is the `recall` expression,
//! `false` without one), not from this reader's output; the identifier of the fixture is the
//! second implementation's (`scripts/law_rules.py` through `tests/data/rule_law_ids_v2.txt`)

use alice_lol::law_input::{parse_json, Json};
use alice_lol::rule_law::{
    memory_semantics_fingerprints, resolve_delta, rule_law_from_file, Answer, RuleKind, RuleLaw,
    MAX_WEIGHT, MEMORY_SEMANTICS_ID, MEMORY_SEMANTICS_PINS,
};

fn hex(b: [u8; 32]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        use std::fmt::Write as _;
        let _ = write!(s, "{x:02x}");
        s
    })
}

const LINK_A: &str = "4f3b2a10c9d8e7f60514233241506f7e8d9cabbac9d8e7f60514233241506f7e";
const LINK_B: &str = "0000000000000000000000000000000000000000000000000000000000000001";

fn festival() -> String {
    format!(
        "kind memory\ncategory emotion\nformed 3\nweight cheer 3\nweight calm -1\nlink {LINK_A}\n\
         x-input topic text\nx-input day integer\nconst first integer = 2\n\
         recall when (topic == \"festival\" or topic == \"school_event\") and day >= first\n\
         line paint: my hands were covered in paint\n"
    )
}

fn ask(law: &RuleLaw, req: &str) -> Answer {
    law.respond(&parse_json(req).expect("JSON"))
}

fn recalled(a: &Answer) -> bool {
    match a {
        Answer::Outputs(o) => match o.get("recalled") {
            Some(Json::Bool(b)) => *b,
            other => panic!("no recalled: {other:?}"),
        },
        other => panic!("not outputs: {other:?}"),
    }
}

fn id(text: &str) -> String {
    hex(rule_law_from_file(text).expect("reads").law_id())
}

#[test]
fn recalled_is_the_recall_expression() {
    let law = rule_law_from_file(&festival()).expect("reads");
    assert_eq!(law.kind(), RuleKind::Memory);
    for (req, want) in [
        (r#"{"topic": "festival", "day": 2}"#, true),
        (r#"{"topic": "school_event", "day": 7}"#, true),
        (r#"{"topic": "festival", "day": 1}"#, false),
        (r#"{"topic": "rain", "day": 5}"#, false),
        (r#"{"topic": "", "day": 2}"#, false),
    ] {
        assert_eq!(recalled(&ask(&law, req)), want, "{req}");
    }
}

#[test]
fn a_memory_without_recall_never_comes_to_mind() {
    let law = rule_law_from_file("kind memory\ncategory taste\nformed 0\n").expect("reads");
    assert!(!recalled(&ask(&law, "{}")));
    // its inputs are still read: a missing one is an error of the request
    let law = rule_law_from_file("kind memory\ncategory taste\nformed 0\nx-input a integer\n")
        .expect("reads");
    assert!(matches!(ask(&law, "{}"), Answer::RequestError(_)));
    assert!(!recalled(&ask(&law, r#"{"a": 1}"#)));
}

#[test]
fn a_missing_input_is_a_request_error_and_a_mistyped_one_is_rejected() {
    let law = rule_law_from_file(&festival()).expect("reads");
    assert!(matches!(
        ask(&law, r#"{"topic": "festival"}"#),
        Answer::RequestError(_)
    ));
    assert!(matches!(
        ask(&law, r#"{"topic": 1, "day": 2}"#),
        Answer::Rejected(_)
    ));
    assert!(matches!(
        ask(&law, r#"{"topic": "festival", "day": 2.5}"#),
        Answer::Rejected(_)
    ));
    // a missing input is decided before a mistyped one
    assert!(matches!(
        ask(&law, r#"{"day": "x"}"#),
        Answer::RequestError(_)
    ));
}

#[test]
fn the_reader_gives_what_the_lines_say() {
    let RuleLaw::Memory(m) =
        rule_law_from_file(&format!("{}link {LINK_B}\n", festival())).expect("reads")
    else {
        panic!("a memory")
    };
    assert_eq!(m.category(), "emotion");
    assert_eq!(m.formed(), 3);
    // in the order of the axis names
    assert_eq!(m.weights(), vec![("calm", -1), ("cheer", 3)]);
    assert_eq!(m.weight("cheer"), 3);
    assert_eq!(m.weight("empathy"), 0);
    // in the order of the identifiers
    let ids: Vec<String> = m.links().iter().map(|x| hex(*x)).collect();
    assert_eq!(ids, [LINK_B, LINK_A]);
    assert_eq!(
        m.lines(),
        [(
            "paint".to_owned(),
            "my hands were covered in paint".to_owned()
        )]
    );
}

#[test]
fn the_shapes_a_reader_refuses() {
    let ok = "kind memory\ncategory emotion\nformed 3\n";
    assert!(rule_law_from_file(ok).is_ok());
    let max = format!("{ok}weight a {MAX_WEIGHT}\nweight b -{MAX_WEIGHT}\n");
    assert!(rule_law_from_file(&max).is_ok());
    for bad in [
        "kind memory\nformed 3\n".to_owned(),
        "kind memory\ncategory emotion\n".to_owned(),
        format!("{ok}category taste\n"),
        format!("{ok}formed 4\n"),
        "kind memory\ncategory Emotion\nformed 3\n".to_owned(),
        "kind memory\ncategory and\nformed 3\n".to_owned(),
        "kind memory\ncategory emotion\nformed -1\n".to_owned(),
        "kind memory\ncategory emotion\nformed 9007199254740992\n".to_owned(),
        "kind memory\ncategory emotion\nformed 03\n".to_owned(),
        format!("{ok}weight a 1\nweight a 2\n"),
        format!("{ok}weight a {}\n", MAX_WEIGHT + 1),
        format!("{ok}weight a -{}\n", MAX_WEIGHT + 1),
        format!("{ok}weight a -0\n"),
        format!("{ok}weight a +1\n"),
        format!("{ok}weight a 01\n"),
        format!("{ok}weight a 1.5\n"),
        format!("{ok}weight A 1\n"),
        format!("{ok}weight a\n"),
        format!("{ok}weight a 1 2\n"),
        format!("{ok}link {}\n", &LINK_A[1..]),
        format!("{ok}link {}\n", LINK_A.to_uppercase()),
        format!("{ok}link {LINK_A}\nlink {LINK_A}\n"),
        format!("{ok}x-input a money\n"),
        format!("{ok}const c money = 1 JPY\n"),
        format!("{ok}x-input a integer\nrecall when a\n"),
        format!("{ok}recall when b == 1\nx-input b integer\n"),
        format!("{ok}x-input a integer\nrecall when a == 1\nrecall when a == 2\n"),
        format!("{ok}x-input a integer\nrecall a == 1\n"),
        format!("{ok}line a:\n"),
        format!("{ok}line a x\n"),
        format!("{ok}line a: x\nline a: y\n"),
        format!("{ok}line and: x\n"),
        format!("{ok}title T\n"),
        format!("{ok}x-output a integer\n"),
        format!("currencies {LINK_A}\n{ok}"),
    ] {
        assert!(rule_law_from_file(&bad).is_err(), "{bad:?}");
    }
}

/// Every field that changes what a program computes moves the identifier; one mutant per field
#[test]
fn every_behavioural_field_is_in_the_identifier() {
    let base = festival();
    let b = id(&base);
    let mutants: [(&str, String); 12] = [
        (
            "category",
            base.replace("category emotion", "category relation"),
        ),
        ("formed", base.replace("formed 3", "formed 4")),
        (
            "weight value",
            base.replace("weight cheer 3", "weight cheer 4"),
        ),
        (
            "weight sign",
            base.replace("weight calm -1", "weight calm 1"),
        ),
        (
            "weight axis",
            base.replace("weight cheer 3", "weight cherry 3"),
        ),
        ("weight added", format!("{base}weight empathy 1\n")),
        ("weight removed", base.replace("weight calm -1\n", "")),
        ("link", base.replace(LINK_A, LINK_B)),
        ("link added", format!("{base}link {LINK_B}\n")),
        (
            "input type",
            base.replace("x-input day integer", "x-input day time")
                .replace("const first integer = 2", "const first time = 2"),
        ),
        (
            "constant",
            base.replace("const first integer = 2", "const first integer = 3"),
        ),
        ("recall", base.replace("day >= first", "day > first")),
    ];
    for (field, text) in &mutants {
        assert_ne!(&id(text), &b, "{field}: the identifier does not cover it");
    }
    assert_ne!(
        id("kind memory\ncategory emotion\nformed 3\n"),
        id("kind memory\ncategory emotion\nformed 3\nx-input a integer\nrecall when a == 1\n"),
        "recall present"
    );
}

#[test]
fn what_the_character_says_and_how_the_file_is_written_are_not() {
    let base = festival();
    let b = id(&base);
    for text in [
        base.replace("my hands were covered in paint", "paint everywhere"),
        base.replace("line paint: my hands were covered in paint\n", ""),
        format!("{base}line lights: the lanterns came on\n"),
        format!("law a name for people\nclaim words\n{base}"),
        base.replace(
            "weight cheer 3\nweight calm -1\n",
            "weight calm -1\nweight cheer 3\n",
        ),
        base.replace("formed 3", "formed\t 3  # when"),
    ] {
        assert_eq!(id(&text), b, "{text:?}");
    }
    // the order of the links does not change the memory
    let two = |a: &str, c: &str| {
        id(&format!(
            "kind memory\ncategory emotion\nformed 3\nlink {a}\nlink {c}\n"
        ))
    };
    assert_eq!(two(LINK_A, LINK_B), two(LINK_B, LINK_A));
}

#[test]
fn the_fixture_has_the_identifier_of_the_second_implementation() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let text = std::fs::read_to_string(root.join("laws/spike/demo_memory.law")).expect("fixture");
    let recorded = std::fs::read_to_string(root.join("alice-lol/tests/data/rule_law_ids_v2.txt"))
        .expect("ids");
    let line = recorded
        .lines()
        .find(|l| l.starts_with("laws/spike/demo_memory.law "))
        .expect("the fixture is recorded");
    assert_eq!(
        line,
        format!("laws/spike/demo_memory.law memory {}", id(&text))
    );
}

#[test]
fn the_memory_semantics_are_their_own_and_measured() {
    let measured = memory_semantics_fingerprints();
    assert_eq!(measured.len(), MEMORY_SEMANTICS_PINS.len());
    for ((name, pinned), (m_name, m)) in MEMORY_SEMANTICS_PINS.iter().zip(&measured) {
        assert_eq!(name, m_name);
        assert_eq!(
            hex(*m),
            hex(*pinned),
            "pin `{name}` is not the measured behaviour"
        );
    }
    assert_eq!(
        hex(alice_lol::law_id::fold_semantics_pins(
            MEMORY_SEMANTICS_PINS
        )),
        hex(MEMORY_SEMANTICS_ID)
    );
    for other in [
        alice_lol::rule_law::RULE_SEMANTICS_ID,
        alice_lol::rule_law::TABLE_SEMANTICS_ID,
        alice_lol::rule_law::STATUTE_SEMANTICS_ID,
        alice_lol::rule_law::CONTRACT_SEMANTICS_ID,
        alice_lol::rule_law::PROCEDURE_SEMANTICS_ID,
    ] {
        assert_ne!(other, MEMORY_SEMANTICS_ID);
    }
}

#[test]
fn an_edition_of_a_memory_and_a_delta_over_it() {
    let header = |edition: &str, sup: &str| {
        format!(
            "domain misaki\nedition {edition}\nsupersedes {sup}\neffective 0\nsemantics {}\n",
            hex(MEMORY_SEMANTICS_ID)
        )
    };
    let first = header("v1", "first") + &festival();
    let law = rule_law_from_file(&first).expect("reads");
    assert_ne!(
        hex(law.law_id()),
        id(&festival()),
        "an edition is another law"
    );
    let resolved = alice_lol::rule_law::edition_text(&first).expect("resolves");
    let delta = header("v2", &hex(law.law_id()))
        + "replace weight cheer: weight cheer 5\nremove line paint\nadd line lights: the lanterns came on\n";
    let (text, _) = resolve_delta(&resolved, &delta).expect("resolves");
    let RuleLaw::Memory(m) = rule_law_from_file(&text).expect("reads") else {
        panic!("a memory")
    };
    assert_eq!(m.weight("cheer"), 5);
    assert_eq!(m.lines()[0].0, "lights");
}
