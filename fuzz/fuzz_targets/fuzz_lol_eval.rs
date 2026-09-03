//! Fuzz target: LOL DSL 経由 SdfNode の eval が任意 point で panic しないことを検証
//!
//! parse_lol → eval の 2-step で構築した SdfNode を任意 3D point で評価
//! NaN / Inf は許容、panic のみ NG

#![no_main]

use alice_lol::{eval, runtime_parser::parse_lol};
use arbitrary::Arbitrary;
use glam::Vec3;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct FuzzInput<'a> {
    lol_source: &'a str,
    px: f32,
    py: f32,
    pz: f32,
}

fuzz_target!(|input: FuzzInput| {
    if let Ok(node) = parse_lol(input.lol_source) {
        let point = Vec3::new(input.px, input.py, input.pz);
        // eval は panic しないことのみ検証、結果 NaN/Inf は許容
        let _ = eval(&node, point);
    }
});
