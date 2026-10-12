//! 幾何評価に名前を付ける — `LOL_SEMANTICS_ID` (audit/research law 専用) とは
//! 別軸で、`SdfNode` の評価・受理境界・emit canonical form を pin する.
//!
//! ```text
//! cargo run -p alice-lol --example geometry_id_demo
//! ```

use alice_lol::geometry_id::{
    canon_angle_deg_probe_digest, euler_xyz_quat_probe_digest, fmt_f32_fidelity_probe_digest,
    fold_geometry_pins, rotation_axis_quat_probe_digest, rotation_quat_boundary_probe_digest,
    stdlib_transcendental_probe_digest, tpms_validity_probe_digest, LOL_GEOMETRY_SEMANTICS_ID,
    LOL_GEOMETRY_SEMANTICS_PINS,
};

fn hex(id: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    id.iter().fold(String::with_capacity(64), |mut s, b| {
        // String への write は失敗しない
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn main() {
    println!("=== 幾何評価の算術・受理境界・canonical form ===");
    for (name, id) in LOL_GEOMETRY_SEMANTICS_PINS {
        println!("  pin {name}");
        println!("      {}", hex(id));
    }
    println!(
        "  fold = {}",
        hex(&fold_geometry_pins(LOL_GEOMETRY_SEMANTICS_PINS))
    );
    println!("  宣言 = {}", hex(&LOL_GEOMETRY_SEMANTICS_ID));

    println!("\n=== 今ここで測り直した各 pin の digest ===");
    println!(
        "  alice-det-math                                   = {}",
        hex(&alice_det_math::SEMANTICS_ID)
    );
    println!(
        "  rotate: angle canonicalisation          = {}",
        hex(&canon_angle_deg_probe_digest())
    );
    println!(
        "  rotate: euler-to-quat arithmetic                = {}",
        hex(&euler_xyz_quat_probe_digest())
    );
    println!(
        "  rotate: single-axis quaternion arithmetic        = {}",
        hex(&rotation_axis_quat_probe_digest())
    );
    println!(
        "  rotation quat: unit-length acceptance boundary   = {}",
        hex(&rotation_quat_boundary_probe_digest())
    );
    println!(
        "  tpms: scale/thickness validity range              = {}",
        hex(&tpms_validity_probe_digest())
    );
    println!(
        "  stdlib: transcendental routing (alice_det_math)  = {}",
        hex(&stdlib_transcendental_probe_digest())
    );
    println!(
        "  emit: fmt_f32 round-trip fidelity class          = {}",
        hex(&fmt_f32_fidelity_probe_digest())
    );
    println!("  ⚠️ 上の pin と測り直した digest が揃わなければ、識別子が振る舞いを名乗れていない");

    println!("\n=== LOL_SEMANTICS_ID との独立性 ===");
    println!(
        "  LOL_GEOMETRY_SEMANTICS_ID = {}",
        hex(&LOL_GEOMETRY_SEMANTICS_ID)
    );
    println!(
        "  LOL_SEMANTICS_ID          = {}",
        hex(&alice_lol::law_id::LOL_SEMANTICS_ID)
    );
    println!("  (audit/research law の意味論とは別の値、片方の変更が他方を動かさない)");
}
