//! `includes <key> <value>...`: the measured set contains every listed value
//!
//! The claim of a list that must hold at least these values (a set of banned methods that
//! every crate's lint configuration must carry, more being allowed). It is a claim, checked
//! with `expect` in the third step, in file order: a key that is not measured is
//! `undecided`, a missing value `breaks` with the key as the subject and the missing values
//! as what was measured. Order and duplicates are ignored on both sides, and values are
//! compared as text.
//!
//! oracle: the definition of set inclusion (A ⊇ B iff every element of B is in A); the
//! expected values below are written from it, not taken from the implementation

use alice_lol::audit_law::{Measurements, Verdict};
use alice_lol::runtime_parser::parse_law;

const LAW: &str = "audit lint-bans\nincludes banned sin cos powi\n";

fn judge(law: &str, m: &Measurements) -> Verdict {
    parse_law(law).expect("the law text parses").evaluate(m)
}

fn banned(values: &[&str]) -> Measurements {
    Measurements::new().with_range("banned", values)
}

#[test]
fn a_superset_supports() {
    assert_eq!(
        judge(LAW, &banned(&["sin", "cos", "powi", "tan"])),
        Verdict::Supports
    );
}

#[test]
fn the_same_set_supports() {
    assert_eq!(
        judge(LAW, &banned(&["powi", "cos", "sin"])),
        Verdict::Supports
    );
}

#[test]
fn order_and_duplicates_are_ignored_on_both_sides() {
    let law = "audit lint-bans\nincludes banned sin sin cos\n";
    assert_eq!(
        judge(law, &banned(&["cos", "sin", "cos"])),
        Verdict::Supports
    );
}

#[test]
fn a_missing_value_breaks_with_the_key_and_only_the_missing_values() {
    match judge(LAW, &banned(&["sin", "tan"])) {
        Verdict::Breaks {
            metric, measured, ..
        } => {
            assert_eq!(metric, "banned");
            // the words after the label are exactly the missing values (sorted, as the set is)
            let words: Vec<&str> = measured.split_whitespace().skip(1).collect();
            assert_eq!(words, ["cos", "powi"], "{measured}");
        }
        other => panic!("expected breaks, got {other:?}"),
    }
}

#[test]
fn an_empty_measured_set_breaks() {
    assert!(matches!(judge(LAW, &banned(&[])), Verdict::Breaks { .. }));
}

#[test]
fn a_key_that_is_not_measured_is_undecided() {
    assert!(matches!(
        judge(LAW, &Measurements::new()),
        Verdict::Undecided { .. }
    ));
}

#[test]
fn values_are_compared_as_text() {
    let law = "audit lint-bans\nincludes banned f32::sin\n";
    assert!(matches!(
        judge(law, &banned(&["f64::sin"])),
        Verdict::Breaks { .. }
    ));
    assert_eq!(judge(law, &banned(&["f32::sin"])), Verdict::Supports);
}

#[test]
fn includes_needs_at_least_one_value() {
    assert!(parse_law("audit lint-bans\nincludes banned\n").is_err());
    assert!(parse_law("audit lint-bans\nincludes\n").is_err());
}

#[test]
fn evidence_comes_before_includes() {
    let law = "audit lint-bans\nincludes banned sin\nevidence compared\n";
    let m = banned(&[]).with_number("compared", 0.0);
    assert!(matches!(judge(law, &m), Verdict::NoEvidence { .. }));
}

#[test]
fn range_comes_before_includes() {
    let law = "audit lint-bans\nincludes banned sin\nrange known a\n";
    let m = banned(&[]);
    assert!(
        matches!(judge(law, &m), Verdict::OutOfRange { .. }),
        "the range key is not measured"
    );
}

#[test]
fn includes_and_expect_are_checked_in_file_order() {
    let first_includes = "audit lint-bans\nincludes banned sin\nexpect n == 1\n";
    let first_expect = "audit lint-bans\nexpect n == 1\nincludes banned sin\n";
    let m = banned(&[]).with_number("n", 2.0);
    match judge(first_includes, &m) {
        Verdict::Breaks { metric, .. } => assert_eq!(metric, "banned"),
        other => panic!("{other:?}"),
    }
    match judge(first_expect, &m) {
        Verdict::Breaks { metric, .. } => assert_eq!(metric, "n"),
        other => panic!("{other:?}"),
    }
}
