//! # coin_dc_vs_mc — 極薄物 (1.7mm coin) の MC vs DC 決定的実測 (Phase 3''.2)
//!
//! Bamboo 実測「SDF+MC で 1.7mm 設計 → 5.1mm 出力、6177 non-manifold edges」を
//! 本 example で再現し、同 SDF を Dual Contouring で mesh 化した結果と比較する
//!
//! **目的**: 極薄物 (< 2mm) で DC が MC の破綻を回避できることを実測で確定させ、
//! Phase A.5 polygon_extrude (ALICE 違反、Phase 1 Data 経路) を deprecate → 削除する
//! 決定的根拠を得る
//!
//! ```bash
//! cargo run --example coin_dc_vs_mc --release
//! ```

use alice_lol::print_export::{
    node_to_3mf, node_to_3mf_dual_contouring, DualContouringConfig, MarchingCubesConfig,
    MeshRepair, PrintConfig,
};
use alice_lol::stdlib::hardsurface::thin_sdf::{
    shopping_cart_coin_sdf, COIN_100YEN_DIAMETER, COIN_100YEN_THICKNESS,
};
use alice_sdf::mesh::{sdf_to_mesh, validate_mesh};
use glam::Vec3;
use std::path::PathBuf;
use std::time::Instant;

fn output_path(name: &str) -> PathBuf {
    let out_dir = std::env::temp_dir().join("alice_lol_coin_dc_vs_mc");
    std::fs::create_dir_all(&out_dir).ok();
    out_dir.join(name)
}

fn main() {
    println!("=== ALICE-LOL Phase 3''.2 — coin (1.7mm 極薄) MC vs DC 決定的実測 ===\n");
    println!("Bamboo 実測: SDF+MC で 1.7mm 設計 → 5.1mm 出力、6177 non-manifold edges");
    println!("本 example で MC 再現 + DC の watertight 実現を実測\n");

    // Coin SDF (100 円型、Φ22.8 × 1.7mm、Bamboo 実プリント合格 spec)
    let coin = shopping_cart_coin_sdf(COIN_100YEN_DIAMETER, COIN_100YEN_THICKNESS);
    // Bounding box (Φ22.8 → hx=hz=13、Y=1.7 → hy=1.5)
    let bounds_min = Vec3::new(-13.0, -1.5, -13.0);
    let bounds_max = Vec3::new(13.0, 1.5, 13.0);

    // ────────────────────────────────
    // Marching Cubes (resolution 128 / 256 / 512)
    //   Bamboo 実測は resolution 256 で 6177 non-manifold と厚さ 5.1mm
    //   本 example で resolution 128/256/512 の 3 段階で MC の破綻を確認
    // ────────────────────────────────
    for &resolution in &[128_usize, 256, 512] {
        println!("--- Marching Cubes (resolution {resolution}) ---");
        let mc_config = MarchingCubesConfig {
            resolution,
            compute_normals: true,
            ..MarchingCubesConfig::default()
        };
        let t0 = Instant::now();
        let mesh_raw = sdf_to_mesh(&coin, bounds_min, bounds_max, &mc_config);
        let mesh = MeshRepair::repair_all(&mesh_raw, 5e-3);
        let elapsed = t0.elapsed();
        let quality = validate_mesh(&mesh);
        let max_y = mesh
            .vertices
            .iter()
            .map(|v| v.position.y)
            .fold(f32::MIN, f32::max);
        let min_y = mesh
            .vertices
            .iter()
            .map(|v| v.position.y)
            .fold(f32::MAX, f32::min);
        let actual_thickness = max_y - min_y;
        println!(
            "  vertices: {}, triangles: {}",
            mesh.vertices.len(),
            mesh.indices.len() / 3
        );
        println!("  time: {elapsed:?}");
        println!(
            "  is_manifold: {}, non_manifold_edges: {}",
            quality.is_manifold, quality.non_manifold_edges
        );
        println!(
            "  厚さ: {actual_thickness:.3}mm (設計 {:.3}mm、誤差 {:+.3}mm)",
            COIN_100YEN_THICKNESS,
            actual_thickness - COIN_100YEN_THICKNESS
        );
        let mc_path = output_path(&format!("coin_mc_r{resolution}.3mf"));
        match node_to_3mf(
            &coin,
            &mc_path,
            &PrintConfig {
                resolution,
                bounds_min,
                bounds_max,
                scale_mm: 1.0,
            },
        ) {
            Ok(stats) => println!("  3MF: {stats}"),
            Err(e) => println!("  3MF export failed: {e}"),
        }
        println!();
    }

    // ────────────────────────────────
    // Dual Contouring (resolution 128 / 256 / 512)
    // ────────────────────────────────
    for &resolution in &[128_usize, 256, 512] {
        println!("--- Dual Contouring (resolution {resolution}) ---");
        let dc_config = DualContouringConfig {
            resolution,
            compute_normals: true,
            ..DualContouringConfig::default()
        };
        let t0 = Instant::now();
        let mesh =
            alice_lol::print_export::dual_contouring(&coin, bounds_min, bounds_max, &dc_config);
        let elapsed = t0.elapsed();
        let quality = validate_mesh(&mesh);
        let max_y = mesh
            .vertices
            .iter()
            .map(|v| v.position.y)
            .fold(f32::MIN, f32::max);
        let min_y = mesh
            .vertices
            .iter()
            .map(|v| v.position.y)
            .fold(f32::MAX, f32::min);
        let actual_thickness = max_y - min_y;
        println!(
            "  vertices: {}, triangles: {}",
            mesh.vertices.len(),
            mesh.indices.len() / 3
        );
        println!("  time: {elapsed:?}");
        println!(
            "  is_manifold: {}, non_manifold_edges: {}",
            quality.is_manifold, quality.non_manifold_edges
        );
        println!(
            "  厚さ: {actual_thickness:.3}mm (設計 {:.3}mm、誤差 {:+.3}mm)",
            COIN_100YEN_THICKNESS,
            actual_thickness - COIN_100YEN_THICKNESS
        );
        let dc_path = output_path(&format!("coin_dc_r{resolution}.3mf"));
        match node_to_3mf_dual_contouring(
            &coin,
            &dc_path,
            &PrintConfig {
                resolution,
                bounds_min,
                bounds_max,
                scale_mm: 1.0,
            },
        ) {
            Ok(stats) => println!("  3MF: {stats}"),
            Err(e) => println!("  3MF export failed: {e}"),
        }
        println!();
    }

    println!("=== Done ===");
    println!("output dir: {}", output_path("").display());
    println!(
        "判定: DC が MC より厚さ精度 + non_manifold_edges の両面で優位なら\n\
         Phase A.5 polygon_extrude (Data 経路 = ALICE 違反) の deprecate 根拠として確定"
    );
}
