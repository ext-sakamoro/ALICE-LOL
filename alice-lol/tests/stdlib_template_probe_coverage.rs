//! `LOL_GEOMETRY_SEMANTICS_ID` は `stdlib` の template が作る形を
//! `geometry_id::STDLIB_TEMPLATE_PROBE_LOL` の文で畳む ここでは parser が `stdlib` に渡す呼び出しの
//! 分岐 (`runtime_parser.rs` の `"名前" => { … crate::stdlib:: … }`) を数え、各分岐の名前にちょうど
//! 1 つの probe があり、probe の名前がどれも分岐にあることを確かめる
//!
//! probe の無い分岐を足すと、その template の形が変わっても識別子が動かない 分岐の無い probe は
//! 古い名前が残っている形 どちらも失敗にし、分岐を 1 つも数えられなかった時も失敗にする

use std::collections::BTreeSet;

use alice_lol::geometry_id::STDLIB_TEMPLATE_PROBE_LOL;

const PARSER: &str = include_str!("../src/runtime_parser.rs");

/// 行頭の空白の後が `"a"` か `"a" | "b"` の並びで `=>` が続く行を分岐の頭とし、次の頭までを本体とする
fn arm_names(line: &str) -> Option<Vec<String>> {
    let rest = line.trim_start();
    let (head, _) = rest.split_once("=>")?;
    let mut names = Vec::new();
    for part in head.split('|') {
        let p = part.trim();
        let inner = p.strip_prefix('"')?.strip_suffix('"')?;
        if inner.is_empty()
            || !inner
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return None;
        }
        names.push(inner.to_owned());
    }
    Some(names)
}

fn stdlib_arms(src: &str) -> BTreeSet<String> {
    let lines: Vec<&str> = src.lines().collect();
    let heads: Vec<(usize, Vec<String>)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| arm_names(l).map(|n| (i, n)))
        .collect();
    let mut out = BTreeSet::new();
    for (k, (i, names)) in heads.iter().enumerate() {
        let end = heads.get(k + 1).map_or(lines.len(), |(j, _)| *j);
        if lines[*i..end].iter().any(|l| l.contains("stdlib::")) {
            out.extend(names.iter().cloned());
        }
    }
    out
}

fn probe_name(text: &str) -> &str {
    text.split_once('(').map_or(text, |(n, _)| n).trim()
}

#[test]
fn every_stdlib_arm_has_exactly_one_template_probe() {
    let arms = stdlib_arms(PARSER);
    assert!(
        arms.len() >= 100,
        "only {} stdlib arms were found in runtime_parser.rs (the scan no longer reads the dispatch)",
        arms.len()
    );
    let mut probes = BTreeSet::new();
    for text in STDLIB_TEMPLATE_PROBE_LOL {
        assert!(
            probes.insert(probe_name(text).to_owned()),
            "two probes call {}",
            probe_name(text)
        );
    }
    let unprobed: Vec<_> = arms.difference(&probes).collect();
    let orphan: Vec<_> = probes.difference(&arms).collect();
    assert!(
        unprobed.is_empty(),
        "stdlib arms without a template probe (add one to STDLIB_TEMPLATE_PROBE_LOL): {unprobed:?}"
    );
    assert!(
        orphan.is_empty(),
        "template probes whose name is not a stdlib arm of the parser: {orphan:?}"
    );
    assert_eq!(arms.len(), STDLIB_TEMPLATE_PROBE_LOL.len());
}

#[test]
fn the_scan_finds_an_added_arm_and_a_missing_probe() {
    let added = PARSER.replacen(
        "            \"shelf_divider\" => {",
        "            \"zz_unprobed_template\" => {\n                Ok(crate::stdlib::x())\n            }\n            \"shelf_divider\" => {",
        1,
    );
    assert_ne!(added, PARSER, "the anchor of the added arm was not found");
    let arms = stdlib_arms(&added);
    assert!(arms.contains("zz_unprobed_template"));
    assert!(arms.contains("shelf_divider"));
    let probes: BTreeSet<String> = STDLIB_TEMPLATE_PROBE_LOL
        .iter()
        .map(|t| probe_name(t).to_owned())
        .collect();
    assert_eq!(
        arms.difference(&probes).cloned().collect::<Vec<_>>(),
        vec!["zz_unprobed_template".to_owned()]
    );
}

/// `41c29e6c…` (この行を足す前の識別子) は以前の識別子にしない: その識別子の下で
/// `shelf_divider()` は別の木を作っていた (alice-lol main 77dc88c で記録した正規形の文の SHA-256 が
/// `b2bda93d…`、今の木とは違う) ので bit が再現しない
#[test]
fn the_previous_geometry_id_does_not_reproduce_the_shelf_divider() {
    use alice_lol::geometry_id::{
        check_lol_geometry_semantics, LOL_GEOMETRY_SEMANTICS_ID,
        PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS,
    };
    use sha2::{Digest, Sha256};

    let hex32 = |s: &str| -> [u8; 32] {
        let mut b = [0u8; 32];
        for (i, out) in b.iter_mut().enumerate() {
            *out = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap();
        }
        b
    };
    let old = hex32("41c29e6c73afd270a653b779a416b97fff8139eddb1a1770dd43cf1dc8c70791");
    assert_ne!(old, LOL_GEOMETRY_SEMANTICS_ID);
    assert!(!PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS.contains(&old));
    assert_eq!(
        check_lol_geometry_semantics(&old),
        alice_det_math::SemanticsCheck::Mismatch
    );

    let node = alice_lol::runtime_parser::parse_lol("shelf_divider()").unwrap();
    let text = alice_lol::emit::to_lol_unstamped(&node).unwrap();
    let now: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    assert_ne!(
        now,
        hex32("b2bda93dab554c3733fd44f7203a39f6ab12851df57a3c56cd7d733a8dd4604e"),
        "shelf_divider() builds the tree it built under 41c29e6c; the old id could be a previous one"
    );
}
