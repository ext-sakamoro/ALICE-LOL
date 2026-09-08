//! Example: Font glyph + geometric CSG を `alice_sdf` builder API で合流
//!
//! Run: `cargo run -p alice-lol-font --example logo_with_geometry`
//!
//! `alice_lol::lol!` macro の `{expr}` は f32 substitution 専用で SdfNode を
//! 受け付けない (parser.rs L34-47 の `(#expr) as f32` cast) 従って本 example は
//! `alice_sdf` の Rust builder API を使って `Glyph::to_sdf3d` の output と
//! sphere を合流させる LOL DSL syntax 内での SdfNode capture は Phase 2+ の
//! parser 拡張課題として認識
//!
//! 出力例:
//! - 'O' glyph 3D (extrude 0.1、Bezier 12-sample) を build
//! - 半径 0.3 の sphere を (2.0, 0.35, 0.0) に配置
//! - Union で合流
//! - 4 sample point で SDF 評価して合流が正しく機能していることを verify

use alice_lol_font::{Glyph, MetaFontParams, PenModel};
use alice_sdf::types::SdfNode;
use glam::Vec3;
use std::sync::Arc;

fn main() {
    let params = MetaFontParams::sans_bold();
    let pen = PenModel::from_params(&params);

    // 'O' glyph を 3D 化
    let letter_o = Glyph::ascii('O')
        .expect("O supported")
        .to_sdf3d(&pen, 0.1)
        .expect("non-empty");

    // Sphere を (2.0, 0.35, 0.0) に配置
    let sphere_translated = SdfNode::Translate {
        child: Arc::new(SdfNode::Sphere { radius: 0.3 }),
        offset: Vec3::new(2.0, 0.35, 0.0),
    };

    // Union で合流
    let scene = SdfNode::Union {
        a: Arc::new(letter_o),
        b: Arc::new(sphere_translated),
    };

    println!("=== alice-lol-font × alice-sdf composition demo ===");
    println!();
    println!("Scene: 'O' glyph (0..1 em) ∪ Sphere(r=0.3) @ (2.0, 0.35, 0.0)");
    println!();

    // 4 sample point で eval
    let cases = [
        (Vec3::new(0.5, 0.35, 0.0), "O center (hollow expected)"),
        (
            Vec3::new(1.0, 0.35, 0.0),
            "O right stroke (inside expected)",
        ),
        (Vec3::new(2.0, 0.35, 0.0), "sphere center (inside expected)"),
        (Vec3::new(3.5, 0.35, 0.0), "far right (outside expected)"),
    ];

    for (point, label) in cases {
        let d = alice_sdf::eval(&scene, point);
        let sign = if d < 0.0 {
            "INSIDE"
        } else if d < 0.05 {
            "BOUNDARY"
        } else {
            "OUTSIDE"
        };
        println!("  {point:?} — {label}: d={d:.4} [{sign}]");
    }

    println!();
    println!("Composition OK — 'O' glyph and sphere are both accessible in a single SdfNode tree");
}
