//! G13 (no-unspecified-order) evidence: the same scene, exported to STL and
//! 3MF by two separate processes under different `RAYON_NUM_THREADS`, must
//! produce byte-identical output.
//!
//! `src/print_export.rs`'s `weld_coincident_vertices` holds a
//! `HashMap<[u32; 3], usize>` but only ever calls `.entry()`/`.len()` on it,
//! never iterates it; the vertices it keeps are pushed in `raw.vertices`'s
//! own order, so the map's internal bucket order (which a `HashMap`'s
//! per-instance random hash seed makes different from run to run) cannot
//! reach the output. A separate process gives each run its own seed, so
//! this test would catch a mutant that iterated the map instead (`for (k,
//! v) in id.iter() { ... }` in place of the `.entry()` lookup) -- verified
//! by hand, not by CI, since planting that mutant on purpose in a committed
//! test would defeat the point of a closed static check
//! (`scripts/print_export_no_hashmap_iteration.py`) existing alongside this
//! one; see that script's own module doc for why both are needed together.
//!
//! Varying `RAYON_NUM_THREADS` additionally exercises alice-sdf's own
//! parallel stages (marching cubes / mesh repair use `rayon::par_iter` in
//! that crate), which this crate's own gate entry does not claim to cover.
//!
//! `euler.vases`/`PrintConfig::high_quality()`'s lattice-infilled bracket
//! (the same scene as `examples/print_export.rs`'s case B) is the fixture:
//! a repeated unit cell produces many vertices landing on the same
//! position, so `weld_coincident_vertices` has real coincident vertices to
//! merge -- a scene with none would not exercise the function this test is
//! about.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

/// Build `examples/print_export_probe` once (release profile) and return its
/// path, read from cargo's own `--message-format=json` output rather than
/// guessed (workspace target dirs, `CARGO_TARGET_DIR` overrides, and
/// per-platform executable suffixes all vary) -- the same approach
/// `tests/degenerate_grid.rs`'s own `probe_binary()` uses.
fn probe_binary() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let output = Command::new(env!("CARGO"))
            .args([
                "build",
                "--release",
                "--example",
                "print_export_probe",
                "--manifest-path",
            ])
            .arg(format!("{manifest_dir}/Cargo.toml"))
            .arg("--message-format=json")
            .output()
            .expect("failed to run `cargo build --example print_export_probe`");
        assert!(
            output.status.success(),
            "cargo build --example print_export_probe failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if !line.contains("\"compiler-artifact\"") || !line.contains("\"print_export_probe\"") {
                continue;
            }
            if let Some(start) = line.find("\"executable\":\"") {
                let rest = &line[start + "\"executable\":\"".len()..];
                if let Some(end) = rest.find('"') {
                    let path = rest[..end].replace("\\\\", "\\");
                    return PathBuf::from(path);
                }
            }
        }
        panic!(
            "cargo build --message-format=json never reported an executable for \
             print_export_probe:\n{stdout}"
        );
    })
}

fn run_probe(out_dir: &std::path::Path, threads: &str) {
    let status = Command::new(probe_binary())
        .arg(out_dir)
        .env("RAYON_NUM_THREADS", threads)
        .status()
        .expect("failed to spawn the probe binary");
    assert!(
        status.success(),
        "print_export_probe exited non-zero under RAYON_NUM_THREADS={threads}"
    );
}

#[test]
fn stl_and_3mf_export_are_byte_identical_across_process_and_thread_count() {
    let base = std::env::temp_dir().join(format!(
        "alice-lol-print-export-determinism-{}",
        std::process::id()
    ));
    let dir_t1 = base.join("threads-1");
    let dir_t3 = base.join("threads-3");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&dir_t1).expect("create_dir_all threads-1");
    std::fs::create_dir_all(&dir_t3).expect("create_dir_all threads-3");

    run_probe(&dir_t1, "1");
    run_probe(&dir_t3, "3");

    let mut compared = 0;
    for ext in ["stl", "3mf"] {
        let a = std::fs::read(dir_t1.join(format!("probe.{ext}")))
            .unwrap_or_else(|e| panic!("read threads-1/probe.{ext}: {e}"));
        let b = std::fs::read(dir_t3.join(format!("probe.{ext}")))
            .unwrap_or_else(|e| panic!("read threads-3/probe.{ext}: {e}"));
        assert!(!a.is_empty(), "probe.{ext} (RAYON_NUM_THREADS=1) is empty");
        assert_eq!(
            a, b,
            "probe.{ext} differs between RAYON_NUM_THREADS=1 and RAYON_NUM_THREADS=3 \
             (same scene, two separate processes)"
        );
        compared += 1;
    }
    assert_eq!(compared, 2, "compared nothing");

    let _ = std::fs::remove_dir_all(&base);
}
