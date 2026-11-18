//! Wide and deep LOL text: an n-ary call is read folded to the left (a smooth union is not
//! associative), so its depth is the number of its operands; nesting deeper than
//! `MAX_PARSE_DEPTH` is refused when read
#![cfg(feature = "wgsl")]

use alice_lol::runtime_parser::{parse_lol, MAX_PARSE_DEPTH};
use alice_lol::to_wgsl;

/// The input a fuzzer found that overflowed the stack in the WGSL transpiler: a smooth union
/// with k 0 of a sphere and 127 boxes
#[test]
fn the_wide_smooth_union_the_fuzzer_found_transpiles() {
    let text = include_str!("fixtures/wide_smooth_union.lol");
    let node = parse_lol(text).expect("it reads");
    let wgsl = to_wgsl(&node);
    assert!(wgsl.contains("fn sdf_eval"), "{} bytes", wgsl.len());
}

/// A union of 10,000 shapes reads and transpiles on a test thread's stack
#[test]
fn a_union_of_ten_thousand_shapes_transpiles() {
    let shapes: Vec<String> = (0..10_000)
        .map(|i| format!("translate({}, 0, 0, sphere(0.5))", i % 97))
        .collect();
    let text = format!("union({})", shapes.join(", "));
    let node = parse_lol(&text).expect("it reads");
    let wgsl = to_wgsl(&node);
    assert!(wgsl.len() > 10_000 * 20, "{} bytes", wgsl.len());
}

fn nested(depth: usize) -> String {
    let mut text = "sphere(1)".to_owned();
    for _ in 0..depth {
        text = format!("translate(0, 0, 1, {text})");
    }
    text
}

/// Calls nested `MAX_PARSE_DEPTH` deep read; one more is refused with an error, not a crash
#[test]
fn nesting_past_the_limit_is_refused() {
    // the leaf is one call, each translate another
    assert!(parse_lol(&nested(MAX_PARSE_DEPTH - 1)).is_ok());
    let err = parse_lol(&nested(MAX_PARSE_DEPTH)).expect_err("past the limit");
    assert!(
        err.message.contains(&MAX_PARSE_DEPTH.to_string()),
        "{}",
        err.message
    );
}
