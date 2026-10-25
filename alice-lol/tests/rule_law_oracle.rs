//! Rule laws (`kind machine` / `kind predicate`, conformance/TASK.md "Rule laws"): reading,
//! answering and the identifier
//!
//! The machine table below is written out by hand from the rule in words (not computed by the
//! code under test). The predicate cases need HMAC-SHA256 and run with the `crypto-calls`
//! feature; the rest runs in every build. The full corpus (every step, every boundary) is
//! scored by `conformance/run_conformance.py --only-rule-laws` against
//! `examples/rule_conformance.rs`.

use std::path::PathBuf;

use alice_lol::law_id::LOL_SEMANTICS_ID;
use alice_lol::law_input::{parse_json, Json};
use alice_lol::rule_law::{
    rule_kind, rule_law_from_file, rule_semantics_fingerprints, Answer, RuleKind, MACHINE_LAW_KIND,
    PREDICATE_LAW_KIND, RULE_SEMANTICS_ID, RULE_SEMANTICS_PINS,
};

fn spike(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../laws/spike")
        .join(format!("{name}.law"));
    std::fs::read_to_string(p).expect("law file")
}

fn json(text: &str) -> Json {
    parse_json(text).expect("test JSON")
}

fn outputs(a: &Answer) -> &Json {
    match a {
        Answer::Outputs(o) => o,
        other => panic!("expected outputs, got {other:?}"),
    }
}

fn text<'a>(o: &'a Json, key: &str) -> &'a str {
    match o.get(key) {
        Some(Json::Text(t)) => t,
        other => panic!("{key}: expected text, got {other:?}"),
    }
}

// ------------------------------------------------------------------ machine

/// (state, event, `device_proof`, new state, taken, analytics in the strict mode)
type Row = (
    &'static str,
    &'static str,
    Option<bool>,
    &'static str,
    bool,
    bool,
);

const STRICT: &[Row] = &[
    ("undecided", "accept", None, "accepted", true, true),
    ("undecided", "decline", None, "declined", true, false),
    (
        "undecided",
        "withdraw",
        Some(false),
        "undecided",
        false,
        false,
    ),
    (
        "undecided",
        "withdraw",
        Some(true),
        "undecided",
        false,
        false,
    ),
    ("accepted", "accept", None, "accepted", false, true),
    ("accepted", "decline", None, "accepted", false, true),
    ("accepted", "withdraw", Some(false), "accepted", false, true),
    ("accepted", "withdraw", Some(true), "declined", true, false),
    ("declined", "accept", None, "accepted", true, true),
    ("declined", "decline", None, "declined", false, false),
    (
        "declined",
        "withdraw",
        Some(false),
        "declined",
        false,
        false,
    ),
    ("declined", "withdraw", Some(true), "declined", false, false),
];

fn step(mode: &str, state: &str, event: &str, proof: Option<bool>) -> String {
    let guards = proof.map_or_else(String::new, |p| format!("\"device_proof\": {p}"));
    format!("{{\"mode\": \"{mode}\", \"state\": \"{state}\", \"event\": \"{event}\", \"guards\": {{{guards}}}}}")
}

#[test]
fn the_pilot_files_are_rule_laws_and_an_audit_file_is_not() {
    assert_eq!(rule_kind(&spike("consent_choice")), Some(RuleKind::Machine));
    assert_eq!(
        rule_kind(&spike("session_cookie_valid")),
        Some(RuleKind::Predicate)
    );
    assert_eq!(rule_kind(&spike("gate_compares_nonzero")), None);
    assert_eq!(rule_kind(&spike("first_order_decay")), None);
    // `kind audit` wins: the audit reading is unchanged whatever other `kind` line is there
    assert_eq!(rule_kind("kind audit\nkind machine\n"), None);
}

#[test]
fn every_step_of_the_consent_machine_in_both_modes() {
    let law = rule_law_from_file(&spike("consent_choice")).expect("reads");
    let mut compared = 0;
    for mode in ["strict", "relaxed"] {
        for &(state, event, proof, new, taken, strict_analytics) in STRICT {
            let a = law.respond(&json(&step(mode, state, event, proof)));
            let o = outputs(&a);
            assert_eq!(text(o, "state"), new, "{mode} {state} {event} {proof:?}");
            assert_eq!(
                text(o, "transition"),
                if taken { "taken" } else { "refused" },
                "{mode} {state} {event} {proof:?}"
            );
            // relaxed: analytics always; strict: only in the accepted state after the step
            let want = mode == "relaxed" || strict_analytics;
            assert_eq!(
                o.get("allow").and_then(|a| a.get("analytics")),
                Some(&Json::Bool(want)),
                "{mode} {state} {event} {proof:?}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 24);
}

#[test]
fn machine_requests_that_are_rejected_or_errors() {
    let law = rule_law_from_file(&spike("consent_choice")).expect("reads");
    let rejected = [
        r#"{"mode": "strict", "state": "nowhere", "event": "accept"}"#,
        r#"{"mode": "strict", "state": 1, "event": "accept"}"#,
        r#"{"mode": "strict", "state": "accepted", "event": "jump"}"#,
        r#"{"mode": "lenient", "state": "accepted", "event": "accept"}"#,
        r#"{"mode": "strict", "state": "accepted", "event": "withdraw"}"#,
        r#"{"mode": "strict", "state": "accepted", "event": "withdraw", "guards": {"device_proof": 1}}"#,
        r#"{"mode": "strict", "state": "accepted", "event": "accept", "guards": []}"#,
    ];
    for r in rejected {
        assert!(matches!(law.respond(&json(r)), Answer::Rejected(_)), "{r}");
    }
    let errors = [
        r#"{"state": "accepted", "event": "accept"}"#,
        r#"{"mode": "strict", "event": "accept"}"#,
        r#"{"mode": "strict", "state": "accepted"}"#,
    ];
    for r in errors {
        assert!(
            matches!(law.respond(&json(r)), Answer::RequestError(_)),
            "{r}"
        );
    }
    // no guards key is `{}`, and keys of other guards are ignored
    let a = law.respond(&json(
        r#"{"mode": "strict", "state": "undecided", "event": "accept", "guards": {"other": 3}}"#,
    ));
    assert_eq!(text(outputs(&a), "state"), "accepted");
}

#[test]
fn the_first_matching_transition_is_taken() {
    let law = rule_law_from_file(
        "kind machine\nstate a initial\nstate b\nstate c\nevent go\n\
         on go from a to b\non go from a to c\n",
    )
    .expect("reads");
    let a = law.respond(&json(r#"{"state": "a", "event": "go"}"#));
    assert_eq!(text(outputs(&a), "state"), "b");
    assert_eq!(outputs(&a).get("allow"), Some(&Json::Object(Vec::new())));
}

// ------------------------------------------------------------------ predicate

#[test]
fn the_cookie_predicate_reads_and_answers_without_the_call_only_when_built_with_it() {
    let law = rule_law_from_file(&spike("session_cookie_valid")).expect("reads in every build");
    let ex = r#"{"cookie": "42.7.1893456000:b13155066f4d53246b4b26c896d79b7f", "now": 1893455000, "key": "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f", "user": {"session_version": 7}}"#;
    let a = law.respond(&json(ex));
    if cfg!(feature = "crypto-calls") {
        assert_eq!(outputs(&a).get("valid"), Some(&Json::Bool(true)));
        assert_eq!(outputs(&a).get("failed"), Some(&Json::Null));
    } else {
        assert!(matches!(a, Answer::Unsupported(_)), "{a:?}");
    }
}

#[cfg(feature = "crypto-calls")]
mod with_hmac {
    use super::*;

    const KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    fn ask(cookie: &str, now: i64, key: &str, version: i64) -> Answer {
        let law = rule_law_from_file(&spike("session_cookie_valid")).expect("reads");
        law.respond(&json(&format!(
            "{{\"cookie\": \"{cookie}\", \"now\": {now}, \"key\": \"{key}\", \"user\": {{\"session_version\": {version}}}}}"
        )))
    }

    fn failed(a: &Answer) -> Option<String> {
        match outputs(a).get("failed") {
            Some(Json::Null) => None,
            Some(Json::Text(t)) => Some(t.clone()),
            other => panic!("failed: {other:?}"),
        }
    }

    const COOKIE: &str = "42.7.1893456000:b13155066f4d53246b4b26c896d79b7f";

    #[test]
    fn the_expiry_is_exclusive() {
        assert_eq!(failed(&ask(COOKIE, 1_893_455_999, KEY, 7)), None);
        assert_eq!(
            failed(&ask(COOKIE, 1_893_456_000, KEY, 7)).as_deref(),
            Some("fresh")
        );
        assert_eq!(
            failed(&ask(COOKIE, 1_893_456_001, KEY, 7)).as_deref(),
            Some("fresh")
        );
    }

    #[test]
    fn the_version_must_be_current() {
        for v in [6, 8] {
            assert_eq!(failed(&ask(COOKIE, 0, KEY, v)).as_deref(), Some("current"));
        }
    }

    #[test]
    fn the_tag_binds_every_part_and_the_key() {
        let cases = [
            // a bit of the tag flipped at either end
            "42.7.1893456000:313155066f4d53246b4b26c896d79b7f",
            "42.7.1893456000:b13155066f4d53246b4b26c896d79b7e",
            // a byte short, a byte long
            "42.7.1893456000:b13155066f4d53246b4b26c896d79b",
            "42.7.1893456000:b13155066f4d53246b4b26c896d79b7f00",
            // another uid, version or expiry under the same tag
            "43.7.1893456000:b13155066f4d53246b4b26c896d79b7f",
            "42.8.1893456000:b13155066f4d53246b4b26c896d79b7f",
            "42.7.1893456001:b13155066f4d53246b4b26c896d79b7f",
        ];
        for c in cases {
            assert_eq!(failed(&ask(c, 0, KEY, 7)).as_deref(), Some("tag"), "{c}");
        }
        let other_key = "ff0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        assert_eq!(
            failed(&ask(COOKIE, 0, other_key, 7)).as_deref(),
            Some("tag")
        );
    }

    #[test]
    fn a_cookie_of_another_shape_does_not_hold() {
        let cases = [
            "42.1893456000:b13155066f4d53246b4b26c896d79b7f", // the old 2-part head
            "42.7.1893456000",                                // no tag
            "42.7.1893456000:b1:31",                          // a separator more
            "42.07.1893456000:b13155066f4d53246b4b26c896d79b7f", // a leading zero
            "42.+7.1893456000:b13155066f4d53246b4b26c896d79b7f", // a plus sign
            "42.x.1893456000:b13155066f4d53246b4b26c896d79b7f", // not a number
            "42.7.1893456000:B13155066F4D53246B4B26C896D79B7F", // upper-case hex
            "42.7.1893456000:b13",                            // odd hex length
            "42.7.9007199254740993:b13155066f4d53246b4b26c896d79b7f", // above 2^53
        ];
        for c in cases {
            assert_eq!(failed(&ask(c, 0, KEY, 7)).as_deref(), Some("shape"), "{c}");
        }
    }

    #[test]
    fn inputs_of_another_type_are_rejected_and_missing_ones_are_errors() {
        let law = rule_law_from_file(&spike("session_cookie_valid")).expect("reads");
        let base =
            format!(r#""cookie": "{COOKIE}", "key": "{KEY}", "user": {{"session_version": 7}}"#);
        for now in ["1.5", "\"1\"", "true", "1e400", "9007199254740993"] {
            let r = format!("{{{base}, \"now\": {now}}}");
            assert!(matches!(law.respond(&json(&r)), Answer::Rejected(_)), "{r}");
        }
        let r = format!(
            r#"{{"cookie": "{COOKIE}", "now": 0, "key": "zz", "user": {{"session_version": 7}}}}"#
        );
        assert!(matches!(law.respond(&json(&r)), Answer::Rejected(_)), "{r}");
        let r = format!(r#"{{"cookie": "{COOKIE}", "now": 0, "key": "{KEY}", "user": {{}}}}"#);
        assert!(matches!(law.respond(&json(&r)), Answer::Rejected(_)), "{r}");
        let r = format!("{{{base}}}");
        assert!(
            matches!(law.respond(&json(&r)), Answer::RequestError(_)),
            "{r}"
        );
    }

    #[test]
    fn hmac_follows_rfc_4231() {
        // RFC 4231 test case 2 through a predicate: key "Jefe", "what do ya want for nothing?"
        let law = rule_law_from_file(
            "kind predicate\nuse alice-sign hmac_sha256\nx-input k bytes\nx-input m text\n\
             require mac: hmac_sha256(k, utf8(m)) == 0x5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843\n\
             example {\"k\": \"4a656665\", \"m\": \"what do ya want for nothing?\"}\n",
        )
        .expect("reads");
        let a = law.respond(&json(
            r#"{"k": "4a656665", "m": "what do ya want for nothing?"}"#,
        ));
        assert_eq!(outputs(&a).get("valid"), Some(&Json::Bool(true)));
    }
}

// ------------------------------------------------------------------ reading

#[test]
fn files_that_do_not_read() {
    let m = "kind machine\nstate a initial\nevent go\non go from a to a\n";
    let p = "kind predicate\nx-input n integer\nrequire pos: n > 0\nexample {\"n\": 1}\n";
    let bad = [
        // an unknown line, a second kind line
        format!("{m}colour red\n"),
        format!("{m}kind predicate\n"),
        // a name declared twice, a reserved word, two initial states, no initial state
        format!("{m}state a\n"),
        format!("{m}state mode\n"),
        format!("{m}state b initial\n"),
        "kind machine\nstate a\nevent go\non go from a to a\n".to_owned(),
        // an `on` line naming an undeclared state or event, a guard of another event
        "kind machine\nstate a initial\nevent go\non go from b to a\n".to_owned(),
        "kind machine\nstate a initial\nevent go\non stop from a to a\n".to_owned(),
        "kind machine\nstate a initial\nevent go\nevent stop guard ok\non go from a to a when ok\n"
            .to_owned(),
        // a mode atom without a mode line, a guard in an allowance
        "kind machine\nstate a initial\nevent go\non go from a to a\nallow x when mode == m\n"
            .to_owned(),
        // predicate: an undeclared name, chained comparisons, order on text, time vs integer
        p.replace("n > 0", "m > 0"),
        p.replace("n > 0", "0 < n < 2"),
        "kind predicate\nx-input t text\nrequire o: t < t\nexample {\"t\": \"a\"}\n".to_owned(),
        "kind predicate\nx-input t time\nrequire o: t < 5\nexample {\"t\": 1}\n".to_owned(),
        // a call without its `use` line, a call nobody provides
        "kind predicate\nx-input k bytes\nrequire o: hmac_sha256(k, k) == k\nexample {\"k\": \"\"}\n"
            .to_owned(),
        format!("{p}use alice-sign ed25519_verify\n"),
        // a require that is not a boolean, no require, no example, an example that fails
        p.replace("n > 0", "n"),
        "kind predicate\nx-input n integer\nexample {\"n\": 1}\n".to_owned(),
        p.replace("example {\"n\": 1}\n", ""),
        p.replace("{\"n\": 1}", "{\"n\": 0}"),
        // an x- line with two spaces, an unknown x- line, an indented x- line
        p.replace("x-input n", "x-input  n"),
        format!("{p}x-metric c = count(n)\n"),
        p.replace("x-input n", " x-input n"),
        // a literal of another type, an integer above 2^53, an odd byte literal
        format!("{p}const c integer = \"1\"\n"),
        format!("{p}const c integer = 9007199254740993\n"),
        format!("{p}const c bytes = 0x0\n"),
        // a control character
        format!("{p}claim a\u{0007}b\n"),
    ];
    for (i, text) in bad.iter().enumerate() {
        assert!(rule_law_from_file(text).is_err(), "case {i} read:\n{text}");
    }
    assert!(rule_law_from_file(m).is_ok());
    assert!(rule_law_from_file(p).is_ok());
}

/// Each operator refuses an operand of the wrong type on either side, by its own message (an
/// error from a later check, such as the example failing, would hide a missing type check)
#[test]
fn operators_refuse_an_operand_of_another_type_on_either_side() {
    let cases = [
        ("n or true", "`or` needs booleans"),
        ("true or n", "`or` needs booleans"),
        ("n and true", "`and` needs booleans"),
        ("true and n", "`and` needs booleans"),
        ("not n", "`not` needs booleans"),
        ("0x01 ++ \"a\" == k", "`++` joins bytes"),
        ("\"a\" ++ 0x01 == k", "`++` joins bytes"),
        ("n ++ 0x01 == k", "`++` joins bytes"),
    ];
    for (expr, message) in cases {
        let text = format!(
            "kind predicate\nx-input n integer\nx-input k bytes\nrequire r: {expr}\n\
             example {{\"n\": 1, \"k\": \"01\"}}\n"
        );
        let e = rule_law_from_file(&text).expect_err(expr).to_string();
        assert!(e.contains(message), "{expr}: {e}");
    }
    // the same operands of the right type read
    for expr in ["n > 0 or true", "true and not false", "0x01 ++ 0x02 == k"] {
        let text = format!(
            "kind predicate\nx-input n integer\nx-input k bytes\nrequire r: {expr}\n\
             example {{\"n\": 1, \"k\": \"0102\"}}\n"
        );
        assert!(rule_law_from_file(&text).is_ok(), "{expr}");
    }
}

/// A parenthesized group overrides the binding of `and` over `or`: `(true or n > 0) and n > 5`
/// fails for n = 1, where the ungrouped `true or (n > 0 and n > 5)` would hold
#[test]
fn a_parenthesized_group_binds_first() {
    let law = |require: &str| {
        rule_law_from_file(&format!(
            "kind predicate\nx-input n integer\nrequire r: {require}\nexample {{\"n\": 6}}\n"
        ))
        .unwrap_or_else(|e| panic!("{require}: {e}"))
    };
    let failed = |l: &alice_lol::rule_law::RuleLaw, n: i64| {
        outputs(&l.respond(&json(&format!("{{\"n\": {n}}}"))))
            .get("failed")
            .cloned()
    };
    let grouped = law("(true or n > 0) and n > 5");
    assert_eq!(failed(&grouped, 1), Some(Json::Text("r".to_owned())));
    assert_eq!(failed(&grouped, 6), Some(Json::Null));
    let ungrouped = law("true or n > 0 and n > 5");
    assert_eq!(failed(&ungrouped, 1), Some(Json::Null));
    // nested and unbalanced groups
    assert_eq!(
        failed(&law("((n > 5))"), 1),
        Some(Json::Text("r".to_owned()))
    );
    for bad in ["(n > 5", "n > 5)", "()"] {
        let text =
            format!("kind predicate\nx-input n integer\nrequire r: {bad}\nexample {{\"n\": 6}}\n");
        assert!(rule_law_from_file(&text).is_err(), "{bad}");
    }
}

#[test]
fn bound_integers_reach_the_largest_and_no_further() {
    let p = "kind predicate\nx-input s text\nx-bind n s integer\nrequire pos: n > 0\nexample {\"s\": \"1\"}\n";
    let law = rule_law_from_file(p).expect("reads");
    let failed = |s: &str| {
        let a = law.respond(&json(&format!("{{\"s\": \"{s}\"}}")));
        outputs(&a).get("failed").cloned()
    };
    assert_eq!(failed("9007199254740991"), Some(Json::Null));
    assert_eq!(
        failed("-9007199254740991"),
        Some(Json::Text("pos".to_owned()))
    );
    for s in [
        "9007199254740992",
        "-9007199254740992",
        "99999999999999999999",
        "",
        "-",
        "-0",
        "00",
    ] {
        assert_eq!(failed(s), Some(Json::Text("shape".to_owned())), "{s}");
    }
}

#[test]
fn a_file_that_does_not_read_says_where() {
    let e = rule_law_from_file(
        "kind machine\nstate a initial\nevent go\non go from a to a\ncolour red\n",
    )
    .expect_err("an unknown line");
    let shown = e.to_string();
    assert_eq!(e.line, 5);
    assert!(shown.starts_with("rule law, line 5: "), "{shown}");
    assert!(shown.contains("colour"), "{shown}");
    let whole = rule_law_from_file("kind machine\n")
        .expect_err("no state")
        .to_string();
    assert!(whole.starts_with("rule law: "), "{whole}");
}

// ------------------------------------------------------------------ identifier

#[test]
fn rule_ids_name_the_rule_not_its_layout() {
    let id = |t: &str| rule_law_from_file(t).expect("reads").law_id();
    let m = "kind machine\nstate a initial\nstate b\nevent go\non go from a to b\n";
    // comments, people lines and spacing are not the rule
    assert_eq!(
        id(m),
        id("# note\nlaw x\nkind machine\nclaim words\nstate  a initial\nstate b\nevent go\non go  from a to b # c\n")
    );
    // the target of a transition, the order of `on` lines and the kind are
    assert_ne!(id(m), id(&m.replace("to b", "to a")));
    let two =
        "kind machine\nstate a initial\nstate b\nevent go\non go from a to b\non go from a to a\n";
    let swapped =
        "kind machine\nstate a initial\nstate b\nevent go\non go from a to a\non go from a to b\n";
    assert_ne!(id(two), id(swapped));
    let p = "kind predicate\nx-input n integer\nrequire pos: n > 0\nexample {\"n\": 1}\n";
    assert_ne!(id(p), id(&p.replace("n > 0", "n >= 0")));
    assert_ne!(id(p), id(&p.replace("pos:", "positive:")));
    // the example is not the rule
    assert_eq!(id(p), id(&p.replace("{\"n\": 1}", "{\"n\": 2}")));
    assert_ne!(MACHINE_LAW_KIND, PREDICATE_LAW_KIND);
}

#[test]
fn the_rule_semantics_are_their_own_and_measured() {
    // the identifier of the older kinds is not touched by the rule kinds
    assert_ne!(RULE_SEMANTICS_ID, LOL_SEMANTICS_ID);
    // every pin is the measured behaviour (recomputed here)
    let measured = rule_semantics_fingerprints();
    for (name, pinned) in RULE_SEMANTICS_PINS {
        if let Some(m) = measured.iter().find(|(n, _)| n == name) {
            assert_eq!(&m.1, pinned, "pin `{name}` is not the measured behaviour");
        } else {
            // the call pin is measured only where the call is built
            assert!(
                name.starts_with("call:") && !cfg!(feature = "crypto-calls"),
                "{name} not measured"
            );
        }
    }
    assert_eq!(
        alice_lol::law_id::fold_semantics_pins(RULE_SEMANTICS_PINS),
        RULE_SEMANTICS_ID
    );
}

/// The v2 identifiers of the rule laws under `laws/`, as `scripts/rule_law_ids.py` writes them from
/// the second implementation (`scripts/law_rules.py`): `(path, kind, hex id)`, and the semantics
/// identifiers the file was written under: `(path, kind, hex id, hex digest of the resolved text
/// for a delta)` rows and `(kind, hex semantics)` rows
type Recorded = (
    Vec<(String, String, String, Option<String>)>,
    Vec<(String, String)>,
);

fn recorded_ids() -> Recorded {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/rule_law_ids_v2.txt");
    let text = std::fs::read_to_string(p).expect("tests/data/rule_law_ids_v2.txt");
    let rows: Vec<Vec<&str>> = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| l.split_whitespace().collect())
        .collect();
    let sem = rows
        .iter()
        .filter(|w| w.len() == 3 && w[0] == "semantics")
        .map(|w| (w[1].to_owned(), w[2].to_owned()))
        .collect();
    let ids = rows
        .iter()
        .filter(|w| w[0] != "semantics")
        .map(|w| {
            assert!(
                w.len() == 3 || w.len() == 4,
                "rule_law_ids_v2.txt: not `<path> <kind> <id> [<digest>]`: {w:?}"
            );
            (
                w[0].to_owned(),
                w[1].to_owned(),
                w[2].to_owned(),
                w.get(3).map(|d| (*d).to_owned()),
            )
        })
        .collect();
    (ids, sem)
}

fn hex(b: [u8; 32]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        use std::fmt::Write as _;
        let _ = write!(s, "{x:02x}");
        s
    })
}

/// The rule laws under `laws/` (any depth), as sorted paths relative to the repository root
fn rule_laws_on_disk(root: &std::path::Path) -> Vec<String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).expect("a directory under laws/") {
            let p = e.expect("entry").path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "law" || x == "delta") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(&root.join("laws"), &mut files);
    let mut out: Vec<String> = files
        .iter()
        .filter(|f| {
            let text = std::fs::read_to_string(f).expect("law file");
            f.extension().is_some_and(|x| x == "delta")
                || alice_lol::rule_law::rule_kind(&text).is_some()
        })
        .map(|f| {
            f.strip_prefix(root)
                .expect("under the root")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    out.sort();
    out
}

#[test]
fn every_rule_law_has_the_identifier_the_second_implementation_computes() {
    let (ids, sem) = recorded_ids();
    assert_eq!(
        sem,
        vec![
            (
                "money_rule".to_owned(),
                hex(alice_lol::rule_law::MONEY_SEMANTICS_ID)
            ),
            (
                "money_table".to_owned(),
                hex(alice_lol::rule_law::MONEY_TABLE_SEMANTICS_ID)
            ),
            ("rule".to_owned(), hex(RULE_SEMANTICS_ID)),
            (
                "table".to_owned(),
                hex(alice_lol::rule_law::TABLE_SEMANTICS_ID)
            ),
        ],
        "the file was written under other semantics identifiers: run scripts/rule_law_ids.py"
    );
    // every rule law under laws/ is listed, so a new one cannot go unchecked
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let listed: Vec<String> = ids.iter().map(|(p, _, _, _)| p.clone()).collect();
    assert_eq!(
        listed,
        rule_laws_on_disk(&root),
        "run scripts/rule_law_ids.py"
    );
    assert!(ids.len() >= 5, "compared {} rule laws", ids.len());
    // the resolved text of each edition, by its identifier, for the deltas that supersede it
    let mut resolved: Vec<([u8; 32], String)> = Vec::new();
    // a money law is read with the currency table its `currencies` line names
    let mut read: Vec<alice_lol::rule_law::RuleLaw> = Vec::new();
    let mut money = 0;
    for (path, kind, want, _) in ids.iter().filter(|r| r.3.is_none()) {
        let text = std::fs::read_to_string(root.join(path)).expect("law file");
        let named = text
            .lines()
            .find_map(|l| l.strip_prefix("currencies "))
            .map(str::trim);
        let table = named.map(|id| {
            money += 1;
            read.iter()
                .find(|t| hex(t.law_id()) == id)
                .unwrap_or_else(|| panic!("{path}: no currency table {id} read before it"))
        });
        let law = alice_lol::rule_law::rule_law_from_file_with(&text, table).expect("reads");
        assert_eq!(hex(law.law_id()), *want, "{path} ({kind})");
        if law.edition().is_some() {
            let r = alice_lol::rule_law::edition_text(&text).expect("resolves");
            resolved.push((law.law_id(), r));
        }
        read.push(law);
    }
    assert!(money >= 1, "no money law compared");
    let mut deltas: Vec<_> = ids.iter().filter(|r| r.3.is_some()).collect();
    assert!(!deltas.is_empty(), "no delta compared");
    while !deltas.is_empty() {
        let before = deltas.len();
        deltas.retain(|(path, kind, want, digest)| {
            let delta = std::fs::read_to_string(root.join(path)).expect("delta file");
            let Some((_, prev)) = resolved
                .iter()
                .find(|(id, _)| delta.contains(&format!("supersedes {}", hex(*id))))
            else {
                return true;
            };
            let (text, _) = alice_lol::rule_law::resolve_delta(prev, &delta).expect("resolves");
            let id = rule_law_from_file(&text).expect("reads").law_id();
            assert_eq!(hex(id), *want, "{path} ({kind})");
            let d: [u8; 32] = <sha2::Sha256 as sha2::Digest>::digest(text.as_bytes()).into();
            assert_eq!(
                Some(hex(d)),
                *digest,
                "{path}: the resolved text differs from the second implementation's"
            );
            resolved.push((id, text));
            false
        });
        assert!(
            deltas.len() < before,
            "a delta supersedes no recorded edition: {deltas:?}"
        );
    }
}

// ------------------------------------------------------------------ editions (TASK.md "Editions of a rule set")

const RATE: &str =
    "kind table\nx-input n integer\nvalue low high\nrow big: high when n > 10\notherwise low\n";

fn rate_header() -> String {
    format!(
        "domain demo-tax\nedition y2025\nsupersedes first\neffective 1735689600 until 1767225600\nsemantics {}\n",
        hex(alice_lol::rule_law::TABLE_SEMANTICS_ID)
    )
}

#[test]
fn an_edition_reads_and_answers_as_the_file_without_its_header() {
    let e = rule_law_from_file(&(rate_header() + RATE)).expect("reads");
    let ed = e.edition().expect("an edition");
    assert_eq!(
        (
            ed.domain(),
            ed.name(),
            ed.supersedes(),
            ed.from(),
            ed.until()
        ),
        (
            "demo-tax",
            "y2025",
            None,
            1_735_689_600,
            Some(1_767_225_600)
        )
    );
    assert_eq!(ed.semantics(), &alice_lol::rule_law::TABLE_SEMANTICS_ID);
    let plain = rule_law_from_file(RATE).expect("reads");
    assert!(plain.edition().is_none());
    for n in [3, 11] {
        let req = alice_lol::law_input::parse_json(&format!(r#"{{"n": {n}}}"#)).expect("json");
        assert_eq!(e.respond(&req), plain.respond(&req), "n = {n}");
    }
}

#[test]
fn the_edition_header_is_part_of_the_identity() {
    let id = |t: &str| rule_law_from_file(t).expect("reads").law_id();
    let text = rate_header() + RATE;
    let base = id(&text);
    assert_ne!(base, id(RATE));
    let other = "ab".repeat(32);
    for (old, new) in [
        ("y2025", "y2026"),
        ("demo-tax", "demo-vat"),
        ("until 1767225600", "until 1767225601"),
        (" until 1767225600", ""),
        ("supersedes first", &*format!("supersedes {other}")),
    ] {
        assert_ne!(base, id(&text.replace(old, new)), "{new}");
    }
    // the position and the spacing of the header lines are not
    let moved = RATE.replace(
        "kind table\n",
        &format!(
            "kind table\n{}",
            rate_header().replace("edition", "edition ")
        ),
    );
    assert_eq!(base, id(&moved));
}

#[test]
fn edition_headers_that_do_not_read() {
    let h = rate_header();
    let sem = hex(alice_lol::rule_law::TABLE_SEMANTICS_ID);
    let bad = [
        h.replace("domain demo-tax\n", ""),
        format!("{h}edition again\n"),
        h.replace("demo-tax", "Demo"),
        h.replace(
            "supersedes first",
            &format!("supersedes {}", "AB".repeat(32)),
        ),
        h.replace("supersedes first", "supersedes 1234"),
        h.replace("until 1767225600", "until 1735689600"),
        h.replace("effective 1735689600", "effective 01735689600"),
        h.replace("1767225600", "9007199254740992"),
        h.replace(&sem, &"11".repeat(32)),
    ];
    for b in bad {
        assert!(rule_law_from_file(&(b.clone() + RATE)).is_err(), "{b}");
    }
}

// ------------------------------------------------------------------ editions as deltas (TASK.md "A later edition written as a delta")

fn e1() -> String {
    format!(
        "law the first edition, written in full\ndomain demo-tax\nedition e1\nsupersedes first\n\
         effective 100 until 200\nsemantics {}\nkind table\nx-input amount integer\n\
         value reduced standard\nrow small: reduced when amount <= 1000   # the bound\notherwise standard\n",
        hex(alice_lol::rule_law::TABLE_SEMANTICS_ID)
    )
}

fn delta(prev: &str, edition: &str, ops: &str, effective: &str) -> String {
    let id = hex(rule_law_from_file(prev).expect("reads").law_id());
    format!(
        "domain demo-tax\nedition {edition}\nsupersedes {id}\neffective {effective}\nsemantics {}\n{ops}",
        hex(alice_lol::rule_law::TABLE_SEMANTICS_ID)
    )
}

#[test]
fn the_resolved_text_of_a_full_edition() {
    let want = format!(
        "domain demo-tax\nedition e1\nsupersedes first\neffective 100 until 200\nsemantics {}\n\
         kind table\nx-input amount integer\nvalue reduced standard\n\
         row small: reduced when amount <= 1000\notherwise standard\n",
        hex(alice_lol::rule_law::TABLE_SEMANTICS_ID)
    );
    assert_eq!(
        alice_lol::rule_law::edition_text(&e1()).expect("resolves"),
        want
    );
}

#[test]
fn a_chain_of_deltas_equals_the_edition_written_in_full() {
    use alice_lol::rule_law::{edition_text, resolve_delta};
    let r1 = edition_text(&e1()).expect("resolves");
    let d2 = delta(
        &r1,
        "e2",
        "replace row small: row small: reduced when amount <= 1200\nchanges mid\n",
        "200 until 300",
    );
    let (r2, ch2) = resolve_delta(&r1, &d2).expect("resolves");
    assert_eq!(ch2, vec!["mid".to_owned()]);
    let d3 = delta(
        &r2,
        "e3",
        "add after row small: row tiny: reduced when amount <= 10\nremove row small\n",
        "300",
    );
    let (r3, _) = resolve_delta(&r2, &d3).expect("resolves");
    let e2_id = hex(rule_law_from_file(&r2).expect("reads").law_id());
    let by_hand = format!(
        "domain demo-tax\nedition e3\nsupersedes {e2_id}\neffective 300\nsemantics {}\n\
         kind table\nx-input amount integer\nvalue reduced standard\n\
         row tiny: reduced when amount <= 10\notherwise standard\n",
        hex(alice_lol::rule_law::TABLE_SEMANTICS_ID)
    );
    assert_eq!(r3, edition_text(&by_hand).expect("resolves"));
    assert_eq!(
        rule_law_from_file(&r3).expect("reads").law_id(),
        rule_law_from_file(&by_hand).expect("reads").law_id()
    );
    let answer = |t: &str| {
        rule_law_from_file(t)
            .expect("reads")
            .respond(&json(r#"{"amount": 1100}"#))
    };
    assert_ne!(
        answer(&r1),
        answer(&r2),
        "the delta changes the answer at 1100"
    );
}

#[test]
fn add_after_and_before_place_the_line() {
    use alice_lol::rule_law::{edition_text, resolve_delta};
    let r1 = edition_text(&e1()).expect("resolves");
    let rows = |t: &str| -> Vec<String> {
        t.lines()
            .filter(|l| l.starts_with("row "))
            .map(|l| l.split_whitespace().nth(1).unwrap_or("").to_owned())
            .collect()
    };
    let add = |side: &str| {
        let ops = format!("add {side} row small: row tiny: reduced when amount <= 10\n");
        resolve_delta(&r1, &delta(&r1, "e2", &ops, "200"))
            .expect("resolves")
            .0
    };
    assert_eq!(rows(&add("after")), ["small:", "tiny:"]);
    assert_eq!(rows(&add("before")), ["tiny:", "small:"]);
}

#[test]
fn two_lines_with_one_key_do_not_resolve() {
    let m = format!(
        "domain demo-tax\nedition e1\nsupersedes first\neffective 100\nsemantics {}\n\
         kind machine\nstate a initial\nstate b\nevent go\non go from a to b\non go from a to a\n",
        hex(RULE_SEMANTICS_ID)
    );
    assert!(rule_law_from_file(&m).is_ok());
    assert!(alice_lol::rule_law::edition_text(&m).is_err());
    let sources = e1().replace("kind table", "source a\nsource b\nkind table");
    assert!(alice_lol::rule_law::edition_text(&sources).is_ok());
}

#[test]
fn independent_operations_commute() {
    use alice_lol::rule_law::{edition_text, resolve_delta};
    let r1 = edition_text(&e1()).expect("resolves");
    let a = "replace row small: row small: reduced when amount <= 999\n";
    let b = "add row big: standard when amount > 5000\n";
    let one = resolve_delta(&r1, &delta(&r1, "e2", &format!("{a}{b}"), "200")).expect("resolves");
    let two = resolve_delta(&r1, &delta(&r1, "e2", &format!("{b}{a}"), "200")).expect("resolves");
    assert_eq!(one, two);
}

#[test]
fn deltas_that_do_not_resolve() {
    use alice_lol::rule_law::{edition_text, resolve_delta};
    let r1 = edition_text(&e1()).expect("resolves");
    let id = hex(rule_law_from_file(&r1).expect("reads").law_id());
    let bad = [
        delta(&r1, "e2", "remove row nothing\n", "200"),
        delta(
            &r1,
            "e2",
            "add row small: reduced when amount <= 5\n",
            "200",
        ),
        delta(
            &r1,
            "e2",
            "replace row small: row big: reduced when amount <= 5\n",
            "200",
        ),
        delta(&r1, "e2", "rename row small\n", "200"),
        delta(&r1, "e2", "", "200").replace(&id, &"ab".repeat(32)),
        delta(&r1, "e2", "", "200").replace("domain demo-tax", "domain demo-vat"),
        delta(&r1, "e2", "add row x: reduced when amount <=\n", "200"),
    ];
    for d in bad {
        assert!(resolve_delta(&r1, &d).is_err(), "{d}");
    }
}

// ------------------------------------------------------------------ rule sets (editions kept, evaluated by date or by identifier)

/// The files of `laws/<domain>/`: `(edition dir, file name, text)` of each `.law` / `.delta`
fn domain_files(domain: &str) -> Vec<(String, String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../laws")
        .join(domain);
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).expect("laws/<domain>") {
        let ed = e.expect("entry").path();
        if !ed.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&ed).expect("edition dir") {
            let f = f.expect("entry").path();
            if f.extension().is_some_and(|x| x == "law" || x == "delta") {
                out.push((
                    ed.file_name().expect("name").to_string_lossy().into_owned(),
                    f.file_name().expect("name").to_string_lossy().into_owned(),
                    std::fs::read_to_string(&f).expect("file"),
                ));
            }
        }
    }
    out.sort();
    out
}

fn rule_set(domain: &str) -> alice_lol::rule_law::RuleSet {
    let files = domain_files(domain);
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(_, n, t)| (n.as_str(), t.as_str()))
        .collect();
    alice_lol::rule_law::RuleSet::from_files(&refs).expect("the rule set resolves")
}

/// A JSON value with every object's keys sorted, for comparing answers whatever their key order
fn sorted(j: &Json) -> Json {
    match j {
        Json::Object(kv) => {
            let mut kv: Vec<(String, Json)> =
                kv.iter().map(|(k, v)| (k.clone(), sorted(v))).collect();
            kv.sort_by(|a, b| a.0.cmp(&b.0));
            Json::Object(kv)
        }
        Json::Array(a) => Json::Array(a.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

fn answer_json(a: &alice_lol::rule_law::Answer) -> Json {
    use alice_lol::rule_law::Answer;
    match a {
        Answer::Outputs(o) => Json::Object(vec![("outputs".to_owned(), sorted(o))]),
        Answer::Rejected(_) => Json::Object(vec![("rejected".to_owned(), Json::Bool(true))]),
        Answer::RequestError(_) => {
            Json::Object(vec![("request_error".to_owned(), Json::Bool(true))])
        }
        Answer::Unsupported(s) => panic!("unsupported: {s}"),
    }
}

/// `(edition dir, name, inputs, frozen answer)` of every example of the domain's frozen corpora
fn frozen(domain: &str) -> Vec<(String, String, Json, Json)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../laws")
        .join(domain);
    let mut out = Vec::new();
    for (ed, _, _) in domain_files(domain) {
        let text =
            std::fs::read_to_string(dir.join(&ed).join("examples.jsonl")).expect("frozen corpus");
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let v = json(line);
            let field = |k: &str| v.get(k).expect(k).clone();
            let Json::Text(name) = field("name") else {
                panic!("name")
            };
            out.push((ed.clone(), name, field("inputs"), sorted(&field("answer"))));
        }
    }
    out
}

#[test]
fn every_frozen_example_is_answered_today_by_its_edition() {
    let set = rule_set("demo-tax");
    let files = domain_files("demo-tax");
    let ids = set.law_ids();
    assert_eq!(ids.len(), files.len());
    let examples = frozen("demo-tax");
    assert!(examples.len() >= 20, "compared {} examples", examples.len());
    for (ed, name, inputs, want) in &examples {
        // the editions are in chain order, and so are the sorted edition directories of the fixture
        let at = files.iter().position(|(d, _, _)| d == ed).expect("edition");
        let (answer, id) = set
            .evaluate_edition(&ids[at], inputs)
            .expect("the edition is kept");
        assert_eq!(id, ids[at]);
        assert_eq!(answer_json(&answer), *want, "{ed} {name}");
    }
}

#[test]
fn two_editions_differ_exactly_where_the_later_one_says() {
    let set = rule_set("demo-tax");
    let ids = set.law_ids();
    let files = domain_files("demo-tax");
    let examples = frozen("demo-tax");
    for w in 0..ids.len() - 1 {
        let (a, b) = (&files[w].0, &files[w + 1].0);
        let mut differ: Vec<String> = examples
            .iter()
            .filter(|(ed, _, _, _)| ed == a || ed == b)
            .filter(|(_, _, inputs, _)| {
                let x = set.evaluate_edition(&ids[w], inputs).expect("kept").0;
                let y = set.evaluate_edition(&ids[w + 1], inputs).expect("kept").0;
                answer_json(&x) != answer_json(&y)
            })
            .map(|(_, n, _, _)| n.clone())
            .collect();
        differ.sort();
        differ.dedup();
        let mut said: Vec<String> = files[w + 1]
            .2
            .lines()
            .filter_map(|l| l.split('#').next())
            .filter_map(|l| l.trim().strip_prefix("changes "))
            .map(|n| n.trim().to_owned())
            .collect();
        said.sort();
        assert!(!differ.is_empty(), "{b} changes no answer of {a}");
        assert_eq!(
            differ, said,
            "{b}: the changed answers are not its `changes` lines"
        );
    }
}

#[test]
fn the_edition_in_force_answers_by_date() {
    let set = rule_set("demo-tax");
    let ids = set.law_ids();
    let req = json(r#"{"amount": 110000}"#);
    let at = |t: u64| set.evaluate_as_of(&req, t);
    // y2025 [1735689600, 1767225600), y2026 [1767225600, 1798761600), y2027 from 1798761600
    assert!(at(1_735_689_599).is_err(), "before the first edition");
    assert_eq!(at(1_735_689_600).expect("in force").1, ids[0]);
    assert_eq!(at(1_767_225_599).expect("in force").1, ids[0]);
    assert_eq!(at(1_767_225_600).expect("in force").1, ids[1]);
    assert_eq!(at(1_798_761_600).expect("in force").1, ids[2]);
    assert_eq!(at(9_007_199_254_740_991).expect("open").1, ids[2]);
    // 110000 is standard in 2025 and reduced in 2026 (the bound moved to 120000)
    let value = |t: u64| answer_json(&at(t).expect("in force").0);
    assert_ne!(value(1_735_689_600), value(1_767_225_600));
    assert!(
        set.evaluate_edition(&[0; 32], &req).is_err(),
        "an unknown identifier is an error"
    );
}

#[test]
fn rule_sets_that_do_not_load() {
    use alice_lol::rule_law::RuleSet;
    let files = domain_files("demo-tax");
    let named: Vec<(&str, &str)> = files
        .iter()
        .map(|(_, n, t)| (n.as_str(), t.as_str()))
        .collect();
    assert!(RuleSet::from_files(&named).is_ok());
    // a delta without the edition it supersedes
    assert!(RuleSet::from_files(&named[1..]).is_err());
    // two first editions
    assert!(RuleSet::from_files(&[named[0], named[0]]).is_err());
    // a gap: the first edition ends a second earlier than the next starts (its identifier moves
    // too, so the second edition is pointed at the first edition rewritten with the gap)
    let gap = named[0].1.replace("until 1767225600", "until 1767225599");
    let id = hex(rule_law_from_file(&gap).expect("reads").law_id());
    let old = hex(rule_law_from_file(named[0].1).expect("reads").law_id());
    let d2 = named[1].1.replace(&old, &id);
    assert!(RuleSet::from_files(&[(named[0].0, gap.as_str()), (named[1].0, d2.as_str())]).is_err());
    assert!(RuleSet::from_files(&[named[0], named[1]]).is_ok());
}

// ------------------------------------------------------------------ money (TASK.md "Types of values" / "The currency table")

const CURRENCIES: &str = "kind table\nx-input code text\nvalue e0 e2 unknown\n\
     row jpy: e0 when code == \"JPY\"\nrow usd: e2 when code == \"USD\"\notherwise unknown\n";

fn currency_table() -> alice_lol::rule_law::RuleLaw {
    rule_law_from_file(CURRENCIES).expect("the currency table reads")
}

fn money_law(kind: &str, body: &str) -> Result<alice_lol::rule_law::RuleLaw, String> {
    let table = currency_table();
    let text = format!("kind {kind}\ncurrencies {}\n{body}", hex(table.law_id()));
    alice_lol::rule_law::rule_law_from_file_with(&text, Some(&table)).map_err(|e| e.to_string())
}

const BIG: i128 = (1 << 63) + 1;
const TOP: i128 = i128::MAX;

fn sum_law() -> alice_lol::rule_law::RuleLaw {
    money_law(
        "predicate",
        &format!(
            "x-input a money\nx-input b money\nconst cap money = {TOP} JPY\n\
             require sum_fits: a + b <= cap\nrequire ordered: a < b\n\
             example {{\"a\": \"1 JPY\", \"b\": \"2 JPY\"}}\n"
        ),
    )
    .expect("reads")
}

fn ask(law: &alice_lol::rule_law::RuleLaw, a: &str, b: &str) -> alice_lol::rule_law::Answer {
    law.respond(&json(&format!(r#"{{"a": {a}, "b": {b}}}"#)))
}

const fn rejected(a: &alice_lol::rule_law::Answer) -> bool {
    matches!(a, alice_lol::rule_law::Answer::Rejected(_))
}

#[test]
fn money_beyond_2_63_compares_and_adds_exactly() {
    use alice_lol::rule_law::Answer;
    let law = sum_law();
    let q = |v: i128| format!("\"{v} JPY\"");
    let valid =
        |a: &Answer| matches!(a, Answer::Outputs(o) if o.get("valid") == Some(&Json::Bool(true)));
    assert!(valid(&ask(&law, &q(BIG), &q(BIG + 1))));
    assert!(!valid(&ask(&law, &q(BIG + 1), &q(BIG))), "ordered fails");
    assert!(!rejected(&ask(&law, &q(TOP - 1), &q(1))));
    assert!(
        rejected(&ask(&law, &q(TOP), &q(1))),
        "a sum beyond 2^127 - 1 rejects"
    );
    assert!(
        rejected(&ask(&law, &q(i128::MIN), &q(-1))),
        "below -2^127 rejects"
    );
    assert!(
        !rejected(&ask(&law, &q(i128::MIN), &q(1))),
        "-2^127 is money"
    );
}

#[test]
fn what_is_not_money() {
    let law = sum_law();
    for v in [
        "\"170141183460469231731687303715884105728 JPY\"",
        "\"-170141183460469231731687303715884105729 JPY\"",
        "\"1 jpy\"",
        "\"1 JP\"",
        "\"1  JPY\"",
        "\"+1 JPY\"",
        "\"01 JPY\"",
        "\"1 EUR\"",
        "\"1JPY\"",
        "5",
        "5.0",
        "null",
        "true",
    ] {
        assert!(rejected(&ask(&law, v, "\"1 JPY\"")), "{v}");
    }
}

#[test]
fn a_code_is_three_upper_case_letters_whatever_the_table_lists() {
    use alice_lol::rule_law::rule_law_from_file_with;
    // a table that lists "jpy" and "JP" does not make them money: the form is checked first
    let odd = rule_law_from_file(&CURRENCIES.replace(
        "row usd",
        "row low: e0 when code == \"jpy\"\nrow two: e0 when code == \"JP\"\nrow usd",
    ))
    .expect("reads");
    let text = format!(
        "kind predicate\ncurrencies {}\nx-input a money\nrequire t: a == a\nexample {{\"a\": \"1 JPY\"}}\n",
        hex(odd.law_id())
    );
    let law = rule_law_from_file_with(&text, Some(&odd)).expect("reads");
    assert!(!rejected(&law.respond(&json(r#"{"a": "1 JPY"}"#))));
    for v in ["1 jpy", "1 JP"] {
        assert!(
            rejected(&law.respond(&json(&format!(r#"{{"a": "{v}"}}"#)))),
            "{v}"
        );
    }
    for c in ["jpy", "JP"] {
        let with = text.replace(
            "require t: a == a",
            &format!("const k money = 1 {c}\nrequire t: a == a"),
        );
        assert!(rule_law_from_file_with(&with, Some(&odd)).is_err(), "{c}");
    }
}

#[test]
fn currencies_do_not_mix() {
    let law = sum_law();
    assert!(rejected(&ask(&law, "\"1 USD\"", "\"2 JPY\"")));
    assert!(money_law(
        "predicate",
        "x-input a money\nconst x money = 1 JPY\nconst y money = 1 USD\nrequire r: x < y\n\
         example {\"a\": \"1 JPY\"}\n"
    )
    .is_err());
    let t = money_law(
        "table",
        "x-input a money\nconst limit money = 100 JPY\nvalue small big\n\
         row s: small when a < limit\notherwise big\n",
    )
    .expect("reads");
    let small = t.respond(&json(r#"{"a": "99 JPY"}"#));
    assert!(
        matches!(&small, alice_lol::rule_law::Answer::Outputs(o) if o.get("value") == Some(&Json::Text("small".to_owned())))
    );
    assert!(rejected(&t.respond(&json(r#"{"a": "99 USD"}"#))));
    // a law without examples: the reading itself refuses, not an example
    assert!(money_law(
        "table",
        "x-input a money\nconst x money = 1 JPY\nconst y money = 1 USD\nvalue small big\n\
         row s: small when a < x or x < y\notherwise big\n",
    )
    .is_err());
    assert!(money_law(
        "table",
        "x-input a money\nconst limit money = 100 EUR\nvalue small big\n\
         row s: small when a < limit\notherwise big\n",
    )
    .is_err());
    // an input in a currency the table does not know is not money, even when nothing mixes
    let only = money_law(
        "predicate",
        "x-input a money\nx-input b money\nrequire o: a < b\n\
         example {\"a\": \"1 JPY\", \"b\": \"2 JPY\"}\n",
    )
    .expect("reads");
    assert!(!rejected(&ask(&only, "\"1 USD\"", "\"2 USD\"")));
    assert!(rejected(&ask(&only, "\"1 EUR\"", "\"2 EUR\"")));
}

#[test]
fn the_currency_table_is_part_of_the_law() {
    use alice_lol::rule_law::rule_law_from_file_with;
    let table = currency_table();
    let text = format!(
        "kind predicate\ncurrencies {}\nx-input a money\nconst zero money = 0 JPY\n\
         require pos: a > zero\nexample {{\"a\": \"1 JPY\"}}\n",
        hex(table.law_id())
    );
    let other = rule_law_from_file(
        &CURRENCIES.replace("row usd", "row eur: e2 when code == \"EUR\"\nrow usd"),
    )
    .expect("reads");
    assert!(
        rule_law_from_file_with(&text, Some(&other)).is_err(),
        "another table"
    );
    assert!(rule_law_from_file(&text).is_err(), "read without its table");
    assert!(rule_law_from_file(
        "kind predicate\nx-input a money\nrequire t: a == a\nexample {\"a\": \"1 JPY\"}\n"
    )
    .is_err());
    assert!(rule_law_from_file_with(
        &format!(
            "kind predicate\ncurrencies {}\nx-input n integer\nrequire t: n == n\nexample {{\"n\": 1}}\n",
            hex(table.law_id())
        ),
        Some(&table)
    )
    .is_err(), "a currencies line without money");
    assert!(rule_law_from_file_with(&text.replace("0 JPY", "0 EUR"), Some(&table)).is_err());
    let base = rule_law_from_file_with(&text, Some(&table))
        .expect("reads")
        .law_id();
    let t2 = rule_law_from_file(&CURRENCIES.replace("unknown", "none")).expect("reads");
    let moved = text.replace(&hex(table.law_id()), &hex(t2.law_id()));
    assert_ne!(
        base,
        rule_law_from_file_with(&moved, Some(&t2))
            .expect("reads")
            .law_id()
    );
    let minus = text.replace("0 JPY", "-1 JPY");
    assert_ne!(
        base,
        rule_law_from_file_with(&minus, Some(&table))
            .expect("reads")
            .law_id()
    );
    let id = |body: &str| money_law("predicate", body).expect("reads").law_id();
    let add = id("x-input a money\nx-input b money\nrequire r: a + b > a\nexample {\"a\": \"1 JPY\", \"b\": \"1 JPY\"}\n");
    let sub = id("x-input a money\nx-input b money\nrequire r: a - b < a\nexample {\"a\": \"1 JPY\", \"b\": \"1 JPY\"}\n");
    assert_ne!(add, sub);
}

#[test]
fn an_operation_not_evaluated_does_not_reject() {
    let law = money_law(
        "predicate",
        "x-input a money\nx-input b money\nx-input go bool\n\
         require r: not go or a + b > a\nexample {\"a\": \"1 JPY\", \"b\": \"1 JPY\", \"go\": true}\n",
    )
    .expect("reads");
    let q = |go: bool| {
        law.respond(&json(&format!(
            r#"{{"a": "1 JPY", "b": "1 USD", "go": {go}}}"#
        )))
    };
    assert!(!rejected(&q(false)));
    assert!(rejected(&q(true)));
}

#[test]
fn money_is_hashed_under_its_own_measured_semantics() {
    use alice_lol::law_id::fold_semantics_pins;
    use alice_lol::rule_law::{
        money_semantics_fingerprints, MONEY_SEMANTICS_ID, MONEY_SEMANTICS_PINS,
        MONEY_TABLE_SEMANTICS_ID, MONEY_TABLE_SEMANTICS_PINS, TABLE_SEMANTICS_ID,
    };
    let measured = money_semantics_fingerprints();
    assert_eq!(measured.len(), MONEY_SEMANTICS_PINS.len());
    for ((name, pinned), (m_name, m)) in MONEY_SEMANTICS_PINS.iter().zip(&measured) {
        assert_eq!(name, m_name);
        assert_eq!(
            hex(*m),
            hex(*pinned),
            "pin `{name}` is not the measured behaviour"
        );
    }
    assert_eq!(
        hex(fold_semantics_pins(MONEY_SEMANTICS_PINS)),
        hex(MONEY_SEMANTICS_ID)
    );
    assert_eq!(
        hex(fold_semantics_pins(MONEY_TABLE_SEMANTICS_PINS)),
        hex(MONEY_TABLE_SEMANTICS_ID)
    );
    // four distinct semantics: a law with money never shares one with a law without
    let all = [
        RULE_SEMANTICS_ID,
        TABLE_SEMANTICS_ID,
        MONEY_SEMANTICS_ID,
        MONEY_TABLE_SEMANTICS_ID,
    ];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn a_rule_set_of_predicate_editions_with_a_delta_loads() {
    use alice_lol::rule_law::{RuleSet, RULE_SEMANTICS_ID};
    let first = format!(
        "domain demo-age\nedition y2025\nsupersedes first\neffective 1735689600 until 1767225600\n\
         semantics {}\nkind predicate\nx-input age integer\nconst adult integer = 20\n\
         require old_enough: age >= adult\nexample {{\"age\": 30}}\n",
        hex(RULE_SEMANTICS_ID)
    );
    let id = rule_law_from_file(&first).expect("reads").law_id();
    let delta = format!(
        "domain demo-age\nedition y2026\nsupersedes {}\neffective 1767225600\nsemantics {}\n\
         replace const adult: const adult integer = 18\nchanges eighteen\n",
        hex(id),
        hex(RULE_SEMANTICS_ID)
    );
    let set = RuleSet::from_files(&[("age.law", first.as_str()), ("age.delta", delta.as_str())])
        .expect("a predicate rule set loads");
    let (later, _) = set
        .evaluate_as_of(&json(r#"{"age": 19}"#), 1_767_225_600)
        .expect("in force");
    let (earlier, _) = set
        .evaluate_as_of(&json(r#"{"age": 19}"#), 1_735_689_600)
        .expect("in force");
    assert_ne!(later, earlier);
}

#[test]
fn an_edition_with_money_names_the_money_semantics_and_a_rule_set_reads_it_with_its_table() {
    use alice_lol::rule_law::{
        resolve_delta, resolve_delta_with, rule_law_from_file_with, RuleSet, MONEY_SEMANTICS_ID,
        RULE_SEMANTICS_ID,
    };
    let table = currency_table();
    let body = format!(
        "kind predicate\ncurrencies {}\nx-input fee money\nconst cap money = 1000 JPY\n\
         require within: fee <= cap\nexample {{\"fee\": \"1 JPY\"}}\n",
        hex(table.law_id())
    );
    let head = |sem: [u8; 32]| {
        format!(
            "domain demo-fee\nedition y2025\nsupersedes first\neffective 1735689600 until 1767225600\n\
             semantics {}\n",
            hex(sem)
        )
    };
    let first = head(MONEY_SEMANTICS_ID) + &body;
    let law = rule_law_from_file_with(&first, Some(&table)).expect("reads");
    assert!(
        rule_law_from_file_with(&(head(RULE_SEMANTICS_ID) + &body), Some(&table)).is_err(),
        "a law with money is not evaluated under the rule semantics"
    );
    let delta = format!(
        "domain demo-fee\nedition y2026\nsupersedes {}\neffective 1767225600\nsemantics {}\n\
         replace const cap: const cap money = 2000 JPY\nchanges raised\n",
        hex(law.law_id()),
        hex(MONEY_SEMANTICS_ID)
    );
    let resolved = alice_lol::rule_law::edition_text_with(&first, Some(&table)).expect("resolves");
    assert!(
        resolve_delta(&resolved, &delta).is_err(),
        "read without its table"
    );
    assert!(resolve_delta_with(&resolved, &delta, Some(&table)).is_ok());
    assert!(
        resolve_delta_with(
            &resolved,
            &delta.replace(&hex(MONEY_SEMANTICS_ID), &hex(RULE_SEMANTICS_ID)),
            Some(&table)
        )
        .is_err(),
        "the delta names the rule semantics"
    );
    let files = [("fee.law", first.as_str()), ("fee.delta", delta.as_str())];
    assert!(
        RuleSet::from_files(&files).is_err(),
        "a rule set with money needs its table"
    );
    let set = RuleSet::from_files_with(&files, Some(&table)).expect("loads");
    let q = json(r#"{"fee": "1500 JPY"}"#);
    let (later, _) = set.evaluate_as_of(&q, 1_767_225_600).expect("in force");
    let (earlier, _) = set.evaluate_as_of(&q, 1_735_689_600).expect("in force");
    assert_ne!(later, earlier, "the raised cap answers differently");
}

#[test]
fn an_edition_of_a_table_with_money_names_the_money_table_semantics() {
    use alice_lol::rule_law::{
        rule_law_from_file_with, MONEY_TABLE_SEMANTICS_ID, TABLE_SEMANTICS_ID,
    };
    let table = currency_table();
    let text = |sem: [u8; 32]| {
        format!(
            "domain demo-band\nedition y2025\nsupersedes first\neffective 1735689600\nsemantics {}\n\
             kind table\ncurrencies {}\nx-input fee money\nconst small_max money = 1000 JPY\n\
             value small large\nrow small: small when fee <= small_max\notherwise large\n",
            hex(sem),
            hex(table.law_id())
        )
    };
    assert!(rule_law_from_file_with(&text(MONEY_TABLE_SEMANTICS_ID), Some(&table)).is_ok());
    assert!(rule_law_from_file_with(&text(TABLE_SEMANTICS_ID), Some(&table)).is_err());
}
