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
//!
//! The pairs the corpus had before the fix are recorded in `tests/data/coplanar_pairs_at_e21b5c2.txt`
//! (template, kind, side, normal, overlap polygon), and each is checked on today's tree from the field itself.
//! The side is read from the tree before the fix (the field just off the pair's plane, both ways): a
//! sheet that lay inside material must now be material, one that lay in the part's air (a cut's mouth,
//! a seam inside a cutter) must now be air. The side is recorded once, from the tree before the fix; a
//! fix that legitimately moves a pair to the other side fails here and is re-recorded by hand in the
//! data file with its reason on a `#` line (never regenerated automatically, which would accept any
//! change). A former material sheet must now be material (the field's maximum over the polygon, sampled at least
//! `2δ` in from its edges, is `≤ −δ`, and the inside test at its centre answers inside, below
//! `-SURFACE_HIT_EPS`); a former flush cut or contact must now be air (minimum `≥ +δ`). `δ` is `1e-3`
//! of the part's extent (a quarter of the polygon's inradius for a strip thinner than that, never below
//! `DELTA_FLOOR_ULPS` ulp at the extent: a strip too thin for that fails as "overlap too thin to verify"): well above f32 rounding at the largest part and well below the overlaps the
//! fixes add (each stated at its constant).
//!
//! **Why mesh triangle counts are not the gate** (tried and discarded): marching cubes emits no faces
//! across a seam's interior — the field is 0 only exactly on the plane, and samples almost never land
//! there — so a seam's extra faces all lie on its polygon's edge, where it meets the outer surface, and
//! so do the facets marching cubes cuts across any real concave corner there. Two criteria were tried to
//! tell them apart, and both read 0 before the fix too (so they measure nothing): "centroid field ≈ 0"
//! (seam facets also have slightly negative centroids) and "one cell inside the polygon" (there are no
//! seam facets inside). Plane-clipped counts are reported as evidence only.

#[path = "common/mod.rs"]
mod common;
use alice_lol::runtime_parser::parse_lol;
use alice_lol::stdlib::hardsurface::fastener::{counterbore, counterbore_depth, MetricSize};
use alice_lol::stdlib::hardsurface::mount::profile_2020;
use alice_lol::stdlib::hardsurface::skadis_sdf::{
    skadis_peg_and_shoulder, BOARD_T, PEG_BLADE_T, SHOULDER_DEPTH, SHOULDER_H,
};
use alice_sdf::coplanar::coplanar_faces;
use alice_sdf::raycast::SURFACE_HIT_EPS;
use alice_sdf::tight_aabb::{compute_tight_aabb_with_config, TightAabbConfig};
use alice_sdf::eval::eval;
use alice_sdf::SdfNode;
use glam::Vec3;

/// Smallest δ allowed, in ulp of the part's extent: 1000 ulp is 0.015 mm at 250 mm (the largest part)
const DELTA_FLOOR_ULPS: f32 = 1000.0;

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

/// A recorded pair: template, kind, unit normal, overlap polygon
struct Recorded {
    name: String,
    kind: String,
    /// "material", "air" or "surface": the side of the part's surface the pair lay on before the fix
    /// (a seam inside a cutter lies in the part's air)
    side: String,
    n: Vec3,
    poly: Vec<Vec3>,
}

fn recorded() -> Vec<Recorded> {
    include_str!("data/coplanar_pairs_at_e21b5c2.txt")
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let mut it = l.split_whitespace();
            let name = it.next().unwrap().to_string();
            let kind = it.next().unwrap().to_string();
            let side = it.next().unwrap().to_string();
            let v: Vec<f32> = it.map(|x| x.parse().unwrap()).collect();
            let n = Vec3::new(v[0], v[1], v[2]);
            let poly = v[3..].chunks_exact(3).map(|c| Vec3::new(c[0], c[1], c[2])).collect();
            Recorded { name, kind, side, n, poly }
        })
        .collect()
}

/// Points of the polygon on a 21 x 21 grid over its extent, each with its distance to the nearest edge
fn grid_points(poly: &[Vec3], n: Vec3) -> Vec<(Vec3, f32)> {
    let u = n.any_orthonormal_vector();
    let v = n.cross(u);
    let o = poly[0];
    let uv: Vec<(f32, f32)> = poly.iter().map(|p| ((*p - o).dot(u), (*p - o).dot(v))).collect();
    let (u0, u1) = uv.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.0), b.max(q.0)));
    let (v0, v1) = uv.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.1), b.max(q.1)));
    let mut out = Vec::new();
    for i in 0..=20 {
        for j in 0..=20 {
            let (a, b) = (u0 + (u1 - u0) * i as f32 / 20.0, v0 + (v1 - v0) * j as f32 / 20.0);
            let p = o + u * a + v * b;
            // inside, and at least `inset` from every edge (convex polygon, either orientation)
            let mut sign = 0.0_f32;
            let mut nearest = f32::MAX;
            let inside = (0..poly.len()).all(|k| {
                let (e0, e1) = (poly[k], poly[(k + 1) % poly.len()]);
                let e = (e1 - e0).normalize_or_zero();
                let side = e.cross(p - e0).dot(n);
                if sign == 0.0 && side.abs() > 1e-9 { sign = side.signum(); }
                nearest = nearest.min((p - e0 - e * (p - e0).dot(e)).length());
                side * sign >= 0.0
            });
            if inside { out.push((p, nearest)); }
        }
    }
    out
}

#[test]
fn every_recorded_seam_is_material_and_every_recorded_cut_is_air() {
    let corpus = corpus();
    let recs = recorded();
    assert!(recs.len() >= 20, "only {} recorded pairs", recs.len());
    let (mut seams, mut seams_inside, mut cuts) = (0usize, 0usize, 0usize);
    let (mut thin, mut smallest_delta) = (0usize, f32::MAX);
    let (mut cleared_air, mut on_surface) = (0usize, 0usize);
    let mut failures = Vec::new();
    for r in &recs {
        let Some((_, src)) = corpus.iter().find(|(n, _)| *n == r.name) else {
            failures.push(format!("{}: template no longer in the corpus", r.name));
            continue;
        };
        let node = parse_lol(src).unwrap();
        let b = compute_tight_aabb_with_config(&node, &TightAabbConfig::preset_large());
        // δ is 1e-3 of the part's extent, or a quarter of the polygon's inradius for a strip thinner
        // than that (across a thin strip the field cannot go below about half its width); points are
        // taken 2δ in from the edges, where the field is not pulled to 0 by the outer surface
        let grid = grid_points(&r.poly, r.n);
        let inradius = grid.iter().map(|(_, d)| *d).fold(0.0_f32, f32::max);
        let extent = (b.max - b.min).max_element();
        let plain = 1e-3 * extent;
        let delta = plain.min(0.25 * inradius);
        // floor: DELTA_FLOOR_ULPS ulp at the part's extent; a strip so thin that its δ would fall below
        // it cannot tell a fix from "0 within rounding", and fails as its own kind (fixed by a larger
        // overlap, not passed)
        let floor = DELTA_FLOOR_ULPS * (extent.next_up() - extent);
        if delta < plain { thin += 1; }
        smallest_delta = smallest_delta.min(delta);
        if !(delta >= floor) {
            failures.push(format!(
                "{} {}: overlap too thin to verify (inradius {inradius:e}, δ {delta:e} < floor {floor:e})",
                r.name, r.kind
            ));
            continue;
        }
        let pts: Vec<Vec3> = grid.iter().filter(|(_, d)| *d >= 2.0 * delta).map(|(p, _)| *p).collect();
        if pts.is_empty() {
            failures.push(format!("{} {}: polygon has no point 2δ inside (inradius {inradius:e})", r.name, r.kind));
            continue;
        }
        let ds: Vec<f32> = pts.iter().map(|p| eval(&node, *p)).collect();
        let (mx, mn) = ds.iter().fold((f32::MIN, f32::MAX), |(a, b), d| (a.max(*d), b.min(*d)));
        let c = r.poly.iter().fold(Vec3::ZERO, |a, q| a + *q) / r.poly.len() as f32;
        let dc = eval(&node, c);
        if r.kind == "UnionSeam" { seams += 1; } else { cuts += 1; }
        match r.side.as_str() {
            // the sheet lay inside material: it must be material now, and read inside
            "material" => {
                if dc < -SURFACE_HIT_EPS { seams_inside += 1; } else {
                    failures.push(format!("{} {} (material): inside test at {c} reads {dc:e}", r.name, r.kind));
                }
                if mx > -delta {
                    failures.push(format!("{} {} (material): field max {mx:e} over {} points > -δ = {:e}", r.name, r.kind, pts.len(), -delta));
                }
            }
            // the sheet lay in the part's air (a cut's mouth, a seam inside a cutter): it must be air now
            "air" => {
                if dc > SURFACE_HIT_EPS { cleared_air += 1; } else {
                    failures.push(format!("{} {} (air): inside test at {c} reads {dc:e}", r.name, r.kind));
                }
                if mn < delta {
                    failures.push(format!("{} {} (air): field min {mn:e} over {} points < δ = {delta:e}", r.name, r.kind, pts.len()));
                }
            }
            _ => on_surface += 1,
        }
    }
    eprintln!(
        "recorded pairs {}: seams {seams}, cuts / contacts {cuts}; now inside (material) {seams_inside}, now air {cleared_air}, on the surface (no sheet, not field-checked) {on_surface}; not sampled {}, thin-strip δ {thin}, smallest δ {smallest_delta:e}",
        recs.len(),
        recs.len() - seams - cuts
    );
    assert!(failures.is_empty(), "{} recorded pairs not fixed:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn the_profile_slot_has_no_membrane_and_keeps_its_section() {
    // profile_2020: 20 x 20, slot on the +x face at x = 10 (opening along z, 6 wide; neck 1.8 deep;
    // cavity 11 wide under the neck, closing 1:1 to 6.1 deep); length along y
    let p = profile_2020(50.0);
    let d = |x: f32, z: f32| eval(&p, Vec3::new(x, 0.0, z));
    // the mouth was a zero-thickness membrane (field exactly 0 across it); now open air
    assert!(d(10.0, 0.0) > 1e-3, "mouth centre {}", d(10.0, 0.0));
    assert!(d(9.0, 0.0) > 1e-3, "neck {}", d(9.0, 0.0));
    // the wall beside the 6-wide opening is material
    assert!(d(9.5, 3.6) < -1e-3, "neck wall {}", d(9.5, 3.6));
    // the cavity 3 below the face is 4.3 half-wide (5.5 + 1.8 - 3): air at 4.0, rib at 4.6
    assert!(d(7.0, 4.0) > 1e-3, "cavity {}", d(7.0, 4.0));
    assert!(d(7.0, 4.6) < -1e-3, "rib {}", d(7.0, 4.6));
    // and the analysis finds no pair in it
    assert!(coplanar_faces(&p).pairs.is_empty(), "{:?}", coplanar_faces(&p).pairs);
}

#[test]
fn a_counterbore_pocket_reaches_past_the_face_and_keeps_its_seat() {
    // M4 counterbore in a 5 mm plate (faces at y = ±2.5): the head pocket's top lay exactly on the
    // plate's face (a flush cut: a membrane over the pocket); it now reaches CAVITY_PUNCH_MARGIN past it
    let (size, t) = (MetricSize::M4, 5.0_f32);
    let plate = SdfNode::box3d(30.0, t, 30.0);
    let part = plate.subtract(counterbore(size, t));
    assert!(coplanar_faces(&part).pairs.is_empty(), "{:?}", coplanar_faces(&part).pairs);
    let head_r = size.head_diameter_socket() * 0.5;
    let seat = t * 0.5 - counterbore_depth(size, t); // the pocket's floor, where the head bears
    let r = 0.5 * (head_r + size.nominal_diameter() * 0.5); // between the screw hole and the head's rim
    let d = |x: f32, y: f32| eval(&part, Vec3::new(x, y, 0.0));
    assert!(d(r, t * 0.5) > 1e-3, "pocket mouth at the face {}", d(r, t * 0.5));
    assert!(d(r, seat + 0.01) > 0.0, "pocket just above its floor {}", d(r, seat + 0.01));
    assert!(d(r, seat - 0.01) < 0.0, "floor just below the seat {}", d(r, seat - 0.01));
    assert!(d(0.0, -t * 0.5) > 1e-3, "through hole open at the bottom {}", d(0.0, -t * 0.5));
    assert!(d(head_r + 1.0, 0.0) < -0.5, "plate beyond the head {}", d(head_r + 1.0, 0.0));
}
