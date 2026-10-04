//! Confirms `#[non_exhaustive]` on `Goal` / `Verdict` does not block
//! downstream construction (the trap applies to structs with pub fields, not to enums with
//! struct-like variants; this test pins that this crate is on the safe
//! side rather than assuming it).

use alice_world_auditor_types::{Aabb, Goal, Verdict};
use glam::Vec3;

#[test]
fn goal_variants_are_constructible_from_outside_the_crate() {
    let target = Aabb {
        min: Vec3::ZERO,
        max: Vec3::ONE,
    };
    let _ = Goal::PositionWithin { target };
    let _ = Goal::PositionWithinAndAtRest { target };
    let _ = Verdict::Proven;
    let _ = Verdict::Violated;
    let _ = Verdict::Undecided;
}
