//! Read a skeleton law and print its joints for the given parameter values
//!
//! ```text
//! skeleton_check <law file> [--param name=value]...        # every joint: parent and place, as numbers and bits
//! skeleton_check <law file> --get <joint> [--param ...]     # one joint's place by name
//! skeleton_check --semantics                               # the measured pins against the built-in ones
//! ```
//!
//! Exit status: 0 read / matched, 1 refused / differs, 2 usage or I/O error.

use std::process::ExitCode;

use alice_lol::skeleton_law::{
    measured_skeleton_semantics_id, skeleton_semantics_fingerprints, SkeletonLaw,
    SKELETON_SEMANTICS_ID, SKELETON_SEMANTICS_PINS,
};

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn semantics() -> ExitCode {
    let mut same = true;
    for ((name, pinned), (_, measured)) in SKELETON_SEMANTICS_PINS
        .iter()
        .zip(skeleton_semantics_fingerprints())
    {
        println!("{name}: measured {}", hex(&measured));
        same &= *pinned == measured;
    }
    let id = measured_skeleton_semantics_id();
    println!("SKELETON_SEMANTICS_ID measured {}", hex(&id));
    if same && id == SKELETON_SEMANTICS_ID {
        println!("matches the pinned semantics");
        ExitCode::SUCCESS
    } else {
        println!(
            "differs from the pinned semantics {}",
            hex(&SKELETON_SEMANTICS_ID)
        );
        ExitCode::from(1)
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: skeleton_check <law file> [--get <joint>] [--param name=value]... | --semantics"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--semantics") {
        return semantics();
    }
    let Some(path) = args.first() else {
        return usage();
    };
    let mut get = None;
    let mut values: Vec<(String, f64)> = Vec::new();
    let mut k = 1;
    while k < args.len() {
        match (args[k].as_str(), args.get(k + 1)) {
            ("--get", Some(n)) => get = Some(n.clone()),
            ("--param", Some(p)) => {
                let Some((n, v)) = p.split_once('=') else {
                    return usage();
                };
                let Ok(v) = v.parse::<f64>() else {
                    return usage();
                };
                values.push((n.to_owned(), v));
            }
            _ => return usage(),
        }
        k += 2;
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    let law = match SkeletonLaw::from_file(&text) {
        Ok(l) => l,
        Err(e) => {
            println!("refused: {e}");
            return ExitCode::from(1);
        }
    };
    let named: Vec<(&str, f64)> = values.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    let bound = match law.bind(&named) {
        Ok(b) => b,
        Err(e) => {
            println!("refused: {e}");
            return ExitCode::from(1);
        }
    };
    println!("law {} id {}", law.name(), hex(&law.law_id()));
    if let Some(n) = get {
        return bound.place(&n).map_or_else(
            || {
                println!("no joint `{n}`");
                ExitCode::from(1)
            },
            |p| {
                println!("{n} = ({}, {}, {})", p[0], p[1], p[2]);
                ExitCode::SUCCESS
            },
        );
    }
    let joint_names: Vec<&str> = bound.joints().map(|(n, _, _)| n).collect();
    for (n, parent, p) in bound.joints() {
        println!(
            "{n} parent {} = ({}, {}, {}) bits {:016x} {:016x} {:016x}",
            parent.map_or("-", |i| joint_names[i]),
            p[0],
            p[1],
            p[2],
            p[0].to_bits(),
            p[1].to_bits(),
            p[2].to_bits()
        );
    }
    ExitCode::SUCCESS
}
