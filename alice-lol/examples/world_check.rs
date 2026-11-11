//! Read a world law and print its identifier and what it does, in plain text
//!
//! ```text
//! world_check <law file> [--param name=value]... [--switch name=on|off]...
//! world_check --semantics                       # the measured pins and their fold
//! ```
//!
//! With the feature `physics`, it also prints what each material sets on the types of
//! `alice-physics`, and how many particles of 1 g the rain gives on a square metre.
//!
//! Exit status: 0 read and bound, 1 refused, 2 usage or I/O error.

use std::process::ExitCode;

use alice_lol::world_law::{
    measured_world_semantics_id, world_semantics_fingerprints, WorldLaw, WorldValue,
};

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: world_check <law file> [--param name=value]... [--switch name=on|off]... [--frequency <Hz>]..."
    );
    eprintln!("       world_check --semantics");
    ExitCode::from(2)
}

#[cfg(feature = "physics")]
fn print_physics(bound: &alice_lol::world_law::BoundWorld) {
    use alice_physics::cloth::ClothConfig;
    use alice_physics::deformable::DeformableConfig;
    use alice_physics::math::{Fix128, Vec3Fix};
    use alice_physics::particle::ParticleEmitter;
    use alice_physics::sdf_sph::SphConfig;
    use alice_physics::solver::RigidBody;

    for name in bound.materials().iter().map(|m| m.name.clone()) {
        let Some(m) = bound.material(&name) else {
            continue;
        };
        let material = m.physics_material(0);
        let mut body = RigidBody::new_dynamic(Vec3Fix::ZERO, Fix128::ONE);
        m.apply_to_body(&mut body);
        let cloth = m.apply_to_cloth(ClothConfig::default());
        let deformable = m.apply_to_deformable(DeformableConfig::default());
        println!(
            "physics {name}: density {} restitution {} body friction {} cloth stretch {} deformable edge {}",
            m.density_fix().to_f64(),
            material.restitution.to_f64(),
            body.friction.to_f64(),
            cloth.stretch_compliance.to_f64(),
            deformable.edge_compliance.to_f64(),
        );
    }
    let rain = bound.rain();
    let mut emitter = ParticleEmitter::new(
        Vec3Fix::ZERO,
        Vec3Fix::from_int(0, -1, 0),
        Fix128::ZERO,
        Fix128::ZERO,
        Fix128::ONE,
        Fix128::ONE,
        Fix128::from_ratio(1, 1000),
    );
    if rain.apply_to_emitter(&mut emitter, 1.0) {
        println!(
            "physics rain: {} drops of 1 g on a square metre each second",
            emitter.emission_rate.to_f64()
        );
    }
    if let Some(rate) = rain.particle_rate_for(&SphConfig::water_like()) {
        println!("physics rain: {rate} SPH particles on a square metre each second");
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--semantics") {
        for (name, pin) in world_semantics_fingerprints() {
            println!("{} {name}", hex(&pin));
        }
        println!("{} fold", hex(&measured_world_semantics_id()));
        return ExitCode::SUCCESS;
    }
    let Some(path) = args.first() else {
        return usage();
    };
    let mut values: Vec<(&str, WorldValue)> = Vec::new();
    let mut frequencies: Vec<f64> = Vec::new();
    let mut rest = args[1..].iter();
    while let Some(flag) = rest.next() {
        if flag == "--frequency" {
            match rest.next().and_then(|v| v.parse().ok()) {
                Some(f) => frequencies.push(f),
                None => return usage(),
            }
            continue;
        }
        let Some((name, value)) = rest.next().and_then(|a| a.split_once('=')) else {
            return usage();
        };
        let value = match (flag.as_str(), value) {
            ("--param", v) => match v.parse() {
                Ok(n) => WorldValue::Number(n),
                Err(_) => return usage(),
            },
            ("--switch", "on") => WorldValue::Switch(true),
            ("--switch", "off") => WorldValue::Switch(false),
            _ => return usage(),
        };
        values.push((name, value));
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    let law = match WorldLaw::from_file(&text) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(1);
        }
    };
    match law.bind(&values) {
        Ok(bound) => {
            println!("id: {}", hex(&law.law_id()));
            print!("{}", bound.describe());
            print_grips(&bound);
            print_dynamic_at(&bound, &frequencies);
            #[cfg(feature = "physics")]
            print_physics(&bound);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{path}: {e}");
            ExitCode::from(1)
        }
    }
}

/// One line per material the hand can hold, for a program to read: the values in SI units and the
/// exact bits of the derived stiffness
fn print_grips(bound: &alice_lol::world_law::BoundWorld) {
    if let Some(h) = bound.hand() {
        println!(
            "hand_skin: youngs_modulus_pa={} poisson_ratio={}",
            h.youngs_modulus, h.poisson_ratio
        );
    }
    for m in bound.materials() {
        if let Some(d) = &m.dynamic {
            let rows: Vec<String> = d
                .rows
                .iter()
                .map(|r| format!("{}:{}:{}", r.frequency, r.storage, r.loss))
                .collect();
            let kind = match d.kind {
                alice_lol::world_law::ModulusKind::Shear => "shear",
                alice_lol::world_law::ModulusKind::Youngs => "youngs",
            };
            let basis = match d.basis {
                Some(alice_lol::world_law::Basis::Measured) => "measured",
                Some(alice_lol::world_law::Basis::Assumed) => "assumed",
                None => "none",
            };
            println!(
                "dynamic {}: kind={kind} basis={basis} rows_hz_storage_pa_loss_pa={}",
                m.name,
                rows.join(",")
            );
        }
        if let Some(a) = m.contact_angle {
            let basis = match m.contact_angle_basis {
                Some(alice_lol::world_law::Basis::Measured) => "measured",
                Some(alice_lol::world_law::Basis::Assumed) => "assumed",
                None => "none",
            };
            println!("contact_angle {}: degrees={a} basis={basis}", m.name);
        }
        if let Some(g) = bound.grip(&m.name) {
            let adhesion = g
                .adhesion
                .map_or_else(|| "none".to_owned(), |f| f.to_string());
            let spread = g
                .stiffness_spread
                .map_or_else(|| "none".to_owned(), |(lo, hi)| format!("{lo}..{hi}"));
            println!(
                "grip {}: stiffness_n_per_m={} bits={:#018x} adhesion_n={adhesion} contact_radius_m={} stiffness_spread_n_per_m={spread}",
                m.name,
                g.stiffness,
                g.stiffness.to_bits(),
                g.contact_radius
            );
        }
    }
}

/// The dynamic moduli of every material at each `--frequency`, interpolated inside its table
fn print_dynamic_at(bound: &alice_lol::world_law::BoundWorld, frequencies: &[f64]) {
    for f in frequencies {
        for m in bound.materials() {
            let Some(d) = &m.dynamic else { continue };
            match d.at(*f) {
                Some((storage, loss)) => println!(
                    "dynamic_at {} {f}: storage_pa={storage} loss_pa={loss}",
                    m.name
                ),
                None => println!("dynamic_at {} {f}: not stated", m.name),
            }
        }
    }
}
