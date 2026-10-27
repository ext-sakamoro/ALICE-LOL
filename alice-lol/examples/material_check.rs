//! Read a material law and print what it binds to and the fields at a point
//!
//! ```text
//! material_check <law file> [--param name=value]...                      # tile, cells, noises, step
//! material_check <law file> --at u,v,edge,c0,c1,c2[,noise...] [--param ...]  # the fields at that point
//! material_check --semantics                                             # the measured pins against the built-in ones
//! ```
//!
//! The cell numbers and noise values of `--at` are what the surface's evaluator would give at the
//! point; the law combines them. Exit status: 0 read / matched, 1 refused / differs, 2 usage or
//! I/O error.

use std::process::ExitCode;

use alice_lol::material_law::{
    material_semantics_fingerprints, measured_material_semantics_id, BoundCell, BoundMaterial,
    MaterialLaw, MaterialPoint, MATERIAL_SEMANTICS_ID, MATERIAL_SEMANTICS_PINS,
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
    for ((name, pinned), (_, measured)) in MATERIAL_SEMANTICS_PINS
        .iter()
        .zip(material_semantics_fingerprints())
    {
        println!("{name}: measured {}", hex(&measured));
        same &= *pinned == measured;
    }
    let id = measured_material_semantics_id();
    println!("MATERIAL_SEMANTICS_ID measured {}", hex(&id));
    if same && id == MATERIAL_SEMANTICS_ID {
        println!("matches the pinned semantics");
        ExitCode::SUCCESS
    } else {
        println!(
            "differs from the pinned semantics {}",
            hex(&MATERIAL_SEMANTICS_ID)
        );
        ExitCode::from(1)
    }
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("material_check: {msg}");
    eprintln!("usage: material_check <law file> [--at u,v,edge,c0,c1,c2[,noise...]] [--param name=value]...");
    eprintln!("       material_check --semantics");
    ExitCode::from(2)
}

/// The tile, cells, noises and step of a bound law
fn print_bound(bound: &BoundMaterial<'_>) {
    let t = bound.tile();
    println!("tile {} x {} m", t[0], t[1]);
    match bound.cell() {
        None => println!("cell none"),
        Some(BoundCell::Grid { period, row_shift }) => {
            println!(
                "cell grid {} x {} m, row shift {row_shift}",
                period[0], period[1]
            );
        }
        Some(BoundCell::Voronoi { period, jitter }) => {
            println!("cell voronoi {period} m, jitter {jitter}");
        }
    }
    for (n, f, a, s) in bound.noises() {
        println!("noise {n}: {f} cycles/m, amplitude {a}, seed {s}");
    }
    println!("normal_step {} m", bound.normal_step());
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--semantics") {
        return semantics();
    }
    let Some(path) = args.first() else {
        return usage("no law file");
    };
    let mut params: Vec<(String, f64)> = Vec::new();
    let mut at: Option<Vec<f64>> = None;
    let mut rest = args[1..].iter();
    while let Some(a) = rest.next() {
        match a.as_str() {
            "--param" => {
                let Some((n, v)) = rest.next().and_then(|p| p.split_once('=')) else {
                    return usage("--param name=value");
                };
                let Ok(v) = v.parse::<f64>() else {
                    return usage("--param value is not a number");
                };
                params.push((n.to_owned(), v));
            }
            "--at" => {
                let Some(list) = rest.next() else {
                    return usage("--at u,v,edge,c0,c1,c2[,noise...]");
                };
                let Ok(vals) = list
                    .split(',')
                    .map(str::parse::<f64>)
                    .collect::<Result<Vec<_>, _>>()
                else {
                    return usage("--at values are numbers");
                };
                if vals.len() < 6 {
                    return usage("--at needs u, v, edge and three cell numbers");
                }
                at = Some(vals);
            }
            other => return usage(&format!("unknown argument `{other}`")),
        }
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("material_check: {path}: {e}");
            return ExitCode::from(2);
        }
    };
    let law = match MaterialLaw::from_file(&text) {
        Ok(l) => l,
        Err(e) => {
            println!("refused: {e}");
            return ExitCode::from(1);
        }
    };
    println!("law {} id {}", law.name(), hex(&law.law_id()));
    println!(
        "noise values of --at, in order: {}",
        law.noise_names().collect::<Vec<_>>().join(", ")
    );
    let named: Vec<(&str, f64)> = params.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    let bound = match law.bind(&named) {
        Ok(b) => b,
        Err(e) => {
            println!("refused: {e}");
            return ExitCode::from(1);
        }
    };
    print_bound(&bound);
    let Some(v) = at else {
        return ExitCode::SUCCESS;
    };
    let point = MaterialPoint {
        u: v[0],
        v: v[1],
        edge: v[2],
        cell: [v[3], v[4], v[5]],
        noise: v[6..].to_vec(),
    };
    match bound.sample(&point) {
        Ok(s) => {
            println!("height {} (bits {:016x})", s.height, s.height.to_bits());
            println!("albedo {} {} {}", s.albedo[0], s.albedo[1], s.albedo[2]);
            println!("roughness {}", s.roughness);
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("refused: {e}");
            ExitCode::from(1)
        }
    }
}
