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
//! - [`thin_sdf`] — 薄物 SDF primitive (shopping cart coin 単純 Cylinder、Phase 3''.2、Dual Contouring 経路推奨、旧 `thin` polygon 経路は Phase 4 で削除済)
//! - [`skadis_sdf`] — SKADIS panel 純 SDF (Phase 3''、Dual Contouring 経路推奨)
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
pub mod thin_sdf;

use alice_sdf::SdfNode;
use std::sync::Arc;

/// N 個の SdfNode を **balanced binary tree** で Union fold する
///
/// 線形左入れ子 fold (`Union(Union(...(Union(a,b),c)...))`, depth O(n)) を使うと
/// grid 系 primitive (skadis_panel 98 holes 等) で eval() recursion が
/// test thread の 2 MB stack を超過する (2026-08-07 CI 事故で発覚)
///
/// 本 helper は隣接ペアを Union で潰しながら fold し、depth O(log n) にする
/// 数値挙動は不変 (Union は結合律)、eval recursion depth のみ改善
///
/// # 例
///
/// - 98 nodes → linear fold: depth 98 (overflow risk)
/// - 98 nodes → balanced fold: depth ceil(log2(98)) = 7 (safe)
#[must_use]
pub fn balanced_union_fold(mut nodes: Vec<SdfNode>) -> Option<SdfNode> {
    if nodes.is_empty() {
        return None;
    }
    while nodes.len() > 1 {
        let mut next: Vec<SdfNode> = Vec::with_capacity(nodes.len().div_ceil(2));
        let mut iter = nodes.into_iter();
        while let Some(a) = iter.next() {
            match iter.next() {
                Some(b) => next.push(SdfNode::Union {
                    a: Arc::new(a),
                    b: Arc::new(b),
                }),
                None => next.push(a),
            }
        }
        nodes = next;
    }
    nodes.into_iter().next()
}

#[cfg(test)]
mod balanced_fold_tests {
    use super::*;

    #[test]
    fn empty_returns_none() {
        assert!(balanced_union_fold(Vec::new()).is_none());
    }

    #[test]
    fn single_returns_self() {
        let n = SdfNode::Sphere { radius: 1.0 };
        let out = balanced_union_fold(vec![n.clone()]).unwrap();
        assert!(matches!(out, SdfNode::Sphere { .. }));
    }

    #[test]
    fn multi_returns_union_tree() {
        let nodes: Vec<_> = (0..8).map(|_| SdfNode::Sphere { radius: 0.1 }).collect();
        let out = balanced_union_fold(nodes).unwrap();
        assert!(matches!(out, SdfNode::Union { .. }));
    }

    #[test]
    fn depth_is_logarithmic() {
        // 100 nodes should produce depth ~7 (ceil log2 100)
        fn depth(n: &SdfNode) -> usize {
            match n {
                SdfNode::Union { a, b } => 1 + depth(a).max(depth(b)),
                _ => 1,
            }
        }
        let nodes: Vec<_> = (0..100).map(|_| SdfNode::Sphere { radius: 0.1 }).collect();
        let out = balanced_union_fold(nodes).unwrap();
        let d = depth(&out);
        assert!(d <= 8, "100 nodes → depth {} (期待: ≤ 8)", d);
    }
}
