//! `kind sheet`: the default sheet law `laws/avatar/sheet.law`, the check of a filled sheet,
//! and the documents generated from the law.
//!
//! oracle: a complete fill written by hand (`tests/data/sheet_fill_example.json`, invented
//! values, ranges inside the avatar laws). It must check complete with every value read; each
//! test then changes one value and the check must name exactly that place and that reason.
//! The spec, the blank template and the check JSON are pinned byte for byte, so what an author
//! is told and what a reader parses cannot drift without a visible change.
//!
//! The plausibility rules are tested one change at a time the same way, each against the
//! clause of the law that names it. Two answers written by an outside model from the
//! specification alone (`tests/data/sheet_fill_outside_answer_{1,2}.json`) are kept as they
//! came: the rules must name the same places in them (what the rules were added for).
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

/// A proportion law of `laws/avatar`, with the law it derives from read first
fn proportion(file: &str) -> ProportionLaw {
    let text = std::fs::read_to_string(root().join("laws/avatar").join(file)).unwrap();
    match ProportionLaw::derives_from(&text) {
        None => ProportionLaw::from_file(&text).unwrap(),
        Some((name, parent)) => {
            ProportionLaw::from_file_with(&text, &[(name.as_str(), &proportion(&parent))]).unwrap()
        }
    }
}

fn law() -> SheetLaw {
    let mut l = SheetLaw::from_file(&law_text()).unwrap();
    let laws: Vec<(String, ProportionLaw)> = l
        .uses
        .iter()
        .map(|(a, f)| (a.clone(), proportion(f)))
        .collect();
    let given: Vec<(&str, &ProportionLaw)> = laws.iter().map(|(a, p)| (a.as_str(), p)).collect();
    l.resolve(&given).unwrap();
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
    assert_eq!((c.read, c.asked), (200, 200));
    assert!(c.pages.iter().all(|(_, ok)| *ok));
    assert_eq!(c.pages.len(), 9);
    assert!(c.warnings.is_empty(), "{}", c.report_japanese());
}

#[test]
fn ranges_come_from_the_avatar_laws() {
    let l = law();
    let range = |file: &str, name: &str| {
        let p = proportion(file);
        let q = p.params().iter().find(|q| q.name == name).unwrap();
        (q.lo, q.hi)
    };
    let slot =
        |l: &SheetLaw, path: &str| l.slots.iter().find(|s| s.path() == path).unwrap().clone();
    // a range through a family alias is one range per family, from that family's law
    assert_eq!(
        slot(&l, "base_body.knee").family_ranges,
        [
            ("reference".to_owned(), range("body.law", "knee")),
            ("stylised".to_owned(), range("body_stylised.law", "knee"))
        ]
    );
    assert_eq!(
        slot(&l, "face.eye_height").family_ranges,
        [
            ("reference".to_owned(), range("face.law", "eye_height")),
            (
                "stylised".to_owned(),
                range("face_stylised.law", "eye_height")
            )
        ]
    );
    // only the stylised body has a shoulder parameter: the slot is asked only for it
    assert_eq!(
        slot(&l, "base_body.shoulder").family_ranges,
        [(
            "stylised".to_owned(),
            range("body_stylised.law", "shoulder")
        )]
    );
    // an unresolved law has no range: a fill is then not judged against a guess
    let raw = SheetLaw::from_file(&law_text()).unwrap();
    assert!(slot(&raw, "base_body.knee").family_ranges.is_empty());
    assert_eq!(slot(&raw, "base_body.knee").range, None);
    let body = proportion("body.law");
    assert!(
        SheetLaw::from_file(&law_text())
            .unwrap()
            .resolve(&[("avatar_body", &body)])
            .is_err(),
        "the other used laws must be given"
    );
}

#[test]
fn the_family_slot_chooses_the_ranges() {
    // the same knee: out of the reference range, inside the stylised one
    only(
        &with("\"knee\": 0.35,", "\"knee\": 0.29,"),
        "base_body.knee",
        "out_of_range",
    );
    let stylised = with(
        "\"family\": \"reference\",",
        "\"family\": \"stylised\",\n      \"shoulder\": 0.88,",
    );
    // the reference face's eyes and brows are out of the stylised ranges
    let c = law().check(&stylised).unwrap();
    assert_eq!(
        findings(&c),
        [
            ("face.eye_height".to_owned(), "out_of_range"),
            ("face.brow_y".to_owned(), "out_of_range")
        ]
    );
    let drawn = stylised
        .replace("\"eye_height\": 0.28,", "\"eye_height\": 0.5,")
        .replace("\"brow_y\": 1.27,", "\"brow_y\": 1.35,")
        .replace("\"mouth_y\": 0.32,", "\"mouth_y\": 0.4,");
    complete(&drawn.replace("\"knee\": 0.35,", "\"knee\": 0.29,"));
    // a stylised body is asked its shoulder, a reference body is not
    only(
        &drawn.replace("\n      \"shoulder\": 0.88,", ""),
        "base_body.shoulder",
        "missing",
    );
    warns(
        &with(
            "\"family\": \"reference\",",
            "\"family\": \"reference\",\n      \"shoulder\": 0.88,",
        ),
        "base_body.shoulder",
        "not_used",
    );
    // no family: the family slot is missing and no family range is judged (the knee 0.29 is
    // not listed)
    let none =
        with("\"family\": \"reference\",\n", "").replace("\"knee\": 0.35,", "\"knee\": 0.29,");
    only(&none, "base_body.family", "missing");
    only(
        &with("\"family\": \"reference\",", "\"family\": \"chibi\","),
        "base_body.family",
        "not_in_vocabulary",
    );
    // the spec names both families' ranges
    let spec = law().spec();
    assert!(
        spec.contains("reference: 0.31 – 0.4; stylised: 0.27 – 0.38 (from body.knee)"),
        "{spec}"
    );
}

#[test]
fn the_family_lines_that_do_not_read_are_refused() {
    let ok = law_text();
    let swap = |a: &str, b: &str| {
        assert_eq!(ok.matches(a).count(), 1, "{a}");
        ok.replace(a, b)
    };
    let cases = [
        ("a value not in the vocabulary", swap("family base_body.family stylised:", "family base_body.family drawn:")),
        ("a value without a line", swap("family base_body.family stylised: body=avatar_body_stylised face=avatar_face_stylised\n", "")),
        ("other aliases", swap("stylised: body=avatar_body_stylised face=avatar_face_stylised", "stylised: body=avatar_body_stylised")),
        ("a law not used", swap("uses avatar_face_stylised face_stylised.law\n", "")),
        ("an alias that is a used law", swap("reference: body=avatar_body face=avatar_face", "reference: avatar_body=avatar_body face=avatar_face")),
        ("a slot that is not an enum", swap("family base_body.family stylised:", "family base_body.knee stylised:")),
        ("a line twice", format!("{ok}family base_body.family stylised: body=avatar_body face=avatar_face\n")),
        ("not alias=law", swap("reference: body=avatar_body face=avatar_face", "reference: body avatar_body face=avatar_face")),
        ("a range alias that is neither", swap("range law body.knee", "range law torso.knee")),
    ];
    for (what, text) in &cases {
        assert!(SheetLaw::from_file(text).is_err(), "{what} was accepted");
    }
    assert_eq!(cases.len(), 9);
    // a slot asked for every family, of a parameter one family lacks, does not resolve
    let mut l = SheetLaw::from_file(&swap(
        "required when family=stylised source law:body.shoulder",
        "required source law:body.shoulder",
    ))
    .unwrap();
    let laws: Vec<(String, ProportionLaw)> = l
        .uses
        .iter()
        .map(|(a, f)| (a.clone(), proportion(f)))
        .collect();
    let given: Vec<(&str, &ProportionLaw)> = laws.iter().map(|(a, p)| (a.as_str(), p)).collect();
    assert!(l
        .resolve(&given)
        .unwrap_err()
        .contains("avatar_body has no parameter `shoulder`"));
}

#[test]
#[allow(clippy::too_many_lines)] // one change per kind of problem, in the order of the codes
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
        ("rule without a reason", format!("{ok}rule when clothing.layers class=skin require ease<=3\n")),
        ("word not in the vocabulary", format!("{ok}rule when clothing.layers class=skn require ease<=3 : x\n")),
        ("deref of a field nobody has", format!("{ok}rule when clothing.layers class=skin require material->elastcity>=0.2 : x\n")),
        ("deref of a non-reference", format!("{ok}rule when clothing.layers class=skin require ease->x>=0.2 : x\n")),
        ("bound on no slot", format!("{ok}rule warn when accessories.items kind=footwear require sole+heel<=turnaround.statur*80 : x\n")),
        ("along over two groups", format!("{ok}rule along clothing.layers.from accessories.items.attach : x\n")),
        ("along as a warning", format!("{ok}rule warn along clothing.layers.from clothing.layers.to : x\n")),
        ("require ending in or", format!("{ok}rule when clothing.layers class=skin require ease<=3 or : x\n")),
        ("chain twice", format!("{ok}chain arm shoulder wrist\n")),
        ("ordered is gone", format!("{ok}rule ordered clothing.layers.from clothing.layers.to : x\n")),
    ];
    for (what, text) in &cases {
        assert!(SheetLaw::from_file(text).is_err(), "{what} was accepted");
    }
    assert_eq!(cases.len(), 25);
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
        .replace("\"stature\": 1.52,", "\"stature\": null,")
        .replace("\"law\": \"avatar_sheet\",", "\"law\": \"avatar_sheet\",\n  \"out_of_range_intent\": [{\"slot\": \"base_body.knee\", \"wants\": 0.27, \"why\": \"the knee is drawn low\"}],");
    let c = law().check(&fill).unwrap();
    let json = c.to_json();
    assert!(json.starts_with(&format!("{{\"sheet_check\":{SHEET_CHECK_JSON_VERSION},")));
    assert!(
        json.contains("\"warnings\":[{"),
        "the note is listed as a warning"
    );
    golden("sheet_check_v2.json", &format!("{json}\n"));
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

fn warnings(c: &SheetCheck) -> Vec<(String, &'static str)> {
    c.warnings
        .iter()
        .map(|f| (f.path.clone(), f.problem.code()))
        .collect()
}

/// The fill checks complete, with exactly this warning
fn warns(fill: &str, path: &str, code: &str) {
    let c = law().check(fill).unwrap();
    assert!(c.findings.is_empty(), "{}", c.report_japanese());
    assert!(c.complete(), "a warning does not stop the drawing");
    assert_eq!(
        warnings(&c),
        [(path.to_owned(), code)],
        "{}",
        c.report_japanese()
    );
}

fn complete(fill: &str) {
    let c = law().check(fill).unwrap();
    assert!(
        c.complete() && c.warnings.is_empty(),
        "{}",
        c.report_japanese()
    );
}

#[test]
fn a_layer_spans_one_chain_from_above() {
    // knee is on the body chain, elbow on the arm chain: no span
    only(
        &with(
            "\"from\": \"shoulder\", \"to\": \"wrist\"",
            "\"from\": \"elbow\", \"to\": \"knee\"",
        ),
        "clothing.layers[1]",
        "implausible",
    );
    // a sleeve: on the arm chain from the shoulder, so shoulder is on two chains
    complete(&with(
        "\"from\": \"shoulder\", \"to\": \"wrist\"",
        "\"from\": \"shoulder\", \"to\": \"elbow\"",
    ));
    only(
        &with(
            "\"from\": \"shoulder\", \"to\": \"wrist\"",
            "\"from\": \"wrist\", \"to\": \"shoulder\"",
        ),
        "clothing.layers[1]",
        "not_ordered",
    );
}

#[test]
fn a_sleeve_belongs_to_a_garment_or_is_worn_apart() {
    only(
        &with("\"part_of\": \"blouse\", ", ""),
        "clothing.layers[1]",
        "implausible",
    );
    complete(&with(
        "\"class\": \"loose\", \"part_of\": \"blouse\"",
        "\"class\": \"detached_sleeve\"",
    ));
    only(
        &with("\"part_of\": \"blouse\"", "\"part_of\": \"cape\""),
        "clothing.layers[1].part_of",
        "no_such_name",
    );
}

#[test]
fn the_ease_follows_the_class() {
    // skin: at most 3 mm, and a stretchy material
    only(
        &with("\"ease\": 2,", "\"ease\": 4,"),
        "clothing.layers[0]",
        "implausible",
    );
    complete(&with("\"ease\": 2,", "\"ease\": 3,"));
    only(
        &with("\"elasticity\": 0.25,", "\"elasticity\": 0.1,"),
        "clothing.layers[0]",
        "implausible",
    );
    complete(&with("\"elasticity\": 0.25,", "\"elasticity\": 0.2,"));
    // loose: at least 5 mm
    only(
        &with("\"ease\": 40,", "\"ease\": 4,"),
        "clothing.layers[2]",
        "implausible",
    );
    complete(&with("\"ease\": 40,", "\"ease\": 5,"));
}

#[test]
fn an_outer_layer_is_not_tighter_than_the_inner_one() {
    // the skirt from the shoulder overlaps the blouse; the blouse put outside it is tighter
    let dress = with(
        "\"from\": \"hips\", \"to\": \"knee\"",
        "\"from\": \"shoulder\", \"to\": \"knee\"",
    );
    complete(&dress);
    let outside = dress.replace(
        "\"name\": \"blouse\", \"order\": 1,",
        "\"name\": \"blouse\", \"order\": 4,",
    );
    only(&outside, "clothing.layers[0]", "implausible");
    // spans that only touch (the blouse ends at the hips, the skirt starts there) do not overlap
    complete(&with(
        "\"name\": \"blouse\", \"order\": 1,",
        "\"name\": \"blouse\", \"order\": 4,",
    ));
}

#[test]
fn accessories_sit_where_their_kind_is_worn() {
    let add = |item: &str| {
        with(
            "\"material\": \"satin\"}\n      ]",
            &format!("\"material\": \"satin\"}},\n        {item}\n      ]"),
        )
    };
    let fastener = "{\"name\": \"buttons\", \"kind\": \"fastener\", \"attach\": \"closure_line\", \"on_layer\": \"blouse\", \"size\": 10, \"material\": \"leather\"}";
    complete(&add(fastener));
    only(
        &add(&fastener.replace("closure_line", "hips")),
        "accessories.items[2]",
        "implausible",
    );
    only(
        &add(&fastener.replace("\"on_layer\": \"blouse\", ", "")),
        "accessories.items[2]",
        "implausible",
    );
    // the sleeves have no closure
    only(
        &add(&fastener.replace("blouse", "sleeves")),
        "accessories.items[2]",
        "implausible",
    );
    only(
        &with("\"attach\": \"sole\"", "\"attach\": \"ankle\""),
        "accessories.items[0]",
        "implausible",
    );
    let gloves = "{\"name\": \"gloves\", \"kind\": \"gloves\", \"attach\": \"wrist\", \"size\": 180, \"material\": \"leather\"}";
    complete(&add(gloves));
    only(
        &add(&gloves.replace("wrist", "elbow")),
        "accessories.items[2]",
        "implausible",
    );
    let hat = "{\"name\": \"hat\", \"kind\": \"hat\", \"attach\": \"head_top\", \"size\": 300, \"material\": \"wool\"}";
    complete(&add(hat));
    only(
        &add(&hat.replace("head_top", "chin")),
        "accessories.items[2]",
        "implausible",
    );
    complete(&with(
        "\"kind\": \"ribbon\", \"attach\": \"hair_tie\"",
        "\"kind\": \"hair_ornament\", \"attach\": \"hair_tie\"",
    ));
    complete(&with(
        "\"kind\": \"ribbon\", \"attach\": \"hair_tie\"",
        "\"kind\": \"hair_ornament\", \"attach\": \"head_top\"",
    ));
    only(
        &with(
            "\"kind\": \"ribbon\", \"attach\": \"hair_tie\"",
            "\"kind\": \"hair_ornament\", \"attach\": \"hips\"",
        ),
        "accessories.items[1]",
        "implausible",
    );
}

#[test]
fn the_transmission_fits_the_material() {
    only(
        &with(
            "{\"name\": \"voile\", \"class\": \"fabric\"",
            "{\"name\": \"voile\", \"class\": \"leather\"",
        ),
        "materials.items[2]",
        "implausible",
    );
    let clear = "\"transmission\": \"opaque\", \"pattern\": \"none\", \"used_by\": \"eyes\"";
    complete(&with(
        clear,
        "\"transmission\": \"clear\", \"ior\": 1.4, \"pattern\": \"none\", \"used_by\": \"eyes\"",
    ));
    let cotton = "\"transmission\": \"opaque\", \"pattern\": \"none\", \"used_by\": \"blouse\"";
    only(&with(cotton, "\"transmission\": \"clear\", \"ior\": 1.4, \"pattern\": \"none\", \"used_by\": \"blouse\""), "materials.items[1]", "implausible");
    complete(&with(cotton, "\"transmission\": \"diffuse\", \"transmittance\": 0.3, \"scattering\": 0.8, \"pattern\": \"none\", \"used_by\": \"blouse\""));
}

#[test]
fn warnings_are_listed_and_do_not_stop_the_drawing() {
    // a colour nobody uses
    warns(
        &with("{\"name\": \"amber\", \"srgb\": \"#c98a2e\"}", "{\"name\": \"amber\", \"srgb\": \"#c98a2e\"},\n        {\"name\": \"spare\", \"srgb\": \"#000000\"}"),
        "palette.colours[7].name",
        "not_used",
    );
    // a material nobody uses: the skirt made of satin leaves the wool unused
    warns(
        &with(
            "\"closure\": \"zip\", \"material\": \"wool\"",
            "\"closure\": \"zip\", \"material\": \"satin\"",
        ),
        "materials.items[3].name",
        "not_used",
    );
    // a sole and heel of 128 mm on a 1.52 m figure (the convention allows 121.6 mm)
    warns(
        &with("\"heel\": 20,", "\"heel\": 120,"),
        "accessories.items[0]",
        "implausible",
    );
    complete(&with("\"heel\": 20,", "\"heel\": 113,"));
    // see-through names on an opaque material, in a layer's name or a material's name
    warns(
        &with("{\"name\": \"skirt\",", "{\"name\": \"Lace skirt\","),
        "clothing.layers[2]",
        "implausible",
    );
    let tulle = example().replace("\"wool\"", "\"tulle wool\"");
    let c = law().check(&tulle).unwrap();
    assert!(c.complete(), "{}", c.report_japanese());
    assert_eq!(
        warnings(&c),
        [("materials.items[3]".to_owned(), "implausible")]
    );
    // the voile is see-through already: no warning
    complete(&with(
        "{\"name\": \"sleeves\",",
        "{\"name\": \"sheer sleeves\",",
    ));
}

#[test]
fn a_value_outside_the_range_is_noted_not_hidden() {
    let note = |n: &str| {
        with(
            "\"law\": \"avatar_sheet\",",
            &format!("\"law\": \"avatar_sheet\",\n  \"out_of_range_intent\": {n},"),
        )
    };
    let ok = note(
        "[{\"slot\": \"base_body.knee\", \"wants\": 0.27, \"why\": \"the knee is drawn low\"}]",
    );
    warns(&ok, "out_of_range_intent[0]", "design_wants_out_of_range");
    let c = law().check(&ok).unwrap();
    assert!(c.report_japanese().contains("base_body.knee = 0.27"));
    complete(&note("[]"));
    only(
        &note("[{\"slot\": \"base_body.knee\", \"wants\": 0.33, \"why\": \"x\"}]"),
        "out_of_range_intent[0]",
        "implausible",
    );
    only(
        &note("[{\"slot\": \"base_body.ankle_bone\", \"wants\": 0.1, \"why\": \"x\"}]"),
        "out_of_range_intent[0]",
        "no_such_name",
    );
    // a slot without a range
    only(
        &note("[{\"slot\": \"face.eye_colour\", \"wants\": 0.1, \"why\": \"x\"}]"),
        "out_of_range_intent[0]",
        "no_such_name",
    );
    only(
        &note("[{\"slot\": \"base_body.knee\", \"wants\": \"low\", \"why\": \"x\"}]"),
        "out_of_range_intent[0]",
        "wrong_type",
    );
    only(
        &note("[{\"slot\": \"base_body.knee\", \"wants\": 0.27}]"),
        "out_of_range_intent[0]",
        "wrong_type",
    );
    only(
        &note(&format!(
            "[{{\"slot\": \"base_body.knee\", \"wants\": 0.27, \"why\": \"{}\"}}]",
            "x".repeat(121)
        )),
        "out_of_range_intent[0]",
        "too_long",
    );
    warns(
        &note(&format!(
            "[{{\"slot\": \"base_body.knee\", \"wants\": 0.27, \"why\": \"{}\"}}]",
            "x".repeat(120)
        )),
        "out_of_range_intent[0]",
        "design_wants_out_of_range",
    );
    only(
        &note("[{\"slot\": \"base_body.knee\", \"wants\": 0.27, \"why\": \"x\", \"by\": 1}]"),
        "out_of_range_intent[0]",
        "unknown_key",
    );
    only(&note("{}"), "out_of_range_intent", "wrong_type");
}

fn outside(n: u32) -> SheetCheck {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("tests/data/sheet_fill_outside_answer_{n}.json")),
    )
    .unwrap();
    law().check(&text).unwrap()
}

fn paths(list: &[(String, &'static str)]) -> Vec<String> {
    list.iter().map(|(p, c)| format!("{p} {c}")).collect()
}

/// The places every answer written for the law before these slots misses
fn new_slots(materials: usize) -> Vec<String> {
    let mut v: Vec<String> = [
        "base_body.family",
        "base_body.skin_material",
        "face.iris_material",
        "hair.material",
    ]
    .iter()
    .map(|p| format!("{p} missing"))
    .collect();
    v.extend((0..materials).map(|i| format!("materials.items[{i}].elasticity missing")));
    v
}

#[test]
fn the_rules_name_what_the_outside_answers_got_wrong() {
    // answer 1: wide sleeves drawn as their own layer, neither part of the jacket nor apart
    let c = outside(1);
    let mut want = new_slots(12);
    want.push("clothing.layers[4] implausible".into());
    assert_eq!(paths(&findings(&c)), want, "{}", c.report_japanese());
    assert_eq!(
        paths(&warnings(&c)),
        [
            "palette.colours[8].name not_used",
            "palette.colours[9].name not_used",
            "materials.items[0].name not_used",
            "materials.items[1].name not_used",
            "materials.items[2].name not_used",
        ]
    );
    // answer 2: detached puff sleeves as a loose layer; a bodice that follows the skin with
    // 6 mm of ease; a corset outside a looser bodice; lace filled as opaque
    let c = outside(2);
    let mut want = new_slots(11);
    want.extend(
        [
            "clothing.layers[2] implausible",
            "clothing.layers[1] implausible",
            "clothing.layers[5] implausible",
        ]
        .map(String::from),
    );
    assert_eq!(paths(&findings(&c)), want, "{}", c.report_japanese());
    assert_eq!(
        paths(&warnings(&c)),
        [
            "clothing.layers[5] implausible",
            "materials.items[5] implausible",
            "palette.colours[7].name not_used",
            "materials.items[0].name not_used",
            "materials.items[1].name not_used",
            "materials.items[2].name not_used",
        ]
    );
}

#[test]
fn the_spec_tells_the_rules_and_the_note() {
    let spec = law().spec();
    for (r, text) in law().item_rules.iter().zip(&law().item_rule_texts) {
        let _ = r;
        assert!(
            spec.contains(&format!("`{text}`")),
            "{text} not in the spec"
        );
    }
    assert_eq!(law().item_rules.len(), law().item_rule_texts.len());
    assert!(spec.contains("- `body`: head_top → chin → shoulder"));
    assert!(spec.contains("\"out_of_range_intent\""));
    assert!(spec.contains("the middle of a range is not a target"));
    assert!(spec.contains("skin or what is behind shows through"));
}

/// The value of `"name": <number>` in a fill (each proportion name occurs once)
fn number_in(fill: &str, name: &str) -> f64 {
    let key = format!("\"{name}\": ");
    assert_eq!(fill.matches(&key).count(), 1, "{name}");
    let rest = &fill[fill.find(&key).unwrap() + key.len()..];
    rest[..rest.find([',', '\n']).unwrap()].parse().unwrap()
}

#[test]
fn the_spread_examples_use_the_ranges_not_the_defaults() {
    let fills: Vec<String> = ["tall_adult", "small_young", "stylised"]
        .iter()
        .map(|n| {
            std::fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("tests/data/sheet_fill_spread_{n}.json")),
            )
            .unwrap()
        })
        .collect();
    for f in &fills {
        complete(f);
    }
    let mut compared = 0;
    // the two reference-family examples: off the defaults, and on both sides of each
    for file in ["body.law", "face.law"] {
        let law = proportion(file);
        for p in law.params() {
            let v: Vec<f64> = fills[..2].iter().map(|f| number_in(f, &p.name)).collect();
            assert!(
                v.iter().all(|x| *x != p.default),
                "{}: {v:?} at the default {}",
                p.name,
                p.default
            );
            assert!(
                v.iter().any(|x| *x < p.default) && v.iter().any(|x| *x > p.default),
                "{}: {v:?} on one side of {}",
                p.name,
                p.default
            );
            compared += 1;
        }
    }
    // the stylised example: off every default of its own family
    for file in ["body_stylised.law", "face_stylised.law"] {
        for p in proportion(file).params() {
            assert_ne!(number_in(&fills[2], &p.name), p.default, "{}", p.name);
        }
    }
    assert!(fills[2].contains("\"family\": \"stylised\""));
    assert_eq!(compared, 18, "every body and face proportion compared");
}
