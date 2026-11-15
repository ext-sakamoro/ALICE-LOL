//! SKADIS の connector (V10) の形: 閉形式の面積と、穴・溝・角の点の内外
//!
//! 基準は V10 の `generate.py` (印刷の pipeline) の
//! `generate_connector_2x2` / `generate_connector_4x4` (2D の形を厚み 5 で押し出す)

use alice_lol::stdlib::hardsurface::skadis_sdf::{
    skadis_connector_2x2_sdf, skadis_connector_4x4_sdf,
};
use alice_sdf::{eval::eval, SdfNode};
use glam::Vec3;
use std::f32::consts::PI;

const T: f32 = 5.0;
const SCREW_R: f32 = 1.35;

/// 板の中央の面 (y = 0) を `step` の格子で数えた面積 (mm²)
fn sampled_area(node: &SdfNode, half: f32, step: f32) -> (f32, usize) {
    let n = (2.0 * half / step).round() as i32;
    let mut inside = 0usize;
    for i in 0..n {
        for k in 0..n {
            let p = Vec3::new(
                (i as f32 + 0.5).mul_add(step, -half),
                0.0,
                (k as f32 + 0.5).mul_add(step, -half),
            );
            if eval(node, p) < 0.0 {
                inside += 1;
            }
        }
    }
    (inside as f32 * step * step, (n * n) as usize)
}

/// 角を半径 r で丸めた w × h の長方形の面積
fn rounded_rect_area(w: f32, h: f32, r: f32) -> f32 {
    (4.0 - PI).mul_add(-r * r, w * h)
}

#[test]
fn connector_2x2_area_matches_the_closed_form() {
    let node = skadis_connector_2x2_sdf(T);
    let want = rounded_rect_area(50.0, 24.0, 2.0)
        - 4.0 * PI * SCREW_R * SCREW_R
        - rounded_rect_area(25.0, 1.5, 0.5);
    let (got, n) = sampled_area(&node, 26.0, 0.05);
    assert!(n > 0, "compared nothing");
    // 境界の cell の分 (周長 × step / 2 程度) を許す: 周長 ~ 150 + 4 穴 34 + 溝 52 → 0.05 × 240 / 2 = 6
    assert!(
        (got - want).abs() < 6.0,
        "2x2 area {got} vs closed form {want}"
    );
}

#[test]
fn connector_4x4_area_matches_the_closed_form() {
    let node = skadis_connector_4x4_sdf(T);
    let want = 24.0f32.mul_add(24.0, -PI * 2.0 * 2.0) - 4.0 * PI * SCREW_R * SCREW_R;
    let (got, n) = sampled_area(&node, 13.0, 0.05);
    assert!(n > 0, "compared nothing");
    assert!(
        (got - want).abs() < 4.0,
        "4x4 area {got} vs closed form {want}"
    );
}

#[test]
fn connector_holes_slot_and_corners_are_where_generate_py_puts_them() {
    let c2 = skadis_connector_2x2_sdf(T);
    let c4 = skadis_connector_4x4_sdf(T);
    let mut checked = 0;
    // 穴の中心は空洞、穴の縁の外 0.3 mm は肉
    for (node, holes) in [
        (
            &c2,
            vec![(-20.0f32, -6.0f32), (-20.0, 6.0), (20.0, -6.0), (20.0, 6.0)],
        ),
        (
            &c4,
            vec![(-6.0, -6.0), (-6.0, 6.0), (6.0, -6.0), (6.0, 6.0)],
        ),
    ] {
        for (x, z) in holes {
            assert!(
                eval(node, Vec3::new(x, 0.0, z)) > 0.0,
                "hole ({x},{z}) not open"
            );
            assert!(
                eval(node, Vec3::new(x + SCREW_R + 0.3, 0.0, z)) < 0.0,
                "hole ({x},{z}) too wide"
            );
            checked += 2;
        }
    }
    // 2x2 の溝: 中心と端 (±12.3) は空洞、溝の外 (z = 1.0) は肉
    for x in [0.0f32, -12.3, 12.3] {
        assert!(
            eval(&c2, Vec3::new(x, 0.0, 0.0)) > 0.0,
            "slot at x={x} not open"
        );
        checked += 1;
    }
    assert!(eval(&c2, Vec3::new(0.0, 0.0, 1.0)) < 0.0, "slot too tall");
    assert!(eval(&c2, Vec3::new(13.0, 0.0, 0.0)) < 0.0, "slot too long");
    // 2x2 の角 R2: 角の点 (24.8, 11.8) は外、(23.5, 10.5) は内
    assert!(
        eval(&c2, Vec3::new(24.8, 0.0, 11.8)) > 0.0,
        "2x2 corner not rounded"
    );
    assert!(eval(&c2, Vec3::new(23.5, 0.0, 10.5)) < 0.0);
    // 4x4: 角の R2 の切欠き (11.5, 11.5) は外、(10.5, 9.0) は内、辺の中央 (11.9, 0) は内
    assert!(
        eval(&c4, Vec3::new(11.5, 0.0, 11.5)) > 0.0,
        "4x4 corner notch missing"
    );
    assert!(eval(&c4, Vec3::new(10.5, 0.0, 9.0)) < 0.0);
    assert!(eval(&c4, Vec3::new(11.9, 0.0, 0.0)) < 0.0);
    // 厚み: y = ±2.4 は肉、±2.6 は外
    for node in [&c2, &c4] {
        assert!(eval(node, Vec3::new(0.0, 2.4, 9.0)) < 0.0);
        assert!(eval(node, Vec3::new(0.0, 2.6, 9.0)) > 0.0);
    }
    checked += 10;
    assert!(checked > 0, "compared nothing");
}
