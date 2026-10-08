//! 監査 Law のデモ: 検査の主張を text で書き、測り方を知らない Law に実測を食わせる
//!
//! ```text
//! cargo run -p alice-lol --example audit_law_demo
//! ```
//!
//! 同じ Law が、渡された実測によって 6 通りの判定を返す 「測れていない」と
//! 「違反した」が別の値なので、検査器の空振りを pass と区別できる

use alice_lol::audit_law::{AuditLaw, Measurements, Verdict};
use alice_lol::runtime_parser::{parse_law, ParseError};

/// 同じ build に同じ crate の semver 非互換な 2 版が入っていないこと
const LAW: &str = "\
# lock に同じ crate の非互換な 2 版が居ないこと
audit lock-single-version
evidence packages
evidence alice_packages
expect unbaselined_duplicates == 0
range alice-det-math 0.3.2 0.4.0
";

/// 既存の検査器が log に出していた実測値
fn healthy() -> Measurements {
    Measurements::new()
        .with_number("packages", 175.0)
        .with_number("alice_packages", 14.0)
        .with_number("unbaselined_duplicates", 0.0)
        .with_range("alice-det-math", &["0.3.2", "0.4.0"])
}

fn report(law: &AuditLaw, case: &str, measured: &Measurements) {
    let verdict = law.evaluate(measured);
    let mark = if verdict.passed() { "pass" } else { "FAIL" };
    println!("  [{mark}] {case}");
    println!("         {verdict}");
}

fn main() -> Result<(), ParseError> {
    let law = parse_law(LAW)?;

    println!("=== 監査 Law: {} ===", law.name());
    println!("主張 {} 件", law.clauses().len());
    for clause in law.clauses() {
        println!("  - {clause:?}");
    }

    println!("\n-- 測った側が渡した値 (Law はどう測ったかを知らない) --");
    let measured = healthy();
    for metric in ["packages", "alice_packages", "unbaselined_duplicates"] {
        if let Some(value) = measured.number(metric) {
            println!("  {metric} = {value}");
        }
    }
    if let Some(values) = measured.range("alice-det-math") {
        println!("  alice-det-math = {}", values.join(" "));
    }

    println!("\n-- 実測を変えると判定が変わる --");
    report(&law, "log の実測そのまま", &healthy());
    report(
        &law,
        "Law が名指ししない量は判定に効かない",
        &healthy().with_number("third_party_duplicates", 6.0),
    );
    report(
        &law,
        "lock が読めず 1 件も測れていない",
        &Measurements::new(),
    );
    report(
        &law,
        "数えたが 0 件 (検査器の空振り)",
        &healthy().with_number("packages", 0.0),
    );
    report(
        &law,
        "新しい重複が出た",
        &healthy().with_number("unbaselined_duplicates", 1.0),
    );
    report(
        &law,
        "成立範囲の版が動いた",
        &healthy().with_range("alice-det-math", &["0.3.1", "0.4.0"]),
    );
    report(
        &law,
        "成立範囲が解消した (行を消す)",
        &Measurements::new()
            .with_number("packages", 175.0)
            .with_number("alice_packages", 14.0)
            .with_number("unbaselined_duplicates", 0.0),
    );
    report(
        &law,
        "期待値の量が測られていない",
        &Measurements::new()
            .with_number("packages", 175.0)
            .with_number("alice_packages", 14.0)
            .with_range("alice-det-math", &["0.3.2", "0.4.0"]),
    );

    println!("\n-- 判定の値は 6 つ 2 値では上の 4 行が同じ fail に潰れる --");
    for verdict in [
        Verdict::Supports,
        Verdict::NoEvidence {
            metric: "packages".to_owned(),
        },
        Verdict::Breaks {
            metric: "unbaselined_duplicates".to_owned(),
            expected: "0".to_owned(),
            measured: "1".to_owned(),
        },
        Verdict::OutOfRange {
            key: "alice-det-math".to_owned(),
        },
        Verdict::ParameterUpdate {
            key: "alice-det-math".to_owned(),
            was: vec!["0.3.2".to_owned(), "0.4.0".to_owned()],
            now: vec!["0.3.1".to_owned(), "0.4.0".to_owned()],
        },
        Verdict::Undecided {
            reason: "`unbaselined_duplicates` is not in the measurement".to_owned(),
        },
    ] {
        println!(
            "  {} -> {verdict}",
            if verdict.passed() { "o" } else { "x" }
        );
    }

    Ok(())
}
