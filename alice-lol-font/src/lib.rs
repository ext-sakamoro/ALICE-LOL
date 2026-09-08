//! # alice-lol-font
//!
//! Font template for `alice_lol` DSL — parametric glyph + Bezier stroke → `SdfNode`
//!
//! `alice-lol-humanoid` と同型の sibling crate として、Font 領域の primitive
//! (`MetaFontParams`、`Glyph`、`Stroke`、layout) を `alice_lol::lol!` マクロから
//! `{expr}` capture で自然に注入できる Rust API として提供する
//!
//! # 三相原理での位置付け
//!
//! Phase 2 Law (parametric font descriptor 10 axis) → `SdfNode` 経由で
//! `alice-lol` の既存 CSG pipeline に合流 `alice-sdf::Sdf2dNode` の
//! `Bezier` / `Line` / `Rect` primitive のみを使い、Foundry 経由 (AGPL)
//! を default から除外することで MIT/Apache pure を維持する
//!
//! # Feature gates
//!
//! - `parametric` (default): 独自 stroke → `Sdf2dNode` 実装、MIT/Apache pure
//! - `foundry` (opt-in): `ALICE-Foundry` 経由で `BIZ UDPGothic` 3344 CJK outline
//!   等が使えるが、**下流全体が AGPL-3.0-or-later に cascade** する
//!
//! # Quick start
//!
//! ```
//! use alice_lol_font::MetaFontParams;
//!
//! let params = MetaFontParams::sans_bold();
//! assert!(params.weight > 0.5);
//! ```
//!
//! # Roadmap (Phase 1.x)
//!
//! - **1.1 (本 crate 初回)**: `MetaFontParams` 10 axis + 8 preset + scaffolding
//! - 1.2: `PenModel` + Bezier stroke sampling
//! - 1.3: `Stroke` primitive + `Sdf2dNode` 変換
//! - 1.4: ASCII 26 upper の parametric definition (MVP)
//! - 1.5: `Glyph` struct + `to_sdf2d()` / `to_sdf3d(extrude)` helper
//! - 1.6: basic layout (advance + kerning)
//! - 1.7: examples + integration tests
//! - 1.8 (opt-in): `foundry` bridge via `alice-foundry::lol_bridge`

#![forbid(unsafe_code)]
#![warn(clippy::pedantic, clippy::nursery)]

pub mod ascii;
#[cfg(feature = "foundry")]
pub mod foundry;
pub mod glyph;
pub mod layout;
pub mod params;
pub mod pen;
pub mod stdlib;
#[cfg(feature = "stl-export")]
pub mod stl_export;
pub mod stroke;

pub use ascii::ascii_uppercase;
#[cfg(feature = "foundry")]
pub use foundry::{
    char_count as foundry_char_count, cjk_glyph, cjk_glyph_3d, cjk_string, cjk_string_3d,
    supports as foundry_supports, FoundryWeight,
};
pub use glyph::{Glyph, DEFAULT_BEZIER_SAMPLES};
pub use layout::{layout_string_2d, layout_string_3d, measure_string, LayoutConfig};
pub use params::MetaFontParams;
pub use pen::PenModel;
pub use stdlib::{
    preset_glyph_2d, preset_glyph_3d, preset_text_2d, preset_text_2d_with_config, preset_text_3d,
    preset_text_3d_with_config, FontPreset,
};
#[cfg(feature = "stl-export")]
pub use stl_export::{
    glyph_to_3mf, glyph_to_stl, preset_text_to_3mf, preset_text_to_stl, text_to_3mf, text_to_stl,
};
pub use stroke::{cubic_bezier_at, strokes_to_sdf2d, Stroke, StrokeWeight};
