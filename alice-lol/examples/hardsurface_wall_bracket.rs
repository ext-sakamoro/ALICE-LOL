//! # hardsurface_wall_bracket — Phase A.1-A.4 統合デモ
//!
//! 壁掛け bracket を Phase A の 4 module から primitive を合成して組立てる
//! - `mount::bracket_l` — L 字本体 (fillet R3 内蔵)
//! - `mount::rack_shelf` (相当) — 水平板に M4 clearance 穴を pattern 配置
//! - `fastener::counterbore` — 垂直板に壁固定用 M5 counterbore ×2
//! - `reinforcement::rib` — 内角に補強 rib ×2
//!
//! ```bash
//! cargo run --example hardsurface_wall_bracket --release
//! ```

use alice_lol::stdlib::hardsurface::{
    fastener::{counterbore, MetricSize},
    mount::bracket_l,
    reinforcement::rib,
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

fn subtract(a: SdfNode, b: SdfNode) -> SdfNode {
    SdfNode::Subtraction {
        a: Arc::new(a),
        b: Arc::new(b),
    }
}

fn main() {
    println!("=== ALICE-LOL Phase A.1-A.4 統合 wall_bracket demo ===\n");

    // ────────────────────────────────
    // L 字本体 (水平 100 × 4 × 60、垂直 4 × 60 × 60、fillet R3)
    // ────────────────────────────────
    let horizontal_len: f32 = 100.0;
    let vertical_h: f32 = 60.0;
    let thickness: f32 = 4.0;
    let depth: f32 = 60.0;
    let bracket = bracket_l(horizontal_len, vertical_h, thickness, depth, 3.0);

    // ────────────────────────────────
    // 垂直板に M5 counterbore 壁固定穴 ×2 (Z 軸方向に離間)
    // 垂直板中心 X = -(horizontal_len - thickness)/2、Y = (vertical_h + thickness)/2
    // ────────────────────────────────
    let vertical_center_x = -(horizontal_len - thickness) * 0.5;
    let vertical_center_y = (vertical_h + thickness) * 0.5;
    let wall_bore = counterbore(MetricSize::M5, thickness);
    // Counterbore の穴軸 = Y 軸なので、垂直板の X 軸貫通にするため Z 軸周り 90° 回転
    let wall_bore_horiz = SdfNode::Rotate {
        child: Arc::new(wall_bore),
        rotation: glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
    };

    let mut assembly = bracket;
    for z in [-20.0_f32, 20.0] {
        let placed = translate(
            wall_bore_horiz.clone(),
            Vec3::new(vertical_center_x, vertical_center_y, z),
        );
        assembly = subtract(assembly, placed);
    }

    // ────────────────────────────────
    // 内角補強 rib ×2 (垂直板と水平板の間、Z 軸方向 2 箇所)
    // rib 長 = 30mm × 高 25mm × 厚 3mm、内角に沿って配置
    // ────────────────────────────────
    let inner_rib = rib(30.0, 25.0, 3.0);
    for z in [-25.0_f32, 25.0] {
        // 内角付近 (X ≈ -35、Y ≈ 15) に配置
        let placed = translate(
            inner_rib.clone(),
            Vec3::new(vertical_center_x + 20.0, thickness + 12.5, z),
        );
        assembly = union(assembly, placed);
    }

    // ────────────────────────────────
    // 診断: 代表点で SDF 値評価
    // ────────────────────────────────
    let samples = [
        ("水平板中央          ", Vec3::new(0.0, 0.0, 0.0)),
        (
            "垂直板中央          ",
            Vec3::new(vertical_center_x, vertical_center_y, 0.0),
        ),
        (
            "壁固定穴 Z=+20     ",
            Vec3::new(vertical_center_x, vertical_center_y, 20.0),
        ),
        ("補強 rib 位置 Z=+25", Vec3::new(-28.0, 12.5, 25.0)),
        ("外部空間            ", Vec3::new(100.0, 100.0, 100.0)),
    ];
    for (label, p) in &samples {
        let d = eval(&assembly, *p);
        let state = if d < 0.0 { "INSIDE" } else { "outside" };
        println!("  {label}: d = {d:+.3}  [{state}]");
    }

    println!("\n=== Done ===");
    println!("Phase A.1-A.4 統合: bracket_l + counterbore(M5)×2 + rib×2");
    println!("primitive 使用: bracket_l (mount) / counterbore (fastener) / rib (reinforcement)");
}
