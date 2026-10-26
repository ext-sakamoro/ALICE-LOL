//! A text without the `semantics(<id>, ...)` wrapper reads exactly as before the wrapper existed
//!
//! The wrapper (ALICE-Law G7) is optional for `parse_lol` / `parse_program` / `parse_declared`
//! / `parse_law`, so every text written before it keeps its meaning. This test reads a frozen
//! corpus through all four entry points and hashes each result: a node or program by its `Debug`
//! form (which spells every number exactly), an error by its message and position. The digest
//! was recorded on the parser before the wrapper was added (ALICE-LOL bbb6539), so a change to
//! how a header-less text reads, accepted or refused, moves it.
//!
//! The corpus (`data/parse_corpus_bbb6539.rs`, 594 texts, written once at bbb6539 and not
//! regenerated): one snippet per construct of the grammar and the deep fixtures of
//! `common::corpus`, every `.lol` file and every `lol` fence of the documents (CHANGELOGs
//! excepted), the audit blocks of `laws/spike/`, and then the first half of each of those.

use std::fmt::Write as _;

use alice_lol::runtime_parser::{parse_declared, parse_law, parse_lol, parse_program, ParseError};
use sha2::{Digest, Sha256};

include!("data/parse_corpus_bbb6539.rs");

/// SHA-256 over every (entry point, text, result) of the corpus, recorded at bbb6539
///
/// The digest was re-recorded once when two constructs of the corpus were renamed (four texts);
/// that the reading did not change was shown by dumping every result before and after the
/// rename (`tests/parse_corpus_dump.rs`) and comparing the dumps with `scripts/compare_corpus_dumps.py`
/// (equal once the old names are put back, error positions moved by the shorter names)
const DIGEST: &str = "9cc83da96dd6b1b75f7728cd624b0f86efb8222c83e7845f17ed426d40953f00";

fn err(e: &ParseError) -> String {
    format!("Err({:?} @ {})", e.message, e.position)
}

fn results(text: &str) -> [String; 4] {
    [
        parse_lol(text).map_or_else(|e| err(&e), |n| format!("{n:?}")),
        parse_program(text).map_or_else(|e| err(&e), |p| format!("{p:?}")),
        parse_declared(text).map_or_else(|e| err(&e), |d| format!("{d:?}")),
        parse_law(text).map_or_else(|e| err(&e), |l| format!("{l:?}")),
    ]
}

#[test]
fn header_less_texts_read_as_before_the_wrapper() {
    assert_eq!(CORPUS.len(), 594, "the frozen corpus changed");
    let mut h = Sha256::new();
    let mut accepted = 0usize;
    for text in CORPUS {
        for (entry, result) in ["lol", "program", "declared", "law"]
            .iter()
            .zip(results(text))
        {
            accepted += usize::from(!result.starts_with("Err("));
            for part in [entry.as_bytes(), text.as_bytes(), result.as_bytes()] {
                h.update((part.len() as u64).to_le_bytes());
                h.update(part);
            }
        }
    }
    assert!(
        accepted > 0,
        "no text of the corpus was accepted by any entry point"
    );
    let hex = h.finalize().iter().fold(String::new(), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    });
    assert_eq!(
        hex, DIGEST,
        "a header-less text reads differently ({accepted} accepted results)"
    );
}
