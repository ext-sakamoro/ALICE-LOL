//! `alice-ui-frame-check` on frame reports measured from a real page (fixtures in
//! `tests/fixtures/frame/`): a web game at 390x844 in two states, before and after fixes.
//!
//! - `cleared-390x844.json`: the game's clear sheet is up, and the tilt knob of the selected part
//!   (same z-index as the sheet, later in the document) is drawn over it; the header takes 23.7 %
//!   of the screen; the hint and the readout below it overlap.
//! - `cleared-390x844-stand-in-fixes.json`: the same state with the sheet above the knob, the knob
//!   moved past the part's end, a header capped at 96 px and the hint hidden.
//! - `selected-plank-390x844.json`: the part selected, its 36 px knob centred on the end of a 75 px
//!   plank (covers 16-24 % of it).

use std::path::PathBuf;
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/frame")
        .join(name)
}

fn check(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_alice-ui-frame-check"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn lines<'a>(out: &'a str, prefix: &str) -> Vec<&'a str> {
    out.lines()
        .map(str::trim)
        .filter(|l| l.starts_with(prefix))
        .collect()
}

#[test]
fn the_reported_defects_are_found_in_the_measured_frame() {
    let f = fixture("cleared-390x844.json");
    let (code, out, err) = check(&[f.to_str().unwrap()]);
    assert_eq!(code, 1, "{out}{err}");
    // the knob over the clear sheet
    assert!(
        lines(&out, "layer-order:")
            .iter()
            .any(|l| l.starts_with("layer-order: #rotHandle is on top of overlay")),
        "{out}"
    );
    // the header share
    assert_eq!(
        lines(&out, "chrome-share:"),
        vec!["chrome-share: chrome covers 23.7 % of the viewport (max 12.0 %)"]
    );
    // the hint over the readout
    assert!(
        lines(&out, "text:")
            .iter()
            .any(|l| l.starts_with("text: #hint and #selInfo overlap")),
        "{out}"
    );
    // the first line is the law in force, with every check and the default thresholds
    assert!(
        out.lines()
            .next()
            .unwrap()
            .starts_with("law: checks [layer-order, hit-target, control-cover"),
        "{out}"
    );
    assert!(
        out.lines().next().unwrap().ends_with("text >= 12 px"),
        "{out}"
    );
    let summary = out.lines().find(|l| l.contains(" compared [")).unwrap();
    for c in [
        "layer-order",
        "hit-target",
        "control-cover",
        "control-overlap",
        "text",
        "offscreen",
        "chrome-share",
        "text-size",
    ] {
        assert!(summary.contains(&format!("{c} ")), "{summary}");
    }
    // the fixture was measured before the collector recorded text sizes, so text-size (and only
    // it) has nothing to compare there; every other check compared something
    assert_eq!(
        lines(&out, "")
            .iter()
            .filter(|l| l.ends_with("compared nothing"))
            .copied()
            .collect::<Vec<_>>(),
        vec!["text-size: compared nothing"],
        "{out}"
    );
}

#[test]
fn the_fixed_frame_clears_the_checks_the_fixes_address() {
    let f = fixture("cleared-390x844-stand-in-fixes.json");
    let (code, out, _) = check(&[f.to_str().unwrap()]);
    assert_eq!(code, 1, "{out}"); // other findings remain (hit targets below 44 px)
    assert!(lines(&out, "layer-order:").is_empty(), "{out}");
    assert!(lines(&out, "chrome-share:").is_empty(), "{out}");
    assert!(!out.contains("#hint and #selInfo"), "{out}");
    assert!(!out.contains("of part:"), "{out}");
}

#[test]
fn the_knob_on_its_plank_is_over_the_measured_maximum() {
    let f = fixture("selected-plank-390x844.json");
    let (_, out, _) = check(&[f.to_str().unwrap()]);
    let covers: Vec<&str> = lines(&out, "control-cover: #rotHandle covers")
        .into_iter()
        .filter(|l| l.contains("of part:"))
        .collect();
    assert_eq!(covers.len(), 1, "{out}");
    let share: f32 = covers[0]
        .split(" covers ")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!((10.0..=30.0).contains(&share), "{share}");
}

#[test]
fn a_config_selects_checks_and_json_names_the_items() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("frame-cli-config");
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("ui-frame.json");
    std::fs::write(&cfg, r#"{"checks":["layer-order"],"viewports":[]}"#).unwrap();
    let f = fixture("cleared-390x844.json");
    let (code, out, err) = check(&[
        "--config",
        cfg.to_str().unwrap(),
        "--json",
        f.to_str().unwrap(),
    ]);
    assert_eq!(code, 1, "{out}{err}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["compared"].as_object().unwrap().len(), 1);
    let viol = &v["violations"][0];
    assert_eq!(viol["check"], "layer-order");
    assert!(viol["ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i == "#rotHandle"));
    // the text minimum of the config is the one in force
    std::fs::write(&cfg, r#"{"checks":["layer-order"],"min_text_px":9}"#).unwrap();
    let (_, out, err) = check(&["--config", cfg.to_str().unwrap(), f.to_str().unwrap()]);
    assert!(
        out.lines()
            .next()
            .is_some_and(|l| l.ends_with("text >= 9 px")),
        "{out}{err}"
    );
    // a bad config and a bad report are exit 2, as is no report
    std::fs::write(&cfg, r#"{"checks":["nope"]}"#).unwrap();
    assert_eq!(
        check(&["--config", cfg.to_str().unwrap(), f.to_str().unwrap()]).0,
        2
    );
    let bad = dir.join("bad.json");
    std::fs::write(&bad, "{").unwrap();
    assert_eq!(check(&[bad.to_str().unwrap()]).0, 2);
    assert_eq!(check(&[]).0, 2);
    // --help prints the usage with the config keys and exits 0
    let (code, out, _) = check(&["--help"]);
    assert_eq!(code, 0);
    assert!(out.contains("touch_target {phone, desktop}"), "{out}");
    assert!(out.contains("min_text_px"), "{out}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn an_empty_report_fails_every_check() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("frame-cli-empty");
    std::fs::create_dir_all(&dir).unwrap();
    let empty = dir.join("empty.json");
    std::fs::write(
        &empty,
        r#"{"version":1,"viewport":{"w":390,"h":844,"dpr":3},"items":[],"samples":[]}"#,
    )
    .unwrap();
    let (code, out, _) = check(&[empty.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert_eq!(
        lines(&out, "")
            .iter()
            .filter(|l| l.ends_with("compared nothing"))
            .count(),
        8,
        "{out}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
