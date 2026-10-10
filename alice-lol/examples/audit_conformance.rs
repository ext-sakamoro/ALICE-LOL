//! The program contract of `conformance/TASK.md` for every audit law of `laws/spike/`,
//! built on this crate: the request is read with `law_input::parse_json`, the law file
//! with `law_input::audit_law_from_file`, each `x-input` with `law_input::read`, and the
//! verdict is `AuditLaw::evaluate`.
//!
//! The measurements, including the derived `x-metric` / `x-at-least` quantities, come from
//! `law_input::measurements`: nothing here knows a particular law.
//!
//! ```text
//! echo '{"law":"gate_compares_nonzero","inputs":{"compared":3}}' | cargo run --example audit_conformance
//! ```
//!
//! An unknown law, a request that is not an object, or `inputs` that is not an object
//! exits with status 2 and writes nothing on standard output.

use std::fmt::Write as _;
use std::io::Read as _;
use std::path::PathBuf;
use std::process::ExitCode;

use alice_lol::audit_law::Verdict;
use alice_lol::law_input::{audit_law_from_file, measurements, parse_json, Json};

fn request_error(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::from(2)
}

fn law_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../laws/spike")
        .join(format!("{name}.law"))
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
    // any audit law file of laws/spike/ (the name is a plain identifier, so it names a file)
    let plain = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    let source = std::fs::read_to_string(law_file(name))
        .ok()
        .filter(|_| plain);
    let Some(source) = source.filter(|t| t.lines().any(|l| l.trim() == "kind audit")) else {
        return request_error(&format!("unknown law: {name}"));
    };
    // no inputs key and inputs null are the same as {}
    let empty = Json::Object(Vec::new());
    let inputs = match req.get("inputs") {
        None | Some(Json::Null) => &empty,
        Some(o @ Json::Object(_)) => o,
        Some(_) => return request_error("inputs is not a JSON object"),
    };
    let law = match audit_law_from_file(&source) {
        Ok(l) => l,
        Err(e) => return request_error(&e.to_string()),
    };
    let verdict = law.evaluate(&measurements(&law, inputs));
    let subject = subject_of(&verdict).map_or_else(|| "null".to_owned(), |s| json_text(&s));
    println!(
        "{{\"outputs\": {{\"verdict\": \"{}\", \"subject\": {subject}}}}}",
        word(&verdict)
    );
    ExitCode::SUCCESS
}
