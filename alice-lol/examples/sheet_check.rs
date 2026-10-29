//! Check a filled character sheet set against its sheet law, and print what the law asks for
//!
//! ```text
//! sheet_check <filled.json>                  # the Japanese report (default law: laws/avatar/sheet.law)
//! sheet_check <filled.json> --json           # the JSON contract (sheet_check version 2)
//! sheet_check --spec                         # the specification an author follows (Markdown)
//! sheet_check --template                     # a blank fill (JSON)
//! sheet_check ... --law <sheet.law>          # another sheet law; its `uses` are relative to it
//! ```
//!
//! Exit status: 0 complete (or a document printed), 1 not complete, 2 usage, I/O or a fill
//! that is not a `sheet_fill 1` for the law.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use alice_lol::proportion_law::ProportionLaw;
use alice_lol::sheet_law::{SheetLaw, SHEET_CHECK_JSON_VERSION, SHEET_FILL_FORMAT};

const USAGE: &str = "usage: sheet_check <filled.json> [--json] [--law <sheet.law>] | sheet_check --spec | --template [--law <sheet.law>]";

fn load(path: &Path) -> Result<SheetLaw, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut law = SheetLaw::from_file(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut laws = Vec::new();
    for (alias, file) in &law.uses {
        laws.push((alias.clone(), proportion(&dir.join(file))?));
    }
    let refs: Vec<(&str, &ProportionLaw)> = laws.iter().map(|(a, l)| (a.as_str(), l)).collect();
    law.resolve(&refs)?;
    Ok(law)
}

/// A proportion law file, with the law it derives from read first (relative to it)
fn proportion(path: &Path) -> Result<ProportionLaw, String> {
    let t = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let parent = match ProportionLaw::derives_from(&t) {
        None => None,
        Some((name, file)) => {
            let dir = path.parent().unwrap_or_else(|| Path::new("."));
            Some((name, proportion(&dir.join(file))?))
        }
    };
    let given: Vec<(&str, &ProportionLaw)> = parent.iter().map(|(n, l)| (n.as_str(), l)).collect();
    ProportionLaw::from_file_with(&t, &given).map_err(|e| format!("{}: {e}", path.display()))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut json, mut spec, mut template) = (false, false, false);
    let mut law_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../laws/avatar/sheet.law");
    let mut fill = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--spec" => spec = true,
            "--template" => template = true,
            "--law" => match it.next() {
                Some(p) => law_path = PathBuf::from(p),
                None => return usage(),
            },
            s if s.starts_with("--") => return usage(),
            s if fill.is_none() => fill = Some(s.to_owned()),
            _ => return usage(),
        }
    }
    let law = match load(&law_path) {
        Ok(l) => l,
        Err(e) => return fail(&e),
    };
    if spec || template {
        if fill.is_some() || (spec && template) {
            return usage();
        }
        print!("{}", if spec { law.spec() } else { law.template() });
        return ExitCode::SUCCESS;
    }
    let Some(fill) = fill else { return usage() };
    let text = match std::fs::read_to_string(&fill) {
        Ok(t) => t,
        Err(e) => return fail(&format!("{fill}: {e}")),
    };
    match law.check(&text) {
        Ok(c) => {
            if json {
                println!("{}", c.to_json());
            } else {
                print!("{}", c.report_japanese());
            }
            if c.complete() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => fail(&format!("{fill}: {e}")),
    }
}

fn usage() -> ExitCode {
    let (fill, json) = (SHEET_FILL_FORMAT, SHEET_CHECK_JSON_VERSION);
    eprintln!("{USAGE}\nreads fills `sheet_fill {fill}`, writes JSON `sheet_check` version {json}");
    ExitCode::from(2)
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("sheet_check: {msg}");
    ExitCode::from(2)
}
