//! Example: 26 ASCII uppercase letter (A..=Z) を build して SDF eval で形が正しいか verify
//!
//! Run: `cargo run -p alice-lol-font --example glyph_to_sdf`
//!
//! Phase 1-5.2: 26 letter 全対応
//!
//! 1. `Glyph::ascii(ch)` で 26 letter 構築
//! 2. `to_sdf2d` / `to_sdf3d` で SDF 化
//! 3. 各 letter の interior / exterior 期待点で SDF 評価
//! 4. 期待通り (interior d ≤ 0.05、exterior d > 0.05) なら PASS 表示

use alice_lol_font::{Glyph, MetaFontParams, PenModel};

fn main() {
    let params = MetaFontParams::sans_bold();
    let pen = PenModel::from_params(&params);

    println!("=== alice-lol-font: 26 letter full alphabet verification ===");
    println!(
        "PenModel: half_width={:.4}, thick={:.4}, thin={:.4}, slant_tan={:.4}",
        pen.half_width, pen.thick_half_width, pen.thin_half_width, pen.slant_tan
    );
    println!();

    // (letter, interior sample, exterior sample) 期待点
    // 各 letter の em 座標系 (baseline y=0、cap_height y=0.7、full_advance x=1)
    let cases = [
        // sans_bold pen (half_width=0.07) は thick、内部 hollow の marginal 点回避に
        // A / G / K / R は bbox 外 exterior に、他 letter は letter shape 依存で選択
        ('A', [0.25_f32, 0.35_f32], [1.5_f32, 0.5_f32]),
        ('B', [0.0, 0.35], [1.5, 0.35]),
        ('C', [0.0, 0.35], [0.5, 0.35]),
        ('D', [0.0, 0.35], [0.5, 0.35]),
        ('E', [0.0, 0.35], [0.5, 0.55]),
        ('F', [0.0, 0.35], [0.5, 0.55]),
        ('G', [0.0, 0.35], [1.5, 0.5]),
        ('H', [0.5, 0.35], [0.5, 0.6]),
        ('I', [0.25, 0.35], [0.0, 0.35]),
        ('J', [0.65, 0.4], [0.2, 0.5]),
        ('K', [0.0, 0.35], [1.5, 0.5]),
        ('L', [0.0, 0.35], [0.5, 0.5]),
        ('M', [0.0, 0.35], [0.5, 0.6]),
        ('N', [0.0, 0.35], [0.5, 0.55]),
        ('O', [1.0, 0.35], [0.5, 0.35]),
        ('P', [0.0, 0.35], [0.5, 0.15]),
        ('Q', [1.0, 0.35], [0.5, 0.35]),
        ('R', [0.0, 0.35], [1.5, 0.5]),
        ('S', [0.5, 0.35], [1.5, 0.35]),
        ('T', [0.5, 0.7], [0.8, 0.35]),
        ('U', [0.0, 0.4], [0.5, 0.5]),
        ('V', [0.25, 0.35], [0.5, 0.5]),
        ('W', [0.125, 0.35], [0.5, 0.65]),
        ('X', [0.5, 0.35], [0.5, 0.6]),
        ('Y', [0.5, 0.15], [0.15, 0.15]),
        ('Z', [0.5, 0.7], [0.2, 0.5]),
    ];

    let mut all_pass = true;
    let mut pass_count = 0_u32;
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
        let exterior_ok = d_out > 0.05;
        let verdict = if interior_ok && exterior_ok {
            pass_count += 1;
            "PASS"
        } else {
            all_pass = false;
            "FAIL"
        };

        println!(
            "  '{ch}': strokes={}, advance={:.2}, interior({:?})={:.4}, exterior({:?})={:.4} [{verdict}]",
            glyph.stroke_count(),
            glyph.advance(),
            interior,
            d_in,
            exterior,
            d_out
        );
    }

    println!();
    println!("Result: {pass_count} / 26 letters PASS");
    if all_pass {
        println!("=== ALL 26 LETTERS PASS ===");
    } else {
        println!("=== SOME LETTERS FAILED — inspect design coordinates ===");
        std::process::exit(1);
    }

    // 3D extrude で "HELLO" を build して total Segment2D 数を報告
    println!();
    println!("=== 3D extrude sample: 'HELLO' (advance layout なし、単純 union) ===");
    let mut total_segments = 0_u32;
    for ch in ['H', 'E', 'L', 'L', 'O'] {
        let letter = Glyph::ascii(ch).expect("H/E/L/O supported");
        let node = letter.to_sdf3d(&pen, 0.1).expect("non-empty");
        let seg = count_segments(&node);
        total_segments += seg;
        println!("  '{ch}': {seg} Segment2D leaves");
    }
    println!("Total Segment2D leaves for 'HELLO': {total_segments}");
}

fn count_segments(n: &alice_sdf::types::SdfNode) -> u32 {
    match n {
        alice_sdf::types::SdfNode::Union { a, b } => count_segments(a) + count_segments(b),
        alice_sdf::types::SdfNode::Segment2D { .. } => 1,
        _ => 0,
    }
}
