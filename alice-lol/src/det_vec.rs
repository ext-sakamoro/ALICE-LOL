//! Lane-wise min / max of glam vectors through this crate's Law helpers
//!
//! `glam`'s `Vec3::min` / `max` / `min_element` / `max_element` / `clamp` are the
//! platform f32 min / max per lane, which leave the sign of an equal pair of
//! zeros unspecified (`aarch64` and `x86_64` differ). The library code uses these
//! helpers instead; `.github/clippy-lib/clippy.toml` bans the glam forms.

use crate::det_math;
use glam::Vec3;

/// Lane-wise minimum, each lane by [`det_math::min`]
#[must_use]
pub fn min(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        det_math::min(a.x, b.x),
        det_math::min(a.y, b.y),
        det_math::min(a.z, b.z),
    )
}

/// Lane-wise maximum, each lane by [`det_math::max`]
#[must_use]
pub fn max(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        det_math::max(a.x, b.x),
        det_math::max(a.y, b.y),
        det_math::max(a.z, b.z),
    )
}

/// The smallest lane, folded as `min(min(x, y), z)`
#[must_use]
pub fn min_element(v: Vec3) -> f32 {
    det_math::min(det_math::min(v.x, v.y), v.z)
}

/// The largest lane, folded as `max(max(x, y), z)`
#[must_use]
pub fn max_element(v: Vec3) -> f32 {
    det_math::max(det_math::max(v.x, v.y), v.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(v: Vec3) -> [u32; 3] {
        [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
    }

    /// Each lane follows the scalar Law helper, including the equal-zero pairs
    #[test]
    fn lanes_follow_the_scalar_helpers_on_the_signed_zero_lattice() {
        let zs = [0.0f32, -0.0, 1.0, -1.0];
        for &a in &zs {
            for &b in &zs {
                let p = Vec3::new(a, b, a);
                let q = Vec3::new(b, a, b);
                assert_eq!(
                    bits(min(p, q)),
                    [
                        det_math::min(a, b).to_bits(),
                        det_math::min(b, a).to_bits(),
                        det_math::min(a, b).to_bits()
                    ]
                );
                assert_eq!(
                    bits(max(p, q)),
                    [
                        det_math::max(a, b).to_bits(),
                        det_math::max(b, a).to_bits(),
                        det_math::max(a, b).to_bits()
                    ]
                );
            }
        }
    }

    /// The element folds use the documented order on equal zeros of different sign
    #[test]
    fn element_folds_have_the_documented_order() {
        let v = Vec3::new(0.0, -0.0, 3.0);
        assert_eq!(
            min_element(v).to_bits(),
            det_math::min(det_math::min(0.0, -0.0), 3.0).to_bits()
        );
        assert_eq!(
            max_element(Vec3::new(-0.0, 0.0, -2.0)).to_bits(),
            det_math::max(det_math::max(-0.0, 0.0), -2.0).to_bits()
        );
        assert_eq!(
            min_element(Vec3::new(4.0, -1.5, 2.0)).to_bits(),
            (-1.5f32).to_bits()
        );
        assert_eq!(
            max_element(Vec3::new(4.0, -1.5, 2.0)).to_bits(),
            4.0f32.to_bits()
        );
    }
}
