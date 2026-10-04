//! ALICE World Auditor — vocabulary types.
//!
//! This crate holds **only the shared language** (`Goal`, `Verdict`) that a
//! law checker (`alice-lol`) and a planner (`alice-world-auditor`) agree on.
//! It proves nothing and searches nothing — see `alice-world-auditor` for
//! that. Kept as a separate, permissively-licensed crate so that consuming
//! `alice-lol`'s MIT/Apache-2.0 terms never pulls in the AGPL-3.0-or-later
//! dual license carried by the planner (dependency inversion:
//! `alice-lol → alice-world-auditor-types`, the
//! planner also depends on this crate but `alice-lol` never depends on the
//! planner).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use glam::Vec3;

/// Axis-aligned bounding box, inclusive on both ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    /// Minimum corner.
    pub min: Vec3,
    /// Maximum corner.
    pub max: Vec3,
}

impl Aabb {
    /// Whether `p` lies within `self`, inclusive on both ends.
    #[must_use]
    pub fn contains(&self, p: Vec3) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }
}

/// A goal predicate.
///
/// The MVP has exactly one body
/// reach a target region, optionally at rest (velocity exactly zero).
///
/// ⚠️ **`#[non_exhaustive]` here only blocks exhaustive `match` from outside
/// this crate** — existing variants with public fields remain constructible
/// from downstream (unlike `#[non_exhaustive]` on a struct, which blocks
/// struct-literal construction entirely). No separate constructor
/// function is needed; `tests/cross_crate_construction.rs` pins this.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Goal {
    /// Position within `target`, velocity irrelevant.
    ///
    /// Used for the gravity-assisted admissibility scene (`alice-world-auditor`'s Phase 3
    /// scene B) — rest is deliberately not required there because a body
    /// falling under gravity cannot, in general, reach exactly zero
    /// velocity at an arbitrary target without an additional control axis.
    PositionWithin {
        /// Target region.
        target: Aabb,
    },
    /// Position within `target` AND velocity exactly zero (rest-to-rest).
    ///
    /// This is the MVP's primary goal: "position only" alone makes the
    /// search trivial (always accelerate toward the target), so requiring
    /// rest forces a genuine accel/decel switch point for the planner to
    /// find.
    PositionWithinAndAtRest {
        /// Target region.
        target: Aabb,
    },
}

/// The 3-valued outcome of checking a [`Goal`] against a world's trajectory.
///
/// Mirrors doctrine §3's 3-value logic (`proven` / `violated` / `undecided`)
/// — `undecided` is **not** a failure value folded into `violated`; keeping
/// it distinct is what lets a planner report "ran out of budget" without
/// claiming the goal is unreachable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verdict {
    /// The goal was reached and verified.
    Proven,
    /// The goal is, or has become, unreachable under the given law.
    Violated,
    /// Neither proven nor violated within the resources given (budget
    /// exhausted, or the trajectory diverged — see `alice-physics`'
    /// `overflow_detected`).
    Undecided,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_contains_is_inclusive_on_both_ends() {
        let b = Aabb {
            min: Vec3::new(0.0, 0.0, 0.0),
            max: Vec3::new(1.0, 1.0, 1.0),
        };
        assert!(b.contains(Vec3::new(0.0, 0.0, 0.0)));
        assert!(b.contains(Vec3::new(1.0, 1.0, 1.0)));
        assert!(b.contains(Vec3::new(0.5, 0.5, 0.5)));
        assert!(!b.contains(Vec3::new(1.000_001, 0.5, 0.5)));
        assert!(!b.contains(Vec3::new(-0.000_001, 0.5, 0.5)));
    }
}
