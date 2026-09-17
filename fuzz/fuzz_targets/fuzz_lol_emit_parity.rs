//! Fuzz target: parse → emit → parse の eval parity (LOL 側の 2 評価 path =
//! parser と emitter の往復が場を変えないこと)
//!
//! strict-eval 8b (評価 path 複数 × parity fuzz) 対応 2026-09-17 grammar corpus
//! の既定引数 (tests/emit_roundtrip.rs) では出ない引数の組合せ (負値 / 0 /
//! 巨大値 / 深い nest) を libFuzzer に探させる

#![no_main]

use alice_lol::emit::to_lol;
use alice_lol::runtime_parser::parse_lol;
use alice_lol::{eval, Vec3};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    if input.len() > 4096 {
        return; // 深い nest は stacker で伸びるが fuzz の時間予算に収める
    }
    let Ok(n1) = parse_lol(input) else {
        return;
    };
    // Unsupported variant (emit が Err) は parity 対象外、panic のみ NG
    let Ok(text) = to_lol(&n1) else {
        return;
    };
    let n2 = parse_lol(&text)
        .unwrap_or_else(|e| panic!("emitted text does not re-parse: {e}\n{text}"));
    let text2 = to_lol(&n2).expect("re-emit");
    assert_eq!(text, text2, "emit is not idempotent");
    let mut seed: u32 = 0x9E37_79B9;
    for i in 0..16 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let r = |s: u32| (s as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let k = if i % 4 == 0 { 8.0 } else { 2.5 };
        let p = Vec3::new(r(seed) * k, r(seed.rotate_left(11)) * k, r(seed.rotate_left(22)) * k);
        let (a, b) = (eval(&n1, p), eval(&n2, p));
        assert!(
            (a.is_nan() && b.is_nan()) || (a - b).abs() <= 1e-4 * a.abs().max(1.0),
            "eval parity broken at {p:?}: {a} vs {b}\n{input}\n{text}"
        );
    }
});
