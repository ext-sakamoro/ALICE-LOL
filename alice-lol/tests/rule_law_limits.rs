//! The limits of a rule law file (conformance/TASK.md "Limits of a rule law file"): the size of the
//! file, the lines read, the tokens and the nesting of an expression, the characters of a numeral
//! Each limit reads exactly at its value, and one more makes the file unreadable (a `RuleLawError`,
//! never a panic, an abort or a cut-off law); an input numeral past the limit does not have its type
//!
//! oracle: the values are the table of TASK.md; the boundary files of `tests/data/rule_limits/` are
//! read by `scripts/law_rules.py` too (`scripts/test_law_rules.py` `Limits`), named by the outcome
//! both readers give

use alice_lol::law_input::parse_json;
use alice_lol::rule_law::{
    rule_law_from_file, rule_law_from_file_with, Answer, RuleLaw, MAX_EXPRESSION_DEPTH,
    MAX_EXPRESSION_TOKENS, MAX_LAW_BYTES, MAX_LAW_LINES, MAX_NUMERAL_CHARS,
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

#[test]
fn the_limits_are_the_values_of_the_task() {
    assert_eq!(MAX_LAW_BYTES, 4 * 1024 * 1024);
    assert_eq!(MAX_LAW_LINES, 100_000);
    assert_eq!(MAX_EXPRESSION_TOKENS, 256);
    assert_eq!(MAX_EXPRESSION_DEPTH, 64);
    // the longest amount of money, -2^127, has 40 characters
    assert_eq!(MAX_NUMERAL_CHARS, i128::MIN.to_string().len());
}

#[test]
fn each_boundary_file_reads_or_not_as_its_name_says() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/rule_limits");
    let mut seen = (0, 0);
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .expect("the boundary files")
        .map(|e| e.expect("an entry").path())
        .collect();
    names.sort();
    for p in names {
        let name = p.file_stem().and_then(|s| s.to_str()).expect("a name");
        let text = std::fs::read_to_string(&p).expect("UTF-8");
        let read = rule_law_from_file(&text);
        if name.starts_with("reads_") {
            assert!(read.is_ok(), "{name}: {read:?}");
            seen.0 += 1;
        } else {
            assert!(
                name.starts_with("unreadable_"),
                "{name}: reads_ or unreadable_"
            );
            // unreadable for its limit, not for another fault of the file
            let why = [
                ("parens", "nests at most 64 levels"),
                ("not", "nests at most 64 levels"),
                ("call", "nests at most 64 levels"),
                ("tokens", "has at most 256 tokens"),
                ("integer", "integer literal"),
                ("time", "is not a time"),
            ]
            .iter()
            .find(|(k, _)| name.contains(k))
            .map(|(_, w)| *w)
            .expect("a limit in the name");
            let e = read.expect_err(name);
            assert!(e.to_string().contains(why), "{name}: {e}");
            seen.1 += 1;
        }
    }
    // nothing compared is a failure
    assert!(seen.0 >= 5 && seen.1 >= 8, "{seen:?}");
}

fn statute_of(lines: usize) -> String {
    let mut text = String::from("kind statute\ntitle T\n");
    for i in 0..lines - 2 {
        let _ = writeln!(text, "clause c{i} obligation: x");
    }
    text
}

#[test]
fn a_file_of_the_largest_number_of_lines_reads_and_one_more_line_does_not() {
    assert!(rule_law_from_file(&statute_of(MAX_LAW_LINES)).is_ok());
    let more = rule_law_from_file(&statute_of(MAX_LAW_LINES + 1));
    assert!(more.is_err(), "{} lines read", MAX_LAW_LINES + 1);
    // lines for nobody (empty, comments) are not counted
    let padded = format!("{}\n# a comment\n\n", statute_of(MAX_LAW_LINES));
    assert!(rule_law_from_file(&padded).is_ok());
}

#[test]
fn a_file_of_the_largest_size_reads_and_one_byte_more_does_not() {
    let head = "kind statute\ntitle T\nclause c obligation: ";
    let fill = |n: usize| format!("{head}{}\n", "x".repeat(n - head.len() - 1));
    assert_eq!(fill(MAX_LAW_BYTES).len(), MAX_LAW_BYTES);
    assert!(rule_law_from_file(&fill(MAX_LAW_BYTES)).is_ok());
    assert!(rule_law_from_file(&fill(MAX_LAW_BYTES + 1)).is_err());
    // the bytes of the UTF-8 text, not its characters
    let wide = format!("{head}{}\n", "é".repeat((MAX_LAW_BYTES - head.len()) / 2));
    assert!(wide.len() > MAX_LAW_BYTES);
    assert!(rule_law_from_file(&wide).is_err());
}

#[test]
fn a_long_chain_or_a_deep_nesting_is_an_error_not_an_abort() {
    let law = |e: String| {
        format!("kind predicate\nx-input a bool\nrequire r: {e}\nexample {{\"a\": true}}\n")
    };
    for e in [
        vec!["a"; 20_000].join(" and "),
        vec!["a"; 20_000].join(" or "),
        format!("{}a{}", "(".repeat(5_000), ")".repeat(5_000)),
        format!("{}a", "not ".repeat(5_000)),
    ] {
        assert!(rule_law_from_file(&law(e)).is_err());
    }
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
