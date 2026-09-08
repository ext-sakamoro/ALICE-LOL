//! STL / 3MF export — text / glyph → `SdfNode` → marching cubes mesh → file
//!
//! Phase 1-9 実装 (2026-09-08、`#[cfg(feature = "stl-export")]` gated)
//!
//! # 依存
//!
//! `alice_lol::print_export` を経由 (`node_to_stl` / `node_to_3mf` /
//! [`PrintConfig`]) alice-lol は MIT/Apache dual license、cascade なし
//!
//! # AABB 自動計算
//!
//! text の [`measure_string`] で計算した width と `extrude_depth` から AABB を
//! 自動決定 padding は [`DEFAULT_PADDING`] em em → mm 変換は
//! [`DEFAULT_MM_SCALE`] (1 em = 10mm)
//!
//! # 使用例
//!
//! ```ignore
//! use alice_lol_font::stl_export::{preset_text_to_stl};
//! use alice_lol_font::FontPreset;
//!
//! let stats = preset_text_to_stl(
//!     FontPreset::SansBold,
//!     "HELLO",
//!     0.1,
//!     "/tmp/hello.stl",
//! )?;
//! println!("Wrote {} tri to {}", stats.triangle_count, stats.path);
//! # Ok::<(), alice_lol_font::stl_export::ExportError>(())
//! ```

use crate::glyph::Glyph;
use crate::layout::{layout_string_3d, measure_string, LayoutConfig};
use crate::pen::PenModel;
use crate::stdlib::FontPreset;
use alice_lol::print_export::{node_to_3mf, node_to_stl};
use glam::Vec3;
use std::path::Path;

pub use alice_lol::print_export::{ExportError, ExportStats, PrintConfig};

/// Default marching cubes resolution (128 = 標準品質、`resolution=64` はプレビュー、
/// 256 は高品質、512 は超高品質)
pub const DEFAULT_RESOLUTION: usize = 128;

/// Default `world_to_mm` scale (1 em = 10mm、font size 10mm cap-height 相当)
pub const DEFAULT_MM_SCALE: f32 = 10.0;

/// Default AABB padding around text bbox in em units (0.2 em ≈ pen thickness × 3)
pub const DEFAULT_PADDING: f32 = 0.2;

/// text の width + `extrude_depth` から AABB を自動計算した `PrintConfig`
///
/// x ∈ `[-padding, text_width + padding]` (text は cursor=0 起点で右に伸びる)
/// y ∈ `[-padding - descender, ascender + padding]` (em: baseline y=0、cap y≈0.7)
/// z ∈ `[-hd - padding, hd + padding]` (`hd = extrude_depth / 2`)
#[must_use]
pub fn auto_bounds(text_width: f32, extrude_depth: f32) -> PrintConfig {
    let pad = DEFAULT_PADDING;
    let hd = extrude_depth * 0.5;
    // descender ≈ 0.22 em、ascender ≈ 1.0 em (glyph coord 上限)
    // safety margin として y range を [-0.3, 1.1] に、padding 追加
    PrintConfig {
        resolution: DEFAULT_RESOLUTION,
        bounds_min: Vec3::new(-pad, -0.3 - pad, -hd - pad),
        bounds_max: Vec3::new(text_width + pad, 1.1 + pad, hd + pad),
        scale_mm: DEFAULT_MM_SCALE,
    }
}

/// glyph 単体の AABB を自動計算した `PrintConfig`
#[must_use]
pub fn auto_bounds_glyph(advance: f32, extrude_depth: f32) -> PrintConfig {
    auto_bounds(advance, extrude_depth)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// text → STL / 3MF (full control)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// text → STL file (custom pen + layout config + resolution)
///
/// AABB は text width から auto 計算 empty text / all-unsupported で `EmptyMesh` エラー
///
/// # Errors
///
/// - [`ExportError::EmptyMesh`]: 全 char が unsupported / empty text / stroke 0 glyph のみ
/// - [`ExportError::Io`]: STL file 書き込み失敗
pub fn text_to_stl(
    text: &str,
    pen: &PenModel,
    layout_config: &LayoutConfig,
    extrude_depth: f32,
    path: impl AsRef<Path>,
    resolution: usize,
) -> Result<ExportStats, ExportError> {
    let node =
        layout_string_3d(text, pen, layout_config, extrude_depth).ok_or(ExportError::EmptyMesh)?;
    let width = measure_string(text, layout_config);
    let mut config = auto_bounds(width, extrude_depth);
    config.resolution = resolution;
    node_to_stl(&node, path, &config)
}

/// text → plain 3MF file (`Bambu` `MakerWorld` 対応外の plain 3MF)
///
/// `MakerWorld` 対応 3MF は `alice_bamboo::bambu_3mf::export_bambu_3mf` 経由 (本 crate 外)
///
/// # Errors
///
/// 同 [`text_to_stl`]
pub fn text_to_3mf(
    text: &str,
    pen: &PenModel,
    layout_config: &LayoutConfig,
    extrude_depth: f32,
    path: impl AsRef<Path>,
    resolution: usize,
) -> Result<ExportStats, ExportError> {
    let node =
        layout_string_3d(text, pen, layout_config, extrude_depth).ok_or(ExportError::EmptyMesh)?;
    let width = measure_string(text, layout_config);
    let mut config = auto_bounds(width, extrude_depth);
    config.resolution = resolution;
    node_to_3mf(&node, path, &config)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// glyph → STL / 3MF (single glyph)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// glyph 単体 → STL file
///
/// # Errors
///
/// - [`ExportError::EmptyMesh`]: stroke 0 glyph (space 等)
/// - [`ExportError::Io`]: STL file 書き込み失敗
pub fn glyph_to_stl(
    glyph: &Glyph,
    pen: &PenModel,
    extrude_depth: f32,
    path: impl AsRef<Path>,
    resolution: usize,
) -> Result<ExportStats, ExportError> {
    let node = glyph
        .to_sdf3d(pen, extrude_depth)
        .ok_or(ExportError::EmptyMesh)?;
    let mut config = auto_bounds_glyph(glyph.advance(), extrude_depth);
    config.resolution = resolution;
    node_to_stl(&node, path, &config)
}

/// glyph 単体 → 3MF file
///
/// # Errors
///
/// 同 [`glyph_to_stl`]
pub fn glyph_to_3mf(
    glyph: &Glyph,
    pen: &PenModel,
    extrude_depth: f32,
    path: impl AsRef<Path>,
    resolution: usize,
) -> Result<ExportStats, ExportError> {
    let node = glyph
        .to_sdf3d(pen, extrude_depth)
        .ok_or(ExportError::EmptyMesh)?;
    let mut config = auto_bounds_glyph(glyph.advance(), extrude_depth);
    config.resolution = resolution;
    node_to_3mf(&node, path, &config)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// One-liner preset version (stdlib と統合、FontPreset + default LayoutConfig)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// One-liner: `FontPreset` + default `LayoutConfig` + default resolution で text → STL
///
/// # Errors
///
/// 同 [`text_to_stl`]
pub fn preset_text_to_stl(
    preset: FontPreset,
    text: &str,
    extrude_depth: f32,
    path: impl AsRef<Path>,
) -> Result<ExportStats, ExportError> {
    text_to_stl(
        text,
        &preset.pen(),
        &LayoutConfig::default(),
        extrude_depth,
        path,
        DEFAULT_RESOLUTION,
    )
}

/// One-liner: `FontPreset` + default `LayoutConfig` + default resolution で text → 3MF
///
/// # Errors
///
/// 同 [`text_to_3mf`]
pub fn preset_text_to_3mf(
    preset: FontPreset,
    text: &str,
    extrude_depth: f32,
    path: impl AsRef<Path>,
) -> Result<ExportStats, ExportError> {
    text_to_3mf(
        text,
        &preset.pen(),
        &LayoutConfig::default(),
        extrude_depth,
        path,
        DEFAULT_RESOLUTION,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetaFontParams;
    use std::env;

    fn tmp_path(name: &str) -> std::path::PathBuf {
        env::temp_dir().join(format!("alice_lol_font_test_{name}"))
    }

    fn regular_pen() -> PenModel {
        PenModel::from_params(&MetaFontParams::sans_regular())
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // auto_bounds
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn auto_bounds_uses_default_resolution() {
        let c = auto_bounds(2.0, 0.1);
        assert_eq!(c.resolution, DEFAULT_RESOLUTION);
    }

    #[test]
    fn auto_bounds_x_covers_text_width_with_padding() {
        let c = auto_bounds(3.0, 0.1);
        assert!(
            c.bounds_min.x < 0.0,
            "bounds_min.x should include left padding"
        );
        assert!(
            c.bounds_max.x > 3.0,
            "bounds_max.x ({}) should exceed text_width 3.0",
            c.bounds_max.x
        );
    }

    #[test]
    fn auto_bounds_z_matches_extrude_depth() {
        let c = auto_bounds(1.0, 0.2);
        // half_depth = 0.1、padding = 0.2 → z range = [-0.3, 0.3]
        assert!(c.bounds_min.z < -0.1);
        assert!(c.bounds_max.z > 0.1);
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // text_to_stl / text_to_3mf
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn text_to_stl_empty_returns_empty_mesh_error() {
        let path = tmp_path("empty.stl");
        let result = text_to_stl("", &regular_pen(), &LayoutConfig::default(), 0.1, &path, 64);
        assert!(matches!(result, Err(ExportError::EmptyMesh)));
    }

    #[test]
    fn text_to_stl_writes_file_with_non_zero_stats() {
        let path = tmp_path("hello.stl");
        let stats = text_to_stl(
            "HI",
            &regular_pen(),
            &LayoutConfig::default(),
            0.1,
            &path,
            64, // preview resolution for fast test
        )
        .expect("STL write should succeed");
        assert!(stats.triangle_count > 0, "should produce triangles");
        assert!(stats.vertex_count > 0);
        assert!(path.exists(), "STL file should be written");
        let size = std::fs::metadata(&path).unwrap().len();
        assert!(size > 84, "STL file should have header + data (> 84 bytes)");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn text_to_3mf_writes_file() {
        let path = tmp_path("hello.3mf");
        let stats = text_to_3mf(
            "H",
            &regular_pen(),
            &LayoutConfig::default(),
            0.1,
            &path,
            64,
        )
        .expect("3MF write should succeed");
        assert!(stats.triangle_count > 0);
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // glyph_to_stl / glyph_to_3mf
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn glyph_to_stl_writes_file() {
        let g = Glyph::ascii('A').expect("A supported");
        let path = tmp_path("a.stl");
        let stats = glyph_to_stl(&g, &regular_pen(), 0.1, &path, 64)
            .expect("glyph STL write should succeed");
        assert!(stats.triangle_count > 0);
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn glyph_to_3mf_writes_file() {
        let g = Glyph::ascii('O').expect("O supported");
        let path = tmp_path("o.3mf");
        let stats = glyph_to_3mf(&g, &regular_pen(), 0.1, &path, 64)
            .expect("glyph 3MF write should succeed");
        assert!(stats.triangle_count > 0);
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // preset_text_to_stl / preset_text_to_3mf one-liner
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn preset_text_to_stl_one_liner_writes_file() {
        let path = tmp_path("preset_hello.stl");
        // Note: DEFAULT_RESOLUTION=128 is slow for test、direct text_to_stl with 64 is preferred
        // But this test verifies the one-liner API works (does not re-verify quality)
        // Use single 'A' to minimize compute
        let stats = preset_text_to_stl(FontPreset::SansBold, "A", 0.1, &path)
            .expect("preset STL write should succeed");
        assert!(stats.triangle_count > 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn preset_text_to_3mf_one_liner_writes_file() {
        let path = tmp_path("preset_hello.3mf");
        let stats = preset_text_to_3mf(FontPreset::SansRegular, "I", 0.1, &path)
            .expect("preset 3MF write should succeed");
        assert!(stats.triangle_count > 0);
        let _ = std::fs::remove_file(&path);
    }
}
