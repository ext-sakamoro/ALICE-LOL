//! Wires the public entry points `audit` and `lower_bound_frames` (the
//! `physics` feature path) on the rest-to-rest goal.
//!
//! `cargo run -p alice-world-auditor --example plan_rest_to_rest --features physics`
//!
//! The same body, goal and parameters are audited under two configs:
//!
//! - dyadic `a*dt` / `dt`, no damping (`damping_rate = 0`): the lattice-optimal plan replays
//!   exactly in the engine, so the verdict is `Proven` (640 frames, the
//!   closed form `2*sqrt(d/a)/dt`);
//! - damping (a retention of 0.99 per frame, the rate `-ln(0.99)/dt` per
//!   second) and the default 8 substeps: the same lattice
//!   plan falls short in the engine, so the verdict is `Undecided`
//!   (`ReplayMismatch`) — the lattice optimum is not presented as an engine
//!   success.
//!
//! The asserts make the example fail loudly if either verdict changes.

use alice_physics::{Fix128, PhysicsConfig, RigidBody, Vec3Fix};
use alice_world_auditor::{
    audit, lower_bound_frames, Aabb, Audit, AxisState, Goal, Params, UndecidedCause, Verdict,
};

fn main() {
    let d = 100.0_f32;
    let a = 4.0_f32;
    let dt = 1.0 / 64.0_f32;
    let params = Params::new(a, dt, 1_000_000);
    let body = RigidBody::new_dynamic(Vec3Fix::ZERO, Fix128::ONE);
    let goal = Goal::PositionWithinAndAtRest {
        target: Aabb {
            min: glam::Vec3::new(d - 0.1, -0.5, -0.5),
            max: glam::Vec3::new(d + 0.1, 0.5, 0.5),
        },
    };

    let h = lower_bound_frames(
        AxisState {
            distance_to_goal: d,
            velocity: 0.0,
        },
        &goal,
        &params,
    );
    println!("lattice lower bound: {h} frames");
    assert_eq!(h, 640);

    let exact = PhysicsConfig {
        gravity: Vec3Fix::ZERO,
        damping_rate: Fix128::ZERO,
        substeps: 1,
        ..PhysicsConfig::default()
    };
    let verdict = audit(&body, exact, &goal, &params);
    match &verdict {
        Audit::Proven(p) => println!("no damping, dyadic dt: Proven in {} frames", p.frames),
        other => println!("no damping, dyadic dt: {other:?}"),
    }
    assert!(matches!(&verdict, Audit::Proven(p) if p.frames == 640));

    // a retention of 0.99 per 1/64 s frame, as a rate per second: -ln(0.99) * 64
    let default_damping = PhysicsConfig {
        gravity: Vec3Fix::ZERO,
        damping_rate: -Fix128::from_ratio(99, 100).ln() * Fix128::from_int(64),
        ..PhysicsConfig::default()
    };
    let verdict = audit(&body, default_damping, &goal, &params);
    match &verdict {
        Audit::Undecided(UndecidedCause::ReplayMismatch {
            plan,
            position_within,
            at_rest,
        }) => println!(
            "default config: Undecided — lattice plan of {} frames replayed, \
             position_within={position_within}, at_rest={at_rest}",
            plan.frames
        ),
        other => println!("default config: {other:?}"),
    }
    assert_eq!(verdict.verdict(), Verdict::Undecided);
}
