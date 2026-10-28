//! A material residual's cell map: cells given by a table instead of the law's procedural cells
//!
//! Domains `rows <R>` and `stones <N>`; block `row_bounds` (R × 2: v0, v1, row r holds
//! v0 <= v < v1) and `stones` (N × 7: row, u0, u1, cell0, cell1, cell2, corner radius). A point (its
//! lookup place, after the warp, reduced into the tile) is in the stone of its row whose
//! [u0, u1) holds it (u0 < 0 or u1 > tu wraps around the tile) and inside the stone's rectangle
//! [u0, u1] × [v0, v1] with its corners rounded by the radius; there `edge` is the distance to that
//! outline and the cell numbers are the stone's. Anywhere else (a joint) `edge` and the numbers
//! are 0. Checked by hand with a 2-row table on a 2 x 1 tile.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use alice_lol::law_residual::{bound_hash, LawResidual, ResidualOwner};
use alice_lol::material_law::{MaterialLaw, MATERIAL_LAW_KIND_STR};
use alice_sdf::surface::hash_noise_2d_periodic;
use sha2::{Digest, Sha256};

const LAW: &str = "law stones
kind material
tile = (2, 1)
cell grid = (0.5, 0.5, 0)
noise wu = (4, 0.03, 3)
noise wv = (4, 0.03, 4)
WARP
normal_step = 0.001
field height = edge
field albedo = (cell0, cell1, cell2)
field roughness = 0.5
";

/// (row, u0, u1, cell0, cell1, cell2, radius)
const STONES: [[f64; 7]; 5] = [
    [0.0, -0.2, 0.7, 0.1, 0.2, 0.3, 0.0], // crosses the tile's left edge: [1.8, 2) and [0, 0.7)
    [0.0, 0.7, 1.4, 0.4, 0.5, 0.6, 0.1],  // rounded corners
    [0.0, 1.4, 1.8, 0.7, 0.8, 0.9, 0.0],
    [1.0, 0.0, 0.9, 0.15, 0.25, 0.35, 0.0],
    [1.0, 1.0, 2.0, 0.45, 0.55, 0.65, 0.0], // a joint before it: [0.9, 1.0)
];
const ROWS: [[f64; 2]; 2] = [[0.0, 0.5], [0.5, 1.0]];

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn law(warp: bool) -> MaterialLaw {
    MaterialLaw::from_file(&LAW.replace("WARP", if warp { "warp = (wu, wv)" } else { "" })).unwrap()
}

fn residual(law: &MaterialLaw) -> LawResidual {
    let b = law.bind(&[]).unwrap();
    let bound = bound_hash(&b.canonical_bound(&[]));
    let rows: Vec<u8> = ROWS
        .iter()
        .flatten()
        .flat_map(|x| x.to_le_bytes())
        .collect();
    let stones: Vec<u8> = STONES
        .iter()
        .flatten()
        .flat_map(|x| x.to_le_bytes())
        .collect();
    let mut h = format!(
        "residual stones_fit\nfor {MATERIAL_LAW_KIND_STR} {}\nbound {}\nframe world\n\
         index rows 2\nindex stones 5\nbetween rows table\nbetween stones table\n",
        hex(&law.law_id()),
        hex(&bound)
    );
    h += &format!(
        "block row_bounds f64 rows 2 2 {}\n",
        hex(&Sha256::digest(&rows))
    );
    h += &format!(
        "block stones f64 stones 5 7 {}\n",
        hex(&Sha256::digest(&stones))
    );
    h += "payload\n";
    let mut out = h.into_bytes();
    out.extend(rows);
    out.extend(stones);
    LawResidual::from_bytes(&out).unwrap()
}

/// (edge, [cell0, cell1, cell2]) at the plain law with the map, where the fields read them
fn at(u: f64, v: f64) -> (f64, [f64; 3]) {
    let l = law(false);
    let b = l.bind(&[]).unwrap();
    let r = residual(&l);
    let s = b.with_residual(&r).unwrap().at(u, v).unwrap();
    (s.height, s.albedo)
}

#[test]
fn inside_a_stone_the_edge_is_the_distance_to_its_outline() {
    // the centre of stone 1 ([0.7, 1.4] x [0, 0.5], radius 0.1): 0.25 to the v edges
    let (e, c) = at(1.05, 0.25);
    assert!((e - 0.25).abs() < 1e-12, "{e}");
    assert_eq!(c, [0.4, 0.5, 0.6]);
    // near stone 3's left edge: 0.05 from u = 0, 0.2 from the v edges
    let (e, c) = at(0.05, 0.7);
    assert!((e - 0.05).abs() < 1e-12, "{e}");
    assert_eq!(c, [0.15, 0.25, 0.35]);
    // either side of the row boundary v = 0.5: row 0's stone 2 below, row 1's stone 4 above (a
    // point exactly on it is on both stones' outlines, distance 0: a joint)
    assert_eq!(at(1.5, 0.5 - 1e-9).1, [0.7, 0.8, 0.9]);
    assert_eq!(at(1.5, 0.5 + 1e-9).1, [0.45, 0.55, 0.65]);
    assert_eq!(at(1.5, 0.5), (0.0, [0.0; 3]));
}

#[test]
fn rounded_corners_measure_to_the_arc() {
    // stone 1's lower-left corner region: the arc centre is (0.8, 0.1), radius 0.1
    // (0.75, 0.05) is 0.05·sqrt(2) from the arc centre, so 0.1 - 0.0707.. inside
    let (e, c) = at(0.75, 0.05);
    assert!((e - (0.1 - 0.05f64.hypot(0.05))).abs() < 1e-12, "{e}");
    assert_eq!(c, [0.4, 0.5, 0.6]);
    // (0.71, 0.01) is inside the rectangle but outside the rounded corner: a joint
    assert_eq!(at(0.71, 0.01), (0.0, [0.0; 3]));
    // stone 0 has no rounding: its corner point region is still inside
    let (e, _) = at(0.69, 0.01);
    assert!((e - 0.01).abs() < 1e-12, "{e}");
}

#[test]
fn a_stone_may_cross_the_tile_edge() {
    // stone 0 is [-0.2, 0.7]: at u = 1.9 (= -0.1 wrapped) it is 0.1 from its left end
    let (e, c) = at(1.9, 0.25);
    assert!((e - 0.1).abs() < 1e-12, "{e}");
    assert_eq!(c, [0.1, 0.2, 0.3]);
    // and the same place one tile over
    assert_eq!(at(1.9 - 2.0, 0.25 + 1.0), at(1.9, 0.25));
}

#[test]
fn between_stones_is_a_joint() {
    assert_eq!(at(0.95, 0.75), (0.0, [0.0; 3]));
}

#[test]
fn the_map_is_looked_up_after_the_warp() {
    // the stone of a point is the one holding (u + wu, v + wv), the warp's noises at the point
    let l = law(true);
    let b = l.bind(&[]).unwrap();
    let r = residual(&l);
    let with = b.with_residual(&r).unwrap();
    let noise = |u: f64, v: f64, seed: u32| {
        0.03 * f64::from(hash_noise_2d_periodic(
            (u * 4.0) as f32,
            (v * 4.0) as f32,
            [8, 4],
            seed,
        ))
    };
    // the stone holding a place, by the table (no rounding needed away from corners)
    let stone_at = |u: f64, v: f64| -> Option<usize> {
        let (u, v) = (u - 2.0 * (u / 2.0).floor(), v - (v).floor());
        let row = ROWS.iter().position(|r| r[0] <= v && v < r[1])?;
        STONES.iter().position(|s| {
            s[0] as usize == row && {
                let du = (u - s[1]) - 2.0 * ((u - s[1]) / 2.0).floor();
                du < s[2] - s[1]
            }
        })
    };
    let mut moved = 0;
    for k in 0..40 {
        // points just either side of stone boundaries at u = 0.7 and 1.4, mid-row
        let u = [0.69, 0.71, 1.39, 1.41][k % 4] + (k / 4) as f64 * 1e-3;
        let v = 0.25;
        let (cu, cv) = (u + noise(u, v, 3), v + noise(u, v, 4));
        let want =
            stone_at(cu, cv).map_or([0.0; 3], |i| [STONES[i][3], STONES[i][4], STONES[i][5]]);
        let got = with.at(u, v).unwrap().albedo;
        if stone_at(cu, cv) != stone_at(u, v) {
            moved += 1;
        }
        assert_eq!(got, want, "({u}, {v}) -> ({cu}, {cv})");
    }
    assert!(
        moved > 0,
        "the warp moved no point across a stone boundary: the check has no teeth"
    );
}

#[test]
fn a_map_that_does_not_lay_out_is_refused() {
    let l = law(false);
    let b = l.bind(&[]).unwrap();
    let r = residual(&l);
    assert!(b.with_residual(&r).is_ok());
    // rebuild with one bad stone each time: a radius past half the stone, a row past the rows,
    // u1 before u0, overlapping stones in a row
    let bad_tables: [[[f64; 7]; 5]; 4] = {
        let mut t = [STONES; 4];
        t[0][1][6] = 0.3; // stone 1 is 0.5 tall: radius past 0.25
        t[1][2][0] = 2.0; // row 2 does not exist
        t[2][2][2] = 1.3; // u1 < u0
        t[3][2][1] = 1.3; // overlaps stone 1 ([0.7, 1.4))
        t
    };
    for (k, table) in bad_tables.iter().enumerate() {
        let bound = bound_hash(&b.canonical_bound(&[]));
        let rows: Vec<u8> = ROWS
            .iter()
            .flatten()
            .flat_map(|x| x.to_le_bytes())
            .collect();
        let stones: Vec<u8> = table
            .iter()
            .flatten()
            .flat_map(|x| x.to_le_bytes())
            .collect();
        let mut h = format!(
            "residual s\nfor {MATERIAL_LAW_KIND_STR} {}\nbound {}\nframe world\nindex rows 2\nindex stones 5\nbetween rows table\nbetween stones table\n",
            hex(&l.law_id()),
            hex(&bound)
        );
        h += &format!(
            "block row_bounds f64 rows 2 2 {}\n",
            hex(&Sha256::digest(&rows))
        );
        h += &format!(
            "block stones f64 stones 5 7 {}\npayload\n",
            hex(&Sha256::digest(&stones))
        );
        let mut out = h.into_bytes();
        out.extend(rows);
        out.extend(stones);
        let bad = LawResidual::from_bytes(&out).unwrap();
        assert!(b.with_residual(&bad).is_err(), "bad table {k} accepted");
    }
}
