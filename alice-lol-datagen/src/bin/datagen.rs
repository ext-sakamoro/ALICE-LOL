//! `datagen --n 1000 --families all --seed 42 --out data.jsonl`
//!
//! 合成 (caption, LOL) pair を JSONL に書く 自己検証で捨てた件数は stderr に集計

use alice_lol_datagen::{generate, Family, ALL_FAMILIES};
use std::collections::BTreeMap;
use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::process;
use std::time::Instant;

fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

#[allow(clippy::cast_precision_loss)] // 件数 / 秒の表示のみ
fn main() {
    let args: Vec<String> = env::args().collect();
    let n: usize = arg_after(&args, "--n")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let seed: u64 = arg_after(&args, "--seed")
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);
    let fam_arg = arg_after(&args, "--families").unwrap_or("all");
    let families: Vec<Family> = if fam_arg == "all" {
        ALL_FAMILIES.to_vec()
    } else {
        fam_arg
            .split(',')
            .map(|s| {
                Family::from_name(s.trim()).unwrap_or_else(|| {
                    eprintln!(
                        "unknown family: {s} (known: {})",
                        ALL_FAMILIES
                            .iter()
                            .map(|f| f.name())
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                    process::exit(2)
                })
            })
            .collect()
    };
    let out_path = arg_after(&args, "--out");

    let t0 = Instant::now();
    let (samples, rejected) = generate(&families, n, seed);
    let elapsed = t0.elapsed();

    let mut out: Box<dyn Write> = match out_path {
        Some(p) => Box::new(BufWriter::new(File::create(p).unwrap_or_else(|e| {
            eprintln!("cannot create {p}: {e}");
            process::exit(1)
        }))),
        None => Box::new(BufWriter::new(std::io::stdout())),
    };
    for s in &samples {
        let _ = writeln!(out, "{}", s.to_jsonl());
    }
    let _ = out.flush();

    let mut per_family: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &samples {
        *per_family.entry(s.family).or_default() += 1;
    }
    let mut rej: BTreeMap<String, usize> = BTreeMap::new();
    for (f, e) in &rejected {
        *rej.entry(format!("{f}: {e}")).or_default() += 1;
    }
    eprintln!(
        "generated {} samples in {:.2}s ({:.0}/s), rejected {}",
        samples.len(),
        elapsed.as_secs_f64(),
        samples.len() as f64 / elapsed.as_secs_f64().max(1e-9),
        rejected.len()
    );
    for (f, c) in &per_family {
        eprintln!("  {f:<18} {c}");
    }
    for (r, c) in rej.iter().take(20) {
        eprintln!("  rejected x{c}: {r}");
    }
    if !rejected.is_empty() && samples.len() < n {
        process::exit(3);
    }
}
