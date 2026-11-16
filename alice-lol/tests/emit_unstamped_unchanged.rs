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
/// Re-recorded a second time when `rounded_box(hx, hy, hz, r)` was given the outer half-extents (as for
/// `box3d`) and the stdlib's rounded-box helper, `counterbore` and `mug` were corrected: the dump of
/// `tests/parse_corpus_dump.rs` before and after differs in 285 lines, every one an entry whose
/// construct builds a rounded box (directly, through the helper or as the emitted `RoundedBox`), a
/// counterbore or the mug Then `shower_caddy` subtracts its drain holes as one union instead of one at
/// a time (the tree was too deep for the debug build's transpiler, the value is the same
/// max(a, -b, -c, ...) since the holes are plain cylinders under a plain union): the dump differs in
/// the 5 lines of `shower_caddy` only
/// Re-recorded when the stdlib containers and plates were corrected for the outer-size `rounded_box` (open
/// shells with a flat rim and vertical edges rounded with r <= wall, plates rounded only along their
/// thickness, mount holes through Z-thin plates along Z, the esp32 USB notch and the pixhawk damper
/// position): the dump differs in the 5 lines of each of 24 constructs (the 23 shapes whose walls the
/// outer-size change had thinned, and `sbc_mount_plate`), and `esp32_enclosure` at the small corpus
/// arguments is now a parse error (a USB opening wider than its wall face is refused)
/// Re-recorded when `shelf_divider` cut its plate holes as round Ø15 through-holes on a triangular
/// lattice (each hole placed by itself, not a `RepeatFinite` read as anchored at a corner): the dump
/// differs in the 5 lines of `shelf_divider` only
const DIGEST: &str = "9fb67c2ab013b8d465d181c983125918f93e285c6b6e8e378352e59aedb6cba7";

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
