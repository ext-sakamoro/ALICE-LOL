//! The sample grid of a recorded coplanar pair, shared by the oracle and by the tool that records the
//! pairs from the tree before a fix: both must take the same points in the same order, because the
//! record keeps one class per point.

use alice_sdf::eval::eval;
use alice_sdf::SdfNode;
use glam::Vec3;

/// Points of the polygon on a 21 x 21 grid over its extent, each with its distance to the nearest edge
pub fn grid_points(poly: &[Vec3], n: Vec3) -> Vec<(Vec3, f32)> {
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

/// The class of a sample point in a tree: the field just off the pair's plane, both ways, `off` away.
/// `m` = a sheet inside material (negative both ways, 0 within `off` on the plane), `a` = a sheet in the
/// part's air (positive both ways), `s` = the part's surface (the sign changes across the plane),
/// `x` = not on a sheet (the plane is inside or outside the part there by more than `off`; the overlap
/// polygon is a convex hull and can cover such points)
#[allow(dead_code)] // the oracle reads classes, the recorder writes them
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
