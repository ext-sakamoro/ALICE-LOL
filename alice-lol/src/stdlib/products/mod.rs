//! # Products — 生活雑貨 / 家庭用 canonical primitive
//!
//! text-to-print β release で判明した「3B LLM は複合形状 (マグカップ / 花瓶 /
//! 椅子) を primitive 組合せに分解できない」問題への対応として、canonical な
//! 生活雑貨形状を LOL DSL の 1 primitive として提供する
//!
//! LLM は `mug(50, 100)` の 1 line を emit するだけで済み、内部で正しい
//! subtract + torus 組合せを展開する 3B model の semantic 分解能力に依存
//! しない robust な生成経路
//!
//! 詳細: [[feedback_llm_3b_complex_shape_hallucination]] (2026-08-23 事案)
//!
//! ## primitive 一覧
//!
//! | primitive | 引数 | 用途 |
//! |--|--|--|
//! | [`mug_sdf`] | `dia`, `height` | マグカップ (円筒 + 内側くぼみ + torus 取手) |

pub mod mug;

pub use mug::mug_sdf;
