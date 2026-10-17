//! Bit-level tie tests of the Law max / min through the roblox export (ALICE-Law G2), see
//! `law_max_min_ties.rs` for the other classes
#![cfg(feature = "roblox")]

use alice_lol::roblox_export::{validate_for_roblox, RobloxConfig};
use alice_sdf::mesh::{Mesh, Vertex};
use glam::Vec3;

const POS_ZERO: u32 = 0x0000_0000;

fn mesh_of(points: &[Vec3]) -> Mesh {
    Mesh {
        vertices: points.iter().map(|&p| Vertex::new(p, Vec3::Y)).collect(),
        indices: Vec::new(),
    }
}

/// Bounding size of an export (`roblox_export::validate_for_roblox`, lane-wise
/// `det_vec` min / max): a +0 / -0 pair of vertices in either order gives a +0
/// extent, and a NaN coordinate is skipped by the lane fold
#[test]
fn roblox_bounds_of_mixed_zeros_in_both_orders_and_a_nan_vertex() {
    let config = RobloxConfig::default();
    let a = validate_for_roblox(
        &mesh_of(&[Vec3::new(0.0, 0.0, 0.0), Vec3::new(-0.0, -0.0, -0.0)]),
        &config,
    );
    let b = validate_for_roblox(
        &mesh_of(&[Vec3::new(-0.0, -0.0, -0.0), Vec3::new(0.0, 0.0, 0.0)]),
        &config,
    );
    for v in [a.bounds_studs, b.bounds_studs] {
        assert_eq!([v.x.to_bits(), v.y.to_bits(), v.z.to_bits()], [POS_ZERO; 3]);
    }
    let with_nan = validate_for_roblox(
        &mesh_of(&[
            Vec3::new(f32::NAN, 1.0, 1.0),
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(3.0, 1.0, 1.0),
        ]),
        &config,
    );
    let finite = validate_for_roblox(
        &mesh_of(&[Vec3::new(1.0, 2.0, 3.0), Vec3::new(3.0, 1.0, 1.0)]),
        &config,
    );
    let (n, f) = (with_nan.bounds_studs, finite.bounds_studs);
    assert_eq!(
        [n.x.to_bits(), n.y.to_bits(), n.z.to_bits()],
        [f.x.to_bits(), f.y.to_bits(), f.z.to_bits()]
    );
}
