//! Each function uses one form that `.github/clippy-lib/clippy.toml` forbids
//! `scripts/clippy_lib_gate.py` runs clippy with that configuration and
//! requires a `disallowed_methods` diagnostic for every one of them

pub fn method_f32(a: f32, b: f32) -> f32 {
    a.max(b)
}

pub fn path_f32(a: f32, b: f32) -> f32 {
    f32::min(a, b)
}

pub fn method_f64(a: f64, b: f64) -> f64 {
    a.min(b)
}

pub fn fold_f64(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

// representative libm forms, to prove the generated lib config still carries
// the full crate-root libm list (alice-lol/clippy.toml), not just the max/min
// entries this script adds on top of it

pub fn libm_sin_cos(x: f32) -> (f32, f32) {
    x.sin_cos()
}

pub fn libm_atan2(y: f32, x: f32) -> f32 {
    y.atan2(x)
}

pub fn libm_tan(z: f32) -> f32 {
    z.tan()
}

// glam vectors: lane-wise platform min / max, one function per form

use glam::{Vec2, Vec3, Vec3A, Vec4};

pub fn glam_min(p: Vec3, q: Vec3) -> Vec3 {
    p.min(q)
}

pub fn glam_max(p: Vec3, q: Vec3) -> Vec3 {
    p.max(q)
}

pub fn glam_min_element(p: Vec3) -> f32 {
    p.min_element()
}

pub fn glam_max_element(p: Vec3) -> f32 {
    p.max_element()
}

pub fn glam_clamp(p: Vec3, q: Vec3, r: Vec3) -> Vec3 {
    p.clamp(q, r)
}

pub fn glam_path_min(p: Vec3, q: Vec3) -> Vec3 {
    Vec3::min(p, q)
}

pub fn glam_vec2_max(u: Vec2, w: Vec2) -> Vec2 {
    u.max(w)
}

pub fn glam_vec4_min_element(h: Vec4) -> f32 {
    h.min_element()
}

pub fn glam_vec3a_clamp(k: Vec3A) -> Vec3A {
    k.clamp(k, k)
}
