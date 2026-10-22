//! The program contract of `conformance/TASK.md` for the rule laws (`kind machine` /
//! `kind predicate`) of a law directory, built on [`alice_lol::rule_law`]: the request is read
//! with `law_input::parse_json`, the law file with `rule_law::rule_law_from_file`, and the
//! answer is `RuleLaw::respond`. Nothing here knows a particular law.
//!
//! ```text
//! echo '{"law":"consent_choice","inputs":{"mode":"strict","state":"undecided","event":"accept"}}' \
//!   | LOL_LAW_DIR=laws/spike cargo run --example rule_conformance --features crypto-calls
//! ```
//!
//! Scored by `conformance/run_conformance.py --only-rule-laws` on the corpus that
//! `conformance/gen_corpus.py` generates (the expected answers come from `scripts/law_rules.py`,
//! a separate implementation). An unknown law, a law file that does not read, a request that is
//! not an object, `inputs` that is not an object or a missing input exits with status 2 and
//! writes nothing on standard output; so does a law whose call this build does not have.

use std::fmt::Write as _;
use std::io::Read as _;
use std::path::PathBuf;
use std::process::ExitCode;

use alice_lol::law_input::{parse_json, Json};
use alice_lol::rule_law::{rule_kind, rule_law_from_file, Answer};

fn request_error(msg: &str) -> ExitCode {
    eprintln!("{msg}");
    ExitCode::from(2)
}

/// The file of a law: `LOL_LAW_DIR` (TASK.md: "the checks always set it"); no fallback, so a
/// harness that forgets it fails at once instead of reading another directory
fn law_file(name: &str) -> PathBuf {
    let Some(dir) = std::env::var_os("LOL_LAW_DIR") else {
        eprintln!("LOL_LAW_DIR is not set (the checks always set it; see conformance/TASK.md)");
        std::process::exit(2);
    };
    PathBuf::from(dir).join(format!("{name}.law"))
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

/// JSON text of an answer's outputs (text, booleans, null and objects of them)
fn write_json(v: &Json, out: &mut String) {
    match v {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Text(t) => out.push_str(&json_text(t)),
        Json::Number(x) => {
            let _ = write!(out, "{x}");
        }
        Json::Array(items) => {
            out.push('[');
            for (i, x) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_json(x, out);
            }
            out.push(']');
        }
        Json::Object(kv) => {
            out.push('{');
            for (i, (k, x)) in kv.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&json_text(k));
                out.push_str(": ");
                write_json(x, out);
            }
            out.push('}');
        }
    }
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
    // the name is a plain identifier, so it names a file of the directory
    let plain = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    let source = plain
        .then(|| std::fs::read_to_string(law_file(name)).ok())
        .flatten()
        .filter(|t| rule_kind(t).is_some());
    let Some(source) = source else {
        return request_error(&format!("unknown law (not a rule law): {name}"));
    };
    let law = match rule_law_from_file(&source) {
        Ok(l) => l,
        Err(e) => return request_error(&e.to_string()),
    };
    // no inputs key and inputs null are the same as {}
    let empty = Json::Object(Vec::new());
    let inputs = match req.get("inputs") {
        None | Some(Json::Null) => &empty,
        Some(o @ Json::Object(_)) => o,
        Some(_) => return request_error("inputs is not a JSON object"),
    };
    match law.respond(inputs) {
        Answer::Outputs(o) => {
            let mut out = String::from("{\"outputs\": ");
            write_json(&o, &mut out);
            out.push('}');
            println!("{out}");
            ExitCode::SUCCESS
        }
        Answer::Rejected(why) => {
            println!("{{\"rejected\": {}}}", json_text(&why));
            ExitCode::SUCCESS
        }
        Answer::RequestError(why) | Answer::Unsupported(why) => request_error(&why),
    }
}
