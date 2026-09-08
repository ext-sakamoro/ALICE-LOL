//! Example: 5 ASCII letter glyph を build して SDF eval で形が正しいか verify
//!
//! Run: `cargo run -p alice-lol-font --example glyph_to_sdf`
//!
//! MVP 5 letter (A / H / I / O / T) について:
//! 1. `Glyph::ascii(ch)` で構築
//! 2. `to_sdf2d` / `to_sdf3d` で SDF 化
//! 3. 各 letter の interior / exterior 期待点で SDF 評価
//! 4. 期待通り (interior d ≤ 0、exterior d > 0) なら PASS 表示

use alice_lol_font::{Glyph, MetaFontParams, PenModel};

fn main() {
    let params = MetaFontParams::sans_bold();
    let pen = PenModel::from_params(&params);

    println!("=== alice-lol-font: 5 letter MVP verification ===");
    println!(
        "PenModel: half_width={:.4}, thick={:.4}, thin={:.4}, slant_tan={:.4}",
        pen.half_width, pen.thick_half_width, pen.thin_half_width, pen.slant_tan
    );
    println!();

    // (letter, interior sample, exterior sample) の期待点
    let cases = [
        ('A', [0.25_f32, 0.35_f32], [0.5_f32, 0.5_f32]), // interior: 左脚中点、exterior: 三角形内部空洞
        ('H', [0.5, 0.35], [0.5, 0.6]),                  // interior: 中央横棒、exterior: 上部空洞
        ('I', [0.25, 0.35], [0.0, 0.35]), // interior: 縦棒中点、exterior: 左側 (I 外)
        ('O', [1.0, 0.35], [0.5, 0.35]),  // interior: 右端 stroke、exterior: 楕円中心 hollow
        ('T', [0.5, 0.7], [0.8, 0.35]), // interior: 上部横棒中央、exterior: 右下 (中央縦棒 x=0.5 より右)
    ];

    let mut all_pass = true;
    for (ch, interior, exterior) in cases {
        let Some(glyph) = Glyph::ascii(ch) else {
            eprintln!("  '{ch}': UNSUPPORTED (Glyph::ascii returned None)");
            all_pass = false;
            continue;
        };
        let Some(node) = glyph.to_sdf2d(&pen) else {
            eprintln!("  '{ch}': to_sdf2d returned None");
            all_pass = false;
            continue;
        };

        let d_in = alice_sdf::sdf2d::eval_2d(&node, interior);
        let d_out = alice_sdf::sdf2d::eval_2d(&node, exterior);

        let interior_ok = d_in <= 0.05;
        let exterior_ok = d_out > 0.0;
        let verdict = if interior_ok && exterior_ok {
            "PASS"
        } else {
            all_pass = false;
            "FAIL"
        };

        println!(
            "  '{ch}': advance={:.3}, strokes={}, interior({:?})={:.4}, exterior({:?})={:.4} [{verdict}]",
            glyph.advance(),
            glyph.stroke_count(),
            interior,
            d_in,
            exterior,
            d_out
        );
    }

    println!();
    if all_pass {
        println!("=== ALL 5 LETTERS PASS ===");
    } else {
        println!("=== SOME LETTERS FAILED — inspect design coordinates ===");
        std::process::exit(1);
    }

    // 3D extrude で 1 letter を build して stroke count 集約
    let letter_o_3d = Glyph::ascii('O')
        .expect("O supported")
        .to_sdf3d(&pen, 0.1)
        .expect("non-empty");
    let seg_count = count_segments(&letter_o_3d);
    println!();
    println!(
        "'O' 3D extrude (depth=0.1): total Segment2D leaves = {seg_count} (2 Bezier × 12 samples = 24 expected)"
    );
}

fn count_segments(n: &alice_sdf::types::SdfNode) -> u32 {
    match n {
        alice_sdf::types::SdfNode::Union { a, b } => count_segments(a) + count_segments(b),
        alice_sdf::types::SdfNode::Segment2D { .. } => 1,
        _ => 0,
    }
}
