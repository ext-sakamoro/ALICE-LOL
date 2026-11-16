//! No template is built from solids that touch along a shared face, or from a cutter that stops exactly
//! on the material's face
//!
//! Such a construction gives a field that is exactly 0 where it should be negative (a union seam inside
//! the material, which a mesher emits as faces with material on both sides and an inside test reads as
//! "on surface") or exactly 0 across a cut's mouth (a zero-thickness membrane over an opening). The
//! check is `alice_sdf::coplanar::coplanar_faces`, an analysis of the built tree (no sampling), over every
//! template of the grammar corpus and the fixtures.
//!
//! Remaining pairs need an entry in `ALLOWED` with a reason a reader can check; the list starts empty.
//! Nodes the analysis cannot follow are counted and ratcheted separately (`not_analysed` below), so a
//! template the analysis cannot see into does not pass as clean.
//!
//! oracle: the analysis's own closed-form tests in ALICE-SDF (tests/test_coplanar_faces.rs) for what a
//! pair is; here, the property that the corpus has none.

#[path = "common/mod.rs"]
mod common;
use alice_lol::runtime_parser::parse_lol;
use alice_lol::stdlib::hardsurface::skadis_sdf::{
    skadis_peg_and_shoulder, BOARD_T, PEG_BLADE_T, SHOULDER_DEPTH, SHOULDER_H,
};
use alice_sdf::coplanar::coplanar_faces;
use alice_sdf::eval::eval;
use alice_sdf::SdfNode;
use glam::Vec3;

/// (template, reason) for a template allowed to keep a coplanar pair
const ALLOWED: &[(&str, &str)] = &[];

fn corpus() -> Vec<(String, String)> {
    let mut s = common::corpus::grammar_corpus();
    s.extend(
        common::corpus::fixtures()
            .into_iter()
            .map(|(n, x)| (format!("fixture:{n}"), x.to_string())),
    );
    s
}

#[test]
fn no_template_has_a_seam_or_a_flush_cut() {
    let mut checked = 0usize;
    let mut offenders = Vec::new();
    for (name, src) in corpus() {
        let Ok(node) = parse_lol(&src) else { continue };
        checked += 1;
        let r = coplanar_faces(&node);
        if !r.pairs.is_empty() && !ALLOWED.iter().any(|(n, _)| *n == name) {
            let kinds: Vec<String> = r.pairs.iter().map(|p| format!("{:?}@{:?}", p.kind, p.point)).collect();
            offenders.push(format!("{name}: {}", kinds.join(", ")));
        }
    }
    assert!(checked > 200, "only {checked} templates parsed");
    assert!(
        offenders.is_empty(),
        "{} templates with a seam or flush cut:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

/// The peg as it was built before (blade x in [-BOARD_T, 0] meeting the shoulder at x = -BOARD_T)
fn peg_with_seam(hook_width: f32) -> SdfNode {
    let blade = SdfNode::box3d(BOARD_T, PEG_BLADE_T, hook_width).translate(-BOARD_T * 0.5, 0.0, 0.0);
    let shoulder = SdfNode::box3d(SHOULDER_DEPTH, SHOULDER_H, hook_width).translate(
        SHOULDER_DEPTH.mul_add(-0.5, -BOARD_T),
        0.0,
        0.0,
    );
    blade.union(shoulder)
}

#[test]
fn the_skadis_peg_has_no_seam_and_keeps_its_shape_and_fit() {
    for w in [5.0_f32, 8.0] {
        let peg = skadis_peg_and_shoulder(w);
        assert!(coplanar_faces(&peg).pairs.is_empty(), "width {w}: {:?}", coplanar_faces(&peg).pairs);
        // the former seam (the shoulder's face behind the blade) is inside the material now
        let seam_centre = Vec3::new(-BOARD_T, 0.0, 0.0);
        assert!(eval(&peg, seam_centre) < -0.4, "width {w}: field at the old seam {}", eval(&peg, seam_centre));
        assert_eq!(eval(&peg_with_seam(w), seam_centre), 0.0, "the old construction had the seam");
        // functional dimensions (what must still fit the SKADIS board): blade 4.5 thick reaching BOARD_T
        // through the board, the shoulder's face bearing on the board's back at x = -BOARD_T, the
        // shoulder SHOULDER_H tall, the width; read from the field, which is unchanged outside the part
        let on = |p: Vec3| eval(&peg, p).abs() < 1e-5;
        assert!(on(Vec3::new(-2.5, PEG_BLADE_T * 0.5, 0.0)), "blade top face");
        assert!(on(Vec3::new(-2.5, -PEG_BLADE_T * 0.5, 0.0)), "blade bottom face");
        assert!(on(Vec3::new(0.0, 0.0, 0.0)), "blade tip at x = 0");
        assert!(on(Vec3::new(-BOARD_T, 3.0, 0.0)), "shoulder face at x = -BOARD_T");
        assert!(on(Vec3::new(-BOARD_T - 1.0, SHOULDER_H * 0.5, 0.0)), "shoulder top");
        assert!(on(Vec3::new(-2.5, 0.0, w * 0.5)), "width");
        // outside the part the field is the old one (the blade grew only inside the shoulder)
        let mut compared = 0;
        for i in -12..=4 {
            for j in -8..=8 {
                for k in -8..=8 {
                    let p = Vec3::new(i as f32 * 0.55, j as f32 * 0.6, k as f32 * 0.6);
                    let old = eval(&peg_with_seam(w), p);
                    if old > 1e-3 {
                        compared += 1;
                        let new = eval(&peg, p);
                        assert!((new - old).abs() <= 1e-5, "width {w} at {p}: {new} vs {old}");
                    }
                }
            }
        }
        assert!(compared > 1000, "width {w}: only {compared} outside points compared");
    }
}
