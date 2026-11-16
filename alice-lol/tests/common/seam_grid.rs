//! The sample grid of a recorded coplanar pair, shared by the oracle and by the tool that records the
//! pairs from the tree before a fix: both must take the same points in the same order, because the
//! record keeps one class per point.
//!
//! The grid is square, with the step the record states. The recorder starts at a twentieth of the
//! polygon's longer side and halves the step until every sheet class present has a point inside its
//! region (a wall 3 mm thick along a 250 mm tray is a sheet far narrower than a twentieth of the tray).
#![allow(dead_code)] // the oracle and the recorder each use a part

use alice_sdf::eval::eval;
use alice_sdf::tight_aabb::{compute_tight_aabb_with_config, TightAabbConfig};
use alice_sdf::SdfNode;
use glam::Vec3;
use std::collections::HashMap;

/// A grid point inside the polygon: position, distance to the nearest edge, grid index
pub struct GridPoint {
    pub p: Vec3,
    pub edge: f32,
    pub ij: (i32, i32),
}

/// The step the recorder starts from: a twentieth of the polygon's longer side
pub fn first_step(poly: &[Vec3], n: Vec3) -> f32 {
    let (_, _, du, dv) = frame(poly, n);
    du.max(dv) / 20.0
}

fn frame(poly: &[Vec3], n: Vec3) -> ((Vec3, Vec3, Vec3), (f32, f32), f32, f32) {
    let u = n.any_orthonormal_vector();
    let v = n.cross(u);
    let o = poly[0];
    let uv: Vec<(f32, f32)> = poly.iter().map(|p| ((*p - o).dot(u), (*p - o).dot(v))).collect();
    let (u0, u1) = uv.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.0), b.max(q.0)));
    let (v0, v1) = uv.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q.1), b.max(q.1)));
    ((o + u * u0 + v * v0, u, v), (u0, v0), u1 - u0, v1 - v0)
}

/// Points of the polygon on a square grid of the given step over its extent, row by row
pub fn grid_points(poly: &[Vec3], n: Vec3, step: f32) -> Vec<GridPoint> {
    let ((corner, u, v), _, du, dv) = frame(poly, n);
    let (nu, nv) = ((du / step).ceil() as i32, (dv / step).ceil() as i32);
    let mut out = Vec::new();
    for i in 0..=nu {
        for j in 0..=nv {
            let p = corner + u * (i as f32 * step).min(du) + v * (j as f32 * step).min(dv);
            // inside (convex polygon, either orientation), with the distance to the nearest edge
            let mut sign = 0.0_f32;
            let mut nearest = f32::MAX;
            let inside = (0..poly.len()).all(|k| {
                let (e0, e1) = (poly[k], poly[(k + 1) % poly.len()]);
                let e = (e1 - e0).normalize_or_zero();
                let side = e.cross(p - e0).dot(n);
                if sign == 0.0 && side.abs() > 1e-9 {
                    sign = side.signum();
                }
                nearest = nearest.min((p - e0 - e * (p - e0).dot(e)).length());
                side * sign >= 0.0
            });
            if inside {
                out.push(GridPoint { p, edge: nearest, ij: (i, j) });
            }
        }
    }
    out
}

/// Whether each point is inside its class region: every grid neighbour (diagonals included) that is
/// in the polygon has the same class
pub fn interior(grid: &[GridPoint], classes: &[char]) -> Vec<bool> {
    let at: HashMap<(i32, i32), char> = grid.iter().zip(classes).map(|(g, c)| (g.ij, *c)).collect();
    grid.iter()
        .zip(classes)
        .map(|(g, c)| {
            (-1..=1).all(|di| {
                (-1..=1).all(|dj| at.get(&(g.ij.0 + di, g.ij.1 + dj)).is_none_or(|k| k == c || *k == '-'))
            })
        })
        .collect()
}

/// The class of a sample point in a tree: the field just off the pair's plane, both ways, `off` away.
/// `m` = a sheet inside material (negative both ways, 0 within `off` on the plane), `a` = a sheet in the
/// part's air (positive both ways), `s` = the part's surface (the sign changes across the plane),
/// `x` = not on a sheet (the plane is inside or outside the part there by more than `off`; the overlap
/// polygon is a convex hull and can cover such points)
pub fn classify(node: &SdfNode, p: Vec3, n: Vec3, off: f32) -> char {
    let (d0, dp, dm) = (eval(node, p), eval(node, p + n * off), eval(node, p - n * off));
    if dp < 0.0 && dm < 0.0 {
        if d0.abs() <= off { 'm' } else { 'x' }
    } else if dp > 0.0 && dm > 0.0 {
        if d0.abs() <= off { 'a' } else { 'x' }
    } else {
        's'
    }
}

/// Run-length form of a class list (`m12s3`), the data file's column
pub fn encode(classes: &[char]) -> String {
    let mut out = String::new();
    let mut k = 0;
    while k < classes.len() {
        let c = classes[k];
        let run = classes[k..].iter().take_while(|x| **x == c).count();
        out.push(c);
        out += &run.to_string();
        k += run;
    }
    out
}

pub fn decode(s: &str) -> Vec<char> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let mut digits = String::new();
        while let Some(d) = chars.peek().filter(|d| d.is_ascii_digit()) {
            digits.push(*d);
            chars.next();
        }
        let run: usize = digits.parse().expect("a run length after each class");
        out.extend(std::iter::repeat_n(c, run));
    }
    out
}

/// The connected sheet regions (8-neighbour) of a class list: for each region, its class and the
/// indices of its points; `s` and `x` points are not sheets and form no region
pub fn sheet_regions(grid: &[GridPoint], classes: &[char]) -> Vec<(char, Vec<usize>)> {
    let index: HashMap<(i32, i32), usize> = grid.iter().enumerate().map(|(k, g)| (g.ij, k)).collect();
    let mut seen = vec![false; grid.len()];
    let mut out = Vec::new();
    for start in 0..grid.len() {
        let c = classes[start];
        if seen[start] || !matches!(c, 'm' | 'a') {
            continue;
        }
        let (mut members, mut stack) = (Vec::new(), vec![start]);
        seen[start] = true;
        while let Some(k) = stack.pop() {
            members.push(k);
            let (i, j) = grid[k].ij;
            for di in -1..=1 {
                for dj in -1..=1 {
                    if let Some(&q) = index.get(&(i + di, j + dj)) {
                        if !seen[q] && classes[q] == c {
                            seen[q] = true;
                            stack.push(q);
                        }
                    }
                }
            }
        }
        out.push((c, members));
    }
    out
}

/// Floor of the check's margin: this many ulp at the part's extent, 0.015 mm at 250 mm (the largest part);
/// below it a fix cannot be told from "0 within rounding"
pub const DELTA_FLOOR_ULPS: f32 = 1000.0;

/// The margin of a pair: `δ` = 1e-3 of the part's extent, or a quarter of the polygon's inradius for a
/// strip thinner than that (across a thin strip the field cannot go below about half its width); with
/// the plain value, the floor and the inradius (measured on a grid of a twentieth of the polygon's
/// shorter side). Points closer than 2δ to the polygon's edges are not checked: there the field is
/// pulled to 0 by the part's outer surface
pub struct Margin {
    pub delta: f32,
    pub plain: f32,
    pub floor: f32,
    pub inradius: f32,
}

pub fn margin(node: &SdfNode, poly: &[Vec3], n: Vec3) -> Margin {
    let b = compute_tight_aabb_with_config(node, &TightAabbConfig::preset_large());
    let extent = (b.max - b.min).max_element();
    let (_, _, du, dv) = frame(poly, n);
    let fine = (du.min(dv) / 20.0).max(du.max(dv) / 2000.0);
    let inradius = grid_points(poly, n, fine).iter().map(|g| g.edge).fold(0.0_f32, f32::max);
    let plain = 1e-3 * extent;
    Margin {
        delta: plain.min(0.25 * inradius),
        plain,
        floor: DELTA_FLOOR_ULPS * (extent.next_up() - extent),
        inradius,
    }
}

/// The recorded class of each grid point: `classify`, or `-` closer than 2δ to the polygon's edges
pub fn classes(node: &SdfNode, grid: &[GridPoint], n: Vec3, delta: f32, off: f32) -> Vec<char> {
    grid.iter()
        .map(|g| if g.edge < 2.0 * delta { '-' } else { classify(node, g.p, n, off) })
        .collect()
}
