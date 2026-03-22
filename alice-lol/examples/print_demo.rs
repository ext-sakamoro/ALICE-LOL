//! 3D Print Structural Intent デモ
//!
//! 用途に応じた3つのモデリング戦略を示す:
//! 1. 装飾品（Decorative）: onion で中空化 → フィラメント節約
//! 2. 構造部品（Structural）: lattice_infill でTPMSラティス充填 → 軽量かつ高強度
//! 3. 重量物（Solid）: そのまま → 最大剛性
//!
//! ```bash
//! cargo run --example print_demo
//! ```

use alice_lol::{eval, lol};
use glam::Vec3;

fn main() {
    println!("=== ALICE-LOL 3D Print Structural Intent Demo ===\n");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 1. 装飾品 — 中空シェル（onion）
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- 1. Decorative: Hollow Shell (onion) ---");
    let decorative = lol! { onion(0.02, sphere(1.0)) };
    print_eval_grid(&decorative, "Hollow Sphere (shell=0.02)");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 2a. 構造部品 — ジャイロイドインフィル
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- 2a. Structural: Gyroid Infill (lattice_infill) ---");
    let gyroid_part = lol! { lattice_infill(0.05, 5.0, 0.02, box3d(1.0, 1.0, 1.0)) };
    print_eval_grid(&gyroid_part, "Box + Gyroid (shell=0.05, scale=5.0)");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 2b. 構造部品 — ダイヤモンドサーフェスインフィル
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- 2b. Structural: Diamond Infill (diamond_infill) ---");
    let diamond_part = lol! { diamond_infill(0.05, 5.0, 0.02, sphere(1.0)) };
    print_eval_grid(&diamond_part, "Sphere + Diamond (shell=0.05, scale=5.0)");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 2c. 構造部品 — Schwarz-P インフィル
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- 2c. Structural: Schwarz-P Infill (schwarz_infill) ---");
    let schwarz_part = lol! { schwarz_infill(0.05, 5.0, 0.02, cylinder(0.8, 1.0)) };
    print_eval_grid(
        &schwarz_part,
        "Cylinder + Schwarz-P (shell=0.05, scale=5.0)",
    );

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 3. 重量物 — ソリッド（変更なし）
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- 3. Solid: No Modification ---");
    let solid = lol! { sphere(1.0) };
    print_eval_grid(&solid, "Solid Sphere (no hollowing)");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // GLSL 出力例
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- GLSL Output (lattice_infill) ---");
    let glsl = alice_lol::to_glsl(&gyroid_part);
    println!("{}\n", &glsl[..glsl.len().min(500)]);

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // ランタイムパーサー（LLM出力をそのままパース）
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- Runtime Parser (LLM Text → SdfNode) ---");
    let lol_text = "lattice_infill(0.05, 5.0, 0.02, sphere(1.0))";
    println!("Input:  {lol_text}");
    let node = alice_lol::runtime_parser::parse_lol(lol_text).unwrap();
    let d = eval(&node, Vec3::ZERO);
    println!("eval(origin) = {d:.4}\n");

    println!("=== Done ===");
}

fn print_eval_grid(node: &alice_lol::SdfNode, label: &str) {
    println!("  {label}");
    let samples = [
        ("origin     ", Vec3::ZERO),
        ("surface    ", Vec3::new(1.0, 0.0, 0.0)),
        ("inside 0.5 ", Vec3::new(0.5, 0.0, 0.0)),
        ("outside 1.5", Vec3::new(1.5, 0.0, 0.0)),
    ];
    for (name, p) in &samples {
        let d = eval(node, *p);
        let inside = if d < 0.0 { "INSIDE" } else { "outside" };
        println!("    {name}: d={d:+.4}  [{inside}]");
    }
    println!();
}
