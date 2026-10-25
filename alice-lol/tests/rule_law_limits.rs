//! The limits of a rule law file (conformance/TASK.md "Limits of a rule law file"): the size of the
//! file, the lines read, the tokens and the nesting of an expression, the characters of a numeral,
//! the nesting of the example's JSON. Each limit reads exactly at its value, and one more makes the
//! file unreadable (a `RuleLawError` naming the limit, never a panic, an abort or a cut-off law); an
//! input numeral past the limit does not have its type
//!
//! oracle: the values are `conformance/rule-limits.toml`, the one table both readers take; the
//! entries are `alice-lol/tests/data/rule_limits.json`, which `scripts/gen_rule_limits.py` generates
//! from that table and which `scripts/test_law_rules.py` `Limits` answers with the second reader

use alice_lol::law_input::{parse_json, Json};
use alice_lol::rule_law::{
    rule_law_from_file, rule_law_from_file_with, Answer, RuleLaw, MAX_EXPRESSION_DEPTH,
    MAX_EXPRESSION_TOKENS, MAX_JSON_DEPTH, MAX_LAW_BYTES, MAX_LAW_LINES, MAX_NUMERAL_CHARS,
};
use std::fmt::Write as _;
use std::path::Path;

fn hex(b: [u8; 32]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

const CURRENCIES: &str = "kind table\nx-input code text\nvalue e0 unknown\n\
     row jpy: e0 when code == \"JPY\"\notherwise unknown\n";

fn table() -> RuleLaw {
    rule_law_from_file(CURRENCIES).expect("the currency table reads")
}

fn repo_file(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

#[test]
fn the_constants_are_the_values_of_the_one_table() {
    // `key = <integer>` lines of conformance/rule-limits.toml (comments and empty lines skipped)
    let text = repo_file("conformance/rule-limits.toml");
    let mut table = std::collections::BTreeMap::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line.split_once('=').expect("key = value");
        let v: usize = v.trim().parse().expect("an integer");
        table.insert(k.trim().to_owned(), v);
    }
    let ours = [
        ("law_bytes", MAX_LAW_BYTES),
        ("law_lines", MAX_LAW_LINES),
        ("expression_tokens", MAX_EXPRESSION_TOKENS),
        ("expression_depth", MAX_EXPRESSION_DEPTH),
        ("numeral_chars", MAX_NUMERAL_CHARS),
        ("json_depth", MAX_JSON_DEPTH),
    ];
    assert_eq!(table.len(), ours.len(), "{table:?}");
    for (k, v) in ours {
        assert_eq!(table.get(k), Some(&v), "{k}");
    }
    // the longest amount of money, -2^127, has the numeral limit's characters
    assert_eq!(MAX_NUMERAL_CHARS, i128::MIN.to_string().len());
}

fn text_of(entry: &Json) -> String {
    let Some(Json::Array(parts)) = entry.get("parts") else {
        panic!("an entry has parts")
    };
    let mut out = String::new();
    for p in parts {
        let Json::Array(pc) = p else { panic!("a part") };
        let (Json::Text(t), Json::Number(c)) = (&pc[0], &pc[1]) else {
            panic!("[text, count]")
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        out.push_str(&t.repeat(*c as usize));
    }
    out
}

#[test]
fn each_boundary_entry_reads_or_not_and_names_its_limit() {
    let manifest = parse_json(&repo_file("alice-lol/tests/data/rule_limits.json")).expect("JSON");
    let Some(Json::Array(entries)) = manifest.get("entries") else {
        panic!("entries")
    };
    let mut seen = (0, 0);
    for e in entries {
        let (Some(Json::Text(name)), Some(Json::Text(expect)), Some(Json::Text(why))) =
            (e.get("name"), e.get("expect"), e.get("why"))
        else {
            panic!("name / expect / why")
        };
        let read = rule_law_from_file(&text_of(e));
        if expect == "reads" {
            assert!(read.is_ok(), "{name}: {read:?}");
            seen.0 += 1;
        } else {
            assert_eq!(expect, "unreadable", "{name}");
            let err = read.expect_err(name).to_string();
            assert!(err.contains(why.as_str()), "{name}: {err}");
            seen.1 += 1;
        }
    }
    // nothing compared is a failure; every dimension has entries on both sides
    assert!(seen.0 >= 15 && seen.1 >= 15, "{seen:?}");
}

#[test]
fn a_money_constant_of_the_longest_amount_reads_and_a_longer_numeral_does_not() {
    let t = table();
    let law = |amount: &str| {
        format!(
            "kind predicate\ncurrencies {}\nx-input fee money\nconst m money = {amount} JPY\n\
             require r: fee >= m\nexample {{\"fee\": \"1 JPY\"}}\n",
            hex(t.law_id())
        )
    };
    assert!(rule_law_from_file_with(&law(&i128::MIN.to_string()), Some(&t)).is_ok());
    for amount in [format!("-{}", "1".repeat(40)), "9".repeat(5_000)] {
        assert!(
            rule_law_from_file_with(&law(&amount), Some(&t)).is_err(),
            "{amount:.50}"
        );
    }
}

#[test]
fn a_time_or_an_amount_of_the_other_kinds_past_the_limit_does_not_read() {
    let d = "9".repeat(5_000);
    for text in [
        format!("kind statute\ntitle T\nclause c obligation from {d}: x\n"),
        format!("kind statute\ntitle T\nclause c obligation until {d}: x\n"),
    ] {
        assert!(rule_law_from_file(&text).is_err());
    }
    let t = table();
    for body in [
        format!("party a\nparty b\ncreated {d}\n"),
        format!("party a\nparty b\ncreated 0\nobligation o a to b {d} JPY due 1\n"),
        format!("party a\nparty b\ncreated 0\nobligation o a to b 1 JPY due {d}\n"),
    ] {
        let text = format!("kind contract\ncurrencies {}\n{body}", hex(t.law_id()));
        assert!(
            rule_law_from_file_with(&text, Some(&t)).is_err(),
            "{body:.60}"
        );
    }
    // the same contract with numerals in range reads (the failures above are the numerals)
    let ok = format!(
        "kind contract\ncurrencies {}\nparty a\nparty b\ncreated 0\nobligation o a to b 1 JPY due 1\n",
        hex(t.law_id())
    );
    assert!(rule_law_from_file_with(&ok, Some(&t)).is_ok());
}

#[test]
fn an_input_numeral_past_the_limit_does_not_have_its_type() {
    let t = table();
    let law = rule_law_from_file_with(
        &format!(
            "kind predicate\ncurrencies {}\nx-input fee money\nx-input n integer\n\
             require r: fee <= fee\nexample {{\"fee\": \"1 JPY\", \"n\": 1}}\n",
            hex(t.law_id())
        ),
        Some(&t),
    )
    .expect("reads");
    for req in [
        format!(r#"{{"fee": "{} JPY", "n": 1}}"#, "9".repeat(5_000)),
        format!(r#"{{"fee": "-{} JPY", "n": 1}}"#, "1".repeat(40)),
        format!(r#"{{"fee": "1 JPY", "n": {}}}"#, "9".repeat(5_000)),
    ] {
        let a = law.respond(&parse_json(&req).expect("JSON"));
        assert!(matches!(a, Answer::Rejected(_)), "{req:.60}: {a:?}");
    }
}
