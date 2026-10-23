//! Read a motion law and print what it sets on each glyph at the given instants
//!
//! ```text
//! motion_check <law file> <glyphs> <t_ms>... [--param name=value]...   # the channels, as numbers and bits
//! motion_check <law file> --ref [--param name=value]...                # the law reference as bytes, read back and resolved
//! motion_check <law file> --frame <t_ms>... [--param name=value]...    # the frame hash of the probe word at each instant
//! motion_check --semantics                                           # the measured pins against the built-in ones
//! ```
//!
//! Exit status: 0 read / matched, 1 refused / differs, 2 usage or I/O error.

use std::process::ExitCode;

use alice_lol::law_ref::LawRef;
use alice_lol::motion_frame::{
    frame_hash, measured_frame_semantics_id, probe_word, render_frame, FRAME_SEMANTICS_ID,
};
use alice_lol::motion_law::{
    measured_motion_semantics_id, motion_semantics_fingerprints, MotionLaw, CHANNELS,
    MOTION_SEMANTICS_ID, MOTION_SEMANTICS_PINS,
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
    for ((name, pinned), (_, measured)) in MOTION_SEMANTICS_PINS
        .iter()
        .zip(motion_semantics_fingerprints())
    {
        println!("{name}: measured {}", hex(&measured));
        same &= *pinned == measured;
    }
    let frame_id = measured_frame_semantics_id();
    println!("FRAME_SEMANTICS_ID measured {}", hex(&frame_id));
    same &= frame_id == FRAME_SEMANTICS_ID;
    let id = measured_motion_semantics_id();
    println!("MOTION_SEMANTICS_ID measured {}", hex(&id));
    if same && id == MOTION_SEMANTICS_ID {
        println!("matches the pinned semantics");
        ExitCode::SUCCESS
    } else {
        println!(
            "differs from the pinned semantics {}",
            hex(&MOTION_SEMANTICS_ID)
        );
        ExitCode::from(1)
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: motion_check <law file> <glyphs> <t_ms>... [--param name=value]... | --semantics"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && args[0] == "--semantics" {
        return semantics();
    }
    if let [path, flag, rest @ ..] = args.as_slice() {
        if flag == "--ref" {
            return reference(path, rest);
        }
        if flag == "--frame" {
            return frames(path, rest);
        }
    }
    let [path, glyphs, rest @ ..] = args.as_slice() else {
        return usage();
    };
    let Ok(n) = glyphs.parse::<u32>() else {
        return usage();
    };
    let mut times = Vec::new();
    let mut params: Vec<(String, f64)> = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if a == "--param" {
            let Some((k, v)) = it.next().and_then(|p| p.split_once('=')) else {
                return usage();
            };
            let Ok(v) = v.parse::<f64>() else {
                return usage();
            };
            params.push((k.to_owned(), v));
        } else if let Ok(t) = a.parse::<u64>() {
            times.push(t);
        } else {
            return usage();
        }
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    let law = match MotionLaw::from_file(&text) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(1);
        }
    };
    println!("law {} id {}", law.name(), hex(&law.law_id()));
    for p in law.params() {
        println!("param {} = {} in {}..={}", p.name, p.default, p.lo, p.hi);
    }
    let given: Vec<(&str, f64)> = params.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let bound = match law.bind(&given) {
        Ok(b) => b,
        Err(e) => {
            println!("refused: {e}");
            return ExitCode::from(1);
        }
    };
    let timing = bound.timing();
    println!(
        "period {} ms, stagger {} ms",
        timing.period_ms, timing.stagger_ms
    );
    for t in times {
        for i in 0..n {
            match bound.sample(t, i, n) {
                Ok(c) => {
                    let bits = c.to_bits();
                    let shown: Vec<String> = CHANNELS
                        .iter()
                        .zip(bits)
                        .map(|(name, b)| format!("{name}={} ({b:08x})", f32::from_bits(b)))
                        .collect();
                    println!("t={t} glyph {i}: {}", shown.join(" "));
                }
                Err(e) => println!("t={t} glyph {i}: refused: {e}"),
            }
        }
    }
    ExitCode::SUCCESS
}

/// `--ref`: the reference for the law and the values, its bytes, and the same bytes read back
/// and resolved against the law
fn reference(path: &str, rest: &[String]) -> ExitCode {
    let mut params: Vec<(String, f64)> = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        let Some((k, v)) = (a == "--param")
            .then(|| it.next())
            .flatten()
            .and_then(|p| p.split_once('='))
        else {
            return usage();
        };
        let Ok(v) = v.parse::<f64>() else {
            return usage();
        };
        params.push((k.to_owned(), v));
    }
    let law = match std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|t| MotionLaw::from_file(&t).map_err(|e| e.to_string()))
    {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    let given: Vec<(&str, f64)> = params.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let bytes = match LawRef::motion(&law, &given).and_then(|r| r.to_bytes()) {
        Ok(b) => b,
        Err(e) => {
            println!("refused: {e}");
            return ExitCode::from(1);
        }
    };
    println!("reference {} bytes {}", bytes.len(), hex(&bytes));
    match LawRef::from_bytes(&bytes).and_then(|r| r.resolve_motion(&law).map(|b| b.timing())) {
        Ok(t) => {
            println!(
                "resolved: period {} ms, stagger {} ms",
                t.period_ms, t.stagger_ms
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("refused: {e}");
            ExitCode::from(1)
        }
    }
}

/// `--frame`: the frame hash of the probe word under the law at each instant
fn frames(path: &str, rest: &[String]) -> ExitCode {
    let mut times = Vec::new();
    let mut params: Vec<(String, f64)> = Vec::new();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if a == "--param" {
            let Some((k, v)) = it.next().and_then(|p| p.split_once('=')) else {
                return usage();
            };
            let Ok(v) = v.parse::<f64>() else {
                return usage();
            };
            params.push((k.to_owned(), v));
        } else if let Ok(t) = a.parse::<u64>() {
            times.push(t);
        } else {
            return usage();
        }
    }
    let law = match std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|t| MotionLaw::from_file(&t).map_err(|e| e.to_string()))
    {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    let given: Vec<(&str, f64)> = params.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let bound = match law.bind(&given) {
        Ok(b) => b,
        Err(e) => {
            println!("refused: {e}");
            return ExitCode::from(1);
        }
    };
    let word = probe_word();
    for t in times {
        match render_frame(&word, &bound, t) {
            Ok(f) => println!(
                "t={t} {}x{} frame {}",
                f.width,
                f.height,
                hex(&frame_hash(&f))
            ),
            Err(e) => {
                println!("t={t} refused: {e}");
                return ExitCode::from(1);
            }
        }
    }
    ExitCode::SUCCESS
}
