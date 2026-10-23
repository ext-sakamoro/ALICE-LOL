//! The `semantics(<id>, ...)` wrapper (ALICE-Law G7): accepted with this release's identifier or
//! an earlier release's that reproduces the same bits, refused with another as a distinct error
//! naming both, required by the `*_stamped` entry points
//!
//! Each `*_stamped` entry point is checked on a text without the wrapper: if one of them accepted
//! it, an exchange that leaves the identifier out would be read unchecked

use alice_lol::geometry_id::LOL_GEOMETRY_SEMANTICS_ID;
use alice_lol::law_id::LOL_SEMANTICS_ID;
use alice_lol::runtime_parser::{
    parse_declared, parse_declared_stamped, parse_law, parse_law_stamped, parse_lol,
    parse_lol_stamped, parse_program, parse_program_stamped, ParseError, ParseErrorKind,
};

fn hex(id: &[u8; 32]) -> String {
    id.iter().fold(String::new(), |mut acc, b| {
        use std::fmt::Write as _;
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

/// another identifier: this release's with the last byte changed
const fn other(id: &[u8; 32]) -> [u8; 32] {
    let mut o = *id;
    o[31] ^= 0x5a;
    o
}

const BODY: &str = "smooth_union(0.3, sphere(1.0), box3d(0.5, 0.5, 0.5))";
const PROGRAM: &str = "program(sphere(1.0), entities(box3d(1.0, 1.0, 1.0)), grasp(0, right, 0.5))";
const DECLARED: &str = "units(mm, print, cylinder(50.0, 5.0))";
const LAW: &str = "audit g\nevidence compared\nexpect compared == 1\n";

type Entry = fn(&str) -> Result<String, ParseError>;
type Wrap = fn(&str, &str) -> String;
/// (name, plain text, wrap with id, plain entry, stamped entry, the identifier it reads)
type Row = (&'static str, &'static str, Wrap, Entry, Entry, [u8; 32]);

fn entries() -> Vec<Row> {
    fn wrap(hex: &str, body: &str) -> String {
        format!("semantics({hex}, {body})")
    }
    fn wrap_law(hex: &str, body: &str) -> String {
        format!("semantics {hex}\n{body}")
    }
    vec![
        (
            "lol",
            BODY,
            wrap,
            |t| parse_lol(t).map(|n| format!("{n:?}")),
            |t| parse_lol_stamped(t).map(|n| format!("{n:?}")),
            LOL_GEOMETRY_SEMANTICS_ID,
        ),
        (
            "program",
            PROGRAM,
            wrap,
            |t| parse_program(t).map(|p| format!("{p:?}")),
            |t| parse_program_stamped(t).map(|p| format!("{p:?}")),
            LOL_GEOMETRY_SEMANTICS_ID,
        ),
        (
            "declared",
            DECLARED,
            wrap,
            |t| parse_declared(t).map(|d| format!("{d:?}")),
            |t| parse_declared_stamped(t).map(|d| format!("{d:?}")),
            LOL_GEOMETRY_SEMANTICS_ID,
        ),
        (
            "law",
            LAW,
            wrap_law,
            |t| parse_law(t).map(|l| format!("{l:?}")),
            |t| parse_law_stamped(t).map(|l| format!("{l:?}")),
            LOL_SEMANTICS_ID,
        ),
    ]
}

#[test]
fn the_wrapper_with_this_releases_identifier_reads_the_same_value() {
    let list = entries();
    assert_eq!(list.len(), 4);
    for (name, plain, wrap, parse, stamped, id) in list {
        let want = parse(plain).unwrap_or_else(|e| panic!("{name}: {e}"));
        let text = wrap(&hex(&id), plain);
        assert_eq!(
            parse(&text).unwrap_or_else(|e| panic!("{name}: {e}")),
            want,
            "{name}"
        );
        assert_eq!(
            stamped(&text).unwrap_or_else(|e| panic!("{name}: {e}")),
            want,
            "{name}"
        );
    }
}

/// the identifier alice-lol 0.4.1 stamped (alice-det-math 0.5.0) reproduces the same bits, so a
/// text wrapped with it reads as the plain text through the plain and the stamped entry points
#[test]
fn an_earlier_release_identifier_of_the_same_bits_reads_the_same_value() {
    use alice_lol::geometry_id::PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS;
    use alice_lol::law_id::PREVIOUS_LOL_SEMANTICS_IDS;
    let mut compared = 0;
    for (name, plain, wrap, parse, stamped, id) in entries() {
        let previous = if id == LOL_SEMANTICS_ID {
            PREVIOUS_LOL_SEMANTICS_IDS
        } else {
            PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS
        };
        let want = parse(plain).unwrap_or_else(|e| panic!("{name}: {e}"));
        for old in previous {
            let text = wrap(&hex(old), plain);
            assert_eq!(
                parse(&text).unwrap_or_else(|e| panic!("{name}: {e}")),
                want,
                "{name}"
            );
            assert_eq!(
                stamped(&text).unwrap_or_else(|e| panic!("{name}: {e}")),
                want,
                "{name}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 4, "one earlier identifier per entry");
}

/// an earlier identifier of the other list is not this reader's: a law text stamped with the
/// 0.4.1 geometry identifier, and a geometry text stamped with the 0.4.1 law identifier, are
/// each the mismatch naming both
#[test]
fn an_earlier_identifier_of_the_other_kind_is_a_mismatch() {
    use alice_lol::geometry_id::PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS;
    use alice_lol::law_id::PREVIOUS_LOL_SEMANTICS_IDS;
    let mut compared = 0;
    for (name, plain, wrap, parse, stamped, id) in entries() {
        let others = if id == LOL_SEMANTICS_ID {
            PREVIOUS_LOL_GEOMETRY_SEMANTICS_IDS
        } else {
            PREVIOUS_LOL_SEMANTICS_IDS
        };
        for theirs in others {
            let text = wrap(&hex(theirs), plain);
            for (entry, e) in [("plain", parse(&text)), ("stamped", stamped(&text))] {
                assert_eq!(
                    e.expect_err(name).kind,
                    ParseErrorKind::SemanticsMismatch {
                        ours: id,
                        theirs: *theirs
                    },
                    "{name} {entry}"
                );
            }
            compared += 1;
        }
    }
    assert_eq!(
        compared, 4,
        "one earlier identifier of the other kind per entry"
    );
}

#[test]
fn another_identifier_is_a_mismatch_naming_both() {
    for (name, plain, wrap, parse, stamped, id) in entries() {
        let theirs = other(&id);
        let text = wrap(&hex(&theirs), plain);
        for (entry, e) in [("plain", parse(&text)), ("stamped", stamped(&text))] {
            let e = e.expect_err(name);
            assert_eq!(
                e.kind,
                ParseErrorKind::SemanticsMismatch { ours: id, theirs },
                "{name} {entry}"
            );
            assert!(
                e.message.contains(&hex(&id)) && e.message.contains(&hex(&theirs)),
                "{name} {entry}: {}",
                e.message
            );
        }
    }
}

#[test]
fn the_identifier_is_compared_before_the_body_is_read() {
    // a body this release cannot read, under another identifier: the mismatch is reported
    let theirs = other(&LOL_GEOMETRY_SEMANTICS_ID);
    let e = parse_lol(&format!(
        "semantics({}, construct_of_a_later_release(1.0))",
        hex(&theirs)
    ))
    .unwrap_err();
    assert!(
        matches!(e.kind, ParseErrorKind::SemanticsMismatch { .. }),
        "{e}"
    );
}

#[test]
fn a_stamped_entry_refuses_a_text_without_the_wrapper() {
    for (name, plain, _, parse, stamped, _) in entries() {
        assert!(parse(plain).is_ok(), "{name}: the plain entry reads it");
        let e = stamped(plain).expect_err(name);
        assert_eq!(e.kind, ParseErrorKind::SemanticsMissing, "{name}: {e}");
    }
}

#[test]
fn syntax_errors_stay_syntax() {
    for (name, text) in [
        ("lol", "sphere(1.0"),
        ("lol", "semantics(1234, sphere(1.0))"),
        ("lol", "semantics sphere(1.0)"),
    ] {
        let e = parse_lol(text).expect_err(name);
        assert_eq!(e.kind, ParseErrorKind::Syntax, "{text}: {e}");
    }
    let id = hex(&LOL_GEOMETRY_SEMANTICS_ID);
    for text in [
        format!("semantics({id}, sphere(1.0)"),
        format!("semantics({id} sphere(1.0))"),
        format!("semantics({}, sphere(1.0))", id.to_uppercase()),
        format!("semantics({id}0, sphere(1.0))"),
        format!("semantics({id}, sphere(1.0)) extra"),
    ] {
        let e = parse_lol(&text).expect_err(&text);
        assert_eq!(e.kind, ParseErrorKind::Syntax, "{text}: {e}");
    }
    let law = hex(&LOL_SEMANTICS_ID);
    for text in [
        format!("audit g\nsemantics {law}\nevidence compared\n"),
        format!("semantics {law} extra\naudit g\nevidence compared\n"),
        "semantics 12\naudit g\nevidence compared\n".to_owned(),
    ] {
        let e = parse_law(&text).expect_err(&text);
        assert_eq!(e.kind, ParseErrorKind::Syntax, "{text}: {e}");
    }
}
