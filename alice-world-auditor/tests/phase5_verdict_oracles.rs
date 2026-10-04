//! Phase 5 oracles: the 3-valued verdict of [`audit`] must agree with what
//! actually happens in the engine.
//!
//! Every test enters through the production entry point [`audit`]. Expected
//! frame counts come from the bang-bang closed form on the integer lattice
//! (`t* = 2*sqrt(d/a)`, minimal frames `ceil(t*/dt)`; for the dyadic scene
//! `d = 100, a = 4, dt = 1/64` this is exactly `640`, and on the lattice
//! `D = d/(a*dt^2) = 102400`, `M(0, 640) = 320^2 = 102400 >= D` while
//! `M(0, 639) = 319^2 + 319 = 102080 < D`). Engine-side facts (final
//! position / velocity / overflow flag) are checked by replaying the
//! returned actions here, independently of `audit`'s own check.
//!
//! Sufficient conditions for `Proven` used throughout: `a*dt` and `dt` are
//! dyadic (exact in `Fix128`), `damping = 1`, zero gravity along the goal
//! axis. Scenes that break one of them are expected to be `Undecided`.
#![cfg(feature = "physics")]

use alice_physics::{Fix128, PhysicsConfig, PhysicsWorld, RigidBody, Vec3Fix};
use alice_world_auditor::{
    audit, plan, Aabb, Action, Audit, Goal, Params, UndecidedCause, Verdict, Violation,
};
use glam::Vec3;

const DT: f32 = 1.0 / 64.0;
const A: f32 = 4.0;
const BUDGET: u64 = 1_000_000;

fn lattice_config() -> PhysicsConfig {
    PhysicsConfig {
        gravity: Vec3Fix::ZERO,
        damping: Fix128::ONE,
        substeps: 1,
        ..PhysicsConfig::default()
    }
}

fn body_at(x: f32) -> RigidBody {
    RigidBody::new_dynamic(Vec3Fix::from_f32(x, 0.0, 0.0), Fix128::ONE)
}

const fn rest_goal(lo: f32, hi: f32) -> Goal {
    Goal::PositionWithinAndAtRest {
        target: Aabb {
            min: Vec3::new(lo, -0.5, -0.5),
            max: Vec3::new(hi, 0.5, 0.5),
        },
    }
}

/// Replays `actions` the way the documented injection works (frame-head
/// velocity impulse of `+-a*dt` along `dir`, then one `step(dt)`), returning
/// the final world. Written here, not taken from the crate, so the oracle
/// does not share code with the checker it judges.
fn replay(
    body: &RigidBody,
    config: PhysicsConfig,
    actions: &[Action],
    a: f32,
    dt: f32,
    dir: f32,
) -> PhysicsWorld {
    let mut w = PhysicsWorld::new(config);
    w.add_body(*body);
    let dv = Fix128::from_f32(a * dt * dir);
    for act in actions {
        match act {
            Action::Accelerate => w.bodies[0].velocity.x = w.bodies[0].velocity.x + dv,
            Action::Decelerate => w.bodies[0].velocity.x = w.bodies[0].velocity.x - dv,
            _ => {}
        }
        w.step(Fix128::from_f32(dt));
    }
    w
}

fn holds_exactly(w: &PhysicsWorld, lo: f32, hi: f32) -> bool {
    let b = &w.bodies[0];
    let x = b.position.x;
    x >= Fix128::from_f32(lo)
        && x <= Fix128::from_f32(hi)
        && b.position.y >= Fix128::from_f32(-0.5)
        && b.position.y <= Fix128::from_f32(0.5)
        && b.position.z >= Fix128::from_f32(-0.5)
        && b.position.z <= Fix128::from_f32(0.5)
        && b.velocity == Vec3Fix::ZERO
}

// ---------------------------------------------------------------- Proven

/// Proven: dyadic rest-to-rest, 640 frames (closed form), and the returned
/// actions really leave the body inside the target at exactly zero velocity.
#[test]
fn proven_dyadic_rest_to_rest_is_frame_minimal_and_replays_exactly() {
    for substeps in [1_usize, 8] {
        let config = PhysicsConfig {
            substeps,
            ..lattice_config()
        };
        let body = body_at(0.0);
        let got = audit(
            &body,
            config,
            &rest_goal(99.9, 100.1),
            &Params::new(A, DT, BUDGET),
        );
        assert_eq!(
            got.verdict(),
            Verdict::Proven,
            "substeps={substeps}: {got:?}"
        );
        let Audit::Proven(p) = got else {
            unreachable!()
        };
        assert_eq!(p.frames, 640, "closed form 2*sqrt(100/4)*64 = 640");
        assert_eq!(p.actions.len(), 640);
        let w = replay(&body, config, &p.actions, A, DT, 1.0);
        assert!(!w.overflow_detected());
        assert!(
            holds_exactly(&w, 99.9, 100.1),
            "evidence does not reach the goal in the engine (substeps={substeps})"
        );
    }
}

/// Proven: target on the `-x` side (negative distance) — mirror of the
/// scene above, same 640 frames.
#[test]
fn proven_negative_direction_is_the_mirror_image() {
    let body = body_at(0.0);
    let got = audit(
        &body,
        lattice_config(),
        &rest_goal(-100.1, -99.9),
        &Params::new(A, DT, BUDGET),
    );
    let Audit::Proven(p) = got else {
        panic!("expected Proven, got {got:?}")
    };
    assert_eq!(p.frames, 640);
    let w = replay(&body, lattice_config(), &p.actions, A, DT, -1.0);
    assert!(holds_exactly(&w, -100.1, -99.9));
}

/// Proven from a non-origin start (x = 50 to [99.9, 100.1]): lattice near
/// edge `round(49.9*1024) = 51098`, `M(0,452) = 226^2 = 51076 < 51098 <=
/// M(0,453) = 226^2 + 226 = 51302`, so 453 frames.
#[test]
fn proven_from_a_non_origin_start() {
    let body = body_at(50.0);
    let got = audit(
        &body,
        lattice_config(),
        &rest_goal(99.9, 100.1),
        &Params::new(A, DT, BUDGET),
    );
    let Audit::Proven(p) = got else {
        panic!("expected Proven, got {got:?}")
    };
    assert_eq!(p.frames, 453);
    let w = replay(&body, lattice_config(), &p.actions, A, DT, 1.0);
    assert!(holds_exactly(&w, 99.9, 100.1));
}

/// Proven with 0 frames: distance 0 (the goal already holds exactly at the
/// start), even with a node budget of 0 — nothing needs to be searched.
#[test]
fn proven_with_zero_frames_when_the_goal_already_holds() {
    for budget in [0_u64, BUDGET] {
        let got = audit(
            &body_at(100.0),
            lattice_config(),
            &rest_goal(99.9, 100.1),
            &Params::new(A, DT, budget),
        );
        let Audit::Proven(p) = got else {
            panic!("budget={budget}: expected Proven, got {got:?}")
        };
        assert_eq!(p.frames, 0);
        assert!(p.actions.is_empty());
    }
}

// ------------------------------------------------------------- Undecided

/// Undecided: the non-dyadic scene (`a = 5, dt = 1/60`). The lattice plan
/// is 537 frames, but `a*dt` and `dt` are not exact in `Fix128`; replayed in
/// the engine the body ends just past the target's far edge with a
/// non-zero velocity, so the goal does not hold and `Proven` must not be
/// returned.
#[test]
fn undecided_when_the_replay_of_a_non_dyadic_plan_misses_the_goal() {
    let a = 5.0_f32;
    let dt = 1.0 / 60.0_f32;
    let body = body_at(0.0);
    let goal = rest_goal(99.9, 100.1);

    // premise: the lattice search alone does claim 537 frames
    let mut w = PhysicsWorld::new(lattice_config());
    w.add_body(body);
    assert_eq!(
        plan(&mut w, &goal, &Params::new(a, dt, BUDGET)).map(|o| o.frames),
        Ok(537)
    );
    assert!(
        !holds_exactly(&w, 99.9, 100.1),
        "premise: engine misses the goal"
    );

    let got = audit(&body, lattice_config(), &goal, &Params::new(a, dt, BUDGET));
    match got {
        Audit::Undecided(UndecidedCause::ReplayMismatch {
            plan,
            position_within,
            at_rest,
        }) => {
            assert_eq!(plan.frames, 537);
            assert!(!position_within, "x ends at 100.1000082 > 100.1");
            assert!(!at_rest, "engine velocity ends at about -1.7e-15, not 0");
        }
        other => panic!("expected Undecided(ReplayMismatch), got {other:?}"),
    }
}

/// Undecided, not Proven: a non-dyadic replay that ends inside the target
/// but not at rest (`a = 5, dt = 1/60`, target [0.9, 1.1]: the body stops
/// at x = 0.90278 with velocity about -1.6e-15). Lattice: near edge
/// `round(0.9*720) = 648`, `M(0,50) = 625 < 648 <= M(0,51) = 650`, so 51
/// frames. Position alone must not be read as the rest-to-rest goal.
#[test]
fn undecided_when_the_replay_ends_inside_the_target_but_still_moving() {
    let got = audit(
        &body_at(0.0),
        lattice_config(),
        &rest_goal(0.9, 1.1),
        &Params::new(5.0, 1.0 / 60.0, BUDGET),
    );
    match got {
        Audit::Undecided(UndecidedCause::ReplayMismatch {
            plan,
            position_within,
            at_rest,
        }) => {
            assert_eq!(plan.frames, 51);
            assert!(position_within);
            assert!(!at_rest);
        }
        other => panic!("expected Undecided(ReplayMismatch), got {other:?}"),
    }
}

/// Undecided: the default config's damping (0.99 per frame) and 8 substeps
/// make the lattice plan fall short in the engine.
#[test]
fn undecided_under_the_default_damping() {
    let config = PhysicsConfig {
        gravity: Vec3Fix::ZERO,
        ..PhysicsConfig::default()
    };
    let got = audit(
        &body_at(0.0),
        config,
        &rest_goal(99.9, 100.1),
        &Params::new(A, DT, BUDGET),
    );
    match got {
        Audit::Undecided(UndecidedCause::ReplayMismatch {
            plan,
            position_within,
            ..
        }) => {
            assert_eq!(plan.frames, 640);
            assert!(!position_within);
        }
        other => panic!("expected Undecided(ReplayMismatch), got {other:?}"),
    }
}

/// Undecided: node budget exhausted (budget 1, and budget 0 when the goal
/// does not already hold).
#[test]
fn undecided_when_the_node_budget_runs_out() {
    for budget in [0_u64, 1] {
        let got = audit(
            &body_at(0.0),
            lattice_config(),
            &rest_goal(99.9, 100.1),
            &Params::new(A, DT, budget),
        );
        match got {
            Audit::Undecided(UndecidedCause::BudgetExhausted(best)) => {
                assert!(best.frames < 640, "budget={budget}: {best:?}");
            }
            other => panic!("budget={budget}: expected BudgetExhausted, got {other:?}"),
        }
    }
}

/// Undecided, not Proven: inside the target but moving (`v = a*dt`), with
/// no budget to brake — position alone must not be read as the goal.
#[test]
fn undecided_when_inside_the_target_but_not_at_rest() {
    let mut body = body_at(100.0);
    body.velocity = Vec3Fix::from_f32(A * DT, 0.0, 0.0);
    let got = audit(
        &body,
        lattice_config(),
        &rest_goal(99.9, 100.1),
        &Params::new(A, DT, 0),
    );
    assert!(
        matches!(got, Audit::Undecided(UndecidedCause::BudgetExhausted(_))),
        "got {got:?}"
    );
}

/// Undecided: overflow during the replay. Gravity `-2^30` along `y` with
/// `dt = 2^20` makes `v.y * dt` leave `Fix128`'s range on the first step;
/// the engine then keeps the old position and re-derives a zero velocity,
/// so the body looks exactly at rest inside the target ("washed"). The
/// sticky overflow flag is the only thing that tells this apart from a
/// genuine arrival.
#[test]
fn undecided_when_the_replay_overflows_even_though_the_final_state_looks_like_the_goal() {
    let dt = 1_048_576.0_f32; // 2^20
    let a = 1.0 / 1_099_511_627_776.0_f32; // 2^-40: a*dt = 2^-20, a*dt^2 = 1
    let config = PhysicsConfig {
        gravity: Vec3Fix::new(Fix128::ZERO, -Fix128::from_int(1 << 30), Fix128::ZERO),
        damping: Fix128::ONE,
        substeps: 1,
        ..PhysicsConfig::default()
    };
    let mut body = body_at(0.0);
    body.velocity = Vec3Fix::from_f32(a * dt, 0.0, 0.0); // lattice s = 1
    let goal = rest_goal(-0.5, 0.5);

    let got = audit(&body, config, &goal, &Params::new(a, dt, BUDGET));
    let Audit::Undecided(UndecidedCause::Overflow(p)) = got else {
        panic!("expected Undecided(Overflow), got {got:?}")
    };
    assert_eq!(p.frames, 1, "one brake frame takes s = 1 to rest");
    // premise: without the flag this replay would look like the goal
    let w = replay(&body, config, &p.actions, a, dt, 1.0);
    assert!(w.overflow_detected(), "premise: the replay overflows");
    assert!(
        holds_exactly(&w, -0.5, 0.5),
        "premise: the washed state satisfies the goal"
    );
}

/// Undecided: `a_max_axis == 0` but the body is moving, or gravity is on.
#[test]
fn undecided_without_actuator_when_the_body_may_drift() {
    let mut moving = body_at(0.0);
    moving.velocity = Vec3Fix::from_f32(1.0, 0.0, 0.0);
    let got = audit(
        &moving,
        lattice_config(),
        &rest_goal(99.9, 100.1),
        &Params::new(0.0, DT, BUDGET),
    );
    assert_eq!(got, Audit::Undecided(UndecidedCause::NoActuator));

    let with_gravity = PhysicsConfig {
        gravity: Vec3Fix::from_f32(0.0, -10.0, 0.0),
        ..lattice_config()
    };
    let got = audit(
        &body_at(0.0),
        with_gravity,
        &rest_goal(99.9, 100.1),
        &Params::new(0.0, DT, BUDGET),
    );
    assert_eq!(got, Audit::Undecided(UndecidedCause::NoActuator));
}

/// Undecided: parameters that cannot be evaluated (`dt = 0`, negative,
/// NaN; negative / NaN acceleration; non-finite target bound).
#[test]
fn undecided_on_invalid_input() {
    let goal = rest_goal(99.9, 100.1);
    for (a, dt) in [
        (A, 0.0),
        (A, -DT),
        (A, f32::NAN),
        (A, f32::INFINITY),
        (-A, DT),
        (f32::NAN, DT),
        (f32::INFINITY, DT),
    ] {
        let got = audit(
            &body_at(0.0),
            lattice_config(),
            &goal,
            &Params::new(a, dt, BUDGET),
        );
        assert_eq!(
            got,
            Audit::Undecided(UndecidedCause::InvalidInput),
            "a={a} dt={dt}"
        );
    }
    // non-finite bound, and a bound too small for Fix128 (1e-30 would
    // truncate to 0 and make a body at x = 0 look inside the target)
    for (lo, hi) in [(99.9, f32::INFINITY), (1e-30, 1.0)] {
        let got = audit(
            &body_at(0.0),
            lattice_config(),
            &rest_goal(lo, hi),
            &Params::new(A, DT, BUDGET),
        );
        assert_eq!(
            got,
            Audit::Undecided(UndecidedCause::InvalidInput),
            "[{lo}, {hi}]"
        );
    }
}

/// Undecided: the exact lower bound exceeds `MAX_AUDIT_FRAMES` (d = 5000:
/// `2*sqrt(5000/4)*64 = 4525.48 > 4096`), or the lattice coordinates are
/// out of range (d = 2^40).
#[test]
fn undecided_beyond_the_search_range() {
    assert_eq!(alice_world_auditor::MAX_AUDIT_FRAMES, 4096);
    for d in [5_000.0_f32, 1_099_511_627_776.0] {
        let got = audit(
            &body_at(0.0),
            lattice_config(),
            &rest_goal(d - 0.5, d + 0.5),
            &Params::new(A, DT, BUDGET),
        );
        assert_eq!(
            got,
            Audit::Undecided(UndecidedCause::OutOfSearchRange),
            "d={d}"
        );
    }
    // just inside the range is still Proven (d = 4000: lattice near edge
    // round(3999.5*1024) = 4095488, M(0,4048) = 2024^2 = 4096576 >= it,
    // M(0,4047) = 2023^2 + 2023 = 4094552 < it, so 4048 frames)
    let got = audit(
        &body_at(0.0),
        lattice_config(),
        &rest_goal(3_999.5, 4_000.5),
        &Params::new(A, DT, BUDGET),
    );
    let Audit::Proven(p) = got else {
        panic!("expected Proven, got {got:?}")
    };
    assert_eq!(p.frames, 4048);
}

// -------------------------------------------------------------- Violated

/// Violated: an empty target (`min.x > max.x`).
#[test]
fn violated_for_an_empty_target() {
    let got = audit(
        &body_at(0.0),
        lattice_config(),
        &rest_goal(100.1, 99.9),
        &Params::new(A, DT, BUDGET),
    );
    assert_eq!(got, Audit::Violated(Violation::EmptyTarget));
    assert_eq!(got.verdict(), Verdict::Violated);
}

/// Violated: no actuator (`a_max_axis == 0`), at rest, no gravity, outside
/// the target — the body never moves. With the goal already holding the
/// same inputs are Proven (0 frames), not Violated.
#[test]
fn violated_without_actuator_at_rest_outside_the_target() {
    let got = audit(
        &body_at(0.0),
        lattice_config(),
        &rest_goal(99.9, 100.1),
        &Params::new(0.0, DT, BUDGET),
    );
    assert_eq!(
        got,
        Audit::Violated(Violation::NoActuatorAtRestOutsideTarget)
    );

    let got = audit(
        &body_at(100.0),
        lattice_config(),
        &rest_goal(99.9, 100.1),
        &Params::new(0.0, DT, BUDGET),
    );
    assert_eq!(got.verdict(), Verdict::Proven, "{got:?}");

    // inside on x but outside on y: all three axes are part of the goal
    let off_axis = RigidBody::new_dynamic(Vec3Fix::from_f32(100.0, 5.0, 0.0), Fix128::ONE);
    let got = audit(
        &off_axis,
        lattice_config(),
        &rest_goal(99.9, 100.1),
        &Params::new(0.0, DT, BUDGET),
    );
    assert_eq!(
        got,
        Audit::Violated(Violation::NoActuatorAtRestOutsideTarget)
    );
}

// ------------------------------------------------- PositionWithin goal

/// `PositionWithin` (no rest requirement): Proven and the replay ends
/// inside the target. Lattice: `r(r+1)/2 >= 102298` (near edge 99.9 in
/// units of `a*dt^2 = 1/1024`, rounded) first holds at `r = 452`.
#[test]
fn proven_for_a_position_only_goal() {
    let goal = Goal::PositionWithin {
        target: Aabb {
            min: Vec3::new(99.9, -0.5, -0.5),
            max: Vec3::new(f32::MAX / 4.0, 0.5, 0.5),
        },
    };
    let got = audit(
        &body_at(0.0),
        lattice_config(),
        &goal,
        &Params::new(A, DT, BUDGET),
    );
    // f32::MAX/4 is not representable in Fix128 -> InvalidInput is the
    // documented answer; a large but representable far edge is Proven.
    assert_eq!(got, Audit::Undecided(UndecidedCause::InvalidInput));

    let goal = Goal::PositionWithin {
        target: Aabb {
            min: Vec3::new(99.9, -0.5, -0.5),
            max: Vec3::new(1000.0, 0.5, 0.5),
        },
    };
    let got = audit(
        &body_at(0.0),
        lattice_config(),
        &goal,
        &Params::new(A, DT, BUDGET),
    );
    let Audit::Proven(p) = got else {
        panic!("expected Proven, got {got:?}")
    };
    assert_eq!(p.frames, 452);
    let w = replay(&body_at(0.0), lattice_config(), &p.actions, A, DT, 1.0);
    let x = w.bodies[0].position.x;
    assert!(x >= Fix128::from_f32(99.9) && x <= Fix128::from_f32(1000.0));
}
