//! `alice_lol::variant`: knob tables of proportion laws, variant identifiers and the seeded sampler.
//!
//! oracles: (1) every knob at 0 steps binds the knob law exactly as its defaults do (the base bit for
//! bit), for `laws/avatar/body_knobs.law` and `laws/avatar/body.law`; (2) a knob vector given in any
//! order, or with the members of an unordered group swapped, is the same variant (same identifier,
//! same step counts); (3) the extreme step counts stay inside `[lo, hi]` after rounding, and the
//! binder accepts them; (4) the identifier moves with the knob law, the base, its role, each knob and
//! the part laws; (5) the sampler refuses a reference base and a gate that compared nothing, and
//! reports its trials, acceptances and per-gate rejections; (6) the samples are uniform over every
//! knob's whole range (χ², `compared` = the number of samples) and nothing near a given knob vector is
//! left out (a box around it receives its share); (7) the `group` grammar's refusals.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops, // the references are written unfused
    clippy::many_single_char_names, // statistics and step arithmetic
    clippy::manual_midpoint // a bisection's midpoint
)]

use std::path::{Path, PathBuf};

use alice_lol::proportion_law::{BoundProportion, Output, ProportionLaw};
use alice_lol::sheet_law::SheetLaw;
use alice_lol::variant::{sample, uniformity, Base, GateResult, KnobTable, KNOB_STEPS};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn law(rel: &str) -> ProportionLaw {
    ProportionLaw::from_file(&std::fs::read_to_string(root().join(rel)).unwrap()).unwrap()
}

const NEUTRAL: Base = Base::Neutral([7; 32]);

/// A bound law's outputs as bits (equal bits = the same instance, bit for bit)
fn bits(b: &BoundProportion) -> Vec<(String, Vec<u64>)> {
    b.outputs()
        .map(|(n, o)| {
            let v = match o {
                Output::Scalar(x) => vec![x.to_bits()],
                Output::Point(p) => p.iter().map(|x| x.to_bits()).collect(),
            };
            (n.to_string(), v)
        })
        .collect()
}

/// A small law with an unordered group of two tails, each a (height, azimuth) pair
const TAILS: &str = "law test_tails
kind proportion
param t0_height number = 0.5 range 0 1
param t0_azimuth number = 0 range -180 180
param t1_height number = 0.5 range 0 1
param t1_azimuth number = 0 range -180 180
param length number = 1 range 0.5 2
group tail: t0_height t0_azimuth
group tail: t1_height t1_azimuth
out reach = length
";

#[test]
fn every_knob_at_zero_is_the_base_bit_for_bit() {
    for rel in ["laws/avatar/body_knobs.law", "laws/avatar/body.law"] {
        let l = law(rel);
        let t = KnobTable::of(&l);
        let v = t.variant(NEUTRAL, &[], &[]).unwrap();
        assert!(v.q.iter().all(|q| *q == 0), "{rel}");
        for k in t.knobs() {
            assert_eq!(
                k.value(0).to_bits(),
                k.default.to_bits(),
                "{rel} {}",
                k.name
            );
        }
        assert_eq!(
            bits(&t.bind(&v).unwrap()),
            bits(&l.bind(&[]).unwrap()),
            "{rel}: the base's own instance"
        );
    }
    let l = law("laws/avatar/body_knobs.law");
    let b = KnobTable::of(&l)
        .bind(&KnobTable::of(&l).variant(NEUTRAL, &[], &[]).unwrap())
        .unwrap();
    assert_eq!(b.scalar("stature_m").unwrap().to_bits(), 1.6f64.to_bits());
}

#[test]
fn the_extreme_steps_stay_inside_the_range_and_bind() {
    // ranges whose step does not divide the distance to the default, and a default at an end
    let text = "law test_ends
kind proportion
param a number = 0.1 range 0.07 0.13
param b number = 0.3 range 0.1 0.3
param c number = -0.012 range -0.012 0.02
param d number = 1.6 range 0.5 2.5
param e number = 0.1 range -0.05 0.15
param f number = 0.1 range -0.012 0.244
out s = a + b + c + d + e + f
";
    let l = ProportionLaw::from_file(text).unwrap();
    let t = KnobTable::of(&l);
    for k in t.knobs() {
        assert!(
            k.value(k.q_min) >= k.lo && k.value(k.q_max) <= k.hi,
            "{}: {} {}",
            k.name,
            k.value(k.q_min),
            k.value(k.q_max)
        );
        // one more step leaves the range (the bounds are the widest that stay inside)
        assert!(k.value(k.q_min - 1) < k.lo || k.q_min == 0, "{}", k.name);
        assert!(k.value(k.q_max + 1) > k.hi || k.q_max == 0, "{}", k.name);
        assert!(
            ((k.q_max - k.q_min + 1) as f64 - (KNOB_STEPS + 1.0)).abs() <= 2.0,
            "{}: about 2^16 steps (a bound pulled in loses one step at each end)",
            k.name
        );
        let lo = t
            .variant(NEUTRAL, &[(k.name.as_str(), k.q_min)], &[])
            .unwrap();
        let hi = t
            .variant(NEUTRAL, &[(k.name.as_str(), k.q_max)], &[])
            .unwrap();
        assert!(t.bind(&lo).is_ok() && t.bind(&hi).is_ok(), "{}", k.name);
        assert!(
            t.variant(NEUTRAL, &[(k.name.as_str(), k.q_max + 1)], &[])
                .is_err(),
            "{}",
            k.name
        );
    }
    // the default at the top of its range: no step above it
    let b = &t.knobs()[1];
    assert_eq!(b.q_max, 0);
    // e and f are ranges where the plain ceil / floor of the distance lands an ulp outside after rounding
    let naive = |d: f64, lo: f64, hi: f64| {
        let step = (hi - lo) / KNOB_STEPS;
        (
            d + ((lo - d) / step).ceil() * step,
            d + ((hi - d) / step).floor() * step,
        )
    };
    assert!(naive(0.1, -0.05, 0.15).0 < -0.05, "e's bottom rounds out");
    assert!(naive(0.1, -0.012, 0.244).1 > 0.244, "f's top rounds out");
}

#[test]
fn the_order_of_the_knobs_and_of_a_groups_members_does_not_matter() {
    let l = law("laws/avatar/body_knobs.law");
    let t = KnobTable::of(&l);
    let a = t
        .variant(
            NEUTRAL,
            &[("stature", 100), ("chest_girth", -5), ("spine_posture", 9)],
            &[],
        )
        .unwrap();
    let b = t
        .variant(
            NEUTRAL,
            &[("spine_posture", 9), ("stature", 100), ("chest_girth", -5)],
            &[],
        )
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.id(), b.id());
    let l = ProportionLaw::from_file(TAILS).unwrap();
    let t = KnobTable::of(&l);
    let x = t
        .variant(
            NEUTRAL,
            &[
                ("t0_height", 300),
                ("t0_azimuth", -20),
                ("t1_height", -100),
                ("t1_azimuth", 50),
            ],
            &[],
        )
        .unwrap();
    let y = t
        .variant(
            NEUTRAL,
            &[
                ("t1_height", 300),
                ("t1_azimuth", -20),
                ("t0_height", -100),
                ("t0_azimuth", 50),
            ],
            &[],
        )
        .unwrap();
    assert_eq!(x.q, y.q, "the tails swapped are the same variant");
    assert_eq!(x.id(), y.id());
    assert_eq!(
        &x.q[..4],
        &[-100, 50, 300, -20],
        "members sorted by their step counts"
    );
    assert_eq!(bits(&t.bind(&x).unwrap()), bits(&t.bind(&y).unwrap()));
    // a tail's own pair is kept together, not sorted knob by knob
    let z = t
        .variant(
            NEUTRAL,
            &[
                ("t0_height", 300),
                ("t0_azimuth", 50),
                ("t1_height", -100),
                ("t1_azimuth", -20),
            ],
            &[],
        )
        .unwrap();
    assert_ne!(z.id(), x.id());
}

#[test]
fn a_variants_values_are_its_steps() {
    let l = law("laws/avatar/body_knobs.law");
    let t = KnobTable::of(&l);
    let k = t.knobs().iter().find(|k| k.name == "stature").unwrap();
    for q in [k.q_min, -1, 1, 1234, k.q_max] {
        let b = t
            .bind(&t.variant(NEUTRAL, &[("stature", q)], &[]).unwrap())
            .unwrap();
        let want = 1.6 + q as f64 * (2.5 - 0.5) / KNOB_STEPS;
        assert_eq!(
            b.scalar("stature_m").unwrap().to_bits(),
            want.to_bits(),
            "q = {q}"
        );
    }
}

#[test]
fn the_identifier_moves_with_each_of_its_inputs() {
    let l = law("laws/avatar/body_knobs.law");
    let t = KnobTable::of(&l);
    let v = t.variant(NEUTRAL, &[("stature", 10)], &[[1; 32]]).unwrap();
    let id = v.id();
    let other_law = law("laws/avatar/body.law");
    let ids = [
        t.variant(NEUTRAL, &[("stature", 11)], &[[1; 32]])
            .unwrap()
            .id(),
        t.variant(NEUTRAL, &[("stature", 10), ("shin_length", 1)], &[[1; 32]])
            .unwrap()
            .id(),
        t.variant(Base::Neutral([8; 32]), &[("stature", 10)], &[[1; 32]])
            .unwrap()
            .id(),
        t.variant(Base::Reference([7; 32]), &[("stature", 10)], &[[1; 32]])
            .unwrap()
            .id(),
        t.variant(NEUTRAL, &[("stature", 10)], &[[2; 32]])
            .unwrap()
            .id(),
        t.variant(NEUTRAL, &[("stature", 10)], &[]).unwrap().id(),
        KnobTable::of(&other_law)
            .variant(NEUTRAL, &[], &[[1; 32]])
            .unwrap()
            .id(),
    ];
    // the same knobs under another law (the same lines, another name): another identifier
    let a = ProportionLaw::from_file(TAILS).unwrap();
    let b =
        ProportionLaw::from_file(&TAILS.replacen("law test_tails", "law test_tails_b", 1)).unwrap();
    let va = KnobTable::of(&a).variant(NEUTRAL, &[], &[]).unwrap();
    let vb = KnobTable::of(&b).variant(NEUTRAL, &[], &[]).unwrap();
    assert_eq!(va.q, vb.q);
    assert_ne!(va.id(), vb.id(), "the knob law is part of the identifier");
    for (k, other) in ids.iter().enumerate() {
        assert_ne!(*other, id, "input {k}");
    }
    assert_eq!(
        t.variant(NEUTRAL, &[("stature", 10)], &[[1; 32]])
            .unwrap()
            .id(),
        id
    );
    assert!(t.variant(NEUTRAL, &[("nope", 1)], &[]).is_err());
    assert!(t
        .variant(NEUTRAL, &[("stature", 1), ("stature", 2)], &[])
        .is_err());
}

fn pass(v: &alice_lol::variant::Variant) -> Vec<GateResult> {
    vec![GateResult {
        gate: "always".into(),
        compared: v.q.len() as u64,
        failed: 0,
    }]
}

#[test]
fn a_reference_base_is_not_sampled_and_a_gate_must_compare_something() {
    let l = law("laws/avatar/body_knobs.law");
    let t = KnobTable::of(&l);
    let e = sample(&t, Base::Reference([7; 32]), 1, 10, 10, &[], &mut pass).unwrap_err();
    assert!(e.contains("only from a neutral base"), "{e}");
    let e = sample(&t, NEUTRAL, 1, 10, 10, &[], &mut |_| {
        vec![GateResult {
            gate: "empty".into(),
            compared: 0,
            failed: 0,
        }]
    })
    .unwrap_err();
    assert!(e.contains("`empty` compared nothing"), "{e}");
}

#[test]
fn the_sampler_is_deterministic_and_reports_its_rejections() {
    let l = law("laws/avatar/body_knobs.law");
    let t = KnobTable::of(&l);
    let i = t.knobs().iter().position(|k| k.name == "stature").unwrap();
    // a gate that rejects the lower half of the stature range
    let mut half = |v: &alice_lol::variant::Variant| {
        vec![
            GateResult {
                gate: "tall".into(),
                compared: 1,
                failed: u64::from(v.q[i] < 0),
            },
            GateResult {
                gate: "always".into(),
                compared: 3,
                failed: 0,
            },
        ]
    };
    let (a, ra) = sample(&t, NEUTRAL, 42, 500, 10_000, &[], &mut half).unwrap();
    let (b, rb) = sample(&t, NEUTRAL, 42, 500, 10_000, &[], &mut half).unwrap();
    assert_eq!(a, b);
    assert_eq!(ra, rb);
    assert_eq!(ra.accepted, 500);
    assert!(a.iter().all(|v| v.q[i] >= 0));
    let tall = ra.gates.iter().find(|g| g.0 == "tall").unwrap();
    assert_eq!(tall.1, ra.trials, "compared once per trial");
    assert_eq!(
        tall.2,
        ra.trials - ra.accepted,
        "every rejection is the tall gate's"
    );
    assert_eq!(
        ra.gates.iter().find(|g| g.0 == "always").unwrap().1,
        3 * ra.trials
    );
    // the default (0 steps) is below the range's middle, so about (0 − q_min) / span is rejected
    let k = &t.knobs()[i];
    let want = (-k.q_min) as f64 / (k.q_max - k.q_min + 1) as f64;
    let got = 1.0 - ra.acceptance();
    assert!(
        (got - want).abs() < 4.0 * (want * (1.0 - want) / ra.trials as f64).sqrt(),
        "{got} vs {want}"
    );
    let (c, _) = sample(&t, NEUTRAL, 43, 500, 10_000, &[], &mut half).unwrap();
    assert_ne!(a, c, "another seed, another sample");
}

/// The standard normal's upper tail Q(z) (Abramowitz-Stegun 7.1.26 for erfc, abs. error < 1.5e-7)
fn upper_tail(z: f64) -> f64 {
    let x = z / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    #[allow(
        clippy::disallowed_methods,
        reason = "a test's reference for a critical value, not a law"
    )]
    let e = (-x * x).exp();
    0.5 * poly * e
}

/// The χ² value exceeded with probability `alpha` at `dof` degrees of freedom (Wilson-Hilferty, z by
/// bisection on the normal tail)
fn chi_square_upper(dof: u32, alpha: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 10.0);
    for _ in 0..100 {
        let mid = 0.5 * (lo + hi);
        if upper_tail(mid) > alpha {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (k, z) = (f64::from(dof), 0.5 * (lo + hi));
    let c = 2.0 / (9.0 * k);
    {
        let w = 1.0 - c + z * c.sqrt();
        k * w * w * w
    }
}

#[test]
fn the_samples_cover_every_knobs_whole_range_evenly() {
    let l = law("laws/avatar/body_knobs.law");
    let t = KnobTable::of(&l);
    let (vs, r) = sample(&t, NEUTRAL, 2026, 20_000, 20_000, &[], &mut pass).unwrap();
    assert_eq!((r.trials, r.accepted), (20_000, 20_000));
    // every knob is tested: the 0.1 % level is shared among them (Bonferroni), so a uniform stream
    // fails this test with probability at most 0.1 % in all, not 0.1 % per knob
    let limit = chi_square_upper(15, 0.001 / t.knobs().len() as f64);
    assert!(
        (chi_square_upper(15, 0.001) - 37.70).abs() < 0.3,
        "the 0.1 % point of χ²(15) is about 37.70"
    );
    // the example's bound (46.7, from an exact normal tail) is this one, within this tail
    // approximation's error that far out
    assert!((limit - 46.70).abs() < 0.2, "the example's bound: {limit}");
    for k in t.knobs() {
        let u = uniformity(&t, &vs, &k.name, 16).unwrap();
        assert_eq!(u.compared, 20_000, "{}", k.name);
        assert!(
            u.chi_square < limit,
            "{}: χ² {} ≥ {limit} (dof {})",
            k.name,
            u.chi_square,
            u.dof
        );
    }
    assert!(uniformity(&t, &[], "stature", 16)
        .unwrap_err()
        .contains("compared 0"));
}

#[test]
fn nothing_near_a_given_knob_vector_is_left_out() {
    // a box of ±10 % of each range around a point (two knobs at once) receives its share
    let l = law("laws/avatar/body_knobs.law");
    let t = KnobTable::of(&l);
    let (vs, _) = sample(&t, NEUTRAL, 7, 20_000, 20_000, &[], &mut pass).unwrap();
    let names = ["chest_girth", "shoulder_width"];
    let at: Vec<usize> = names
        .iter()
        .map(|n| t.knobs().iter().position(|k| &k.name == n).unwrap())
        .collect();
    let centre = [1200i64, -3000];
    let mut p = 1.0;
    let mut inside = vec![true; vs.len()];
    for (j, &i) in at.iter().enumerate() {
        let k = &t.knobs()[i];
        let w = (k.q_max - k.q_min) / 10;
        let (lo, hi) = (centre[j] - w, centre[j] + w);
        p *= (hi - lo + 1) as f64 / (k.q_max - k.q_min + 1) as f64;
        for (v, ok) in vs.iter().zip(inside.iter_mut()) {
            *ok &= (lo..=hi).contains(&v.q[i]);
        }
    }
    let n = vs.len() as f64;
    let got = inside.iter().filter(|x| **x).count() as f64;
    let sd = (n * p * (1.0 - p)).sqrt();
    assert!(
        (got - n * p).abs() < 5.0 * sd,
        "{got} in the box, {} expected",
        n * p
    );
}

#[test]
fn the_stature_range_is_the_sheet_laws() {
    let sheet = SheetLaw::from_file(
        &std::fs::read_to_string(root().join("laws/avatar/sheet.law")).unwrap(),
    )
    .unwrap();
    let slot = sheet
        .slots
        .iter()
        .find(|s| s.page == "turnaround" && s.name == "stature")
        .unwrap();
    let k = law("laws/avatar/body_knobs.law");
    let stature = k.params().iter().find(|p| p.name == "stature").unwrap();
    assert_eq!(slot.range, Some((stature.lo, stature.hi)));
}

#[test]
fn the_group_grammar_refuses_what_it_cannot_canonicalise() {
    let with = |extra: &str| {
        ProportionLaw::from_file(&TAILS.replace(
            "out reach = length\n",
            &format!("{extra}\nout reach = length\n"),
        ))
    };
    for (extra, why) in [
        ("group other: t0_height", "is in two group members"),
        ("group tail: length", "differ in length"),
        ("group extra: nothing", "is not a declared parameter"),
        ("group extra:", "an empty member"),
        ("group 9bad: length", "is not a group name"),
        ("group extra: length length", "is in two group members"),
    ] {
        let e = with(extra).unwrap_err().to_string();
        assert!(e.contains(why), "{extra}: {e}");
    }
    let base = ProportionLaw::from_file(TAILS).unwrap();
    assert_eq!(base.groups().len(), 1);
    assert_eq!(base.groups()[0].members.len(), 2);
    // the groups are part of the law: the same law without them has another identifier
    let plain = ProportionLaw::from_file(&TAILS.replace(
        "group tail: t0_height t0_azimuth\ngroup tail: t1_height t1_azimuth\n",
        "",
    ))
    .unwrap();
    assert_ne!(plain.law_id(), base.law_id());
}
