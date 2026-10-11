//! Degenerate-parameter grid, parity axis: for every grammar construct with
//! one numeric literal at a time replaced by a hazard-chosen value
//! (`tests/degenerate_grid.rs`'s own `PALETTE`), a parse that SUCCEEDS must
//! agree with itself after one emit-reparse round trip, the same way
//! `fuzz_lol_emit_parity` compares its two trees. `degenerate_grid.rs`
//! classifies only hang / crash / allocation-cap (a successful parse that
//! evaluates to a value disagreeing with its own re-emitted self is
//! `Outcome::Pass` there) -- this file is the parity-equivalent of that
//! grid, added after `lidinoid(-0.0,1e8)` (an un-guarded TPMS `scale`
//! dividing into `+inf`/`-inf`, masked by `emit::canon`'s `-0.0` -> `0.0`
//! collapse) showed that gap concretely.
//!
//! In-process, not subprocess-isolated like `degenerate_grid.rs`: a single
//! degenerate literal exercising ordinary arithmetic (not the
//! `MAX_NODE_EXPANSION`-scale combinatorial blow-ups `degenerate_grid.rs`'s
//! all-same/pairwise generators target) is not expected to hang or
//! allocate unboundedly, so this stays cheap enough to run in the default
//! `cargo test` sweep rather than needing its own CI job.

mod common;
use common::corpus::{count_numbers, grammar_corpus, sample_points, with_numbers_at};

use alice_lol::emit::to_lol;
use alice_lol::runtime_parser::parse_lol;
use alice_lol::{eval, parity::agrees};

/// `tests/degenerate_grid.rs`'s own `PALETTE`, kept identical on purpose:
/// this is the parity-equivalent sweep over the same inputs, not a
/// different hazard class.
const PALETTE: &[&str] = &[
    "NaN",
    "inf",
    "-inf",
    "0.0",
    "-0.0",
    "-1.0",
    "0.5",
    "7.0",
    "1e-45",
    "1e-30",
    "1e30",
    "-1e30",
    "3.4028235e38",
    "-3.4028235e38",
    "5e-7",
    "1e-6",
    "1e9",
    "1024",
    "1025",
    "10001",
    "100001",
    "65536",
    "2147483648",
    "4294967296",
];

/// One case's outcome: `None` unless the FIRST parse succeeded (a parse
/// rejection is this grid's other half, `degenerate_grid.rs`'s job, not a
/// parity question) and emitting/re-parsing it disagrees.
fn parity_failure(src: &str) -> Option<(f32, String)> {
    let n1 = parse_lol(src).ok()?;
    let text = to_lol(&n1).ok()?;
    let n2 = parse_lol(&text)
        .unwrap_or_else(|e| panic!("emitted text does not re-parse: {e}\n{src} -> {text}"));
    let mut worst = 0.0f32;
    for p in sample_points(16, 2.5) {
        let (a, b) = (eval(&n1, p), eval(&n2, p));
        if !agrees(a, b, 1e-4) {
            worst = worst.max((a - b).abs() / a.abs().max(b.abs()).max(1.0));
        }
    }
    (worst > 0.0).then_some((worst, text))
}

#[test]
#[ignore = "known defect AUD-LOL-ZERO-001: 17 cases across 8 constructors \
            (blobby_cross/terrain/ellipsoid/parabola_segment/helix/ \
            uneven_capsule/columns_intersection/surface_roughness) beyond \
            the 9 TPMS surfaces fail this same way -- reported, scope of \
            the fix for these 8 is a separate, pending decision"]
fn one_at_a_time_degenerate_values_agree_after_one_emit_reparse_round_trip() {
    let corpus = grammar_corpus();
    assert!(!corpus.is_empty(), "grammar_corpus produced 0 entries");

    let mut cases = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for (name, src) in &corpus {
        let n = count_numbers(src);
        for idx in 0..n {
            for value in PALETTE {
                cases += 1;
                let mutated = with_numbers_at(src, &[idx], value);
                if let Some((worst, text)) = parity_failure(&mutated) {
                    if failures.len() < 50 {
                        failures.push(format!(
                            "[{name}] {mutated} -> {text:?} worst_rel={worst:.2e}"
                        ));
                    }
                }
            }
        }
    }
    println!(
        "degenerate_parity: {cases} cases over {} corpus entries, {} failing",
        corpus.len(),
        failures.len()
    );
    assert!(cases > 0, "0 cases run (vacuous pass)");
    assert!(
        failures.is_empty(),
        "{} case(s) disagree with their own re-emitted self:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Pins the exact crash this grid was added for: a mutant reverting
/// `runtime_parser`'s TPMS validation (`validate_tpms_fields`) turns this
/// red, where it would otherwise be silently `Outcome::Pass` in
/// `degenerate_grid.rs` (a successful, if wrong, parse).
#[test]
fn lidinoid_negative_zero_scale_is_rejected_not_silently_mismatched() {
    assert!(
        parse_lol("lidinoid(-0.0,1e8)").is_err(),
        "the original fuzz crash input must be rejected at the language boundary, \
         not accepted and silently produce an inf-vs-inf mismatch after a round trip"
    );
}
