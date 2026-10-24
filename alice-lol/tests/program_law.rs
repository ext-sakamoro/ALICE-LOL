//! The law of a LOL program's shape as a law reference v2 (`alice_lol::program_law`)
//!
//! Oracle: the expected bytes and law identifiers were written by ALICE-Zip's independent
//! implementation (`scripts/law_ref_v2_reference.py` at fec22a0, standard-library Python) for kind
//! `lol.law.program/v1`, the geometry semantics identifier of alice-det-math 0.5.1 and one `source`
//! row; they are not computed by the code under test.

use alice_lol::geometry_id::LOL_GEOMETRY_SEMANTICS_ID;
use alice_lol::program_law::{
    program_law_ref, resolve_program, ProgramLawError, PROGRAM_LAW_KIND, SOURCE_ROW,
};
use alice_lol::runtime_parser::parse_lol;
use alice_zip::law_ref::{LawRef, Value};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

const SPHERE: &str = "sphere(1.0)";
const SPHERE_LAW_ID: &str = "ae8b46297dc00049065279ca16cfe2be05e0c44b63e22b652dcb8343bbe5c0aa";
const SPHERE_BYTES: &str = "4c4f4c520002126c6f6c2e6c61772e70726f6772616d2f7631d1cbba69a1b33ce7363ac178f16ad0cb062699ff8c9c76af7058c6d006bc8a24ae8b46297dc00049065279ca16cfe2be05e0c44b63e22b652dcb8343bbe5c0aa0106736f7572636505000000000000000b73706865726528312e3029";
const BOX: &str = "box3d(1.0, 1.0, 1.0)";
const BOX_LAW_ID: &str = "189840c8b20de5cb11fb0b77b6181b222ffe7960e81e1ab9423d5f625c2d29c9";

#[test]
fn the_kind_and_the_semantics_are_the_program_law_s() {
    assert_eq!(PROGRAM_LAW_KIND, b"lol.law.program/v1");
    assert_eq!(SOURCE_ROW, "source");
    let r = program_law_ref(SPHERE).unwrap();
    assert_eq!(r.kind(), b"lol.law.program/v1");
    assert_eq!(r.semantics_id(), &LOL_GEOMETRY_SEMANTICS_ID);
    assert_eq!(
        r.rows(),
        &[("source".to_string(), Value::Str(SPHERE.to_string()))]
    );
}

#[test]
fn the_bytes_and_the_law_id_equal_the_independent_reference() {
    let r = program_law_ref(SPHERE).unwrap();
    assert_eq!(r.law_id().to_vec(), unhex(SPHERE_LAW_ID));
    assert_eq!(r.to_bytes(), unhex(SPHERE_BYTES));
    assert_eq!(
        program_law_ref(BOX).unwrap().law_id().to_vec(),
        unhex(BOX_LAW_ID)
    );
}

#[test]
fn a_reference_from_its_bytes_resolves_to_the_program_s_shape() {
    for src in [SPHERE, BOX] {
        let bytes = program_law_ref(src).unwrap().to_bytes();
        let back = LawRef::from_bytes(&bytes).unwrap();
        let shape = resolve_program(&back).unwrap();
        assert_eq!(
            format!("{shape:?}"),
            format!("{:?}", parse_lol(src).unwrap()),
            "{src}"
        );
    }
}

/// The geometry semantics of alice-det-math 0.5.0 (alice-lol up to 0.4.1), which reproduces the same
/// bits as the current one: a reference written under it resolves to the same shape
const PREVIOUS_GEOMETRY_SEMANTICS: &str =
    "251e73bf84e00c4c1916184bdcbbb75e5cf3f74b62fd4adcbc3922dd18df907d";

#[test]
fn a_reference_under_an_earlier_semantics_with_the_same_bits_resolves() {
    let theirs: [u8; 32] = unhex(PREVIOUS_GEOMETRY_SEMANTICS).try_into().unwrap();
    assert_ne!(
        theirs, LOL_GEOMETRY_SEMANTICS_ID,
        "an earlier generation, not this one"
    );
    for src in [SPHERE, BOX] {
        let r = LawRef::new(
            PROGRAM_LAW_KIND,
            theirs,
            vec![(SOURCE_ROW.into(), Value::Str(src.into()))],
        )
        .unwrap();
        let back = LawRef::from_bytes(&r.to_bytes()).unwrap();
        let shape = resolve_program(&back).unwrap_or_else(|e| panic!("{src}: {e}"));
        assert_eq!(
            format!("{shape:?}"),
            format!("{:?}", parse_lol(src).unwrap()),
            "{src}"
        );
    }
}

#[test]
fn another_kind_is_refused() {
    let r = LawRef::new(
        b"lol.law.motion/v1",
        LOL_GEOMETRY_SEMANTICS_ID,
        vec![("source".into(), Value::Str(SPHERE.into()))],
    )
    .unwrap();
    assert!(
        matches!(resolve_program(&r), Err(ProgramLawError::OtherKind(k)) if k == b"lol.law.motion/v1")
    );
}

#[test]
fn another_semantics_is_refused_naming_both() {
    let theirs = [7u8; 32];
    let r = LawRef::new(
        PROGRAM_LAW_KIND,
        theirs,
        vec![("source".into(), Value::Str(SPHERE.into()))],
    )
    .unwrap();
    match resolve_program(&r) {
        Err(ProgramLawError::SemanticsMismatch { theirs: t, ours }) => {
            assert_eq!(t, theirs);
            assert_eq!(ours, LOL_GEOMETRY_SEMANTICS_ID);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn rows_other_than_one_source_string_are_refused() {
    let cases: Vec<Vec<(String, Value)>> = vec![
        vec![],
        vec![("source".into(), Value::Bytes(SPHERE.as_bytes().to_vec()))],
        vec![
            ("source".into(), Value::Str(SPHERE.into())),
            ("quality".into(), Value::Str("draft".into())),
        ],
        vec![("src".into(), Value::Str(SPHERE.into()))],
    ];
    for rows in cases {
        let r = LawRef::new(PROGRAM_LAW_KIND, LOL_GEOMETRY_SEMANTICS_ID, rows.clone()).unwrap();
        assert!(
            matches!(resolve_program(&r), Err(ProgramLawError::Rows(_))),
            "{rows:?}"
        );
    }
}

#[test]
fn a_program_that_does_not_parse_is_a_parse_error() {
    let r = program_law_ref("not a program (").unwrap();
    assert!(matches!(
        resolve_program(&r),
        Err(ProgramLawError::Parse(_))
    ));
}

#[test]
fn errors_say_what_was_refused() {
    let r = LawRef::new(b"x/v1", LOL_GEOMETRY_SEMANTICS_ID, vec![]).unwrap();
    let msg = resolve_program(&r).unwrap_err().to_string();
    assert!(
        msg.contains("x/v1") && msg.contains("lol.law.program/v1"),
        "{msg}"
    );
}
