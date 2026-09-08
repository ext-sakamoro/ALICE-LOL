//! Text layout — advance + `letter_spacing` + space handling で複数 glyph を横並び配置
//!
//! Phase 1-6 実装 (2026-09-08)
//! 単一行のみ (multi-line / line-break は Phase 1.6.2 で追加予定)
//! pair-specific kerning table (AV/TA/VA/WA 等の詰め) も Phase 1.6.2 で追加予定
//!
//! # 動作規約
//!
//! - **supported char** (A..=Z): [`Glyph::ascii`] で glyph 生成、advance で cursor 進める
//! - **space (' ')**: [`LayoutConfig::space_width`] em advance で skip (SDF に何も追加しない)
//! - **unsupported char** (lowercase / digit / 記号 / 日本語): skip (advance 0、= 詰めて次)
//! - **`letter_spacing`**: 各 glyph 間に [`LayoutConfig::letter_spacing`] em 追加
//!
//! # 使用例
//!
//! ```
//! use alice_lol_font::{MetaFontParams, PenModel};
//! use alice_lol_font::layout::{layout_string_2d, LayoutConfig};
//!
//! let pen = PenModel::from_params(&MetaFontParams::sans_bold());
//! let config = LayoutConfig::default();
//! let node = layout_string_2d("HELLO", &pen, &config).expect("non-empty output");
//! # let _ = node;
//! ```

use crate::glyph::Glyph;
use crate::pen::PenModel;
use alice_sdf::sdf2d::Sdf2dNode;
use alice_sdf::types::SdfNode;

/// Layout config for glyph string rendering
///
/// 各値は em unit (glyph body の `cap_height` ≈ 0.7 em 基準)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutConfig {
    /// 各 glyph 間の追加 spacing (0.0 = tight、default 0.05 em)
    pub letter_spacing: f32,
    /// space char (' ') の advance 幅 (default 0.5 em)
    pub space_width: f32,
    /// Line height in em units (Phase 1.6.2 multi-line 用に予約、現状未使用)
    pub line_height: f32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            letter_spacing: 0.05,
            space_width: 0.5,
            line_height: 1.2,
        }
    }
}

impl LayoutConfig {
    /// Tight layout — `letter_spacing` 0.0 で glyph をぎゅっと詰める
    #[must_use]
    pub const fn tight() -> Self {
        Self {
            letter_spacing: 0.0,
            space_width: 0.4,
            line_height: 1.0,
        }
    }

    /// Loose layout — `letter_spacing` 0.15 で glyph を広めに配置
    #[must_use]
    pub const fn loose() -> Self {
        Self {
            letter_spacing: 0.15,
            space_width: 0.7,
            line_height: 1.5,
        }
    }
}

/// Cursor advance を pure 計算 (SDF 構築なし)
///
/// text 全体を通して累積 advance を返す 未 support char は 0 消費
/// space は `config.space_width` 消費 supported char は `glyph.advance()` + `letter_spacing`
/// (先頭 `glyph` には `letter_spacing` 加算しない)
///
/// # 使用例
///
/// ```
/// use alice_lol_font::layout::{measure_string, LayoutConfig};
/// let width = measure_string("HI", &LayoutConfig::default());
/// // 'H' advance 1.0 + letter_spacing 0.05 + 'I' advance 0.5 = 1.55
/// assert!((width - 1.55).abs() < 1e-4);
/// ```
#[must_use]
pub fn measure_string(text: &str, config: &LayoutConfig) -> f32 {
    let mut cursor = 0.0_f32;
    let mut first = true;
    for ch in text.chars() {
        if ch == ' ' {
            cursor += config.space_width;
            first = true; // space の後は letter_spacing リセット
            continue;
        }
        let Some(glyph) = Glyph::ascii(ch) else {
            continue; // unsupported char は skip
        };
        if !first {
            cursor += config.letter_spacing;
        }
        cursor += glyph.advance();
        first = false;
    }
    cursor
}

/// 2D layout: text → single `Sdf2dNode` (Union fold with x-offset translate)
///
/// 全 char が unsupported / empty text の場合 `None` を返す (empty union)
/// space は SDF に何も追加せず、cursor advance のみ消費
#[must_use]
pub fn layout_string_2d(text: &str, pen: &PenModel, config: &LayoutConfig) -> Option<Sdf2dNode> {
    let mut result: Option<Sdf2dNode> = None;
    let mut cursor = 0.0_f32;
    let mut first = true;

    for ch in text.chars() {
        if ch == ' ' {
            cursor += config.space_width;
            first = true;
            continue;
        }
        let Some(glyph) = Glyph::ascii(ch) else {
            continue;
        };
        if !first {
            cursor += config.letter_spacing;
        }
        let Some(node) = glyph.to_sdf2d(pen) else {
            // Empty glyph (stroke 0) は SDF なし、advance だけ消費
            cursor += glyph.advance();
            first = false;
            continue;
        };
        let placed = node.translate(cursor, 0.0);
        result = Some(match result {
            None => placed,
            Some(acc) => acc.union(placed),
        });
        cursor += glyph.advance();
        first = false;
    }

    result
}

/// 3D layout: text → `SdfNode` with extrusion (`extrude_depth` em along Z)
///
/// 全 char が unsupported / empty text の場合 `None` を返す
/// `Bezier` は 12-sample subdivision ([`Glyph::to_sdf3d`] default) で tessellation される
#[must_use]
pub fn layout_string_3d(
    text: &str,
    pen: &PenModel,
    config: &LayoutConfig,
    extrude_depth: f32,
) -> Option<SdfNode> {
    let mut result: Option<SdfNode> = None;
    let mut cursor = 0.0_f32;
    let mut first = true;

    for ch in text.chars() {
        if ch == ' ' {
            cursor += config.space_width;
            first = true;
            continue;
        }
        let Some(glyph) = Glyph::ascii(ch) else {
            continue;
        };
        if !first {
            cursor += config.letter_spacing;
        }
        let Some(node) = glyph.to_sdf3d(pen, extrude_depth) else {
            cursor += glyph.advance();
            first = false;
            continue;
        };
        let placed = node.translate(cursor, 0.0, 0.0);
        result = Some(match result {
            None => placed,
            Some(acc) => acc.union(placed),
        });
        cursor += glyph.advance();
        first = false;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetaFontParams;

    fn regular_pen() -> PenModel {
        PenModel::from_params(&MetaFontParams::sans_regular())
    }

    #[test]
    fn default_config_values() {
        let c = LayoutConfig::default();
        assert!((c.letter_spacing - 0.05).abs() < 1e-6);
        assert!((c.space_width - 0.5).abs() < 1e-6);
        assert!((c.line_height - 1.2).abs() < 1e-6);
    }

    #[test]
    fn tight_config_has_zero_letter_spacing() {
        let c = LayoutConfig::tight();
        assert!(c.letter_spacing.abs() < 1e-6);
    }

    #[test]
    fn loose_config_wider_than_default() {
        let default = LayoutConfig::default();
        let loose = LayoutConfig::loose();
        assert!(loose.letter_spacing > default.letter_spacing);
        assert!(loose.space_width > default.space_width);
    }

    #[test]
    fn measure_empty_string_is_zero() {
        assert!((measure_string("", &LayoutConfig::default())).abs() < 1e-6);
    }

    #[test]
    fn measure_single_letter_h() {
        let w = measure_string("H", &LayoutConfig::default());
        assert!((w - 1.0).abs() < 1e-4, "H width should be 1.0, got {w}");
    }

    #[test]
    fn measure_two_letters_hi() {
        // H(1.0) + letter_spacing(0.05) + I(0.5) = 1.55
        let w = measure_string("HI", &LayoutConfig::default());
        assert!((w - 1.55).abs() < 1e-4, "HI width should be 1.55, got {w}");
    }

    #[test]
    fn measure_with_space() {
        // H(1.0) + space(0.5) + I(0.5) = 2.0 (space 後は letter_spacing リセット)
        let w = measure_string("H I", &LayoutConfig::default());
        assert!((w - 2.0).abs() < 1e-4, "H I width should be 2.0, got {w}");
    }

    #[test]
    fn measure_skips_unsupported_chars() {
        // "Hi" の 'i' (lowercase) は skip、'H' のみ (advance 1.0)
        let w = measure_string("Hi", &LayoutConfig::default());
        assert!(
            (w - 1.0).abs() < 1e-4,
            "'Hi' width should be 1.0 (i skipped), got {w}"
        );
    }

    #[test]
    fn measure_tight_narrower_than_default() {
        let d = measure_string("HELLO", &LayoutConfig::default());
        let t = measure_string("HELLO", &LayoutConfig::tight());
        assert!(t < d, "tight ({t}) must be narrower than default ({d})");
    }

    #[test]
    fn measure_loose_wider_than_default() {
        let d = measure_string("HELLO", &LayoutConfig::default());
        let l = measure_string("HELLO", &LayoutConfig::loose());
        assert!(l > d, "loose ({l}) must be wider than default ({d})");
    }

    #[test]
    fn layout_2d_empty_returns_none() {
        let pen = regular_pen();
        assert!(layout_string_2d("", &pen, &LayoutConfig::default()).is_none());
    }

    #[test]
    fn layout_2d_all_unsupported_returns_none() {
        let pen = regular_pen();
        assert!(layout_string_2d("abc123", &pen, &LayoutConfig::default()).is_none());
    }

    #[test]
    fn layout_2d_single_letter_not_wrapped_in_union() {
        let pen = regular_pen();
        let node = layout_string_2d("H", &pen, &LayoutConfig::default()).expect("non-empty");
        // 先頭 glyph は cursor=0 で translate されるので Translate variant
        assert!(matches!(node, Sdf2dNode::Translate { .. }));
    }

    #[test]
    fn layout_2d_two_letters_returns_union() {
        let pen = regular_pen();
        let node = layout_string_2d("HI", &pen, &LayoutConfig::default()).expect("non-empty");
        assert!(matches!(node, Sdf2dNode::Union(_, _)));
    }

    #[test]
    fn layout_2d_h_position_at_cursor_0() {
        let pen = regular_pen();
        let node = layout_string_2d("H", &pen, &LayoutConfig::default()).expect("non-empty");
        let d = alice_sdf::sdf2d::eval_2d(&node, [0.5, 0.35]);
        assert!(
            d <= 0.05,
            "H at cursor=0, middle bar interior expected, got d={d}"
        );
    }

    #[test]
    fn layout_2d_i_after_h_at_expected_position() {
        // 'HI' で 'I' の縦棒は cursor = 1.0(H advance) + 0.05(letter_spacing) + 0.25(I 中央)
        // = 1.30、y=0.35 で interior 期待
        let pen = regular_pen();
        let node = layout_string_2d("HI", &pen, &LayoutConfig::default()).expect("non-empty");
        let expected_x = 1.0 + 0.05 + 0.25;
        let d = alice_sdf::sdf2d::eval_2d(&node, [expected_x, 0.35]);
        assert!(
            d <= 0.05,
            "'HI' second letter 'I' center at x={expected_x} should be interior, got d={d}"
        );
    }

    #[test]
    fn layout_2d_space_creates_gap() {
        // 'H I' で space の間 (cursor=1.0..1.5) は SDF に何もない
        let pen = regular_pen();
        let node = layout_string_2d("H I", &pen, &LayoutConfig::default()).expect("non-empty");
        // space の中央 x=1.25 は any letter の外
        let d = alice_sdf::sdf2d::eval_2d(&node, [1.25, 0.35]);
        assert!(d > 0.1, "space region should have no strokes, got d={d}");
    }

    #[test]
    fn layout_2d_skips_lowercase() {
        // 'Hi' の 'i' は skip、'H' のみが出力される (単独 glyph 扱い)
        let pen = regular_pen();
        let node = layout_string_2d("Hi", &pen, &LayoutConfig::default()).expect("non-empty");
        assert!(matches!(node, Sdf2dNode::Translate { .. }));
    }

    #[test]
    fn layout_2d_letter_spacing_affects_position() {
        let pen = regular_pen();
        // tight (spacing 0): 'I' 中心 = 1.0(H) + 0 + 0.25 = 1.25
        let node_tight = layout_string_2d("HI", &pen, &LayoutConfig::tight()).expect("non-empty");
        let d_tight = alice_sdf::sdf2d::eval_2d(&node_tight, [1.25, 0.35]);
        assert!(
            d_tight <= 0.05,
            "tight: I center at x=1.25 interior expected, got d={d_tight}"
        );

        // default (spacing 0.05): 'I' 中心 = 1.30
        let node_default =
            layout_string_2d("HI", &pen, &LayoutConfig::default()).expect("non-empty");
        let d_default = alice_sdf::sdf2d::eval_2d(&node_default, [1.30, 0.35]);
        assert!(
            d_default <= 0.05,
            "default: I center at x=1.30 interior expected, got d={d_default}"
        );
    }

    #[test]
    fn layout_3d_empty_returns_none() {
        let pen = regular_pen();
        assert!(layout_string_3d("", &pen, &LayoutConfig::default(), 0.1).is_none());
    }

    #[test]
    fn layout_3d_multi_letter_returns_union() {
        let pen = regular_pen();
        let node =
            layout_string_3d("HELLO", &pen, &LayoutConfig::default(), 0.1).expect("non-empty");
        assert!(matches!(node, SdfNode::Union { .. }));
    }

    #[test]
    fn layout_3d_h_interior_at_cursor_0() {
        let pen = regular_pen();
        let node = layout_string_3d("H", &pen, &LayoutConfig::default(), 0.1).expect("non-empty");
        let d = alice_sdf::eval(&node, glam::Vec3::new(0.5, 0.35, 0.0));
        assert!(d.is_finite(), "eval must be finite");
        assert!(
            d < 0.1,
            "H middle bar center should be near/inside, got d={d}"
        );
    }

    #[test]
    fn layout_3d_far_from_text_is_exterior() {
        let pen = regular_pen();
        let node =
            layout_string_3d("HELLO", &pen, &LayoutConfig::default(), 0.1).expect("non-empty");
        let d = alice_sdf::eval(&node, glam::Vec3::new(2.5, 5.0, 0.0));
        assert!(
            d > 0.5,
            "far from all glyphs should be well outside, got d={d}"
        );
    }
}
