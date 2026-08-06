//! # hardsurface_thin_pattern — Phase A.5.2 薄物 2D+extrude 経路デモ
//!
//! Bamboo 実プリント合格の shopping cart coin と SKADIS panel を LOL primitive で
//! 生成し、alice-sdf `polygon_extrude` 経路で watertight mesh 化 (非多様体なし)
//!
//! ```bash
//! cargo run --example hardsurface_thin_pattern --release
//! ```
//!
//! 3MF 出力までを LOL 単独で完結 (Bamboo Python `generate.py` の代替)

use alice_lol::print_export::{polygon_to_3mf, polygon_to_mesh, polygon_to_stl};
use alice_lol::stdlib::hardsurface::thin::{
    shopping_cart_coin_2d, skadis_panel_2d, thin_plate, COIN_100YEN_DIAMETER,
    COIN_100YEN_THICKNESS, SKADIS_PANEL_THICKNESS,
};
use std::path::PathBuf;

fn output_path(name: &str) -> PathBuf {
    let out_dir = std::env::temp_dir().join("alice_lol_thin_pattern");
    std::fs::create_dir_all(&out_dir).ok();
    out_dir.join(name)
}

fn main() {
    println!("=== ALICE-LOL Phase A.5.2 薄物 2D+extrude pattern demo ===\n");

    // ────────────────────────────────
    // 1. Shopping cart coin (100yen 型、Bamboo 実プリント合格 spec)
    // ────────────────────────────────
    println!("--- Shopping cart coin (Φ{COIN_100YEN_DIAMETER}mm × {COIN_100YEN_THICKNESS}mm) ---");
    let coin = shopping_cart_coin_2d(COIN_100YEN_DIAMETER, 48);
    let coin_mesh = polygon_to_mesh(&coin, COIN_100YEN_THICKNESS * 0.5, 1.0);
    println!(
        "  vertices: {}, triangles: {}",
        coin_mesh.vertices.len(),
        coin_mesh.indices.len() / 3
    );
    // Bamboo 実測「SDF resolution 256 で 6177 non-manifold edges」の対比:
    // 本経路は earcutr triangulation + top/bottom/side wall で watertight 保証
    let max_y = coin_mesh
        .vertices
        .iter()
        .map(|v| v.position.y)
        .fold(f32::MIN, f32::max);
    let min_y = coin_mesh
        .vertices
        .iter()
        .map(|v| v.position.y)
        .fold(f32::MAX, f32::min);
    println!(
        "  thickness: Y range = [{min_y:.3}, {max_y:.3}] (target ± {:.3})",
        COIN_100YEN_THICKNESS * 0.5
    );
    let coin_path = output_path("shopping_cart_coin_100yen.3mf");
    let stats =
        polygon_to_3mf(&coin, COIN_100YEN_THICKNESS * 0.5, &coin_path).expect("coin 3MF export");
    println!("  {stats}");

    // ────────────────────────────────
    // 2. SKADIS panel (300×300×5mm、Bamboo canonical Python 版と同 spec)
    // ────────────────────────────────
    println!("\n--- SKADIS panel (300×300×{SKADIS_PANEL_THICKNESS}mm) ---");
    let panel = skadis_panel_2d(300.0, 5.0);
    println!("  peg holes: {}", panel.holes.len());
    let panel_mesh = polygon_to_mesh(&panel, SKADIS_PANEL_THICKNESS * 0.5, 1.0);
    println!(
        "  vertices: {}, triangles: {}",
        panel_mesh.vertices.len(),
        panel_mesh.indices.len() / 3
    );
    let panel_path = output_path("skadis_panel_300x300.3mf");
    let stats = polygon_to_3mf(&panel, SKADIS_PANEL_THICKNESS * 0.5, &panel_path)
        .expect("panel 3MF export");
    println!("  {stats}");

    // ────────────────────────────────
    // 3. Thin plate (汎用 100×50×2mm、Φ4mm bolt holes 2 個)
    // ────────────────────────────────
    println!("\n--- Thin plate (100×50×2mm、M3 clearance holes ×2) ---");
    let plate = thin_plate(100.0, 50.0, 3.0, &[(30.0, 0.0, 1.6), (-30.0, 0.0, 1.6)]);
    let plate_mesh = polygon_to_mesh(&plate, 1.0, 1.0);
    println!(
        "  vertices: {}, triangles: {}",
        plate_mesh.vertices.len(),
        plate_mesh.indices.len() / 3
    );
    let plate_stl_path = output_path("thin_plate_100x50x2.stl");
    let stats = polygon_to_stl(&plate, 1.0, &plate_stl_path).expect("plate STL export");
    println!("  {stats}");

    println!("\n=== Done ===");
    println!("output dir: {}", output_path("").display());
    println!("経路: LOL Polygon2D → alice_sdf::polygon_extrude → watertight Mesh → 3MF/STL");
    println!(
        "SDF+MC 経由でないため薄物 (<=5mm) の非多様体問題なし (Bamboo canonical Python の Rust 版)"
    );
}
