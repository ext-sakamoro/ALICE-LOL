//! World laws against `alice-physics`: the values reach the types, and the plain text of a bounce
//! is what the engine does
//!
//! The rebound height of a body dropped from `h` with restitution `e` is `e^2 h` (Newton's law of
//! restitution with `v = sqrt(2 g h)`), compared here with a body dropped in the engine.
#![cfg(feature = "physics")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use alice_lol::world_law::WorldLaw;
use alice_physics::cloth::ClothConfig;
use alice_physics::deformable::DeformableConfig;
use alice_physics::math::Fix128;
use alice_physics::particle::ParticleEmitter;
use alice_physics::sdf_sph::SphConfig;
use alice_physics::{PhysicsConfig, PhysicsWorld, RigidBody, SleepConfig, Vec3Fix};

const ALL: &str = "law all
kind world
material a density = 1500
material a friction = 0.25
material a static_friction = 0.75
material a bounce = 0.5
material a stretch_compliance = 0.125
material a bend_compliance = 0.375
material a edge_compliance = 0.625
material a volume_compliance = 0.875
material a shape_stiffness = 0.5
material a damping = 0.25
material a sdf_friction = 0.0625
rain = on
rain_rate = 36
";

const SHORT: &str = "law short
kind world
material a density = 1500
material a friction = 0.25
material a bounce = 0.5
";

fn fix(n: i64, d: i64) -> Fix128 {
    Fix128::from_ratio(n, d)
}

#[test]
fn the_values_reach_a_physics_material() {
    let w = WorldLaw::from_file(ALL).unwrap().bind(&[]).unwrap();
    let m = w.material("a").unwrap();
    let p = m.physics_material(7);
    assert_eq!(p.id, 7);
    assert_eq!(p.dynamic_friction, fix(1, 4));
    assert_eq!(p.static_friction, fix(3, 4));
    assert_eq!(p.restitution, fix(1, 2));
    assert_eq!(m.density_fix(), Fix128::from_int(1500));
}

#[test]
fn the_values_reach_a_cloth_and_a_deformable() {
    let w = WorldLaw::from_file(ALL).unwrap().bind(&[]).unwrap();
    let m = w.material("a").unwrap();
    let c = m.apply_to_cloth(ClothConfig::default());
    assert_eq!(c.stretch_compliance, fix(1, 8));
    assert_eq!(c.bend_compliance, fix(3, 8));
    assert_eq!(c.damping, fix(1, 4));
    assert_eq!(c.sdf_friction, fix(1, 16));
    let d = m.apply_to_deformable(DeformableConfig::default());
    assert_eq!(d.edge_compliance, fix(5, 8));
    assert_eq!(d.volume_compliance, fix(7, 8));
    assert_eq!(d.shape_stiffness, fix(1, 2));
    assert_eq!(d.damping, fix(1, 4));
    assert_eq!(d.sdf_friction, fix(1, 16));
}

#[test]
fn a_field_the_law_does_not_state_stays_as_the_configuration_has_it() {
    let w = WorldLaw::from_file(SHORT).unwrap().bind(&[]).unwrap();
    let m = w.material("a").unwrap();
    let (c0, d0) = (ClothConfig::default(), DeformableConfig::default());
    let c = m.apply_to_cloth(c0);
    assert_eq!(c.stretch_compliance, c0.stretch_compliance);
    assert_eq!(c.bend_compliance, c0.bend_compliance);
    assert_eq!(c.damping, c0.damping);
    assert_eq!(c.sdf_friction, c0.sdf_friction);
    let d = m.apply_to_deformable(d0);
    assert_eq!(d.edge_compliance, d0.edge_compliance);
    assert_eq!(d.volume_compliance, d0.volume_compliance);
    assert_eq!(d.shape_stiffness, d0.shape_stiffness);
    assert_eq!(d.damping, d0.damping);
    assert_eq!(d.sdf_friction, d0.sdf_friction);
}

#[test]
fn the_values_reach_a_body() {
    let w = WorldLaw::from_file(ALL).unwrap().bind(&[]).unwrap();
    let mut body = RigidBody::new_dynamic(Vec3Fix::ZERO, Fix128::ONE);
    w.material("a").unwrap().apply_to_body(&mut body);
    assert_eq!(body.friction, fix(1, 4));
    assert_eq!(body.restitution, fix(1, 2));
}

/// 36 mm/h is 0.01 kg a second on a square metre, so 2 m^2 at 0.5 kg a particle is 0.04 a second
#[test]
fn rain_sets_the_rate_of_an_emitter() {
    let w = WorldLaw::from_file(ALL).unwrap().bind(&[]).unwrap();
    let emitter = |mass: Fix128| {
        ParticleEmitter::new(
            Vec3Fix::ZERO,
            Vec3Fix::from_int(0, -1, 0),
            Fix128::ZERO,
            fix(7, 1),
            Fix128::ONE,
            Fix128::ONE,
            mass,
        )
    };
    let mut e = emitter(fix(1, 2));
    assert!(w.rain().apply_to_emitter(&mut e, 2.0));
    assert!((e.emission_rate.to_f64() - 0.04).abs() < 1e-15);

    // refused, and the emitter as it was
    for (mass, area) in [
        (fix(1, 2), 0.0),
        (fix(1, 2), -1.0),
        (fix(1, 2), f64::NAN),
        (fix(1, 2), f64::INFINITY),
        (Fix128::ZERO, 2.0),
    ] {
        let mut e = emitter(mass);
        assert!(!w.rain().apply_to_emitter(&mut e, area), "{area}");
        assert_eq!(e.emission_rate, fix(7, 1));
    }
}

/// oracle: 1 mm of water on 1 m^2 is 1 litre, 1 kg, so r mm/h on A m^2 is r A / 3600 kg/s. What an
/// emitter sends out in a second, its rate times its particle mass, is that mass whatever the size
/// of the drop.
#[test]
fn the_mass_an_emitter_sends_is_the_closed_form_of_the_rain() {
    for (rate, area, mass) in [
        (4.0, 1.0, 1e-3),
        (36.0, 2.0, 0.5),
        (0.5, 100.0, 3.3e-5),
        (100.0, 0.25, 2e-2),
    ] {
        let text = format!("law r\nkind world\nrain = on\nrain_rate = {rate}\n");
        let rain = WorldLaw::from_file(&text)
            .unwrap()
            .bind(&[])
            .unwrap()
            .rain();
        let mut e = ParticleEmitter::new(
            Vec3Fix::ZERO,
            Vec3Fix::from_int(0, -1, 0),
            Fix128::ZERO,
            Fix128::ZERO,
            Fix128::ONE,
            Fix128::ONE,
            Fix128::from_f64(mass),
        );
        assert!(rain.apply_to_emitter(&mut e, area));
        let per_second = (e.emission_rate * e.particle_mass).to_f64();
        let closed_form = rate * area / 3600.0;
        assert!(
            (per_second - closed_form).abs() <= closed_form * 1e-12,
            "{rate} mm/h on {area} m^2 at {mass} kg: {per_second} kg/s, not {closed_form}"
        );
    }
}

/// With the rain off, an emitter sends nothing.
#[test]
fn rain_that_is_off_sends_nothing() {
    let text = "law r\nkind world\nrain = off\nrain_rate = 36\n";
    let rain = WorldLaw::from_file(text).unwrap().bind(&[]).unwrap().rain();
    let mut e = ParticleEmitter::new(
        Vec3Fix::ZERO,
        Vec3Fix::from_int(0, -1, 0),
        Fix128::ZERO,
        fix(7, 1),
        Fix128::ONE,
        Fix128::ONE,
        fix(1, 2),
    );
    assert!(rain.apply_to_emitter(&mut e, 2.0));
    assert_eq!(e.emission_rate, Fix128::ZERO);
}

#[test]
fn rain_gives_particles_for_the_mass_of_an_sph_configuration() {
    let w = WorldLaw::from_file(ALL).unwrap().bind(&[]).unwrap();
    let config = SphConfig {
        particle_mass: 0.5,
        ..SphConfig::water_like()
    };
    // 36 mm/h is 0.01 kg a second on a square metre, 0.02 particles of 0.5 kg
    assert_eq!(w.rain().particle_rate_for(&config), Some(0.02));
}

/// The apex after the first bounce of a ball of radius 1/2 dropped from 1 m on a ground of the
/// same material, in metres above the ground
fn rebound_apex(bounce: &str) -> f64 {
    let text = format!(
        "law b\nkind world\nmaterial a density = 1000\nmaterial a friction = 0\nmaterial a bounce = {bounce}\n"
    );
    let bound = WorldLaw::from_file(&text).unwrap().bind(&[]).unwrap();
    let material = bound.material("a").unwrap().physics_material(0);
    let mut w = PhysicsWorld::new(PhysicsConfig {
        gravity: Vec3Fix::new(Fix128::ZERO, Fix128::from_int(-10), Fix128::ZERO),
        damping_rate: Fix128::ZERO,
        ..PhysicsConfig::default()
    });
    w.set_sleep_config(SleepConfig {
        frames_to_sleep: u32::MAX,
        ..SleepConfig::default()
    });
    let r = Fix128::from_int(1000);
    let ground = w.add_body_with_radius(
        RigidBody::new_static(Vec3Fix::new(Fix128::ZERO, -r, Fix128::ZERO)),
        r,
    );
    let id = w.material_table.register(material);
    w.set_body_material(ground, id);
    let ball = w.add_body_with_radius(
        RigidBody::new_dynamic(
            Vec3Fix::new(Fix128::ZERO, fix(3, 2), Fix128::ZERO),
            Fix128::ONE,
        ),
        fix(1, 2),
    );
    w.set_body_material(ball, id);
    let dt = fix(1, 60);
    let mut heights = Vec::with_capacity(180);
    for _ in 0..180 {
        w.step(dt);
        heights.push(w.bodies[ball].position.y.to_f64() - 0.5);
    }
    let hit = heights.iter().position(|y| *y < 1e-2).expect("it landed");
    heights[hit..].iter().copied().fold(f64::MIN, f64::max)
}

/// `describe` says a body dropped from 1 m comes back up `e^2` m, and the engine agrees within the
/// integration budget of the engine's own bounce test: `(1 + e + e^2) v h + g dt^2 / 8`
#[test]
fn the_rebound_in_the_plain_text_is_what_the_engine_does() {
    let (g, dt) = (10.0f64, 1.0f64 / 60.0);
    let h = dt / f64::from(u32::try_from(PhysicsConfig::default().substeps).unwrap());
    for (text, e) in [("0.25", 0.25f64), ("0.5", 0.5), ("0.75", 0.75)] {
        let v = (2.0 * g).sqrt();
        let budget = (1.0 + e + e * e) * v * h + g * dt * dt / 8.0;
        let apex = rebound_apex(text);
        assert!(
            (apex - e * e).abs() <= budget,
            "bounce {e}: apex {apex}, e^2 {}, budget {budget}",
            e * e
        );
    }
}
