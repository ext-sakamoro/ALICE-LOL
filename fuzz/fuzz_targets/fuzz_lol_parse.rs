//! Fuzz target: LOL DSL parser (`parse_lol`) が任意 input で panic しないことを検証
//!
//! canonical CI template [[reference_alice_ci_canonical_template]] 準拠
//! DSL parser は user-facing 面が広い = 攻撃対象になり得る (LLM 出力の grammar-constrained
//! decoding 経由でも malformed DSL が入ってくる可能性あり)

#![no_main]

use alice_lol::runtime_parser::parse_lol;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // 非 UTF-8 は前処理で弾かれるはずだが、fuzz が UTF-8 boundary 攻撃を作らないよう
    // lossy 変換で境界問題を試験可能に
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    // parse_lol は Result 返却なので Ok / Err どちらでも許容、panic のみ NG
    let _ = parse_lol(input);
});
