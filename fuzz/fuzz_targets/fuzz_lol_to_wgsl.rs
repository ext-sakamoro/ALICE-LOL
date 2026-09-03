//! Fuzz target: LOL DSL → WGSL transpile pipeline で panic しないことを検証
//!
//! parse_lol → to_wgsl の 2-step で SdfNode 中間表現を経て shader source を生成
//! 中間 SdfNode が想定外の topology / 深度で構築されても panic ないことを保証

#![no_main]

use alice_lol::{runtime_parser::parse_lol, to_wgsl};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };
    // parse 段階で fail する input が大半、パスした場合のみ transpile 実行
    if let Ok(node) = parse_lol(input) {
        let _ = to_wgsl(&node);
    }
});
