//! World laws (`kind world`): reading, refusing, binding, the identifier and the plain text
//!
//! The expected values are worked out by hand from the law text and are not computed by the code
//! under test. The golden test compares bits, and `scripts/frame_targets.sh` runs this file on
//! wasm32-wasip1 under wasmtime too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::fmt::Write as _;

use alice_lol::law_id::fold_semantics_pins;
use alice_lol::world_law::{
    measured_world_semantics_id, world_semantics_fingerprints, Basis, BindError, ModulusKind,
    WorldLaw,
    WorldValue::{Number, Switch},
    MAX_VALUE, WORLD_SEMANTICS_ID, WORLD_SEMANTICS_PINS,
};

const MOCHI: &str = include_str!("../../laws/world/mochi_in_the_rain.law");

const ONE: &str = "law one
kind world
material a density = 1000
material a friction = 0.5
material a bounce = 0.5
";

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn refused(text: &str, line: usize, part: &str) {
    let e = WorldLaw::from_file(text).expect_err(text);
    assert_eq!(e.line, line, "{e}");
    assert!(e.message.contains(part), "{e}");
}

#[test]
fn the_values_are_the_ones_the_law_states() {
    let w = WorldLaw::from_file(MOCHI).unwrap().bind(&[]).unwrap();
    let m = w.material("mochi").unwrap();
    assert_eq!(m.density, 1100.0);
    assert_eq!(m.friction, 0.6);
    assert_eq!(m.static_friction, 0.6);
    assert_eq!(m.bounce, 0.2);
    // mochi is a solid: the law states no cloth or deformable parameters for it
    assert_eq!(m.stretch_compliance, None);
    assert_eq!(m.bend_compliance, None);
    assert_eq!(m.edge_compliance, None);
    assert_eq!(m.volume_compliance, None);
    assert_eq!(m.shape_stiffness, None);
    assert_eq!(m.damping, None);
    // the scene's mochi is the assumed combination; the measured conditions are their own materials
    assert_eq!(m.dynamic.as_ref().unwrap().basis, Some(Basis::Assumed));
    let isono = w.material("mochi_48h_30c").unwrap();
    let row = isono.dynamic.as_ref().unwrap().rows[0];
    assert_eq!(
        (row.frequency, row.storage, row.loss),
        (0.1, 427_000.0, 10_000.0)
    );
    assert_eq!(isono.dynamic.as_ref().unwrap().basis, Some(Basis::Measured));
    assert_eq!(m.sdf_friction, None);
    assert_eq!(m.shear_modulus, Some(427_000.0));
    assert_eq!(m.poisson_ratio, Some(0.49));
    assert_eq!(m.shear_modulus_basis, Some(Basis::Measured));
    assert_eq!(m.poisson_ratio_basis, Some(Basis::Assumed));
    // E = 2 G (1 + nu) = 2 * 427000 * 1.49
    assert!((m.youngs_modulus().unwrap() - 1_272_460.0).abs() < 1e-6);
    assert!(w.rain().on);
    assert_eq!(w.rain().rate_mm_per_hour, 4.0);
    assert!(w.material("nothing").is_none());
}

#[test]
fn softer_is_another_value_of_the_same_law() {
    let law = WorldLaw::from_file(MOCHI).unwrap();
    let w = law.bind(&[("strength", Number(9.0))]).unwrap();
    assert_eq!(w.rain().rate_mm_per_hour, 9.0);
    let before = law.law_id();
    assert_eq!(law.law_id(), before, "binding leaves the identifier");
}

#[test]
fn every_field_reaches_its_own_slot() {
    let law = WorldLaw::from_file(
        "law all
kind world
material a density = 1
material a friction = 2
material a static_friction = 3
material a bounce = 0.25
material a stretch_compliance = 5
material a bend_compliance = 6
material a edge_compliance = 7
material a volume_compliance = 8
material a shape_stiffness = 0.5
material a damping = 0.75
material a sdf_friction = 0.875
",
    )
    .unwrap();
    let w = law.bind(&[]).unwrap();
    let m = &w.materials()[0];
    assert_eq!(
        (m.density, m.friction, m.static_friction, m.bounce),
        (1.0, 2.0, 3.0, 0.25)
    );
    assert_eq!(m.stretch_compliance, Some(5.0));
    assert_eq!(m.bend_compliance, Some(6.0));
    assert_eq!(m.edge_compliance, Some(7.0));
    assert_eq!(m.volume_compliance, Some(8.0));
    assert_eq!(m.shape_stiffness, Some(0.5));
    assert_eq!(m.damping, Some(0.75));
    assert_eq!(m.sdf_friction, Some(0.875));
}

#[test]
fn a_let_and_the_functions_are_read_in_order() {
    let law = WorldLaw::from_file(
        "law l
kind world
param k number = 2 range 0 4
let twice = 2 * k
let more = twice + 1
material a density = more ^ 2
material a friction = sqrt(more - 1)
material a bounce = cos(pi) + 1
",
    )
    .unwrap();
    let w = law.bind(&[]).unwrap();
    let m = &w.materials()[0];
    assert_eq!(m.density, 25.0);
    assert_eq!(m.friction, 2.0);
    assert_eq!(m.bounce, 0.0);
    assert_eq!(m.bounce.to_bits(), 0.0f64.to_bits());
}

#[test]
fn rain_follows_its_switch_and_its_flux_is_the_rate_over_3600() {
    let law = WorldLaw::from_file(MOCHI).unwrap();
    // 36 mm/h: 36 litres on a square metre an hour, 0.01 kg a second
    let wet = law.bind(&[("strength", Number(36.0))]).unwrap().rain();
    assert!(wet.on);
    assert_eq!(wet.mass_flux(), 0.01);
    assert_eq!(wet.particle_rate(0.002), Some(5.0));
    assert_eq!(wet.particle_rate(0.0), None);
    assert_eq!(wet.particle_rate(-1.0), None);
    assert_eq!(wet.particle_rate(f64::NAN), None);
    assert_eq!(wet.particle_rate(f64::INFINITY), None);

    let dry = law
        .bind(&[("strength", Number(36.0)), ("raining", Switch(false))])
        .unwrap()
        .rain();
    assert!(!dry.on);
    assert_eq!(dry.rate_mm_per_hour, 36.0, "the stated strength stays");
    assert_eq!(dry.mass_flux(), 0.0);
    assert_eq!(dry.particle_rate(0.002), Some(0.0));
}

#[test]
fn rain_can_be_fixed_or_not_stated() {
    let fixed = |v: &str| {
        WorldLaw::from_file(&format!("{ONE}rain = {v}\nrain_rate = 3\n"))
            .unwrap()
            .bind(&[])
            .unwrap()
            .rain()
            .on
    };
    assert!(fixed("on"));
    assert!(!fixed("off"));
    let none = WorldLaw::from_file(ONE).unwrap().bind(&[]).unwrap();
    assert!(!none.rain().on);
    assert_eq!(none.rain().rate_mm_per_hour, 0.0);
    assert!(none.describe().contains("rain: not stated"));
    // a law of rain alone
    let only = WorldLaw::from_file("law r\nkind world\nrain = on\nrain_rate = 1\n").unwrap();
    assert!(only.bind(&[]).unwrap().materials().is_empty());
}

#[test]
fn a_value_outside_its_field_is_refused_naming_the_line() {
    for (field, v, range) in [
        ("density", "0", "above 0"),
        ("density", "0 - 1", "above 0"),
        ("density", "2 ^ 54", "above 0"),
        ("friction", "0 - 0.5", "0..=2^53"),
        ("friction", "2 ^ 54", "0..=2^53"),
        ("bounce", "1.5", "0..=1"),
        ("bounce", "0 - 0.5", "0..=1"),
    ] {
        let mut lines = ["density = 1000", "friction = 0.5", "bounce = 0.5"];
        let wrong = format!("{field} = {v}");
        let at = lines.iter().position(|l| l.starts_with(field)).unwrap();
        lines[at] = &wrong;
        let text = format!(
            "law w\nkind world\nmaterial a {}\nmaterial a {}\nmaterial a {}\n",
            lines[0], lines[1], lines[2]
        );
        let e = WorldLaw::from_file(&text).expect_err(&text);
        assert!(e.to_string().contains(&format!("line {}", at + 3)), "{e}");
        assert!(e.to_string().contains(range), "{e}");
    }
    for (field, v, range) in [
        ("stretch_compliance", "0 - 1", "0..=2^53"),
        ("bend_compliance", "0 - 1", "0..=2^53"),
        ("edge_compliance", "0 - 1", "0..=2^53"),
        ("volume_compliance", "0 - 1", "0..=2^53"),
        ("shape_stiffness", "1.5", "0..=1"),
        ("shape_stiffness", "0 - 1", "0..=1"),
        ("damping", "1.5", "0..=1"),
        ("damping", "0 - 1", "0..=1"),
        ("sdf_friction", "1.5", "0..=1"),
        ("sdf_friction", "0 - 1", "0..=1"),
    ] {
        let text = format!("{ONE}material a {field} = {v}\n");
        let e = WorldLaw::from_file(&text).expect_err(&text);
        assert!(e.to_string().contains("line 6"), "{e}");
        assert!(e.to_string().contains(range), "{e}");
    }
}

#[test]
fn elastic_constants_are_optional_and_their_basis_is_stated() {
    let w = WorldLaw::from_file(ONE).unwrap().bind(&[]).unwrap();
    let m = &w.materials()[0];
    assert_eq!(
        (m.shear_modulus, m.poisson_ratio, m.youngs_modulus()),
        (None, None, None)
    );
    assert!(w.describe().contains("elastic constants: not stated"));
    // one without the other: no Young's modulus, and the missing one is said
    let only_g = WorldLaw::from_file(&format!("{ONE}material a shear_modulus = 1000\n"))
        .unwrap()
        .bind(&[])
        .unwrap();
    assert_eq!(only_g.materials()[0].youngs_modulus(), None);
    let text = only_g.describe();
    assert!(
        text.contains("shear modulus 1.0000e3 Pa (basis not stated)"),
        "{text}"
    );
    assert!(text.contains("Poisson's ratio: not stated"), "{text}");
    assert!(!text.contains("Young"), "{text}");
    // the basis comes after the value and only for these two fields
    refused(
        &format!("{ONE}material a shear_modulus basis measured\n"),
        6,
        "not stated before its basis",
    );
    refused(
        &format!("{ONE}material a density basis measured\n"),
        6,
        "takes no basis",
    );
    refused(
        &format!("{ONE}material b poisson_ratio basis assumed\n"),
        6,
        "has no field yet",
    );
    refused(
        &format!("{ONE}material a poisson_ratio = 0.3\nmaterial a poisson_ratio basis guessed\n"),
        7,
        "measured or assumed",
    );
    refused(
        &format!("{ONE}material a poisson_ratio = 0.3\nmaterial a poisson_ratio basis assumed\nmaterial a poisson_ratio basis measured\n"),
        8,
        "given twice",
    );
    // the basis is in the identifier
    let id = |b: &str| {
        WorldLaw::from_file(&format!(
            "{ONE}material a poisson_ratio = 0.3\nmaterial a poisson_ratio basis {b}\n"
        ))
        .unwrap()
        .law_id()
    };
    assert_ne!(id("assumed"), id("measured"));
    // ranges: G above 0, nu in -1..0.5
    for (field, v, range) in [
        ("shear_modulus", "0", "above 0"),
        ("poisson_ratio", "0.5", "-1..0.5"),
        ("poisson_ratio", "0 - 1.5", "-1..0.5"),
    ] {
        let text = format!("{ONE}material a {field} = {v}\n");
        let e = WorldLaw::from_file(&text).expect_err(&text);
        assert!(e.to_string().contains(range), "{e}");
    }
    assert!(WorldLaw::from_file(&format!("{ONE}material a poisson_ratio = 0 - 1\n")).is_ok());
}

#[test]
fn the_edges_of_every_range_are_in_it() {
    let w = WorldLaw::from_file(
        "law e
kind world
material a density = 2 ^ 53
material a friction = 2 ^ 53
material a bounce = 1
material a shape_stiffness = 1
material a damping = 1
material b density = 0.001
material b friction = 0
material b bounce = 0
material b shape_stiffness = 0
material b damping = 0
rain = on
rain_rate = 2 ^ 53
",
    )
    .unwrap()
    .bind(&[])
    .unwrap();
    let a = w.material("a").unwrap();
    assert_eq!(a.density, MAX_VALUE);
    assert_eq!(a.friction, MAX_VALUE);
    assert_eq!(w.rain().rate_mm_per_hour, MAX_VALUE);
    assert_eq!(a.bounce, 1.0);
    assert_eq!(w.material("b").unwrap().bounce, 0.0);
}

#[test]
fn negative_zero_reads_as_zero() {
    let w = WorldLaw::from_file(
        "law z
kind world
material a density = 1
material a friction = 0 * (0 - 1)
material a bounce = 0
rain = on
rain_rate = 0 * (0 - 1)
",
    )
    .unwrap()
    .bind(&[])
    .unwrap();
    assert_eq!(w.materials()[0].friction.to_bits(), 0);
    assert_eq!(w.rain().rate_mm_per_hour.to_bits(), 0);
}

#[test]
fn a_rain_rate_outside_its_range_is_refused_naming_the_line() {
    let e = WorldLaw::from_file(&format!("{ONE}rain = on\nrain_rate = 0 - 1\n")).unwrap_err();
    assert!(e.to_string().contains("line 7"), "{e}");
    let e = WorldLaw::from_file(&format!("{ONE}rain = on\nrain_rate = 2 ^ 54\n")).unwrap_err();
    assert!(e.to_string().contains("0..=2^53"), "{e}");
}

#[test]
fn bad_parameter_values_are_refused() {
    let law = WorldLaw::from_file(MOCHI).unwrap();
    let e = |v: &[(&str, _)]| law.bind(v).unwrap_err();
    assert_eq!(
        e(&[("nope", Number(1.0))]),
        BindError::UnknownParam("nope".into())
    );
    assert_eq!(
        e(&[("strength", Number(0.0)), ("strength", Number(0.0))]),
        BindError::RepeatedParam("strength".into())
    );
    for v in [-0.001, 100.001, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            e(&[("strength", Number(v))]),
            BindError::ParamOutOfRange("strength".into())
        );
    }
    assert_eq!(
        e(&[("strength", Switch(true))]),
        BindError::WrongType("strength".into())
    );
    assert_eq!(
        e(&[("raining", Number(1.0))]),
        BindError::WrongType("raining".into())
    );
    // the ends of a declared range are in it
    assert!(law.bind(&[("strength", Number(0.0))]).is_ok());
    assert!(law.bind(&[("strength", Number(100.0))]).is_ok());
}

#[test]
fn a_value_that_is_not_finite_is_refused_when_bound() {
    let law = WorldLaw::from_file(
        "law n
kind world
param k number = 1 range 0 1
let d = 1 / k
material a density = d
material a friction = 0
material a bounce = 0
",
    )
    .unwrap();
    let e = law.bind(&[("k", Number(0.0))]).unwrap_err();
    assert!(matches!(e, BindError::NonFinite { line: 4, .. }), "{e}");
}

#[test]
fn what_does_not_read_is_refused_naming_the_line() {
    refused("", 0, "empty");
    refused("law x\n", 0, "not a world law");
    refused("kind world\n", 1, "first line");
    refused("law X\nkind world\n", 1, "first line");
    refused("law x\nkind motion\n", 2, "second line");
    refused("law x\nkind world\n", 0, "no material and no rain");
    refused("law x\nkind world\nfoo\n", 3, "unknown line");
    refused("law x\nkind world\nmaterial a\n", 3, "expected");
    refused("law x\nkind world\nmaterial a b c = 1\n", 3, "expected");
    refused(
        "law x\nkind world\nmaterial A density = 1\n",
        3,
        "not a name",
    );
    refused(
        "law x\nkind world\nmaterial a softness = 1\n",
        3,
        "not a field",
    );
    refused("law x\nkind world\nmaterial a density = nope\n", 3, "nope");
    refused(
        "law x\nkind world\nmaterial a density = 1\nmaterial a density = 2\n",
        4,
        "twice",
    );
    refused(
        "law x\nkind world\nmaterial a density = 1\nmaterial a friction = 1\n",
        0,
        "no `bounce`",
    );
    refused(
        "law x\nkind world\nmaterial a density = 1\nmaterial a bounce = 1\n",
        0,
        "no `friction`",
    );
    refused(
        "law x\nkind world\nmaterial a friction = 1\nmaterial a bounce = 1\n",
        0,
        "no `density`",
    );
    refused("law x\nkind world\nparam a\n", 3, "expected");
    refused(
        "law x\nkind world\nparam a number = 5 range 0 1\n",
        3,
        "outside the range",
    );
    refused(
        "law x\nkind world\nparam a number = 1e0 range 0 1\n",
        3,
        "plain decimals",
    );
    refused("law x\nkind world\nparam a switch = yes\n", 3, "on or off");
    refused(
        "law x\nkind world\nparam pi number = 1 range 0 1\n",
        3,
        "reserved",
    );
    refused(
        "law x\nkind world\nparam a switch = on\nparam a switch = off\n",
        4,
        "reserved",
    );
    refused("law x\nkind world\nlet a\n", 3, "expected");
    refused("law x\nkind world\nlet a = 1\nlet a = 2\n", 4, "reserved");
    refused("law x\nkind world\nrain on\n", 3, "expected");
    refused("law x\nkind world\nrain = maybe\n", 3, "not on, off");
    refused(
        "law x\nkind world\nparam n number = 1 range 0 1\nrain = n\n",
        4,
        "not on, off",
    );
    refused("law x\nkind world\nrain = on\n", 3, "without `rain_rate`");
    refused("law x\nkind world\nrain_rate = 1\n", 3, "without `rain`");
    refused("law x\nkind world\nrain_rate 1\n", 3, "expected");
    refused(
        "law x\nkind world\nrain = on\nrain = off\nrain_rate = 1\n",
        4,
        "second `rain`",
    );
    refused(
        "law x\nkind world\nrain = on\nrain_rate = 1\nrain_rate = 2\n",
        5,
        "second `rain_rate`",
    );
    refused("law x\nkind world\n\u{7}\n", 3, "control character");
    let many = "# x\n".repeat(1025);
    refused(&many, 0, "more than 1024");
}

#[test]
fn a_switch_is_not_a_number_in_an_expression() {
    refused(
        "law x\nkind world\nparam s switch = on\nmaterial a density = s\n",
        4,
        "s",
    );
}

#[test]
fn the_limits_hold() {
    let mut params = String::from("law x\nkind world\nrain = on\nrain_rate = 1\n");
    for i in 0..65 {
        writeln!(params, "param p{i} switch = on").unwrap();
    }
    refused(&params, 69, "more than 64 parameters");

    let mut lets = String::from("law x\nkind world\nrain = on\nrain_rate = 1\n");
    for i in 0..257 {
        writeln!(lets, "let v{i} = 1").unwrap();
    }
    refused(&lets, 261, "more than 256");

    let mut mats = String::from("law x\nkind world\n");
    for i in 0..65 {
        writeln!(mats, "material m{i} density = 1").unwrap();
    }
    refused(&mats, 67, "more than 64 materials");
}

#[test]
fn the_identifier_sees_the_meaning_only() {
    let id = |t: &str| WorldLaw::from_file(t).unwrap().law_id();
    let base = id(ONE);
    // notes, comments, spacing and the order of the lines are not in it
    assert_eq!(
        base,
        id("# note\nlaw one\nkind world\nclaim x\nmaterial a bounce=0.5\nmaterial a density =  1000\nmaterial a friction = 0.5\n")
    );
    assert_eq!(
        id("law t\nkind world\nmaterial a density = 1\nmaterial a friction = 1\nmaterial a bounce = 1\nmaterial b density = 2\nmaterial b friction = 2\nmaterial b bounce = 0\n"),
        id("law t\nkind world\nmaterial b density = 2\nmaterial b friction = 2\nmaterial b bounce = 0\nmaterial a density = 1\nmaterial a friction = 1\nmaterial a bounce = 1\n")
    );
    // each of these is in it
    for other in [
        "law two\nkind world\nmaterial a density = 1000\nmaterial a friction = 0.5\nmaterial a bounce = 0.5\n",
        "law one\nkind world\nmaterial b density = 1000\nmaterial b friction = 0.5\nmaterial b bounce = 0.5\n",
        "law one\nkind world\nmaterial a density = 1001\nmaterial a friction = 0.5\nmaterial a bounce = 0.5\n",
        "law one\nkind world\nmaterial a density = 1000\nmaterial a friction = 0.5\nmaterial a bounce = 0.25\n",
        "law one\nkind world\nparam p number = 1 range 0 1\nmaterial a density = 1000\nmaterial a friction = 0.5\nmaterial a bounce = 0.5\n",
        "law one\nkind world\nparam p switch = on\nmaterial a density = 1000\nmaterial a friction = 0.5\nmaterial a bounce = 0.5\n",
        "law one\nkind world\nlet q = 1\nmaterial a density = 1000\nmaterial a friction = 0.5\nmaterial a bounce = 0.5\n",
    ] {
        assert_ne!(base, id(other), "{other}");
    }
    let swap = |d: &str| {
        id(&format!(
            "law s\nkind world\nparam p {d}\nrain = on\nrain_rate = 1\n"
        ))
    };
    assert_ne!(swap("number = 1 range 0 1"), swap("number = 0 range 0 1"));
    assert_ne!(swap("number = 1 range 0 1"), swap("number = 1 range 0 2"));
    assert_ne!(swap("number = 1 range 0 1"), swap("number = 1 range -1 1"));
    assert_ne!(swap("switch = on"), swap("switch = off"));
    let rain = |r: &str, v: &str| id(&format!("{ONE}rain = {r}\nrain_rate = {v}\n"));
    assert_ne!(rain("on", "1"), rain("off", "1"));
    assert_ne!(rain("on", "1"), rain("on", "2"));
    assert_ne!(base, rain("off", "0"));
    let named = |d: &str, r: &str| {
        id(&format!(
            "{ONE}param {d} switch = on\nparam other switch = on\nrain = {r}\nrain_rate = 1\n"
        ))
    };
    assert_ne!(named("s", "s"), named("s", "other"));
    assert_ne!(named("s", "s"), rain("on", "1"));
}

#[test]
fn the_plain_text_is_what_the_values_do() {
    let law = WorldLaw::from_file(MOCHI).unwrap();
    let text = law.bind(&[]).unwrap().describe();
    assert_eq!(
        text,
        "law: mochi_in_the_rain
material: mochi
  1100.000 kg in a cubic metre, 1.100 times water, sinks in water
  grip: sliding 0.600, at rest 0.600, the share of its weight it takes to move it
  bounce 0.200: dropped from 1 m on the same material, it comes back up 0.040 m
  shear modulus 4.2700e5 Pa (measured)
  Poisson's ratio 0.490 (assumed, not measured)
  Young's modulus 1.2725e6 Pa, derived as 2 G (1 + Poisson's ratio), not measured
  dynamic youngs modulus (E' and E'', assumed, not measured) stated at 4 frequencies 3.0000e0..1.0000e2 Hz; between them interpolated on log-log axes, outside them not stated
    3.0000e0 Hz: storage 1.2725e6 Pa, loss 4.2135e4 Pa, loss tangent 0.0331
    1.0000e1 Hz: storage 1.3952e6 Pa, loss 4.8378e4 Pa, loss tangent 0.0347
    3.0000e1 Hz: storage 1.3479e6 Pa, loss 6.9126e4 Pa, loss tangent 0.0513
    1.0000e2 Hz: storage 1.9039e6 Pa, loss 2.3423e5 Pa, loss tangent 0.1230
  adhesion 2.0000e0 N (assumed, not measured): the pull at which a hand's hold lets go
  contact area 2.0000e-4 m^2 (assumed, not measured)
  grip stiffness 5.1815e3 N/m, derived: the material (2.6721e4 N/m) and the hand (6.4279e3 N/m) in series, each 2 a E / (1 - Poisson's ratio^2) for a flat contact of radius a = 7.9788e-3 m on an elastic half-space (Boussinesq), valid while a is much smaller than the body's thickness
  grip stiffness over the skin's stated spread 4.0000e4..3.1000e5 Pa: 8.0443e2..5.1815e3 N/m
material: mochi_48h_30c
  1100.000 kg in a cubic metre, 1.100 times water, sinks in water
  grip: sliding 0.600, at rest 0.600, the share of its weight it takes to move it
  bounce 0.200: dropped from 1 m on the same material, it comes back up 0.040 m
  shear modulus 4.2700e5 Pa (measured)
  Poisson's ratio 0.490 (assumed, not measured)
  Young's modulus 1.2725e6 Pa, derived as 2 G (1 + Poisson's ratio), not measured
  dynamic shear modulus (G' and G'', measured) stated at one frequency, 1.0000e-1 Hz; other frequencies not stated
    1.0000e-1 Hz: storage 4.2700e5 Pa, loss 1.0000e4 Pa, loss tangent 0.0234
material: mochi_5d_19c
  1100.000 kg in a cubic metre, 1.100 times water, sinks in water
  grip: sliding 0.600, at rest 0.600, the share of its weight it takes to move it
  bounce 0.200: dropped from 1 m on the same material, it comes back up 0.040 m
  elastic constants: not stated
  dynamic youngs modulus (E' and E'', measured) stated at 4 frequencies 3.0000e0..1.0000e2 Hz; between them interpolated on log-log axes, outside them not stated
    3.0000e0 Hz: storage 1.3032e7 Pa, loss 5.0699e5 Pa, loss tangent 0.0389
    1.0000e1 Hz: storage 1.3964e7 Pa, loss 7.0795e5 Pa, loss tangent 0.0507
    3.0000e1 Hz: storage 1.3335e7 Pa, loss 7.7625e5 Pa, loss tangent 0.0582
    1.0000e2 Hz: storage 2.2131e7 Pa, loss 3.7154e6 Pa, loss tangent 0.1679
material: mochi_5d_30c
  1100.000 kg in a cubic metre, 1.100 times water, sinks in water
  grip: sliding 0.600, at rest 0.600, the share of its weight it takes to move it
  bounce 0.200: dropped from 1 m on the same material, it comes back up 0.040 m
  elastic constants: not stated
  dynamic youngs modulus (E' and E'', measured) stated at 4 frequencies 3.0000e0..1.0000e2 Hz; between them interpolated on log-log axes, outside them not stated
    3.0000e0 Hz: storage 1.0839e7 Pa, loss 3.5892e5 Pa, loss tangent 0.0331
    1.0000e1 Hz: storage 1.1885e7 Pa, loss 4.1210e5 Pa, loss tangent 0.0347
    3.0000e1 Hz: storage 1.1482e7 Pa, loss 5.8884e5 Pa, loss tangent 0.0513
    1.0000e2 Hz: storage 1.6218e7 Pa, loss 1.9953e6 Pa, loss tangent 0.1230
hand: skin Young's modulus 3.1000e5 Pa (assumed, not measured, reported values spread over 4.0000e4..3.1000e5 Pa), Poisson's ratio 0.480 (assumed, not measured)
rain: on, 4.000 mm an hour, 0.001111 kg of water on each square metre each second
"
    );
    let dry = law.bind(&[("raining", Switch(false))]).unwrap().describe();
    assert!(dry.ends_with("rain: off\n"), "{dry}");
    let density = |d: &str| {
        WorldLaw::from_file(&format!(
            "law d\nkind world\nmaterial a density = {d}\nmaterial a friction = 0\nmaterial a bounce = 0\n"
        ))
        .unwrap()
        .bind(&[])
        .unwrap()
        .describe()
    };
    assert!(density("999").contains("floats on water"));
    assert!(density("1000").contains("as dense as water"));
    assert!(density("1001").contains("sinks in water"));
}

/// Recorded on arm64, and the same on every target this file runs on
#[test]
fn the_bits_are_the_recorded_ones() {
    let law = WorldLaw::from_file(MOCHI).unwrap();
    assert_eq!(
        hex(&law.law_id()),
        "682b0a5e8615a112f12703fff96a88795804d54a103c5bb072125905a402b008"
    );
    let w = law.bind(&[("strength", Number(7.5))]).unwrap();
    // 7.5/3600 as its exact f64 bits
    assert_eq!(w.rain().mass_flux().to_bits(), (7.5f64 / 3600.0).to_bits());
    // a function goes through alice-det-math, so its bits are pinned too
    let f = WorldLaw::from_file(
        "law f\nkind world\nmaterial a density = 1\nmaterial a friction = exp(0.5)\nmaterial a bounce = sin(1) ^ 2\n",
    )
    .unwrap()
    .bind(&[])
    .unwrap();
    // e^0.5 = 1.6487212707001282 and sin(1)^2 = 0.7080734182735712, as recorded bits
    assert_eq!(f.materials()[0].friction.to_bits(), 0x3FFA_6129_8E1E_069C);
    assert_eq!(f.materials()[0].bounce.to_bits(), 0x3FE6_A889_95D4_DC81);
}

#[test]
fn the_semantics_id_is_the_fold_of_the_measured_pins() {
    let measured = world_semantics_fingerprints();
    assert_eq!(measured.len(), WORLD_SEMANTICS_PINS.len());
    for ((name, got), (want_name, want)) in measured.iter().zip(WORLD_SEMANTICS_PINS) {
        assert_eq!(name, want_name);
        assert_eq!(hex(got), hex(want), "{name}");
    }
    assert_eq!(
        hex(&fold_semantics_pins(WORLD_SEMANTICS_PINS)),
        hex(&WORLD_SEMANTICS_ID)
    );
    assert_eq!(
        hex(&measured_world_semantics_id()),
        hex(&WORLD_SEMANTICS_ID)
    );
}

/// The grip of the mochi law, against the flat circular punch on an elastic half-space
/// (Boussinesq: `k = 2 a E / (1 - nu^2)`, `a = sqrt(A / pi)`) for the mochi and the skin in series.
/// Expected values computed by hand from the law text: G = 427000 Pa, nu = 0.49, so
/// E = 2 G (1 + nu) = 1272460 Pa; A = 2e-4 m^2, a = 7.978845608e-3 m; skin E = 310000 Pa, nu = 0.48
#[test]
fn the_grip_is_two_flat_contacts_in_series() {
    let w = WorldLaw::from_file(MOCHI).unwrap().bind(&[]).unwrap();
    let g = w.grip("mochi").unwrap();
    let close = |got: f64, want: f64| (got - want).abs() <= want * 1e-12;
    assert!(close(g.contact_radius, 0.007_978_845_608_028_654), "{g:?}");
    assert!(close(g.material_stiffness, 26_721.310_389_241_058), "{g:?}");
    assert!(close(g.hand_stiffness, 6_427.864_185_262_168), "{g:?}");
    assert!(close(g.stiffness, 5_181.454_930_295_214), "{g:?}");
    // in series the pair is softer than either spring and the reciprocals add
    assert!(g.stiffness < g.hand_stiffness && g.hand_stiffness < g.material_stiffness);
    let reciprocal = 1.0 / g.material_stiffness + 1.0 / g.hand_stiffness;
    assert!(close(1.0 / g.stiffness, reciprocal), "{g:?}");
    assert_eq!(g.adhesion, Some(2.0));
    // over the skin's spread 40..310 kPa: the low end by hand, the high end is the point value
    let (lo, hi) = g.stiffness_spread.unwrap();
    assert!(close(lo, 804.433_060_376_326_9), "{g:?}");
    assert_eq!(hi, g.stiffness);
}

#[test]
fn a_spread_is_after_the_value_and_contains_it() {
    let base = "law h\nkind world\nmaterial a density = 1\nmaterial a friction = 0\nmaterial a bounce = 0\n";
    let hand =
        |lines: &str| format!("{base}hand_youngs_modulus = 100\n{lines}hand_poisson_ratio = 0.4\n");
    assert!(WorldLaw::from_file(&hand("hand_youngs_modulus spread 50 200\n")).is_ok());
    assert!(WorldLaw::from_file(&hand("hand_youngs_modulus spread 100 100\n")).is_ok());
    refused(
        &format!("{base}hand_youngs_modulus spread 1 2\n"),
        6,
        "not stated before its spread",
    );
    refused(
        &hand("hand_youngs_modulus spread 1 2\nhand_youngs_modulus spread 1 2\n"),
        8,
        "given twice",
    );
    refused(&hand("hand_youngs_modulus spread 2 1\n"), 7, "0 < lo <= hi");
    refused(&hand("hand_youngs_modulus spread 0 1\n"), 7, "0 < lo <= hi");
    refused(
        &hand("hand_youngs_modulus spread 1e2 2e2\n"),
        7,
        "plain decimals",
    );
    refused(
        &hand("hand_youngs_modulus spread 1\n"),
        7,
        "spread <lo> <hi>",
    );
    refused(&format!("{base}hand_youngs_modulus = 1\nhand_poisson_ratio = 0.4\nhand_poisson_ratio spread 1 2\n"), 8, "takes no spread");
    // the value must lie inside the spread
    refused(
        &hand("hand_youngs_modulus spread 200 300\n"),
        0,
        "inside its stated spread",
    );
    refused(
        &hand("hand_youngs_modulus spread 10 50\n"),
        0,
        "inside its stated spread",
    );
    // the spread is in the identifier
    let id = |lines: &str| WorldLaw::from_file(&hand(lines)).unwrap().law_id();
    assert_ne!(id(""), id("hand_youngs_modulus spread 50 200\n"));
    assert_ne!(
        id("hand_youngs_modulus spread 50 200\n"),
        id("hand_youngs_modulus spread 50 300\n")
    );
}

/// Four times the area is twice the radius, so every stiffness doubles exactly (k is linear in a,
/// and scaling by a power of two is exact in f64)
#[test]
fn four_times_the_area_doubles_every_stiffness() {
    let at = |area: &str| {
        let text = format!(
            "law g\nkind world\nmaterial a density = 1\nmaterial a friction = 0\nmaterial a bounce = 0\n\
             material a shear_modulus = 100000\nmaterial a poisson_ratio = 0.3\nmaterial a contact_area = {area}\n\
             hand_youngs_modulus = 200000\nhand_poisson_ratio = 0.45\n"
        );
        WorldLaw::from_file(&text)
            .unwrap()
            .bind(&[])
            .unwrap()
            .grip("a")
            .unwrap()
    };
    let (one, four) = (at("0.0001"), at("0.0004"));
    assert_eq!(four.contact_radius, 2.0 * one.contact_radius);
    assert_eq!(four.material_stiffness, 2.0 * one.material_stiffness);
    assert_eq!(four.hand_stiffness, 2.0 * one.hand_stiffness);
    assert_eq!(four.stiffness, 2.0 * one.stiffness);
    // and the material's spring is the closed form with E = 2 G (1 + nu) = 260000 Pa
    let a = (0.0001f64 / std::f64::consts::PI).sqrt();
    let want = 2.0 * a * 260_000.0 / (1.0 - 0.09);
    assert!(
        (one.material_stiffness - want).abs() <= want * 1e-12,
        "{one:?}"
    );
    assert_eq!(one.adhesion, None);
}

#[test]
fn a_grip_needs_the_material_constants_the_area_and_the_hand() {
    let base = "law g\nkind world\nmaterial a density = 1\nmaterial a friction = 0\nmaterial a bounce = 0\n";
    let full = "material a shear_modulus = 1000\nmaterial a poisson_ratio = 0.3\nmaterial a contact_area = 0.0001\n";
    let hand = "hand_youngs_modulus = 200000\nhand_poisson_ratio = 0.45\n";
    let grip = |text: &str| {
        WorldLaw::from_file(text)
            .unwrap()
            .bind(&[])
            .unwrap()
            .grip("a")
    };
    assert!(grip(&format!("{base}{full}{hand}")).is_some());
    assert!(grip(&format!("{base}{full}")).is_none(), "no hand");
    for drop in ["shear_modulus", "poisson_ratio", "contact_area"] {
        let mut kept = String::new();
        for l in full.lines().filter(|l| !l.contains(drop)) {
            kept.push_str(l);
            kept.push('\n');
        }
        assert!(grip(&format!("{base}{kept}{hand}")).is_none(), "{drop}");
    }
    let text = WorldLaw::from_file(&format!("{base}material a adhesion = 1\n"))
        .unwrap()
        .bind(&[])
        .unwrap()
        .describe();
    assert!(
        text.contains("  adhesion 1.0000e0 N (basis not stated)"),
        "{text}"
    );
    assert!(text.contains("  contact area: not stated"), "{text}");
    assert!(text.contains("  grip stiffness: not derived"), "{text}");
    assert!(text.contains("hand: not stated"), "{text}");
    // a material that states neither says nothing about a grip
    assert!(!WorldLaw::from_file(ONE)
        .unwrap()
        .bind(&[])
        .unwrap()
        .describe()
        .contains("grip stiffness"));
}

#[test]
fn the_hand_lines_are_refused_out_of_place_or_range() {
    let base = "law h\nkind world\nmaterial a density = 1\nmaterial a friction = 0\nmaterial a bounce = 0\n";
    refused(
        &format!("{base}hand_youngs_modulus = 1\n"),
        6,
        "without `hand_poisson_ratio`",
    );
    refused(
        &format!("{base}hand_poisson_ratio = 0.4\n"),
        6,
        "without `hand_youngs_modulus`",
    );
    refused(
        &format!("{base}hand_youngs_modulus basis assumed\n"),
        6,
        "not stated before its basis",
    );
    refused(
        &format!(
            "{base}hand_youngs_modulus = 1\nhand_youngs_modulus = 2\nhand_poisson_ratio = 0.4\n"
        ),
        7,
        "a second `hand_youngs_modulus`",
    );
    refused(
        &format!("{base}hand_youngs_modulus = 1\nhand_youngs_modulus basis guessed\nhand_poisson_ratio = 0.4\n"),
        7,
        "measured or assumed",
    );
    // the values are bound with the field ranges of G and nu: above 0, and in -1..0.5
    let bind = |e: &str, nu: &str| {
        WorldLaw::from_file(
            "law h\nkind world\nparam e number = 1 range -1 1000000\nparam nu number = 0.4 range -2 2\n\
             material a density = 1\nmaterial a friction = 0\nmaterial a bounce = 0\n\
             hand_youngs_modulus = e\nhand_poisson_ratio = nu\n",
        )
        .unwrap()
        .bind(&[("e", Number(e.parse().unwrap())), ("nu", Number(nu.parse().unwrap()))])
    };
    assert!(bind("1", "0.4").is_ok());
    assert!(matches!(
        bind("0", "0.4"),
        Err(BindError::OutOfRange { line: 8, .. })
    ));
    assert!(matches!(
        bind("1", "0.5"),
        Err(BindError::OutOfRange { line: 9, .. })
    ));
    // the contact area is above 0; the adhesion may be 0
    refused(
        &format!("{base}material a contact_area = 0\n"),
        0,
        "contact_area",
    );
    assert!(WorldLaw::from_file(&format!("{base}material a adhesion = 0\n")).is_ok());
    // adhesion and contact_area take a basis after their value
    let b = WorldLaw::from_file(&format!(
        "{base}material a adhesion = 1\nmaterial a adhesion basis measured\nmaterial a contact_area = 1\nmaterial a contact_area basis assumed\n"
    ))
    .unwrap()
    .bind(&[])
    .unwrap();
    let m = b.material("a").unwrap();
    assert_eq!(
        (m.adhesion_basis, m.contact_area_basis),
        (Some(Basis::Measured), Some(Basis::Assumed))
    );
}

/// Between two stated rows each modulus is interpolated on log-log axes: at the geometric mean of
/// two frequencies it is the geometric mean of the two moduli; a loss of 0 at an end interpolates
/// linearly; a stated frequency gives its row exactly; outside the range there is no answer
#[test]
fn a_dynamic_table_interpolates_on_log_log_axes_inside_its_range_only() {
    let w = WorldLaw::from_file(
        "law d\nkind world\nmaterial a density = 1\nmaterial a friction = 0\nmaterial a bounce = 0\n\
         material a dynamic youngs at 100 storage 10000 loss 0\nmaterial a dynamic youngs at 1 storage 100 loss 10\n",
    )
    .unwrap()
    .bind(&[])
    .unwrap();
    let d = w.material("a").unwrap().dynamic.clone().unwrap();
    assert_eq!(d.kind, ModulusKind::Youngs);
    assert_eq!(d.basis, None);
    // rows are sorted by frequency whatever the line order
    assert_eq!(
        d.rows.iter().map(|r| r.frequency).collect::<Vec<_>>(),
        [1.0, 100.0]
    );
    assert_eq!(d.at(1.0), Some((100.0, 10.0)));
    assert_eq!(d.at(100.0), Some((10_000.0, 0.0)));
    let (storage, loss) = d.at(10.0).unwrap();
    // sqrt(100 * 10000) = 1000; the loss is 10 + (0 - 10) / 2 = 5
    assert!((storage - 1000.0).abs() <= 1000.0 * 1e-12, "{storage}");
    assert!((loss - 5.0).abs() <= 5.0 * 1e-12, "{loss}");
    for f in [0.5, 0.999, 100.001, 1e6, 0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(d.at(f), None, "{f}");
    }
}

/// The scene reads `mochi`; it is Horiuchi's 30 C table scaled to Isono's level, stated as an
/// assumption, and the measured tables stay as measured, not spliced
#[test]
fn the_scene_mochi_is_the_assumed_combination_of_two_measured_states() {
    let w = WorldLaw::from_file(MOCHI).unwrap().bind(&[]).unwrap();
    let scene = w.material("mochi").unwrap().dynamic.clone().unwrap();
    let five_days = w.material("mochi_5d_30c").unwrap().dynamic.clone().unwrap();
    let isono = w.material("mochi_48h_30c").unwrap();
    assert_eq!(scene.basis, Some(Basis::Assumed));
    assert_eq!(five_days.basis, Some(Basis::Measured));
    // level = E(48 h) / E'(5 d, 3 Hz), the same factor on every row and on both moduli
    let e_48h = 2.0 * 427_000.0 * (1.0 + 0.49);
    // the 5 day 30 C storage modulus at 3 Hz is 10^7.035 Pa = 1.0839e7 Pa
    assert!((five_days.rows[0].storage / 1.0839e7 - 1.0).abs() < 1e-4);
    let level = e_48h / five_days.rows[0].storage;
    assert!((level - 0.1174).abs() < 5e-4, "{level}");
    for (s, f) in scene.rows.iter().zip(&five_days.rows) {
        assert_eq!(s.frequency, f.frequency);
        assert!((s.storage / f.storage - level).abs() <= level * 1e-12);
        assert!((s.loss / f.loss - level).abs() <= level * 1e-12);
    }
    // at 3 Hz the scene's storage modulus is Isono's Young's modulus
    assert!((scene.rows[0].storage - isono.youngs_modulus().unwrap()).abs() <= e_48h * 1e-12);
    // the logo's motion, 4 to 15 Hz, is inside the stated range; 0.1 Hz is not, for the scene
    assert!(scene.at(4.0).is_some() && scene.at(15.0).is_some());
    assert_eq!(scene.at(0.1), None);
    // the two measured 5 day temperatures are different tables
    let nineteen = w.material("mochi_5d_19c").unwrap().dynamic.clone().unwrap();
    assert!(nineteen.rows[0].storage > five_days.rows[0].storage);
    // only the scene's mochi is held by a hand
    assert!(w.grip("mochi").is_some());
    for m in ["mochi_48h_30c", "mochi_5d_19c", "mochi_5d_30c"] {
        assert!(w.grip(m).is_none(), "{m}");
    }
}

#[test]
fn a_dynamic_table_is_refused_out_of_form_or_range() {
    let base = "law d\nkind world\nmaterial a density = 1\nmaterial a friction = 0\nmaterial a bounce = 0\n";
    let row = |f: &str| format!("material a dynamic shear at {f} storage 100 loss 1\n");
    refused(&format!("{base}{}{}", row("1"), row("1")), 7, "given twice");
    refused(
        &format!(
            "{base}{}material a dynamic youngs at 2 storage 100 loss 1\n",
            row("1")
        ),
        7,
        "one kind per material",
    );
    refused(
        &format!("{base}material a dynamic basis measured\n"),
        6,
        "no dynamic row before its basis",
    );
    refused(
        &format!(
            "{base}{}material a dynamic basis measured\n{}",
            row("1"),
            row("2")
        ),
        8,
        "after its basis",
    );
    refused(
        &format!(
            "{base}{}material a dynamic basis measured\nmaterial a dynamic basis assumed\n",
            row("1")
        ),
        8,
        "given twice",
    );
    for f in ["0", "-1", "1e2", "x"] {
        refused(&format!("{base}{}", row(f)), 6, "plain decimal above 0");
    }
    refused(
        &format!("{base}material a dynamic shear at 1 storage 100\n"),
        6,
        "expected",
    );
    refused(
        &format!("{base}material a dynamic bulk at 1 storage 100 loss 1\n"),
        6,
        "expected",
    );
    refused(
        &format!("{base}material a dynamic shear at 1 storage 0 loss 1\n"),
        0,
        "storage",
    );
    refused(
        &format!("{base}material a dynamic shear at 1 storage 1 loss 0 - 1\n"),
        0,
        "loss",
    );
    // the table is in the identifier, a row's value and its frequency both
    let id = |text: &str| {
        WorldLaw::from_file(&format!("{base}{text}"))
            .unwrap()
            .law_id()
    };
    assert_ne!(id(&row("1")), id(&row("2")));
    assert_ne!(
        id(&row("1")),
        id("material a dynamic shear at 1 storage 101 loss 1\n")
    );
    assert_ne!(
        id(&row("1")),
        id("material a dynamic youngs at 1 storage 100 loss 1\n")
    );
}
