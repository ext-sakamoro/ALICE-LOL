//! Real-size wall thickness of the stdlib containers and plates at the bundled template arguments
//!
//! `rounded_box(hx, hy, hz, r)` takes the outer half-extents; an outer shell rounded with a radius
//! larger than its wall thins the rim and the corners (the arc's centre falls inside the cavity
//! corner: the corner wall is `r - √2 (r - wall)`), and a thin plate rounded on every edge becomes a
//! knife edge. Each shape here is meshed over its whole bounding box and every triangle's local
//! thickness is measured along `-∇d`, read just below the surface; the thinnest must reach the wall
//! the spec gives (`runtime_parser` defaults), so a fix that is reverted turns its row red.
//!
//! Families and the fix each row depends on:
//! - open shells (cavity open at +Y): `open_shell` (flat rim and floor, vertical edges rounded with
//!   `r <= wall`)
//! - blocks and racks whose wells, holes or slots open through the top: `rounded_plate` on Y (only the
//!   vertical edges rounded) — a rounded top edge eats the wall between the opening and the side
//! - plates: `rounded_plate` (only the edges along the plate's thickness rounded)
//! - mount holes through Z-thin plates: `cylinder_z` (`caliper_holder`, `wall_hook`)
//! - `esp32_enclosure`: the USB opening is cut through the rim when the lip above it would be thinner
//!   than the wall
//! - `pixhawk_mount`: the damper pockets sit where the wall to the plate edge equals the wall to the M3
//!   hole

// a / b / c are a triangle's corners and n the normal, as in the validator's own code
#![allow(clippy::many_single_char_names)]

use alice_lol::runtime_parser::parse_lol;
use alice_sdf::eval::{eval, eval_normal};
use alice_sdf::mesh::{sdf_to_mesh, MarchingCubesConfig};
use alice_sdf::tight_aabb::{analytic_aabb, AnalyticAabb};
use alice_sdf::validity::local_thickness;
use alice_sdf::SdfNode;
use glam::Vec3;

/// The thinnest local wall of `node` over its analytic bounding box (mesh resolution `res`)
fn thinnest_wall(node: &SdfNode, res: usize) -> f32 {
    let AnalyticAabb::Bounded(b) = analytic_aabb(node) else {
        panic!("no analytic bound for the shape (the oracle compared nothing)");
    };
    let pad = Vec3::splat(1.0);
    let (lo, hi) = (b.min - pad, b.max + pad);
    let m = sdf_to_mesh(
        node,
        lo,
        hi,
        &MarchingCubesConfig {
            resolution: res,
            ..MarchingCubesConfig::default()
        },
    );
    let span = (hi - lo).length();
    let pv = |i: u32| m.vertices[i as usize].position;
    let mut min = f32::INFINITY;
    let mut measured = 0usize;
    // every third triangle: a thin region spans hundreds of triangles, so the sample still lands in it
    for t in m.indices.chunks(3).step_by(3) {
        let (a, b, c) = (pv(t[0]), pv(t[1]), pv(t[2]));
        if (b - a).cross(c - a).length_squared() <= 0.0 {
            continue;
        }
        let cen = (a + b + c) / 3.0;
        let inside = cen - eval_normal(node, cen) * (eval(node, cen) + 1e-3);
        let n = eval_normal(node, inside);
        let origin = inside + n * (1e-4 - eval(node, inside));
        if let Some(th) = local_thickness(node, origin, -n, span) {
            min = min.min(th);
            measured += 1;
        }
    }
    assert!(measured > 50, "only {measured} triangles measured");
    min
}

/// (LOL text of a bundled template, the wall the spec gives, where the value comes from)
const ROWS: &[(&str, f32, &str)] = &[
    ("esp32_enclosure(45, 18, 12)", 1.6, "wall_thickness 1.6"),
    ("esp32_enclosure(68.6, 53.4, 20)", 1.6, "wall_thickness 1.6"),
    (
        "sbc_case(65, 30, 15)",
        1.0,
        "port opening lip (pre-existing feature, wall_thickness 2.0)",
    ),
    (
        "outdoor_enclosure(100, 70, 35)",
        0.75,
        "gasket groove floor (pre-existing feature)",
    ),
    (
        "soap_tray(200, 90, 6)",
        2.0,
        "floor_thickness 2.0 (drain slots through the floor)",
    ),
    ("pill_organizer(7, 2, 20)", 1.5, "wall_thickness 1.5"),
    ("sd_card_holder(5, 5, 15)", 1.5, "wall_thickness 1.5"),
    ("makeup_organizer(4, 6, 30)", 2.0, "wall_thickness 2.0"),
    (
        "card_tray(44, 68, 25)",
        0.6,
        "finger notch edge (pre-existing feature, wall_thickness 2.0)",
    ),
    ("swatch_holder(8, 5, 24)", 2.0, "wall_thickness 2.0"),
    ("wrench_holder(8, 24, 8)", 3.0, "wall_thickness 3.0"),
    ("sink_caddy(200, 100, 8)", 2.5, "wall_thickness 2.5"),
    ("hairdryer_holder(60, 100, 3)", 3.0, "wall_thickness 3.0"),
    ("drill_bit_holder(1, 10, 19)", 3.0, "wall_thickness 3.0"),
    ("cutlery_tray(3, 35, 220)", 3.0, "wall_thickness 3.0"),
    ("chopstick_holder(4, 13, 260)", 2.0, "wall_thickness 2.0"),
    (
        "driver_rack(12, 15, 80)",
        3.0,
        "wall_thickness 3.0 (fixed in the generator)",
    ),
    ("hex_key_holder(9, 1.5, 10)", 3.0, "wall_thickness 3.0"),
    ("bag_clip_org(8, 8, 100)", 2.5, "wall_thickness 2.5"),
    (
        "caliper_holder(150, 40, 3)",
        5.0,
        "wall_thickness 5.0 (the backplate)",
    ),
    (
        "arduino_mount_plate(1, 4)",
        2.9,
        "plate_thickness 4.0 under the counterbores (floor of M3)",
    ),
    (
        "magnetic_strip(12, 10, 40)",
        2.5,
        "bar_thickness 5.0, magnet pocket depth half of it",
    ),
    (
        "pixhawk_mount(45, 1)",
        1.3,
        "damper pocket walls (edge = M3 hole, closed form 1.37)",
    ),
    (
        "wall_hook()",
        4.9,
        "backplate_thickness 6.0 at the mount holes",
    ),
    (
        "t_slot_bracket_2020(20, 20)",
        2.5,
        "counterbore floor of M5 (max(0.5 d, 1.2))",
    ),    (
        "sock_divider(4, 80, 89)",
        2.0,
        "open box: partition and side walls (the same shape with square edges reaches it)",
    ),
    (
        "led_hub_box(120, 80, 50)",
        3.0,
        "open box: wall_thickness 3.0 (the same shape with square edges reaches it)",
    ),
    (
        "led_hub_box(80, 60, 30)",
        2.5,
        "open box: the LED window's lip (the same shape with square edges reaches it)",
    ),
    (
        "dry_box(2, 2, 68)",
        3.0,
        "block with wells: well to side (the same shape with square edges reaches it)",
    ),
    (
        "battery_18650_holder(4, 2.5, 0)",
        2.5,
        "block with cell holes: hole to side (the same shape with square edges reaches it)",
    ),
    (
        "token_well(20, 20, 4)",
        1.5,
        "block with wells: floor_thickness 1.5 (the same shape with square edges reaches it)",
    ),
    (
        "nozzle_holder(12, 8, 6)",
        3.0,
        "block with holes: hole to side (the same shape with square edges reaches it)",
    ),
    (
        "kcup_holder(2, 5, 39)",
        3.0,
        "block with wells: well to side (the same shape with square edges reaches it)",
    ),
    (
        "utensil_caddy(3, 50, 120)",
        4.0,
        "block with compartments: floor_thickness 4.0 (the same shape with square edges reaches it)",
    ),
    (
        "cutting_board_rack(3, 12, 220)",
        4.0,
        "slotted rack: slot to side (the same shape with square edges reaches it)",
    ),
    (
        "pliers_rack(6, 15, 60)",
        5.0,
        "slotted rack: slot to side (the same shape with square edges reaches it)",
    ),
    (
        "build_plate_rack(5, 15, 200)",
        5.0,
        "slotted rack: slot to side (the same shape with square edges reaches it)",
    ),
    (
        "cable_clip(9, 36)",
        1.9,
        "clip: cable bore and slot to side (the same shape with square edges reaches it)",
    ),
    (
        "razor_holder(12, 22, 4.5)",
        3.0,
        "backplate: wall_thickness 3.0 at the slot (the same shape with square edges reaches it)",
    ),
];

/// Every row reaches its spec wall (0.05 mm meshing tolerance); a row that misses names its value
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "meshes 39 shapes at real size: runs in release (release-profile.yml)"
)]
fn stdlib_shapes_keep_their_spec_walls_at_real_size() {
    // one thread per row (the rows are independent; the slowest row bounds the test)
    let results: Vec<(usize, f32)> = std::thread::scope(|scope| {
        // every thread is spawned before the first join (collecting the handles is what runs them together)
        #[allow(clippy::needless_collect)]
        let handles: Vec<_> = ROWS
            .iter()
            .enumerate()
            .map(|(i, (lol, _, _))| {
                scope.spawn(move || {
                    let node = parse_lol(lol).unwrap_or_else(|e| panic!("{lol}: {e}"));
                    (i, thinnest_wall(&node, 112))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("row thread"))
            .collect()
    });
    let compared = results.len();
    let mut short = Vec::new();
    for (i, t) in results {
        let (lol, wall, source) = ROWS[i];
        if t < wall - 0.05 {
            short.push(format!("{lol}: thinnest {t:.3} mm, spec {wall} ({source})"));
        }
    }
    assert_eq!(compared, ROWS.len(), "compared nothing");
    assert!(
        short.is_empty(),
        "walls below the spec:\n{}",
        short.join("\n")
    );
}
