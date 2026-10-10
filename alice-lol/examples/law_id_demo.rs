//! Law に名前を付ける — 同じ主張に同じ識別子、算術が違えば別の識別子.
//!
//! ```text
//! cargo run -p alice-lol --example law_id_demo
//! ```
//!
//! 識別子が何から決まるかを、同じ Law・1 文字違いの Law・算術世代を変えた場合の
//! 3 通りで並べて示す 最後に、下流が自分の種類の Law に名前を付ける時の組み立ても出す

use alice_lol::audit_law::AuditLaw;
use alice_lol::law_id::{
    audit_verdict_order_fingerprint, fold_semantics_pins, LawIdHasher, AUDIT_LAW_KIND,
    LOL_SEMANTICS_ID, LOL_SEMANTICS_PINS,
};
use alice_lol::runtime_parser::{parse_law, ParseError};

const LAW: &str = "\
audit lock-single-version
evidence packages
expect unbaselined_duplicates == 0
range alice-det-math 0.3.2 0.4.0
";

/// 算術世代を 1 つ変えた架空の値 (比較のためだけに使う)
const OTHER_SEMANTICS: [u8; 32] = [0x5a; 32];

fn hex(id: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    id.iter().fold(String::with_capacity(64), |mut s, b| {
        // String への write は失敗しない
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn short(id: &[u8; 32]) -> String {
    hex(id)[..16].to_owned()
}

fn main() -> Result<(), ParseError> {
    println!("=== 算術と判定の意味論 ===");
    for (name, id) in LOL_SEMANTICS_PINS {
        println!("  pin {name}");
        println!("      {}", hex(id));
    }
    println!("  fold = {}", hex(&fold_semantics_pins(LOL_SEMANTICS_PINS)));
    println!("  宣言 = {}", hex(&LOL_SEMANTICS_ID));
    println!(
        "  判定順序を今ここで測り直した指紋 = {}",
        hex(&audit_verdict_order_fingerprint())
    );
    println!(
        "  入力の読み方を今ここで測り直した指紋 = {}",
        hex(&alice_lol::law_input::input_reading_fingerprint())
    );
    println!(
        "  研究 Law の式の関数を今ここで測り直した指紋 = {}",
        hex(&alice_lol::research_law::expression_functions_fingerprint())
    );
    println!("  ⚠️ 上の pin と測り直した指紋が揃わなければ、識別子が振る舞いを名乗れていない");

    println!("\n=== 同じ Law は同じ名前 ===");
    let law: AuditLaw = parse_law(LAW)?;
    let again: AuditLaw = parse_law(LAW)?;
    println!("  1 回目 {}", short(&law.law_id(&LOL_SEMANTICS_ID)));
    println!("  2 回目 {}", short(&again.law_id(&LOL_SEMANTICS_ID)));

    println!("\n=== 1 文字変えれば別の名前 ===");
    for (label, text) in [
        ("期待値 0 -> 1", LAW.replace("== 0", "== 1")),
        (
            "量の名前",
            LAW.replace("evidence packages", "evidence crates"),
        ),
        ("Law の名前", LAW.replace("lock-single-version", "lock-one")),
    ] {
        let other: AuditLaw = parse_law(&text)?;
        println!("  {label:<14} {}", short(&other.law_id(&LOL_SEMANTICS_ID)));
    }

    println!("\n=== 算術が違えば別の名前 (text は同じ) ===");
    println!("  この世代     {}", short(&law.law_id(&LOL_SEMANTICS_ID)));
    println!("  別の世代     {}", short(&law.law_id(&OTHER_SEMANTICS)));

    println!("\n=== 下流が自分の種類の Law に名前を付ける時 ===");
    // 値ごとに型と長さを書くので、連結が曖昧にならない
    let built = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .str("my-own-law")
        .bytes(b"raw")
        .u32(1)
        .u64(1)
        .f64(0.5)
        .finish();
    println!("  組み立てた名前 {}", short(&built));
    let swapped = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .str("my-own-law")
        .bytes(b"raw")
        .u64(1)
        .u32(1)
        .f64(0.5)
        .finish();
    println!("  u32 と u64 を入れ替えると {}", short(&swapped));
    println!("  ⚠️ 同じ 1 でも型が違えば別の名前になる (連結の曖昧さを塞いでいる)");

    Ok(())
}
