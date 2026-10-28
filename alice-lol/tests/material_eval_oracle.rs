//! Evaluating a material law on a surface: `BoundMaterial::at(u, v)` gives the fields at a point
//! from the law's own cells and noises (ALICE-SDF's surface parts), `normal_at` the normal
//!
//! The contract, written out here by hand from the parts' owner (the expected values below are
//! assembled from `alice_sdf::surface` directly and the law's text, not from `at`):
//! - the point is reduced into the tile, `u' = u - tu·floor(u / tu)` (and v), before any lookup;
//! - each noise `(f, amp, seed)` is `amp · hash_noise_2d_periodic(u'·f, v'·f, [tu·f, tv·f], seed)`
//!   (its lattice wraps once per tile);
//! - the cell is looked up at the reduced point moved by the warp's noises, `(u' + a, v' + b)`
//!   (no warp: `(u', v')`), with the tile's cell counts and seed `MATERIAL_CELL_SEED`;
//! - the fields read the point's own `u`, `v`, the cell's edge and numbers, and the noises;
//! - the normal is the central difference of `height` at `normal_step`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use alice_lol::material_law::{MaterialError, MaterialLaw, MaterialPoint, MATERIAL_CELL_SEED};
use alice_sdf::surface::{grid_cell, hash_noise_2d_periodic, voronoi_cell};

const STONES: &str = "law stones
kind material
param size number = 0.25 range 0.125 0.5
tile = (1, 1)
cell grid = (size, size, 0.5)
noise wu = (8, 0.01, 3)
noise wv = (8, 0.01, 4)
noise grain = (16, 0.002, 7)
WARP
normal_step = 0.001
field height = min(edge / 0.004, 1) * 0.004 + grain
field albedo = (0.2 + 0.1 * cell0, 0.3, 0.2 + 0.1 * cell2)
field roughness = 0.5 + 0.2 * cell1 + 10 * wu
";

fn stones(warp: &str) -> MaterialLaw {
    MaterialLaw::from_file(&STONES.replace("WARP", warp)).unwrap()
}

/// the noise of the contract, by hand
fn noise(u: f64, v: f64, f: f64, amp: f64, seed: u32, tile: [f64; 2]) -> f64 {
    let period = [(tile[0] * f).round() as u32, (tile[1] * f).round() as u32];
    amp * f64::from(hash_noise_2d_periodic(
        (u * f) as f32,
        (v * f) as f32,
        period,
        seed,
    ))
}

fn reduce(x: f64, t: f64) -> f64 {
    x - t * (x / t).floor()
}

/// the point of the contract for STONES (tile 1 x 1, grid of 0.25 with a half shift, 4 x 4 cells)
fn stones_point(u: f64, v: f64, warped: bool) -> MaterialPoint {
    let (ur, vr) = (reduce(u, 1.0), reduce(v, 1.0));
    let n = [
        noise(ur, vr, 8.0, 0.01, 3, [1.0, 1.0]),
        noise(ur, vr, 8.0, 0.01, 4, [1.0, 1.0]),
        noise(ur, vr, 16.0, 0.002, 7, [1.0, 1.0]),
    ];
    let (cu, cv) = if warped {
        (ur + n[0], vr + n[1])
    } else {
        (ur, vr)
    };
    let c = grid_cell(cu, cv, [0.25, 0.25], 0.5, [4, 4], MATERIAL_CELL_SEED);
    MaterialPoint {
        u,
        v,
        edge: c.edge,
        cell: c.numbers,
        noise: n.to_vec(),
    }
}

const POINTS: [(f64, f64); 6] = [
    (0.1, 0.2),
    (0.37, 0.81),
    (0.124, 0.126),
    (0.5, 0.5),
    (0.999, 0.001),
    (0.66, 0.33),
];

#[test]
fn the_fields_at_a_point_follow_the_contract() {
    for (warp, warped) in [("", false), ("warp = (wu, wv)", true)] {
        let law = stones(warp);
        let b = law.bind(&[]).unwrap();
        for (u, v) in POINTS {
            let want = b.sample(&stones_point(u, v, warped)).unwrap();
            assert_eq!(b.at(u, v).unwrap(), want, "warp `{warp}` at ({u}, {v})");
        }
    }
}

#[test]
fn the_warp_moves_the_cells_not_the_point_the_fields_read() {
    let b_law = stones("warp = (wu, wv)");
    let b = b_law.bind(&[]).unwrap();
    let plain_law = stones("");
    let plain = plain_law.bind(&[]).unwrap();
    // somewhere near a cell edge the warp must put the point in another cell or at another
    // distance from the edge (else the warp has no effect at all)
    let moved = (0..64)
        .map(|k| (f64::from(k) + 0.5) / 64.0)
        .filter(|&u| b.at(u, 0.3).unwrap() != plain.at(u, 0.3).unwrap())
        .count();
    assert!(moved > 0, "the warp moved no cell along v = 0.3");
    // a field that reads u and v: with a warp it still reads the point's own u, v (the contract's
    // point keeps them), not the moved place of the cell lookup
    let text = STONES.replace("WARP", "warp = (wu, wv)").replace(
        "0.5 + 0.2 * cell1 + 10 * wu",
        "0.4 + 0.2 * cell1 + 0.2 * u + 0.1 * v",
    );
    let reads_law = MaterialLaw::from_file(&text).unwrap();
    let reads = reads_law.bind(&[]).unwrap();
    for (u, v) in POINTS {
        let want = reads.sample(&stones_point(u, v, true)).unwrap();
        assert_eq!(reads.at(u, v).unwrap(), want, "({u}, {v})");
    }
}

#[test]
fn a_warped_pattern_repeats_with_the_tile() {
    // dyadic points: u + 1 and its reduction are exact, so the two evaluations are bit-equal
    for warp in ["", "warp = (wu, wv)", "warp = (wv, grain)"] {
        let b_law = stones(warp);
        let b = b_law.bind(&[]).unwrap();
        for k in 0..16 {
            let u = f64::from(k) / 16.0 + 1.0 / 128.0;
            let v = f64::from(15 - k) / 16.0 + 3.0 / 256.0;
            let here = b.at(u, v).unwrap();
            // far tiles too: the point is reduced into the tile before the f32 noise lattice, which
            // would otherwise lose the fraction (16 x 4096 already needs 17 bits of the 24)
            for (du, dv) in [
                (1.0, 0.0),
                (0.0, 1.0),
                (-1.0, 2.0),
                (3.0, -1.0),
                (4096.0, -65536.0),
            ] {
                let there = b.at(u + du, v + dv).unwrap();
                // roughness and albedo read no u, v: the whole sample repeats
                assert_eq!(there, here, "warp `{warp}` at ({u}, {v}) + ({du}, {dv})");
            }
        }
    }
}

#[test]
fn voronoi_cells_follow_the_contract() {
    let text = "law cobbles
kind material
tile = (1, 1)
cell voronoi = (0.125, 0.8)
noise wu = (8, 0.01, 3)
noise wv = (8, 0.01, 4)
warp = (wu, wv)
normal_step = 0.001
field height = min(edge / 0.004, 1) * 0.004
field albedo = (0.2 + 0.1 * cell0, 0.3, 0.3)
field roughness = 0.6
";
    let b_law = MaterialLaw::from_file(text).unwrap();
    let b = b_law.bind(&[]).unwrap();
    for (u, v) in POINTS {
        let n = [
            noise(u, v, 8.0, 0.01, 3, [1.0, 1.0]),
            noise(u, v, 8.0, 0.01, 4, [1.0, 1.0]),
        ];
        let c = voronoi_cell(u + n[0], v + n[1], 0.125, 0.8, [8, 8], MATERIAL_CELL_SEED);
        let want = b
            .sample(&MaterialPoint {
                u,
                v,
                edge: c.edge,
                cell: c.numbers,
                noise: n.to_vec(),
            })
            .unwrap();
        assert_eq!(b.at(u, v).unwrap(), want, "({u}, {v})");
    }
}

#[test]
fn the_normal_is_the_central_difference_of_the_height() {
    // a height of u and v alone: h = 0.01 sin(2 pi u) + 0.005 cos(2 pi v), gradient by hand
    let text = "law waves
kind material
tile = (1, 1)
normal_step = 0.0001
field height = 0.01 * sin(2 * pi * u) + 0.005 * cos(2 * pi * v)
field albedo = (0.5, 0.5, 0.5)
field roughness = 0.5
";
    let b_law = MaterialLaw::from_file(text).unwrap();
    let b = b_law.bind(&[]).unwrap();
    let tau = std::f64::consts::TAU;
    for (u, v) in POINTS {
        let (hu, hv) = (0.01 * tau * (tau * u).cos(), -0.005 * tau * (tau * v).sin());
        let len = (hu * hu + hv * hv + 1.0).sqrt();
        let want = [-hu / len, -hv / len, 1.0 / len];
        let got = b.normal_at(u, v).unwrap();
        // the central difference's error: h''' step^2 / 6, at most 0.01 (2 pi)^3 1e-8 / 6 < 5e-9
        for k in 0..3 {
            assert!(
                (got[k] - want[k]).abs() < 1e-8,
                "({u}, {v}) [{k}] {got:?} vs {want:?}"
            );
        }
        let n = (got[0] * got[0] + got[1] * got[1] + got[2] * got[2]).sqrt();
        assert!((n - 1.0).abs() < 1e-12, "a unit normal");
    }
}

#[test]
fn a_point_that_is_not_finite_is_refused() {
    let b_law = stones("warp = (wu, wv)");
    let b = b_law.bind(&[]).unwrap();
    for (u, v) in [
        (f64::NAN, 0.0),
        (0.0, f64::INFINITY),
        (f64::NEG_INFINITY, 1.0),
    ] {
        assert!(
            matches!(b.at(u, v), Err(MaterialError::NonFinite { field }) if field == "u" || field == "v"),
            "({u}, {v})"
        );
        assert!(matches!(
            b.normal_at(u, v),
            Err(MaterialError::NonFinite { .. })
        ));
    }
}
