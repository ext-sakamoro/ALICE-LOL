//! # Standard Library — LOL 標準パターンライブラリ
//!
//! LOL DSL の 71 primitive を組み合わせて構築した、実プリント検証済みの
//! ハードサーフェス pattern を提供する
//!
//! - [`hardsurface`] — メカ / 建築 / パーツ / 道具 / 家具 の構造要素
//!   (Phase A.1: fastener 締結 6 primitive)
//!
//! 各 pattern は既存の [`alice_sdf::SdfNode`] variant を Rust helper 関数で組み立てる
//! LOL DSL の syntax / proc_macro / GBNF `lol.gbnf` は無変更、既存 SdfNode → GLSL/WGSL/HLSL
//! transpile pipeline がそのまま流用できる

pub mod hardsurface;
pub mod pattern;
