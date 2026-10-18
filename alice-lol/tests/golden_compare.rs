//! digest golden の照合 (`common/golden.rs`) が違いを拒むことの試験 (ALICE-Law G11 の canary)
//!
//! `laser_pattern_digest_golden` と `stdlib_profile_determinism_oracle` は golden との照合を
//! `golden::compare` に任せる その照合が違う digest を通す、0 行で通る、といった壊れ方を
//! すると、goldens は何も見ないまま green になる ここでは既知の行に 1 箇所ずつ違いを入れ、
//! それぞれが拒まれることを見る

#[path = "common/golden.rs"]
mod golden;

use sha2::{Digest, Sha256};

/// SHA-256 of the empty input, written out from the standard's test vectors (not computed here)
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn rows() -> Vec<golden::Row> {
    vec![
        ("a(1)".to_string(), 3, EMPTY_SHA256.to_string()),
        ("b(2, 3)".to_string(), 5, "0".repeat(64)),
    ]
}

#[test]
fn hex32_of_sha256_is_the_known_answer() {
    assert_eq!(golden::hex32(&Sha256::digest(b"")), EMPTY_SHA256);
}

#[test]
fn identical_rows_compare_and_count() {
    assert_eq!(golden::compare(&rows(), &rows()), Ok(2));
}

#[test]
fn a_wrong_digest_is_refused() {
    let mut want = rows();
    want[1].2.replace_range(63..64, "1");
    let err = golden::compare(&rows(), &want).unwrap_err();
    assert!(err.contains("b(2, 3): digest"), "{err}");
}

#[test]
fn a_wrong_count_is_refused() {
    let mut want = rows();
    want[0].1 = 4;
    assert!(golden::compare(&rows(), &want)
        .unwrap_err()
        .contains("a(1): count 3, golden 4"));
}

#[test]
fn a_different_order_is_refused() {
    let mut want = rows();
    want.swap(0, 1);
    assert!(golden::compare(&rows(), &want)
        .unwrap_err()
        .contains("order or name differs"));
}

#[test]
fn a_missing_row_is_refused() {
    let want = rows()[..1].to_vec();
    assert_eq!(
        golden::compare(&rows(), &want),
        Err("2 case(s) now, 1 in the golden".to_string())
    );
}

#[test]
fn nothing_compared_is_refused() {
    assert!(golden::compare(&[], &[])
        .unwrap_err()
        .starts_with("compared nothing"));
    assert!(golden::compare(&rows(), &[])
        .unwrap_err()
        .starts_with("compared nothing"));
}

#[test]
fn render_and_parse_round_trip() {
    assert_eq!(golden::parse(&golden::render(&rows())), Ok(rows()));
}

#[test]
fn a_malformed_golden_line_is_refused() {
    assert!(golden::parse("a|3\n").unwrap_err().contains("line 1"));
    assert!(golden::parse("a|x|00\n")
        .unwrap_err()
        .contains("not an integer"));
    let short = format!("a|3|{}\n", &EMPTY_SHA256[..63]);
    assert!(golden::parse(&short)
        .unwrap_err()
        .contains("64 lower-case hex"));
    let upper = format!("a|3|{}\n", EMPTY_SHA256.to_uppercase());
    assert!(golden::parse(&upper)
        .unwrap_err()
        .contains("64 lower-case hex"));
}
