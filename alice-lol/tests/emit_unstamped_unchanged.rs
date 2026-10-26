//! `to_lol_unstamped` writes exactly what `to_lol` wrote before `to_lol` began to stamp
//!
//! `to_lol` now writes the exchange form `semantics(<id>, ...)` (ALICE-Law G7); the canonical
//! text it wrote before is `to_lol_unstamped`. This test reads the frozen corpus of
//! `parse_without_semantics_wrapper.rs` with `parse_program`, writes every accepted program back
//! with the canonical writer (and the bare SDF with the node writer) and hashes the results,
//! errors included. The digest was recorded with `to_lol` before the change (ALICE-LOL bbb6539),
//! so a byte that moved in the canonical form moves it.

use std::fmt::Write as _;

use alice_lol::emit;
use alice_lol::runtime_parser::parse_program;
use sha2::{Digest, Sha256};

include!("data/parse_corpus_bbb6539.rs");

/// SHA-256 over every (text, node output, program output) of the corpus, recorded at bbb6539
///
/// The digest was re-recorded once when two constructs of the corpus were renamed (four texts);
/// that the reading did not change was shown by dumping every result before and after the
/// rename (`tests/parse_corpus_dump.rs`) and comparing the dumps with `scripts/compare_corpus_dumps.py`
/// (equal once the old names are put back, error positions moved by the shorter names)
const DIGEST: &str = "ae71911661b784455c5266fbcacae7fd28e2d1688551e752f60c0dbaf665fd9f";

#[test]
fn the_canonical_form_is_the_output_of_before() {
    assert_eq!(CORPUS.len(), 594, "the frozen corpus changed");
    let mut h = Sha256::new();
    let mut written = 0usize;
    for text in CORPUS {
        let Ok(program) = parse_program(text) else {
            continue;
        };
        let node = format!("{:?}", emit::to_lol_unstamped(&program.sdf));
        let whole = format!("{:?}", program.to_lol_unstamped());
        written += usize::from(node.starts_with("Ok("));
        for part in [text.as_bytes(), node.as_bytes(), whole.as_bytes()] {
            h.update((part.len() as u64).to_le_bytes());
            h.update(part);
        }
    }
    assert!(written > 100, "only {written} programs were written");
    let hex = h.finalize().iter().fold(String::new(), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    });
    assert_eq!(
        hex, DIGEST,
        "the canonical form moved ({written} programs written)"
    );
}
