//! Law の識別子が「何を同じと呼ぶか」を固定する.
//!
//! Law は「何が成立すべきか」の記述なので、**同じ主張には同じ名前**が付かなければ
//! 保存した Law と評価する Law が同じものだと言えない 逆に主張が 1 bit でも違えば
//! 別の名前になる必要がある ここはその両方向を固定する
//!
//! ⚠️ **識別子は算術世代を含む** 同じ text の Law でも、評価する算術が違えば別の答えが
//! 出るので別の Law として扱う (`semantics_id` を混ぜる理由)
//!
//! ⚠️ **連結の曖昧さが最も危ない** 長さを前置せずに bytes を繋ぐと `["ab", "c"]` と
//! `["a", "bc"]` が同じ hash になり、**別の Law が同じ名前を名乗る** 下の衝突試験が
//! その経路を塞ぐ

use alice_lol::law_id::{
    audit_verdict_order_fingerprint, LawIdHasher, AUDIT_LAW_KIND, LOL_SEMANTICS_ID,
    LOL_SEMANTICS_PINS,
};
use alice_lol::runtime_parser::parse_law;

/// 算術世代を 1 つ変えた架空の識別子 (値そのものに意味はない)
const OTHER_SEMANTICS: [u8; 32] = [0x5a; 32];

fn audit(text: &str) -> alice_lol::audit_law::AuditLaw {
    parse_law(text).expect("the law text parses")
}

const LAW_A: &str = "\
audit lock-single-version
evidence packages
expect unbaselined_duplicates == 0
range alice-det-math 0.3.2 0.4.0
";

#[test]
fn the_same_law_gets_the_same_identifier() {
    let x = audit(LAW_A).law_id(&LOL_SEMANTICS_ID);
    let y = audit(LAW_A).law_id(&LOL_SEMANTICS_ID);
    assert_eq!(x, y);
}

#[test]
fn a_different_expected_value_gets_a_different_identifier() {
    let a = audit(LAW_A).law_id(&LOL_SEMANTICS_ID);
    let b = audit(&LAW_A.replace("== 0", "== 1")).law_id(&LOL_SEMANTICS_ID);
    assert_ne!(a, b);
}

#[test]
fn a_different_metric_name_gets_a_different_identifier() {
    let a = audit(LAW_A).law_id(&LOL_SEMANTICS_ID);
    let b = audit(&LAW_A.replace("evidence packages", "evidence crates")).law_id(&LOL_SEMANTICS_ID);
    assert_ne!(a, b);
}

#[test]
fn the_order_of_the_clauses_is_part_of_the_law() {
    // ⚠️ 判定は「証拠 → 成立範囲 → 期待値」の順に見るので、書いた順が答えを変える
    // ことはない しかし Law の text としては別物なので、識別子は分かれる
    let reordered = "\
audit lock-single-version
expect unbaselined_duplicates == 0
evidence packages
range alice-det-math 0.3.2 0.4.0
";
    assert_ne!(
        audit(LAW_A).law_id(&LOL_SEMANTICS_ID),
        audit(reordered).law_id(&LOL_SEMANTICS_ID)
    );
}

#[test]
fn the_arithmetic_generation_is_part_of_the_identifier() {
    // 同じ text でも評価する算術が違えば別の答えが出るので、別の Law として扱う
    let same_text = audit(LAW_A);
    assert_ne!(
        same_text.law_id(&LOL_SEMANTICS_ID),
        same_text.law_id(&OTHER_SEMANTICS)
    );
}

#[test]
fn the_name_of_the_law_is_part_of_the_identifier() {
    let a = audit(LAW_A).law_id(&LOL_SEMANTICS_ID);
    let b =
        audit(&LAW_A.replace("lock-single-version", "lock-one-version")).law_id(&LOL_SEMANTICS_ID);
    assert_ne!(a, b);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 衝突試験 — 連結の曖昧さで別の Law が同じ名前を名乗らないこと
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn concatenation_is_not_ambiguous() {
    // ⚠️ 長さを前置しないと `["ab", "c"]` と `["a", "bc"]` が同じ hash になる
    let ab_c = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .bytes(b"ab")
        .bytes(b"c")
        .finish();
    let a_bc = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .bytes(b"a")
        .bytes(b"bc")
        .finish();
    assert_ne!(ab_c, a_bc);
}

#[test]
fn an_empty_field_is_not_the_same_as_no_field() {
    let one_empty = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .bytes(b"")
        .finish();
    let none = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID).finish();
    assert_ne!(one_empty, none);
}

#[test]
fn integers_of_different_widths_do_not_collide() {
    let as_u32 = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .u32(5)
        .finish();
    let as_u64 = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .u64(5)
        .finish();
    assert_ne!(as_u32, as_u64);
}

#[test]
fn the_kind_separates_laws_that_encode_the_same_bytes() {
    // 別の種類の Law が偶然同じ byte 列を書いても、名前は分かれる
    let audit_kind = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .u64(1)
        .finish();
    let other_kind = LawIdHasher::new(b"lol.law.not-a-real-kind", &LOL_SEMANTICS_ID)
        .u64(1)
        .finish();
    assert_ne!(audit_kind, other_kind);
}

#[test]
fn a_float_is_encoded_by_its_bits_not_its_text() {
    // ⚠️ `-0.0` と `0.0` は `==` では等しいが、別の値として識別する
    let neg_zero = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .f64(-0.0)
        .finish();
    let pos_zero = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .f64(0.0)
        .finish();
    assert_ne!(neg_zero, pos_zero);
}

#[test]
fn every_nan_payload_is_the_same_law() {
    // ⚠️ 逆に NaN は payload が違っても「判定できない」という同じ意味なので揃える
    let a = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .f64(f64::NAN)
        .finish();
    let b = LawIdHasher::new(AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .f64(-f64::NAN)
        .finish();
    assert_eq!(a, b);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 算術世代の識別子そのもの
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// ⚠️ `hex32` は大文字の 16 進も受けるが、repo 内の literal は全部小文字なので
/// **この 1 本が無いと大文字の arm を消す変異が生き残る** (2026-10-09 実測: 大文字 arm の
/// 削除と、その中の `c - b'A' + 10` を壊す変異 3 件が合わせて missed だった)
#[test]
fn hex_literals_are_read_the_same_in_either_case() {
    let lower = "0123456789abcdef".repeat(4);
    let upper = lower.to_uppercase();
    assert_eq!(
        alice_lol::law_id::hex32(&lower),
        alice_lol::law_id::hex32(&upper),
        "大文字と小文字で違う byte 列になった"
    );
    // 値そのものも固定する (arm 内の算術を壊す変異の歯、assert を大小一致だけに
    // すると「両方同じだけ狂う」変異を見逃す)
    assert_eq!(
        alice_lol::law_id::hex32(&upper)[..8],
        [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef]
    );
}

#[test]
fn the_semantics_identifier_is_not_all_zero() {
    // 「まだ決めていない」値を識別子として配ってしまう経路を塞ぐ
    assert_ne!(LOL_SEMANTICS_ID, [0_u8; 32]);
}

#[test]
fn the_semantics_identifier_folds_the_upstream_arithmetic() {
    // 上流の算術世代が変われば、こちらの識別子も変わらなければならない
    // (上流の id を pin 表の 1 項目として含めていることの確認)
    assert!(
        alice_lol::law_id::LOL_SEMANTICS_PINS
            .iter()
            .any(|(name, _)| *name == "alice-det-math"),
        "{:?}",
        alice_lol::law_id::LOL_SEMANTICS_PINS
            .iter()
            .map(|(n, _)| *n)
            .collect::<Vec<_>>()
    );
}

#[test]
fn the_semantics_identifier_is_the_fold_of_its_pins() {
    // ⚠️ 識別子が、それが名乗る振る舞いから drift できないことの検査
    // pin 表を書き換えたら定数も変わる (定数だけ直しても pin と食い違えば fail)
    assert_eq!(
        LOL_SEMANTICS_ID,
        alice_lol::law_id::fold_semantics_pins(alice_lol::law_id::LOL_SEMANTICS_PINS)
    );
}

/// ⚠️⚠️ **この 1 本が「識別子が振る舞いから drift できない」ことの本体**
///
/// `LOL_SEMANTICS_ID` と pin 表の fold を比べるだけでは、**定数 vs 定数**なので
/// 振る舞いを変えても通ってしまう (実測: 判定順序を入れ替える変異が 16 passed のまま
/// 通った) pin の値を**その場で測り直した指紋**と比べて初めて歯になる
#[test]
fn the_audit_order_pin_is_the_measured_behaviour() {
    let (_, pinned) = LOL_SEMANTICS_PINS
        .iter()
        .find(|(name, _)| name.contains("audit verdict order"))
        .expect("判定順序の pin");
    assert_eq!(
        *pinned,
        audit_verdict_order_fingerprint(),
        "判定順序を変えたなら pin を再記録する (module doc の「再記録する順序」)"
    );
}

#[test]
fn the_upstream_arithmetic_pin_is_the_upstream_constant() {
    // 上流の世代が上がったら、こちらの pin も追従しなければならない
    let (_, pinned) = LOL_SEMANTICS_PINS
        .iter()
        .find(|(name, _)| *name == "alice-det-math")
        .expect("上流の pin");
    assert_eq!(*pinned, alice_det_math::SEMANTICS_ID);
}

#[test]
fn the_judgment_order_is_pinned_as_a_semantics() {
    // 監査 Law の「証拠 → 成立範囲 → 期待値」の順は verdict を変えるので、
    // 算術と同じく識別子の一部でなければならない
    assert!(
        alice_lol::law_id::LOL_SEMANTICS_PINS
            .iter()
            .any(|(name, _)| name.contains("audit verdict order")),
        "判定順序の pin が無い"
    );
}

/// 証拠を `0 でない` と読み、成立範囲の重複を残していた世代の判定順序の pin
///
/// 残す理由: 証拠を「有限で 0 より大きい数」、成立範囲を集合と読むように変えた時に、
/// pin と識別子が**本当に動いた**ことを固定するため 指紋の case を足さずに読み方だけ
/// 変えると、旧 4 case はどちらの読み方でも同じ判定を出すので pin が動かず、
/// 判定の変わった Law が同じ識別子を名乗る
const PREVIOUS_AUDIT_ORDER_PIN: [u8; 32] =
    alice_lol::law_id::hex32("49769f9b44d0397b7ace9e2ab5352e97d2f7203f12943efc694be400e7b6dd4d");

/// 上の pin を fold した旧世代の `LOL_SEMANTICS_ID` (理由は同じ)
const PREVIOUS_SEMANTICS_ID: [u8; 32] =
    alice_lol::law_id::hex32("673481121a3ed4ba1efb33c2ca62c8e6d2febded9d47e21e79294148d63f6a6f");

#[test]
fn changing_how_evidence_and_ranges_are_read_moved_the_identifier() {
    let (_, pinned) = LOL_SEMANTICS_PINS
        .iter()
        .find(|(name, _)| name.contains("audit verdict order"))
        .expect("判定順序の pin");
    assert_ne!(
        *pinned, PREVIOUS_AUDIT_ORDER_PIN,
        "判定順序の pin が旧世代のまま"
    );
    assert_ne!(
        LOL_SEMANTICS_ID, PREVIOUS_SEMANTICS_ID,
        "識別子が旧世代のまま"
    );
    // 旧 ID は旧 pin の fold であること (定数の写し間違いで assert_ne が空振りしない歯)
    // 入力の読み方の pin はその世代には無かった
    let previous_pins: Vec<(&str, [u8; 32])> = LOL_SEMANTICS_PINS
        .iter()
        .filter(|(name, _)| {
            !name.contains("input reading")
                && !name.contains("research expressions")
                && !name.contains("audit derivations")
        })
        .map(|&(name, id)| {
            if name.contains("audit verdict order") {
                (name, PREVIOUS_AUDIT_ORDER_PIN)
            } else {
                (name, id)
            }
        })
        .collect();
    assert_eq!(
        alice_lol::law_id::fold_semantics_pins(&previous_pins),
        PREVIOUS_SEMANTICS_ID
    );
}

/// 入力の読み方の pin を足す前の `LOL_SEMANTICS_ID`
///
/// 残す理由: x-input に型を持たせ、型に合わない値を入力全体として読むように変えた時に、
/// 識別子が**本当に動いた**ことを固定するため (読み方は識別子の外にあると、同じ識別子・
/// 同じ request で判定が変わる)
const SEMANTICS_ID_BEFORE_INPUT_READING: [u8; 32] =
    alice_lol::law_id::hex32("bc2befdcd5c0e1e190a145eb3f0e15c5f5698886e946dd6aecf33e406c39b71a");

#[test]
fn the_input_reading_pin_is_the_measured_behaviour() {
    let (_, pinned) = LOL_SEMANTICS_PINS
        .iter()
        .find(|(name, _)| name.contains("input reading"))
        .expect("入力の読み方の pin が無い");
    assert_eq!(*pinned, alice_lol::law_input::input_reading_fingerprint());
}

#[test]
fn typing_the_inputs_moved_the_identifier() {
    assert_ne!(LOL_SEMANTICS_ID, SEMANTICS_ID_BEFORE_INPUT_READING);
    // 旧 ID は入力の読み方の pin を除いた fold であること (写し間違いで空振りしない歯)
    let before: Vec<(&str, [u8; 32])> = LOL_SEMANTICS_PINS
        .iter()
        .filter(|(name, _)| {
            !name.contains("input reading")
                && !name.contains("research expressions")
                && !name.contains("audit derivations")
        })
        .copied()
        .collect();
    assert_eq!(
        alice_lol::law_id::fold_semantics_pins(&before),
        SEMANTICS_ID_BEFORE_INPUT_READING
    );
}

#[test]
fn the_type_of_an_input_is_part_of_the_law() {
    use alice_lol::law_input::InputType;
    let t = |s: &str| InputType::parse(s).expect("type");
    let base = audit("audit a\nevidence builds\n");
    let typed = base.clone().with_input("builds", t("list of text"));
    let other = base.clone().with_input("builds", t("list of number"));
    let rec = base
        .clone()
        .with_input("builds", t("record(a: text, b: optional number)"));
    let rec_spaced = base
        .clone()
        .with_input("builds", t("record( a:text ,b:  optional number )"));
    // 型を足すと動く、型が違えば動く、書き方の空白では動かない
    assert_ne!(
        base.law_id(&LOL_SEMANTICS_ID),
        typed.law_id(&LOL_SEMANTICS_ID)
    );
    assert_ne!(
        typed.law_id(&LOL_SEMANTICS_ID),
        other.law_id(&LOL_SEMANTICS_ID)
    );
    assert_eq!(
        rec.law_id(&LOL_SEMANTICS_ID),
        rec_spaced.law_id(&LOL_SEMANTICS_ID)
    );
}

/// 研究 Law の式に atan2 / min / max を足す前の `LOL_SEMANTICS_ID`
///
/// 残す理由: 式の関数を足した時に識別子が**本当に動いた**ことを固定するため
const SEMANTICS_ID_BEFORE_EXPRESSION_FUNCTIONS: [u8; 32] =
    alice_lol::law_id::hex32("e1b05c475eaa4209264d7e74211baa68f8d5e550359bfa05f812d645af9a6e3c");

#[test]
fn the_expression_functions_pin_is_the_measured_behaviour() {
    let (_, pinned) = LOL_SEMANTICS_PINS
        .iter()
        .find(|(name, _)| name.contains("research expressions"))
        .expect("式の関数の pin が無い");
    assert_eq!(
        *pinned,
        alice_lol::research_law::expression_functions_fingerprint()
    );
}

#[test]
fn adding_expression_functions_moved_the_identifier() {
    assert_ne!(LOL_SEMANTICS_ID, SEMANTICS_ID_BEFORE_EXPRESSION_FUNCTIONS);
    let before: Vec<(&str, [u8; 32])> = LOL_SEMANTICS_PINS
        .iter()
        .filter(|(name, _)| {
            !name.contains("research expressions") && !name.contains("audit derivations")
        })
        .copied()
        .collect();
    assert_eq!(
        alice_lol::law_id::fold_semantics_pins(&before),
        SEMANTICS_ID_BEFORE_EXPRESSION_FUNCTIONS
    );
}

/// 監査の量の導出を式にする前の `LOL_SEMANTICS_ID`
///
/// 残す理由: 導出を LOL が読むようにした時に識別子が**本当に動いた**ことを固定するため
const SEMANTICS_ID_BEFORE_DERIVATIONS: [u8; 32] =
    alice_lol::law_id::hex32("764a737399d4a1eac0e390fed351fe731a8d537a8ed3c5b071b3cc0f1a3568fc");

#[test]
fn the_derivation_pin_is_the_measured_behaviour() {
    let (_, pinned) = LOL_SEMANTICS_PINS
        .iter()
        .find(|(name, _)| name.contains("audit derivations"))
        .expect("導出の pin が無い");
    assert_eq!(*pinned, alice_lol::law_input::derivation_fingerprint());
}

#[test]
fn reading_the_derivations_moved_the_identifier() {
    assert_ne!(LOL_SEMANTICS_ID, SEMANTICS_ID_BEFORE_DERIVATIONS);
    let before: Vec<(&str, [u8; 32])> = LOL_SEMANTICS_PINS
        .iter()
        .filter(|(name, _)| !name.contains("audit derivations"))
        .copied()
        .collect();
    assert_eq!(
        alice_lol::law_id::fold_semantics_pins(&before),
        SEMANTICS_ID_BEFORE_DERIVATIONS
    );
}

#[test]
fn the_derivations_of_a_law_are_part_of_the_law() {
    use alice_lol::law_input::{audit_law_from_file, MetricExpr};
    let text = "x-input b list of record(f: list of text)\nx-metric n = count(b)\nx-at-least n 2\nbegin audit\naudit a\nevidence n\nend audit\n";
    let base = audit_law_from_file(text).expect("law");
    let other_expr =
        audit_law_from_file(&text.replace("count(b)", "distinct(set(b[].f))")).expect("law");
    let other_floor =
        audit_law_from_file(&text.replace("x-at-least n 2", "x-at-least n 3")).expect("law");
    let id = |l: &alice_lol::audit_law::AuditLaw| l.law_id(&LOL_SEMANTICS_ID);
    assert_ne!(id(&base), id(&other_expr));
    assert_ne!(id(&base), id(&other_floor));
    assert_eq!(
        base.metrics()[0].1,
        MetricExpr::parse("count(b)").expect("expr")
    );
}

#[test]
fn the_order_of_declarations_is_not_part_of_the_law_but_the_order_of_clauses_is() {
    use alice_lol::law_input::audit_law_from_file;
    let law = |metrics: &str, clauses: &str| {
        audit_law_from_file(&format!(
            "x-input a list of record(f: list of text, g: optional text)\nx-input k list of text\n{metrics}x-at-least m 2\nx-at-least n 1\nbegin audit\naudit o\n{clauses}end audit\n"
        ))
        .expect("law")
        .law_id(&LOL_SEMANTICS_ID)
    };
    let m = "x-metric m = count(a)\nx-metric n = distinct(a[].g)\n";
    let m_swapped = "x-metric n = distinct(a[].g)\nx-metric m = count(a)\n";
    let c = "evidence m\nexpect n == 1\n";
    let c_swapped = "expect n == 1\nevidence m\n";
    // swapping two x-metric lines (or x-input / x-at-least lines) is the same law
    assert_eq!(law(m, c), law(m_swapped, c));
    let reordered = audit_law_from_file(
        "x-input k list of text\nx-input a list of record(f: list of text, g: optional text)\nx-metric n = distinct(a[].g)\nx-metric m = count(a)\nx-at-least n 1\nx-at-least m 2\nbegin audit\naudit o\nevidence m\nexpect n == 1\nend audit\n",
    )
    .expect("law")
    .law_id(&LOL_SEMANTICS_ID);
    assert_eq!(law(m, c), reordered);
    // swapping two clauses is another law (the clause order decides which failure is reported)
    assert_ne!(law(m, c), law(m, c_swapped));
}
