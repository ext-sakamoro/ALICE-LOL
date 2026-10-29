//! Draw a filled character sheet set as SVG pages (one per page of the sheet law), with each
//! page's identifier and the set's
//!
//! ```text
//! sheet_render <filled.json> <out dir>                 # default law: laws/avatar/sheet.law
//! sheet_render <filled.json> <out dir> --law <sheet.law>
//! ```
//!
//! Writes `<out dir>/<page>.svg` and `<out dir>/ids.txt` (`<page> <page id>` lines and `set <set id>`);
//! prints `[sheet-render]` log lines. Exit status: 0 drawn, 1 the fill is not complete, 2 usage or I/O.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use alice_lol::proportion_law::ProportionLaw;
use alice_lol::sheet_law::SheetLaw;
use alice_lol::sheet_render::hex;

const USAGE: &str = "usage: sheet_render <filled.json> <out dir> [--law <sheet.law>]";

fn load(path: &Path) -> Result<(SheetLaw, Vec<(String, ProportionLaw)>), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut law = SheetLaw::from_file(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut laws = Vec::new();
    for (alias, file) in &law.uses {
        laws.push((alias.clone(), proportion(&dir.join(file))?));
    }
    let refs: Vec<(&str, &ProportionLaw)> = laws.iter().map(|(a, l)| (a.as_str(), l)).collect();
    law.resolve(&refs)?;
    Ok((law, laws))
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
    let mut law_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../laws/avatar/sheet.law");
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
    let [fill_path, out] = pos.as_slice() else {
        return usage();
    };
    let (law, laws) = match load(&law_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[sheet-render] {e}");
            return ExitCode::from(2);
        }
    };
    let fill = match std::fs::read_to_string(fill_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[sheet-render] {fill_path}: {e}");
            return ExitCode::from(2);
        }
    };
    let refs: Vec<(&str, &ProportionLaw)> = laws.iter().map(|(a, l)| (a.as_str(), l)).collect();
    let r = match law.render(&fill, &refs) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[sheet-render] {e}");
            return ExitCode::from(1);
        }
    };
    for w in &r.warnings {
        println!("[sheet-render] warning {w}");
    }
    let out = PathBuf::from(out);
    if let Err(e) = std::fs::create_dir_all(&out) {
        eprintln!("[sheet-render] {}: {e}", out.display());
        return ExitCode::from(2);
    }
    let mut ids = String::new();
    for p in &r.pages {
        let path = out.join(format!("{}.svg", p.name));
        if let Err(e) = std::fs::write(&path, &p.svg) {
            eprintln!("[sheet-render] {}: {e}", path.display());
            return ExitCode::from(2);
        }
        let _ = writeln!(ids, "{} {}", p.name, hex(&p.page_id));
        println!(
            "[sheet-render] page {} {} ({} bytes)",
            p.name,
            hex(&p.page_id),
            p.svg.len()
        );
    }
    let _ = writeln!(ids, "set {}", hex(&r.set_id));
    println!(
        "[sheet-render] set {} ({} pages)",
        hex(&r.set_id),
        r.pages.len()
    );
    if let Err(e) = std::fs::write(out.join("ids.txt"), ids) {
        eprintln!("[sheet-render] ids.txt: {e}");
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}
