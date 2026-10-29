//! `kind sheet`: the default sheet law `laws/avatar/sheet.law`, the check of a filled sheet,
//! and the documents generated from the law.
//!
//! oracle: a complete fill written by hand (`tests/data/sheet_fill_example.json`, invented
//! values, ranges inside the avatar laws). It must check complete with every value read; each
//! test then changes one value and the check must name exactly that place and that reason.
//! The spec, the blank template and the check JSON are pinned byte for byte, so what an author
//! is told and what a reader parses cannot drift without a visible change.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::path::{Path, PathBuf};

use alice_lol::proportion_law::ProportionLaw;
use alice_lol::sheet_law::{SheetCheck, SheetLaw, SHEET_CHECK_JSON_VERSION};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn law_text() -> String {
    std::fs::read_to_string(root().join("laws/avatar/sheet.law")).unwrap()
}

fn law() -> SheetLaw {
    let mut l = SheetLaw::from_file(&law_text()).unwrap();
    let body = ProportionLaw::from_file(
        &std::fs::read_to_string(root().join("laws/avatar/body.law")).unwrap(),
    )
    .unwrap();
    let face = ProportionLaw::from_file(
        &std::fs::read_to_string(root().join("laws/avatar/face.law")).unwrap(),
    )
    .unwrap();
    l.resolve(&[("avatar_body", &body), ("avatar_face", &face)])
        .unwrap();
    l
}

fn example() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/sheet_fill_example.json"),
    )
    .unwrap()
}

/// The fill with one exact text replaced (the text must occur once)
fn with(from: &str, to: &str) -> String {
    let e = example();
    assert_eq!(
        e.matches(from).count(),
        1,
        "`{from}` must occur once in the example"
    );
    e.replace(from, to)
}

fn findings(c: &SheetCheck) -> Vec<(String, &'static str)> {
    c.findings
        .iter()
        .map(|f| (f.path.clone(), f.problem.code()))
        .collect()
}

fn only(fill: &str, path: &str, code: &str) {
    let c = law().check(fill).unwrap();
    assert_eq!(
        findings(&c),
        [(path.to_owned(), code)],
        "{}",
        c.report_japanese()
    );
    assert!(!c.complete());
}

#[test]
fn the_example_fill_is_complete() {
    let c = law().check(&example()).unwrap();
    assert!(c.findings.is_empty(), "{}", c.report_japanese());
    assert!(c.complete());
    assert_eq!((c.read, c.asked), (175, 175));
    assert!(c.pages.iter().all(|(_, ok)| *ok));
    assert_eq!(c.pages.len(), 8);
}

#[test]
fn ranges_come_from_the_avatar_laws() {
    let l = law();
    let body = ProportionLaw::from_file(
        &std::fs::read_to_string(root().join("laws/avatar/body.law")).unwrap(),
    )
    .unwrap();
    let knee = body.params().iter().find(|p| p.name == "knee").unwrap();
    let slot = l
        .slots
        .iter()
        .find(|s| s.path() == "base_body.knee")
        .unwrap();
    assert_eq!(slot.range, Some((knee.lo, knee.hi)));
    // an unresolved law has no range: a fill is then not judged against a guess
    let raw = SheetLaw::from_file(&law_text()).unwrap();
    assert_eq!(
        raw.slots
            .iter()
            .find(|s| s.path() == "base_body.knee")
            .unwrap()
            .range,
        None
    );
    assert!(
        SheetLaw::from_file(&law_text())
            .unwrap()
            .resolve(&[("avatar_body", &body)])
            .is_err(),
        "avatar_face is used and must be given"
    );
}

#[test]
fn each_problem_is_named_at_its_place() {
    only(
        &with("\"stature\": 1.52,", "\"stature\": null,"),
        "turnaround.stature",
        "missing",
    );
    only(
        &with("\"knee\": 0.35,", "\"knee\": 0.5,"),
        "base_body.knee",
        "out_of_range",
    );
    only(
        &with(
            "\"views\": \"front_side_back\"",
            "\"views\": \"front_only\"",
        ),
        "turnaround.views",
        "not_in_vocabulary",
    );
    only(
        &with("\"joint_marks\": true", "\"joint_marks\": \"yes\""),
        "turnaround.joint_marks",
        "wrong_type",
    );
    only(
        &with("\"order\": 3,", "\"order\": 3.5,"),
        "clothing.layers[2].order",
        "wrong_type",
    );
    only(
        &with("\"srgb\": \"#c98a2e\"", "\"srgb\": \"orange\""),
        "palette.colours[6].srgb",
        "wrong_type",
    );
    only(
        &with(
            "\"note\": \"eyes half closed\"",
            &format!("\"note\": \"{}\"", "x".repeat(81)),
        ),
        "face.expressions[1].note",
        "too_long",
    );
    only(
        &with(
            "\"closure\": \"zip\", \"material\": \"wool\"",
            "\"closure\": \"zip\", \"material\": \"silk\"",
        ),
        "clothing.layers[2].material",
        "no_such_name",
    );
    only(
        &with("\"order\": 3,", "\"order\": 2,"),
        "clothing.layers[2].order",
        "not_unique",
    );
    only(
        &with(
            "\"from\": \"hips\", \"to\": \"knee\"",
            "\"from\": \"knee\", \"to\": \"hips\"",
        ),
        "clothing.layers[2]",
        "not_ordered",
    );
    only(
        &with("\"openness\": 0.45, ", ""),
        "materials.items[2].openness",
        "missing",
    );
    only(
        &with("\"author\": \"the sheet's author\", ", ""),
        "materials.items[5].texture",
        "asset_incomplete",
    );
    // the whole provenance missing, not only one of its fields
    let no_provenance = example();
    let start = no_provenance.find(", \"provenance\": {").unwrap();
    let end = no_provenance[start..].find("}}").unwrap() + start + 1;
    only(
        &format!("{}{}", &no_provenance[..start], &no_provenance[end..]),
        "materials.items[5].texture",
        "asset_incomplete",
    );
    only(
        &with(
            "\"arm_angle\": 30,",
            "\"arm_angle\": 30, \"colour\": \"blue\",",
        ),
        "turnaround.colour",
        "unknown_key",
    );
    only(
        &with("\"turnaround\": {", "\"extra_page\": {}, \"turnaround\": {"),
        "extra_page",
        "unknown_key",
    );
    only(
        &with(
            "{\"name\": \"neutral\"},",
            "{\"name\": \"neutral\", \"mood\": \"calm\"},",
        ),
        "face.expressions[0].mood",
        "unknown_key",
    );
}

#[test]
fn conditional_slots_are_asked_only_when_their_condition_holds() {
    // arm_angle is required for an A pose, not for a T pose
    only(
        &with("\"arm_angle\": 30,", ""),
        "turnaround.arm_angle",
        "missing",
    );
    let t = with(
        "\"pose\": \"A\",\n      \"arm_angle\": 30,",
        "\"pose\": \"T\",",
    );
    assert!(law().check(&t).unwrap().complete());
    // curl_pitch only for curled hair
    only(
        &with("\"curl_pitch\": 90,", ""),
        "hair.curl_pitch",
        "missing",
    );
    let straight = with(
        "\"curl_amplitude\": 4,\n      \"curl_pitch\": 90,",
        "\"curl_amplitude\": 0,",
    );
    assert!(law().check(&straight).unwrap().complete());
    // the weave fields only for an open weave
    let opaque = with("\"transmission\": \"open_weave\", \"openness\": 0.45, \"thread_colour\": \"cream\", \"thickness\": 0.15, \"stretch_sensitivity\": 0.3,", "\"transmission\": \"opaque\",");
    assert!(law().check(&opaque).unwrap().complete());
    let diffuse = with("\"transmission\": \"open_weave\", \"openness\": 0.45, \"thread_colour\": \"cream\", \"thickness\": 0.15, \"stretch_sensitivity\": 0.3,", "\"transmission\": \"diffuse\",");
    let c = law().check(&diffuse).unwrap();
    assert_eq!(
        findings(&c),
        [
            ("materials.items[2].transmittance".to_owned(), "missing"),
            ("materials.items[2].scattering".to_owned(), "missing")
        ]
    );
}

#[test]
fn group_sizes_are_checked() {
    let none = with("\"expressions\": [\n        {\"name\": \"neutral\"},\n        {\"name\": \"smile\", \"note\": \"eyes half closed\"},\n        {\"name\": \"surprise\"}\n      ]", "\"expressions\": []");
    only(&none, "face.expressions", "too_few");
    let twice = with("{\"name\": \"surprise\"}", "{\"name\": \"neutral\"}");
    only(&twice, "face.expressions[2].name", "not_unique");
}

#[test]
fn an_empty_fill_is_a_failure_not_a_success() {
    let c = law()
        .check("{\"sheet_fill\": 1, \"law\": \"avatar_sheet\", \"pages\": {}}")
        .unwrap();
    assert_eq!(c.read, 0);
    assert!(!c.complete());
    assert!(c
        .findings
        .iter()
        .any(|f| f.problem.code() == "nothing_read"));
    // the blank template reads nothing either, and asks for every required slot
    let t = law().check(&law().template()).unwrap();
    assert_eq!(t.read, 0);
    assert!(t
        .findings
        .iter()
        .any(|f| f.problem.code() == "nothing_read"));
    assert!(
        t.findings
            .iter()
            .filter(|f| f.problem.code() == "missing")
            .count()
            >= 40
    );
    // not a fill for this law: refused outright
    for bad in [
        "{\"sheet_fill\": 2, \"law\": \"avatar_sheet\", \"pages\": {}}",
        "{\"sheet_fill\": 1, \"law\": \"other\", \"pages\": {}}",
        "{\"sheet_fill\": 1, \"law\": \"avatar_sheet\", \"pages\": [], \"x\": 1}",
        "[1]",
    ] {
        assert!(law().check(bad).is_err(), "{bad}");
    }
}

#[test]
fn the_law_reader_refuses_malformed_laws() {
    let ok = law_text();
    let cases = [
        ("no kind", ok.replacen("kind sheet", "kind proportion", 1)),
        ("unknown line", format!("{ok}colour red\n")),
        ("slot on an unknown page", format!("{ok}slot nowhere.x number required source authored : x\n")),
        ("field before its group", format!("{ok}field hair.strands.count int required source authored : x\n")),
        ("rule on an unknown field", format!("{ok}rule unique palette.colours.hue : x\n")),
        ("ref to nothing", format!("{ok}slot hair.tint ref palette.colours.hue required source authored : x\n")),
        ("range on an enum", format!("{ok}slot hair.shine enum vocab a|b range 0 1 required source authored : x\n")),
        ("enum without vocab", format!("{ok}slot hair.shine enum required source authored : x\n")),
        ("slot twice", format!("{ok}slot hair.length number unit eye_height range 0 1 required source authored : x\n")),
        ("no need stated", format!("{ok}slot hair.shine number range 0 1 source authored : x\n")),
        ("unknown word", format!("{ok}slot hair.shine number range 0 1 required source authored shiny : x\n")),
        ("range law not used", format!("{ok}slot hair.shine number range law avatar_hair.x required source authored : x\n")),
        ("text twice", format!("{ok}text forbid tool_names : again\n")),
        ("text policy unknown", format!("{ok}text prefer logos : x\n")),
        ("text without a reason", format!("{ok}text forbid logos\n")),
    ];
    for (what, text) in &cases {
        assert!(SheetLaw::from_file(text).is_err(), "{what} was accepted");
    }
    assert_eq!(cases.len(), 15);
    assert!(SheetLaw::from_file(&ok).is_ok());
}

#[test]
fn the_law_id_ignores_comments_and_spacing_only() {
    let a = SheetLaw::from_file(&law_text()).unwrap();
    let spaced = law_text().replace("law avatar_sheet", "# a comment\nlaw   avatar_sheet");
    assert_eq!(SheetLaw::from_file(&spaced).unwrap().law_id(), a.law_id());
    let changed = law_text().replace("range 0.5 2.5", "range 0.5 2.6");
    assert_ne!(SheetLaw::from_file(&changed).unwrap().law_id(), a.law_id());
}

#[test]
fn every_pending_slot_is_listed_with_its_part() {
    let c = law().check(&example()).unwrap();
    assert!(c
        .pending
        .iter()
        .any(|(p, part)| p == "hair.style_class" && part == "hair"));
    assert!(c
        .pending
        .iter()
        .any(|(p, part)| p == "materials.items.transmission" && part == "material"));
    assert!(
        !c.pending.iter().any(|(p, _)| p.starts_with("base_body.")),
        "the body proportions are owned by avatar_body"
    );
}

fn golden(name: &str, got: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var_os("SHEET_BLESS").is_some() {
        std::fs::write(&path, got).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        got, want,
        "{name}: regenerate with SHEET_BLESS=1 only for an intended change"
    );
}

#[test]
fn the_generated_spec_and_template_are_pinned() {
    let l = law();
    golden("sheet_spec_avatar_sheet.md", &l.spec());
    golden("sheet_template_avatar_sheet.json", &l.template());
    // the spec names every slot and every group the check reads
    let spec = l.spec();
    // every table row has its six cells (a `|` inside a cell would split it)
    let rows: Vec<&str> = spec.lines().filter(|r| r.starts_with("| `")).collect();
    assert_eq!(rows.len(), l.slots.len(), "one row per slot and field");
    for r in &rows {
        assert_eq!(r.matches('|').count(), 7, "{r}");
    }
    for s in &l.slots {
        assert!(
            spec.contains(&format!("| `{}` |", s.name)),
            "{} not in the spec",
            s.path()
        );
    }
}

#[test]
fn the_check_json_is_pinned() {
    let fill = with("\"knee\": 0.35,", "\"knee\": 0.5,")
        .replace("\"stature\": 1.52,", "\"stature\": null,");
    let c = law().check(&fill).unwrap();
    let json = c.to_json();
    assert!(json.starts_with(&format!("{{\"sheet_check\":{SHEET_CHECK_JSON_VERSION},")));
    golden("sheet_check_v1.json", &format!("{json}\n"));
}

#[test]
fn what_may_be_written_on_a_page_is_part_of_the_law() {
    let l = law();
    let forbidden: Vec<&str> = l
        .texts
        .iter()
        .filter(|(allow, _, _)| !allow)
        .map(|(_, n, _)| n.as_str())
        .collect();
    assert_eq!(
        forbidden,
        ["tool_names", "unsourced_claims", "other_proportion_units"]
    );
    assert_eq!(l.texts.iter().filter(|(allow, _, _)| *allow).count(), 2);
    let spec = l.spec();
    assert!(spec.contains("## What may be written on a page"));
    for (_, n, _) in &l.texts {
        assert!(spec.contains(&format!("`{n}`")), "{n} not in the spec");
    }
}
