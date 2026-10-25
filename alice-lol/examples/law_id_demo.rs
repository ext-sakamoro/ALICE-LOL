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
    audit_verdict_order_fingerprint, fold_semantics_pins, known_semantics_id, LawIdHasher,
    AUDIT_LAW_KIND, LOL_SEMANTICS_BY_DET_MATH, LOL_SEMANTICS_ID, LOL_SEMANTICS_PINS,
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

/// 規則の Law (kind machine / predicate) の pin、測り直した指紋、fold と宣言
fn rule_semantics() {
    println!("\n=== 規則の Law (kind machine / predicate) の意味論 ===");
    let measured = alice_lol::rule_law::rule_semantics_fingerprints();
    for (name, id) in alice_lol::rule_law::RULE_SEMANTICS_PINS {
        let now = measured
            .iter()
            .find(|(n, _)| n == name)
            .map_or_else(|| "(この build では測れない)".to_owned(), |(_, m)| hex(m));
        println!("  pin {name}");
        println!("      {}  測り直し {now}", hex(id));
    }
    println!(
        "  fold = {}",
        hex(&fold_semantics_pins(
            alice_lol::rule_law::RULE_SEMANTICS_PINS
        ))
    );
    println!("  宣言 = {}", hex(&alice_lol::rule_law::RULE_SEMANTICS_ID));
    let machine = alice_lol::rule_law::rule_law_from_file(
        "kind machine\nstate a initial\nstate b\nevent go\non go from a to b\n",
    )
    .expect("the demo machine reads");
    println!(
        "  {:?} の識別子 {}",
        machine.kind(),
        short(&machine.law_id())
    );
}

/// money を持つ Law と法令の Law (kind statute) の pin、測り直した指紋、fold と宣言
fn money_and_statute_semantics() {
    use alice_lol::rule_law as r;
    println!("\n=== money を持つ規則の Law の意味論 ===");
    for ((name, id), (_, m)) in r::MONEY_SEMANTICS_PINS
        .iter()
        .zip(&r::money_semantics_fingerprints())
    {
        println!("  pin {name}");
        println!("      {}  測り直し {}", hex(id), hex(m));
    }
    println!(
        "  predicate: fold = {}  宣言 = {}",
        hex(&fold_semantics_pins(r::MONEY_SEMANTICS_PINS)),
        hex(&r::MONEY_SEMANTICS_ID)
    );
    println!(
        "  table:     fold = {}  宣言 = {}",
        hex(&fold_semantics_pins(r::MONEY_TABLE_SEMANTICS_PINS)),
        hex(&r::MONEY_TABLE_SEMANTICS_ID)
    );
    println!("\n=== 法令の Law (kind statute) の意味論 ===");
    for ((name, id), (_, m)) in r::STATUTE_SEMANTICS_PINS
        .iter()
        .zip(&r::statute_semantics_fingerprints())
    {
        println!("  pin {name}");
        println!("      {}  測り直し {}", hex(id), hex(m));
    }
    println!(
        "  fold = {}  宣言 = {}",
        hex(&fold_semantics_pins(r::STATUTE_SEMANTICS_PINS)),
        hex(&r::STATUTE_SEMANTICS_ID)
    );
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../laws/spike/demo_statute.law"),
    )
    .expect("laws/spike/demo_statute.law");
    if let r::RuleLaw::Statute(s) = r::rule_law_from_file(&text).expect("the demo statute reads") {
        println!("  「{}」 {} 条", s.title(), s.clauses().len());
        for c in s.clauses() {
            println!(
                "    {} {} {}..{}",
                c.label(),
                c.kind().word(),
                c.from(),
                c.until().map_or_else(String::new, |u| u.to_string())
            );
        }
    }
}

/// 表の Law (kind table) の pin、測り直した指紋、fold と宣言
fn table_semantics() {
    println!("\n=== 表の Law (kind table) の意味論 ===");
    let measured = alice_lol::rule_law::table_semantics_fingerprints();
    for ((name, id), (_, m)) in alice_lol::rule_law::TABLE_SEMANTICS_PINS
        .iter()
        .zip(&measured)
    {
        println!("  pin {name}");
        println!("      {}  測り直し {}", hex(id), hex(m));
    }
    println!(
        "  fold = {}",
        hex(&fold_semantics_pins(
            alice_lol::rule_law::TABLE_SEMANTICS_PINS
        ))
    );
    println!("  宣言 = {}", hex(&alice_lol::rule_law::TABLE_SEMANTICS_ID));
    let table = alice_lol::rule_law::rule_law_from_file(
        "kind table\nx-input n integer\nvalue a b\nrow pos: a when n > 0\notherwise b\n",
    )
    .expect("the demo table reads");
    println!(
        "  {:?} ({}) の識別子 {}",
        table.kind(),
        String::from_utf8_lossy(alice_lol::rule_law::TABLE_LAW_KIND),
        short(&table.law_id())
    );
}

fn main() -> Result<(), ParseError> {
    println!("=== 算術と判定の意味論 ===");
    for (name, id) in LOL_SEMANTICS_PINS {
        println!("  pin {name}");
        println!("      {}", hex(id));
    }
    println!("  fold = {}", hex(&fold_semantics_pins(LOL_SEMANTICS_PINS)));
    println!("  宣言 = {}", hex(&LOL_SEMANTICS_ID));
    // alice-det-math の世代ごとの識別子 (前の世代で作った識別子もどの世代のものか分かる)
    for (det_math, lol) in LOL_SEMANTICS_BY_DET_MATH {
        let linked = if *lol == LOL_SEMANTICS_ID {
            " (build した alice-det-math)"
        } else {
            ""
        };
        println!("  alice-det-math {} -> {}{linked}", hex(det_math), hex(lol));
    }
    match known_semantics_id(&LOL_SEMANTICS_ID) {
        Some(det_math) => println!("  宣言は alice-det-math {} の世代", hex(&det_math)),
        None => println!("  宣言はどの世代の表にも無い"),
    }
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
    println!(
        "  監査の量の導出を今ここで測り直した指紋 = {}",
        hex(&alice_lol::law_input::derivation_fingerprint())
    );
    println!("  ⚠️ 上の pin と測り直した指紋が揃わなければ、識別子が振る舞いを名乗れていない");

    rule_semantics();
    table_semantics();
    money_and_statute_semantics();

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

    println!("\n=== 研究 Law も主張で名前が決まる ===");
    research_law_ids();

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

/// 研究 Law の識別子: 式の書き方 (空白・括弧) では変わらず、値や単位の文字列で変わり、
/// 主張を支えるだけのもの (parameter の不確かさ) では変わらない
fn research_law_ids() {
    use alice_lol::research_law::{Param, ResearchLaw, Var};
    use alice_zip::law::{Provenance, ValidRange};
    let gas = |expr: &str, r_unc: f64, v_unit: &str| {
        ResearchLaw::new(
            "ideal gas",
            expr,
            Var::new("P", "Pa"),
            &[
                Var::new("n", "mol"),
                Var::new("T", "K"),
                Var::new("V", v_unit),
            ],
            &[Param::new("R", 8.314_462_618, r_unc, "J/(mol*K)")],
            &[(
                "T",
                ValidRange {
                    lo: 1.0,
                    hi: 2000.0,
                },
            )],
            Provenance::new("closed form", "stated"),
        )
        .expect("a valid law")
    };
    let base = gas("n*R*T/V", 0.0, "m^3");
    for (label, law) in [
        ("そのもの", gas("n*R*T/V", 0.0, "m^3")),
        ("空白と括弧", gas("((n*R)*T) / V", 0.0, "m^3")),
        ("不確かさだけ", gas("n*R*T/V", 0.5, "m^3")),
        ("単位の書き方", gas("n*R*T/V", 0.0, "m*m*m")),
        ("掛ける順序", gas("R*n*T/V", 0.0, "m^3")),
    ] {
        let id = law.law_id(&LOL_SEMANTICS_ID);
        let same = if id == base.law_id(&LOL_SEMANTICS_ID) {
            "同じ"
        } else {
            "別"
        };
        println!("  {label:<10} {} ({same})", short(&id));
    }
}
