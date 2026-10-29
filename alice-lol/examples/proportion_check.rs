//! Read a proportion law and print its outputs for the given parameter values (a law with a
//! `derive` line is read with the law it derives from, its file relative to the law's)
//!
//! ```text
//! proportion_check <law file> [--param name=value]...         # the outputs, as numbers and bits
//! proportion_check <law file> --get <output> [--param ...]     # one output by name
//! proportion_check <law file> --ref [--param name=value]...   # the law reference as bytes, read back and resolved
//! proportion_check --semantics                                # the measured pins against the built-in ones
//! ```
//!
//! Exit status: 0 read / matched, 1 refused / differs, 2 usage or I/O error.

use std::process::ExitCode;

use alice_lol::law_ref::LawRef;
use alice_lol::proportion_law::{
    measured_proportion_semantics_id, proportion_semantics_fingerprints, BoundProportion, Output,
    ProportionLaw, PROPORTION_SEMANTICS_ID, PROPORTION_SEMANTICS_PINS,
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
    for ((name, pinned), (_, measured)) in PROPORTION_SEMANTICS_PINS
        .iter()
        .zip(proportion_semantics_fingerprints())
    {
        println!("{name}: measured {}", hex(&measured));
        same &= *pinned == measured;
    }
    let id = measured_proportion_semantics_id();
    println!("PROPORTION_SEMANTICS_ID measured {}", hex(&id));
    if same && id == PROPORTION_SEMANTICS_ID {
        println!("matches the pinned semantics");
        ExitCode::SUCCESS
    } else {
        println!(
            "differs from the pinned semantics {}",
            hex(&PROPORTION_SEMANTICS_ID)
        );
        ExitCode::from(1)
    }
}

/// Bind the law directly, or through its reference written out as bytes and read back
fn bind<'a>(
    law: &'a ProportionLaw,
    named: &[(&str, f64)],
    reference: bool,
) -> Result<BoundProportion<'a>, String> {
    if !reference {
        return law.bind(named).map_err(|e| e.to_string());
    }
    let bytes = LawRef::proportion(law, named)
        .and_then(|r| r.to_bytes())
        .map_err(|e| e.to_string())?;
    println!("reference {} bytes {}", bytes.len(), hex(&bytes));
    LawRef::from_bytes(&bytes)
        .and_then(|back| back.resolve_proportion(law))
        .map_err(|e| e.to_string())
}

/// A proportion law file, with the law it derives from read first (relative to it)
fn read_law(path: &std::path::Path) -> Result<ProportionLaw, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let parent = match ProportionLaw::derives_from(&text) {
        None => None,
        Some((name, file)) => {
            let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
            Some((name, read_law(&dir.join(file))?))
        }
    };
    let given: Vec<(&str, &ProportionLaw)> = parent.iter().map(|(n, l)| (n.as_str(), l)).collect();
    ProportionLaw::from_file_with(&text, &given).map_err(|e| format!("{}: {e}", path.display()))
}

fn usage() -> ExitCode {
    eprintln!("usage: proportion_check <law file> [--ref] [--param name=value]... | --semantics");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--semantics" {
        return semantics();
    }
    let Some(path) = args.first() else {
        return usage();
    };
    let mut reference = false;
    let mut get: Option<String> = None;
    let mut values: Vec<(String, f64)> = Vec::new();
    let mut rest = args[1..].iter();
    while let Some(a) = rest.next() {
        match a.as_str() {
            "--ref" => reference = true,
            "--get" => match rest.next() {
                Some(n) => get = Some(n.clone()),
                None => return usage(),
            },
            "--param" => {
                let Some((n, v)) = rest.next().and_then(|p| p.split_once('=')) else {
                    return usage();
                };
                let Ok(v) = v.parse::<f64>() else {
                    return usage();
                };
                values.push((n.to_owned(), v));
            }
            _ => return usage(),
        }
    }
    if let Err(e) = std::fs::metadata(path) {
        eprintln!("{path}: {e}");
        return ExitCode::from(2);
    }
    let law = match read_law(std::path::Path::new(path)) {
        Ok(l) => l,
        Err(e) => {
            println!("{e}");
            return ExitCode::from(1);
        }
    };
    let named: Vec<(&str, f64)> = values.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    println!("law {} id {}", law.name(), hex(&law.law_id()));
    if let Some(p) = law.derived_from() {
        println!("derives from {p}");
    }
    for r in law.requires() {
        println!("require {r}");
    }
    println!(
        "outputs {}",
        law.output_names().collect::<Vec<_>>().join(" ")
    );
    let bound = match bind(&law, &named, reference) {
        Ok(b) => b,
        Err(e) => {
            println!("{e}");
            return ExitCode::from(1);
        }
    };
    if let Some(n) = get {
        return match (bound.scalar(&n), bound.point(&n)) {
            (Some(v), _) => {
                println!("out {n} = {v}");
                ExitCode::SUCCESS
            }
            (_, Some(p)) => {
                println!("point {n} = ({}, {}, {})", p[0], p[1], p[2]);
                ExitCode::SUCCESS
            }
            _ => {
                println!("the law has no output `{n}`");
                ExitCode::from(1)
            }
        };
    }
    for (p, v) in law.params().iter().zip(bound.values()) {
        println!("param {} = {v}", p.name);
    }
    for (name, o) in bound.outputs() {
        match o {
            Output::Scalar(v) => println!("out {name} = {v} ({:016x})", v.to_bits()),
            Output::Point(p) => println!(
                "point {name} = ({}, {}, {}) ({:016x} {:016x} {:016x})",
                p[0],
                p[1],
                p[2],
                p[0].to_bits(),
                p[1].to_bits(),
                p[2].to_bits()
            ),
        }
    }
    ExitCode::SUCCESS
}
