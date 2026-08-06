//! # hardsurface — メカ / 建築 / パーツ / 道具 / 家具 の構造要素 primitive library
//!
//! ALICE-Bamboo 実プリント検証で確立された formulas を LOL の SdfNode として提供する
//!
//! ## モジュール
//!
//! - [`fastener`] — 締結 6 primitive (ネジ穴 / タップ穴 / 座ぐり / 皿頭沈み / ボルト実体 / ヒートセットインサート穴)
//! - [`joint`] — 組立 6 primitive (片持ち snap-fit / 環状 snap-fit / スロット / T スロット 2020 / アリ継ぎ / ピンヒンジ)
//! - [`reinforcement`] — 補強 6 primitive (リブ / ボス / フィレット / 面取り / ハニカム infill / Gyroid infill)
//! - [`mount`] — 建築/取付 6 primitive (L 字ブラケット / 円形フランジ / ラック / SKADIS peg / 2020 profile / 3030 profile)
//! - [`thin`] — 薄物 3 primitive (shopping cart coin / SKADIS panel / thin plate)、2D polygon + extrude 経路 (Phase A.5.2)
//! - [`pattern_sdf`] — Bamboo Rust generator を SdfNode 直接構築に翻訳した完成 pattern 4 種 (Phase B.1.b、wall_hook / gridfinity_bin / drawer_organizer / shelf_divider)
//!
//! ## 準拠 formulas (ALICE-Bamboo `src/formulas.rs`)
//!
//! | 用途 | 式 | 出典 |
//! |------|-----|-----|
//! | タップ下穴径 | `screw_dia * 0.85 + 2 * accuracy` | Bamboo `PrintParams::tap_hole()` |
//! | ヒートセットインサート下穴径 | `insert_od + 0.2` | Bamboo `heat_insert_hole()` |
//! | クリアランス穴径 (H2D 0.4 nozzle FDM) | `screw_dia + 0.2` | H2D 実測 |
//!
//! ## ALICE-Bamboo 実プリント合格の baseline
//!
//! umbrella 削除後 (2026-08-06、Bamboo commit `6727f3f`) 残った 5 generator
//! (drawer / gridfinity / hook / shelf_divider / skadis) が本 module の骨格となる
//! ハードサーフェス pattern の実プリント合格 baseline

pub mod fastener;
pub mod joint;
pub mod mount;
pub mod pattern_sdf;
pub mod reinforcement;
pub mod skadis_sdf;
pub mod thin;
