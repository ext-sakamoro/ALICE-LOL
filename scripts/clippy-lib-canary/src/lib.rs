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

// G10: a wall-clock read in library code (the configuration also lists unseeded RNG and
// fresh UUID constructors, which this canary cannot call without those crates)

pub fn wall_clock_instant() -> std::time::Instant {
    std::time::Instant::now()
}

pub fn wall_clock_system() -> std::time::SystemTime {
    std::time::SystemTime::now()
}
