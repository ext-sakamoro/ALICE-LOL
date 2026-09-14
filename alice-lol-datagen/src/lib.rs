//! alice-lol-datagen — 合成 (caption, LOL) pair generator (Track C1)
//!
//! LOL は実行可能なので、教師 data を人手ゼロで量産できる 本 crate は
//! **template family** ごとに parametric generator を持ち、同じ parameter から
//! LOL text (正規形 = `parse → emit`) / 英日 caption / oracle 点 (内外判定) を
//! 同時に出す random tree に後付けで caption を書くと嘘が混じるため、caption は
//! 常に parameter から決定論的に組み立てる
//!
//! # family と狙う fail 型 (`llm_bench` baseline 2026-09-14 の 4 型)
//!
//! | family | fail 型 |
//! |---|---|
//! | [`families::primitive_placed`] | translate 欠落 / half 引数 (caption は全寸、LOL は half) |
//! | [`families::stacked`] | translate + 合成省略 (Y 軸に積む) |
//! | [`families::attachment`] | 合成省略 (本体 + 取手 / 脚 / subtract) |
//! | [`families::plate_holes`] | `subtract` / `polar_repeat` / `repeat_finite` |
//! | [`families::transformed`] | rotate / scale |
//! | [`families::intent_program`] | Intent 構造 (seq / par、entities、id) |
//! | [`families::random_tree`] | 語彙カバレッジ (grammar bucket から生成、caption は構造列挙) |
//! | [`families::product_shortcut`] | product shortcut (`pen_cup(50,100)` 等) をそのまま学習 target に (`lol` は shortcut、`lol_canonical` は展開形) |
//!
//! # 自己検証 ([`Sample::verify`])
//!
//! 全 sample について `parse_program` 成功 / `to_lol` 冪等 / oracle 点が `eval` で
//! 実際に in/out であることを確認し、矛盾があれば [`VerifyError`] で捨てる
//! (`grammar-check` feature 時は LLM grammar `Fsm::accepts_str` も)
//!
//! # 決定論
//!
//! [`Rng`] は xorshift64\* (外部 dep なし)、同じ seed で同じ JSONL

#![forbid(unsafe_code)]
// 寸法 / 座標 / 乱数を f32 ↔ 整数で行き来する data 生成 code のため、
// 精度・符号系の cast lint と mul_add 提案は crate 単位で抑止 (値域は各 family が
// 5〜300 mm 級に制限しており、精度損失は起きない)
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::suboptimal_flops
)]

pub mod caption;
pub mod families;
pub mod rng;
pub mod sample;

pub use families::{Family, ALL_FAMILIES};
pub use rng::Rng;
pub use sample::{OraclePoint, Sample, Side, VerifyError};

/// `n` 個の sample を family を round-robin で回して生成し、自己検証に通ったものだけ返す
///
/// 戻り値の 2 番目は検証で捨てた件数 (`family` 名, エラー) 生成側の bug 検知用に
/// 呼び出し側が集計して報告する
#[must_use]
pub fn generate(
    families: &[Family],
    n: usize,
    seed: u64,
) -> (Vec<Sample>, Vec<(String, VerifyError)>) {
    let mut rng = Rng::new(seed);
    let mut out = Vec::with_capacity(n);
    let mut rejected = Vec::new();
    if families.is_empty() {
        return (out, rejected);
    }
    let mut i = 0usize;
    while out.len() < n {
        let fam = families[i % families.len()];
        i += 1;
        let sample_seed = rng.next_u64();
        let mut s = fam.generate(&mut Rng::new(sample_seed));
        s.seed = sample_seed;
        s.id = format!("{}-{sample_seed:016x}", fam.name());
        match s.verify() {
            Ok(()) => out.push(s),
            Err(e) => rejected.push((fam.name().to_string(), e)),
        }
        // 全 family が壊れている時の無限 loop 防止
        if rejected.len() > n.saturating_mul(4).max(64) {
            break;
        }
    }
    (out, rejected)
}
