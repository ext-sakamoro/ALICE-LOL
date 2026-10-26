//! Measure a body in a binary FBX file in the units of a proportion law and bind the law to it
//!
//! ```text
//! measure_fbx <file.fbx> <body.law>
//! ```
//!
//! Prints each measured parameter and whether the law's range holds it. Exit status: 0 every
//! value is in range, 1 some are outside (another body type) or the body could not be measured,
//! 2 usage or I/O error.

use std::process::ExitCode;

use alice_fbx::{read_scene, read_tree};
use alice_lol::proportion_law::ProportionLaw;
use alice_lol_humanoid::fbx::body_marks;
use alice_lol_humanoid::measure::measure;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [fbx, law] = args.as_slice() else {
        eprintln!("usage: measure_fbx <file.fbx> <body.law>");
        return ExitCode::from(2);
    };
    let (bytes, text) = match (std::fs::read(fbx), std::fs::read_to_string(law)) {
        (Ok(b), Ok(t)) => (b, t),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let law = match ProportionLaw::from_file(&text) {
        Ok(l) => l,
        Err(e) => {
            println!("{e}");
            return ExitCode::from(1);
        }
    };
    let scene = match read_tree(&bytes).and_then(|t| read_scene(&t)) {
        Ok(s) => s,
        Err(e) => {
            println!("{e}");
            return ExitCode::from(1);
        }
    };
    let measured = match body_marks(&scene).and_then(|m| measure(&m)) {
        Ok(m) => m,
        Err(e) => {
            println!("{e}");
            return ExitCode::from(1);
        }
    };
    let mut inside = true;
    for (name, v) in measured.values() {
        let range = law.params().iter().find(|p| p.name == *name);
        let ok = range.is_some_and(|p| p.lo <= *v && *v <= p.hi);
        inside &= ok;
        match range {
            Some(p) => println!(
                "{name} = {v:.4} ({} {} .. {})",
                if ok { "in" } else { "OUTSIDE" },
                p.lo,
                p.hi
            ),
            None => println!("{name} = {v:.4} (the law has no such parameter)"),
        }
    }
    match law.bind(&measured.as_params()) {
        Ok(_) => println!("the law binds to this body"),
        Err(e) => println!("the law does not bind: {e}"),
    }
    if inside {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
