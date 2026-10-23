//! Law references: the bytes against their layout law, and resolving against a law text
//!
//! The layout of the bytes is declared once in `laws/layout/law_ref.layout`; every encoding
//! here, and every one-byte change and truncation of it, is read both by the layout law and by
//! `LawRef::from_bytes`, and the two must agree.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::path::Path;

use alice_lol::law_ref::{LawRef, LawRefError, HEADER_LEN, MAGIC, ROW_LEN};
use alice_lol::layout::{decode, parse, Budget, DecodeError, FieldValue, Layout};
use alice_lol::motion_law::{MotionLaw, SampleError, MOTION_SEMANTICS_ID};

fn file(rel: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel)).unwrap()
}

fn heartbeat() -> MotionLaw {
    MotionLaw::from_file(&file("laws/motion/heartbeat.law")).unwrap()
}

fn layout() -> Layout {
    parse(&file("laws/layout/law_ref.layout")).unwrap()
}

#[test]
fn a_reference_is_the_declared_bytes() {
    let law = heartbeat();
    let r = LawRef::motion(&law, &[("bpm", 60.0)]).unwrap();
    // every parameter, in declared order, the one not given at its default
    assert_eq!(
        r.params,
        vec![("bpm".to_owned(), 60.0), ("depth".to_owned(), 0.06)]
    );
    let b = r.to_bytes().unwrap();
    assert_eq!(b.len(), HEADER_LEN + 2 * ROW_LEN);
    // written out by hand: LOLR, version 1, two rows, the two identifiers
    assert_eq!(&b[..8], b"LOLR\x01\x00\x02\x00");
    assert_eq!(&b[8..40], &MOTION_SEMANTICS_ID);
    assert_eq!(&b[40..72], &law.law_id());
    let mut name = [0u8; 32];
    name[..3].copy_from_slice(b"bpm");
    assert_eq!(&b[72..104], &name);
    // 60.0 is 0x404e000000000000
    assert_eq!(&b[104..112], &[0, 0, 0, 0, 0, 0, 0x4e, 0x40]);
    assert_eq!(LawRef::from_bytes(&b).unwrap(), r);
    assert_eq!(MAGIC, *b"LOLR");

    // the layout law reads the same fields
    let lay = layout();
    let d = decode(&lay, &b, Budget::for_input(b.len())).unwrap();
    assert_eq!(d.value("header", "count"), Some(FieldValue::Int(2)));
    assert_eq!(
        d.value("header", "law_id"),
        Some(FieldValue::Bytes(&law.law_id()))
    );
    assert_eq!(
        d.row_value("params", 0, "value"),
        Some(FieldValue::Int(60.0_f64.to_bits()))
    );
}

/// What the layout law says about `b`: read, or the label of the refusal
fn by_layout(lay: &Layout, b: &[u8]) -> Result<(), String> {
    decode(lay, b, Budget::for_input(b.len()))
        .map(|_| ())
        .map_err(|e| match e {
            DecodeError::Refused { label, .. } => label,
            e @ DecodeError::BudgetExceeded { .. } => e.to_string(),
        })
}

/// What `from_bytes` says, in the same form (a name check is past the layout, so a refusal
/// for `name` is compared as a read)
fn by_reader(b: &[u8]) -> Result<(), String> {
    match LawRef::from_bytes(b) {
        Ok(_) | Err(LawRefError::Format("name")) => Ok(()),
        Err(LawRefError::Format(label)) => Err(label.to_owned()),
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn every_change_and_truncation_is_judged_as_the_layout_law_judges_it() {
    let lay = layout();
    let law = heartbeat();
    let mut cases = 0;
    let mut labels = std::collections::BTreeSet::new();
    let none = MotionLaw::from_file("law still\nkind motion\nperiod 1000\n").unwrap();
    for r in [
        LawRef::motion(&law, &[]).unwrap(),
        LawRef::motion(&none, &[]).unwrap(),
    ] {
        let b = r.to_bytes().unwrap();
        for i in 0..b.len() {
            for bit in [0x01u8, 0x80] {
                let mut m = b.clone();
                m[i] ^= bit;
                assert_eq!(by_reader(&m), by_layout(&lay, &m), "byte {i} ^ {bit:#x}");
                if let Err(l) = by_layout(&lay, &m) {
                    labels.insert(l);
                }
                cases += 1;
            }
        }
        for n in 0..=b.len() + 1 {
            let mut m = b.clone();
            m.resize(n, 0);
            assert_eq!(by_reader(&m), by_layout(&lay, &m), "length {n}");
            if let Err(l) = by_layout(&lay, &m) {
                labels.insert(l);
            }
            cases += 1;
        }
    }
    // a count past the limit
    let mut m = LawRef::motion(&none, &[]).unwrap().to_bytes().unwrap();
    m[6] = 65;
    assert_eq!(by_reader(&m), Err("count".to_owned()));
    assert_eq!(by_layout(&lay, &m), Err("count".to_owned()));
    for l in ["too_short", "bad_magic", "version", "count", "layout"] {
        assert!(
            labels.contains(l) || l == "count",
            "no case refused as {l}: {labels:?}"
        );
    }
    assert!(cases > 400, "{cases}");
}

#[test]
fn names_that_are_not_names_are_refused() {
    let law = heartbeat();
    let b = LawRef::motion(&law, &[]).unwrap().to_bytes().unwrap();
    let mut empty = b.clone();
    empty[72..75].fill(0);
    assert_eq!(
        LawRef::from_bytes(&empty).unwrap_err(),
        LawRefError::Format("name")
    );
    let mut gap = b.clone();
    gap[80] = b'x';
    assert_eq!(
        LawRef::from_bytes(&gap).unwrap_err(),
        LawRefError::Format("name")
    );
    let mut utf8 = b;
    utf8[72] = 0xff;
    assert_eq!(
        LawRef::from_bytes(&utf8).unwrap_err(),
        LawRefError::Format("name")
    );
}

#[test]
fn a_reference_resolves_to_its_law_with_its_values() {
    let law = heartbeat();
    let r = LawRef::motion(&law, &[("bpm", 120.0), ("depth", 0.2)]).unwrap();
    let back = LawRef::from_bytes(&r.to_bytes().unwrap()).unwrap();
    let bound = back.resolve_motion(&law).unwrap();
    assert_eq!(bound.values(), &[120.0, 0.2]);
    assert_eq!(bound.timing().period_ms, 500);
    // the same instant gives the same channels as binding directly
    let direct = law.bind(&[("bpm", 120.0), ("depth", 0.2)]).unwrap();
    assert_eq!(bound.sample(50, 0, 1), direct.sample(50, 0, 1));
    assert_eq!(
        LawRef::motion(&law, &[("bpm", 300.0)]).unwrap_err(),
        LawRefError::Param(SampleError::ParamOutOfRange { name: "bpm".into() })
    );
}

/// A reference written by another build (other motion semantics) or for another law is
/// refused, and the error names both identifiers
#[test]
fn a_reference_from_another_build_or_law_is_refused_naming_both() {
    let law = heartbeat();
    let r = LawRef::motion(&law, &[]).unwrap();

    let mut other_build = r.clone();
    other_build.semantics_id[31] ^= 1;
    let e = LawRef::from_bytes(&other_build.to_bytes().unwrap())
        .unwrap()
        .resolve_motion(&law)
        .unwrap_err();
    assert_eq!(
        e,
        LawRefError::SemanticsMismatch {
            ours: MOTION_SEMANTICS_ID,
            theirs: other_build.semantics_id
        }
    );
    let shown = e.to_string();
    assert!(
        shown.contains(&hex(&MOTION_SEMANTICS_ID))
            && shown.contains(&hex(&other_build.semantics_id))
    );

    let other_law = MotionLaw::from_file(
        &file("laws/motion/heartbeat.law").replace("stagger 40", "stagger 41"),
    )
    .unwrap();
    let e = r.resolve_motion(&other_law).unwrap_err();
    assert_eq!(
        e,
        LawRefError::LawMismatch {
            ours: other_law.law_id(),
            theirs: law.law_id()
        }
    );
    let shown = e.to_string();
    assert!(shown.contains(&hex(&law.law_id())) && shown.contains(&hex(&other_law.law_id())));

    // the parameters must be the law's, in its order
    let mut swapped = r.clone();
    swapped.params.reverse();
    assert!(matches!(
        swapped.resolve_motion(&law).unwrap_err(),
        LawRefError::ParamNames { .. }
    ));
    let mut missing = r.clone();
    missing.params.pop();
    assert!(matches!(
        missing.resolve_motion(&law).unwrap_err(),
        LawRefError::ParamNames { .. }
    ));
    let mut out_of_range = r;
    out_of_range.params[0].1 = 1000.0;
    assert_eq!(
        out_of_range.resolve_motion(&law).unwrap_err(),
        LawRefError::Param(SampleError::ParamOutOfRange { name: "bpm".into() })
    );
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}
