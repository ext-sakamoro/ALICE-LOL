//! Check a three-view record against the avatar proportion laws
//!
//! ```text
//! three_view_check <record.txt>                 # a readable report
//! three_view_check <record.txt> --json          # the JSON contract (three_view_check version 1)
//! three_view_check <landmarks.json> --from-json # the record built from landmarks given as JSON
//! three_view_check ... --laws <dir>             # body.law and face.law from another directory
//! ```
//!
//! Exit status: 0 every derived value in range, 1 something out of range or two views
//! disagree, 2 usage, I/O or a refused record.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use alice_lol::proportion_law::ProportionLaw;
use alice_lol::three_view::{check, record_from_json, Record, Verdict};

const USAGE: &str =
    "usage: three_view_check <record.txt | landmarks.json --from-json> [--json] [--laws <dir>]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut file = None;
    let (mut json, mut from_json) = (false, false);
    let mut laws = Path::new(env!("CARGO_MANIFEST_DIR")).join("../laws/avatar");
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--from-json" => from_json = true,
            "--laws" => match it.next() {
                Some(d) => laws = PathBuf::from(d),
                None => return usage(),
            },
            s if s.starts_with("--") => return usage(),
            s if file.is_none() => file = Some(s.to_owned()),
            _ => return usage(),
        }
    }
    let Some(file) = file else { return usage() };
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) => return fail(&format!("{file}: {e}")),
    };
    let record = if from_json {
        record_from_json(&text)
    } else {
        Record::parse(&text)
    };
    let record = match record {
        Ok(r) => r,
        Err(e) => return fail(&format!("{file}: {e}")),
    };
    let load = |name: &str| -> Result<ProportionLaw, String> {
        let p = laws.join(name);
        let t = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        ProportionLaw::from_file(&t).map_err(|e| format!("{}: {e}", p.display()))
    };
    let (body, face) = match (load("body.law"), load("face.law")) {
        (Ok(b), Ok(f)) => (b, f),
        (Err(e), _) | (_, Err(e)) => return fail(&e),
    };
    let ranges = |l: &ProportionLaw| {
        l.params()
            .iter()
            .map(|p| (p.name.clone(), p.lo, p.hi))
            .collect::<Vec<_>>()
    };
    let c = match check(
        &record,
        (&body.law_id(), &ranges(&body)),
        (&face.law_id(), &ranges(&face)),
    ) {
        Ok(c) => c,
        Err(e) => return fail(&e),
    };
    let bad = c
        .judged
        .iter()
        .any(|j| matches!(j.verdict, Verdict::Below(_) | Verdict::Above(_)))
        || !c.derivation.cross_view.is_empty();
    if json {
        println!("{}", c.to_json());
    } else {
        report(&c);
    }
    if bad {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn report(c: &alice_lol::three_view::Check) {
    println!(
        "{:<12} {:<12} {:>9} {:>8}  {:<13} verdict",
        "law", "param", "value", "±", "range"
    );
    for j in &c.judged {
        let v = &j.value;
        let verdict = match j.verdict {
            Verdict::In => "in".to_owned(),
            Verdict::Edge => "edge (± reaches past an end)".to_owned(),
            Verdict::Below(d) => format!("BELOW by {d:.4}"),
            Verdict::Above(d) => format!("ABOVE by {d:.4}"),
        };
        let corr = v.correction.map_or_else(String::new, |k| {
            format!("  (calibrated correction {k:+.3})")
        });
        println!(
            "{:<12} {:<12} {:>9.4} {:>8.4}  {:<13} {verdict}{corr}",
            v.law.name(),
            v.name,
            v.value,
            v.u,
            format!("{:.2}..{:.2}", j.lo, j.hi)
        );
    }
    let d = &c.derivation;
    for f in &d.fixed {
        let agrees = if (f.value - f.fixed).abs() <= f.u {
            "agrees"
        } else {
            "DIFFERS"
        };
        println!(
            "fixed  {:<12} {:<12} {:>9.4} {:>8.4}  law fixes {:.2}: {agrees}",
            f.law.name(),
            f.name,
            f.value,
            f.u,
            f.fixed
        );
    }
    for v in &d.extra {
        println!(
            "extra  {:<12} {:>9.4} {:>8.4}  (in no law)",
            v.name, v.value, v.u
        );
    }
    for (law, name, why) in &d.not_measurable {
        println!("not measurable  {} {name}: {why}", law.name());
    }
    for x in &d.cross_view {
        println!(
            "CROSS-VIEW  {} on {} at {:.1} px vs {} at {:.1} px (allowed ±{:.1})",
            x.landmark,
            x.a.name(),
            x.height_a,
            x.b.name(),
            x.height_b,
            x.tolerance
        );
    }
    if let Some((m, u)) = d.eye_height_m {
        println!("eye height from the stature: {m:.4} ± {u:.4} m");
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("three_view_check: {msg}");
    ExitCode::from(2)
}
