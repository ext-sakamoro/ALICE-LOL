//! # `skadis_v10_panel` — SKADIS の板 (V10 の spec) と connector 2 種を作り、最も薄い肉を確かめる
//!
//! 板は spec (格子 / 抜き穴 / 縁の余白 / clearance) から作り、最も薄い肉が下限に届かない spec は
//! `SpecError::Geometry` で拒む 板の面は X-Z、厚みは Y (中心が原点)
//!
//! ```bash
//! cargo run --example skadis_v10_panel --release
//! ```

use alice_lol::stdlib::hardsurface::skadis_sdf::{
    skadis_connector_2x2_sdf, skadis_connector_4x4_sdf, try_skadis_panel, SkadisPanelSpec,
    SKADIS_CONN_2X2_H, SKADIS_CONN_2X2_R, SKADIS_CONN_2X2_SLOT, SKADIS_CONN_2X2_W,
    SKADIS_CONN_4X4_FILLET, SKADIS_CONN_4X4_REACH, SKADIS_PANEL_THICKNESS, SKADIS_V10_CORNER_R,
    SKADIS_V10_PEG_CLEARANCE,
};
use alice_sdf::eval::eval;
use glam::Vec3;

fn main() {
    let spec = SkadisPanelSpec::v10(300.0);
    let wall = spec.min_wall_mm().expect("the V10 spec is valid");
    println!(
        "V10 300 mm: corner R{SKADIS_V10_CORNER_R}, peg clearance {SKADIS_V10_PEG_CLEARANCE} mm, thinnest wall {wall:.2} mm"
    );
    // 下限 1.2 mm (3 周回 × 0.4 mm の nozzle) は通り、2.5 mm は通らない
    let panel = try_skadis_panel(&spec, 1.2).expect("2.0 mm walls pass a 1.2 mm bound");
    println!(
        "  centre (0, 0, 0) is {}",
        if eval(&panel, Vec3::ZERO) > 0.0 {
            "a cutout"
        } else {
            "material"
        }
    );
    match try_skadis_panel(&spec, 2.5) {
        Err(e) => println!("  2.5 mm bound: refused ({e})"),
        Ok(_) => println!("  2.5 mm bound: built (unexpected)"),
    }
    let c2 = skadis_connector_2x2_sdf(SKADIS_PANEL_THICKNESS);
    let c4 = skadis_connector_4x4_sdf(SKADIS_PANEL_THICKNESS);
    let (slot_w, slot_h, slot_r) = SKADIS_CONN_2X2_SLOT;
    println!(
        "connector 2x2: {SKADIS_CONN_2X2_W} x {SKADIS_CONN_2X2_H} mm R{SKADIS_CONN_2X2_R}, slot {slot_w} x {slot_h} R{slot_r}, hole (20, 0, 6) {}",
        if eval(&c2, Vec3::new(20.0, 0.0, 6.0)) > 0.0 { "open" } else { "closed" }
    );
    println!(
        "connector 4x4: {} mm square, corner notch R{SKADIS_CONN_4X4_FILLET}, hole (6, 0, 6) {}",
        2.0 * SKADIS_CONN_4X4_REACH,
        if eval(&c4, Vec3::new(6.0, 0.0, 6.0)) > 0.0 {
            "open"
        } else {
            "closed"
        }
    );
}
