//! Statute laws (conformance/TASK.md "Statute laws"): the clauses in force at a time, the shapes a
//! reader refuses, what the identifier covers, and editions of a statute
//!
//! oracle: the answers are written from the contract (a clause is in force from its start,
//! inclusive, until its end, exclusive), not from this reader's output

use alice_lol::law_input::{parse_json, Json};
use alice_lol::rule_law::{
    resolve_delta, rule_law_from_file, statute_semantics_fingerprints, Answer, ClauseKind,
    RuleKind, RuleLaw, StatuteClause, MAX_LEGAL_TIME, STATUTE_SEMANTICS_ID, STATUTE_SEMANTICS_PINS,
};

fn hex(b: [u8; 32]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        use std::fmt::Write as _;
        let _ = write!(s, "{x:02x}");
        s
    })
}

const ACT: &str = "law a demo act (fixture)\nkind statute\ntitle Demo Act\n\
     clause pay obligation from 100 until 200: the buyer pays within 30 days\n\
     clause waive exception of pay from 150: unless the seller waives payment\n\
     clause late penalty of pay from 100: a late payment bears interest\n\
     clause term definition: buyer means the person who orders\n";

fn in_force(law: &RuleLaw, at: &str) -> Answer {
    law.respond(&parse_json(&format!(r#"{{"at": {at}}}"#)).expect("JSON"))
}

fn labels(a: &Answer) -> Vec<String> {
    match a {
        Answer::Outputs(o) => match o.get("in_force") {
            Some(Json::Array(v)) => v
                .iter()
                .map(|x| match x {
                    Json::Text(t) => t.clone(),
                    other => panic!("not a label: {other:?}"),
                })
                .collect(),
            other => panic!("no in_force: {other:?}"),
        },
        other => panic!("not outputs: {other:?}"),
    }
}

#[test]
fn the_clauses_in_force_at_a_time_in_file_order() {
    let law = rule_law_from_file(ACT).expect("reads");
    assert_eq!(law.kind(), RuleKind::Statute);
    let want: [(&str, &[&str]); 7] = [
        ("0", &["term"]),
        ("99", &["term"]),
        ("100", &["pay", "late", "term"]),
        ("149", &["pay", "late", "term"]),
        ("150", &["pay", "waive", "late", "term"]),
        ("199", &["pay", "waive", "late", "term"]),
        ("200", &["waive", "late", "term"]),
    ];
    for (at, w) in want {
        assert_eq!(labels(&in_force(&law, at)), w, "at {at}");
    }
    assert_eq!(
        labels(&in_force(&law, "9007199254740991")),
        ["waive", "late", "term"]
    );
}

#[test]
fn a_request_without_at_is_an_error_and_one_of_another_type_is_rejected() {
    let law = rule_law_from_file(ACT).expect("reads");
    assert!(matches!(
        law.respond(&parse_json("{}").expect("JSON")),
        Answer::RequestError(_)
    ));
    for v in ["1.5", "\"100\"", "null", "true", "9007199254740992"] {
        assert!(matches!(in_force(&law, v), Answer::Rejected(_)), "at {v}");
    }
    assert_eq!(labels(&in_force(&law, "100.0")), ["pay", "late", "term"]);
    // a time before 1970 is a time: no clause is in force then (not the clauses at its magnitude)
    assert_eq!(labels(&in_force(&law, "-150")), Vec::<String>::new());
    assert_eq!(
        labels(&in_force(&law, "-9007199254740991")),
        Vec::<String>::new()
    );
}

#[test]
fn the_clauses_keep_the_shape_alice_legal_holds() {
    let RuleLaw::Statute(s) = rule_law_from_file(ACT).expect("reads") else {
        panic!("a statute")
    };
    assert_eq!(s.title(), "Demo Act");
    let c = s.clauses();
    assert_eq!(c.len(), 4);
    assert_eq!(
        c.iter().map(StatuteClause::kind).collect::<Vec<_>>(),
        [
            ClauseKind::Obligation,
            ClauseKind::Exception,
            ClauseKind::Penalty,
            ClauseKind::Definition
        ]
    );
    assert_eq!(
        c.iter().map(StatuteClause::parent).collect::<Vec<_>>(),
        [None, Some(0), Some(0), None]
    );
    assert_eq!((c[0].from(), c[0].until()), (100, Some(200)));
    assert_eq!((c[3].from(), c[3].until()), (0, None));
    assert_eq!(c[1].text(), "unless the seller waives payment");
    // every time's nanoseconds fit 64 bits
    assert_eq!(MAX_LEGAL_TIME, 18_446_744_073);
    assert!(MAX_LEGAL_TIME.checked_mul(1_000_000_000).is_some());
    assert!((MAX_LEGAL_TIME + 1).checked_mul(1_000_000_000).is_none());
}

#[test]
fn what_does_not_read() {
    let base = "kind statute\ntitle T\nclause a obligation: x\n";
    assert!(rule_law_from_file(base).is_ok());
    for (bad, why) in [
        ("kind statute\nclause a obligation: x\n", "no title"),
        (
            "kind statute\ntitle T\ntitle U\nclause a obligation: x\n",
            "two titles",
        ),
        (
            "kind statute\ntitle\nclause a obligation: x\n",
            "an empty title",
        ),
        ("kind statute\ntitle T\n", "no clause"),
        (
            "kind statute\ntitle T\nclause a duty: x\n",
            "not a clause kind",
        ),
        ("kind statute\ntitle T\nclause a obligation:\n", "no text"),
        ("kind statute\ntitle T\nclause a obligation x\n", "no colon"),
        (
            "kind statute\ntitle T\nclause A obligation: x\n",
            "not a label",
        ),
        (
            "kind statute\ntitle T\nclause a obligation: x\nclause a permission: y\n",
            "a second a",
        ),
        (
            "kind statute\ntitle T\nclause e exception: x\n",
            "an exception of nothing",
        ),
        (
            "kind statute\ntitle T\nclause e exception of a: x\nclause a obligation: y\n",
            "of a later clause",
        ),
        (
            "kind statute\ntitle T\nclause a obligation from 5 until 5: x\n",
            "until not after from",
        ),
        (
            "kind statute\ntitle T\nclause a obligation until 0: x\n",
            "until 0 from 0",
        ),
        (
            "kind statute\ntitle T\nclause a obligation from 18446744074: x\n",
            "after the latest time",
        ),
        (
            "kind statute\ntitle T\nclause a obligation until 18446744074: x\n",
            "until after the latest time",
        ),
        (
            "kind statute\ntitle T\nclause a obligation from 01: x\n",
            "a leading zero",
        ),
        (
            "kind statute\ntitle T\nclause a obligation until 9 from 1: x\n",
            "until before from",
        ),
        (
            "kind statute\ntitle T\nclause a obligation from 1 extra: x\n",
            "an extra word",
        ),
        (
            "kind statute\ntitle T\nclause a obligation: x\nrequire r: true\n",
            "a line of another kind",
        ),
    ] {
        assert!(rule_law_from_file(bad).is_err(), "{why}");
    }
    assert!(rule_law_from_file(
        "kind statute\ntitle T\nclause a obligation from 18446744073 : x\n"
    )
    .is_ok());
    // a label is any lower-case name, words reserved in expressions included
    assert!(rule_law_from_file("kind statute\ntitle T\nclause initial obligation: x\nclause and_2 exception of initial: y\n").is_ok());
}

#[test]
fn the_identifier_names_every_clause_field_and_not_the_layout() {
    let id = |t: &str| rule_law_from_file(t).expect("reads").law_id();
    let base = id(ACT);
    for (from, to) in [
        ("title Demo Act", "title Demo Act 2"),
        ("clause term definition", "clause terms definition"),
        ("clause term definition", "clause term permission"),
        ("waive exception of pay", "waive exception of late"),
        ("from 100 until 200", "from 100 until 201"),
        ("from 100 until 200", "from 101 until 200"),
        ("from 100 until 200", "from 100"),
        ("within 30 days", "within 31 days"),
        ("clause late penalty of pay", "clause late penalty"),
    ] {
        let text = if from == "waive exception of pay" {
            // `late` must come before `waive` to be its parent
            ACT.replace(
                "clause waive exception of pay from 150: unless the seller waives payment\n\
                 clause late penalty of pay from 100: a late payment bears interest\n",
                "clause late penalty of pay from 100: a late payment bears interest\n\
                 clause waive exception of late from 150: unless the seller waives payment\n",
            )
        } else {
            ACT.replace(from, to)
        };
        assert_ne!(id(&text), base, "{from} -> {to}");
    }
    // the order of the clauses is part of it
    assert_ne!(
        id("kind statute\ntitle T\nclause a obligation: x\nclause b obligation: y\n"),
        id("kind statute\ntitle T\nclause b obligation: y\nclause a obligation: x\n")
    );
    // comments, people lines and spacing are not
    let spaced = ACT
        .replace("law a demo act (fixture)\n", "")
        .replace("clause term definition:", "clause\tterm  definition :")
        + "# a comment\n";
    assert_eq!(id(&spaced), base);
    // a clause without `from` takes the start 0, the same as `from 0`
    assert_eq!(
        id("kind statute\ntitle T\nclause a obligation: x\n"),
        id("kind statute\ntitle T\nclause a obligation from 0: x\n")
    );
}

#[test]
fn the_statute_semantics_are_their_own_and_measured() {
    let measured = statute_semantics_fingerprints();
    assert_eq!(measured.len(), STATUTE_SEMANTICS_PINS.len());
    for ((name, pinned), (m_name, m)) in STATUTE_SEMANTICS_PINS.iter().zip(&measured) {
        assert_eq!(name, m_name);
        assert_eq!(
            hex(*m),
            hex(*pinned),
            "pin `{name}` is not the measured behaviour"
        );
    }
    assert_eq!(
        hex(alice_lol::law_id::fold_semantics_pins(
            STATUTE_SEMANTICS_PINS
        )),
        hex(STATUTE_SEMANTICS_ID)
    );
    for other in [
        alice_lol::rule_law::RULE_SEMANTICS_ID,
        alice_lol::rule_law::TABLE_SEMANTICS_ID,
        alice_lol::rule_law::MONEY_SEMANTICS_ID,
        alice_lol::rule_law::MONEY_TABLE_SEMANTICS_ID,
    ] {
        assert_ne!(other, STATUTE_SEMANTICS_ID);
    }
}

fn header(edition: &str, sup: &str, eff: &str) -> String {
    format!(
        "domain demo-act\nedition {edition}\nsupersedes {sup}\neffective {eff}\nsemantics {}\n",
        hex(STATUTE_SEMANTICS_ID)
    )
}

#[test]
fn an_edition_of_a_statute_and_a_delta_over_it() {
    let first = header("y2025", "first", "100 until 300") + ACT;
    let law = rule_law_from_file(&first).expect("reads");
    // a clause without `from` takes the edition's start
    let RuleLaw::Statute(s) = &law else {
        panic!("a statute")
    };
    assert_eq!(s.clauses()[3].from(), 100);
    assert_eq!(labels(&in_force(&law, "99")), Vec::<String>::new());
    // under another semantics it does not read
    assert!(rule_law_from_file(&first.replace(
        &hex(STATUTE_SEMANTICS_ID),
        &hex(alice_lol::rule_law::RULE_SEMANTICS_ID)
    ))
    .is_err());
    let resolved = alice_lol::rule_law::edition_text(&first).expect("resolves");
    let delta = header("y2026", &hex(law.law_id()), "300")
        + "replace clause pay: clause pay obligation from 300: the buyer pays within 14 days\n\
           remove clause late\nchanges shorter\n";
    let (text, changes) = resolve_delta(&resolved, &delta).expect("resolves");
    assert_eq!(changes, ["shorter"]);
    let later = rule_law_from_file(&text).expect("reads");
    assert_eq!(labels(&in_force(&later, "300")), ["pay", "waive", "term"]);
    // removing the parent of an exception leaves the exception of nothing: it does not resolve
    let orphan = header("y2026", &hex(law.law_id()), "300") + "remove clause pay\nchanges gone\n";
    assert!(resolve_delta(&resolved, &orphan).is_err());
}

#[test]
fn a_statute_has_no_money() {
    assert!(rule_law_from_file(&format!(
        "kind statute\ncurrencies {}\ntitle T\nclause a obligation: x\n",
        "0".repeat(64)
    ))
    .is_err());
}

#[test]
fn a_statute_of_many_clauses_reads_in_linear_time() {
    use std::fmt::Write as _;
    // a label is looked up once per clause (its uniqueness, the clause it is `of`): reading n
    // clauses is linear in n, 100,000 well within seconds even in a debug build
    let n = 100_000;
    let mut text = String::from("kind statute\ntitle T\nclause c0 obligation: x\n");
    for i in 1..n {
        let _ = writeln!(text, "clause c{i} exception of c{}: y", i - 1);
    }
    let start = std::time::Instant::now();
    let law = rule_law_from_file(&text).expect("reads");
    let took = start.elapsed();
    assert!(took.as_secs() < 10, "read {n} clauses in {took:?}");
    let RuleLaw::Statute(s) = law else {
        panic!("a statute")
    };
    assert_eq!(s.clauses()[n - 1].parent(), Some(n - 2));
}
