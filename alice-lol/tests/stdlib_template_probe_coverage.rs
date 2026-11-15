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

/// 行頭の空白の後が `"a"` か `"a" | "b"` の並びで `=>` が続く行を分岐の頭とする (本体は頭の `{` と対の `}` まで)
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

/// `src` から comment と文字列・文字の literal を空白に置き換えた写し (byte の位置は同じ)
fn code_only(src: &str) -> Vec<u8> {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'*') => {
                while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                    if b[i] != b'\n' {
                        out[i] = b' ';
                    }
                    i += 1;
                }
                if i < b.len() {
                    out[i..i + 2].fill(b' ');
                    i += 2;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
            }
            b'"' => {
                out[i] = b' ';
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        out[i] = b' ';
                        i += 1;
                    }
                    if i < b.len() && b[i] != b'\n' {
                        out[i] = b' ';
                    }
                    i += 1;
                }
                if i < b.len() {
                    out[i] = b' ';
                    i += 1;
                }
            }
            b'\'' if b.get(i + 2) == Some(&b'\'') => {
                out[i..i + 3].fill(b' ');
                i += 3;
            }
            _ => i += 1,
        }
    }
    out
}

/// 分岐の頭 (`"名前" => {` の 1 行) ごとに、`{` と対になる `}` までの byte 範囲を返す
fn arm_bodies(src: &str) -> Vec<(Vec<String>, std::ops::Range<usize>)> {
    let code = code_only(src);
    let mut out = Vec::new();
    let mut start = 0;
    for line in src.split_inclusive('\n') {
        if let Some(names) = arm_names(line) {
            let arrow = start + line.find("=>").expect("a head has =>");
            if let Some(open) = (arrow..start + line.len()).find(|&k| code[k] == b'{') {
                let mut depth = 0i32;
                let mut k = open;
                while k < code.len() {
                    match code[k] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    k += 1;
                }
                out.push((names, open..k + 1));
            }
        }
        start += line.len();
    }
    out
}

/// 分岐の外で `stdlib::` を使ってよい関数 (形を作らないもの) と理由 形を作る呼び出しは分岐の中に置く
const HELPERS_OUTSIDE_ARMS: &[(&str, &str)] = &[(
    "metric_size",
    "maps a parsed number to the fastener size enum; the arms that build the screw holes call stdlib themselves and are probed",
)];

/// 位置 `k` を含む関数の名前 (`fn 名前` を後ろ向きに探す)
fn enclosing_fn(code: &[u8], k: usize) -> Option<String> {
    let before = std::str::from_utf8(&code[..k]).ok()?;
    let at = before.rfind("fn ")?;
    let name: String = before[at + 3..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// `stdlib::` を本体に持つ分岐の名前と、どの分岐の本体にも入らない `stdlib::` の位置 (行番号)
fn stdlib_arms_and_strays(src: &str) -> (BTreeSet<String>, Vec<usize>) {
    let code = code_only(src);
    let bodies = arm_bodies(src);
    // `stdlib::` の呼び出しと、別名で持ち込む `stdlib as` / `stdlib;` / `stdlib}` (`use crate::stdlib as s;`)
    let hits: Vec<usize> = (0..code.len())
        .filter(|&k| code[k..].starts_with(b"stdlib"))
        .filter(|&k| k == 0 || !(code[k - 1].is_ascii_alphanumeric() || code[k - 1] == b'_'))
        .filter(|&k| {
            let rest = &code[k + b"stdlib".len()..];
            let rest = rest.trim_ascii_start();
            rest.starts_with(b"::")
                || rest.starts_with(b"as ")
                || rest.starts_with(b";")
                || rest.starts_with(b"}")
        })
        .collect();
    let mut arms = BTreeSet::new();
    let mut strays = Vec::new();
    for k in hits {
        if let Some((names, _)) = bodies.iter().find(|(_, r)| r.contains(&k)) {
            arms.extend(names.iter().cloned());
        } else {
            let allowed = enclosing_fn(&code, k)
                .is_some_and(|f| HELPERS_OUTSIDE_ARMS.iter().any(|(n, _)| *n == f));
            if !allowed {
                strays.push(src[..k].matches('\n').count() + 1);
            }
        }
    }
    (arms, strays)
}

fn stdlib_arms(src: &str) -> BTreeSet<String> {
    stdlib_arms_and_strays(src).0
}

fn probe_name(text: &str) -> &str {
    text.split_once('(').map_or(text, |(n, _)| n).trim()
}

/// 分岐の外 (複数行の pattern・guard 付きの頭・stdlib を呼ぶ helper・`use` ・macro) から stdlib を呼ぶと、
/// 上の走査が名前を拾えずその template の形が識別子に入らない その形を書いたら失敗にして、分岐の頭の
/// 形に直すか probe を数える方法を足すまで通さない parser の試験 module (`#[cfg(test)]` 以降) は数えない
#[test]
fn every_stdlib_call_in_the_parser_is_inside_a_parsed_arm() {
    let dispatch = PARSER
        .split_once("#[cfg(test)]\nmod tests")
        .map_or(PARSER, |(head, _)| head);
    let (arms, strays) = stdlib_arms_and_strays(dispatch);
    assert!(arms.len() >= 100, "only {} arms", arms.len());
    assert!(
        strays.is_empty(),
        "runtime_parser.rs calls stdlib outside a single-line `\"name\" => {{` arm at lines {strays:?}"
    );
}

/// 引数の無い template は probe 1 つ、引数のある template は 2 つ (違う引数の組) 同じ文は 2 度置かない
#[test]
fn every_stdlib_arm_has_its_template_probes() {
    let arms = stdlib_arms(PARSER);
    assert!(
        arms.len() >= 100,
        "only {} stdlib arms were found in runtime_parser.rs (the scan no longer reads the dispatch)",
        arms.len()
    );
    let mut texts = BTreeSet::new();
    let mut per_name: std::collections::BTreeMap<String, Vec<&str>> =
        std::collections::BTreeMap::new();
    for text in STDLIB_TEMPLATE_PROBE_LOL {
        assert!(texts.insert(*text), "the probe {text} is listed twice");
        per_name
            .entry(probe_name(text).to_owned())
            .or_default()
            .push(text);
    }
    let probes: BTreeSet<String> = per_name.keys().cloned().collect();
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
    for (name, list) in &per_name {
        let takes_arguments = list.iter().any(|t| !t.ends_with("()"));
        let want = if takes_arguments { 2 } else { 1 };
        assert_eq!(list.len(), want, "{name}: probes {list:?}");
    }
    assert_eq!(arms.len(), per_name.len());
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

/// 走査の歯: 分岐の外の stdlib の呼び方を差し込んだ text では、分岐の外の位置が報告される
#[test]
fn the_stray_check_reports_each_way_of_calling_stdlib_outside_an_arm() {
    let dispatch = PARSER
        .split_once("#[cfg(test)]\nmod tests")
        .map_or(PARSER, |(head, _)| head);
    let (_, base) = stdlib_arms_and_strays(dispatch);
    assert!(base.is_empty(), "the parser itself has strays {base:?}");
    let anchor = "            \"shelf_divider\" => {";
    assert!(dispatch.contains(anchor), "the anchor arm was not found");
    let cases: &[(&str, &str)] = &[
        (
            "guard arm",
            "            \"zz_guarded\" if true => {\n                crate::stdlib::x()\n            }\n",
        ),
        (
            "multi-line pattern",
            "            \"zz_a\"\n            | \"zz_b\" => {\n                crate::stdlib::x()\n            }\n",
        ),
        (
            "single-expression arm",
            "            \"zz_expr\" => crate::stdlib::x(),\n",
        ),
    ];
    for (what, snippet) in cases {
        let fed = dispatch.replacen(anchor, &format!("{snippet}{anchor}"), 1);
        let (_, strays) = stdlib_arms_and_strays(&fed);
        assert!(!strays.is_empty(), "{what}: not reported");
    }
    for (what, item) in [
        (
            "helper fn",
            "fn zz_helper() -> SdfNode {\n    crate::stdlib::x()\n}\n",
        ),
        ("alias use", "use crate::stdlib as zz;\n"),
        ("glob use", "use crate::stdlib::*;\n"),
    ] {
        let fed = format!("{item}{dispatch}");
        let (_, strays) = stdlib_arms_and_strays(&fed);
        assert!(!strays.is_empty(), "{what}: not reported");
    }
    let commented = format!("/* crate::stdlib::x() */\n{dispatch}");
    assert_eq!(
        stdlib_arms_and_strays(&commented).1,
        Vec::<usize>::new(),
        "a block comment is not a call"
    );
}

/// 2 組目の probe は 1 組目と違う正規形の木を作る (同じ木なら 2 組目は何も足していない)
#[test]
fn the_second_probe_of_each_template_builds_a_different_tree() {
    use alice_lol::emit::to_lol_unstamped;
    use alice_lol::runtime_parser::parse_lol;
    let mut per_name: std::collections::BTreeMap<&str, Vec<String>> =
        std::collections::BTreeMap::new();
    for text in STDLIB_TEMPLATE_PROBE_LOL {
        let tree = to_lol_unstamped(&parse_lol(text).unwrap()).unwrap();
        per_name.entry(probe_name(text)).or_default().push(tree);
    }
    let mut pairs = 0;
    for (name, trees) in &per_name {
        if trees.len() == 2 {
            assert_ne!(
                trees[0], trees[1],
                "{name}: both probes build the same tree"
            );
            pairs += 1;
        }
    }
    assert!(pairs >= 100, "only {pairs} templates have two probes");
}
