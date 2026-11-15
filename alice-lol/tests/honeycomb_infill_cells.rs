//! honeycomb_infill: 文書どおり、X-Z の平面で軸ごとに 2·count + 1 個の 6 角形の cell を
//! (X に cell_size·√3、Z に cell_size·1.5 の間隔で原点中心に) 並べ、板の厚み (Y) の方向に抜く
//!
//! 期待値はこの試験の中で cell の規則から出す (builder の code を使わない)

use alice_lol::stdlib::hardsurface::reinforcement::honeycomb_infill;
use alice_sdf::{eval::eval, SdfNode};
use glam::Vec3;

const CELL: f32 = 8.0;
const WALL: f32 = 1.2;
const COUNT: i32 = 3;
const HALF: f32 = 60.0; // 板は X・Z に ±60、厚み Y に ±5

fn plate() -> SdfNode {
    SdfNode::Box3d {
        half_extents: Vec3::new(HALF, 5.0, HALF),
    }
}

fn centres() -> Vec<(f32, f32)> {
    let (sx, sz) = (CELL * 3.0f32.sqrt(), CELL * 1.5);
    let mut v = Vec::new();
    for k in -COUNT..=COUNT {
        for l in -COUNT..=COUNT {
            v.push((k as f32 * sx, l as f32 * sz));
        }
    }
    v
}

#[test]
fn every_cell_of_the_2c_plus_1_grid_is_cut_through_the_thickness() {
    let n = honeycomb_infill(plate(), CELL, WALL, COUNT as u32);
    let cs = centres();
    assert_eq!(cs.len(), 49, "7 x 7 cells");
    for &(x, z) in &cs {
        for y in [-4.9f32, 0.0, 4.9] {
            assert!(
                eval(&n, Vec3::new(x, y, z)) > 0.0,
                "cell ({x}, {z}) not open at y = {y}"
            );
        }
    }
}

#[test]
fn material_stays_between_cells_and_outside_the_grid() {
    let n = honeycomb_infill(plate(), CELL, WALL, COUNT as u32);
    let (sx, sz) = (CELL * 3.0f32.sqrt(), CELL * 1.5);
    let mut checked = 0;
    // 隣り合う cell の中点 (X 方向、Z 方向) は肉
    for k in -COUNT..COUNT {
        for l in -COUNT..=COUNT {
            let p = Vec3::new((k as f32 + 0.5) * sx, 0.0, l as f32 * sz);
            assert!(eval(&n, p) < 0.0, "no wall between cells at {p}");
            checked += 1;
        }
    }
    for k in -COUNT..=COUNT {
        for l in -COUNT..COUNT {
            let p = Vec3::new(k as f32 * sx, 0.0, (l as f32 + 0.5) * sz);
            assert!(eval(&n, p) < 0.0, "no wall between cells at {p}");
            checked += 1;
        }
    }
    // 格子の外 (count + 1 番目の cell の位置) は肉
    for (x, z) in [
        ((COUNT + 1) as f32 * sx, 0.0),
        (0.0, (COUNT + 1) as f32 * sz),
    ] {
        assert!(
            eval(&n, Vec3::new(x, 0.0, z)) < 0.0,
            "cell outside the 2c+1 grid at ({x}, {z})"
        );
        checked += 1;
    }
    assert!(checked > 0);
}
