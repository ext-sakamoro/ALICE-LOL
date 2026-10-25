//! A rule set with editions: `laws/demo-tax` (a first edition and two editions written as deltas),
//! answered by the edition in force at a date and by the edition an identifier names
//!
//! cargo run -p alice-lol --example rule_set

use alice_lol::law_input::parse_json;
use alice_lol::rule_law::{edition_text, resolve_delta, rule_law_from_file, RuleSet};

fn hex(b: &[u8; 32]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../laws/demo-tax");
    let read = |p: &str| std::fs::read_to_string(dir.join(p)).expect("a file of laws/demo-tax");
    let (y2025, y2026, y2027) = (
        read("y2025/rate.law"),
        read("y2026/rate.delta"),
        read("y2027/rate.delta"),
    );

    // a delta is resolved over the edition it supersedes into a full rule law text
    let first = edition_text(&y2025).expect("the first edition resolves");
    let (second, changes) = resolve_delta(&first, &y2026).expect("the 2026 delta resolves");
    let law = rule_law_from_file(&second).expect("the resolved text reads");
    let ed = law.edition().expect("an edition");
    println!(
        "{} {}: in force from {}{}, supersedes {}, changes {} examples",
        ed.domain(),
        ed.name(),
        ed.from(),
        ed.until().map_or(String::new(), |u| format!(" until {u}")),
        ed.supersedes().map_or_else(|| "nothing".to_owned(), hex),
        changes.len()
    );
    println!("semantics {}", hex(ed.semantics()));

    let set = RuleSet::from_files(&[
        ("rate.law", y2025.as_str()),
        ("rate.delta", y2026.as_str()),
        ("rate.delta", y2027.as_str()),
    ])
    .expect("the rule set loads");
    let request = parse_json(r#"{"amount": 110000}"#).expect("JSON");
    for at in [1_735_689_600, 1_767_225_600, 1_798_761_600] {
        let (answer, id) = set
            .evaluate_as_of(&request, at)
            .expect("an edition is in force");
        println!("at {at}: {answer:?} (edition {})", &hex(&id)[..12]);
    }
    // an old edition answers as it did, whatever the date
    let first_id = set.law_ids()[0];
    let (answer, _) = set
        .evaluate_edition(&first_id, &request)
        .expect("the first edition is kept");
    println!("the first edition, asked today: {answer:?}");
}
