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
//! The pairs the corpus had before the fix, at the corpus's numbers and at each part's printed size
//! (`template_sizes.txt`, named `<template>@size`), are recorded in
//! `tests/data/coplanar_pairs_at_e21b5c2.txt`: template, kind, the side at the overlap's centroid (kept
//! for reading only), one class per sample point, normal, overlap polygon. The classes are read from the
//! tree before the fix, the field just off the plane both ways at each point of the shared grid
//! (`common/seam_grid.rs`): `m` a sheet inside material, `a` a sheet in the part's air (a cut's mouth,
//! a seam inside a cutter), `s` a real surface, `x` not on a sheet (the overlap polygon is a convex hull
//! and can cover points where the two faces never met, such as a boss's bore inside its ring seam).
//! Per point, a former material sheet must now be material (`≤ −δ`) and a former air sheet air
//! (`≥ +δ`), sampled at least `2δ` in from the polygon's edges; the deepest point of each kind also
//! takes the inside / outside test at `SURFACE_HIT_EPS`. A per-pair verdict would let a sheet hide
//! behind a surface majority, or a surface fail a sheet's test. The classes are recorded once, from the
//! tree before the fix; a fix that legitimately moves a point to another class fails here and is
//! re-recorded by hand in the data file with its reason on a `#` line (never regenerated
//! automatically, which would accept any change). `δ` is `1e-3` of the part's extent (a quarter of the
//! polygon's inradius for a strip thinner than that, never below `DELTA_FLOOR_ULPS` ulp at the extent:
//! a strip too thin for that fails as "overlap too thin to verify"): well above f32 rounding at the
//! largest part and well below the overlaps the fixes add (each stated at its constant).
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
#[path = "common/seam_grid.rs"]
mod seam_grid;
use seam_grid::grid_points;
use alice_lol::runtime_parser::parse_lol;
use alice_lol::stdlib::hardsurface::fastener::{bolt, counterbore, counterbore_depth, MetricSize};
use alice_lol::stdlib::hardsurface::joint::{
    snap_fit_cantilever, t_slot_2020, SnapFitCantileverSpec, T_SLOT_2020_INNER_DEPTH, T_SLOT_2020_INNER_WIDTH,
    T_SLOT_2020_OPENING_DEPTH, T_SLOT_2020_OPENING_WIDTH,
};
use alice_lol::stdlib::hardsurface::mount::profile_2020;
use alice_lol::stdlib::hardsurface::pattern_sdf::{pen_cup, phone_stand, PenCupSpec, PhoneStandSpec};
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

/// Each part template at a size a person would print (`scripts/gen_template_sizes.py`): the corpus
/// calls templates with stand-in numbers (3, 2, 1, ...) that empty many cavities, slots and walls, so a
/// check run only there misses the shape that is printed (pen_cup's membrane over its mouth was found
/// at its real size, not at the corpus numbers)
fn template_sizes() -> Vec<(String, String)> {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/template_sizes.txt"))
        .expect("tests/data/template_sizes.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let call = l.split("  #").next().unwrap().trim().to_string();
            (format!("{}@size", call.split('(').next().unwrap()), call)
        })
        .collect()
}

#[test]
fn no_template_has_a_seam_or_a_flush_cut() {
    let (mut checked, mut sized) = (0usize, 0usize);
    let mut offenders = Vec::new();
    let mut unparsed_sizes = Vec::new();
    let sizes = template_sizes();
    for (name, src) in corpus().into_iter().chain(sizes.iter().cloned()) {
        let at_size = name.ends_with("@size");
        let Ok(node) = parse_lol(&src) else {
            if at_size { unparsed_sizes.push(src); }
            continue;
        };
        checked += 1;
        sized += usize::from(at_size);
        let r = coplanar_faces(&node);
        let base = name.trim_end_matches("@size");
        if !r.pairs.is_empty() && !ALLOWED.iter().any(|(n, _)| *n == base) {
            let kinds: Vec<String> = r.pairs.iter().map(|p| format!("{:?}@{:?}", p.kind, p.point)).collect();
            offenders.push(format!("{name}: {}", kinds.join(", ")));
        }
    }
    assert!(checked > 200, "only {checked} templates parsed");
    assert!(sized > 80, "only {sized} templates checked at a printed size");
    assert!(unparsed_sizes.is_empty(), "size rows that do not parse: {unparsed_sizes:?}");
    // every part template (a composite whose corpus call takes numbers only, not child shapes) has a
    // size row, or takes no arguments (its spec's defaults); operators take their children's shape
    let mut without = Vec::new();
    for (name, src) in common::corpus::grammar_corpus() {
        let Ok(node) = parse_lol(&src) else { continue };
        let r = coplanar_faces(&node);
        let composite = r.ops_analysed + r.ops_not_analysed > 0;
        let args = &src[src.find('(').map_or(src.len(), |i| i + 1)..];
        let numbers_only = !args.contains('(');
        let no_args = args.trim() == ")";
        if composite && numbers_only && !no_args && !sizes.iter().any(|(n, _)| n.trim_end_matches("@size") == name) {
            without.push(name);
        }
    }
    assert!(without.is_empty(), "part templates without a size row: {without:?}");
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
    /// the class of each sample point of `grid_points` before the fix (`seam_grid::classify`): `m` a
    /// sheet in material, `a` a sheet in air, `s` a real surface, `x` not on a sheet (the convex-hull
    /// polygon can cover points where the faces did not meet, e.g. a boss's bore inside its ring seam)
    classes: Vec<char>,
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
            let _side_at_centroid = it.next().unwrap();
            let classes = it.next().unwrap().chars().collect();
            let v: Vec<f32> = it.map(|x| x.parse().unwrap()).collect();
            let n = Vec3::new(v[0], v[1], v[2]);
            let poly = v[3..].chunks_exact(3).map(|c| Vec3::new(c[0], c[1], c[2])).collect();
            Recorded { name, kind, classes, n, poly }
        })
        .collect()
}

/// The spacing of a recorded pair's grid (the smallest distance between two of its points)
fn grid_step(grid: &[(Vec3, f32)]) -> f32 {
    let mut step = f32::MAX;
    for (i, (p, _)) in grid.iter().enumerate() {
        for (q, _) in &grid[i + 1..] {
            let d = (*q - *p).length();
            if d > 0.0 { step = step.min(d); }
        }
    }
    step
}

#[test]
fn every_recorded_seam_is_material_and_every_recorded_cut_is_air() {
    let mut corpus = corpus();
    corpus.extend(template_sizes());
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
        if r.classes.len() != grid.len() {
            failures.push(format!("{} {}: {} recorded classes for {} grid points", r.name, r.kind, r.classes.len(), grid.len()));
            continue;
        }
        // a point is checked only inside its sheet region, as it is only inside the polygon: at least 2δ
        // from the polygon's edges, and every grid neighbour (one grid step, diagonals included) of the
        // same class (next to a bore or a real surface the field is pulled toward 0 by that surface)
        let step = grid_step(&grid);
        let deep: Vec<(Vec3, char, f32)> = grid
            .iter()
            .zip(&r.classes)
            .filter(|((_, d), _)| *d >= 2.0 * delta)
            .filter(|((p, _), c)| {
                grid.iter().zip(&r.classes).all(|((q, _), k)| k == *c || (*q - *p).length() > 1.5 * step)
            })
            .map(|((p, d), c)| (*p, *c, *d))
            .collect();
        let sheet_points = r.classes.iter().filter(|c| matches!(c, 'm' | 'a')).count();
        if sheet_points > 0 && !deep.iter().any(|(_, c, _)| matches!(c, 'm' | 'a')) {
            failures.push(format!(
                "{} {}: {sheet_points} sheet points, none inside its region (a sheet narrower than the grid)",
                r.name, r.kind
            ));
            continue;
        }
        let pts: Vec<Vec3> = deep.iter().map(|(p, _, _)| *p).collect();
        if pts.is_empty() {
            failures.push(format!("{} {}: polygon has no point 2δ inside (inradius {inradius:e})", r.name, r.kind));
            continue;
        }
        if r.kind == "UnionSeam" { seams += 1; } else { cuts += 1; }
        // each point is held to its own class: a material sheet must be material now (<= -δ), an air
        // sheet air (>= +δ); surface and off-sheet points are not sheets and are not checked
        let mut bad_m = (0usize, f32::MIN);
        let mut bad_a = (0usize, f32::MAX);
        let (mut n_m, mut n_a) = (0usize, 0usize);
        for (p, c, _) in &deep {
            let d = eval(&node, *p);
            match c {
                'm' => { n_m += 1; if d > -delta { bad_m = (bad_m.0 + 1, bad_m.1.max(d)); } }
                'a' => { n_a += 1; if d < delta { bad_a = (bad_a.0 + 1, bad_a.1.min(d)); } }
                _ => {}
            }
        }
        if bad_m.0 > 0 {
            failures.push(format!("{} {} (material sheet): {} of {n_m} points above -δ = {:e}, max {:e}", r.name, r.kind, bad_m.0, -delta, bad_m.1));
        }
        if bad_a.0 > 0 {
            failures.push(format!("{} {} (air sheet): {} of {n_a} points below δ = {delta:e}, min {:e}", r.name, r.kind, bad_a.0, bad_a.1));
        }
        // the inside / outside test at the sheet's deepest point (farthest from the polygon's edges)
        let deepest = |want: char| deep.iter().filter(|(_, c, _)| *c == want).max_by(|a, b| a.2.total_cmp(&b.2)).map(|(p, _, _)| *p);
        if let Some(p) = deepest('m') {
            let d = eval(&node, p);
            if d < -SURFACE_HIT_EPS { seams_inside += 1; } else {
                failures.push(format!("{} {} (material sheet): inside test at {p} reads {d:e}", r.name, r.kind));
            }
        }
        if let Some(p) = deepest('a') {
            let d = eval(&node, p);
            if d > SURFACE_HIT_EPS { cleared_air += 1; } else {
                failures.push(format!("{} {} (air sheet): outside test at {p} reads {d:e}", r.name, r.kind));
            }
        }
        if n_m + n_a == 0 {
            on_surface += 1;
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

#[test]
fn the_snap_fit_hook_has_no_seam_and_keeps_its_shape() {
    // PLA_STANDARD: beam x in [-5, 5], y in [-1, 1], z in [-2.5, 2.5]; hook x in [3.5, 5] on the beam's
    // top, up to y = 1.5. The hook stood on the beam's face (a seam over its footprint)
    let spec = SnapFitCantileverSpec::PLA_STANDARD;
    let part = snap_fit_cantilever(spec);
    assert!(coplanar_faces(&part).pairs.is_empty(), "{:?}", coplanar_faces(&part).pairs);
    let on = |x: f32, y: f32| eval(&part, Vec3::new(x, y, 0.0)).abs() < 1e-5;
    assert!(on(4.25, 1.5), "hook top");
    assert!(on(5.0, 1.25), "hook front");
    assert!(on(3.5, 1.25), "hook back");
    assert!(on(2.0, 1.0), "beam top beside the hook");
    let old_seam = eval(&part, Vec3::new(4.25, 1.0, 0.0));
    assert!(old_seam < -0.4, "the old seam is inside now: {old_seam}");
    // outside the part the field is the two-box construction's (the hook grew only into the beam)
    let beam = SdfNode::box3d(spec.length, spec.thickness, spec.width);
    let hook = SdfNode::box3d(spec.hook_offset, spec.hook_height, spec.width).translate(
        spec.hook_offset.mul_add(-0.5, spec.length * 0.5),
        spec.hook_height.mul_add(0.5, spec.thickness * 0.5),
        0.0,
    );
    let old = beam.union(hook);
    let mut compared = 0;
    for i in -14..=14 {
        for j in -6..=6 {
            for k in -8..=8 {
                let p = Vec3::new(i as f32 * 0.43, j as f32 * 0.37, k as f32 * 0.41);
                let o = eval(&old, p);
                if o > 1e-3 {
                    compared += 1;
                    let n = eval(&part, p);
                    assert!((n - o).abs() <= 1e-5, "at {p}: {n} vs {o}");
                }
            }
        }
    }
    assert!(compared > 1000, "only {compared} outside points compared");
}

#[test]
fn the_pen_cup_mouth_is_open_and_its_floor_and_wall_stay() {
    // standard 75 x 100: outer z in [-50, 50]; the cavity's top lay exactly on the rim (a membrane over
    // the mouth); it now reaches past it, its floor unchanged
    let spec = PenCupSpec::standard_75x100();
    let cup = pen_cup(&spec);
    assert!(coplanar_faces(&cup).pairs.is_empty(), "{:?}", coplanar_faces(&cup).pairs);
    let h = spec.height * 0.5;
    let floor = -h + spec.floor_thickness;
    let d = |x: f32, z: f32| eval(&cup, Vec3::new(x, 0.0, z));
    assert!(d(0.0, h) > 1e-3, "mouth at the rim {}", d(0.0, h));
    assert!(d(10.0, floor + 0.01) > 0.0, "just above the floor {}", d(10.0, floor + 0.01));
    assert!(d(10.0, floor - 0.01) < 0.0, "just below the floor {}", d(10.0, floor - 0.01));
    let wall_mid = spec.inner_diameter * 0.5 + spec.wall_thickness * 0.5;
    assert!(d(wall_mid, 0.0) < -0.5, "wall {}", d(wall_mid, 0.0));
}

#[test]
fn the_phone_stand_slot_is_open_and_its_back_stands_in_the_base() {
    // default: base 90 x 90 x 6 (z in [0, 6]); back 5 thick (y in [-45, -40]) up to z = 106; slot 14 wide
    // cut 80 down from the top. The slot's top lay on the back's top (a membrane over it) and the back
    // stood on the base's face (a seam)
    let spec = PhoneStandSpec::phone_default();
    let stand = phone_stand(&spec);
    assert!(coplanar_faces(&stand).pairs.is_empty(), "{:?}", coplanar_faces(&stand).pairs);
    let top = spec.base_thickness + spec.back_height;
    let slot_floor = top - spec.back_height * 0.8;
    let y = -spec.base_depth * 0.5 + spec.back_thickness * 0.5;
    let d = |x: f32, z: f32| eval(&stand, Vec3::new(x, y, z));
    assert!(d(0.0, top) > 1e-3, "slot mouth at the top {}", d(0.0, top));
    assert!(d(0.0, slot_floor + 0.01) > 0.0, "just above the slot floor {}", d(0.0, slot_floor + 0.01));
    assert!(d(0.0, slot_floor - 0.01) < 0.0, "just below the slot floor {}", d(0.0, slot_floor - 0.01));
    assert!(d(spec.slot_width * 0.5 + 3.0, 60.0) < -0.5, "wall beside the slot {}", d(spec.slot_width * 0.5 + 3.0, 60.0));
    let seam = d(20.0, spec.base_thickness);
    assert!(seam < -0.4, "the old base / back seam is inside now: {seam}");
}

#[test]
fn the_can_rack_shelves_back_and_lips_reach_into_the_side_walls() {
    // can_rack(2, 66, 10): shelf_width 74 between side walls 3 thick (inner faces at x = +-37, outer at
    // +-40). The back wall, the shelves and the lips stopped exactly at the walls' inner faces (14 seams)
    let rack = parse_lol("can_rack(2, 66, 10)").unwrap();
    assert!(coplanar_faces(&rack).pairs.is_empty(), "{:?}", coplanar_faces(&rack).pairs);
    // the old seams: every recorded can_rack pair's centre is inside now (the recorded-pair gate checks
    // the whole polygons; here the shape around them)
    let d = |x: f32, y: f32, z: f32| eval(&rack, Vec3::new(x, y, z));
    // the walls' outer faces did not move, and between the tiers the walls' inner faces are still faces
    assert!(d(40.0, 0.0, 40.0).abs() < 1e-4, "outer wall face {}", d(40.0, 0.0, 40.0));
    assert!(d(40.5, 0.0, 40.0) > 0.4, "air outside the wall {}", d(40.5, 0.0, 40.0));
    let gap_z = 3.0 + 81.0 * 0.5 + 10.0; // above the first shelf, below the second
    assert!(d(36.5, 0.0, gap_z) > 0.4, "air inside the wall between the tiers {}", d(36.5, 0.0, gap_z));
    assert!(d(37.5, 0.0, gap_z) < -0.4, "the wall itself {}", d(37.5, 0.0, gap_z));
}

#[test]
fn the_sock_divider_partitions_reach_into_the_floor_and_the_walls() {
    // sock_divider(4, 80, 89): 4 cells 80 wide, partitions 2.5 thick at x = -82.5, 0, 82.5, height 89
    // over a 2 mm floor (z in [-45.5, 45.5] after the Z-up turn), cells 100 deep between 2.5 walls.
    // Each partition stood on the floor and stopped at the front and back walls' inner faces (6 seams)
    let part = parse_lol("sock_divider(4, 80, 89)").unwrap();
    assert!(coplanar_faces(&part).pairs.is_empty(), "{:?}", coplanar_faces(&part).pairs);
    let d = |x: f32, y: f32, z: f32| eval(&part, Vec3::new(x, y, z));
    assert!(d(-123.0, 0.0, 0.0) > 0.4, "a cell is air {}", d(-123.0, 0.0, 0.0));
    assert!(d(-82.5, 0.0, 0.0) < -0.4, "a partition is material {}", d(-82.5, 0.0, 0.0));
    assert!(d(-82.5, 0.0, 45.6) > 0.0, "air just above the partition's top {}", d(-82.5, 0.0, 45.6));
    assert!(d(-82.5, 0.0, 45.4) < 0.0, "the partition up to the top {}", d(-82.5, 0.0, 45.4));
    assert!(d(-80.5, 0.0, 0.0) > 0.4, "the cell beside the partition {}", d(-80.5, 0.0, 0.0));
}

#[test]
fn the_boss_array_bosses_stand_in_the_plate_and_keep_their_bores() {
    // boss_array at its printed size (template_sizes.txt): the bosses stood on the plate's top face (a
    // ring seam under each); each boss now reaches into the plate, the bore still ends at the plate's
    // top (the plate fills the barrel's lower part) and the plate's underside is untouched
    let call = template_sizes().into_iter().find(|(n, _)| n == "boss_array@size").expect("boss_array row").1;
    let part = parse_lol(&call).unwrap();
    assert!(coplanar_faces(&part).pairs.is_empty(), "{call}: {:?}", coplanar_faces(&part).pairs);
    // m3_2x2: plate 2 thick (z in [-1, 1] after the Z-up turn), bosses 8 tall above it at (+-10, +-10)
    let d = |x: f32, y: f32, z: f32| eval(&part, Vec3::new(x, y, z));
    assert!(d(10.0, 10.0, 5.0) > 0.3, "the bore is open {}", d(10.0, 10.0, 5.0));
    assert!(d(10.0, 10.0, 1.05) > 0.0, "the bore reaches the plate's top {}", d(10.0, 10.0, 1.05));
    assert!(d(10.0, 10.0, 0.5) < -0.3, "the plate under the bore is solid {}", d(10.0, 10.0, 0.5));
    assert!(d(0.0, 0.0, -1.0).abs() < 1e-4, "the plate's underside {}", d(0.0, 0.0, -1.0));
    assert!(d(0.0, 0.0, 1.0).abs() < 1e-4, "the plate's top between the bosses {}", d(0.0, 0.0, 1.0));
    let m = coplanar_faces(&part);
    assert!(m.ops_not_analysed == 0, "{call}: {} operations not analysed", m.ops_not_analysed);
}

#[test]
fn the_bolt_shank_reaches_into_its_head() {
    // M4 x 20: shank y in [-10, 10] radius 2, socket head 7 across on top (y in [10, 10 + h]). The head
    // stood on the shank's end (a seam over the shank's disc)
    let (size, len) = (MetricSize::M4, 20.0_f32);
    let b = bolt(size, len);
    assert!(coplanar_faces(&b).pairs.is_empty(), "{:?}", coplanar_faces(&b).pairs);
    let (h, hr, sr) = (size.head_height_socket(), size.head_diameter_socket() * 0.5, size.nominal_diameter() * 0.5);
    let d = |x: f32, y: f32| eval(&b, Vec3::new(x, y, 0.0));
    assert!(d(0.0, len * 0.5) < -0.5, "the old seam is inside {}", d(0.0, len * 0.5));
    assert!(d(0.0, len * 0.5 + h).abs() < 1e-4, "head top {}", d(0.0, len * 0.5 + h));
    assert!(d(0.0, -len * 0.5).abs() < 1e-4, "shank end {}", d(0.0, -len * 0.5));
    assert!(d(sr, 0.0).abs() < 1e-4, "shank side {}", d(sr, 0.0));
    assert!(d(hr, len * 0.5 + h * 0.5).abs() < 1e-4, "head side {}", d(hr, len * 0.5 + h * 0.5));
    assert!(d((sr + hr) * 0.5, len * 0.5 - 0.1) > 0.05, "air under the head's rim {}", d((sr + hr) * 0.5, len * 0.5 - 0.1));
}

#[test]
fn the_t_slot_cutter_opening_reaches_into_its_chamber() {
    // the T-slot cutter: opening 6 wide, x in [-2.5, 2.5]; chamber 11 wide, x in [-8.5, -2.5]. They met
    // at x = -2.5 (a seam inside the cutter, the part's air once cut)
    let c = t_slot_2020(100.0);
    assert!(coplanar_faces(&c).pairs.is_empty(), "{:?}", coplanar_faces(&c).pairs);
    let top = T_SLOT_2020_OPENING_DEPTH * 0.5;
    let floor = top - T_SLOT_2020_OPENING_DEPTH - T_SLOT_2020_INNER_DEPTH;
    let d = |x: f32, z: f32| eval(&c, Vec3::new(x, 0.0, z));
    assert!(d(-top, 0.0) < -0.5, "the old seam is inside {}", d(-top, 0.0));
    assert!(d(top, 0.0).abs() < 1e-4, "the mouth's face {}", d(top, 0.0));
    assert!(d(floor, 0.0).abs() < 1e-4, "the chamber's floor {}", d(floor, 0.0));
    assert!(d(0.0, T_SLOT_2020_OPENING_WIDTH * 0.5).abs() < 1e-4, "the opening's side");
    assert!(d(-top - 1.0, T_SLOT_2020_INNER_WIDTH * 0.5).abs() < 1e-4, "the chamber's side");
    assert!(d(0.0, (T_SLOT_2020_OPENING_WIDTH + T_SLOT_2020_INNER_WIDTH) * 0.25) > 0.5, "beside the opening");
}

#[test]
fn the_tape_dispenser_back_wall_reaches_into_the_base_and_the_hood() {
    // tape_dispenser(76, 50, 3) after the Z-up turn: base z in [0, 3], back wall from the base's top to
    // the hood's underside; it stood on the base and met the hood face to face (4 seams)
    let part = parse_lol("tape_dispenser(76, 50, 3)").unwrap();
    assert!(coplanar_faces(&part).pairs.is_empty(), "{:?}", coplanar_faces(&part).pairs);
    let d = |x: f32, y: f32, z: f32| eval(&part, Vec3::new(x, y, z));
    assert!(d(0.0, 0.0, 0.0).abs() < 1e-4, "the base's underside {}", d(0.0, 0.0, 0.0));
    assert!(d(0.0, 0.0, 1.5) < -1.0, "the base {}", d(0.0, 0.0, 1.5));
    assert!(d(40.0, 0.0, 20.0) > 1.0, "the roll's space {}", d(40.0, 0.0, 20.0));
}

#[test]
fn the_sbc_case_standoffs_stand_in_the_floor() {
    // sbc_case(85, 56, 25) after the Z-up turn: floor top at z = -11, standoffs 6 across and 5 tall at
    // (+-39, +-24.5) with a 2.2 pilot hole; each standoff stood on the floor's top (4 ring seams)
    let part = parse_lol("sbc_case(85, 56, 25)").unwrap();
    assert!(coplanar_faces(&part).pairs.is_empty(), "{:?}", coplanar_faces(&part).pairs);
    let d = |x: f32, y: f32, z: f32| eval(&part, Vec3::new(x, y, z));
    assert!(d(0.0, 0.0, -11.0).abs() < 1e-4, "the floor's top {}", d(0.0, 0.0, -11.0));
    let wall_r = (1.1 + 3.0) * 0.5; // between the pilot hole and the standoff's side
    assert!(d(39.0 + wall_r, 24.5, -8.5) < -0.3, "the standoff's wall {}", d(39.0 + wall_r, 24.5, -8.5));
    assert!(d(39.0 + wall_r, 24.5, -5.9) > 0.0, "air above the standoff {}", d(39.0 + wall_r, 24.5, -5.9));
    assert!(d(39.0, 24.5, -8.5) > 0.3, "the pilot hole {}", d(39.0, 24.5, -8.5));
}

#[test]
fn the_spice_rack_lip_stands_in_the_base() {
    // spice_rack(6, 48, 100) after the Z-up turn: base top at z = -5, the front lip (y in [31.5, 34.5])
    // stood on it (1 seam over the lip's footprint)
    let part = parse_lol("spice_rack(6, 48, 100)").unwrap();
    assert!(coplanar_faces(&part).pairs.is_empty(), "{:?}", coplanar_faces(&part).pairs);
    let d = |x: f32, y: f32, z: f32| eval(&part, Vec3::new(x, y, z));
    assert!(d(0.0, 33.0, -5.0) < -0.3, "the old seam is inside {}", d(0.0, 33.0, -5.0));
    assert!(d(0.0, 30.0, -4.0) > 0.3, "air behind the lip above the base {}", d(0.0, 30.0, -4.0));
}

#[test]
fn the_cable_grommet_body_reaches_into_its_flange() {
    // cable_grommet(60, 15) after the Z-up turn: body r 30, z in [-7.5, 7.5]; flange r 34, 3 thick on
    // top (z in [7.5, 10.5]); bore r 25. The body's top met the flange's underside (a ring seam)
    let part = parse_lol("cable_grommet(60, 15)").unwrap();
    assert!(coplanar_faces(&part).pairs.is_empty(), "{:?}", coplanar_faces(&part).pairs);
    let d = |x: f32, z: f32| eval(&part, Vec3::new(x, 0.0, z));
    assert!(d(27.5, 7.5) < -0.3, "the old seam is inside {}", d(27.5, 7.5));
    assert!(d(32.0, 7.5).abs() < 1e-4, "the flange's underside beyond the body {}", d(32.0, 7.5));
    assert!(d(32.0, 7.0) > 0.3, "air under the flange {}", d(32.0, 7.0));
    assert!(d(30.0, 0.0).abs() < 1e-4, "the body's side {}", d(30.0, 0.0));
    assert!(d(10.0, 7.5) > 5.0, "the bore {}", d(10.0, 7.5));
}
