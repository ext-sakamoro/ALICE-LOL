//! 監査 Law の読み方のうち、規則の文面だけでは決まっていなかった 2 点を固定する.
//!
//! 1. **証拠** — 有限で 0 より大きい数だけが証拠になる 負の数・NaN・±∞ は
//!    「数えた」ことを示さないので、測られていないのと同じ (`NoEvidence`)
//! 2. **成立範囲** — 値の組は**集合**として比べる 順序と重複は判定に影響しない
//!    (実測側も Law 側も同じ正規化を通る)
//!
//! 3. **期待値の実測が有限でない** — NaN・±∞ は測られていないのと同じ (`Undecided`)
//!    ⚠️ `(NaN - v).abs() > tol` は false なので、検査しないと `Supports` に倒れる
//!
//! どれも別の言語で書いた実装と同じ判定を出すための規則で、
//! `conformance/TASK.md` の監査の表と同じ内容

use alice_lol::audit_law::{Measurements, Verdict};
use alice_lol::runtime_parser::parse_law;

fn evidence_law() -> alice_lol::audit_law::AuditLaw {
    parse_law("audit evidence-only\nevidence compared\n").expect("the law text parses")
}

fn no_evidence() -> Verdict {
    Verdict::NoEvidence {
        metric: "compared".to_owned(),
    }
}

fn judge_evidence(value: f64) -> Verdict {
    evidence_law().evaluate(&Measurements::new().with_number("compared", value))
}

#[test]
fn a_negative_count_is_not_evidence() {
    assert_eq!(judge_evidence(-1.0), no_evidence());
}

#[test]
fn nan_is_not_evidence() {
    assert_eq!(judge_evidence(f64::NAN), no_evidence());
}

#[test]
fn an_infinite_count_is_not_evidence() {
    assert_eq!(judge_evidence(f64::INFINITY), no_evidence());
    assert_eq!(judge_evidence(f64::NEG_INFINITY), no_evidence());
}

#[test]
fn zero_and_negative_zero_are_not_evidence() {
    assert_eq!(judge_evidence(0.0), no_evidence());
    assert_eq!(judge_evidence(-0.0), no_evidence());
}

#[test]
fn a_positive_fraction_is_evidence() {
    // 「0 より大きい有限の数」なので整数でなくてよい
    assert_eq!(judge_evidence(0.5), Verdict::Supports);
    assert_eq!(judge_evidence(f64::MIN_POSITIVE), Verdict::Supports);
}

const RANGE_LAW: &str = "audit range-only\nrange k 0.3.2 0.4.0\n";

fn judge_range(values: &[&str]) -> Verdict {
    parse_law(RANGE_LAW)
        .expect("the law text parses")
        .evaluate(&Measurements::new().with_range("k", values))
}

#[test]
fn a_duplicate_in_the_measured_range_is_the_same_set() {
    assert_eq!(judge_range(&["0.3.2", "0.4.0", "0.3.2"]), Verdict::Supports);
    assert_eq!(judge_range(&["0.4.0", "0.4.0", "0.3.2"]), Verdict::Supports);
}

#[test]
fn a_measured_range_in_another_order_is_the_same_set() {
    assert_eq!(judge_range(&["0.4.0", "0.3.2"]), Verdict::Supports);
}

#[test]
fn a_different_set_is_still_a_parameter_update() {
    // 正規化が「何でも同じに見える」方向に壊れていないことの歯
    assert_eq!(
        judge_range(&["0.3.2", "0.3.2"]),
        Verdict::ParameterUpdate {
            key: "k".to_owned(),
            was: vec!["0.3.2".to_owned(), "0.4.0".to_owned()],
            now: vec!["0.3.2".to_owned()],
        }
    );
}

#[test]
fn the_measured_range_is_kept_as_a_sorted_set() {
    let m = Measurements::new().with_range("k", &["0.10.0", "0.9.0", "0.10.0", "0.9.0"]);
    assert_eq!(
        m.range("k"),
        Some(["0.9.0".to_owned(), "0.10.0".to_owned()].as_slice())
    );
}

#[test]
fn a_duplicate_in_the_law_range_is_the_same_law() {
    // Law の側も同じ正規化を通る 重複を書いた Law と書かない Law は同じ主張で、
    // 同じ識別子を持つ
    let with_dup = parse_law("audit range-only\nrange k 0.4.0 0.3.2 0.4.0\n").expect("parses");
    let plain = parse_law(RANGE_LAW).expect("parses");
    assert_eq!(with_dup, plain);
    let id = alice_lol::law_id::LOL_SEMANTICS_ID;
    assert_eq!(with_dup.law_id(&id), plain.law_id(&id));
    assert_eq!(
        with_dup.evaluate(&Measurements::new().with_range("k", &["0.3.2", "0.4.0"])),
        Verdict::Supports
    );
}

#[test]
fn values_with_the_same_version_key_still_form_a_set() {
    // `1.0` と `1-0`、`01` と `1` は版の並び順では同じ位置に来る 文字列で tie を
    // 切らないと、入力順が残って重複が隣り合わず、同じ集合が別の列になる
    let law = parse_law("audit tie\nrange k 1.0 1-0 01 1\n").expect("parses");
    for measured in [
        ["1-0", "1.0", "1", "01", "1-0"].as_slice(),
        ["1", "1.0", "01", "1-0", "1.0"].as_slice(),
    ] {
        assert_eq!(
            law.evaluate(&Measurements::new().with_range("k", measured)),
            Verdict::Supports,
            "{measured:?}"
        );
    }
}

fn judge_expect(value: f64) -> Verdict {
    parse_law("audit expect-only\nexpect mismatches == 0\n")
        .expect("the law text parses")
        .evaluate(&Measurements::new().with_number("mismatches", value))
}

const fn is_undecided(v: &Verdict) -> bool {
    matches!(v, Verdict::Undecided { .. })
}

#[test]
fn a_nan_measurement_of_an_expectation_is_undecided() {
    assert!(is_undecided(&judge_expect(f64::NAN)));
}

#[test]
fn an_infinite_measurement_of_an_expectation_is_undecided() {
    assert!(is_undecided(&judge_expect(f64::INFINITY)));
    assert!(is_undecided(&judge_expect(f64::NEG_INFINITY)));
}

#[test]
fn a_finite_measurement_of_an_expectation_is_still_judged() {
    assert_eq!(judge_expect(0.0), Verdict::Supports);
    assert!(matches!(judge_expect(1.0), Verdict::Breaks { .. }));
}
