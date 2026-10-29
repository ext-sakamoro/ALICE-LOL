//! Build a three-view record from landmarks given as JSON (what a model or a detector writes)
//!
//! ```text
//! three_view_record <landmarks.json>   # the record text, in canonical order, on stdout
//! ```
//!
//! The JSON shape is documented at `alice_lol::three_view::record_from_json`. Exit status: 0
//! written, 2 usage, I/O or a refused input.

use std::process::ExitCode;

use alice_lol::three_view::record_from_json;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file] = args.as_slice() else {
        eprintln!("usage: three_view_record <landmarks.json>");
        return ExitCode::from(2);
    };
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("three_view_record: {file}: {e}");
            return ExitCode::from(2);
        }
    };
    match record_from_json(&text) {
        Ok(r) => {
            print!("{}", r.to_text());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("three_view_record: {file}: {e}");
            ExitCode::from(2)
        }
    }
}
