//! 監査 Law が script で書かれた既存の検査器と同じ判定を出すこと.
//!
//! 移植が正しいことの証跡は「新しく書いた方が動く」ではなく **「古い方と同じ答えを
//! 出す」** なので、`scripts/lock_single_version.py` の試験入力をそのまま Law に
//! 食わせて突き合わせる
//!
//! script 版は 16 本の試験を持つ そのうち
//!
//! * **13 本は Law の側** — 証拠・期待値・成立範囲の判定
//! * ⚠️ **3 本は測る側** — lock の読み方 (version 行の無い `name` を package と
//!   数えない / 成立範囲の値の順序 / comment だけの成立範囲) これらは「どう測るか」
//!   なので Law には現れない 分離できていることの確認として最後の 1 本に残す
//!
//! Law が script 版より **強い** 点が 1 つある: 成立範囲の組が変わった場合、script 版は
//! 一般の error を 1 つ返すだけだが、Law は `ParameterUpdate` として何が何に変わったかを返す

use alice_lol::audit_law::{Measurements, Verdict};
use alice_lol::runtime_parser::parse_law;

const LAW: &str = "\
# 同じ build に同じ crate の semver 非互換な 2 版が入っていないこと
audit lock-single-version
evidence packages
evidence alice_packages
expect unbaselined_duplicates == 0
range alice-det-math 0.3.2 0.4.0
";

fn law() -> alice_lol::audit_law::AuditLaw {
    parse_law(LAW).expect("the law text parses")
}

/// 既存 gate が CI の log に出していた実測値
fn healthy() -> Measurements {
    Measurements::new()
        .with_number("packages", 175.0)
        .with_number("alice_packages", 14.0)
        .with_number("unbaselined_duplicates", 0.0)
        .with_range("alice-det-math", &["0.3.2", "0.4.0"])
}

#[test]
fn the_law_text_states_four_clauses() {
    let l = law();
    assert_eq!(l.name(), "lock-single-version");
    assert_eq!(l.clauses().len(), 4, "{:?}", l.clauses());
}

#[test]
fn the_measurement_from_ci_supports_the_law() {
    // script 版 `test_this_repository_passes` / CI の
    // `compared: packages 175, alice packages 14, duplicates 1, baseline rows 1`
    assert_eq!(law().evaluate(&healthy()), Verdict::Supports);
}

#[test]
fn a_clean_lock_supports_the_law() {
    // script 版 `test_a_clean_lock_passes`
    let m = Measurements::new()
        .with_number("packages", 4.0)
        .with_number("alice_packages", 2.0)
        .with_number("unbaselined_duplicates", 0.0)
        .with_range("alice-det-math", &["0.3.2", "0.4.0"]);
    assert_eq!(law().evaluate(&m), Verdict::Supports);
}

#[test]
fn third_party_duplicates_are_not_the_law_s_business() {
    // script 版 `test_third_party_duplicates_are_not_reported`
    let m = healthy().with_number("third_party_duplicates", 6.0);
    assert_eq!(law().evaluate(&m), Verdict::Supports);
}

#[test]
fn a_new_duplicate_breaks_the_law() {
    // script 版 `test_a_new_alice_duplicate_fails`
    let m = healthy().with_number("unbaselined_duplicates", 1.0);
    assert_eq!(
        law().evaluate(&m),
        Verdict::Breaks {
            metric: "unbaselined_duplicates".to_owned(),
            expected: "0".to_owned(),
            measured: "1".to_owned(),
        }
    );
}

#[test]
fn a_duplicate_inside_the_range_supports_the_law() {
    // script 版 `test_a_duplicate_listed_in_the_baseline_passes`
    assert!(law().evaluate(&healthy()).passed());
}

#[test]
fn a_range_whose_versions_moved_is_a_parameter_update() {
    // script 版 `test_a_baseline_row_with_other_versions_fails`
    // ⚠️ script 版は一般の error、Law は何が何に変わったかを返す
    let m = healthy().with_range("alice-det-math", &["0.3.1", "0.4.0"]);
    assert_eq!(
        law().evaluate(&m),
        Verdict::ParameterUpdate {
            key: "alice-det-math".to_owned(),
            was: vec!["0.3.2".to_owned(), "0.4.0".to_owned()],
            now: vec!["0.3.1".to_owned(), "0.4.0".to_owned()],
        }
    );
}

#[test]
fn a_resolved_range_is_out_of_range() {
    // script 版 `test_a_resolved_baseline_row_fails`
    let m = Measurements::new()
        .with_number("packages", 175.0)
        .with_number("alice_packages", 14.0)
        .with_number("unbaselined_duplicates", 0.0);
    assert_eq!(
        law().evaluate(&m),
        Verdict::OutOfRange {
            key: "alice-det-math".to_owned(),
        }
    );
}

#[test]
fn a_range_for_a_crate_that_left_the_lock_is_out_of_range() {
    // script 版 `test_a_baseline_row_for_a_crate_that_left_the_lock_fails`
    // Law が名指しした key が実測に無いので上と同じ判定 (消す話としては同じ)
    let m = Measurements::new()
        .with_number("packages", 175.0)
        .with_number("alice_packages", 14.0)
        .with_number("unbaselined_duplicates", 0.0)
        .with_range("alice-other", &["0.1.0", "0.2.0"]);
    assert_eq!(
        law().evaluate(&m),
        Verdict::OutOfRange {
            key: "alice-det-math".to_owned(),
        }
    );
}

#[test]
fn three_versions_still_break_the_law() {
    // script 版 `test_three_versions_are_reported_in_order`
    let m = healthy().with_number("unbaselined_duplicates", 1.0);
    assert!(matches!(law().evaluate(&m), Verdict::Breaks { .. }));
}

#[test]
fn an_empty_lock_is_no_evidence_not_a_pass() {
    // script 版 `test_an_empty_lock_fails` ⚠️ 2 値ではここが潰れる
    let m = Measurements::new()
        .with_number("packages", 0.0)
        .with_number("alice_packages", 0.0)
        .with_number("unbaselined_duplicates", 0.0)
        .with_range("alice-det-math", &["0.3.2", "0.4.0"]);
    assert_eq!(
        law().evaluate(&m),
        Verdict::NoEvidence {
            metric: "packages".to_owned(),
        }
    );
}

#[test]
fn a_lock_without_alice_packages_is_no_evidence() {
    // script 版 `test_a_lock_without_alice_packages_fails`
    let m = healthy()
        .with_number("packages", 1.0)
        .with_number("alice_packages", 0.0);
    assert_eq!(
        law().evaluate(&m),
        Verdict::NoEvidence {
            metric: "alice_packages".to_owned(),
        }
    );
}

#[test]
fn an_unreadable_lock_is_no_evidence() {
    // script 版 `test_a_missing_lock_fails` / `test_a_binary_lock_fails`
    // 読めなければ何も測れない = 証拠なし (2 件とも同じ判定)
    assert_eq!(
        law().evaluate(&Measurements::new()),
        Verdict::NoEvidence {
            metric: "packages".to_owned(),
        }
    );
}

#[test]
fn evidence_is_judged_before_expectations() {
    // ⚠️ 測れていない時に `0 == 0` が偶然通るのを防ぐ
    // (script 版が「比較 0 件なら fail」を手で書いていた理由がこれ)
    let m = Measurements::new()
        .with_number("packages", 0.0)
        .with_number("alice_packages", 0.0)
        .with_number("unbaselined_duplicates", 0.0)
        .with_range("alice-det-math", &["0.3.2", "0.4.0"]);
    assert!(matches!(law().evaluate(&m), Verdict::NoEvidence { .. }));
}

#[test]
fn the_order_of_range_values_does_not_matter() {
    // script 版 `test_the_versions_of_a_baseline_row_may_be_written_in_any_order`
    // 残る 2 本 (version 行の無い `name` / comment だけの成立範囲) は
    // lock の読み方なので Law には現れない = 分離できている
    let any_order = healthy().with_range("alice-det-math", &["0.4.0", "0.3.2"]);
    assert_eq!(law().evaluate(&any_order), Verdict::Supports);
}

#[test]
fn a_law_that_states_nothing_is_rejected() {
    assert!(parse_law("audit empty\n").is_err());
    assert!(parse_law("evidence packages\n").is_err());
}
