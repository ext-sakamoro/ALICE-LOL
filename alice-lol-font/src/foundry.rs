//! Foundry bridge — `BIZ UDPGothic` 3344 CJK outline data 経由の CJK / 全 ASCII 対応
//!
//! Phase 1-8 実装 (2026-09-08、`#[cfg(feature = "foundry")]` gated)
//!
//! # AGPL cascade 警告
//!
//! 本 module は [`alice_foundry`] (AGPL-3.0-or-later or 商用) に依存する
//! `--features foundry` 有効化で **下流全体が AGPL に切り替わる** 商用 license
//! 保有者以外での有効化は非推奨 default (`parametric` feature) では本 module 不在
//!
//! # 提供 API
//!
//! - [`FoundryWeight`]: Regular / Bold の 2 variant
//! - [`cjk_glyph`] / [`cjk_glyph_3d`]: 単一 char → outline `Sdf2dNode` / extruded `SdfNode`
//! - [`cjk_string`] / [`cjk_string_3d`]: text → layout 済 SDF (advance + `letter_spacing`)
//! - [`supports`] / [`char_count`]: outline data の存在 / 総数確認
//!
//! # 使用例
//!
//! ```ignore
//! use alice_lol_font::foundry::{cjk_string, FoundryWeight};
//! let node = cjk_string("森林明", FoundryWeight::Bold, 0.02, 0.05).expect("chars in data");
//! # let _ = node;
//! ```

use alice_foundry::data::biz_udpgothic::{FONT_OUTLINES_BOLD, FONT_OUTLINES_REGULAR};
use alice_foundry::lol_bridge::glyph_outline_to_sdf2d;
use alice_sdf::sdf2d::Sdf2dNode;
use alice_sdf::types::SdfNode;
use glam::Vec2;
use std::sync::Arc;

/// `BIZ UDPGothic` outline data entry type: `(codepoint, contours, advance)`
///
/// `contours` は各 polygon (`&[(f32, f32)]` の連続 vertex 列) の集合
type OutlineEntry = (char, &'static [&'static [(f32, f32)]], f32);

/// Weight variant for `BIZ UDPGothic` outline lookup
///
/// Regular / Bold の 2 variant、各々 3344 char (JIS X 0208 Level 1 + ASCII +
/// hiragana + katakana) を持つ
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundryWeight {
    /// Regular weight (`BIZ UDPGothic` Regular)
    Regular,
    /// Bold weight (`BIZ UDPGothic` Bold)
    Bold,
}

impl FoundryWeight {
    /// 対応する outline data static array
    #[must_use]
    fn outline_data(self) -> &'static [OutlineEntry] {
        match self {
            Self::Regular => FONT_OUTLINES_REGULAR,
            Self::Bold => FONT_OUTLINES_BOLD,
        }
    }
}

/// outline data 内の char の総数 (Regular/Bold それぞれ 3344 char)
#[must_use]
pub fn char_count(weight: FoundryWeight) -> usize {
    weight.outline_data().len()
}

/// 指定 char が outline data に含まれるかを確認
#[must_use]
pub fn supports(ch: char, weight: FoundryWeight) -> bool {
    weight.outline_data().iter().any(|(c, _, _)| *c == ch)
}

/// CJK char lookup — outline `Sdf2dNode` + `advance` を返す
///
/// data に無い char は `None` `thickness` は outline stroke の半太さ (em 単位、
/// 典型値 0.02)
#[must_use]
pub fn cjk_glyph(ch: char, weight: FoundryWeight, thickness: f32) -> Option<(Sdf2dNode, f32)> {
    weight
        .outline_data()
        .iter()
        .find(|(c, _, _)| *c == ch)
        .map(|(_, contours, advance)| (glyph_outline_to_sdf2d(contours, thickness), *advance))
}

/// CJK char lookup 3D — polygon contour を [`SdfNode::Segment2D`] に tessellate
///
/// 各 contour の連続 2 vertex を `Segment2D` 1 個に変換、Union で fold
/// `extrude_depth` は Z 軸方向の厚み (`half_height = extrude_depth * 0.5`)
/// contour が空 (space 等) は `None`
#[must_use]
pub fn cjk_glyph_3d(
    ch: char,
    weight: FoundryWeight,
    thickness: f32,
    extrude_depth: f32,
) -> Option<(SdfNode, f32)> {
    let entry = weight.outline_data().iter().find(|(c, _, _)| *c == ch)?;
    let (_, contours, advance) = entry;

    if contours.is_empty() {
        return None;
    }

    let half_depth = extrude_depth * 0.5;
    let mut segments: Vec<SdfNode> = Vec::new();
    for contour in *contours {
        for w in contour.windows(2) {
            segments.push(SdfNode::Segment2D {
                a: Vec2::new(w[0].0, w[0].1),
                b: Vec2::new(w[1].0, w[1].1),
                thickness,
                half_height: half_depth,
            });
        }
    }

    if segments.is_empty() {
        return None;
    }

    let mut iter = segments.into_iter();
    let first = iter.next()?;
    let root = iter.fold(first, |acc, next| SdfNode::Union {
        a: Arc::new(acc),
        b: Arc::new(next),
    });

    Some((root, *advance))
}

/// CJK string layout — text → single `Sdf2dNode` (advance + `letter_spacing`)
///
/// unsupported char (data に無い) は skip、全 char が unsupported / empty text で `None`
/// space (' ') は data 内で advance のみ持つ空 contour として存在するため、
/// SDF に何も追加せず advance だけ消費する (Foundry data 直参照の自然な挙動)
#[must_use]
pub fn cjk_string(
    text: &str,
    weight: FoundryWeight,
    thickness: f32,
    letter_spacing: f32,
) -> Option<Sdf2dNode> {
    let mut result: Option<Sdf2dNode> = None;
    let mut cursor = 0.0_f32;
    let mut first = true;

    for ch in text.chars() {
        let Some((node, advance)) = cjk_glyph(ch, weight, thickness) else {
            continue;
        };
        if !first {
            cursor += letter_spacing;
        }
        let placed = node.translate(cursor, 0.0);
        result = Some(match result {
            None => placed,
            Some(acc) => acc.union(placed),
        });
        cursor += advance;
        first = false;
    }

    result
}

/// CJK string 3D — text → single `SdfNode` (extruded、advance + `letter_spacing`)
///
/// 各 char を [`cjk_glyph_3d`] で tessellate、Union で fold
/// space 等の空 contour char は advance のみ消費 (SDF なし)
#[must_use]
pub fn cjk_string_3d(
    text: &str,
    weight: FoundryWeight,
    thickness: f32,
    letter_spacing: f32,
    extrude_depth: f32,
) -> Option<SdfNode> {
    let mut result: Option<SdfNode> = None;
    let mut cursor = 0.0_f32;
    let mut first = true;

    for ch in text.chars() {
        // 空 contour char (space 等) は cjk_glyph_3d が None を返すが advance は消費したい
        // まず data lookup で advance を取り、SDF は cjk_glyph_3d で試みる
        let Some((_, _, advance)) = weight.outline_data().iter().find(|(c, _, _)| *c == ch) else {
            continue; // 未知 char は完全 skip
        };
        if !first {
            cursor += letter_spacing;
        }
        if let Some((node, _)) = cjk_glyph_3d(ch, weight, thickness, extrude_depth) {
            let placed = node.translate(cursor, 0.0, 0.0);
            result = Some(match result {
                None => placed,
                Some(acc) => acc.union(placed),
            });
        }
        cursor += *advance;
        first = false;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // char_count + supports
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn char_count_regular_and_bold_both_have_data() {
        assert!(char_count(FoundryWeight::Regular) > 100);
        assert!(char_count(FoundryWeight::Bold) > 100);
    }

    #[test]
    fn char_count_regular_bold_similar() {
        let r = char_count(FoundryWeight::Regular);
        let b = char_count(FoundryWeight::Bold);
        // Regular / Bold は data set が近い (BIZ UDPGothic の各 weight は 3344 char)
        let diff = r.abs_diff(b);
        assert!(
            diff < 10,
            "regular={r}, bold={b} should be near-equal (diff={diff})"
        );
    }

    #[test]
    fn supports_ascii_a() {
        assert!(supports('A', FoundryWeight::Regular));
        assert!(supports('A', FoundryWeight::Bold));
    }

    #[test]
    fn supports_space() {
        assert!(supports(' ', FoundryWeight::Regular));
    }

    #[test]
    fn supports_hiragana_a() {
        assert!(
            supports('あ', FoundryWeight::Regular),
            "'あ' (U+3042) should be in BIZ UDPGothic Regular data"
        );
    }

    #[test]
    fn supports_kanji_mori() {
        assert!(
            supports('森', FoundryWeight::Regular),
            "'森' (U+68EE) should be in BIZ UDPGothic Regular data"
        );
    }

    #[test]
    fn supports_returns_false_for_unlikely_char() {
        // 絵文字は Level 1 に含まれない
        assert!(!supports('🎉', FoundryWeight::Regular));
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // cjk_glyph
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn cjk_glyph_a_returns_some_with_advance() {
        let (_, advance) = cjk_glyph('A', FoundryWeight::Regular, 0.02).expect("A in data");
        assert!(advance > 0.0, "A advance should be positive, got {advance}");
    }

    #[test]
    fn cjk_glyph_space_returns_some_with_empty_contour() {
        // space は data 内で空 contour を持つ、cjk_glyph は Some を返す
        // (glyph_outline_to_sdf2d は空 contour で Circle{radius:0} を返す)
        let result = cjk_glyph(' ', FoundryWeight::Regular, 0.02);
        assert!(result.is_some(), "space should be in data with advance");
    }

    #[test]
    fn cjk_glyph_unsupported_returns_none() {
        assert!(cjk_glyph('🎉', FoundryWeight::Regular, 0.02).is_none());
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // cjk_glyph_3d
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn cjk_glyph_3d_a_returns_some() {
        let (_, advance) = cjk_glyph_3d('A', FoundryWeight::Bold, 0.02, 0.1).expect("A in data");
        assert!(advance > 0.0);
    }

    #[test]
    fn cjk_glyph_3d_space_returns_none() {
        // space は contour が空 → 3D では None
        assert!(cjk_glyph_3d(' ', FoundryWeight::Regular, 0.02, 0.1).is_none());
    }

    #[test]
    fn cjk_glyph_3d_kanji_mori_returns_some() {
        assert!(cjk_glyph_3d('森', FoundryWeight::Regular, 0.02, 0.1).is_some());
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // cjk_string
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn cjk_string_empty_returns_none() {
        assert!(cjk_string("", FoundryWeight::Regular, 0.02, 0.0).is_none());
    }

    #[test]
    fn cjk_string_all_unsupported_returns_none() {
        assert!(cjk_string("🎉🎊🎃", FoundryWeight::Regular, 0.02, 0.0).is_none());
    }

    #[test]
    fn cjk_string_hiragana_hello_returns_some() {
        // "こんにちは" (5 char) の layout
        let node = cjk_string("こんにちは", FoundryWeight::Bold, 0.02, 0.05);
        assert!(node.is_some(), "こんにちは should render");
    }

    #[test]
    fn cjk_string_mixed_ascii_kanji() {
        let node = cjk_string("HELLO 森", FoundryWeight::Regular, 0.02, 0.05);
        assert!(node.is_some(), "'HELLO 森' should render");
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // cjk_string_3d
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn cjk_string_3d_empty_returns_none() {
        assert!(cjk_string_3d("", FoundryWeight::Regular, 0.02, 0.0, 0.1).is_none());
    }

    #[test]
    fn cjk_string_3d_kanji_returns_some() {
        let node = cjk_string_3d("森", FoundryWeight::Bold, 0.02, 0.0, 0.1);
        assert!(node.is_some());
    }

    #[test]
    fn cjk_string_3d_multiple_chars_returns_union() {
        let node =
            cjk_string_3d("森林", FoundryWeight::Regular, 0.02, 0.05, 0.1).expect("non-empty");
        assert!(matches!(node, SdfNode::Union { .. }));
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // eval sanity check (real SDF)
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn cjk_string_a_far_exterior_positive() {
        let node = cjk_string("A", FoundryWeight::Bold, 0.02, 0.0).expect("A in data");
        // 遠方 (5.0, 5.0) は letter 外
        let d = alice_sdf::sdf2d::eval_2d(&node, [5.0, 5.0]);
        assert!(d > 1.0, "far from A should be well outside, got d={d}");
    }
}
