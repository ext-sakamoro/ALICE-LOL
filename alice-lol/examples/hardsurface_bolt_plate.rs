//! # hardsurface_bolt_plate — Phase A.1 締結 primitive デモ
//!
//! 板 (100 × 50 × 8 mm) を M4 ボルト 4 本で固定する pattern を組立てる
//! - 4 隅に counterbore (M4 ソケットキャップ頭が沈む)
//! - 中央に heat_set_insert_hole (M3 ヒートセット下穴)
//! - 貫通穴 4 隅 + M3 熱融着 1 中央 の 5 hole plate
//!
//! ```bash
//! cargo run --example hardsurface_bolt_plate --release
//! ```
//!
//! LOL DSL primitive 71 種 + Phase A.1 fastener 6 種を組み合わせて 1 部品を構築する

use alice_lol::stdlib::hardsurface::fastener::{counterbore, heat_set_insert_hole, MetricSize};
use alice_sdf::{eval, SdfNode};
use glam::Vec3;
use std::sync::Arc;

fn translate(child: SdfNode, offset: Vec3) -> SdfNode {
    SdfNode::Translate {
        child: Arc::new(child),
        offset,
    }
}

fn subtract(a: SdfNode, b: SdfNode) -> SdfNode {
    SdfNode::Subtraction {
        a: Arc::new(a),
        b: Arc::new(b),
    }
}

fn main() {
    println!("=== ALICE-LOL Phase A.1 hardsurface::fastener demo ===\n");

    // ────────────────────────────────
    // 板本体 (100 × 50 × 8 mm)
    // ────────────────────────────────
    let plate_hx: f32 = 50.0;
    let plate_hy: f32 = 4.0;
    let plate_hz: f32 = 25.0;
    let plate = SdfNode::Box3d {
        half_extents: Vec3::new(plate_hx, plate_hy, plate_hz),
    };

    // ────────────────────────────────
    // 4 隅の M4 counterbore (Y 軸方向に貫通)
    // ────────────────────────────────
    let corner_offset_x = plate_hx - 8.0;
    let corner_offset_z = plate_hz - 8.0;
    let cb = counterbore(MetricSize::M4, plate_hy * 2.0);

    let mut assembly = plate;
    for (sx, sz) in [(1.0_f32, 1.0_f32), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        let hole = translate(
            cb.clone(),
            Vec3::new(sx * corner_offset_x, 0.0, sz * corner_offset_z),
        );
        assembly = subtract(assembly, hole);
    }

    // ────────────────────────────────
    // 中央に M3 heat_set 下穴 (深さ 4.1mm、板中心から +Y に配置して裏面に貫通させない)
    // ────────────────────────────────
    let hs = heat_set_insert_hole(MetricSize::M3);
    let hs_placed = translate(hs, Vec3::new(0.0, plate_hy - 2.05, 0.0));
    assembly = subtract(assembly, hs_placed);

    // ────────────────────────────────
    // 診断: 代表点で SDF 値を評価
    // ────────────────────────────────
    let samples = [
        ("板中央 (穴内側)     ", Vec3::new(0.0, 0.0, 0.0)),
        ("板端 (材料内側)    ", Vec3::new(20.0, 0.0, 5.0)),
        ("板外 (空間)        ", Vec3::new(100.0, 0.0, 100.0)),
        ("角穴中心 (穴内側)  ", Vec3::new(42.0, 0.0, 17.0)),
    ];
    for (label, p) in &samples {
        let d = eval(&assembly, *p);
        let inside = if d < 0.0 { "INSIDE" } else { "outside" };
        println!("  {label}: d = {d:+.3}  [{inside}]");
    }

    println!("\n=== Done ===");
    println!("assembly = plate(100×50×8) - 4×counterbore(M4) - 1×heat_set_insert(M3)");
}
