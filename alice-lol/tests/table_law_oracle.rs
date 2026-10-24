//! Table laws (`kind table`, conformance/TASK.md "Table"): reading, answering and the
//! identifier
//!
//! The answers below are written out by hand from the rule of `laws/spike/power_mode.law` in
//! words (not computed by the code under test); `scripts/test_law_rules.py` holds the same
//! cases for the Python reader. The full corpus (every combination of the values the rows
//! compare with) is scored by `conformance/score_rules.sh` against
//! `examples/rule_conformance.rs`.

use std::path::PathBuf;

use alice_lol::law_input::{parse_json, Json};
use alice_lol::rule_law::{rule_kind, rule_law_from_file, Answer, RuleKind};

fn spike(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../laws/spike")
        .join(format!("{name}.law"));
    std::fs::read_to_string(p).expect("law file")
}

fn json(text: &str) -> Json {
    parse_json(text).expect("test JSON")
}

/// (`battery_pct`, thermal, `frame_drops`, value, row)
const POWER: &[(i64, &str, i64, &str, Option<&str>)] = &[
    (50, "nominal", 0, "full", None),
    (50, "serious", 0, "minimal", Some("hot")),
    (50, "critical", 0, "minimal", Some("hot")),
    // hot and empty both hold: the row listed first decides
    (10, "critical", 20, "minimal", Some("hot")),
    (14, "nominal", 0, "minimal", Some("empty")),
    (15, "nominal", 0, "standard", Some("low")),
    // low and warm both hold
    (29, "fair", 0, "standard", Some("low")),
    (30, "fair", 0, "standard", Some("warm")),
    (30, "nominal", 10, "standard", Some("dropping")),
    (30, "nominal", 9, "full", None),
    (100, "Fair", 0, "full", None),
    (-5, "", 0, "minimal", Some("empty")),
];

fn request(b: &str, t: &str, d: &str) -> Json {
    json(&format!(
        "{{\"battery_pct\": {b}, \"thermal\": {t}, \"frame_drops\": {d}}}"
    ))
}

#[test]
fn the_power_file_is_a_table_law() {
    assert_eq!(rule_kind(&spike("power_mode")), Some(RuleKind::Table));
    assert_eq!(rule_kind("kind audit\nkind table\n"), None);
    let law = rule_law_from_file(&spike("power_mode")).expect("reads");
    assert_eq!(law.kind(), RuleKind::Table);
}

#[test]
fn the_power_rows_are_tried_in_order() {
    let law = rule_law_from_file(&spike("power_mode")).expect("reads");
    let mut compared = 0;
    for &(b, t, d, value, row) in POWER {
        let a = law.respond(&request(&b.to_string(), &format!("\"{t}\""), &d.to_string()));
        let want = Json::Object(vec![
            ("value".to_owned(), Json::Text(value.to_owned())),
            (
                "row".to_owned(),
                row.map_or(Json::Null, |r| Json::Text(r.to_owned())),
            ),
        ]);
        assert_eq!(a, Answer::Outputs(want), "{b} {t} {d}");
        compared += 1;
    }
    assert_eq!(compared, POWER.len());
}

#[test]
fn inputs_of_another_type_are_rejected_and_a_missing_one_is_a_request_error() {
    let law = rule_law_from_file(&spike("power_mode")).expect("reads");
    for (b, t, d) in [("1.5", "\"nominal\"", "0"), ("50", "3", "0"), ("50", "\"nominal\"", "null")] {
        assert!(
            matches!(law.respond(&request(b, t, d)), Answer::Rejected(_)),
            "{b} {t} {d}"
        );
    }
    // a missing input is decided before a value of another type
    assert!(matches!(
        law.respond(&json(r#"{"battery_pct": "x", "thermal": "nominal"}"#)),
        Answer::RequestError(_)
    ));
    // 3.0 is the integer 3
    let a = law.respond(&request("3.0", "\"nominal\"", "0"));
    assert_eq!(
        a,
        Answer::Outputs(Json::Object(vec![
            ("value".to_owned(), Json::Text("minimal".to_owned())),
            ("row".to_owned(), Json::Text("empty".to_owned())),
        ]))
    );
}

const TB: &str = "kind table\nx-input n integer\nvalue a b\nrow pos: a when n > 0\notherwise b\n";

#[test]
fn table_files_that_do_not_read() {
    let bad = [
        TB.replace("value a b\n", ""),
        format!("{TB}value c d\n"),
        TB.replace("value a b", "value a"),
        TB.replace("otherwise b\n", ""),
        format!("{TB}otherwise a\n"),
        "kind table\nx-input n integer\nvalue a b\notherwise b\nrow pos: a when n > 0\n".to_owned(),
        "kind table\nx-input n integer\nvalue a b\notherwise b\n".to_owned(),
        TB.replace("row pos: a", "row pos: c"),
        TB.replace("otherwise b", "otherwise c"),
        TB.replace("value a b", "value a b c"),
        TB.replace("otherwise b", "row pos: b when n < 0\notherwise b"),
        TB.replace("n > 0", "n"),
        TB.replace("n > 0", "m > 0"),
        format!("{TB}use alice-sign hmac_sha256\n"),
        TB.replace(
            "x-input n integer",
            "x-input n integer\nx-input s text\nx-split s \".\" p q",
        ),
        TB.replace("row pos:", "row:"),
        TB.replace("row pos:", "row true:"),
        TB.replace("value a b", "value a n"),
        TB.replace("row pos: a when", "row pos: a if"),
        format!("{TB}example {{\"n\": 1}}\n"),
        TB.replace("x-input n", "x-input  n"),
        TB.replace("x-input n integer", "x-input n integer\nconst n integer = 1"),
    ];
    for (i, text) in bad.iter().enumerate() {
        assert!(
            rule_law_from_file(text).is_err(),
            "case {i} reads:\n{text}"
        );
    }
    assert!(rule_law_from_file(TB).is_ok());
}

#[test]
fn the_identifier_names_the_rows_not_the_layout() {
    let id = |t: &str| rule_law_from_file(t).expect("reads").law_id();
    let base = id(TB);
    // comments, lines for people and spacing are not the rule
    assert_eq!(base, id(&format!("# c\nclaim x\n{TB}\n")));
    assert_eq!(base, id(&TB.replace("value a b", "value  a  b")));
    // the rows, their order, their labels, their values and the otherwise are
    assert_ne!(base, id(&TB.replace("n > 0", "n >= 0")));
    assert_ne!(base, id(&TB.replace("row pos:", "row positive:")));
    assert_ne!(base, id(&TB.replace("otherwise b", "row neg: b when n < 0\notherwise b")));
    let two = "kind table\nx-input n integer\nvalue a b\nrow p: a when n > 0\nrow q: b when n > 1\notherwise b\n";
    let swapped = "kind table\nx-input n integer\nvalue a b\nrow q: b when n > 1\nrow p: a when n > 0\notherwise b\n";
    assert_ne!(id(two), id(swapped));
    // a predicate with the same expression is another law
    let p = "kind predicate\nx-input n integer\nrequire pos: n > 0\nexample {\"n\": 1}\n";
    assert_ne!(base, id(p));
}
