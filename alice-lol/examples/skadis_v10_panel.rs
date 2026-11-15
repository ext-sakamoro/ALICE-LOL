//! # `skadis_v10_panel` — SKADIS の板を繋ぐ connector 2 種を作り、穴の位置を確かめる
//!
//! 板の面は X-Z、厚みは Y (中心が原点)
//!
//! ```bash
//! cargo run --example skadis_v10_panel --release
//! ```

use alice_lol::stdlib::hardsurface::skadis_sdf::{
    skadis_connector_2x2_sdf, skadis_connector_4x4_sdf, SKADIS_CONN_2X2_H, SKADIS_CONN_2X2_R,
    SKADIS_CONN_2X2_SLOT, SKADIS_CONN_2X2_W, SKADIS_CONN_4X4_FILLET, SKADIS_CONN_4X4_REACH,
    SKADIS_PANEL_THICKNESS,
};
use alice_sdf::eval::eval;
use glam::Vec3;

fn main() {
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
