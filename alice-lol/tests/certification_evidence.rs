//! `certifications.yaml`: `certified_by` が `UserFieldTest` / `Both` の項目は、LOL の出力を印刷した記録
//! (`field_test.date` = YYYY-MM-DD と `field_test.evidence` = {path, sha256}、path は repo 内の file で
//! sha256 が一致) を持つ registry の `certified_by` と yaml の値は一致する
//!
//! 2026-11-15 に日付と証跡の無い 10 項目のラベルを外したので、今の yaml には field test の項目が無い
//! 検査の歯は下の fixture (日付 TBD / 解決しない証跡 / 正しい項目) で見る

use alice_lol::stdlib::pattern::registry;
use sha2::{Digest, Sha256};
use std::path::Path;

struct Entry {
    name: String,
    certified_by: String,
    field_test: String,
}

fn parse(yaml: &str) -> Vec<Entry> {
    let mut v: Vec<Entry> = Vec::new();
    for line in yaml.lines() {
        let t = line.trim();
        if let Some(n) = t.strip_prefix("- pattern_name:") {
            v.push(Entry {
                name: n.trim().to_string(),
                certified_by: String::new(),
                field_test: String::new(),
            });
        } else if let Some(e) = v.last_mut() {
            if let Some(c) = t.strip_prefix("certified_by:") {
                e.certified_by = c.split('#').next().unwrap_or("").trim().to_string();
            } else if let Some(f) = t.strip_prefix("field_test:") {
                e.field_test = f.trim().to_string();
            }
        }
    }
    v
}

/// `key: value` を `{...}` の中から取る (値は `,` か `}` まで)
fn field<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let i = s.find(&format!("{key}:"))? + key.len() + 1;
    let rest = &s[i..];
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    Some(rest[..end].trim().trim_matches('"'))
}

fn is_date(d: &str) -> bool {
    let b = d.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// 検査した項目数、または違反の一覧
fn check(yaml: &str, root: &Path) -> Result<usize, Vec<String>> {
    let entries = parse(yaml);
    let mut bad = Vec::new();
    for e in &entries {
        if !(e.certified_by == "UserFieldTest" || e.certified_by == "Both") {
            continue;
        }
        let ft = &e.field_test;
        match field(ft, "date") {
            Some(d) if is_date(d) => {}
            other => bad.push(format!("{}: field test without a date ({other:?})", e.name)),
        }
        let (path, sha) = (field(ft, "path"), field(ft, "sha256"));
        match (path, sha) {
            (Some(p), Some(h)) => match std::fs::read(root.join(p)) {
                Ok(bytes) => {
                    let got = Sha256::digest(&bytes)
                        .iter()
                        .fold(String::new(), |mut s, b| {
                            use std::fmt::Write as _;
                            let _ = write!(s, "{b:02x}");
                            s
                        });
                    if got != h {
                        bad.push(format!("{}: evidence {p} sha256 {got} != {h}", e.name));
                    }
                }
                Err(_) => bad.push(format!("{}: evidence {p} does not resolve", e.name)),
            },
            _ => bad.push(format!(
                "{}: field test without evidence {{path, sha256}}",
                e.name
            )),
        }
    }
    if bad.is_empty() {
        Ok(entries.len())
    } else {
        Err(bad)
    }
}

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

#[test]
fn every_field_test_label_in_the_yaml_has_a_date_and_resolving_evidence() {
    let yaml = std::fs::read_to_string(repo_root().join("certifications.yaml")).unwrap();
    let n = check(&yaml, repo_root()).unwrap_or_else(|b| panic!("{b:#?}"));
    assert!(n > 0, "inspected 0 entries");
    assert_eq!(n, registry::ALL.len(), "yaml entries vs registry");
}

#[test]
fn registry_and_yaml_agree_on_certified_by() {
    let yaml = std::fs::read_to_string(repo_root().join("certifications.yaml")).unwrap();
    let entries = parse(&yaml);
    let mut compared = 0;
    for p in registry::ALL {
        let e = entries
            .iter()
            .find(|e| e.name == p.name)
            .unwrap_or_else(|| panic!("{} not in yaml", p.name));
        assert_eq!(
            format!("{:?}", p.certified_by),
            e.certified_by,
            "{}",
            p.name
        );
        compared += 1;
    }
    assert!(compared > 0);
}

const FIX_HASH: &str = "5797f42746544674483d404433a99b3e9aca67688ccf09487ebe652f17f5e70d";

#[test]
fn fixtures_give_the_check_its_teeth() {
    let root = repo_root();
    let tbd = "- pattern_name: a\n  field_test: {date: TBD, material: PLA, evidence: {path: alice-lol/tests/data/cert_evidence_fixture.txt, sha256: HASH}}\n  certified_by: UserFieldTest\n".replace("HASH", FIX_HASH);
    assert!(check(&tbd, root).is_err(), "date TBD passed");
    let missing = "- pattern_name: b\n  field_test: {date: 2026-11-15, evidence: {path: alice-lol/tests/data/no_such_file.txt, sha256: HASH}}\n  certified_by: Both\n".replace("HASH", FIX_HASH);
    assert!(check(&missing, root).is_err(), "unresolved evidence passed");
    let wrong_hash = "- pattern_name: c\n  field_test: {date: 2026-11-15, evidence: {path: alice-lol/tests/data/cert_evidence_fixture.txt, sha256: 00}}\n  certified_by: Both\n";
    assert!(check(wrong_hash, root).is_err(), "wrong hash passed");
    let no_evidence = "- pattern_name: d\n  field_test: {date: 2026-11-15, material: PLA}\n  certified_by: UserFieldTest\n";
    assert!(check(no_evidence, root).is_err(), "no evidence passed");
    let good = "- pattern_name: e\n  field_test: {date: 2026-11-15, material: PLA, evidence: {path: alice-lol/tests/data/cert_evidence_fixture.txt, sha256: HASH}}\n  certified_by: UserFieldTest\n".replace("HASH", FIX_HASH);
    assert_eq!(check(&good, root), Ok(1));
    // field test でない項目は日付も証跡も要らない
    let sim = "- pattern_name: f\n  field_test: null\n  certified_by: SimulationOnly\n";
    assert_eq!(check(sim, root), Ok(1));
}
