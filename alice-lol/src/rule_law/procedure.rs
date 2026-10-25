//! Procedure laws (`kind procedure`, conformance/TASK.md "Procedure (`kind procedure`)"): a state
//! machine whose states are the statuses of an administrative procedure and whose events are the
//! kinds of its steps, the ones ALICE-Legal's `Procedure` records.
//!
//! The law says which step may come in which status; the status a step leads to is not the law's
//! choice but the one ALICE-Legal derives ([`next_status`]): an approval approves, a rejection
//! rejects, a completion completes, and any other step moves a pending procedure into progress and
//! leaves any other status as it is. A law whose `on` line goes elsewhere does not read, so a
//! procedure answered by the law and the same steps recorded in ALICE-Legal end in one status.

use super::{err, MachineLaw, RuleLawError};

/// The statuses of a procedure, the states of a procedure law (ALICE-Legal's `ProcedureStatus`)
pub const PROCEDURE_STATUSES: [&str; 5] = [
    "pending",
    "in_progress",
    "approved",
    "rejected",
    "completed",
];

/// The kinds of a step, the events of a procedure law (ALICE-Legal's `StepKind`)
pub const STEP_KINDS: [&str; 7] = [
    "application",
    "review",
    "approval",
    "rejection",
    "amendment",
    "notification",
    "completion",
];

/// The status a step of kind `step` leads to from `status` (ALICE-Legal's `Procedure::add_step`)
#[must_use]
pub fn next_status<'a>(status: &'a str, step: &str) -> &'a str {
    match step {
        "approval" => "approved",
        "rejection" => "rejected",
        "completion" => "completed",
        _ if status == "pending" => "in_progress",
        _ => status,
    }
}

/// Checks a machine law read from a `kind procedure` file against the shape of a procedure
pub(super) fn check_procedure(m: &MachineLaw) -> Result<(), RuleLawError> {
    if let Some(s) = m
        .states
        .iter()
        .find(|s| !PROCEDURE_STATUSES.contains(&s.as_str()))
    {
        return Err(err(
            0,
            format!(
                "`{s}` is not a status of a procedure ({})",
                PROCEDURE_STATUSES.join(" ")
            ),
        ));
    }
    if m.initial != "pending" {
        return Err(err(
            0,
            "a procedure starts `pending` (`state pending initial`)",
        ));
    }
    if let Some((e, _)) = m
        .events
        .iter()
        .find(|(e, _)| !STEP_KINDS.contains(&e.as_str()))
    {
        return Err(err(
            0,
            format!("`{e}` is not a kind of step ({})", STEP_KINDS.join(" ")),
        ));
    }
    for (event, from, to, _) in &m.on {
        if let Some(f) = from.iter().find(|f| next_status(f, event) != to) {
            return Err(err(
                0,
                format!(
                    "`on {event} from {f} to {to}`: a {event} step from {f} leads to {}",
                    next_status(f, event)
                ),
            ));
        }
    }
    Ok(())
}

/// The measured value of every pin of [`super::PROCEDURE_SEMANTICS_PINS`]
///
/// The first is the digest of the status every step kind leads to from every status, which fixed
/// files read as procedures, and the answers of a fixed procedure law to fixed requests.
#[must_use]
pub fn procedure_semantics_fingerprints() -> Vec<(&'static str, [u8; 32])> {
    vec![
        (
            super::PROCEDURE_SEMANTICS_PINS[0].0,
            procedure_fingerprint(),
        ),
        (
            super::PROCEDURE_SEMANTICS_PINS[1].0,
            super::RULE_SEMANTICS_ID,
        ),
    ]
}

fn procedure_fingerprint() -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for s in PROCEDURE_STATUSES {
        for k in STEP_KINDS {
            h.update(format!("{s} {k} -> {}\n", next_status(s, k)).as_bytes());
        }
    }
    let law = "kind procedure\nstate pending initial\nstate in_progress\nstate approved\n\
               state completed\nevent application\nevent review guard complete\nevent approval\n\
               event completion\non application from pending to in_progress\n\
               on review from in_progress to in_progress when complete\n\
               on approval from in_progress to approved\non completion from approved to completed\n";
    for (text, name) in [
        (law.to_owned(), "the fixture"),
        (
            law.replace("to approved\n", "to completed\n"),
            "an approval that completes",
        ),
        (
            law.replace("state pending initial\n", "state pending\n")
                .replace("state in_progress\n", "state in_progress initial\n"),
            "a start other than pending",
        ),
        (
            law.replace("event review", "event audit")
                .replace("on review", "on audit"),
            "a step kind ALICE-Legal has not",
        ),
    ] {
        // whether it reads, not the wording of the error
        let reads = super::rule_law_from_file(&text).is_ok();
        h.update(format!("{name}: {reads}\n").as_bytes());
    }
    let law = super::rule_law_from_file(law).expect("the fingerprint procedure reads");
    for r in [
        r#"{"state": "pending", "event": "application"}"#,
        r#"{"state": "pending", "event": "approval"}"#,
        r#"{"state": "in_progress", "event": "review", "guards": {"complete": false}}"#,
        r#"{"state": "in_progress", "event": "review", "guards": {"complete": true}}"#,
        r#"{"state": "approved", "event": "completion"}"#,
        r#"{"state": "completed", "event": "application"}"#,
        r#"{"state": "rejected", "event": "application"}"#,
    ] {
        let a = law.respond(&crate::law_input::parse_json(r).expect("fingerprint JSON"));
        h.update(format!("{r} -> {a:?}\n").as_bytes());
    }
    h.finalize().into()
}
