//! `alice-ui-frame-check [--config ui-frame.json] [--json] <report.json>...`: checks frame
//! reports (from `js/collect.mjs`) against [`FrameLaw`] and prints, per report, what each check
//! compared and every violation. `--config` reads the project's checks and thresholds;
//! `--json` prints one JSON object per report (the runner uses it to outline items).
//!
//! `alice-ui-frame-check run --config ui-frame.json --scenario <module> --url <base> --out <dir>
//! [--shots <dir>] [--label <name>]`: measures the page at the config's viewports in a browser
//! (`js/run.mjs`, carried in this binary) and checks each report; run it from the project whose
//! `node_modules` holds Playwright.
//!
//! Exit status: 0 every report passed, 1 a violation (a check that compared nothing included),
//! 2 a report or the config could not be read, or no report was given; `run` also 3 when a
//! scenario could not prepare the page.

use alice_lol_ui::frame::{FrameCheck, FrameConfig, FrameLaw, FrameReport};
use std::process::ExitCode;

fn read<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

/// The browser side, carried in the binary so one `cargo install` gives a project everything
const COLLECT_JS: &str = include_str!("../../js/collect.mjs");
const RUN_JS: &str = include_str!("../../js/run.mjs");

/// `run <run.mjs args>`: writes the browser side next to the run's output (`<out>/.frame-check-js/`)
/// and runs it with node, this binary as the checker (Playwright comes from the current
/// directory's project)
fn run(args: &[String]) -> ExitCode {
    let Some(out) = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1))
    else {
        eprintln!("run: --out <dir> is required");
        return ExitCode::from(2);
    };
    let dir = std::path::Path::new(out).join(".frame-check-js");
    let written = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(dir.join("collect.mjs"), COLLECT_JS))
        .and_then(|()| std::fs::write(dir.join("run.mjs"), RUN_JS));
    if let Err(e) = written {
        eprintln!("cannot write the runner to {}: {e}", dir.display());
        return ExitCode::from(2);
    }
    let me = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cannot find this binary: {e}");
            return ExitCode::from(2);
        }
    };
    let status = std::process::Command::new("node")
        .arg(dir.join("run.mjs"))
        .args(args)
        .arg("--checker")
        .arg(me)
        .status();
    match status {
        Ok(s) => ExitCode::from(u8::try_from(s.code().unwrap_or(2)).unwrap_or(2)),
        Err(e) => {
            eprintln!("cannot start node: {e}");
            ExitCode::from(2)
        }
    }
}

/// the law in force, on one line before the reports
fn print_law(law: &FrameLaw) {
    let names: Vec<&str> = law.checks().iter().map(|c| c.name()).collect();
    println!(
        "law: checks [{}], touch target >= {} px phone / {} px desktop, control cover <= {:.0} %, chrome <= {:.0} % phone / {:.0} % desktop, text >= {} px",
        names.join(", "),
        law.min_touch_target(true),
        law.min_touch_target(false),
        law.max_control_cover() * 100.0,
        law.max_chrome_share(true) * 100.0,
        law.max_chrome_share(false) * 100.0,
        law.min_text_px()
    );
}

const USAGE: &str = "usage: alice-ui-frame-check [--config ui-frame.json] [--json] <report.json>...
       alice-ui-frame-check run --config ui-frame.json --scenario <module> --url <base> --out <dir> [--shots <dir>] [--label <name>]
config (ui-frame.json): checks, touch_target {phone, desktop}, max_control_cover, chrome_share {phone, desktop},
  min_text_px, phone_max_short_side, scenarios {name: {checks}}; run also reads viewports [{name, w, h, touch}], roles {selector: role}, probe
exit: 0 pass, 1 a violation (a check that compared nothing included), 2 unreadable input, 3 (run) a scenario failed";

/// the options and report paths (`None`: `--config` without a path, reported)
fn parse(all: Vec<String>) -> Option<(Option<String>, bool, Vec<String>)> {
    let mut args = all.into_iter();
    let mut config: Option<String> = None;
    let mut json = false;
    let mut paths = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--config" => {
                let Some(p) = args.next() else {
                    eprintln!("--config needs a path");
                    return None;
                };
                config = Some(p);
            }
            "--json" => json = true,
            _ => paths.push(a),
        }
    }
    Some((config, json, paths))
}

fn main() -> ExitCode {
    let all: Vec<String> = std::env::args().skip(1).collect();
    if matches!(all.first().map(String::as_str), Some("--help" | "-h")) {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if all.first().map(String::as_str) == Some("run") {
        return run(&all[1..]);
    }
    let Some((config, json, paths)) = parse(all) else {
        return ExitCode::from(2);
    };
    if paths.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let law = match &config {
        None => FrameLaw::default(),
        Some(p) => match read::<FrameConfig>(p).and_then(|c| FrameLaw::from_config(&c)) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("{p}: cannot use the config: {e}");
                return ExitCode::from(2);
            }
        },
    };
    if !json {
        print_law(&law);
    }
    let mut failed = false;
    for path in &paths {
        let report: FrameReport = match read(path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{path}: cannot read the report: {e}");
                return ExitCode::from(2);
            }
        };
        let verdict = law.check(&report);
        if json {
            let compared: serde_json::Map<String, serde_json::Value> = verdict
                .compared
                .iter()
                .map(|(c, n)| (c.name().to_string(), (*n).into()))
                .collect();
            let violations: Vec<serde_json::Value> = verdict
                .violations
                .iter()
                .map(|v| {
                    serde_json::json!({
                        "check": v.check().map(FrameCheck::name),
                        "text": v.to_string(),
                        "ids": v.ids(),
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::json!({ "report": path, "passed": verdict.passed(), "compared": compared, "violations": violations })
            );
        } else {
            let counts: Vec<String> = verdict
                .compared
                .iter()
                .map(|(c, n)| format!("{} {n}", c.name()))
                .collect();
            println!(
                "{path}: {}x{} compared [{}] -> {}",
                report.viewport.w,
                report.viewport.h,
                counts.join(", "),
                if verdict.passed() {
                    "PASS".to_string()
                } else {
                    format!("{} violation(s)", verdict.violations.len())
                }
            );
            for v in &verdict.violations {
                println!("  {v}");
            }
        }
        failed |= !verdict.passed();
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
