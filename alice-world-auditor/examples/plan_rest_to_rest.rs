//! Wires the public entry point `plan` (World Auditor wiring guard, example
//! covers the `physics` feature path) — see `project_alice_world_model_mvp_plan`
//! §1 for the rest-to-rest goal this exercises.
//!
//! `cargo run --example plan_rest_to_rest --features physics` prints both
//! `lower_bound_frames`' estimate and `plan`'s actual result. ⚠️ The two
//! calls below use different distances on purpose (`axis_state`'s point
//! distance `10.0` vs. `goal.target`'s near edge `9.0`) — they are two
//! independent illustrations of the two entry points, not a
//! consistency check (`lower_bound_frames` only ever sees the scalar
//! `AxisState` the caller resolved, never `goal.target`'s bounds, per its
//! own doc comment).

use alice_physics::{Fix128, PhysicsConfig, PhysicsWorld, RigidBody, Vec3Fix};
use alice_world_auditor::{AxisState, Goal, Params};

fn main() {
    let mut world = PhysicsWorld::new(PhysicsConfig {
        gravity: Vec3Fix::ZERO,
        ..PhysicsConfig::default()
    });
    world.add_body(RigidBody::new_dynamic(Vec3Fix::ZERO, Fix128::ONE));

    let goal = Goal::PositionWithinAndAtRest {
        target: alice_world_auditor::Aabb {
            min: glam::Vec3::new(9.0, -1.0, -1.0),
            max: glam::Vec3::new(11.0, 1.0, 1.0),
        },
    };

    let params = Params::new(5.0, 1.0 / 60.0, 100_000);

    let axis_state = AxisState {
        distance_to_goal: 10.0,
        velocity: 0.0,
    };
    let h = alice_world_auditor::lower_bound_frames(axis_state, &goal, &params);
    println!("lower bound: {h} frames");

    match alice_world_auditor::plan(&mut world, &goal, &params) {
        Ok(optimal) => println!("optimal: {} frames", optimal.frames),
        Err(best) => println!("budget exhausted, best so far: {} frames", best.frames),
    }
}
