//! Contract laws (conformance/TASK.md "Contract (`kind contract`)"): the status of a contract at a
//! time with some obligations fulfilled, what is still owed, the shapes a reader refuses, and what
//! the identifier covers
//!
//! oracle: the status is written from ALICE-Legal 9490cf2 `src/contract.rs` (`check_status`: an
//! unfulfilled obligation with `now > due` breaches; otherwise all fulfilled is fulfilled;
//! otherwise active), not from this reader

use alice_lol::law_input::{parse_json, Json};
use alice_lol::rule_law::{
    contract_semantics_fingerprints, rule_law_from_file, rule_law_from_file_with, Answer, RuleKind,
    RuleLaw, CONTRACT_SEMANTICS_ID, CONTRACT_SEMANTICS_PINS,
};

fn hex(b: [u8; 32]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        use std::fmt::Write as _;
        let _ = write!(s, "{x:02x}");
        s
    })
}

const CURRENCIES: &str = "kind table\nx-input code text\nvalue e0 e2 unknown\n\
     row jpy: e0 when code == \"JPY\"\nrow usd: e2 when code == \"USD\"\notherwise unknown\n";

fn table() -> RuleLaw {
    rule_law_from_file(CURRENCIES).expect("the currency table reads")
}

fn sale_text(t: &RuleLaw) -> String {
    format!(
        "law a demo sale (fixture)\nkind contract\ncurrencies {}\nparty buyer\nparty seller\n\
         created 100\ncondition inspected decided by {}: the goods were inspected\n\
         obligation deposit buyer to seller 1000 JPY due 200\n\
         obligation price buyer to seller 9223372036854775900 JPY due 300\n\
         obligation delivery_fee seller to buyer 50 JPY due 300\n",
        hex(t.law_id()),
        "cd".repeat(32)
    )
}

fn sale() -> RuleLaw {
    let t = table();
    rule_law_from_file_with(&sale_text(&t), Some(&t)).expect("reads")
}

fn ask(law: &RuleLaw, req: &str) -> Answer {
    law.respond(&parse_json(req).expect("JSON"))
}

fn out(a: &Answer) -> (String, Vec<String>, String, bool) {
    let Answer::Outputs(o) = a else {
        panic!("not outputs: {a:?}")
    };
    let text = |k: &str| match o.get(k) {
        Some(Json::Text(t)) => t.clone(),
        other => panic!("{k}: {other:?}"),
    };
    let list = match o.get("unfulfilled") {
        Some(Json::Array(v)) => v
            .iter()
            .map(|x| match x {
                Json::Text(t) => t.clone(),
                other => panic!("{other:?}"),
            })
            .collect(),
        other => panic!("unfulfilled: {other:?}"),
    };
    let met = matches!(o.get("conditions_met"), Some(Json::Bool(true)));
    (text("status"), list, text("owed"), met)
}

#[test]
fn the_status_is_alice_legal_s_check_status() {
    let law = sale();
    assert_eq!(law.kind(), RuleKind::Contract);
    let all = ["deposit", "price", "delivery_fee"]
        .map(str::to_owned)
        .to_vec();
    // not yet due: active
    assert_eq!(
        out(&ask(&law, r#"{"now": 150}"#)),
        (
            "active".into(),
            all,
            "9223372036854776950 JPY".into(),
            false
        )
    );
    // at the due time it is not breached (`now > due`), one second after it is
    assert_eq!(out(&ask(&law, r#"{"now": 200}"#)).0, "active");
    assert_eq!(out(&ask(&law, r#"{"now": 201}"#)).0, "breached");
    let r = out(&ask(&law, r#"{"now": 201, "fulfilled": ["deposit"]}"#));
    assert_eq!(
        (r.0.as_str(), r.2.as_str()),
        ("active", "9223372036854775950 JPY")
    );
    assert_eq!(
        out(&ask(
            &law,
            r#"{"now": 301, "fulfilled": ["deposit", "price"]}"#
        ))
        .0,
        "breached"
    );
    let done = out(&ask(
        &law,
        r#"{"now": 999, "fulfilled": ["delivery_fee", "deposit", "price"], "met": ["inspected"]}"#,
    ));
    assert_eq!(done, ("fulfilled".into(), vec![], "0 JPY".into(), true));
    // before 1970 is a time too: nothing is due yet
    assert_eq!(out(&ask(&law, r#"{"now": -5}"#)).0, "active");
}

#[test]
fn every_condition_is_met_and_what_is_owed_stays_in_range() {
    let t = table();
    let two = sale_text(&t).replace(
        "obligation deposit",
        &format!(
            "condition paid decided by {}: the bank confirmed\nobligation deposit",
            "ef".repeat(32)
        ),
    );
    let law = rule_law_from_file_with(&two, Some(&t)).expect("reads");
    assert!(
        !out(&ask(&law, r#"{"now": 150, "met": ["inspected"]}"#)).3,
        "one of two conditions"
    );
    assert!(out(&ask(&law, r#"{"now": 150, "met": ["paid", "inspected"]}"#)).3);
    // two amounts of 2^126 each: their sum, 2^127, is not an amount
    let big = sale_text(&t)
        .replace(
            "1000 JPY due 200",
            "85070591730234615865843651857942052864 JPY due 200",
        )
        .replace(
            "9223372036854775900 JPY",
            "85070591730234615865843651857942052864 JPY",
        );
    let law = rule_law_from_file_with(&big, Some(&t)).expect("reads");
    assert!(matches!(ask(&law, r#"{"now": 150}"#), Answer::Rejected(_)));
    assert!(!matches!(
        ask(&law, r#"{"now": 150, "fulfilled": ["deposit"]}"#),
        Answer::Rejected(_)
    ));
}

#[test]
fn what_a_request_must_be() {
    let law = sale();
    assert!(matches!(
        ask(&law, r#"{"fulfilled": []}"#),
        Answer::RequestError(_)
    ));
    for bad in [
        r#"{"now": "150"}"#,
        r#"{"now": 1.5}"#,
        r#"{"now": 150, "fulfilled": "deposit"}"#,
        r#"{"now": 150, "fulfilled": ["deposit", "deposit"]}"#,
        r#"{"now": 150, "fulfilled": ["buyer"]}"#,
        r#"{"now": 150, "fulfilled": ["inspected"]}"#,
        r#"{"now": 150, "fulfilled": [1]}"#,
        r#"{"now": 150, "met": ["deposit"]}"#,
        r#"{"now": 150, "met": null}"#,
    ] {
        assert!(matches!(ask(&law, bad), Answer::Rejected(_)), "{bad}");
    }
}

#[test]
fn what_does_not_read() {
    let t = table();
    let base = sale_text(&t);
    let reads = |s: &str| rule_law_from_file_with(s, Some(&t)).is_ok();
    assert!(reads(&base));
    for (from, to, why) in [
        ("party seller\n", "", "one party"),
        (
            "party seller\n",
            "party seller\nparty seller\n",
            "a second party",
        ),
        ("created 100\n", "", "no created"),
        ("created 100\n", "created 100\ncreated 101\n", "two created"),
        (
            "created 100\n",
            "created 301\n",
            "an obligation due before the contract",
        ),
        (
            "created 100\n",
            "created 18446744074\n",
            "after the latest time",
        ),
        (
            "buyer to seller 1000 JPY",
            "buyer to buyer 1000 JPY",
            "a party owing itself",
        ),
        (
            "buyer to seller 1000 JPY",
            "buyer to carrier 1000 JPY",
            "an undeclared party",
        ),
        ("1000 JPY due 200", "1000 USD due 200", "two currencies"),
        (
            "1000 JPY due 200",
            "1000 EUR due 200",
            "a currency the table does not know",
        ),
        ("1000 JPY due 200", "0 JPY due 200", "an obligation of 0"),
        (
            "1000 JPY due 200",
            "-1 JPY due 200",
            "an obligation below 0",
        ),
        ("1000 JPY due 200", "1000 jpy due 200", "not money"),
        ("1000 JPY due 200", "1000 JPY by 200", "not `due`"),
        (
            "due 200\n",
            "due 18446744074\n",
            "a due time after the latest",
        ),
        ("decided by cdcd", "decided bycdcd", "not `decided by`"),
        (
            ": the goods were inspected",
            ":",
            "a condition without a description",
        ),
        (
            "condition inspected",
            "condition deposit",
            "a condition with an obligation's label",
        ),
        (
            "obligation price",
            "obligation buyer",
            "an obligation with a party's label",
        ),
        (
            "created 100\n",
            "created 100\nrequire r: true\n",
            "a line of another kind",
        ),
    ] {
        assert!(!reads(&base.replace(from, to)), "{why}");
    }
    // without its table, or with another, it does not read
    assert!(rule_law_from_file(&base).is_err());
    let other = rule_law_from_file(
        &CURRENCIES.replace("row usd", "row eur: e2 when code == \"EUR\"\nrow usd"),
    )
    .expect("reads");
    assert!(rule_law_from_file_with(&base, Some(&other)).is_err());
    // a contract needs a currency table even without a `currencies` line
    let without: String = without_lines(&base, "currencies");
    assert!(rule_law_from_file_with(&without, Some(&t)).is_err());
    // no obligation
    let none: String = without_lines(&base, "obligation");
    assert!(!reads(&none));
}

/// The text without the lines that start with `word`
fn without_lines(text: &str, word: &str) -> String {
    text.lines()
        .filter(|l| !l.starts_with(word))
        .flat_map(|l| [l, "\n"])
        .collect()
}

#[test]
fn the_identifier_names_every_field() {
    let t = table();
    let base_text = sale_text(&t);
    let id = |s: &str| {
        rule_law_from_file_with(s, Some(&t))
            .expect("reads")
            .law_id()
    };
    let base = id(&base_text);
    for (from, to) in [
        ("party buyer\nparty seller\n", "party seller\nparty buyer\n"),
        ("created 100", "created 101"),
        ("condition inspected", "condition checked"),
        ("decided by cdcd", "decided by cdcc"),
        ("the goods were inspected", "the goods were seen"),
        ("obligation deposit", "obligation down_payment"),
        ("deposit buyer to seller", "deposit seller to buyer"),
        ("1000 JPY due 200", "1001 JPY due 200"),
        ("1000 JPY due 200", "1000 JPY due 201"),
    ] {
        assert_ne!(id(&base_text.replace(from, to)), base, "{from} -> {to}");
    }
    let spaced = base_text.replace("law a demo sale (fixture)\n", "") + "# a comment\n";
    assert_eq!(id(&spaced), base);
    // the currency table is part of it
    let t2 = rule_law_from_file(&CURRENCIES.replace("unknown", "none")).expect("reads");
    let moved = base_text.replace(&hex(t.law_id()), &hex(t2.law_id()));
    assert_ne!(
        rule_law_from_file_with(&moved, Some(&t2))
            .expect("reads")
            .law_id(),
        base
    );
}

#[test]
fn the_contract_keeps_the_shape_alice_legal_holds() {
    let RuleLaw::Contract(c) = sale() else {
        panic!("a contract")
    };
    assert_eq!(c.parties(), ["buyer", "seller"]);
    assert_eq!((c.created(), c.currency()), (100, "JPY"));
    assert_eq!(c.conditions()[0].decided_by(), &[0xcd; 32]);
    let o = &c.obligations()[1];
    assert_eq!(
        (o.label(), o.debtor(), o.creditor(), o.amount(), o.due()),
        ("price", 0, 1, 9_223_372_036_854_775_900, 300)
    );
}

#[test]
fn the_contract_semantics_are_their_own_and_measured() {
    let measured = contract_semantics_fingerprints();
    assert_eq!(measured.len(), CONTRACT_SEMANTICS_PINS.len());
    for ((name, pinned), (m_name, m)) in CONTRACT_SEMANTICS_PINS.iter().zip(&measured) {
        assert_eq!(name, m_name);
        assert_eq!(
            hex(*m),
            hex(*pinned),
            "pin `{name}` is not the measured behaviour"
        );
    }
    assert_eq!(
        hex(alice_lol::law_id::fold_semantics_pins(
            CONTRACT_SEMANTICS_PINS
        )),
        hex(CONTRACT_SEMANTICS_ID)
    );
    assert_ne!(
        CONTRACT_SEMANTICS_ID,
        alice_lol::rule_law::MONEY_SEMANTICS_ID
    );
}

#[test]
fn an_edition_of_a_contract_and_a_delta_over_it() {
    let t = table();
    let head = |ed: &str, sup: &str, sem: [u8; 32]| {
        format!(
            "domain demo-sale\nedition {ed}\nsupersedes {sup}\neffective 100\nsemantics {}\n",
            hex(sem)
        )
    };
    let first = head("v1", "first", CONTRACT_SEMANTICS_ID) + &sale_text(&t);
    let law = rule_law_from_file_with(&first, Some(&t)).expect("reads");
    assert!(rule_law_from_file_with(
        &(head("v1", "first", alice_lol::rule_law::MONEY_SEMANTICS_ID) + &sale_text(&t)),
        Some(&t)
    )
    .is_err());
    let resolved = alice_lol::rule_law::edition_text_with(&first, Some(&t)).expect("resolves");
    let delta = head("v2", &hex(law.law_id()), CONTRACT_SEMANTICS_ID)
        + "replace obligation deposit: obligation deposit buyer to seller 1000 JPY due 250\n\
           changes later_deposit\n";
    let (text, changes) =
        alice_lol::rule_law::resolve_delta_with(&resolved, &delta, Some(&t)).expect("resolves");
    assert_eq!(changes, ["later_deposit"]);
    let later = rule_law_from_file_with(&text, Some(&t)).expect("reads");
    assert_eq!(out(&ask(&later, r#"{"now": 201}"#)).0, "active");
    assert_eq!(out(&ask(&law, r#"{"now": 201}"#)).0, "breached");
}

#[test]
fn a_contract_of_many_obligations_reads_in_linear_time() {
    use std::fmt::Write as _;
    let t = table();
    // reading 4n obligations takes about four times as long as n (sixteen if each label were
    // looked up among the earlier ones); the ratio, not a time, so a slow runner does not fail
    // it, and a bound of 8 leaves a factor of 2 either side
    let text_of = |n: usize| {
        let mut text = format!(
            "kind contract\ncurrencies {}\nparty a\nparty b\ncreated 0\n",
            hex(t.law_id())
        );
        for i in 0..n {
            let _ = writeln!(text, "obligation o{i} a to b 1 JPY due 10");
        }
        text
    };
    let best = |text: &str| {
        (0..3)
            .map(|_| {
                let start = std::time::Instant::now();
                rule_law_from_file_with(text, Some(&t)).expect("reads");
                start.elapsed()
            })
            .min()
            .expect("three runs")
    };
    let (small, large) = (10_000, 40_000);
    let (t_small, t_large) = (best(&text_of(small)), best(&text_of(large)));
    let ratio = t_large.as_secs_f64() / t_small.as_secs_f64().max(1e-9);
    assert!(
        ratio < 8.0,
        "{small} obligations in {t_small:?}, {large} in {t_large:?}: ratio {ratio:.2}"
    );
}
