//! Procedure laws (conformance/TASK.md "Procedure (`kind procedure`)"): a machine over the statuses
//! and step kinds of an administrative procedure, each step leading where ALICE-Legal's
//! `Procedure::add_step` leads
//!
//! oracle: the status table is written from ALICE-Legal 9490cf2 `src/procedure.rs` (`add_step`:
//! approval → approved, rejection → rejected, completion → completed, any other step: pending →
//! `in_progress`, any other status unchanged), not from this reader

use alice_lol::law_input::{parse_json, Json};
use alice_lol::rule_law::{
    next_status, procedure_semantics_fingerprints, rule_law_from_file, Answer, RuleKind,
    PROCEDURE_SEMANTICS_ID, PROCEDURE_SEMANTICS_PINS, PROCEDURE_STATUSES, STEP_KINDS,
};

fn hex(b: [u8; 32]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        use std::fmt::Write as _;
        let _ = write!(s, "{x:02x}");
        s
    })
}

const PERMIT: &str = "law a demo permit procedure (fixture)\nkind procedure\n\
     state pending initial\nstate in_progress\nstate approved\nstate rejected\nstate completed\n\
     event application\nevent review guard complete\nevent approval\nevent rejection\n\
     event notification\nevent completion\n\
     on application from pending to in_progress\n\
     on review from in_progress to in_progress when complete\n\
     on approval from in_progress to approved\non rejection from in_progress to rejected\n\
     on notification from approved to approved\non notification from rejected to rejected\n\
     on completion from approved rejected to completed\n";

#[test]
fn the_status_a_step_leads_to_is_alice_legal_s() {
    let want = |s: &str, k: &str| -> &'static str {
        match k {
            "approval" => "approved",
            "rejection" => "rejected",
            "completion" => "completed",
            _ => match s {
                "pending" | "in_progress" => "in_progress",
                "approved" => "approved",
                "rejected" => "rejected",
                _ => "completed",
            },
        }
    };
    assert_eq!(PROCEDURE_STATUSES.len(), 5);
    assert_eq!(STEP_KINDS.len(), 7);
    for s in PROCEDURE_STATUSES {
        for k in STEP_KINDS {
            assert_eq!(next_status(s, k), want(s, k), "{s} {k}");
        }
    }
}

fn step(law: &alice_lol::rule_law::RuleLaw, req: &str) -> Answer {
    law.respond(&parse_json(req).expect("JSON"))
}

#[test]
fn a_procedure_answers_as_a_machine_over_its_steps() {
    let law = rule_law_from_file(PERMIT).expect("reads");
    assert_eq!(law.kind(), RuleKind::Procedure);
    let to = |a: &Answer| match a {
        Answer::Outputs(o) => o.get("state").cloned(),
        other => panic!("not outputs: {other:?}"),
    };
    let t = |s: &str| Some(Json::Text(s.to_owned()));
    assert_eq!(
        to(&step(
            &law,
            r#"{"state": "pending", "event": "application"}"#
        )),
        t("in_progress")
    );
    assert_eq!(
        to(&step(
            &law,
            r#"{"state": "in_progress", "event": "review", "guards": {"complete": true}}"#
        )),
        t("in_progress")
    );
    assert_eq!(
        to(&step(
            &law,
            r#"{"state": "in_progress", "event": "approval"}"#
        )),
        t("approved")
    );
    assert_eq!(
        to(&step(
            &law,
            r#"{"state": "approved", "event": "completion"}"#
        )),
        t("completed")
    );
    // a step the law does not allow in a status is refused: the state stays
    let refused = step(&law, r#"{"state": "pending", "event": "approval"}"#);
    assert_eq!(to(&refused), t("pending"));
    assert!(
        matches!(&refused, Answer::Outputs(o) if o.get("transition") == Some(&Json::Text("refused".to_owned())))
    );
    assert!(matches!(
        step(&law, r#"{"state": "draft", "event": "application"}"#),
        Answer::Rejected(_)
    ));
}

#[test]
fn a_procedure_that_goes_where_alice_legal_does_not_go_does_not_read() {
    assert!(rule_law_from_file(PERMIT).is_ok());
    for (from, to, why) in [
        (
            "on approval from in_progress to approved\n",
            "on approval from in_progress to completed\n",
            "an approval that completes",
        ),
        (
            "on application from pending to in_progress\n",
            "on application from pending to pending\n",
            "an application that stays pending",
        ),
        (
            "on notification from approved to approved\n",
            "on notification from approved to in_progress\n",
            "a notification that reopens",
        ),
        (
            "on completion from approved rejected to completed\n",
            "on completion from approved rejected to rejected\n",
            "a completion that rejects",
        ),
        (
            "state pending initial\nstate in_progress\n",
            "state pending\nstate in_progress initial\n",
            "a start other than pending",
        ),
        (
            "state completed\n",
            "state completed\nstate archived\n",
            "a status ALICE-Legal has not",
        ),
        (
            "event completion\n",
            "event completion\nevent audit\n",
            "a step kind ALICE-Legal has not",
        ),
        (
            "kind procedure\n",
            "kind procedure\nconst k integer = 1\n",
            "a constant is fine",
        ),
    ] {
        let text = PERMIT.replace(from, to);
        assert_eq!(
            rule_law_from_file(&text).is_ok(),
            why == "a constant is fine",
            "{why}"
        );
    }
    // one `from` list with a status that leads elsewhere does not read
    assert!(rule_law_from_file(&PERMIT.replace(
        "on notification from rejected to rejected\n",
        "on notification from rejected approved to rejected\n"
    ))
    .is_err());
}

#[test]
fn a_procedure_is_not_the_same_law_as_the_same_machine() {
    let p = rule_law_from_file(PERMIT).expect("reads");
    let m = rule_law_from_file(&PERMIT.replace("kind procedure", "kind machine")).expect("reads");
    assert_eq!(m.kind(), RuleKind::Machine);
    assert_ne!(p.law_id(), m.law_id());
    for r in [
        r#"{"state": "pending", "event": "application"}"#,
        r#"{"state": "in_progress", "event": "rejection"}"#,
        r#"{"state": "approved", "event": "notification"}"#,
    ] {
        assert_eq!(
            format!("{:?}", step(&p, r)),
            format!("{:?}", step(&m, r)),
            "{r}"
        );
    }
}

#[test]
fn the_procedure_semantics_are_their_own_and_measured() {
    let measured = procedure_semantics_fingerprints();
    assert_eq!(measured.len(), PROCEDURE_SEMANTICS_PINS.len());
    for ((name, pinned), (m_name, m)) in PROCEDURE_SEMANTICS_PINS.iter().zip(&measured) {
        assert_eq!(name, m_name);
        assert_eq!(
            hex(*m),
            hex(*pinned),
            "pin `{name}` is not the measured behaviour"
        );
    }
    assert_eq!(
        hex(alice_lol::law_id::fold_semantics_pins(
            PROCEDURE_SEMANTICS_PINS
        )),
        hex(PROCEDURE_SEMANTICS_ID)
    );
    for other in [
        alice_lol::rule_law::RULE_SEMANTICS_ID,
        alice_lol::rule_law::TABLE_SEMANTICS_ID,
        alice_lol::rule_law::STATUTE_SEMANTICS_ID,
    ] {
        assert_ne!(other, PROCEDURE_SEMANTICS_ID);
    }
}

#[test]
fn an_edition_of_a_procedure_names_the_procedure_semantics() {
    let head = |sem: [u8; 32]| {
        format!(
            "domain demo-permit\nedition y2025\nsupersedes first\neffective 0\nsemantics {}\n",
            hex(sem)
        )
    };
    assert!(rule_law_from_file(&(head(PROCEDURE_SEMANTICS_ID) + PERMIT)).is_ok());
    assert!(rule_law_from_file(&(head(alice_lol::rule_law::RULE_SEMANTICS_ID) + PERMIT)).is_err());
}
