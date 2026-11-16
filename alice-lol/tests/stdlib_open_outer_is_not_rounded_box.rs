//! A stdlib shape whose outer is cut open through a face is not built on a fully rounded box
//!
//! `rounded_box` rounds all twelve edges. When a cavity, well, hole or slot is cut from that box through
//! one of its faces, the rounding of that face's edges eats the wall between the opening and the side:
//! at the face the wall is `wall − r`, and nothing when `r ≥ wall` (measured 2026-11-16 on the bundled
//! templates: `led_hub_box` 0.54 mm for a 3 mm wall, `dry_box` 1.11 for 3, `sock_divider` 1.44 for
//! 2.5). Open shapes are built with `open_shell` / `rounded_plate`, which round only the edges along the
//! opening axis and keep the opened face flat.
//!
//! This test walks the tree of every stdlib template probe (both argument sets) and refuses a
//! `Subtraction` whose minuend (through nested subtractions and placements) is a `RoundedBox` with a
//! positive radius while the subtrahend's bound crosses the box's bound. Shapes where that is intended
//! are listed in `ALLOWED` with the reason. The walk is checked to have visited every node of each tree
//! (`SdfNode::node_count`), so a variant it does not descend into fails the test instead of hiding a
//! subtraction; and at least one subtraction must be examined.

use alice_lol::geometry_id::STDLIB_TEMPLATE_PROBE_LOL;
use alice_lol::runtime_parser::parse_lol;
use alice_sdf::tight_aabb::{analytic_aabb, AnalyticAabb};
use alice_sdf::SdfNode;

/// (template name, why its rounded box may be cut through a face)
const ALLOWED: &[(&str, &str)] = &[];

#[derive(Default)]
struct Walk {
    visited: u32,
    examined: usize,
    rounded: usize,
    refused: Vec<String>,
}

/// The node a subtraction cuts from: through nested subtractions and placements
fn minuend_base(node: &SdfNode) -> &SdfNode {
    match node {
        SdfNode::Subtraction { a, .. } => minuend_base(a),
        SdfNode::Translate { child, .. } | SdfNode::Rotate { child, .. } => minuend_base(child),
        other => other,
    }
}

/// The base's own bound (a subtraction can only remove material, so its minuend's bound contains it)
fn minuend_bound(node: &SdfNode) -> Option<alice_sdf::prelude::Aabb> {
    match node {
        SdfNode::Subtraction { a, .. } => minuend_bound(a),
        other => match analytic_aabb(other) {
            AnalyticAabb::Bounded(b) => Some(b),
            _ => None,
        },
    }
}

fn crosses(cutter: &alice_sdf::prelude::Aabb, outer: &alice_sdf::prelude::Aabb) -> bool {
    let overlaps = (0..3).all(|k| cutter.min[k] < outer.max[k] && cutter.max[k] > outer.min[k]);
    let pokes_out = (0..3).any(|k| cutter.min[k] < outer.min[k] || cutter.max[k] > outer.max[k]);
    overlaps && pokes_out
}

impl Walk {
    fn node(&mut self, template: &str, node: &SdfNode) {
        self.visited += 1;
        match node {
            SdfNode::Subtraction { a, b } => {
                // every subtraction's minuend is classified; only a rounded box with r > 0 is then
                // checked against its cutter
                self.examined += 1;
                if let SdfNode::RoundedBox { round_radius, .. } = minuend_base(a) {
                    if *round_radius > 0.0 {
                        self.rounded += 1;
                        let cutter = match analytic_aabb(b) {
                            AnalyticAabb::Bounded(x) => Some(x),
                            _ => None,
                        };
                        match (cutter, minuend_bound(a)) {
                            (Some(c), Some(o)) => {
                                if crosses(&c, &o) {
                                    self.refused.push(format!(
                                        "{template}: subtract from rounded_box (r {round_radius}) with a \
                                         cutter crossing its face (cutter {:?}..{:?}, outer {:?}..{:?})",
                                        c.min, c.max, o.min, o.max
                                    ));
                                }
                            }
                            // no bound: the crossing cannot be decided, so it is not passed
                            _ => self.refused.push(format!(
                                "{template}: subtract from rounded_box (r {round_radius}) whose cutter or outer \
                                 has no analytic bound"
                            )),
                        }
                    }
                }
                self.node(template, a);
                self.node(template, b);
            }
            SdfNode::Union { a, b }
            | SdfNode::Intersection { a, b }
            | SdfNode::SmoothUnion { a, b, .. }
            | SdfNode::SmoothIntersection { a, b, .. }
            | SdfNode::SmoothSubtraction { a, b, .. }
            | SdfNode::ChamferUnion { a, b, .. }
            | SdfNode::ChamferIntersection { a, b, .. }
            | SdfNode::ChamferSubtraction { a, b, .. } => {
                self.node(template, a);
                self.node(template, b);
            }
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
            | SdfNode::PolarRepeat { child, .. } => self.node(template, child),
            _ => {}
        }
    }
}

#[test]
fn no_stdlib_template_cuts_a_fully_rounded_box_through_a_face() {
    let mut total = Walk::default();
    let mut trees = 0usize;
    for lol in STDLIB_TEMPLATE_PROBE_LOL {
        let name = lol.split('(').next().unwrap_or(lol);
        let node = parse_lol(lol).unwrap_or_else(|e| panic!("{lol}: {e}"));
        let mut w = Walk::default();
        w.node(lol, &node);
        assert_eq!(
            w.visited,
            node.node_count(),
            "{lol}: the walk visited {} of {} nodes (a variant it does not descend into)",
            w.visited,
            node.node_count()
        );
        trees += 1;
        total.visited += w.visited;
        total.examined += w.examined;
        total.rounded += w.rounded;
        let allowed = ALLOWED.iter().any(|(n, _)| *n == name);
        if !allowed {
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
    assert!(total.examined > 100, "examined {} subtractions (the check compared nothing)", total.examined);
    assert!(
        total.refused.is_empty(),
        "fully rounded boxes cut open through a face:\n{}",
        total.refused.join("\n")
    );
}
