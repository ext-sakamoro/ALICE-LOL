//! Motion laws (`kind motion`): reading, timing, sampling and the identifier
//!
//! The expected channel values below are worked out by hand from the law text (integer time,
//! the loop position from integers, one conversion to `f32`), not computed by the code under
//! test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]
// the expected values are worked out the way the law states them: in f64 without fused
// multiply-add, converted to f32 once
#![allow(clippy::cast_possible_truncation, clippy::suboptimal_flops)]

use alice_lol::law_id::{fold_semantics_pins, LOL_SEMANTICS_ID};
use alice_lol::motion_law::{
    measured_motion_semantics_id, motion_semantics_fingerprints, Channels, MotionLaw, SampleError,
    Timing, MOTION_PROBE_LAW, MOTION_SEMANTICS_ID, MOTION_SEMANTICS_PINS,
};

const STEP: &str = "law step
kind motion
param amp number = 2 range 0 10
period 1000
stagger 100
channel offset_y = amp * phase
channel offset_x = cycle
channel alpha = 2 * phase
";

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

#[test]
fn time_is_integer_and_the_loop_position_comes_from_integers() {
    let law = MotionLaw::from_file(STEP).unwrap();
    let b = law.bind(&[]).unwrap();
    assert_eq!(
        b.timing(),
        Timing {
            period_ms: 1000,
            stagger_ms: 100
        }
    );
    // glyph 0 at 250 ms: phase 0.25, cycle 0
    let c = b.sample(250, 0, 4).unwrap();
    assert_eq!((c.offset_y, c.offset_x, c.alpha), (0.5, 0.0, 0.5));
    // glyph 3 runs 300 ms behind: its time is -50, so cycle -1 and phase 0.95
    let c = b.sample(250, 3, 4).unwrap();
    assert_eq!(c.offset_x, -1.0);
    assert_eq!(c.offset_y, (2.0 * (950.0_f64 / 1000.0)) as f32);
    // alpha is clamped after the conversion (2 * 0.95 = 1.9)
    assert_eq!(c.alpha, 1.0);
    // the loop repeats exactly
    assert_eq!(
        b.sample(250, 1, 4),
        b.sample(250 + 7 * 1000, 1, 4).map(|mut c| {
            c.offset_x -= 7.0;
            c
        })
    );
    // channels not given keep their identity values
    let c = b.sample(0, 0, 1).unwrap();
    assert_eq!(
        (c.scale, c.rotate, c.dilate, c.luminance),
        (1.0, 0.0, 0.0, 1.0)
    );
    assert_eq!(Channels::IDENTITY.scale, 1.0);
}

#[test]
fn parameters_are_named_declared_and_in_range() {
    let law = MotionLaw::from_file(STEP).unwrap();
    assert_eq!(law.params().len(), 1);
    let c = law
        .bind(&[("amp", 4.0)])
        .unwrap()
        .sample(500, 0, 1)
        .unwrap();
    assert_eq!(c.offset_y, 2.0);
    assert_eq!(
        law.bind(&[("speed", 1.0)]).unwrap_err(),
        SampleError::UnknownParam("speed".into())
    );
    assert_eq!(
        law.bind(&[("amp", 1.0), ("amp", 2.0)]).unwrap_err(),
        SampleError::RepeatedParam("amp".into())
    );
    for v in [-0.5, 10.5, f64::NAN, f64::INFINITY] {
        assert_eq!(
            law.bind(&[("amp", v)]).unwrap_err(),
            SampleError::ParamOutOfRange { name: "amp".into() },
            "{v}"
        );
    }
    // the range ends are in it
    assert!(law.bind(&[("amp", 0.0)]).is_ok() && law.bind(&[("amp", 10.0)]).is_ok());
}

#[test]
fn the_period_rounds_to_whole_milliseconds_halves_up() {
    let law = |p: &str| {
        MotionLaw::from_file(&format!(
            "law p\nkind motion\nparam x number = 1 range 0 100000\nperiod {p}\n"
        ))
    };
    for (p, ms) in [
        ("999.5", 1000),
        ("999.49", 999),
        ("1000 / 3", 333),
        ("0.5", 1),
    ] {
        assert_eq!(
            law(p).unwrap().bind(&[]).unwrap().timing().period_ms,
            ms,
            "{p}"
        );
    }
    // below one millisecond, above an hour, or depending on a bound value out of bounds
    for p in ["0.49", "3600000.5", "0 * x"] {
        assert!(law(p).is_err(), "{p}");
    }
    let l = law("x").unwrap();
    assert_eq!(
        l.bind(&[("x", 0.2)]).unwrap_err(),
        SampleError::Timing { which: "period" }
    );
}

#[test]
fn samples_that_cannot_be_computed_say_why() {
    let law = MotionLaw::from_file(
        "law bad\nkind motion\nparam z number = 1 range 0 1\nperiod 10\nlet q = 1 / (z - phase * 0)\nchannel scale = z\nchannel dilate = q\n",
    )
    .unwrap();
    let b = law.bind(&[("z", 0.0)]).unwrap();
    assert_eq!(
        b.sample(0, 0, 1).unwrap_err(),
        SampleError::NonFinite { name: "q".into() }
    );
    let ok = law.bind(&[]).unwrap();
    assert_eq!(ok.sample(0, 1, 1).unwrap_err(), SampleError::GlyphIndex);
    let zero_scale =
        MotionLaw::from_file("law s\nkind motion\nperiod 10\nchannel scale = phase\n").unwrap();
    assert_eq!(
        zero_scale.bind(&[]).unwrap().sample(0, 0, 1).unwrap_err(),
        SampleError::ScaleNotPositive
    );
    let far = MotionLaw::from_file("law f\nkind motion\nperiod 10\nstagger 3600000\n").unwrap();
    let b = far.bind(&[]).unwrap();
    assert!(b.sample(u64::MAX, 0, 1).is_err());
    assert_eq!(
        b.sample(u64::MAX, 0, 1).unwrap_err(),
        SampleError::TimeOutOfRange
    );
}

#[test]
fn files_that_do_not_read() {
    let ok = "law m\nkind motion\nparam a number = 1 range 0 2\nperiod 100\nlet b = a * phase\nchannel scale = 1 + b\n";
    assert!(MotionLaw::from_file(ok).is_ok());
    let bad = [
        "kind motion\nperiod 1\n".to_owned(),
        "law m\nkind research\nperiod 1\n".to_owned(),
        "law m\nkind motion\n".to_owned(),
        ok.replace("period 100", "period 100 * phase"),
        format!("{ok}period 5\n"),
        ok.replace("range 0 2", "range 2 3"),
        ok.replace("param a", "param phase"),
        ok.replace("let b", "let a"),
        ok.replace("1 + b", "1 + c"),
        format!("{ok}channel glow = 1\n"),
        format!("{ok}channel scale = 2\n"),
        format!("{ok}let late = 1\n"),
        format!("{ok}colour red\n"),
        ok.replace("number = 1", "number = 1e0"),
        format!("{ok}claim a\u{7}b\n"),
        ok.replace("let b = a * phase", "let b = a *"),
    ];
    for (k, text) in bad.iter().enumerate() {
        assert!(
            MotionLaw::from_file(text).is_err(),
            "case {k} read:\n{text}"
        );
    }
    let e = MotionLaw::from_file(&format!("{ok}colour red\n")).unwrap_err();
    assert_eq!(e.line, 7);
    assert!(e.to_string().starts_with("motion law, line 7: "), "{e}");
}

#[test]
fn the_identifier_names_the_law_not_its_spelling() {
    let a = MotionLaw::from_file(STEP).unwrap().law_id();
    let spaced = STEP.replace("amp * phase", "amp*phase") + "# a comment\nclaim a note\n";
    assert_eq!(MotionLaw::from_file(&spaced).unwrap().law_id(), a);
    for changed in [
        STEP.replace("= 2 range", "= 3 range"),
        STEP.replace("stagger 100", "stagger 101"),
        STEP.replace("2 * phase", "3 * phase"),
        STEP.replace("law step", "law steps"),
        STEP.replace("channel alpha = 2 * phase\n", ""),
    ] {
        assert_ne!(
            MotionLaw::from_file(&changed).unwrap().law_id(),
            a,
            "{changed}"
        );
    }
    // pinned: a literal for a fixed declaration
    assert_eq!(
        hex(&a),
        "233b609d727f2f8bf886d385b9727d361af4083cb958655fc788a69562166001"
    );
}

#[test]
fn the_motion_semantics_are_their_own_and_measured() {
    assert_ne!(MOTION_SEMANTICS_ID, LOL_SEMANTICS_ID);
    let measured = motion_semantics_fingerprints();
    assert_eq!(measured.len(), MOTION_SEMANTICS_PINS.len());
    for ((name, pinned), (m_name, m)) in MOTION_SEMANTICS_PINS.iter().zip(&measured) {
        assert_eq!(name, m_name);
        assert_eq!(
            hex(m),
            hex(pinned),
            "pin `{name}` is not the measured behaviour"
        );
    }
    assert_eq!(
        fold_semantics_pins(MOTION_SEMANTICS_PINS),
        MOTION_SEMANTICS_ID
    );
    assert_eq!(measured_motion_semantics_id(), MOTION_SEMANTICS_ID);
    assert!(MotionLaw::from_file(MOTION_PROBE_LAW).is_ok());
}

/// The heartbeat law of `laws/motion`: it reads, its identifier is pinned, and its two swells
/// are where its claims put them (10% and 30% of the beat, the second 0.6 as strong)
#[test]
fn the_heartbeat_law_swells_twice_per_beat() {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../laws/motion/heartbeat.law"),
    )
    .unwrap();
    let law = MotionLaw::from_file(&text).unwrap();
    assert_eq!(
        hex(&law.law_id()),
        "f8fbe0185a62b9de65242c08d4216f040dcc695bf6c2b331f6e87e50e22a3612"
    );
    let b = law.bind(&[("bpm", 60.0), ("depth", 0.1)]).unwrap();
    assert_eq!(b.timing().period_ms, 1000);
    let scale = |t: u64| b.sample(t, 0, 1).unwrap().scale;
    // at the centres: 1 + 0.1 * 1 and 1 + 0.1 * 0.6
    assert_eq!(scale(100), (1.0_f64 + 0.1 * 1.0) as f32);
    assert_eq!(scale(300), (1.0_f64 + 0.1 * (0.6 * 1.0)) as f32);
    // at rest between beats, and every beat the same
    assert_eq!(scale(600), 1.0);
    assert_eq!(scale(100), scale(5100));
    // the second glyph is 40 ms behind
    assert_eq!(b.sample(140, 1, 2).unwrap().scale, scale(100));
}
