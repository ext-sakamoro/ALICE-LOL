//! shelf divider の天板の抜き穴: Ø15 の丸穴を pitch 20 の三角格子 (行の間 20·√3/2、1 行おきに 10
//! ずらす) に並べ、天板 (Z の厚み) を貫く 穴の縁から天板の縁までは border (15) の肉の枠
//!
//! 書かれた意図: `LLM_REFERENCE.md`「Large Flat Plate Anti-Warp: Hex Cutout Pattern」(Ø15、pitch 20 =
//! 穴の間 5 mm、border 15 = 縁の肉の枠、0.866 = √3/2 の行の間隔)
//!
//! 期待値はこの試験の中で格子の規則から出す (builder の code を使わない)

// 期待値は builder と独立に素の算術で出す: 添字と座標の変換 (cast_*)、fma にしない式
// (suboptimal_flops)、float の走査 (while_float)、座標名は 1 文字 (many_single_char_names)
#![allow(
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::while_float,
    clippy::many_single_char_names
)]

use alice_lol::stdlib::hardsurface::pattern_sdf::{shelf_divider, ShelfDividerSpec};
use alice_sdf::eval::eval;
use glam::Vec3;

const fn spec() -> ShelfDividerSpec {
    ShelfDividerSpec::assembled_560x250x120()
}

/// (中心の一覧、天板の厚みの中央の z)
fn intended() -> (Vec<(f32, f32)>, f32) {
    let s = spec();
    let hx = s.total_width * 0.25; // 片方の U (280 幅) の半分
    let hy = s.depth * 0.5;
    let r = s.hex_hole_radius;
    let (gx, gy) = (hx - s.hex_border - r, hy - s.hex_border - r);
    let p = s.hex_hole_pitch;
    let row = p * 3.0f32.sqrt() * 0.5;
    let mut v = Vec::new();
    for j in -20i32..=20 {
        let y = j as f32 * row;
        if y.abs() > gy {
            continue;
        }
        let off = if j.rem_euclid(2) == 1 { p * 0.5 } else { 0.0 };
        for k in -40i32..=40 {
            let x = (k as f32).mul_add(p, off);
            if x.abs() <= gx {
                v.push((x, y));
            }
        }
    }
    (v, s.thickness * 0.5)
}

#[test]
fn every_lattice_centre_is_a_through_hole() {
    let n = shelf_divider(&spec());
    let (cs, zc) = intended();
    assert!(cs.len() > 100, "compared {}", cs.len());
    let t = spec().thickness;
    let open = cs
        .iter()
        .filter(|&&(x, y)| {
            eval(&n, Vec3::new(x, y, zc)) > 0.0
                && eval(&n, Vec3::new(x, y, 0.1)) > 0.0
                && eval(&n, Vec3::new(x, y, t - 0.1)) > 0.0
        })
        .count();
    assert_eq!(
        open,
        cs.len(),
        "{open}/{} holes open through the plate",
        cs.len()
    );
}

#[test]
fn holes_are_round_15_and_the_wall_between_neighbours_is_5() {
    let n = shelf_divider(&spec());
    let (cs, zc) = intended();
    let mut checked = 0;
    for &(x, y) in &cs {
        // 半径 7.5: 7.3 は空洞、7.7 は肉 (x と斜め 45° の両方)
        for (dx, dy) in [
            (1.0f32, 0.0f32),
            (0.0, 1.0),
            (
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            ),
        ] {
            assert!(
                eval(&n, Vec3::new(x + dx * 7.3, y + dy * 7.3, zc)) > 0.0,
                "hole ({x},{y}) too small"
            );
        }
        // 隣の穴との間 (中心から 10 = 肉の中央) は肉
        for (dx, dy) in [(10.0f32, 0.0f32), (5.0, 8.660_254)] {
            let q = (x + dx, y + dy);
            if cs
                .iter()
                .any(|&(a, b)| (a - q.0 - dx).abs() < 1e-3 && (b - q.1 - dy).abs() < 1e-3)
            {
                assert!(
                    eval(&n, Vec3::new(q.0, q.1, zc)) < -2.4,
                    "wall at {q:?} thinner than 5"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "compared {checked}");
}

#[test]
fn the_part_is_one_280_wide_half_of_the_560_divider() {
    let n = shelf_divider(&spec());
    // 片方の U は幅 280 (X ±140)、奥行 250、高さ 120 側板は X = ±(140 - t/2) の中心
    assert!(
        eval(&n, Vec3::new(139.0, 0.0, 60.0)) < 0.0,
        "side wall missing at x = 139"
    );
    assert!(
        eval(&n, Vec3::new(141.0, 0.0, 60.0)) > 0.0,
        "wider than 280"
    );
    assert!(
        eval(&n, Vec3::new(0.0, 124.0, 2.5)) < 0.0 && eval(&n, Vec3::new(0.0, 126.0, 2.5)) > 0.0
    );
}

#[test]
fn a_15_mm_solid_frame_runs_round_the_plate() {
    let s = spec();
    let n = shelf_divider(&s);
    let (hx, hy) = (s.total_width * 0.25, s.depth * 0.5);
    let zc = s.thickness * 0.5;
    let mut checked = 0;
    // 縁から 0.5〜14.5 mm の帯は全部肉 (1 mm 刻み)
    let mut x = -hx + 0.5;
    while x < hx {
        for d in [0.5f32, 7.0, 14.5] {
            for y in [-hy + d, hy - d] {
                assert!(
                    eval(&n, Vec3::new(x, y, zc)) < 0.0,
                    "frame open at ({x}, {y})"
                );
                checked += 1;
            }
        }
        x += 1.0;
    }
    // 左右の縁 (側板の付く辺) も同じ
    let mut y = -hy + 0.5;
    while y < hy {
        for d in [0.5f32, 7.0, 14.5] {
            for x in [-hx + d, hx - d] {
                assert!(
                    eval(&n, Vec3::new(x, y, zc)) < 0.0,
                    "frame open at ({x}, {y})"
                );
                checked += 1;
            }
        }
        y += 1.0;
    }
    assert!(checked > 0);
}

/// 天板の面の肉の点から、面内の勾配の向きの前後に面まで進んだ距離の和 (= その点を通る肉の幅)
fn wall_through(n: &alice_sdf::SdfNode, p: Vec3) -> Option<f32> {
    let h = 1e-3;
    let g = Vec3::new(
        eval(n, p + Vec3::X * h) - eval(n, p - Vec3::X * h),
        eval(n, p + Vec3::Y * h) - eval(n, p - Vec3::Y * h),
        0.0,
    );
    if g.length_squared() < 1e-12 {
        return None;
    }
    let dir = g.normalize();
    let mut sum = 0.0;
    for s in [1.0f32, -1.0] {
        let mut t = 0.0f32;
        for _ in 0..4000 {
            let d = eval(n, p + dir * (s * t));
            if d >= -1e-4 {
                break;
            }
            t += (-d).max(1e-3);
        }
        sum += t;
    }
    Some(sum)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "runtime only: plate scan × ray march, run in release"
)]
fn thinnest_wall_between_holes_sampled_is_5() {
    let s = spec();
    let n = shelf_divider(&s);
    let (gx, gy) = (
        s.total_width * 0.25 - s.hex_border,
        s.depth * 0.5 - s.hex_border,
    );
    let zc = s.thickness * 0.5;
    let step = 0.5f32;
    let (mut min, mut sampled) = (f32::INFINITY, 0usize);
    let mut x = -gx;
    while x <= gx {
        let mut y = -gy;
        while y <= gy {
            let p = Vec3::new(x, y, zc);
            let d = eval(&n, p);
            if d < -0.05 && d > -2.4 {
                if let Some(w) = wall_through(&n, p) {
                    min = min.min(w);
                    sampled += 1;
                }
            }
            y += step;
        }
        x += step;
    }
    assert!(sampled > 1000, "sampled {sampled}");
    // 中心の間 20 − Ø15 = 5 (隣の穴は全部 20 離れる)
    assert!((min - 5.0).abs() < 0.05, "thinnest wall {min}");
}
