//! Degenerate-parameter grid test: every SDF keyword's `.lol` text, with one
//! numeric literal at a time replaced by a value chosen to be hostile to a
//! cast/limit/division (`PALETTE`), must never make the parser hang, crash,
//! or allocate unboundedly -- a clean `Ok`/`Err` is the only acceptable
//! outcome. This is the parser-layer (untrusted `.lol` text) counterpart to
//! the `try_*` unit tests on the Spec layer: those assert on a specific
//! known hazard; this sweeps the whole keyword surface for an *unknown* one.
//!
//! Each case runs in its own process (`examples/parse_one_lol.rs`), killed
//! after a 2s wall-clock timeout and classified by the child's own exit
//! code: 0 = `Ok`, 1 = `Err` (both a pass -- a detected-and-refused input is
//! the parser doing its job), 42 = the probe's own counting allocator hit
//! its cap (an allocation-size regression), anything else (including a
//! signal) = a crash. In-process `catch_unwind` cannot stand in for this:
//! a hang or an unbounded allocation does not let `catch_unwind` run at all.
//!
//! Generators, all over `tests/common/corpus.rs`'s `grammar_corpus()`
//! (derived from `lol.gbnf`'s name buckets, so a snippet is already "the
//! right arity/types for this keyword"):
//!
//! 1. **one-at-a-time** (`PALETTE`, ~24 values): exactly one literal
//!    degenerate, every other literal at the corpus's own safe default.
//!    Finds single-field hazards (a `NaN`/huge/zero in one argument).
//! 2. **all-same** (`COMBINATION_PALETTE`, 13 values sized so two of them
//!    multiplied together clears `MAX_NODE_EXPANSION` = 10,000): every
//!    literal in the snippet set to the same value at once.
//! 3. **pairwise** (same `COMBINATION_PALETTE`): for every unordered pair of
//!    literal positions, both set to the same value, the rest at default.
//!
//! (2) and (3) exist because (1) is structurally blind to a hazard that
//! needs *several* coordinates pushed up together and no single one of them
//! alone -- which is exactly the shape of the regression this audit started
//! from (`gridfinity_bin_ex`'s `dividers` is two counts, each individually
//! within its own per-axis bound, whose *product* was unchecked; no single
//! literal in that snippet, mutated alone against its small corpus default,
//! can reproduce that). Verified: with only (1), the grid test does NOT
//! turn red when that regression is reintroduced (`scripts/grid_teeth_check.sh`'s
//! M1 mutant); with (2)+(3) added, it does.
//!
//! Secondary generator (keyword-coverage fallback, not hazard-coverage): for
//! any keyword the corpus does not generate a snippet for (its grammar
//! bucket has no `snippet()` arm), a 0..=8-arg black-box sweep with every
//! arg set to the same palette value -- this path existing and firing a
//! nonzero number of times for real (not just existing in the code) is
//! itself asserted, so a keyword silently skipped by both the corpus and
//! the sweep would be visible rather than silently passing.
//!
//! CI: ubuntu only (job on `linux` already runs `cargo test --test
//! degenerate_grid`). macOS and Windows are not covered: killing a
//! subprocess cleanly on a timeout has different semantics there, and this
//! gate's value is in the breadth of the sweep, not its OS coverage (the
//! existing per-OS `cargo test` matrix already runs the full test suite,
//! this file included, on every OS -- that still catches anything that
//! doesn't need the 2s-kill isolation to observe).

use std::collections::BTreeSet;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

mod common;
use common::corpus::{count_numbers, grammar_corpus, with_numbers_at};

/// Values chosen to be hostile to the hazard classes this audit found:
/// non-finite (rejected earlier, at the lexer -- kept here as a regression
/// check that this still holds for every keyword, not just the ones with a
/// dedicated unit test), a degenerate pitch/size (0, negative), and values
/// that saturate or overflow an `as u32` / `as usize` cast or sit exactly at
/// a count limit (`MAX_STDLIB_COUNT` = 1024, `MAX_NODE_EXPANSION` = 10,000,
/// 2^31, 2^32, the largest finite `f32`).
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

/// Sized so that two of these multiplied together clear `MAX_NODE_EXPANSION`
/// (10,000) -- e.g. 101*101 = 10,201 -- the shape of the regression this
/// audit started from (two per-axis-valid counts whose *product* was
/// unchecked). `with_one_number` alone cannot combine two literals, hence
/// the separate all-same/pairwise generators that use this palette.
const COMBINATION_PALETTE: &[&str] = &[
    "2", "10", "32", "100", "101", "316", "1024", "1025", "4096", "10000", "10001", "65536",
    "100001",
];

const TIMEOUT: Duration = Duration::from_secs(2);
const ALLOC_CAP_EXIT_CODE: i32 = 42;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Pass,
    Hang,
    AllocCap,
    Crash(Option<i32>),
}

/// Build `examples/parse_one_lol` once (release profile: thousands of cases
/// spawn it, a debug build's slower startup would dominate the wall clock)
/// and return its path, via `cargo build --message-format=json` so the path
/// is read from cargo's own output rather than guessed (workspace target
/// dirs, `CARGO_TARGET_DIR` overrides, and per-platform executable suffixes
/// all vary).
fn probe_binary() -> &'static std::path::PathBuf {
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let output = Command::new(env!("CARGO"))
            .args([
                "build",
                "--release",
                "--example",
                "parse_one_lol",
                "--manifest-path",
            ])
            .arg(format!("{manifest_dir}/Cargo.toml"))
            .arg("--message-format=json")
            .output()
            .expect("failed to run `cargo build --example parse_one_lol`");
        assert!(
            output.status.success(),
            "cargo build --example parse_one_lol failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if !line.contains("\"compiler-artifact\"") || !line.contains("\"parse_one_lol\"") {
                continue;
            }
            // executable:"<path>" -- a hand-rolled scan avoids adding a JSON
            // dependency just to read one field out of cargo's own output
            if let Some(start) = line.find("\"executable\":\"") {
                let rest = &line[start + "\"executable\":\"".len()..];
                if let Some(end) = rest.find('"') {
                    let path = rest[..end].replace("\\\\", "\\");
                    return std::path::PathBuf::from(path);
                }
            }
        }
        panic!("cargo build --message-format=json never reported an executable for parse_one_lol:\n{stdout}");
    })
}

fn run_case(src: &str) -> Outcome {
    let mut child = Command::new(probe_binary())
        .arg(src)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn the probe binary");
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("failed to poll the probe process") {
            return match status.code() {
                Some(0 | 1) => Outcome::Pass,
                Some(ALLOC_CAP_EXIT_CODE) => Outcome::AllocCap,
                other => Outcome::Crash(other),
            };
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Outcome::Hang;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// `src` 内の全 literal を `value` 1 つに揃えた black-box sweep 用の call text
/// (corpus が snippet を持たない keyword の arity を推測せずに済ませる)
fn sweep_call(name: &str, argc: usize, value: &str) -> String {
    let joined = vec![value; argc].join(", ");
    format!("{name}({joined})")
}

/// `syntax_table.rs` の `SDF_SYNTAX` を test 専用の可視性を変えずに独立に読む
/// (`scripts/runtime_keywords.py` の Python 版と同じ技法、こちらは Rust 版)
fn sdf_syntax_names() -> BTreeSet<String> {
    let src = include_str!("../src/syntax_table.rs");
    let start = src
        .find("const SDF_SYNTAX")
        .expect("syntax_table.rs has no SDF_SYNTAX");
    let end = src[start..]
        .find("\n];\n")
        .expect("SDF_SYNTAX has no closing `];`")
        + start;
    let body = &src[start..end];
    let mut out = BTreeSet::new();
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        if c != '"' {
            continue;
        }
        let rest = &body[i + 1..];
        if let Some(j) = rest.find('"') {
            out.insert(rest[..j].to_string());
            // skip past the closing quote so the next search starts after it
            for _ in 0..=j {
                chars.next();
            }
        }
    }
    out
}

struct GridResult {
    cases: usize,
    cases_by_generator: Vec<(&'static str, usize)>,
    hangs: Vec<String>,
    alloc_caps: Vec<String>,
    crashes: Vec<String>,
    covered_by_corpus: BTreeSet<String>,
    covered_by_sweep: BTreeSet<String>,
}

/// Run one generated case, bump `cases`, file the outcome under `label`
/// (which generator produced it) into the right bucket, and return whether
/// this specific case passed (the caller needs that, not whether the run
/// has had ANY failure anywhere so far).
#[allow(clippy::too_many_arguments)]
fn record(
    label: &str,
    name: &str,
    src: &str,
    cases: &mut usize,
    hangs: &mut Vec<String>,
    alloc_caps: &mut Vec<String>,
    crashes: &mut Vec<String>,
) -> bool {
    *cases += 1;
    match run_case(src) {
        Outcome::Pass => true,
        Outcome::Hang => {
            hangs.push(format!("[{label}] {name}: {src}"));
            false
        }
        Outcome::AllocCap => {
            alloc_caps.push(format!("[{label}] {name}: {src}"));
            false
        }
        Outcome::Crash(code) => {
            crashes.push(format!("[{label}] {name}: {src} (exit {code:?})"));
            false
        }
    }
}

#[allow(clippy::too_many_lines)] // one straight-line walk over 4 generators, splitting it would scatter the shared state
fn run_grid() -> GridResult {
    let corpus = grammar_corpus();
    let covered_by_corpus: BTreeSet<String> = corpus.iter().map(|(n, _)| n.clone()).collect();
    let all_names = sdf_syntax_names();
    assert!(
        all_names.len() > 200,
        "found only {} SDF names",
        all_names.len()
    );
    assert!(
        !covered_by_corpus.is_empty(),
        "grammar_corpus produced 0 entries"
    );

    let mut cases = 0usize;
    let mut hangs = Vec::new();
    let mut alloc_caps = Vec::new();
    let mut crashes = Vec::new();
    let (mut one_at_a_time, mut all_same, mut pairwise) = (0usize, 0usize, 0usize);

    for (name, src) in &corpus {
        let n = count_numbers(src);

        // 1. one-at-a-time: single-field hazards
        for idx in 0..n {
            for value in PALETTE {
                one_at_a_time += 1;
                let mutated = with_numbers_at(src, &[idx], value);
                record(
                    "one-at-a-time",
                    name,
                    &mutated,
                    &mut cases,
                    &mut hangs,
                    &mut alloc_caps,
                    &mut crashes,
                );
            }
        }

        // 2. all-same: every literal pushed to the same combination-sized value
        if n > 0 {
            for value in COMBINATION_PALETTE {
                all_same += 1;
                let targets: Vec<usize> = (0..n).collect();
                let mutated = with_numbers_at(src, &targets, value);
                record(
                    "all-same",
                    name,
                    &mutated,
                    &mut cases,
                    &mut hangs,
                    &mut alloc_caps,
                    &mut crashes,
                );
            }
        }

        // 3. pairwise: every unordered pair of literals pushed to the same
        // combination-sized value, the rest at default -- this is what
        // actually reproduces a 2-coordinate product overflow like
        // gridfinity_bin_ex's dividers, which (1) alone cannot
        if n >= 2 {
            for i in 0..n {
                for j in (i + 1)..n {
                    for value in COMBINATION_PALETTE {
                        pairwise += 1;
                        let mutated = with_numbers_at(src, &[i, j], value);
                        record(
                            "pairwise",
                            name,
                            &mutated,
                            &mut cases,
                            &mut hangs,
                            &mut alloc_caps,
                            &mut crashes,
                        );
                    }
                }
            }
        }
    }

    // sweep coverage means "cases were generated for this name", not "a case
    // passed" (a syntactically-valid-but-wrong-arity call legitimately
    // returns Err, which is itself a pass per Outcome::Pass) -- every name
    // in `uncovered` gets the full 0..=8 x COMBINATION_PALETTE treatment
    // below regardless of any individual case's result, so membership in
    // `uncovered` already is the coverage guarantee
    let uncovered: Vec<&String> = all_names.difference(&covered_by_corpus).collect();
    let covered_by_sweep: BTreeSet<String> = uncovered.iter().map(|n| (*n).clone()).collect();
    let mut sweep_cases = 0usize;
    for name in &uncovered {
        for argc in 0..=8 {
            for value in COMBINATION_PALETTE {
                sweep_cases += 1;
                let src = sweep_call(name, argc, value);
                record(
                    "sweep",
                    name,
                    &src,
                    &mut cases,
                    &mut hangs,
                    &mut alloc_caps,
                    &mut crashes,
                );
            }
        }
    }

    GridResult {
        cases,
        cases_by_generator: vec![
            ("one-at-a-time", one_at_a_time),
            ("all-same", all_same),
            ("pairwise", pairwise),
            ("sweep", sweep_cases),
        ],
        hangs,
        alloc_caps,
        crashes,
        covered_by_corpus,
        covered_by_sweep,
    }
}

// Not part of the default `cargo test` sweep: the existing Test job matrix
// runs 5 times per push (macOS + 4 ubuntu feature legs), and this test alone
// takes ~90s plus a release build of its probe binary, which that matrix
// doesn't need 5 copies of. A dedicated ubuntu-only CI job
// (`degenerate-grid`) runs it explicitly with `--ignored`. Subprocess-kill
// timing isn't macOS/Windows-portable enough to bother running it there too.
#[test]
#[ignore = "slow (~90s, builds a release probe binary): run via `cargo test --test degenerate_grid -- --ignored`, wired into the dedicated degenerate-grid CI job (ubuntu only)"]
fn no_keyword_degenerates_into_a_hang_crash_or_unbounded_allocation() {
    let start = Instant::now();
    let r = run_grid();
    let elapsed = start.elapsed();

    let covered: BTreeSet<String> = r
        .covered_by_corpus
        .union(&r.covered_by_sweep)
        .cloned()
        .collect();
    let all_names = sdf_syntax_names();
    let uncovered: Vec<&String> = all_names.difference(&covered).collect();

    println!(
        "degenerate_grid: {} cases, {:.1}s, covered by corpus {}, by sweep {}, uncovered {}",
        r.cases,
        elapsed.as_secs_f64(),
        r.covered_by_corpus.len(),
        r.covered_by_sweep.len(),
        uncovered.len()
    );
    for (label, n) in &r.cases_by_generator {
        println!("degenerate_grid: generator {label}: {n} cases");
    }

    assert!(r.cases > 0, "0 cases run (vacuous pass)");
    assert!(
        uncovered.is_empty(),
        "keywords reached by neither the corpus nor the sweep: {uncovered:?}"
    );
    assert!(
        r.hangs.is_empty(),
        "{} case(s) hung (>{:?}):\n{}",
        r.hangs.len(),
        TIMEOUT,
        r.hangs.join("\n")
    );
    assert!(
        r.alloc_caps.is_empty(),
        "{} case(s) hit the allocation cap:\n{}",
        r.alloc_caps.len(),
        r.alloc_caps.join("\n")
    );
    assert!(
        r.crashes.is_empty(),
        "{} case(s) crashed:\n{}",
        r.crashes.len(),
        r.crashes.join("\n")
    );
}
