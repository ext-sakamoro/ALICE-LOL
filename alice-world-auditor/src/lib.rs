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
//! Phase 4 (`project_alice_world_model_phase4_design_confirmed`,
//! `~/claude-config/memory/`) implements the search as an exact-integer
//! IDA\* over a 1-D bang-bang lattice: `lower_bound_frames` and `plan`
//! convert the caller's `f32` state into integer lattice units
//! (`S` = velocity in units of `a*dt`, `D` = distance in units of `a*dt²`)
//! and reason about `AtRest` as "lattice `S == 0`", **not** by re-querying
//! `alice-physics`' derived (XPBD, position-diff-based) velocity for exact
//! equality to zero — the engine's `update_velocities` re-derives velocity
//! from a position delta every substep, which drifts by a few dozen ulp per
//! frame even with `Fix128` and zero damping (confirmed by driving the
//! witness trajectory through a real `PhysicsWorld`: `v == Fix128::ZERO` is
//! `false`). The design decision and its rationale are recorded in
//! `project_alice_world_model_phase4_design_confirmed` §(4).

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_wrap)]

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

/// Converts a real-valued axis distance/velocity into integer lattice units
/// (`D` = `a*dt²`, `S` = `a*dt`), per
/// `reference_bangbang_integer_lattice_closed_form` §0. Rounds to the
/// nearest integer; exact for the MVP's own oracle scenes (`a*dt²` divides
/// the distances used there), approximate otherwise (documented limitation,
/// `project_alice_world_model_phase4_design_confirmed` §3).
#[allow(clippy::cast_possible_truncation)]
fn to_lattice(distance: f32, velocity: f32, a_max_axis: f32, dt: f32) -> (i64, i64) {
    let unit_v = a_max_axis * dt;
    let unit_p = a_max_axis * dt * dt;
    let s = (velocity / unit_v).round() as i64;
    let d = (distance / unit_p).round() as i64;
    (s, d)
}

/// `M(s, r)`: the maximum position-index reachable in `r` frames starting
/// at velocity-index `s` and ending at velocity-index `0`, per
/// `reference_bangbang_integer_lattice_closed_form` §1. Requires `r >= |s|`;
/// the caller is responsible for that (checked via `debug_assert!`, not
/// re-validated here on every call since this runs inside the search's hot
/// loop).
#[allow(clippy::suspicious_operation_groupings)] // p*p / hold*p sharing `p` is the actual formula, not a typo
fn m_reachable(s: i64, r: i64) -> i64 {
    debug_assert!(r >= s.abs(), "M(s, r) requires r >= |s| (r={r}, s={s})");
    let p = (r + s).div_euclid(2);
    let hold = (r + s) - 2 * p;
    let decel_offset = s * (s + 1) / 2;
    p * p + hold * p - decel_offset
}

/// Exact minimal frame count to go from velocity-index `s` to rest (`0`)
/// while covering at least `d_lo` position-index, via monotone search over
/// `m_reachable`. `reference_bangbang_integer_lattice_closed_form` §2: `M`
/// is monotone non-decreasing in `r`, so the first `r` with `M(s, r)` at
/// least `d_lo` is the exact answer — not merely an admissible bound.
fn exact_rest_to_rest_h(s: i64, d_lo: i64) -> u32 {
    let mut r = s.abs();
    while m_reachable(s, r) < d_lo {
        r += 1;
    }
    u32::try_from(r).unwrap_or(u32::MAX)
}

/// Exact minimal frame count to cover at least `d_lo` position-index from
/// velocity-index `s`, with **no** rest requirement at arrival. Optimal is
/// to accelerate every frame (`u = +1`): there is never a reason to brake
/// when arrival velocity is unconstrained, so position after `r` frames of
/// pure acceleration is `s*r + r*(r+1)/2` — solved for the minimal integer
/// `r`, this is exact (not merely admissible), matching
/// `tests/phase3_search_oracles.rs` scene B's closed form (`t =
/// sqrt(2*distance/a_total)`, the `C = sqrt(2)` case).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops
)]
fn exact_position_only_h(s: i64, d_lo: i64) -> u32 {
    if d_lo <= 0 {
        return 0;
    }
    // r^2 + (1 + 2s) r - 2*d_lo >= 0, solve via the quadratic formula then
    // nudge to the exact minimal integer root (float sqrt is a starting
    // guess only, not the oracle — `reference_bangbang_integer_lattice_closed_form`
    // §3's "closed form off by one near non-integer windows" caution, and
    // `mul_add` is deliberately not used here per
    // `feedback_mul_add_breaks_bit_exactness` even though this value is only
    // a seed for the exact integer search below, not itself load-bearing;
    // `s`/`d_lo` stay well inside f64's exact-integer range for all MVP
    // scenes, so the precision-loss lint is a non-issue here).
    let b = 1.0 + 2.0 * s as f64;
    let c = -2.0 * d_lo as f64;
    let discriminant = b * b - 4.0 * c;
    let r0 = f64::midpoint(-b, discriminant.sqrt()).ceil() as i64;
    let mut r = (r0 - 2).max(0);
    while s * r + r * (r + 1) / 2 < d_lo {
        r += 1;
    }
    u32::try_from(r).unwrap_or(u32::MAX)
}

/// L∞ admissible lower bound on the number of frames needed to reach the
/// goal from `state`, given `params`.
///
/// # Goal dependence
///
/// `goal`'s variant — not its `target` bounds, which the caller has already
/// folded into `state.distance_to_goal` — selects which closed form applies:
/// `Goal::PositionWithinAndAtRest` requires decelerating to zero velocity on
/// arrival (symmetric bang-bang), while `Goal::PositionWithin` only requires
/// reaching the position (the body may arrive at any velocity, a strictly
/// cheaper bound). A single formula cannot be admissible for both
/// (`feedback_world_auditor_phase3_oracle_contradictions`, `ys-3a`
/// 2026-10-03: no constant `C` in `ceil(C*sqrt(d/a)/dt)` satisfies both
/// oracle scenes simultaneously).
///
/// # Exactness
///
/// Returns the **exact** minimal frame count (not merely an admissible
/// lower bound) for both variants — `reference_bangbang_integer_lattice_closed_form`
/// §2/§3. `params.a_max_axis` must already include any assisting/opposing
/// acceleration (e.g. gravity) the caller wants accounted for — see
/// `tests/phase3_search_oracles.rs` scene B for the admissible/inadmissible
/// pair this enables.
///
/// # Panics
///
/// If `goal` is a future `#[non_exhaustive]` variant this crate's version
/// predates (fail-fast: no closed form has been derived for it yet).
#[must_use]
pub fn lower_bound_frames(state: AxisState, goal: &Goal, params: &Params) -> u32 {
    let (s, d) = to_lattice(
        state.distance_to_goal,
        state.velocity,
        params.a_max_axis,
        params.dt,
    );
    match goal {
        Goal::PositionWithinAndAtRest { .. } => exact_rest_to_rest_h(s, d),
        Goal::PositionWithin { .. } => exact_position_only_h(s, d),
        _ => panic!(
            "lower_bound_frames: no closed form derived yet for this Goal variant \
             (project_alice_world_model_phase4_design_confirmed §(2))"
        ),
    }
}

/// IDA\* bookkeeping shared across the recursion (keeps `search_step`'s
/// argument count sane — `clippy::too_many_arguments` otherwise).
#[cfg(feature = "physics")]
struct SearchContext {
    rest_required: bool,
    d_lo: i64,
    d_hi: i64,
    node_budget: u64,
    nodes_expanded: u64,
}

#[cfg(feature = "physics")]
impl SearchContext {
    fn admissible_h(&self, s: i64, remaining: i64) -> u32 {
        if self.rest_required {
            exact_rest_to_rest_h(s, remaining)
        } else {
            exact_position_only_h(s, remaining)
        }
    }
}

/// Outcome of one depth-first probe at a fixed `f`-bound.
#[cfg(feature = "physics")]
enum Probe {
    Found,
    /// No child stayed within `bound`; carries the smallest `f` seen among
    /// the children tried (the next outer-loop bound).
    Exceeded(u32),
    BudgetExhausted,
}

/// Recursive IDA\* step over the frame-granularity (`k = 1`,
/// `project_alice_world_model_phase4_design_confirmed` §(1)) action space.
/// Action-index order `{+1 (Accelerate), 0 (Coast), -1 (Decelerate)}` is the
/// MVP's fixed tie-break (plan §1.2's "行動 index 辞書順"); `ctx.admissible_h`
/// being **exact** (not merely admissible) means at most one child per node
/// stays within `bound`, so this explores a single path with zero
/// backtracking for the MVP's oracle scenes
/// (`reference_bangbang_integer_lattice_closed_form` §2, 実測6) — the
/// recursive structure is kept general (not hand-collapsed into a greedy
/// walk) so it stays correct if `admissible_h` is ever weakened to
/// merely-admissible for a future, harder goal predicate.
#[cfg(feature = "physics")]
fn search_step(
    ctx: &mut SearchContext,
    s: i64,
    p: i64,
    depth: u32,
    bound: u32,
    actions: &mut Vec<Action>,
) -> Probe {
    let at_goal = (!ctx.rest_required || s == 0) && p >= ctx.d_lo && p <= ctx.d_hi;
    if at_goal {
        return Probe::Found;
    }
    let h = ctx.admissible_h(s, ctx.d_lo - p);
    let f = depth + h;
    if f > bound {
        return Probe::Exceeded(f);
    }
    let mut min_exceeded = u32::MAX;
    for u in [1_i64, 0, -1] {
        ctx.nodes_expanded += 1;
        if ctx.nodes_expanded > ctx.node_budget {
            return Probe::BudgetExhausted;
        }
        let new_s = s + u;
        let new_p = p + new_s;
        actions.push(match u {
            1 => Action::Accelerate,
            0 => Action::Coast,
            _ => Action::Decelerate,
        });
        match search_step(ctx, new_s, new_p, depth + 1, bound, actions) {
            Probe::Found => return Probe::Found,
            Probe::BudgetExhausted => return Probe::BudgetExhausted,
            Probe::Exceeded(next) => {
                actions.pop();
                min_exceeded = min_exceeded.min(next);
            }
        }
    }
    Probe::Exceeded(min_exceeded)
}

/// Drives `world`'s single body (plan §1.1: MVP scope is one rigid body)
/// along the goal axis by injecting a frame-head velocity impulse per
/// `action` then stepping — the same injection point validated against a
/// real `PhysicsWorld` for the witness trajectory
/// (`feedback_world_auditor_phase3_oracle_contradictions` §5: `x =
/// 100.00001` after 537 frames with `damping: ONE, substeps: 1`).
#[cfg(feature = "physics")]
fn drive_world(
    world: &mut alice_physics::PhysicsWorld,
    actions: &[Action],
    dir: f32,
    params: &Params,
) {
    use alice_physics::Fix128;
    let dt_fix = Fix128::from_f32(params.dt);
    let delta_v = Fix128::from_f32(params.a_max_axis * params.dt * dir);
    for action in actions {
        let signed_delta = match action {
            Action::Accelerate => delta_v,
            Action::Decelerate => -delta_v,
            Action::Coast => Fix128::ZERO,
        };
        world.bodies[0].velocity.x = world.bodies[0].velocity.x + signed_delta;
        world.step(dt_fix);
    }
}

/// Result of the lattice-only search shared by [`plan`] and [`audit`].
#[cfg(feature = "physics")]
enum LatticeSearch {
    /// An optimal plan in lattice units, and the `x` direction (`+1.0` /
    /// `-1.0`) it moves the body in.
    Found(Optimal, f32),
    /// Node budget ran out first.
    BudgetExhausted(BestSoFar),
    /// The IDA\* bound grew past `max_bound` (only [`audit`] sets a finite
    /// cap; [`plan`] passes `u32::MAX` and so never sees this).
    TooDeep,
}

/// Runs the frame-granularity IDA\* over the integer lattice for a body at
/// `pos_x` / `vel_x` (no engine access — the engine is only touched by the
/// caller afterwards).
#[cfg(feature = "physics")]
fn lattice_search(
    pos_x: f32,
    vel_x: f32,
    target: &Aabb,
    rest_required: bool,
    params: &Params,
    max_bound: u32,
) -> LatticeSearch {
    let center_x = f32::midpoint(target.min.x, target.max.x);
    let dir: f32 = if center_x >= pos_x { 1.0 } else { -1.0 };
    let (near_dist, far_dist) = if dir > 0.0 {
        (target.min.x - pos_x, target.max.x - pos_x)
    } else {
        (pos_x - target.max.x, pos_x - target.min.x)
    };
    let forward_vel = vel_x * dir;

    let (s, d_lo) = to_lattice(near_dist, forward_vel, params.a_max_axis, params.dt);
    let (_, d_hi) = to_lattice(far_dist, forward_vel, params.a_max_axis, params.dt);

    let mut ctx = SearchContext {
        rest_required,
        d_lo,
        d_hi,
        node_budget: params.node_budget,
        nodes_expanded: 0,
    };
    let mut bound = ctx.admissible_h(s, d_lo);

    loop {
        if bound > max_bound {
            return LatticeSearch::TooDeep;
        }
        let mut actions = Vec::new();
        match search_step(&mut ctx, s, 0, 0, bound, &mut actions) {
            Probe::Found => {
                let frames = u32::try_from(actions.len()).unwrap_or(u32::MAX);
                return LatticeSearch::Found(Optimal { frames, actions }, dir);
            }
            Probe::BudgetExhausted => {
                let frames = u32::try_from(actions.len()).unwrap_or(u32::MAX);
                return LatticeSearch::BudgetExhausted(BestSoFar { frames, actions });
            }
            Probe::Exceeded(next_bound) => {
                assert!(
                    next_bound > bound,
                    "IDA* bound did not increase (bug): bound={bound}, next={next_bound}"
                );
                bound = next_bound;
            }
        }
    }
}

/// Search for a frame-minimal action sequence from `world`'s current state
/// to `goal`, then execute it against `world`.
///
/// ⚠️ **`Ok(Optimal)` is a claim about the integer lattice, not about
/// `world`.** The plan is optimal in the lattice model; whether driving
/// `world` with it actually satisfies `goal` depends on `world`'s config
/// (damping, substeps) and on whether `a_max_axis * dt` and `dt` are exactly
/// representable in `Fix128`. Use [`audit`] for a verdict that is checked
/// against the engine.
///
/// # Scope (MVP)
///
/// Single rigid body (`world.bodies[0]`), goal axis fixed to `x` (1 axis).
/// Direction is inferred from which side of `goal`'s `target` the body
/// starts on, so targets reached by moving in `-x` work too.
///
/// # Errors
///
/// Returns `Err(BestSoFar)` if the node budget is exhausted before an
/// optimal plan is found. `world` is left unmodified in that case — the
/// trajectory is only executed once a fully optimal plan is in hand, so a
/// caller retrying with a larger budget doesn't see a partially-driven body.
///
/// # Panics
///
/// If `goal` is a future `#[non_exhaustive]` variant this crate's version
/// predates — see [`lower_bound_frames`].
#[cfg(feature = "physics")]
pub fn plan(
    world: &mut alice_physics::PhysicsWorld,
    goal: &Goal,
    params: &Params,
) -> Result<Optimal, BestSoFar> {
    let (target, rest_required) = goal_parts(goal);
    let pos_x = world.bodies[0].position.x.to_f32();
    let vel_x = world.bodies[0].velocity.x.to_f32();
    match lattice_search(pos_x, vel_x, target, rest_required, params, u32::MAX) {
        LatticeSearch::Found(optimal, dir) => {
            drive_world(world, &optimal.actions, dir, params);
            Ok(optimal)
        }
        LatticeSearch::BudgetExhausted(best) => Err(best),
        LatticeSearch::TooDeep => unreachable!("plan passes max_bound = u32::MAX"),
    }
}

/// Splits `goal` into its target and whether rest is required.
///
/// # Panics
///
/// If `goal` is a future `#[non_exhaustive]` variant.
#[cfg(feature = "physics")]
fn goal_parts(goal: &Goal) -> (&Aabb, bool) {
    match goal {
        Goal::PositionWithinAndAtRest { target } => (target, true),
        Goal::PositionWithin { target } => (target, false),
        _ => panic!(
            "no search strategy derived yet for this Goal variant (only \
             PositionWithin / PositionWithinAndAtRest are supported)"
        ),
    }
}

/// Largest IDA\* `f`-bound (= frame count) [`audit`] will search to.
///
/// The search recurses once per frame, so this caps the recursion depth as
/// well as the replay length; a goal whose exact lower bound is beyond it
/// is reported as [`UndecidedCause::OutOfSearchRange`], not searched.
pub const MAX_AUDIT_FRAMES: u32 = 1 << 12;

/// The outcome of [`audit`]: a [`Verdict`] together with its evidence or
/// its reason.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Audit {
    /// The goal holds in the engine after replaying `Optimal::actions`
    /// (the evidence). `frames` is minimal on the integer lattice.
    Proven(Optimal),
    /// The goal can never hold — see [`Violation`] for the only inputs
    /// that produce this in the MVP.
    Violated(Violation),
    /// Neither proven nor violated — see [`UndecidedCause`].
    Undecided(UndecidedCause),
}

impl Audit {
    /// The 3-valued verdict this audit carries.
    #[must_use]
    pub const fn verdict(&self) -> Verdict {
        match self {
            Self::Proven(_) => Verdict::Proven,
            Self::Violated(_) => Verdict::Violated,
            Self::Undecided(_) => Verdict::Undecided,
        }
    }
}

/// Why [`audit`] returned [`Verdict::Violated`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Violation {
    /// `target` is empty (`min > max` on some axis), so no position is
    /// ever within it.
    EmptyTarget,
    /// `a_max_axis == 0`, the body's velocity is exactly zero, the
    /// config's gravity is exactly zero and the body starts outside
    /// `target`: with no input and no force the body never moves.
    NoActuatorAtRestOutsideTarget,
}

/// Why [`audit`] returned [`Verdict::Undecided`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum UndecidedCause {
    /// `params` or `goal` cannot be evaluated: `dt` not finite and
    /// positive, `a_max_axis` not finite and non-negative, or a `target`
    /// bound that is not finite or not exactly representable in `Fix128`.
    InvalidInput,
    /// `a_max_axis == 0` but the body is moving or gravity is non-zero, so
    /// the body may drift; this crate does not decide that case.
    NoActuator,
    /// The lattice coordinates or the exact frame lower bound are beyond
    /// what this crate searches ([`MAX_AUDIT_FRAMES`]).
    OutOfSearchRange,
    /// The node budget ran out before an optimal lattice plan was found.
    BudgetExhausted(BestSoFar),
    /// A lattice-optimal plan was found, but replaying it in the engine set
    /// `alice-physics`' sticky `overflow_detected` flag; the final state
    /// is not trusted, whatever it looks like.
    Overflow(Optimal),
    /// A lattice-optimal plan was found and replayed without overflow, but
    /// the engine's final state does not satisfy the goal exactly.
    ReplayMismatch {
        /// The lattice-optimal plan that was replayed.
        plan: Optimal,
        /// Whether the final position was within `target`.
        position_within: bool,
        /// Whether the final linear velocity was exactly zero.
        at_rest: bool,
    },
}

/// Largest lattice coordinate magnitudes [`audit`] converts (velocity
/// index `|S|` and position index `|D|`); beyond these the integer
/// arithmetic of the search is not guaranteed to stay in range.
#[cfg(feature = "physics")]
const MAX_LATTICE_S: f64 = 1_048_576.0; // 2^20
#[cfg(feature = "physics")]
const MAX_LATTICE_D: f64 = 1_099_511_627_776.0; // 2^40

/// `f` as an exactly-equal `Fix128`, or `None` if `f` is not finite or not
/// exactly representable (so that a bound comparison in `Fix128` is the
/// same comparison as in `f32`).
#[cfg(feature = "physics")]
fn exact_fix(f: f32) -> Option<alice_physics::Fix128> {
    const LIMIT: f32 = 4_611_686_018_427_387_904.0; // 2^62
    if !f.is_finite() || f.abs() >= LIMIT {
        return None;
    }
    let x = alice_physics::Fix128::from_f32(f);
    #[allow(clippy::float_cmp)] // exact round-trip is the point
    let exact = x.to_f64() == f64::from(f);
    exact.then_some(x)
}

/// `target` converted bound by bound with [`exact_fix`].
#[cfg(feature = "physics")]
struct FixAabb {
    min: [alice_physics::Fix128; 3],
    max: [alice_physics::Fix128; 3],
}

#[cfg(feature = "physics")]
impl FixAabb {
    fn new(t: &Aabb) -> Option<Self> {
        Some(Self {
            min: [
                exact_fix(t.min.x)?,
                exact_fix(t.min.y)?,
                exact_fix(t.min.z)?,
            ],
            max: [
                exact_fix(t.max.x)?,
                exact_fix(t.max.y)?,
                exact_fix(t.max.z)?,
            ],
        })
    }

    fn is_empty(&self) -> bool {
        (0..3).any(|i| self.min[i] > self.max[i])
    }

    fn contains(&self, p: alice_physics::Vec3Fix) -> bool {
        let p = [p.x, p.y, p.z];
        (0..3).all(|i| p[i] >= self.min[i] && p[i] <= self.max[i])
    }
}

/// `(position_within, at_rest)` for `body`, both exact in `Fix128`:
/// position within `target` on all three axes, and linear velocity exactly
/// zero.
#[cfg(feature = "physics")]
fn exact_state(body: &alice_physics::RigidBody, target: &FixAabb) -> (bool, bool) {
    (
        target.contains(body.position),
        body.velocity == alice_physics::Vec3Fix::ZERO,
    )
}

/// Audit `goal` for a single `body` simulated under `config`: search the
/// integer lattice for a frame-minimal plan, replay it in a fresh
/// `PhysicsWorld` built from `config`, and return a 3-valued verdict.
///
/// `Proven` is returned **only** when the replay, under exactly this
/// `config`, ends with the goal holding exactly in `Fix128` (position
/// within `target` on all three axes; for
/// [`Goal::PositionWithinAndAtRest`] also linear velocity exactly zero)
/// **and** without the engine's sticky overflow flag. The frame count is
/// minimal on the integer lattice (the claim of this crate is
/// "k 粒度の macro-action 空間で、整数格子上で厳密に最適", `k = 1`); it
/// is not a claim about every engine trajectory.
///
/// # Sufficient conditions for `Proven`
///
/// For a reachable target, the replay matches the lattice exactly when
/// `a_max_axis * dt` and `dt` are dyadic (exact in `Fix128`, e.g.
/// `a = 4, dt = 1/64`), `config.damping == 1`, gravity is zero along the
/// goal axis, and `target`'s `x` window contains a lattice point. When any
/// of these fails (e.g. `dt = 1/60`, or the default damping `0.99`), the
/// replay generally misses the goal and the answer is
/// [`UndecidedCause::ReplayMismatch`] — never a `Proven` the engine does
/// not back.
///
/// # When `Violated` is returned
///
/// Only for the two inputs listed in [`Violation`]: an empty `target`, or
/// no actuator (`a_max_axis == 0`) with the body at rest, zero gravity and
/// the body outside `target`. Proving unreachability from a law in general
/// (obstacles, region constraints) is not attempted: with `a_max_axis > 0`
/// every target is reachable in 1-D, so `Violated` never comes from search.
///
/// # Degenerate inputs
///
/// | input | answer |
/// |---|---|
/// | goal already holds (distance 0, at rest) | `Proven`, 0 frames, even with `node_budget == 0` |
/// | `node_budget == 0`, goal not holding | `Undecided(BudgetExhausted)` |
/// | target on the `-x` side (negative distance) | searched in the `-x` direction |
/// | `a_max_axis == 0` | `Violated` / `Undecided(NoActuator)` / `Proven` (see above) |
/// | `dt <= 0`, non-finite `dt` / `a_max_axis`, `a_max_axis < 0`, unrepresentable `target` bound | `Undecided(InvalidInput)` |
/// | exact lower bound over [`MAX_AUDIT_FRAMES`] | `Undecided(OutOfSearchRange)` |
///
/// # Scope (MVP)
///
/// One rigid body, goal axis `x`, input injected as a frame-head velocity
/// impulse of `±a_max_axis * dt`. Angular velocity is not part of `AtRest`.
///
/// # Panics
///
/// If `goal` is a future `#[non_exhaustive]` variant this crate's version
/// predates.
#[cfg(feature = "physics")]
#[must_use]
pub fn audit(
    body: &alice_physics::RigidBody,
    config: alice_physics::PhysicsConfig,
    goal: &Goal,
    params: &Params,
) -> Audit {
    let (target, rest_required) = goal_parts(goal);

    let dt_ok = params.dt.is_finite() && params.dt > 0.0;
    let a_ok = params.a_max_axis.is_finite() && params.a_max_axis >= 0.0;
    if !(dt_ok && a_ok) {
        return Audit::Undecided(UndecidedCause::InvalidInput);
    }
    let Some(fix_target) = FixAabb::new(target) else {
        return Audit::Undecided(UndecidedCause::InvalidInput);
    };
    if fix_target.is_empty() {
        return Audit::Violated(Violation::EmptyTarget);
    }

    let (within, at_rest) = exact_state(body, &fix_target);
    if within && (at_rest || !rest_required) {
        return Audit::Proven(Optimal {
            frames: 0,
            actions: Vec::new(),
        });
    }

    #[allow(clippy::float_cmp)] // exactly zero means "no actuator"
    if params.a_max_axis == 0.0 {
        let at_rest = body.velocity == alice_physics::Vec3Fix::ZERO;
        let no_gravity = config.gravity == alice_physics::Vec3Fix::ZERO;
        return if at_rest && no_gravity {
            Audit::Violated(Violation::NoActuatorAtRestOutsideTarget)
        } else {
            Audit::Undecided(UndecidedCause::NoActuator)
        };
    }

    let pos_x = body.position.x.to_f32();
    let vel_x = body.velocity.x.to_f32();
    let unit_v = f64::from(params.a_max_axis) * f64::from(params.dt);
    let unit_p = unit_v * f64::from(params.dt);
    let far = f64::from(target.min.x - pos_x)
        .abs()
        .max(f64::from(target.max.x - pos_x).abs());
    let s_mag = f64::from(vel_x).abs() / unit_v;
    let d_mag = far / unit_p;
    if !(s_mag.is_finite() && d_mag.is_finite()) || s_mag > MAX_LATTICE_S || d_mag > MAX_LATTICE_D {
        return Audit::Undecided(UndecidedCause::OutOfSearchRange);
    }

    let found = match lattice_search(
        pos_x,
        vel_x,
        target,
        rest_required,
        params,
        MAX_AUDIT_FRAMES,
    ) {
        LatticeSearch::Found(optimal, dir) => (optimal, dir),
        LatticeSearch::BudgetExhausted(best) => {
            return Audit::Undecided(UndecidedCause::BudgetExhausted(best));
        }
        LatticeSearch::TooDeep => return Audit::Undecided(UndecidedCause::OutOfSearchRange),
    };
    let (optimal, dir) = found;

    let mut world = alice_physics::PhysicsWorld::new(config);
    world.add_body(*body);
    drive_world(&mut world, &optimal.actions, dir, params);
    if world.overflow_detected() {
        return Audit::Undecided(UndecidedCause::Overflow(optimal));
    }
    let (position_within, at_rest) = exact_state(&world.bodies[0], &fix_target);
    if position_within && (at_rest || !rest_required) {
        Audit::Proven(optimal)
    } else {
        Audit::Undecided(UndecidedCause::ReplayMismatch {
            plan: optimal,
            position_within,
            at_rest,
        })
    }
}
