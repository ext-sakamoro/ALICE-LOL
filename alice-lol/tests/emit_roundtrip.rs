//! `emit::to_lol` round-trip (Track C0)
//!
//! `lol.gbnf` の name bucket を走査して **全 construct** に既定引数を与えた
//! snippet を生成し、`parse → emit → parse` の 2 tree が eval parity を持つ
//! ことを検証する grammar に construct が増えれば自動的に対象になる
//! (`grammar_covers_every_runtime_parser_construct` と合わせて
//! parser ⊆ grammar ⊆ emit の 3 者を CI で固定)
//!
//! 判定が eval parity なのは `Rotate` の Quat ↔ Euler 往復等で bit 一致が
//! 期待できないため 2 回目の emit は 1 回目と文字列一致する (正規形の冪等性)

#![allow(clippy::cast_precision_loss)] // sample 点生成

use alice_lol::emit::to_lol;
use alice_lol::runtime_parser::parse_lol;
use alice_lol::{eval, SdfNode, Vec3};

mod common;
use common::corpus::{fixtures, grammar_corpus};

/// 64 sample 点 (原点近傍 + 少し外) で eval parity
fn assert_parity(a: &SdfNode, b: &SdfNode, label: &str) {
    let mut seed: u32 = 0x9E37_79B9;
    for i in 0..64 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let r = |s: u32| (s as f32 / u32::MAX as f32).mul_add(2.0, -1.0);
        let scale = if i % 4 == 0 { 8.0 } else { 2.5 };
        let p = Vec3::new(
            r(seed) * scale,
            r(seed.rotate_left(11)) * scale,
            r(seed.rotate_left(22)) * scale,
        );
        let da = eval(a, p);
        let db = eval(b, p);
        assert!(
            (da.is_nan() && db.is_nan()) || (da - db).abs() <= 1e-4 * da.abs().max(1.0),
            "{label}: eval mismatch at {p:?}: {da} vs {db}"
        );
    }
}

#[test]
fn every_grammar_construct_round_trips() {
    let corpus = grammar_corpus();
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for (name, src) in &corpus {
        let n1 = match parse_lol(src) {
            Ok(n) => n,
            Err(e) => {
                failures.push(format!(
                    "{name}: parse of default snippet failed: {e} ({src})"
                ));
                continue;
            }
        };
        let text = match to_lol(&n1) {
            Ok(t) => t,
            Err(e) => {
                failures.push(format!("{name}: emit failed: {e}"));
                continue;
            }
        };
        let n2 = match parse_lol(&text) {
            Ok(n) => n,
            Err(e) => {
                failures.push(format!("{name}: re-parse failed: {e} (emitted `{text}`)"));
                continue;
            }
        };
        assert_parity(&n1, &n2, name);
        // 正規形の冪等性
        let text2 = to_lol(&n2).unwrap();
        assert_eq!(text, text2, "{name}: emit is not idempotent");
        checked += 1;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // 257 parser construct − Intent verb 18 − program/entities 2 = 237 (2026-09-14)
    assert!(checked >= 230, "only {checked} constructs round-tripped");
}

#[test]
fn fixtures_round_trip() {
    for (_, src) in fixtures() {
        let n1 = parse_lol(src).unwrap_or_else(|e| panic!("{e}: {src}"));
        let text = to_lol(&n1).unwrap_or_else(|e| panic!("{e}: {src}"));
        let n2 = parse_lol(&text).unwrap_or_else(|e| panic!("{e}: emitted `{text}`"));
        assert_parity(&n1, &n2, src);
        assert_eq!(text, to_lol(&n2).unwrap());
    }
}

#[test]
fn emitted_text_is_accepted_by_llm_grammar_rules() {
    // emit は comment を書かず、token 間 whitespace は `", "` の 1 個だけ
    let n = parse_lol("union(pen_cup(50,100), translate(33,0,50, rotate(0,90,0, torus(15,5))))")
        .unwrap();
    let text = to_lol(&n).unwrap();
    assert!(!text.contains("//"));
    assert!(!text.contains("  "));
    assert!(!text.contains('\n'));
}
