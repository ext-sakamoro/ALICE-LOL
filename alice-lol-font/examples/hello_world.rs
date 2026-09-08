//! Example: "HELLO WORLD" を layout で 2D / 3D 両方に配置して verify
//!
//! Run: `cargo run -p alice-lol-font --example hello_world`
//!
//! Phase 1-6 実装 (layout)
//!
//! 1. `layout_string_2d("HELLO WORLD", ...)` で 2D SDF 化
//! 2. `layout_string_3d(..., 0.1)` で 3D extrude
//! 3. `measure_string` で cursor 幅計算
//! 4. 各 glyph の期待位置で interior 判定 (space 位置は exterior 判定)

use alice_lol_font::layout::{layout_string_2d, layout_string_3d, measure_string, LayoutConfig};
use alice_lol_font::{MetaFontParams, PenModel};

fn main() {
    let text = "HELLO WORLD";
    let params = MetaFontParams::sans_bold();
    let pen = PenModel::from_params(&params);
    let config = LayoutConfig::default();

    println!("=== alice-lol-font: layout demo ({text}) ===");
    println!(
        "PenModel: half_width={:.4}, letter_spacing={:.3}, space_width={:.2}",
        pen.half_width, config.letter_spacing, config.space_width
    );
    println!();

    // measure_string で幅計算
    let width = measure_string(text, &config);
    println!("Total width: {width:.3} em");
    println!();

    // 2D layout
    let node_2d = layout_string_2d(text, &pen, &config).expect("non-empty output");

    // interior 判定 point: 各 letter の期待位置に interior sample
    // cursor 累積: H(0..1) + s(0.05) + E(1.05..2.05) + s(0.05) + L(2.10..3.10) + s(0.05)
    // + L(3.15..4.15) + s(0.05) + O(4.20..5.20) + space(0.5→5.70) + W(5.70..6.70)
    // + s(0.05) + O(6.75..7.75) + s(0.05) + R(7.80..8.80) + s(0.05) + L(8.85..9.85)
    // + s(0.05) + D(9.90..10.90)
    let cases = [
        ('H', [0.5_f32, 0.35_f32]), // H 中央横棒
        ('E', [1.05, 0.35]),        // E 左縦棒
        ('L', [2.10, 0.35]),        // L 左縦棒
        ('L', [3.15, 0.35]),        // 2 つ目 L
        ('O', [5.20, 0.35]),        // O 右端 stroke
        ('W', [5.825, 0.35]),       // W の 1 対角線 (0.125 em 内側)
        ('O', [7.75, 0.35]),        // 2 つ目 O 右端
        ('R', [7.80, 0.35]),        // R 左縦棒
        ('L', [8.85, 0.35]),        // 3 つ目 L
        ('D', [9.90, 0.35]),        // D 左縦棒
    ];

    let mut interior_pass = 0_u32;
    for (ch, point) in cases {
        let d = alice_sdf::sdf2d::eval_2d(&node_2d, point);
        let verdict = if d <= 0.05 {
            interior_pass += 1;
            "INTERIOR"
        } else {
            "MISS"
        };
        println!("  '{ch}' at {point:?}: d={d:.4} [{verdict}]");
    }
    println!();
    println!("Interior samples PASS: {interior_pass} / {}", cases.len());

    // space 位置の exterior 判定 (H..O の後、W の前)
    let space_x = 5.45; // "HELLO " の後、" WORLD" の前の中間
    let d_space = alice_sdf::sdf2d::eval_2d(&node_2d, [space_x, 0.35]);
    println!();
    println!("Space region at x={space_x}: d={d_space:.4} (should be > 0.1)");

    // 3D layout verify
    let node_3d = layout_string_3d(text, &pen, &config, 0.1).expect("non-empty output");
    let seg_count = count_segments(&node_3d);
    println!();
    println!("3D layout: total Segment2D leaves = {seg_count}");

    // final judgment
    let all_pass = interior_pass == cases.len() as u32 && d_space > 0.1;
    println!();
    if all_pass {
        println!(
            "=== ALL PASS ({interior_pass}/{} interior + space gap verified) ===",
            cases.len()
        );
    } else {
        println!("=== SOME CHECKS FAILED — inspect layout ===");
        std::process::exit(1);
    }
}

fn count_segments(n: &alice_sdf::types::SdfNode) -> u32 {
    match n {
        alice_sdf::types::SdfNode::Union { a, b } => count_segments(a) + count_segments(b),
        alice_sdf::types::SdfNode::Translate { child, .. } => count_segments(child),
        alice_sdf::types::SdfNode::Segment2D { .. } => 1,
        _ => 0,
    }
}
