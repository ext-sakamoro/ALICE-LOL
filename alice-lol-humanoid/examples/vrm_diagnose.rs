//! Check a VRM file (0.x or 1.0) for what general VRM readers handle badly
//!
//! ```text
//! vrm_diagnose <file.vrm>
//! ```
//!
//! Prints each finding and what every check compared. Exit status: 0 no finding, 1 some finding,
//! 2 usage or I/O error, or a file that is not a VRM / compared nothing.

use std::process::ExitCode;

use alice_lol_humanoid::vrm::{diagnose_glb, VrmReport};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path] = args.as_slice() else {
        eprintln!("usage: vrm_diagnose <file.vrm>");
        return ExitCode::from(2);
    };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    let report: VrmReport = match diagnose_glb(&bytes) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    for d in &report.diagnostics {
        println!("{d:?}");
    }
    let c = report.compared;
    println!(
        "compared: hips {}, height {}, feet {}, positions {}, materials {}, extensions {}, morphs {} (total {})",
        c.hips,
        c.height,
        c.feet,
        c.positions,
        c.materials,
        c.extensions,
        c.morphs,
        c.total()
    );
    if report.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
