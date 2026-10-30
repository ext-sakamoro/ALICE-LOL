//! Sample variants of a knob law from a neutral base and report how evenly they cover each knob
//!
//! ```text
//! variant_sample <seed> <count>                  # default law: laws/avatar/body_knobs.law
//! variant_sample <seed> <count> --law <knobs.law>
//! ```
//!
//! The base here is a placeholder neutral base (32 zero bytes) and the only gate accepts everything
//! (it compares each knob once): the real gates of a part are the caller's. Prints the first variants'
//! identifiers, the sampling report and the χ² of every knob over 16 bins. Exit status: 0 done, 1 a
//! knob's χ² above its bound (0.1 % shared among the knobs), 2 usage or I/O.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use alice_lol::proportion_law::ProportionLaw;
use alice_lol::sheet_render::hex;
use alice_lol::variant::{sample, uniformity, Base, GateResult, KnobTable};

const USAGE: &str = "usage: variant_sample <seed> <count> [--law <knobs.law>]";

/// χ²(15) at the upper 0.1 % / 21 point (Bonferroni over the body knob law's 21 knobs), from the
/// Wilson-Hilferty approximation
const BOUND: f64 = 46.7;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut law_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../laws/avatar/body_knobs.law");
    let mut pos = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--law" => match it.next() {
                Some(p) => law_path = PathBuf::from(p),
                None => return usage(),
            },
            s if s.starts_with("--") => return usage(),
            s => pos.push(s.to_owned()),
        }
    }
    let [seed, count] = pos.as_slice() else {
        return usage();
    };
    let (Ok(seed), Ok(count)) = (seed.parse::<u64>(), count.parse::<u64>()) else {
        return usage();
    };
    let law = match std::fs::read_to_string(&law_path)
        .map_err(|e| e.to_string())
        .and_then(|t| ProportionLaw::from_file(&t).map_err(|e| e.to_string()))
    {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[variant] {}: {e}", law_path.display());
            return ExitCode::from(2);
        }
    };
    let table = KnobTable::of(&law);
    let mut gate = |v: &alice_lol::variant::Variant| {
        vec![GateResult {
            gate: "accept".into(),
            compared: v.q.len() as u64,
            failed: 0,
        }]
    };
    let (variants, report) = match sample(
        &table,
        Base::Neutral([0; 32]),
        seed,
        count,
        count,
        &[],
        &mut gate,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[variant] {e}");
            return ExitCode::from(2);
        }
    };
    // every knob at 0 steps: the base itself
    match table.variant(Base::Neutral([0; 32]), &[], &[]) {
        Ok(base) => println!(
            "[variant] base {} (every knob at its default)",
            hex(&base.id())
        ),
        Err(e) => {
            eprintln!("[variant] {e}");
            return ExitCode::from(2);
        }
    }
    for v in variants.iter().take(3) {
        println!("[variant] {}", hex(&v.id()));
    }
    println!(
        "[variant] {} trials, {} accepted ({:.3})",
        report.trials,
        report.accepted,
        report.acceptance()
    );
    let mut worst = 0.0_f64;
    for k in table.knobs() {
        match uniformity(&table, &variants, &k.name, 16) {
            Ok(u) => {
                println!(
                    "[variant] {} χ² {:.2} (dof {}, compared {})",
                    k.name, u.chi_square, u.dof, u.compared
                );
                if u.chi_square > worst {
                    worst = u.chi_square;
                }
            }
            Err(e) => {
                eprintln!("[variant] {}: {e}", k.name);
                return ExitCode::from(2);
            }
        }
    }
    if worst > BOUND {
        eprintln!("[variant] a knob's χ² {worst:.2} is above {BOUND}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}
