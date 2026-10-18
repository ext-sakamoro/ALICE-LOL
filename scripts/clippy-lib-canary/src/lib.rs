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

// the rest of the glam entries, so every library-only entry has a form (a mistyped or removed
// entry forbids nothing and clippy does not say so for a method path)

pub fn glam_vec2_min(a2: Vec2, a2_o: Vec2) -> Vec2 {
    a2.min(a2_o)
}

pub fn glam_vec2_min_element(a2: Vec2) -> f32 {
    a2.min_element()
}

pub fn glam_vec2_max_element(a2: Vec2) -> f32 {
    a2.max_element()
}

pub fn glam_vec2_clamp(a2: Vec2, a2_lo: Vec2, a2_hi: Vec2) -> Vec2 {
    a2.clamp(a2_lo, a2_hi)
}

pub fn glam_vec3a_min(a3: Vec3A, a3_o: Vec3A) -> Vec3A {
    a3.min(a3_o)
}

pub fn glam_vec3a_max(a3: Vec3A, a3_o: Vec3A) -> Vec3A {
    a3.max(a3_o)
}

pub fn glam_vec3a_min_element(a3: Vec3A) -> f32 {
    a3.min_element()
}

pub fn glam_vec3a_max_element(a3: Vec3A) -> f32 {
    a3.max_element()
}

pub fn glam_vec4_min(a4: Vec4, a4_o: Vec4) -> Vec4 {
    a4.min(a4_o)
}

pub fn glam_vec4_max(a4: Vec4, a4_o: Vec4) -> Vec4 {
    a4.max(a4_o)
}

pub fn glam_vec4_max_element(a4: Vec4) -> f32 {
    a4.max_element()
}

pub fn glam_vec4_clamp(a4: Vec4, a4_lo: Vec4, a4_hi: Vec4) -> Vec4 {
    a4.clamp(a4_lo, a4_hi)
}

// ALICE-Law G10: the wall clock and OS-seeded hashing (each line one form of scripts/clippy_lib_gate.toml)
pub fn clock_instant_now() -> std::time::Instant {
    std::time::Instant::now()
}

pub fn clock_system_now() -> std::time::SystemTime {
    std::time::SystemTime::now()
}

pub fn clock_instant_elapsed(started: std::time::Instant) -> std::time::Duration {
    started.elapsed()
}

pub fn clock_system_elapsed(stamp: std::time::SystemTime) -> Result<std::time::Duration, std::time::SystemTimeError> {
    stamp.elapsed()
}

pub fn seeded_state() {
    let _ = std::collections::hash_map::RandomState::new();
}

pub fn seeded_param(_seeded: std::collections::hash_map::RandomState) {}

pub fn held_clock(later: std::time::Instant, earlier: std::time::Instant) -> std::time::Duration {
    later.duration_since(earlier)
}
