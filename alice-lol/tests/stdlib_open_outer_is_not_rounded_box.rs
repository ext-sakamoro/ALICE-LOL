//! A stdlib shape whose outer is cut open through a face is not built on a fully rounded box
//!
//! `rounded_box` rounds all twelve edges. When a cavity, well, hole or slot is cut from that box through
//! one of its faces, the rounding of that face's edges eats the wall between the opening and the side:
//! at the face the wall is `wall − r`, and nothing when `r ≥ wall` (measured 2026-11-16 on the bundled
//! templates: `led_hub_box` 0.54 mm for a 3 mm wall, `dry_box` 1.11 for 3, `sock_divider` 1.44 for
//! 2.5). Open shapes are built with `open_shell` / `rounded_plate`, which round only the edges along one
//! axis and clip the other faces flat with a box.
//!
//! The walk visits every stdlib template probe (both argument sets). At every subtraction (plain, smooth
//! or chamfer) it classifies the minuend:
//! - looking through nested subtractions and the members of unions (each rounded part keeps its own
//!   bound, so a cutter is checked only against the parts it reaches);
//! - a `rounded_plate` (a rounded box intersected with a box) is flat on the clipped faces: not refused;
//! - a `RoundedBox` with a positive radius is refused when the cutter's bound reaches or crosses the
//!   minuend's bound on any face (a cutter flush with the face opens it as well);
//! - any other node that has a rounded box with a positive radius below it (scale, round, mirror, …)
//!   cannot be classified and is refused.
//!
//! Probe texts where a refusal is known and has its own cause go in `ALLOWED` with the reason. The walk checks that
//! it visited every node of each tree (`SdfNode::node_count`), so a variant it does not descend into
//! fails the test instead of hiding a subtraction; it must examine more than 100 subtractions.

use alice_lol::geometry_id::STDLIB_TEMPLATE_PROBE_LOL;
use alice_lol::runtime_parser::parse_lol;
use alice_sdf::prelude::Aabb;
use alice_sdf::tight_aabb::{analytic_aabb, AnalyticAabb};
use alice_sdf::SdfNode;

/// (exact probe text, why its rounded box may be cut through a face)
const ALLOWED: &[(&str, &str)] = &[
    (
        "clamp_rack(12, 8, 4)",
    "at 12 clamps the first and last mount holes reach 3.5 mm into the hook arms (a layout defect of its \
     own, not the rim: the holes and arms overlap; the shipped presets with 5 and 8 clamps do not)",
    ),
    (
        "headphone_holder(3.0, 2.0, 1.0)",
        "at these small arguments the hook tip lies over the top mount hole (the hole and the tip overlap, \
         a layout defect of its own); at (30, 20, 10) the holes are clear of the arm",
    ),
];

/// A cutter within this distance of a face counts as reaching it (mm)
const FLUSH: f32 = 1e-3;

/// The children of a node, for every variant the stdlib builds with (the walk's visit count against
/// `node_count` proves this list is complete for the trees it reads)
fn children(node: &SdfNode) -> Vec<&SdfNode> {
    match node {
        SdfNode::Union { a, b }
        | SdfNode::Intersection { a, b }
        | SdfNode::Subtraction { a, b }
        | SdfNode::SmoothUnion { a, b, .. }
        | SdfNode::SmoothIntersection { a, b, .. }
        | SdfNode::SmoothSubtraction { a, b, .. }
        | SdfNode::ChamferUnion { a, b, .. }
        | SdfNode::ChamferIntersection { a, b, .. }
        | SdfNode::ChamferSubtraction { a, b, .. } => vec![a, b],
        SdfNode::Translate { child, .. }
        | SdfNode::Rotate { child, .. }
        | SdfNode::Scale { child, .. }
        | SdfNode::ScaleNonUniform { child, .. }
        | SdfNode::Round { child, .. }
        | SdfNode::Onion { child, .. }
        | SdfNode::RepeatFinite { child, .. }
        | SdfNode::RepeatInfinite { child, .. }
        | SdfNode::Mirror { child, .. }
        | SdfNode::Elongate { child, .. }
        | SdfNode::Revolution { child, .. }
        | SdfNode::Extrude { child, .. }
        | SdfNode::Twist { child, .. }
        | SdfNode::Bend { child, .. }
        | SdfNode::Taper { child, .. }
        | SdfNode::PolarRepeat { child, .. } => vec![child],
        _ => Vec::new(),
    }
}

fn is_rounded(node: &SdfNode) -> bool {
    matches!(node, SdfNode::RoundedBox { round_radius, .. } if *round_radius > 0.0)
}

fn has_rounded_below(node: &SdfNode) -> bool {
    is_rounded(node) || children(node).into_iter().any(has_rounded_below)
}

/// Through placements only
fn unplaced(node: &SdfNode) -> &SdfNode {
    match node {
        SdfNode::Translate { child, .. } | SdfNode::Rotate { child, .. } => unplaced(child),
        other => other,
    }
}

/// The rounded boxes (positive radius) a cutter could open in a subtraction's minuend, each with its
/// bound in the minuend's frame: through nested subtractions and the members of unions (which share the
/// minuend's frame); a placed part contributes the bound of the placed subtree. A `rounded_plate` (a
/// rounded box clipped by a box) contributes nothing. `Err` when a rounded box sits under a node this
/// check cannot reason about (scale, round, mirror, …)
fn rounded_parts(node: &SdfNode) -> Result<Vec<Option<Aabb>>, ()> {
    match node {
        SdfNode::Subtraction { a, .. }
        | SdfNode::SmoothSubtraction { a, .. }
        | SdfNode::ChamferSubtraction { a, .. } => rounded_parts(a),
        SdfNode::Union { a, b }
        | SdfNode::SmoothUnion { a, b, .. }
        | SdfNode::ChamferUnion { a, b, .. } => {
            let mut parts = rounded_parts(a)?;
            parts.extend(rounded_parts(b)?);
            Ok(parts)
        }
        // rounded_plate: a rounded box clipped by a box, the clipped faces are flat
        SdfNode::Intersection { a, b }
            if (is_rounded(unplaced(a)) && matches!(unplaced(b), SdfNode::Box3d { .. }))
                || (is_rounded(unplaced(b)) && matches!(unplaced(a), SdfNode::Box3d { .. })) =>
        {
            Ok(Vec::new())
        }
        SdfNode::Translate { child, .. } | SdfNode::Rotate { child, .. } => {
            // the parts below a placement are reported with the placed subtree's bound
            let below = rounded_parts(child)?;
            Ok(if below.is_empty() {
                below
            } else {
                vec![bound(node)]
            })
        }
        n if is_rounded(n) => Ok(vec![bound(n)]),
        n if has_rounded_below(n) => Err(()),
        _ => Ok(Vec::new()),
    }
}

fn bound(node: &SdfNode) -> Option<Aabb> {
    match analytic_aabb(node) {
        AnalyticAabb::Bounded(b) => Some(b),
        _ => None,
    }
}

/// The bounds of a cutter's parts: the members of a union are checked one by one (they share the
/// cutter's frame), anything else by its whole bound
fn cutter_parts(node: &SdfNode) -> Vec<Option<Aabb>> {
    match node {
        SdfNode::Union { a, b } => {
            let mut parts = cutter_parts(a);
            parts.extend(cutter_parts(b));
            parts
        }
        other => vec![bound(other)],
    }
}

/// The cutter overlaps the outer and reaches or crosses one of its faces
fn opens_a_face(cutter: &Aabb, outer: &Aabb) -> bool {
    let overlaps = (0..3).all(|k| cutter.min[k] < outer.max[k] && cutter.max[k] > outer.min[k]);
    let reaches = (0..3)
        .any(|k| cutter.min[k] <= outer.min[k] + FLUSH || cutter.max[k] >= outer.max[k] - FLUSH);
    overlaps && reaches
}

#[derive(Default)]
struct Walk {
    visited: u32,
    examined: usize,
    rounded: usize,
    refused: Vec<String>,
}

impl Walk {
    fn node(&mut self, template: &str, node: &SdfNode) {
        self.visited += 1;
        if let SdfNode::Subtraction { a, b }
        | SdfNode::SmoothSubtraction { a, b, .. }
        | SdfNode::ChamferSubtraction { a, b, .. } = node
        {
            self.examined += 1;
            match rounded_parts(a) {
                Err(()) => self.refused.push(format!(
                    "{template}: a rounded box sits under a node the check cannot classify in a subtraction's \
                     minuend"
                )),
                Ok(parts) => {
                    let cutters = cutter_parts(b);
                    for part in parts {
                        self.rounded += 1;
                        for cutter in &cutters {
                        match (*cutter, part) {
                            (Some(c), Some(o)) => {
                                if opens_a_face(&c, &o) {
                                    self.refused.push(format!(
                                        "{template}: a fully rounded box is cut through its face (cutter \
                                         {:?}..{:?}, part {:?}..{:?})",
                                        c.min, c.max, o.min, o.max
                                    ));
                                }
                            }
                            // no bound: the crossing cannot be decided, so it is not passed
                            _ => self.refused.push(format!(
                                "{template}: a fully rounded box is cut and the cutter or part has no analytic \
                                 bound"
                            )),
                        }
                        }
                    }
                }
            }
        }
        for c in children(node) {
            self.node(template, c);
        }
    }
}

fn walk(lol: &str) -> (Walk, u32) {
    let node = parse_lol(lol).unwrap_or_else(|e| panic!("{lol}: {e}"));
    let mut w = Walk::default();
    w.node(lol, &node);
    (w, node.node_count())
}

#[test]
fn no_stdlib_template_cuts_a_fully_rounded_box_through_a_face() {
    let mut total = Walk::default();
    let mut trees = 0usize;
    let mut allowed_hit: Vec<&str> = Vec::new();
    for lol in STDLIB_TEMPLATE_PROBE_LOL {
        let (w, nodes) = walk(lol);
        assert_eq!(
            w.visited, nodes,
            "{lol}: the walk visited {} of {nodes} nodes (a variant it does not descend into)",
            w.visited
        );
        trees += 1;
        total.visited += w.visited;
        total.examined += w.examined;
        total.rounded += w.rounded;
        if ALLOWED.iter().any(|(n, _)| n == lol) {
            if !w.refused.is_empty() {
                allowed_hit.push(lol);
            }
        } else {
            total.refused.extend(w.refused);
        }
    }
    eprintln!(
        "open outer check: {trees} templates, {} nodes, {} subtractions examined, {} from a rounded box, {} refused",
        total.visited,
        total.examined,
        total.rounded,
        total.refused.len()
    );
    assert!(trees > 100, "compared {trees} templates");
    assert!(
        total.examined > 100,
        "examined {} subtractions (the check compared nothing)",
        total.examined
    );
    // an allow-list entry that no longer refuses anything is stale (remove it)
    for (lol, _) in ALLOWED {
        assert!(
            allowed_hit.contains(lol),
            "{lol}: allow-listed but the check no longer refuses it (remove the entry)"
        );
    }
    assert!(
        total.refused.is_empty(),
        "fully rounded boxes cut open through a face:\n{}",
        total.refused.join("\n")
    );
}

/// The rules on hand-written trees (LOL text): each refused case and each passed case
#[test]
fn the_check_refuses_and_passes_the_stated_cases() {
    let refused = [
        // a well from the top of a rounded block
        "subtract(rounded_box(10, 10, 10, 2), translate(0, 6, 0, box3d(4, 6, 4)))",
        // the same well flush with the top face (it still opens the face)
        "subtract(rounded_box(10, 10, 10, 2), translate(0, 4, 0, box3d(4, 6, 4)))",
        // a smooth subtraction
        "smooth_subtract(0.5, rounded_box(10, 10, 10, 2), translate(0, 6, 0, box3d(4, 6, 4)))",
        // the rounded box inside a union on the minuend side
        "subtract(union(rounded_box(10, 10, 10, 2), translate(0, -12, 0, box3d(2, 2, 2))), translate(0, 6, 0, box3d(4, 6, 4)))",
        // under a node the check does not classify
        "subtract(scale(2, rounded_box(10, 10, 10, 2)), translate(0, 12, 0, box3d(4, 10, 4)))",
    ];
    for lol in refused {
        let (w, nodes) = walk(lol);
        assert_eq!(w.visited, nodes, "{lol}");
        assert!(!w.refused.is_empty(), "{lol}: passed, should be refused");
    }
    let passed = [
        // a pocket fully inside the rounded box (no face opened)
        "subtract(rounded_box(10, 10, 10, 2), box3d(4, 4, 4))",
        // a well from the top of a square block
        "subtract(box3d(10, 10, 10), translate(0, 6, 0, box3d(4, 6, 4)))",
        // a rounded arm joined to a square plate, the hole goes through the plate only
        "subtract(union(box3d(10, 10, 2), translate(0, 0, 8, rounded_box(2, 2, 6, 1))), translate(0, 6, 0, box3d(1, 1, 4)))",
        // a rounded_plate (rounded box clipped flat by a box) opened from the top
        "subtract(intersection(rounded_box(10, 12, 10, 2), box3d(10, 10, 10)), translate(0, 6, 0, box3d(4, 6, 4)))",
    ];
    for lol in passed {
        let (w, nodes) = walk(lol);
        assert_eq!(w.visited, nodes, "{lol}");
        assert!(w.examined > 0, "{lol}: examined nothing");
        assert!(w.refused.is_empty(), "{lol}: refused {:?}", w.refused);
    }
}
