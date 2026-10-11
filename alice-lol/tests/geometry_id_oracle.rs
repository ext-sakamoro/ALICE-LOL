//! `LOL_GEOMETRY_SEMANTICS_ID` の識別子が「幾何評価の振る舞い」から drift
//! しないことを固定する.
//!
//! ⚠️ `law_id_oracle.rs` と同じ規律: 各 pin は **振る舞いから計算**した digest
//! であり、この test が pin 関数を再実行して `LOL_GEOMETRY_SEMANTICS_PINS` に
//! 埋め込んだ literal と突合する 不一致は「振る舞いが変わったのに、この
//! 定数を再記録し忘れた」ことを意味する (同じ commit で直す)

use alice_lol::geometry_id::{
    canon_angle_deg_probe_digest, euler_xyz_quat_probe_digest, fmt_f32_fidelity_probe_digest,
    fold_geometry_pins, rotation_axis_quat_probe_digest, rotation_quat_boundary_probe_digest,
    tpms_validity_probe_digest, LOL_GEOMETRY_SEMANTICS_ID, LOL_GEOMETRY_SEMANTICS_PINS,
};

fn pin(name: &str) -> [u8; 32] {
    LOL_GEOMETRY_SEMANTICS_PINS
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("no pin named {name:?} in LOL_GEOMETRY_SEMANTICS_PINS"))
        .1
}

#[test]
fn every_pin_matches_its_live_behaviour_digest() {
    let cases: &[(&str, [u8; 32])] = &[
        ("alice-det-math", alice_det_math::SEMANTICS_ID),
        (
            "rotate: angle canonicalisation",
            canon_angle_deg_probe_digest(),
        ),
        (
            "rotate: euler-to-quat arithmetic",
            euler_xyz_quat_probe_digest(),
        ),
        (
            "rotate: single-axis quaternion arithmetic",
            rotation_axis_quat_probe_digest(),
        ),
        (
            "rotation quat: unit-length acceptance boundary",
            rotation_quat_boundary_probe_digest(),
        ),
        (
            "tpms: scale/thickness validity range",
            tpms_validity_probe_digest(),
        ),
        (
            "emit: fmt_f32 round-trip fidelity class",
            fmt_f32_fidelity_probe_digest(),
        ),
    ];
    // ⚠️ 比較件数 0 件 fail (空振り防止、検査器自身の空振りを防ぐ既存の規律と同軸)
    assert_eq!(
        cases.len(),
        LOL_GEOMETRY_SEMANTICS_PINS.len(),
        "pin の数と検査した件数が一致しない (新しい pin を足したらここにも足す)"
    );
    let mut checked = 0;
    for (name, live) in cases {
        assert_eq!(
            *live,
            pin(name),
            "{name}: 振る舞いの digest が LOL_GEOMETRY_SEMANTICS_PINS の literal と不一致 \
             (この pin を再記録し忘れている)"
        );
        checked += 1;
    }
    assert!(checked > 0, "比較件数が 0 件 (空振り防止)");
}

#[test]
fn the_pins_fold_to_the_exported_identifier() {
    assert_eq!(
        fold_geometry_pins(LOL_GEOMETRY_SEMANTICS_PINS),
        LOL_GEOMETRY_SEMANTICS_ID,
        "LOL_GEOMETRY_SEMANTICS_ID が LOL_GEOMETRY_SEMANTICS_PINS の fold と不一致"
    );
}

#[test]
fn the_identifier_is_independent_of_lol_semantics_id() {
    // この識別子は audit/research law 専用の LOL_SEMANTICS_ID と別軸 (module doc の
    // scope 節) 2 つの定数が偶然にも同じ値になっていないことを確かめる (もし一致
    // していたら、どちらかが他方を参照しているだけで独立に計算されていない疑いがある)
    assert_ne!(
        alice_lol::geometry_id::LOL_GEOMETRY_SEMANTICS_ID,
        alice_lol::law_id::LOL_SEMANTICS_ID,
        "LOL_GEOMETRY_SEMANTICS_ID と LOL_SEMANTICS_ID が同じ値 (独立性が壊れている疑い)"
    );
}

#[test]
fn a_different_probe_set_gets_a_different_digest() {
    // pin 関数が実際に入力に依存していること (定数を返しているだけではないこと) を
    // 確かめる 既存の probe digest と、既知で異なる値 (単位 quaternion と明確な
    // 非単位 quaternion の 2 件だけの縮小 probe) の fold が一致しないことを見る
    use alice_lol::limits::checked_rotation_quat;
    let reduced: [u8; 32] = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for (x, y, z, w) in [(0.0_f32, 0.0, 0.0, 1.0), (2.0, 0.0, 0.0, 0.0)] {
            match checked_rotation_quat(x, y, z, w) {
                Ok(q) => {
                    h.update([1u8]);
                    h.update(q.x.to_bits().to_le_bytes());
                    h.update(q.y.to_bits().to_le_bytes());
                    h.update(q.z.to_bits().to_le_bytes());
                    h.update(q.w.to_bits().to_le_bytes());
                }
                Err(_) => h.update([0u8]),
            }
        }
        h.finalize().into()
    };
    assert_ne!(
        reduced,
        rotation_quat_boundary_probe_digest(),
        "縮小した probe set が元の digest と同じ (pin が probe set に依存していない疑い)"
    );
}
