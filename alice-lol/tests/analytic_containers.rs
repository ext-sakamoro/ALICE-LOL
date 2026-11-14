//! Real-size probes for stdlib containers: one connected part, and every wall at least the
//! generator's own specified wall thickness
//!
//! The wall is measured per triangle of a marching-cubes mesh along the negative SDF gradient read
//! just below the surface (the face normal of a coarse mesh is not trusted near edges). A shape
//! that only reached its wall thickness through an oversized rounded box, or whose rim tapers to
//! nothing, fails here.

use alice_lol::runtime_parser::parse_lol;
use alice_sdf::eval::{eval, eval_normal};
use alice_sdf::mesh::{sdf_to_mesh, MarchingCubesConfig};
use alice_sdf::tight_aabb::{compute_tight_aabb_with_config, TightAabbConfig};
use alice_sdf::validity::local_thickness;
use alice_sdf::SdfNode;
use glam::Vec3;
use std::collections::{HashMap, HashSet};

struct Probe {
    components: usize,
    min_wall: f32,
    at: Vec3,
    measured: usize,
}

fn find(p: &mut [usize], mut x: usize) -> usize {
    while p[x] != x {
        p[x] = p[p[x]];
        x = p[x];
    }
    x
}

fn probe(node: &SdfNode) -> Probe {
    let bb = compute_tight_aabb_with_config(node, &TightAabbConfig::preset_large());
    let pad = (bb.max - bb.min).max_element() * 0.02 + 0.5;
    let (lo, hi) = (bb.min - Vec3::splat(pad), bb.max + Vec3::splat(pad));
    let res = 160;
    let m = sdf_to_mesh(
        node,
        lo,
        hi,
        &MarchingCubesConfig {
            resolution: res,
            ..MarchingCubesConfig::default()
        },
    );
    #[allow(clippy::cast_precision_loss)]
    let q = (hi - lo).max_element() / res as f32 * 1e-3;
    let mut ids: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut vid = Vec::with_capacity(m.vertices.len());
    for v in &m.vertices {
        #[allow(clippy::cast_possible_truncation)]
        let k = (
            (v.position.x / q).round() as i64,
            (v.position.y / q).round() as i64,
            (v.position.z / q).round() as i64,
        );
        let n = ids.len();
        vid.push(*ids.entry(k).or_insert(n));
    }
    let mut p: Vec<usize> = (0..ids.len()).collect();
    for t in m.indices.chunks(3) {
        let (a, b, c) = (vid[t[0] as usize], vid[t[1] as usize], vid[t[2] as usize]);
        let (ra, rb) = (find(&mut p, a), find(&mut p, b));
        p[ra] = rb;
        let (rb, rc) = (find(&mut p, b), find(&mut p, c));
        p[rb] = rc;
    }
    let used: HashSet<usize> = m.indices.iter().map(|&i| vid[i as usize]).collect();
    let components = used
        .iter()
        .map(|&x| find(&mut p, x))
        .collect::<HashSet<_>>()
        .len();
    let span = (hi - lo).length();
    let pv = |i: u32| m.vertices[i as usize].position;
    let (mut min_wall, mut at, mut measured) = (f32::INFINITY, Vec3::ZERO, 0);
    for t in m.indices.chunks(3) {
        let (a, b, c) = (pv(t[0]), pv(t[1]), pv(t[2]));
        if (b - a).cross(c - a).length_squared() <= 0.0 {
            continue;
        }
        let cen = (a + b + c) / 3.0;
        let inside = cen - eval_normal(node, cen) * (eval(node, cen) + 1e-3);
        let n = eval_normal(node, inside);
        let origin = inside + n * (1e-4 - eval(node, inside));
        if let Some(th) = local_thickness(node, origin, -n, span) {
            measured += 1;
            if th < min_wall {
                min_wall = th;
                at = cen;
            }
        }
    }
    Probe {
        components,
        min_wall,
        at,
        measured,
    }
}

/// One part, and the thinnest wall at least `wall` (minus the meshing tolerance)
fn check(lol: &str, wall: f32) {
    let node = parse_lol(lol).expect("parse");
    let p = probe(&node);
    assert!(
        p.measured > 0,
        "{lol}: no triangle measured (compared nothing)"
    );
    assert_eq!(p.components, 1, "{lol}: {} separate parts", p.components);
    assert!(
        p.min_wall >= wall - 0.1,
        "{lol}: thinnest wall {} mm at {} (spec {wall})",
        p.min_wall,
        p.at
    );
}

#[test]
fn storage_box_walls_are_the_specified_two_millimetres_up_to_the_rim() {
    check("storage_box(150, 100, 60)", 2.0);
}

#[test]
fn tissue_box_cover_walls_are_the_specified_thickness_round_the_corners() {
    // the default wall is 1.6 mm (runtime_parser)
    check("tissue_box_cover(240, 120, 60)", 1.6);
}

#[test]
fn sticky_note_holder_walls_are_the_specified_thickness_up_to_the_rim() {
    // the default wall and floor are 1.5 mm (runtime_parser)
    check("sticky_note_holder(76, 76, 40)", 1.5);
}

#[test]
fn business_card_holder_walls_are_the_specified_thickness_up_to_the_rim() {
    // the default wall is 1.5 mm (runtime_parser)
    check("business_card_holder(91, 55, 10)", 1.5);
}

/// The toilet-roll holder's backplate is `wall_thickness` (5 mm here) and its mount holes go
/// through it: no wall thinner than the plate around them
#[test]
fn tp_holder_mount_holes_go_through_the_backplate() {
    check("tp_holder(40, 110, 5)", 4.5);
}

/// The razor holder's backplate is 3 mm (runtime_parser default) and its mount hole goes through it
/// (the thinnest sample sits on the hole's rim, where a mesh of about 0.5 mm cells reads up to
/// 0.25 mm short; a hole running inside the plate left 0.05 mm)
#[test]
fn razor_holder_mount_hole_goes_through_the_backplate() {
    check("razor_holder(12, 22, 4.5)", 2.85);
}
