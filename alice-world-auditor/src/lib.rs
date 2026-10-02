//! ALICE World Auditor — deterministic planner.
//!
//! Searches for a sequence of actions that drives an `alice-physics` world
//! toward a [`Goal`](alice_world_auditor_types::Goal), returning a
//! [`Verdict`](alice_world_auditor_types::Verdict)-compatible 3-valued
//! outcome. See `project_alice_world_model_mvp_plan` (`~/claude-config/memory/`)
//! for the full design; this crate currently implements **Phase 2 only**
//! (public entry points + oracle tests, search body not yet written — see
//! `plan` / `lower_bound_frames`).
//!
//! ⚠️ **Phase 4 (IDA\* body) is not implemented.** `plan` and
//! `lower_bound_frames` fail fast with `todo!()`. The oracle tests in
//! `tests/` are `#[ignore = "src gap: ..."]` for exactly that reason — they
//! pin the expected closed-form answers now, before the search exists, per
//! this workspace's oracle-first discipline.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub use alice_world_auditor_types::{Aabb, Goal, Verdict};

/// 1-D kinematic state along the goal-relevant axis (position, velocity).
///
/// The MVP action space (`project_alice_world_model_mvp_plan` §3 Phase 1.3)
/// is 1 axis × `{+a_max, -a_max, 0}`, so the heuristic only ever needs a
/// 1-D projection of the body's state — not a full 3-D `PhysicsWorld`
/// snapshot. This keeps [`lower_bound_frames`] usable without the
/// `physics` feature.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisState {
    /// Signed distance to the goal position along the axis.
    pub distance_to_goal: f32,
    /// Signed velocity along the axis.
    pub velocity: f32,
}

/// Search parameters.
///
/// `a_max_axis` is supplied by the caller, not computed here — this is
/// what lets the admissibility oracle (plan §3 Phase 3 scene B) pass the
/// same [`lower_bound_frames`] two different values (`a_input` alone vs.
/// `a_input + |g_axis|`) and observe one admissible, one not.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Params {
    /// Maximum acceleration magnitude along the goal axis.
    pub a_max_axis: f32,
    /// Simulation timestep (seconds).
    pub dt: f32,
    /// Maximum search nodes to expand before returning [`BestSoFar`]
    /// instead of [`Optimal`].
    pub node_budget: u64,
}

impl Params {
    /// Construct `Params` from its three fields.
    ///
    /// ⚠️ **Required because of `#[non_exhaustive]`** — unlike an enum, a
    /// `#[non_exhaustive]` struct with public fields cannot be built via
    /// struct-literal syntax from outside this crate (`E0639`,
    /// `feedback_non_exhaustive_without_constructor`); `Params` is a
    /// caller-constructed input type, so it needs this constructor (as
    /// opposed to `Optimal` / `BestSoFar`, which this crate creates and
    /// downstream only reads).
    #[must_use]
    pub const fn new(a_max_axis: f32, dt: f32, node_budget: u64) -> Self {
        Self {
            a_max_axis,
            dt,
            node_budget,
        }
    }
}

/// One of the three MVP actions (plan §3 Phase 1.3: `b = 3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// Accelerate toward the goal (`+a_max`).
    Accelerate,
    /// Accelerate away from the goal / brake (`-a_max`).
    Decelerate,
    /// No acceleration input.
    Coast,
}

/// A plan found to be optimal (frame-minimal) within the search's
/// admissible heuristic.
///
/// ⚠️ **Construct only from within this crate** — downstream reads this,
/// it does not build one (see `feedback_non_exhaustive_without_constructor`
/// §1's "crate が作って downstream が読む型" exemption).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Optimal {
    /// Minimal frame count to reach the goal.
    pub frames: u32,
    /// The action taken on each frame, in order.
    pub actions: Vec<Action>,
}

/// The best plan found before the node budget ran out — **not** claimed to
/// be optimal (plan §0: "予算超過時に最適性を名乗らない").
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct BestSoFar {
    /// Frame count of the best plan found so far.
    pub frames: u32,
    /// The action taken on each frame, in order.
    pub actions: Vec<Action>,
}

/// L∞ admissible lower bound on the number of frames needed to reach the
/// goal from `state`, given `params`.
///
/// # Admissibility
///
/// Must never exceed the true minimal frame count, or IDA\* built on top of
/// it is unsound. The MVP's bang-bang closed form is
/// `t* = 2 * sqrt(distance / a)`; `params.a_max_axis` must already include
/// any assisting/opposing acceleration (e.g. gravity) the caller wants
/// accounted for — see `tests/phase3_search_oracles.rs` scene B for the
/// admissible/inadmissible pair this enables.
///
/// # Panics
///
/// Always, currently: `todo!()` — Phase 4 (plan §3) has not been
/// implemented. This function's *signature* and the oracle tests calling
/// it are the Phase 2/3 deliverable; the body is Phase 4.
#[must_use]
pub fn lower_bound_frames(_state: AxisState, _params: &Params) -> u32 {
    todo!("STUB: Phase 4 (project_alice_world_model_mvp_plan §3 Phase 4.1) — L∞ bang-bang lower bound not yet implemented")
}

/// Search for a frame-minimal action sequence from `world`'s current state
/// to `goal`.
///
/// # Errors
///
/// Returns `Err(BestSoFar)` if the node budget is exhausted before an
/// optimal plan is found or proven unreachable.
///
/// # Panics
///
/// Always, currently: `todo!()` — see [`lower_bound_frames`].
#[cfg(feature = "physics")]
pub fn plan(
    _world: &mut alice_physics::PhysicsWorld,
    _goal: &Goal,
    _params: &Params,
) -> Result<Optimal, BestSoFar> {
    todo!("STUB: Phase 4 (project_alice_world_model_mvp_plan §3 Phase 4.2) — IDA* search not yet implemented")
}
