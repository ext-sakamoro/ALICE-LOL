//! Series quantities of audit laws (`x-series`): the flash count worked out by hand, the
//! identifier rule, and the flash audit law judging the frames of motion laws
//!
//! The series semantics enter the identifier of a law that declares a series and of no other,
//! so the identifiers of the existing audit laws do not move (`law_id_oracle` keeps them).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::path::Path;

use alice_lol::audit_law::Verdict;
use alice_lol::law_id::{fold_semantics_pins, LOL_SEMANTICS_ID};
use alice_lol::law_input::{
    audit_law_from_file, audit_series_fingerprints, max_flashes, measurements, Json, SeriesExpr,
    AUDIT_SERIES_SEMANTICS_ID, AUDIT_SERIES_SEMANTICS_PINS,
};
use alice_lol::motion_frame::{luminance_series, probe_word};
use alice_lol::motion_law::MotionLaw;

fn file(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel)).unwrap()
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

/// `n` samples alternating between `hi` and `lo` every `run` samples
fn square(run: usize, n: usize, hi: f64, lo: f64) -> Vec<f64> {
    (0..n)
        .map(|i| if (i / run).is_multiple_of(2) { hi } else { lo })
        .collect()
}

#[test]
fn flashes_are_pairs_of_transitions_in_the_busiest_window() {
    // a change every 5 samples: 11 transitions in any 60 samples (at indices 5, 10, ..., 55
    // for the window from 0, 12 for a window from 1), so 6 flashes
    assert_eq!(max_flashes(&square(5, 120, 1.0, 0.0), 60, 0.1, 0.8), 6);
    // a change every 10 samples: 6 transitions, 3 flashes
    assert_eq!(max_flashes(&square(10, 120, 1.0, 0.0), 60, 0.1, 0.8), 3);
    // the darker end at 0.85 is not below 0.8: no transition
    assert_eq!(max_flashes(&square(5, 120, 0.95, 0.85), 60, 0.1, 0.8), 0);
    // 0.3 - 0.2 is 0.09999999999999998 in binary64, below delta: no transition; 0.3 - 0.19
    // is above it
    assert_eq!(max_flashes(&square(5, 120, 0.3, 0.2), 60, 0.1, 0.8), 0);
    assert_eq!(max_flashes(&square(5, 120, 0.3, 0.19), 60, 0.1, 0.8), 6);
    // a run in one direction is one transition, however long; equal samples do not split it
    assert_eq!(
        max_flashes(&[0.0, 0.2, 0.4, 0.4, 0.6, 1.0, 0.0], 60, 0.1, 0.8),
        1
    );
    assert_eq!(max_flashes(&[0.0, 1.0, 0.0, 1.0, 0.0], 60, 0.1, 0.8), 2);
    // a ramp has one transition
    let ramp: Vec<f64> = (0..120).map(|i| f64::from(i) / 119.0).collect();
    assert_eq!(max_flashes(&ramp, 60, 0.1, 0.8), 0);
    // the window counts only the transitions inside it: transitions at samples 1, 4, 7 and 8
    let spread = [0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    assert_eq!(max_flashes(&spread, 9, 0.1, 0.8), 2);
    // samples 6..9 hold the two at 7 and 8: one flash
    assert_eq!(max_flashes(&spread, 3, 0.1, 0.8), 1);
    // without the last sample no 3 samples hold two transitions (1 and 4 are 3 apart)
    assert_eq!(max_flashes(&spread[..8], 3, 0.1, 0.8), 0);
    assert_eq!(max_flashes(&spread, 4, 0.1, 0.8), 1);
    // the boundaries are exact (binary fractions): a step of exactly delta is a transition,
    // and a darker end exactly at darker_below is not below it
    assert_eq!(
        max_flashes(&[0.25, 0.5, 0.25, 0.5, 0.25], 60, 0.25, 0.75),
        2
    );
    assert_eq!(max_flashes(&[0.5, 1.0, 0.5, 1.0, 0.5], 60, 0.25, 0.5), 0);
    assert_eq!(max_flashes(&[0.25, 1.0, 0.25, 1.0, 0.25], 60, 0.25, 0.5), 2);
    // nothing to count
    assert_eq!(max_flashes(&[], 60, 0.1, 0.8), 0);
    assert_eq!(max_flashes(&[0.5], 2, 0.1, 0.8), 0);
}

#[test]
fn a_reversal_smaller_than_delta_does_not_split_a_change() {
    // WCAG 2.3.1: a flash is a pair of opposing changes of at least 10%. 0 -> 0.5 -> 0.45 ->
    // 0.95 is one rise (the 0.05 dip is below delta), not two, so no flash
    assert_eq!(max_flashes(&[0.0, 0.5, 0.45, 0.95], 60, 0.1, 0.8), 0);
    // rise, small dip, rise, fall, twice: one rise and one fall per cycle, 4 changes, 2 flashes
    let cycles = [0.0, 0.5, 0.45, 0.95, 0.0, 0.5, 0.45, 0.95, 0.0];
    assert_eq!(max_flashes(&cycles, 60, 0.1, 0.8), 2);
    // the same inside a window: samples 4..9 hold the second cycle's rise and fall
    assert_eq!(max_flashes(&cycles, 5, 0.1, 0.8), 1);
    // a dip of exactly delta does turn the direction (binary fractions): rise, fall, rise
    assert_eq!(max_flashes(&[0.0, 0.5, 0.25, 0.75, 0.0], 60, 0.25, 0.8), 2);
    // a fall that does not reach below darker_below is left out, and the rise after it with it:
    // up 0 -> 0.9, down 0.9 -> 0.85 (both ends at or above 0.8), up 0.85 -> 0.95, down 0.95 -> 0
    // keeps the first rise and the last fall, one flash
    assert_eq!(max_flashes(&[0.0, 0.9, 0.85, 0.95, 0.0], 60, 0.05, 0.8), 1);
}

#[test]
fn a_series_expression_reads_and_says_its_value_above_the_limit() {
    let e = SeriesExpr::parse("flashes_above(l, 60, 0.1, 0.8, 3)").unwrap();
    assert_eq!(e.canonical(), "flashes_above(l,60,0.1,0.8,3)");
    let list = |v: Vec<f64>| Json::Array(v.into_iter().map(Json::Number).collect());
    assert_eq!(e.evaluate(Some(&list(square(5, 120, 1.0, 0.0)))), Some(3.0));
    assert_eq!(
        e.evaluate(Some(&list(square(10, 120, 1.0, 0.0)))),
        Some(0.0)
    );
    // not measured: missing, not a list, a sample that is not a finite number
    assert_eq!(e.evaluate(None), None);
    assert_eq!(e.evaluate(Some(&Json::Number(1.0))), None);
    assert_eq!(e.evaluate(Some(&list(vec![0.0, f64::NAN]))), None);
    for bad in [
        "flashes_above(l, 1, 0.1, 0.8, 3)",
        "flashes_above(l, 60, 0, 0.8, 3)",
        "flashes_above(l, 60, 0.1, 1.5, 3)",
        "flashes_above(l, 60, 0.1, 0.8, -1)",
        "flashes_above(l, 60, 0.1, 0.8)",
        "flashes_above(1l, 60, 0.1, 0.8, 3)",
        "flashes(l, 60, 0.1, 0.8, 3)",
        "flashes_above(l, 060, 0.1, 0.8, 3)",
    ] {
        assert!(SeriesExpr::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn only_a_law_with_a_series_carries_the_series_semantics() {
    let with = file("laws/audit/flash_three_per_second.law");
    let law = audit_law_from_file(&with).unwrap();
    assert_eq!(law.series().len(), 1);
    // the same law without its series line is another law, and the series line is what
    // brings the series semantics into the identifier (the law without it hashes exactly as
    // a law of the earlier reader would)
    let without = with.replace(
        "x-series excess = flashes_above(luminance, 60, 0.1, 0.8, 3)\n",
        "",
    );
    let plain = audit_law_from_file(&without).unwrap();
    assert!(plain.series().is_empty());
    assert_ne!(
        law.law_id(&LOL_SEMANTICS_ID),
        plain.law_id(&LOL_SEMANTICS_ID)
    );
    // spelling of the expression does not count; its numbers do
    let spaced = with.replace(
        "flashes_above(luminance, 60, 0.1, 0.8, 3)",
        "flashes_above( luminance,60 , 0.1,0.8,3 )",
    );
    assert_eq!(
        audit_law_from_file(&spaced)
            .unwrap()
            .law_id(&LOL_SEMANTICS_ID),
        law.law_id(&LOL_SEMANTICS_ID)
    );
    let stricter = with.replace("0.8, 3)", "0.8, 2)");
    assert_ne!(
        audit_law_from_file(&stricter)
            .unwrap()
            .law_id(&LOL_SEMANTICS_ID),
        law.law_id(&LOL_SEMANTICS_ID)
    );
    // pinned: a literal for the flash law
    assert_eq!(
        hex(&law.law_id(&LOL_SEMANTICS_ID)),
        "1f944811bd7ccfedbe66b63986c224d215351098f8ca5681196d8fea6eea9d4f"
    );
    // a name used twice, a floor on nothing, a bad expression
    for bad in [
        with.replace("x-series excess", "x-series samples"),
        format!("{with}x-at-least nothing 1\n"),
        with.replace("flashes_above(luminance, 60", "flashes_above(luminance, 1"),
        with.replace("x-series excess", "x-series  excess"),
    ] {
        assert!(audit_law_from_file(&bad).is_err(), "{bad}");
    }
}

/// A law that declares a series shows it in its printed form, and a law without one prints
/// as before the series existed (no `series` field), which the frozen-corpus digest of
/// `parse_without_semantics_wrapper` holds
#[test]
fn the_printed_form_shows_a_series_only_when_declared() {
    let with = file("laws/audit/flash_three_per_second.law");
    let law = audit_law_from_file(&with).unwrap();
    let shown = format!("{law:?}");
    assert!(shown.contains("series: ["), "{shown}");
    assert!(
        shown.contains("flashes") || shown.contains("SeriesExpr"),
        "{shown}"
    );
    assert!(shown.contains("window: 60"), "{shown}");
    let plain = audit_law_from_file(&with.replace(
        "x-series excess = flashes_above(luminance, 60, 0.1, 0.8, 3)\n",
        "",
    ))
    .unwrap();
    let shown = format!("{plain:?}");
    assert!(!shown.contains("series"), "{shown}");
    assert!(shown.starts_with("AuditLaw { name: "), "{shown}");
}

fn judge(series: &[f64]) -> Verdict {
    let law = audit_law_from_file(&file("laws/audit/flash_three_per_second.law")).unwrap();
    let inputs = Json::Object(vec![(
        "luminance".to_owned(),
        Json::Array(series.iter().copied().map(Json::Number).collect()),
    )]);
    law.evaluate(&measurements(&law, &inputs))
}

/// The flash audit law on the frames of motion laws: both sides that hold the same laws and
/// values reach the same verdict
#[test]
fn the_flash_law_passes_a_heartbeat_and_breaks_a_strobe() {
    let word = probe_word();
    let heartbeat = MotionLaw::from_file(&file("laws/motion/heartbeat.law")).unwrap();
    let b = heartbeat.bind(&[("bpm", 180.0), ("depth", 0.2)]).unwrap();
    let series = luminance_series(&word, &b, 0, 120, 60).unwrap();
    assert_eq!(series.len(), 120);
    assert_eq!(judge(&series), Verdict::Supports);

    // the whole word turns on and off 8 times a second (a change every 62.5 ms)
    let strobe = MotionLaw::from_file(
        "law strobe\nkind motion\nperiod 125\nchannel alpha = 2 * (1 - phase) - 1 + 1000 * (0.5 - phase)\n",
    )
    .unwrap();
    let s = luminance_series(&word, &strobe.bind(&[]).unwrap(), 0, 120, 60).unwrap();
    assert!(
        matches!(judge(&s), Verdict::Breaks { metric, .. } if metric == "excess"),
        "{s:?}"
    );

    // nothing measured is not a pass
    assert!(matches!(judge(&[]), Verdict::NoEvidence { .. }));
}

#[test]
fn the_series_semantics_are_their_own_and_measured() {
    let measured = audit_series_fingerprints();
    assert_eq!(measured.len(), AUDIT_SERIES_SEMANTICS_PINS.len());
    for ((name, pinned), (m_name, m)) in AUDIT_SERIES_SEMANTICS_PINS.iter().zip(&measured) {
        assert_eq!(name, m_name);
        assert_eq!(
            hex(m),
            hex(pinned),
            "pin `{name}` is not the measured behaviour"
        );
    }
    assert_eq!(
        fold_semantics_pins(AUDIT_SERIES_SEMANTICS_PINS),
        AUDIT_SERIES_SEMANTICS_ID
    );
    assert_ne!(AUDIT_SERIES_SEMANTICS_ID, LOL_SEMANTICS_ID);
}
