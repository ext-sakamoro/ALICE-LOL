//! The `H` argument kind of `alice-lol/language/constructs.toml` is measured, not assumed
//!
//! `H` means "a radius or a half dimension: the shape's full size along it is twice the number"; the
//! training data and the documents' units rely on it to tell a radius from a diameter. Oracle: for each
//! construct below, raising one length argument (`H` or `L`) by `D` grows the shape's bounding box by
//! `2·D` along some axis if and only if the argument is `H` (the box is alice-sdf's interval tight
//! box, which is the same for both arguments' perturbations). A construct marked `H` whose box grows
//! by `D` or not at all, or an `L` argument whose box grows by `2·D`, fails.

use alice_lol::print_export::{node_to_mesh, PrintConfig};
use alice_sdf::tight_aabb::{analytic_aabb, AnalyticAabb};
use glam::Vec3;

const DEFINITION: &str = include_str!("../language/constructs.toml");
const D: f32 = 0.4;

/// one valid instance of every construct with an `H` argument: its numbers, and the children text
const SAMPLES: &[(&str, &[f32], &str)] = &[
    ("sphere", &[1.0], ""),
    ("octahedron", &[1.0], ""),
    ("tetrahedron", &[1.0], ""),
    ("cylinder", &[1.0, 0.7], ""),
    ("torus", &[1.0, 0.3], ""),
    ("cone", &[1.0, 0.7], ""),
    ("capsule", &[0.4, 1.0], ""),
    ("hex_prism", &[1.0, 0.7], ""),
    ("egg", &[1.0, 0.5], ""),
    ("diamond", &[1.0, 0.7], ""),
    ("circle_2d", &[1.0, 0.3], ""),
    ("box3d", &[1.0, 0.7, 0.5], ""),
    ("ellipsoid", &[1.0, 0.7, 0.5], ""),
    ("rounded_cone", &[0.6, 0.3, 1.0], ""),
    ("link", &[0.6, 0.5, 0.15], ""),
    ("capped_cone", &[1.0, 0.6, 0.3], ""),
    ("capped_torus", &[1.0, 0.3, 120.0], ""),
    ("rounded_cylinder", &[1.0, 0.2, 0.7], ""),
    ("tube", &[1.0, 0.2, 0.7], ""),
    ("barrel", &[1.0, 0.7, 0.2], ""),
    ("rounded_box", &[1.0, 0.7, 0.5, 0.1], ""),
    ("helix", &[1.0, 0.2, 0.5, 1.0], ""),
    ("box_frame", &[1.0, 0.7, 0.5, 0.1], ""),
    ("star_polygon", &[1.0, 5.0, 2.0, 0.3], ""),
    ("cross_shape", &[1.0, 0.3, 0.1, 0.3], ""),
    ("horseshoe", &[60.0, 1.0, 0.5, 0.3, 0.2], ""),
    ("superellipsoid", &[1.0, 0.7, 0.5, 1.0, 1.0], ""),
    ("segment_2d", &[-1.0, 0.0, 1.0, 0.0, 0.2, 0.3], ""),
    (
        "bezier",
        &[-1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.2],
        "",
    ),
    ("pipe", &[0.1], "sphere(1.0), box3d(0.5, 0.5, 0.5)"),
    ("round", &[0.2], "box3d(1.0, 0.7, 0.5)"),
    ("extrude", &[0.5], "circle_2d(1.0, 0.3)"),
];

fn kinds(name: &str) -> String {
    let key = format!("name = \"{name}\",");
    let line = DEFINITION
        .lines()
        .find(|l| l.contains(&key))
        .unwrap_or_else(|| panic!("no row {name}"));
    let a = line
        .split("args = \"")
        .nth(1)
        .unwrap_or_else(|| panic!("{name}: no args"));
    a.split('"').next().unwrap().to_string()
}

fn program(name: &str, nums: &[f32], children: &str) -> String {
    let mut parts: Vec<String> = nums.iter().map(|n| format!("{n:?}")).collect();
    if !children.is_empty() {
        parts.push(children.to_string());
    }
    format!("{name}({})", parts.join(", "))
}

/// the shape's extent: alice-sdf's analytic box where it is exact for the construct, otherwise the
/// box of the exported mesh (a grid of 0.05 inside ±4, small against the 2·D = 0.8 being looked for)
fn extent(src: &str) -> Vec3 {
    let node = alice_lol::runtime_parser::parse_lol(src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
    if let AnalyticAabb::Bounded(b) = analytic_aabb(&node) {
        return b.max - b.min;
    }
    let config = PrintConfig {
        resolution: 160,
        bounds_min: Vec3::splat(-4.0),
        bounds_max: Vec3::splat(4.0),
        scale_mm: 1.0,
    };
    let mesh = node_to_mesh(&node, &config);
    assert!(!mesh.vertices.is_empty(), "{src}: empty mesh");
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for v in &mesh.vertices {
        lo = lo.min(v.position);
        hi = hi.max(v.position);
    }
    hi - lo
}

/// the growth of the box along each axis when number `i` is raised by `D`
fn growth(name: &str, nums: &[f32], children: &str, i: usize) -> Vec3 {
    let mut up = nums.to_vec();
    up[i] += D;
    extent(&program(name, &up, children)) - extent(&program(name, nums, children))
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "meshes every sampled construct twice per argument: run in release (CI job doc-programs, preflight)"
)]
fn an_h_argument_grows_the_shape_by_twice_its_change_and_an_l_argument_does_not() {
    let mut compared = 0;
    let mut wrong = Vec::new();
    for (name, nums, children) in SAMPLES {
        let k = kinds(name);
        assert_eq!(k.len(), nums.len(), "{name}: args {k:?} vs sample {nums:?}");
        for (i, kind) in k
            .chars()
            .enumerate()
            .filter(|(_, c)| *c == 'H' || *c == 'L')
        {
            let g = growth(name, nums, children, i);
            let doubles = g.to_array().iter().any(|x| (x - 2.0 * D).abs() < 0.25 * D);
            compared += 1;
            if doubles != (kind == 'H') {
                wrong.push(format!(
                    "{name} argument {i} is {kind}, its box grows by {g}"
                ));
            }
        }
    }
    assert!(compared > 0, "compared nothing");
    assert!(
        wrong.is_empty(),
        "{} of {compared}:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn every_construct_with_an_h_argument_has_a_sample() {
    let mut with_h: Vec<&str> = Vec::new();
    for line in DEFINITION.lines() {
        if let (Some(n), Some(a)) = (
            line.split("name = \"").nth(1),
            line.split("args = \"").nth(1),
        ) {
            if a.split('"').next().unwrap().contains('H') {
                with_h.push(n.split('"').next().unwrap());
            }
        }
    }
    let sampled: Vec<&str> = SAMPLES.iter().map(|s| s.0).collect();
    let missing: Vec<&&str> = with_h.iter().filter(|n| !sampled.contains(n)).collect();
    assert!(!with_h.is_empty());
    assert!(
        missing.is_empty(),
        "constructs with an H argument and no sample: {missing:?}"
    );
}
