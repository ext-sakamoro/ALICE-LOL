//! Example: BIZ UDPGothic 3344 CJK outline data 経由の CJK / mixed text rendering
//!
//! Run: `cargo run -p alice-lol-font --example foundry_outline --features foundry`
//!
//! Phase 1-8 実装 (`#[cfg(feature = "foundry")]`、AGPL cascade opt-in)
//!
//! 1. `FoundryWeight::Regular` / `Bold` の char count 確認
//! 2. 単一 CJK char lookup (`'森'`) の 2D / 3D SDF verify
//! 3. 混在 text (`"HELLO 森林"`) の layout + eval sanity check

use alice_lol_font::foundry::{
    char_count, cjk_glyph, cjk_string, cjk_string_3d, supports, FoundryWeight,
};

fn main() {
    println!("=== alice-lol-font foundry bridge demo ===");
    println!(
        "BIZ UDPGothic char count: Regular={}, Bold={}",
        char_count(FoundryWeight::Regular),
        char_count(FoundryWeight::Bold)
    );
    println!();

    // supports 判定
    let sample_chars = ['A', 'あ', '森', '林', '明', '本', '日', '🎉'];
    println!("Char support (Regular):");
    for ch in sample_chars {
        let ok = supports(ch, FoundryWeight::Regular);
        println!(
            "  '{ch}' (U+{:04X}): {}",
            ch as u32,
            if ok { "YES" } else { "NO" }
        );
    }
    println!();

    // 単一 kanji lookup
    let thickness = 0.02_f32;
    if let Some((_, advance)) = cjk_glyph('森', FoundryWeight::Bold, thickness) {
        println!("'森' (Bold) advance: {advance:.4} em");
    } else {
        eprintln!("'森' unexpectedly missing from Bold data");
        std::process::exit(1);
    }

    // mixed string 2D layout
    let text = "HELLO 森林";
    let node_2d = cjk_string(text, FoundryWeight::Regular, thickness, 0.05).unwrap_or_else(|| {
        eprintln!("cjk_string returned None for '{text}'");
        std::process::exit(1);
    });
    let d_far = alice_sdf::sdf2d::eval_2d(&node_2d, [50.0, 50.0]);
    println!();
    println!("'{text}' 2D layout: far point d={d_far:.4} (should be > 1.0)");
    assert!(d_far > 1.0, "far exterior should have d > 1.0, got {d_far}");

    // 3D layout
    let node_3d =
        cjk_string_3d(text, FoundryWeight::Bold, thickness, 0.05, 0.1).unwrap_or_else(|| {
            eprintln!("cjk_string_3d returned None for '{text}'");
            std::process::exit(1);
        });
    let seg_count = count_segments(&node_3d);
    println!("'{text}' 3D layout: Segment2D leaves = {seg_count}");
    assert!(
        seg_count > 100,
        "'{text}' 3D should produce many Segment2D leaves (contour tessellation), got {seg_count}"
    );

    println!();
    println!("=== ALL PASS (foundry bridge functional) ===");
}

fn count_segments(n: &alice_sdf::types::SdfNode) -> u32 {
    match n {
        alice_sdf::types::SdfNode::Union { a, b } => count_segments(a) + count_segments(b),
        alice_sdf::types::SdfNode::Translate { child, .. } => count_segments(child),
        alice_sdf::types::SdfNode::Segment2D { .. } => 1,
        _ => 0,
    }
}
