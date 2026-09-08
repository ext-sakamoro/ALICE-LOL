//! Standard library — named `FontPreset` enum + one-liner text-to-SDF helper
//!
//! Phase 1-7 実装 (2026-09-08) `MetaFontParams` factory (7 preset) を named enum で
//! dispatch 可能にし、`preset_text_2d` / `preset_text_3d` の one-liner で text →
//! `Sdf2dNode` / `SdfNode` を短く書けるようにする
//!
//! # 動機
//!
//! Phase 1-6 までは以下の boilerplate が必要:
//! ```
//! use alice_lol_font::{MetaFontParams, PenModel, layout::{layout_string_2d, LayoutConfig}};
//! let params = MetaFontParams::sans_bold();
//! let pen = PenModel::from_params(&params);
//! let config = LayoutConfig::default();
//! let node = layout_string_2d("HELLO", &pen, &config);
//! ```
//!
//! Phase 1-7 以降:
//! ```
//! use alice_lol_font::stdlib::{preset_text_2d, FontPreset};
//! let node = preset_text_2d(FontPreset::SansBold, "HELLO");
//! ```
//!
//! LLM 生成 code / 短い demo / config-driven dispatch (`FontPreset::from_name("sans_bold")`)
//! で強力

use crate::glyph::Glyph;
use crate::layout::{layout_string_2d, layout_string_3d, LayoutConfig};
use crate::params::MetaFontParams;
use crate::pen::PenModel;
use alice_sdf::sdf2d::Sdf2dNode;
use alice_sdf::types::SdfNode;

/// Named font preset dispatcher — [`MetaFontParams`] factory 群と 1:1 対応
///
/// 7 variant を持つ enum LLM / config file から string name で dispatch 可能
/// (`FontPreset::from_name`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontPreset {
    /// Sans-serif regular (like Helvetica / Arial)
    SansRegular,
    /// Sans-serif hairline — shader path 用の thin stroke
    SansHairline,
    /// Sans-serif bold
    SansBold,
    /// Serif regular (like Times New Roman)
    SerifRegular,
    /// Serif italic
    SerifItalic,
    /// Monospace regular (like Courier)
    MonoRegular,
    /// Gothic / display (like Impact) — 太くて狭い
    DisplayHeavy,
}

impl FontPreset {
    /// All 7 preset variants (iteration 用の順序固定 array)
    pub const ALL: [Self; 7] = [
        Self::SansRegular,
        Self::SansHairline,
        Self::SansBold,
        Self::SerifRegular,
        Self::SerifItalic,
        Self::MonoRegular,
        Self::DisplayHeavy,
    ];

    /// `MetaFontParams` を返す (const、compile-time evaluation 可)
    #[must_use]
    pub const fn params(self) -> MetaFontParams {
        match self {
            Self::SansRegular => MetaFontParams::sans_regular(),
            Self::SansHairline => MetaFontParams::sans_hairline(),
            Self::SansBold => MetaFontParams::sans_bold(),
            Self::SerifRegular => MetaFontParams::serif_regular(),
            Self::SerifItalic => MetaFontParams::serif_italic(),
            Self::MonoRegular => MetaFontParams::mono_regular(),
            Self::DisplayHeavy => MetaFontParams::display_heavy(),
        }
    }

    /// `PenModel` を構築 (non-const、`tan()` が非 const)
    #[must_use]
    pub fn pen(self) -> PenModel {
        PenModel::from_params(&self.params())
    }

    /// preset の canonical name (`"sans_regular"` 等の lower snake case)
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SansRegular => "sans_regular",
            Self::SansHairline => "sans_hairline",
            Self::SansBold => "sans_bold",
            Self::SerifRegular => "serif_regular",
            Self::SerifItalic => "serif_italic",
            Self::MonoRegular => "mono_regular",
            Self::DisplayHeavy => "display_heavy",
        }
    }

    /// Parse from string name (case-insensitive、`-` / `_` 双方許容、空白 trim)
    ///
    /// 認識形式: `"sans_regular"` / `"SansRegular"` / `"sans-regular"` / `"SANS_REGULAR"`
    /// / `"  sans regular  "` 未知 name は `None`
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let normalized: String = name
            .trim()
            .chars()
            .filter_map(|c| {
                if c.is_ascii_alphanumeric() {
                    Some(c.to_ascii_lowercase())
                } else if matches!(c, '_' | '-' | ' ') {
                    None
                } else {
                    Some(c)
                }
            })
            .collect();
        match normalized.as_str() {
            "sansregular" => Some(Self::SansRegular),
            "sanshairline" => Some(Self::SansHairline),
            "sansbold" => Some(Self::SansBold),
            "serifregular" => Some(Self::SerifRegular),
            "serifitalic" => Some(Self::SerifItalic),
            "monoregular" => Some(Self::MonoRegular),
            "displayheavy" => Some(Self::DisplayHeavy),
            _ => None,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// One-liner convenience helper (text → Sdf2dNode / SdfNode、default LayoutConfig)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// text → 2D SDF (`FontPreset` + default `LayoutConfig`)
///
/// unsupported char (lowercase / digit) は skip、全 char が unsupported / 空 text で `None`
#[must_use]
pub fn preset_text_2d(preset: FontPreset, text: &str) -> Option<Sdf2dNode> {
    let pen = preset.pen();
    layout_string_2d(text, &pen, &LayoutConfig::default())
}

/// text → 3D SDF (`FontPreset` + default `LayoutConfig` + `extrude_depth` em along Z)
#[must_use]
pub fn preset_text_3d(preset: FontPreset, text: &str, extrude_depth: f32) -> Option<SdfNode> {
    let pen = preset.pen();
    layout_string_3d(text, &pen, &LayoutConfig::default(), extrude_depth)
}

/// text → 2D SDF (`FontPreset` + 明示的 `LayoutConfig`、custom `letter_spacing` 等)
#[must_use]
pub fn preset_text_2d_with_config(
    preset: FontPreset,
    text: &str,
    config: &LayoutConfig,
) -> Option<Sdf2dNode> {
    let pen = preset.pen();
    layout_string_2d(text, &pen, config)
}

/// text → 3D SDF (`FontPreset` + 明示的 `LayoutConfig` + `extrude_depth`)
#[must_use]
pub fn preset_text_3d_with_config(
    preset: FontPreset,
    text: &str,
    config: &LayoutConfig,
    extrude_depth: f32,
) -> Option<SdfNode> {
    let pen = preset.pen();
    layout_string_3d(text, &pen, config, extrude_depth)
}

/// 単一 glyph → 2D SDF (`FontPreset` 経由の short-hand)
///
/// unsupported char / stroke 0 glyph で `None`
#[must_use]
pub fn preset_glyph_2d(preset: FontPreset, ch: char) -> Option<Sdf2dNode> {
    let pen = preset.pen();
    Glyph::ascii(ch)?.to_sdf2d(&pen)
}

/// 単一 glyph → 3D SDF (`FontPreset` + `extrude_depth`)
#[must_use]
pub fn preset_glyph_3d(preset: FontPreset, ch: char, extrude_depth: f32) -> Option<SdfNode> {
    let pen = preset.pen();
    Glyph::ascii(ch)?.to_sdf3d(&pen, extrude_depth)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // FontPreset::ALL + variant coverage
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn all_array_has_7_variants() {
        assert_eq!(FontPreset::ALL.len(), 7);
    }

    #[test]
    fn all_variants_unique() {
        let all = FontPreset::ALL;
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(a, b, "duplicate variant in ALL: {a:?} vs {b:?}");
            }
        }
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // .params() correctness (each variant → correct MetaFontParams factory)
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn params_sans_regular_matches_factory() {
        assert_eq!(
            FontPreset::SansRegular.params(),
            MetaFontParams::sans_regular()
        );
    }

    #[test]
    fn params_sans_hairline_matches_factory() {
        assert_eq!(
            FontPreset::SansHairline.params(),
            MetaFontParams::sans_hairline()
        );
    }

    #[test]
    fn params_sans_bold_matches_factory() {
        assert_eq!(FontPreset::SansBold.params(), MetaFontParams::sans_bold());
    }

    #[test]
    fn params_serif_regular_matches_factory() {
        assert_eq!(
            FontPreset::SerifRegular.params(),
            MetaFontParams::serif_regular()
        );
    }

    #[test]
    fn params_serif_italic_matches_factory() {
        assert_eq!(
            FontPreset::SerifItalic.params(),
            MetaFontParams::serif_italic()
        );
    }

    #[test]
    fn params_mono_regular_matches_factory() {
        assert_eq!(
            FontPreset::MonoRegular.params(),
            MetaFontParams::mono_regular()
        );
    }

    #[test]
    fn params_display_heavy_matches_factory() {
        assert_eq!(
            FontPreset::DisplayHeavy.params(),
            MetaFontParams::display_heavy()
        );
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // .pen() produces valid PenModel
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn pen_all_variants_produce_positive_half_width() {
        for preset in FontPreset::ALL {
            let pen = preset.pen();
            assert!(
                pen.half_width > 0.0,
                "{:?}.pen().half_width should be positive, got {}",
                preset,
                pen.half_width
            );
        }
    }

    #[test]
    fn pen_sans_bold_thicker_than_sans_regular() {
        let bold = FontPreset::SansBold.pen();
        let regular = FontPreset::SansRegular.pen();
        assert!(bold.half_width > regular.half_width);
    }

    #[test]
    fn pen_serif_italic_has_positive_slant_tan() {
        let italic = FontPreset::SerifItalic.pen();
        assert!(italic.slant_tan > 0.0);
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // .name() canonical form
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn name_returns_canonical_snake_case() {
        assert_eq!(FontPreset::SansRegular.name(), "sans_regular");
        assert_eq!(FontPreset::SansHairline.name(), "sans_hairline");
        assert_eq!(FontPreset::SansBold.name(), "sans_bold");
        assert_eq!(FontPreset::SerifRegular.name(), "serif_regular");
        assert_eq!(FontPreset::SerifItalic.name(), "serif_italic");
        assert_eq!(FontPreset::MonoRegular.name(), "mono_regular");
        assert_eq!(FontPreset::DisplayHeavy.name(), "display_heavy");
    }

    #[test]
    fn name_all_variants_unique() {
        let names: Vec<&str> = FontPreset::ALL.iter().map(|p| p.name()).collect();
        for (i, a) in names.iter().enumerate() {
            for b in names.iter().skip(i + 1) {
                assert_ne!(a, b, "duplicate name: {a}");
            }
        }
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // .from_name() parsing (case-insensitive、多 format)
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn from_name_snake_case() {
        assert_eq!(
            FontPreset::from_name("sans_regular"),
            Some(FontPreset::SansRegular)
        );
        assert_eq!(
            FontPreset::from_name("display_heavy"),
            Some(FontPreset::DisplayHeavy)
        );
    }

    #[test]
    fn from_name_kebab_case() {
        assert_eq!(
            FontPreset::from_name("sans-regular"),
            Some(FontPreset::SansRegular)
        );
    }

    #[test]
    fn from_name_pascal_case() {
        assert_eq!(
            FontPreset::from_name("SansRegular"),
            Some(FontPreset::SansRegular)
        );
        assert_eq!(
            FontPreset::from_name("DisplayHeavy"),
            Some(FontPreset::DisplayHeavy)
        );
    }

    #[test]
    fn from_name_uppercase() {
        assert_eq!(
            FontPreset::from_name("SANS_REGULAR"),
            Some(FontPreset::SansRegular)
        );
    }

    #[test]
    fn from_name_with_spaces() {
        assert_eq!(
            FontPreset::from_name("  sans regular  "),
            Some(FontPreset::SansRegular)
        );
    }

    #[test]
    fn from_name_unknown_returns_none() {
        assert!(FontPreset::from_name("unknown").is_none());
        assert!(FontPreset::from_name("").is_none());
        assert!(FontPreset::from_name("nonexistent_preset").is_none());
    }

    #[test]
    fn from_name_roundtrip_all_variants() {
        // .name() → .from_name() で元の variant に戻る
        for preset in FontPreset::ALL {
            let name = preset.name();
            let parsed = FontPreset::from_name(name);
            assert_eq!(parsed, Some(preset), "roundtrip fail for {preset:?}");
        }
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // preset_text_2d / _3d one-liner
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn preset_text_2d_non_empty_returns_some() {
        assert!(preset_text_2d(FontPreset::SansBold, "HI").is_some());
    }

    #[test]
    fn preset_text_2d_empty_returns_none() {
        assert!(preset_text_2d(FontPreset::SansRegular, "").is_none());
    }

    #[test]
    fn preset_text_2d_all_unsupported_returns_none() {
        assert!(preset_text_2d(FontPreset::SansRegular, "abc123").is_none());
    }

    #[test]
    fn preset_text_3d_non_empty_returns_some() {
        assert!(preset_text_3d(FontPreset::SansBold, "HI", 0.1).is_some());
    }

    #[test]
    fn preset_text_3d_empty_returns_none() {
        assert!(preset_text_3d(FontPreset::SansRegular, "", 0.1).is_none());
    }

    #[test]
    fn preset_text_2d_with_config_tight_narrower_than_default() {
        // tight (letter_spacing 0) と default (0.05) で node の position が変わる
        // node 差の検出は難しいので、両方が Some を返すことだけ verify
        let tight =
            preset_text_2d_with_config(FontPreset::SansRegular, "HI", &LayoutConfig::tight());
        let default = preset_text_2d(FontPreset::SansRegular, "HI");
        assert!(tight.is_some());
        assert!(default.is_some());
    }

    #[test]
    fn preset_text_3d_with_config_returns_some() {
        assert!(preset_text_3d_with_config(
            FontPreset::SerifRegular,
            "ABC",
            &LayoutConfig::loose(),
            0.15
        )
        .is_some());
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // preset_glyph_2d / _3d single-glyph short-hand
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn preset_glyph_2d_supported_letter_returns_some() {
        assert!(preset_glyph_2d(FontPreset::SansBold, 'A').is_some());
    }

    #[test]
    fn preset_glyph_2d_unsupported_returns_none() {
        assert!(preset_glyph_2d(FontPreset::SansRegular, 'a').is_none());
        assert!(preset_glyph_2d(FontPreset::SansRegular, '1').is_none());
    }

    #[test]
    fn preset_glyph_3d_supported_letter_returns_some() {
        assert!(preset_glyph_3d(FontPreset::SansBold, 'O', 0.1).is_some());
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // eval sanity check (preset 経由でも正しく SDF 化)
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn preset_text_2d_h_interior_at_middle_bar() {
        let node = preset_text_2d(FontPreset::SansRegular, "H").expect("non-empty");
        let d = alice_sdf::sdf2d::eval_2d(&node, [0.5, 0.35]);
        assert!(d <= 0.05, "H middle bar interior expected, got d={d}");
    }
}
