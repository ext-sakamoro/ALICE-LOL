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
//! - `scroll-sheet-390x844.json`: `js/test/fixtures/scroll-sheet.html` (a list sheet scrolling over
//!   a stage, a control bar below the stage), the fifth card half past the sheet's bottom edge: its
//!   whole box reaches over the bar's controls, but it is in the sheet's scroller and they are not.
//! - `scroll-sheet-overlap-390x844.json`: the same page with that card pulled up over the card
//!   above it (class `overlap`).

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
        "text-cover",
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
    assert!(out.contains("text_elements"), "{out}");
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
        alice_lol_ui::frame::FrameCheck::ALL.len(),
        "{out}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_probe_text_box_missing_a_side_is_refused_not_read_as_no_box() {
    // the collector keeps a side the probe left out as not-a-number, which JSON writes as null:
    // the report is refused, so a canvas label never passes the text checks by having no box
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("frame-cli-half-box");
    std::fs::create_dir_all(&dir).unwrap();
    let half = dir.join("half.json");
    std::fs::write(
        &half,
        r#"{"version":1,"viewport":{"w":390,"h":844,"dpr":3},"items":[{"id":"half","source":"probe","role":"readout","rect":[0,0,10,10],"text":{"scroll_w":46,"client_w":100,"scroll_h":null,"client_h":null}}],"samples":[]}"#,
    )
    .unwrap();
    let (code, out, err) = check(&[half.to_str().unwrap()]);
    let said = format!("{out}{err}");
    let whole = dir.join("whole.json");
    std::fs::write(
        &whole,
        r#"{"version":1,"viewport":{"w":390,"h":844,"dpr":3},"items":[{"id":"half","source":"probe","role":"readout","rect":[0,0,10,10],"text":{"scroll_w":46,"client_w":100,"scroll_h":17,"client_h":20}}],"samples":[]}"#,
    )
    .unwrap();
    let (whole_code, whole_out, _) = check(&[whole.to_str().unwrap()]);
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(code, 2, "{said}");
    assert!(said.contains("cannot read the report"), "{said}");
    // the same report with every side read: checked (and failing only on what it lacks)
    assert_eq!(whole_code, 1, "{whole_out}");
    assert!(whole_out.contains("text 1"), "{whole_out}");
}

#[test]
fn a_card_scrolled_past_its_sheet_does_not_overlap_the_bar_and_cards_overlapping_do() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("frame-cli-scroll-sheet");
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("ui-frame.json");
    std::fs::write(&cfg, r#"{"checks":["control-overlap"]}"#).unwrap();
    let run = |f: &PathBuf| -> (i32, serde_json::Value) {
        let (code, out, err) = check(&[
            "--config",
            cfg.to_str().unwrap(),
            "--json",
            f.to_str().unwrap(),
        ]);
        (
            code,
            serde_json::from_str(out.trim()).unwrap_or_else(|e| panic!("{e}: {out}{err}")),
        )
    };
    // the measured page: 9 controls compared, none overlapping
    let (code, v) = run(&fixture("scroll-sheet-390x844.json"));
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["compared"]["control-overlap"], 9, "{v}");
    // the same report without the scroller of each item (as the collector wrote it before): the
    // card's whole box is compared with the bar's controls, the false overlap this fixture pins
    let raw = std::fs::read_to_string(fixture("scroll-sheet-390x844.json")).unwrap();
    let mut no_scroller: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mut removed = 0;
    for item in no_scroller["items"].as_array_mut().unwrap() {
        if item.as_object_mut().unwrap().remove("scroller").is_some() {
            removed += 1;
        }
    }
    assert!(removed > 0, "the fixture carries scrollers");
    let stripped = dir.join("no-scroller.json");
    std::fs::write(&stripped, no_scroller.to_string()).unwrap();
    let (code, v) = run(&stripped);
    assert_eq!(code, 1, "{v}");
    assert!(
        v["violations"].as_array().unwrap().iter().any(|x| {
            x["ids"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i == "#cards>button:4")
                && x["ids"].as_array().unwrap().iter().any(|i| i == "#back")
        }),
        "{v}"
    );
    // two cards overlapping in the sheet are caught
    let (code, v) = run(&fixture("scroll-sheet-overlap-390x844.json"));
    assert_eq!(code, 1, "{v}");
    let viol = v["violations"].as_array().unwrap();
    assert_eq!(viol.len(), 1, "{v}");
    let ids = viol[0]["ids"].as_array().unwrap();
    assert!(
        ids.iter().any(|i| i == "#cards>button:3") && ids.iter().any(|i| i == "#cards>button:4"),
        "{v}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
