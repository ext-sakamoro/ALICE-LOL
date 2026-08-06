//! # skadis_panel_dc_vs_mc — Marching Cubes vs Dual Contouring 実測比較 (Phase 3'')
//!
//! 同じ SKADIS panel SDF (`skadis_panel_sdf`) を MC / DC 両 algorithm で mesh 化し、
//! triangle 数 / vertex 数 / 実行時間 / watertight 判定を比較する
//!
//! **目的**: ALICE 三相原理 Phase 2 Law 経路 (SDF+DC) が Bamboo 実測「MC で 1.7mm → 5.1mm、
//! 6177 non-manifold edges」問題を回避できるかの実証
//!
//! ```bash
//! cargo run --example skadis_panel_dc_vs_mc --release
//! ```

use alice_lol::print_export::{
    node_to_3mf, node_to_3mf_dual_contouring, node_to_mesh_dual_contouring, DualContouringConfig,
    MarchingCubesConfig, MeshRepair, PrintConfig,
};
use alice_lol::stdlib::hardsurface::skadis_sdf::{skadis_panel_sdf, SKADIS_PANEL_THICKNESS};
use alice_sdf::mesh::{sdf_to_mesh, validate_mesh};
use glam::Vec3;
use std::path::PathBuf;
use std::time::Instant;

fn output_path(name: &str) -> PathBuf {
    let out_dir = std::env::temp_dir().join("alice_lol_skadis_dc_vs_mc");
    std::fs::create_dir_all(&out_dir).ok();
    out_dir.join(name)
}

fn main() {
    println!("=== ALICE-LOL Phase 3'' — SKADIS panel MC vs DC 実測比較 ===\n");

    // SKADIS panel SDF (300×300×5mm、Bamboo canonical spec)
    let panel = skadis_panel_sdf(300.0, SKADIS_PANEL_THICKNESS, 5.0);
    // Bounding box (300×300 板、Y 方向 5mm)
    let bounds_min = Vec3::new(-155.0, -3.0, -155.0);
    let bounds_max = Vec3::new(155.0, 3.0, 155.0);

    // ────────────────────────────────
    // Marching Cubes (resolution 128 / 256)
    // ────────────────────────────────
    for &resolution in &[128_usize, 256] {
        println!("--- Marching Cubes (resolution {resolution}) ---");
        let mc_config = MarchingCubesConfig {
            resolution,
            compute_normals: true,
            ..MarchingCubesConfig::default()
        };
        let t0 = Instant::now();
        let mesh_raw = sdf_to_mesh(&panel, bounds_min, bounds_max, &mc_config);
        let mesh = MeshRepair::repair_all(&mesh_raw, 5e-3);
        let elapsed = t0.elapsed();
        let quality = validate_mesh(&mesh);
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
        let mc_path = output_path(&format!("skadis_panel_mc_r{resolution}.3mf"));
        match node_to_3mf(
            &panel,
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
    // Dual Contouring (resolution 128 / 256)
    // ────────────────────────────────
    for &resolution in &[128_usize, 256] {
        println!("--- Dual Contouring (resolution {resolution}) ---");
        let dc_config = DualContouringConfig {
            resolution,
            compute_normals: true,
            ..DualContouringConfig::default()
        };
        let t0 = Instant::now();
        let mesh =
            alice_lol::print_export::dual_contouring(&panel, bounds_min, bounds_max, &dc_config);
        let elapsed = t0.elapsed();
        let quality = validate_mesh(&mesh);
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
        // node_to_mesh_dual_contouring は PrintConfig 経由なので raw mesh を再度使わず、
        // 直接 alice_sdf::io::export_3mf で出力
        let dc_path = output_path(&format!("skadis_panel_dc_r{resolution}.3mf"));
        match node_to_3mf_dual_contouring(
            &panel,
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
        // node_to_mesh_dual_contouring 経由の mesh 品質も確認 (path 経由と同一)
        let _mesh_via_wrapper = node_to_mesh_dual_contouring(
            &panel,
            &PrintConfig {
                resolution,
                bounds_min,
                bounds_max,
                scale_mm: 1.0,
            },
        );
        println!();
    }

    println!("=== Done ===");
    println!("output dir: {}", output_path("").display());
    println!("観察点: DC の non_manifold_edges が MC より劇的に少なければ ALICE way 回帰の裏付け");
    println!("Bamboo 実測「MC で 6177 non-manifold edges」と本 example の DC 実測値を比較");
}
