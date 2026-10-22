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

/// The identifiers of the rule laws of `laws/spike`, recorded when the rule kinds were added
/// (a change of the rule, of its encoding or of [`RULE_SEMANTICS_ID`] moves them)
const SPIKE_IDS: &[(&str, &str)] = &[
    (
        "consent_choice",
        "6c399782ba49f09dfd8f67ef4e0e1008d104c002850d3e2298014897f8371380",
    ),
    (
        "display_name_valid",
        "48832a1e84930b3ea43678016ac50c398c2a139a12111a1ed0cdec37e3060121",
    ),
    (
        "session_cookie_valid",
        "910089651f52f2488371f9bb4b6531a0a49b0e27fe7786e3be277cce337ac9bf",
    ),
    (
        "turnstile_override",
        "2b44332d7a08df9fd5523d890329f41877ddd4ff293ee68baca945322294b39c",
    ),
];

#[test]
fn the_spike_rule_laws_keep_their_identifiers() {
    let hex = |b: [u8; 32]| {
        b.iter().fold(String::new(), |mut s, x| {
            use std::fmt::Write as _;
            let _ = write!(s, "{x:02x}");
            s
        })
    };
    let got: Vec<String> = SPIKE_IDS
        .iter()
        .map(|(name, _)| hex(rule_law_from_file(&spike(name)).expect("reads").law_id()))
        .collect();
    for ((name, _), got) in SPIKE_IDS.iter().zip(&got) {
        println!("ID {name} {got}");
    }
    for ((name, want), got) in SPIKE_IDS.iter().zip(&got) {
        assert_eq!(got, want, "{name}");
    }
}
