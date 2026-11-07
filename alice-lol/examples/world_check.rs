//! Read a world law and print its identifier and what it does, in plain text
//!
//! ```text
//! world_check <law file> [--param name=value]... [--switch name=on|off]...
//! world_check --semantics                       # the measured pins and their fold
//! ```
//!
//! Exit status: 0 read and bound, 1 refused, 2 usage or I/O error.

use std::process::ExitCode;

use alice_lol::world_law::{
    measured_world_semantics_id, world_semantics_fingerprints, WorldLaw, WorldValue,
};

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn usage() -> ExitCode {
    eprintln!("usage: world_check <law file> [--param name=value]... [--switch name=on|off]...");
    eprintln!("       world_check --semantics");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--semantics") {
        for (name, pin) in world_semantics_fingerprints() {
            println!("{} {name}", hex(&pin));
        }
        println!("{} fold", hex(&measured_world_semantics_id()));
        return ExitCode::SUCCESS;
    }
    let Some(path) = args.first() else {
        return usage();
    };
    let mut values: Vec<(&str, WorldValue)> = Vec::new();
    let mut rest = args[1..].iter();
    while let Some(flag) = rest.next() {
        let Some((name, value)) = rest.next().and_then(|a| a.split_once('=')) else {
            return usage();
        };
        let value = match (flag.as_str(), value) {
            ("--param", v) => match v.parse() {
                Ok(n) => WorldValue::Number(n),
                Err(_) => return usage(),
            },
            ("--switch", "on") => WorldValue::Switch(true),
            ("--switch", "off") => WorldValue::Switch(false),
            _ => return usage(),
        };
        values.push((name, value));
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    let law = match WorldLaw::from_file(&text) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(1);
        }
    };
    match law.bind(&values) {
        Ok(bound) => {
            println!("id: {}", hex(&law.law_id()));
            print!("{}", bound.describe());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{path}: {e}");
            ExitCode::from(1)
        }
    }
}
