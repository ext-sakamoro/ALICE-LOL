//! # hardsurface_snap_case — Phase A.2 組立 primitive デモ
//!
//! ケース底 (60 × 40 × 3mm) の 4 隅に snap_fit_cantilever を配置し、
//! 蓋 (60 × 40 × 3mm) をパチンと閉じられる closure を組立てる
//!
//! ```bash
//! cargo run --example hardsurface_snap_case --release
//! ```
//!
//! joint::snap_fit_cantilever + PLA 応力計算の実使用例

use alice_lol::stdlib::hardsurface::joint::{
    snap_fit_cantilever, SnapFitCantileverSpec, PLA_ELASTIC_MODULUS_GPA, PLA_YIELD_STRESS_MPA,
};
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
    println!("=== ALICE-LOL Phase A.2 hardsurface::joint demo ===\n");

    // ────────────────────────────────
    // Snap-fit 応力チェック
    // ────────────────────────────────
    let spec = SnapFitCantileverSpec::PLA_STANDARD;
    let pla_stress = spec.peak_stress_mpa(PLA_ELASTIC_MODULUS_GPA);
    let safety_factor = PLA_YIELD_STRESS_MPA / pla_stress;
    println!("--- Snap-fit spec (PLA_STANDARD) ---");
    println!(
        "  L={}mm t={}mm δ={}mm w={}mm",
        spec.length, spec.thickness, spec.hook_height, spec.width
    );
    println!("  Peak stress = {pla_stress:.2} MPa (PLA E=3.5GPa)");
    println!("  Yield stress = {PLA_YIELD_STRESS_MPA} MPa → safety factor = {safety_factor:.2}");

    // ────────────────────────────────
    // ケース底 (60 × 40 × 3mm)
    // ────────────────────────────────
    let case_hx: f32 = 30.0;
    let case_hy: f32 = 1.5; // 厚 3mm
    let case_hz: f32 = 20.0;
    let case_bottom = SdfNode::Box3d {
        half_extents: Vec3::new(case_hx, case_hy, case_hz),
    };

    // ────────────────────────────────
    // 4 隅の snap-fit cantilever
    //   梁は Y 方向に立ち上がる (case 上面から上へ)
    //   snap_fit_cantilever は X 軸方向に伸びる → 90° Y 回りに rotate + Z 軸に立てる
    //   ここでは 4 隅を単純化して「梁を Y 方向に立てる」変換のみ
    // ────────────────────────────────
    let cantilever = snap_fit_cantilever(spec);
    // 90° around Z axis で X 方向梁を Y 方向に立てる
    let cantilever_vertical = SdfNode::Rotate {
        child: Arc::new(cantilever),
        rotation: glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
    };

    let mut assembly = case_bottom;
    for (sx, sz) in [(1.0_f32, 1.0_f32), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        // corner 位置に配置、Y は case 上面 + 梁長さ/2
        let corner = translate(
            cantilever_vertical.clone(),
            Vec3::new(
                sx * (case_hx - spec.thickness),
                case_hy + spec.length * 0.5,
                sz * (case_hz - spec.thickness),
            ),
        );
        assembly = union(assembly, corner);
    }

    // ────────────────────────────────
    // 診断: 代表点で SDF 値評価
    // ────────────────────────────────
    let samples = [
        ("ケース底中央 (材料内)  ", Vec3::new(0.0, 0.0, 0.0)),
        ("ケース上面 (材料表面)  ", Vec3::new(0.0, 1.5, 0.0)),
        ("cantilever 位置 (Y=6.5)", Vec3::new(28.0, 6.5, 18.0)),
        ("空間 (Y=20)            ", Vec3::new(0.0, 20.0, 0.0)),
    ];
    for (label, p) in &samples {
        let d = eval(&assembly, *p);
        let state = if d < 0.0 { "INSIDE" } else { "outside" };
        println!("  {label}: d = {d:+.3}  [{state}]");
    }

    println!("\n=== Done ===");
    println!("assembly = case(60×40×3) ∪ 4×snap_fit_cantilever(vertical)");
}
