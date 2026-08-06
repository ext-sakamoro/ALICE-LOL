//! # hardsurface_ribbed_bracket — Phase A.3 補強 primitive デモ
//!
//! L 字ブラケット (水平板 + 垂直板) の内角にフィレット + 3 本のリブを配置
//! Phase A.3 の rib / fillet を組合わせた実プリント想定 pattern
//!
//! ```bash
//! cargo run --example hardsurface_ribbed_bracket --release
//! cargo run --example hardsurface_ribbed_bracket --release --features physics
//! ```
//!
//! `--features physics` を追加すると alice-physics の材料 spec / fillet Kt 診断も出力

use alice_lol::stdlib::hardsurface::reinforcement::{fillet, rib};
use alice_sdf::{eval, SdfNode};
use glam::Vec3;
use std::sync::Arc;

fn translate(child: SdfNode, offset: Vec3) -> SdfNode {
    SdfNode::Translate {
        child: Arc::new(child),
        offset,
    }
}

fn union(a: SdfNode, b: SdfNode) -> SdfNode {
    SdfNode::Union {
        a: Arc::new(a),
        b: Arc::new(b),
    }
}

fn main() {
    println!("=== ALICE-LOL Phase A.3 hardsurface::reinforcement demo ===\n");

    // ────────────────────────────────
    // L 字ブラケット: 水平板 60 × 40 × 4 + 垂直板 4 × 40 × 40
    // ────────────────────────────────
    let horizontal = SdfNode::Box3d {
        half_extents: Vec3::new(30.0, 2.0, 20.0),
    };
    let vertical_raw = SdfNode::Box3d {
        half_extents: Vec3::new(2.0, 20.0, 20.0),
    };
    // 垂直板を水平板の -X 端に立てる
    let vertical = translate(vertical_raw, Vec3::new(-28.0, 22.0, 0.0));

    // 内角に fillet R=3mm (SmoothUnion)
    let l_bracket = fillet(horizontal, vertical, 3.0);

    // ────────────────────────────────
    // 3 rib (Z 軸方向に等間隔、L 字の内側で補強)
    // 各 rib: 長 20mm × 高 15mm × 厚 1.6mm、内角に沿って三角配置は複雑なので Box で代用
    // ────────────────────────────────
    let single_rib = rib(20.0, 15.0, 1.6);
    // 内角の rib は L 字の内側 (X=-20 付近、Y=+8) に配置
    let z_positions = [-14.0_f32, 0.0, 14.0];
    let mut assembly = l_bracket;
    for z in z_positions {
        let r = translate(single_rib.clone(), Vec3::new(-18.0, 9.5, z));
        assembly = union(assembly, r);
    }

    // ────────────────────────────────
    // 診断: 代表点で SDF 値評価
    // ────────────────────────────────
    let samples = [
        ("水平板中央           ", Vec3::new(0.0, 0.0, 0.0)),
        ("垂直板中央           ", Vec3::new(-28.0, 22.0, 0.0)),
        ("内角 fillet 領域    ", Vec3::new(-24.0, 4.0, 0.0)),
        ("rib 位置 (Z=0)      ", Vec3::new(-18.0, 10.0, 0.0)),
        ("外部空間             ", Vec3::new(50.0, 50.0, 50.0)),
    ];
    for (label, p) in &samples {
        let d = eval(&assembly, *p);
        let state = if d < 0.0 { "INSIDE" } else { "outside" };
        println!("  {label}: d = {d:+.3}  [{state}]");
    }

    // ────────────────────────────────
    // Physics feature 診断 (opt-in)
    // ────────────────────────────────
    #[cfg(feature = "physics")]
    {
        use alice_lol::stdlib::hardsurface::reinforcement::{
            fillet_kt_shaft_shoulder, material_elastic_modulus_gpa, recommended_fillet_radius_mm,
        };
        println!("\n--- Physics diagnostics (feature=\"physics\", alice-physics AGPL-3.0) ---");
        if let Some(e) = material_elastic_modulus_gpa("pla") {
            println!("  PLA elastic modulus (FilamentDb): {e:.2} GPa");
        }
        if let Some(e) = material_elastic_modulus_gpa("petg") {
            println!("  PETG elastic modulus (FilamentDb): {e:.2} GPa");
        }
        let kt = fillet_kt_shaft_shoulder(3.0, 10.0, 20.0);
        println!("  Fillet Kt (R=3mm, d=10 D=20): {kt:.2}");
        let r_target = recommended_fillet_radius_mm(10.0, 20.0, 1.5);
        println!("  Recommended R for Kt=1.5: {r_target:.2} mm");
    }
    #[cfg(not(feature = "physics"))]
    {
        println!("\n(Physics diagnostics available with --features physics)");
    }

    println!("\n=== Done ===");
    println!("assembly = L-bracket(60×4×40 + 4×40×40) with fillet R=3 and 3 rib");
}
