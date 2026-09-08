//! Example: "HELLO" を STL file に書き出す (Bambu Studio / OrcaSlicer で読み込み可能)
//!
//! Run: `cargo run -p alice-lol-font --example stl_hello --features stl-export`
//!
//! Phase 1-9 実装 (`#[cfg(feature = "stl-export")]`)
//!
//! 出力先: `$TMPDIR/alice_lol_font_hello.stl` (macOS: `/var/folders/...`、Linux: `/tmp/...`)
//! resolution=128 (標準品質)、extrude_depth=0.15 em、sans_bold preset

use alice_lol_font::stl_export::preset_text_to_stl;
use alice_lol_font::FontPreset;

fn main() {
    let path = std::env::temp_dir().join("alice_lol_font_hello.stl");
    let text = "HELLO";
    let extrude_depth = 0.15_f32;

    println!("=== alice-lol-font STL export demo ===");
    println!("Text: {text:?}");
    println!("Preset: SansBold (BIZ UDPGothic 参考の parametric bold)");
    println!(
        "Extrude depth: {extrude_depth} em (= {}mm at DEFAULT_MM_SCALE 10.0)",
        extrude_depth * 10.0
    );
    println!("Resolution: DEFAULT_RESOLUTION=128 (標準品質)");
    println!("Output: {}", path.display());
    println!();

    let stats = preset_text_to_stl(FontPreset::SansBold, text, extrude_depth, &path)
        .unwrap_or_else(|e| {
            eprintln!("STL export failed: {e:?}");
            std::process::exit(1);
        });

    let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    println!("=== SUCCESS ===");
    println!("  vertices: {}", stats.vertex_count);
    println!("  triangles: {}", stats.triangle_count);
    println!(
        "  file size: {file_size} bytes ({:.2} KB)",
        file_size as f64 / 1024.0
    );
    println!("  path: {}", stats.path);
    println!();
    println!(
        "次のステップ: このファイルを Bambu Studio / OrcaSlicer / PrusaSlicer で開いて 3D print"
    );
}
