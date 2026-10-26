//! Writes every result the two frozen-corpus tests hash, one line per (text, output), to the
//! file `LOL_CORPUS_DUMP` names
//!
//! `parse_without_semantics_wrapper.rs` and `emit_unstamped_unchanged.rs` keep only a digest, so
//! a change of the corpus text itself (a construct renamed) cannot be compared to the recorded
//! state through them. This dump is that comparison: written before and after such a change, the
//! two files must be equal once the old names are put back into the new one (the reading did not
//! change, only the names). Ignored: it runs only when asked, with the variable set.

use std::fmt::Write as _;

use alice_lol::emit;
use alice_lol::runtime_parser::{parse_declared, parse_law, parse_lol, parse_program, ParseError};

include!("data/parse_corpus_bbb6539.rs");

fn err(e: &ParseError) -> String {
    format!("Err({:?} @ {})", e.message, e.position)
}

#[test]
#[ignore = "writes a file; run with LOL_CORPUS_DUMP=<path> and --ignored"]
fn dump_the_frozen_corpus_results() {
    let path = std::env::var("LOL_CORPUS_DUMP").expect("LOL_CORPUS_DUMP names the output file");
    let mut out = String::new();
    for text in CORPUS {
        let mut line = |entry: &str, result: String| {
            let _ = writeln!(out, "{text:?}\t{entry}\t{result:?}");
        };
        line(
            "lol",
            parse_lol(text).map_or_else(|e| err(&e), |n| format!("{n:?}")),
        );
        line(
            "program",
            parse_program(text).map_or_else(|e| err(&e), |p| format!("{p:?}")),
        );
        line(
            "declared",
            parse_declared(text).map_or_else(|e| err(&e), |d| format!("{d:?}")),
        );
        line(
            "law",
            parse_law(text).map_or_else(|e| err(&e), |l| format!("{l:?}")),
        );
        if let Ok(program) = parse_program(text) {
            line(
                "emit node",
                format!("{:?}", emit::to_lol_unstamped(&program.sdf)),
            );
            line("emit program", format!("{:?}", program.to_lol_unstamped()));
        }
    }
    assert!(
        out.lines().count() >= CORPUS.len() * 4,
        "every text has its four parse results"
    );
    std::fs::write(&path, out).expect("the dump is written");
}
