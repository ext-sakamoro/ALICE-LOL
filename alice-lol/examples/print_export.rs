//! LOL → STL/3MF エクスポートデモ
//!
//! 3Dプリントの3つの用途別モデルをSTL & 3MFファイルに出力する。
//! 出力先: カレントディレクトリの `lol_print_output/`
//!
//! ```bash
//! cargo run --example print_export
//! ```

use alice_lol::lol;
use alice_lol::print_export::{node_to_stl, node_to_3mf, lol_to_stl, PrintConfig};
use glam::Vec3;

fn main() {
    let out_dir = std::path::Path::new("lol_print_output");
    std::fs::create_dir_all(out_dir).expect("failed to create output directory");

    println!("=== ALICE-LOL → 3D Print Export ===\n");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 共通設定
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    let config = PrintConfig::high_quality()
        .with_bounds(Vec3::splat(-1.5), Vec3::splat(1.5))
        .with_scale_mm(20.0); // 1.0 unit = 20mm → 球 r=1.0 → 直径40mm

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // A. 装飾品 — 中空ツイスト花瓶
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- A. Decorative: Hollow Twisted Vase ---");
    let vase = lol! {
        onion(0.03,
            twist(0.8,
                taper(0.4,
                    cylinder(0.8, 1.2)
                )
            )
        )
    };
    match node_to_stl(&vase, out_dir.join("A_decorative_vase.stl"), &config) {
        Ok(stats) => println!("  STL: {stats}"),
        Err(e) => println!("  STL error: {e}"),
    }
    match node_to_3mf(&vase, out_dir.join("A_decorative_vase.3mf"), &config) {
        Ok(stats) => println!("  3MF: {stats}"),
        Err(e) => println!("  3MF error: {e}"),
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // B. 構造部品 — ジャイロイドインフィル入りブラケット
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- B. Structural: Gyroid-infilled Bracket ---");
    let bracket = lol! {
        lattice_infill(0.05, 6.0, 0.02,
            subtract(
                rounded_box(1.0, 0.6, 0.4, 0.05),
                translate(0.5, 0.0, 0.0, cylinder(0.2, 0.5))
            )
        )
    };
    match node_to_stl(&bracket, out_dir.join("B_structural_bracket.stl"), &config) {
        Ok(stats) => println!("  STL: {stats}"),
        Err(e) => println!("  STL error: {e}"),
    }
    match node_to_3mf(&bracket, out_dir.join("B_structural_bracket.3mf"), &config) {
        Ok(stats) => println!("  3MF: {stats}"),
        Err(e) => println!("  3MF error: {e}"),
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // C. ソリッド — 比較用の無垢球体
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- C. Solid: Reference Sphere ---");
    let solid = lol! { sphere(1.0) };
    match node_to_stl(&solid, out_dir.join("C_solid_sphere.stl"), &config) {
        Ok(stats) => println!("  STL: {stats}"),
        Err(e) => println!("  STL error: {e}"),
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // D. ダイヤモンドインフィル — 高剛性パーツ
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- D. Diamond Infill: High-stiffness Part ---");
    let diamond_part = lol! {
        diamond_infill(0.04, 5.0, 0.02, box3d(0.8, 0.8, 0.8))
    };
    match node_to_stl(&diamond_part, out_dir.join("D_diamond_infill.stl"), &config) {
        Ok(stats) => println!("  STL: {stats}"),
        Err(e) => println!("  STL error: {e}"),
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // E. ランタイムパーサー（LLM出力テキスト → STL直行）
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- E. LLM Text → STL (runtime parser) ---");
    let lol_text = r#"schwarz_infill(0.05, 4.0, 0.02,
        smooth_union(0.1,
            capsule(0.3, 1.0),
            translate(0.0, 1.0, 0.0, sphere(0.4))
        )
    )"#;
    println!("  LOL input: {}", lol_text.lines().next().unwrap_or(""));
    match lol_to_stl(lol_text, out_dir.join("E_llm_schwarz.stl"), &config) {
        Ok(stats) => println!("  STL: {stats}"),
        Err(e) => println!("  STL error: {e}"),
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // ファイルサイズ比較
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("\n=== File Size Comparison ===");
    for entry in std::fs::read_dir(out_dir).expect("read dir") {
        if let Ok(e) = entry {
            let meta = e.metadata().expect("metadata");
            let size_kb = meta.len() as f64 / 1024.0;
            println!("  {:>8.1} KB  {}", size_kb, e.file_name().to_string_lossy());
        }
    }

    println!("\n=== Done — files in {} ===", out_dir.display());
}
