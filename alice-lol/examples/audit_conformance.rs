//! The program contract of `conformance/TASK.md` for the audit laws of `laws/spike/`,
//! built on this crate: the request is read with `law_input::parse_json`, the law file
//! with `law_input::audit_law_from_file`, each `x-input` with `law_input::read`, and the
//! verdict is `AuditLaw::evaluate`.
//!
//! The derived metrics of `identifier_feature_independent` (its `x-metric` / `x-at-least`
//! lines) are written out here; they are prose in the law file.
//!
//! ```text
//! echo '{"law":"gate_compares_nonzero","inputs":{"compared":3}}' | cargo run --example audit_conformance
//! ```
//!
//! An unknown law, a request that is not an object, or `inputs` that is not an object
//! exits with status 2 and writes nothing on standard output.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::PathBuf;
use std::process::ExitCode;

use alice_lol::audit_law::{Measurements, Verdict};
use alice_lol::law_input::{audit_law_from_file, parse_json, read, Json, LawKind, Read};

const LAWS: &[&str] = &["gate_compares_nonzero", "identifier_feature_independent"];

fn request_error(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::from(2)
}

fn law_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../laws/spike")
        .join(format!("{name}.law"))
}

/// Reads the measurements of one request (inputs already checked to be an object or absent)
fn measurements(
    law_name: &str,
    law: &alice_lol::audit_law::AuditLaw,
    inputs: &Json,
) -> Measurements {
    let typed: BTreeSet<&str> = law.inputs().iter().map(|(n, _)| n.as_str()).collect();
    let mut m = Measurements::new();
    // numbers on plain `input` lines: a finite JSON number, otherwise not measured
    if let Json::Object(kv) = inputs {
        let keys: BTreeSet<&str> = kv.iter().map(|(k, _)| k.as_str()).collect();
        for k in keys.difference(&typed) {
            // the last value of a repeated key is the one read
            if let Some(Json::Number(x)) = inputs.get(k) {
                if x.is_finite() {
                    m = m.with_number(k, *x);
                }
            }
        }
    }
    for (name, ty) in law.inputs() {
        let Read::Value(v) = read(name, ty, inputs.get(name), LawKind::Audit) else {
            continue;
        };
        if law_name == "identifier_feature_independent" && name == "builds" {
            m = identifier_metrics(m, v);
        } else if let Json::Array(xs) = v {
            let texts: Vec<&str> = xs
                .iter()
                .filter_map(|x| {
                    if let Json::Text(t) = x {
                        Some(t.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            m = m.with_range(name, &texts);
        }
    }
    m
}

/// The `x-metric` / `x-at-least` lines of `identifier_feature_independent`, on a `builds`
/// that matches its type
fn identifier_metrics(mut m: Measurements, builds: &Json) -> Measurements {
    let Json::Array(bs) = builds else { return m };
    #[allow(clippy::cast_precision_loss, reason = "counts of builds are small")]
    let count = |n: usize| n as f64;
    m = m.with_number("builds", count(bs.len()));
    let sets: BTreeSet<BTreeSet<&str>> = bs
        .iter()
        .map(|b| match b.get("features") {
            Some(Json::Array(fs)) => fs
                .iter()
                .filter_map(|f| {
                    if let Json::Text(t) = f {
                        Some(t.as_str())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => BTreeSet::new(),
        })
        .collect();
    // x-at-least feature_sets 2: fewer than 2 counts as not measured (0)
    let fs = if sets.len() >= 2 { sets.len() } else { 0 };
    m = m.with_number("feature_sets", count(fs));
    let ids: Option<BTreeSet<&str>> = bs
        .iter()
        .map(|b| match b.get("id") {
            Some(Json::Text(t)) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    if let (false, Some(ids)) = (bs.is_empty(), ids) {
        m = m.with_number("distinct_identifiers", count(ids.len()));
    }
    m
}

fn subject_of(v: &Verdict) -> Option<String> {
    match v {
        Verdict::Supports => None,
        Verdict::NoEvidence { metric } | Verdict::Breaks { metric, .. } => Some(metric.clone()),
        Verdict::OutOfRange { key } | Verdict::ParameterUpdate { key, .. } => Some(key.clone()),
        // the reason names the metric between backticks
        Verdict::Undecided { reason } => reason.split('`').nth(1).map(str::to_owned),
    }
}

const fn word(v: &Verdict) -> &'static str {
    match v {
        Verdict::Supports => "supports",
        Verdict::NoEvidence { .. } => "no_evidence",
        Verdict::Breaks { .. } => "breaks",
        Verdict::OutOfRange { .. } => "out_of_range",
        Verdict::ParameterUpdate { .. } => "parameter_update",
        Verdict::Undecided { .. } => "undecided",
    }
}

fn json_text(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                // String への write は失敗しない
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn main() -> ExitCode {
    let mut text = String::new();
    if std::io::stdin().read_to_string(&mut text).is_err() {
        return request_error("request is not UTF-8 text");
    }
    let req = match parse_json(&text) {
        Ok(r) => r,
        Err(e) => return request_error(&format!("request is not JSON: {e}")),
    };
    if !matches!(req, Json::Object(_)) {
        return request_error("request is not a JSON object");
    }
    let Some(Json::Text(name)) = req.get("law") else {
        return request_error("unknown law");
    };
    if !LAWS.contains(&name.as_str()) {
        return request_error(&format!("unknown law: {name}"));
    }
    // no inputs key and inputs null are the same as {}
    let empty = Json::Object(Vec::new());
    let inputs = match req.get("inputs") {
        None | Some(Json::Null) => &empty,
        Some(o @ Json::Object(_)) => o,
        Some(_) => return request_error("inputs is not a JSON object"),
    };
    let source = match std::fs::read_to_string(law_file(name)) {
        Ok(s) => s,
        Err(e) => return request_error(&format!("cannot read the law file: {e}")),
    };
    let law = match audit_law_from_file(&source) {
        Ok(l) => l,
        Err(e) => return request_error(&e.to_string()),
    };
    let verdict = law.evaluate(&measurements(name, &law, inputs));
    let subject = subject_of(&verdict).map_or_else(|| "null".to_owned(), |s| json_text(&s));
    println!(
        "{{\"outputs\": {{\"verdict\": \"{}\", \"subject\": {subject}}}}}",
        word(&verdict)
    );
    ExitCode::SUCCESS
}
