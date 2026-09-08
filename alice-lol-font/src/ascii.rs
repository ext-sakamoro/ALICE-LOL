//! ASCII glyph library — MVP 5 letter (A / H / I / O / T)
//!
//! Phase 1-5 MVP scope 26 upper letter 全実装は Phase 1-5.2 で追加予定
//! 5 letter を先行実装する意図: 1 letter あたり Line-only (I) + Line-mix (A/H/T) +
//! Bezier (O) の 3 pattern を最小網羅し、`Glyph::to_sdf3d` の tessellation 経路と
//! `Glyph::to_sdf2d` の直接経路を実 letter で verify する
//!
//! # em 座標系規約
//!
//! - baseline: `y = 0`
//! - cap height: `y = 0.7` (`MetaFontParams::sans_regular().cap_height = 0.72` に近い)
//! - letter width: `x = 0` から `advance` まで
//! - stroke 位置は em 単位で hardcode (parametric variation は Phase 1.6+ で追加)
//!
//! # 各 letter の skeleton design
//!
//! | letter | strokes | 特徴 |
//! |---|---|---|
//! | A | 3 Line | 左脚 (0,0→0.5,0.7) + 右脚 (0.5,0.7→1.0,0) + 横棒 (0.2,0.3→0.8,0.3) |
//! | H | 3 Line | 左縦棒 + 右縦棒 + 中央横棒 |
//! | I | 1 Line | 単一縦棒 (0.25,0→0.25,0.7)、advance 0.5 |
//! | O | 2 Bezier | 上半 + 下半 の楕円 approximation |
//! | T | 2 Line | 上部横棒 + 中央縦棒 |

use crate::glyph::Glyph;
use crate::stroke::Stroke;

/// Cap height in em units (glyph 上端の y 座標)
const CAP_HEIGHT: f32 = 0.7;

/// Middle bar y position (A / H の中央横棒用)
const MIDDLE_Y: f32 = 0.35;

/// Standard advance for full-width letters (A/H/O/T)
const FULL_ADVANCE: f32 = 1.0;

/// Narrow advance for I letter
const NARROW_ADVANCE: f32 = 0.5;

/// Dispatch to per-letter parametric definition for an ASCII uppercase letter
///
/// MVP scope: A / H / I / O / T Unsupported letters return `None`
/// `to_ascii_uppercase` は caller 側で適用推奨 (本 fn は case-sensitive)
#[must_use]
pub fn ascii_uppercase(ch: char) -> Option<Glyph> {
    match ch {
        'A' => Some(letter_a()),
        'H' => Some(letter_h()),
        'I' => Some(letter_i()),
        'O' => Some(letter_o()),
        'T' => Some(letter_t()),
        _ => None,
    }
}

/// Letter 'A' — 左脚 + 右脚 + 中央横棒
fn letter_a() -> Glyph {
    Glyph::new(
        'A',
        vec![
            Stroke::line([0.0, 0.0], [0.5, CAP_HEIGHT]),
            Stroke::line([0.5, CAP_HEIGHT], [FULL_ADVANCE, 0.0]),
            Stroke::line([0.2, MIDDLE_Y], [0.8, MIDDLE_Y]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'H' — 左右縦棒 + 中央横棒
fn letter_h() -> Glyph {
    Glyph::new(
        'H',
        vec![
            Stroke::line([0.0, 0.0], [0.0, CAP_HEIGHT]),
            Stroke::line([FULL_ADVANCE, 0.0], [FULL_ADVANCE, CAP_HEIGHT]),
            Stroke::line([0.0, MIDDLE_Y], [FULL_ADVANCE, MIDDLE_Y]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'I' — 単一縦棒 (advance 0.5 で narrow)
fn letter_i() -> Glyph {
    Glyph::new(
        'I',
        vec![Stroke::line(
            [NARROW_ADVANCE * 0.5, 0.0],
            [NARROW_ADVANCE * 0.5, CAP_HEIGHT],
        )],
        NARROW_ADVANCE,
    )
}

/// Letter 'O' — 上半 + 下半 の 2 Bezier で楕円 approximation
///
/// 各 Bezier の control point は em 単位 `y = CAP_HEIGHT / 2` (0.35) で
/// 上下対称になるよう配置 tessellation は `DEFAULT_BEZIER_SAMPLES = 12` で
/// 各半円が 12 segment に分割される
fn letter_o() -> Glyph {
    let mid_y = CAP_HEIGHT * 0.5;
    Glyph::new(
        'O',
        vec![
            // 上半: right → top → left
            Stroke::bezier(
                [FULL_ADVANCE, mid_y],
                [FULL_ADVANCE, CAP_HEIGHT],
                [0.0, CAP_HEIGHT],
                [0.0, mid_y],
            ),
            // 下半: left → bottom → right
            Stroke::bezier(
                [0.0, mid_y],
                [0.0, 0.0],
                [FULL_ADVANCE, 0.0],
                [FULL_ADVANCE, mid_y],
            ),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'T' — 上部横棒 + 中央縦棒
fn letter_t() -> Glyph {
    Glyph::new(
        'T',
        vec![
            Stroke::line([0.0, CAP_HEIGHT], [FULL_ADVANCE, CAP_HEIGHT]),
            Stroke::line([FULL_ADVANCE * 0.5, CAP_HEIGHT], [FULL_ADVANCE * 0.5, 0.0]),
        ],
        FULL_ADVANCE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MetaFontParams, PenModel};

    #[test]
    fn all_5_letters_supported() {
        assert!(ascii_uppercase('A').is_some());
        assert!(ascii_uppercase('H').is_some());
        assert!(ascii_uppercase('I').is_some());
        assert!(ascii_uppercase('O').is_some());
        assert!(ascii_uppercase('T').is_some());
    }

    #[test]
    fn unsupported_letters_return_none() {
        assert!(ascii_uppercase('B').is_none());
        assert!(ascii_uppercase('Z').is_none());
        assert!(ascii_uppercase('a').is_none()); // lowercase 未サポート
        assert!(ascii_uppercase('1').is_none()); // digit 未サポート
    }

    #[test]
    fn stroke_counts_match_design() {
        assert_eq!(ascii_uppercase('A').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('H').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('I').unwrap().stroke_count(), 1);
        assert_eq!(ascii_uppercase('O').unwrap().stroke_count(), 2);
        assert_eq!(ascii_uppercase('T').unwrap().stroke_count(), 2);
    }

    #[test]
    fn advance_widths_match_design() {
        assert!((ascii_uppercase('A').unwrap().advance() - FULL_ADVANCE).abs() < 1e-6);
        assert!((ascii_uppercase('H').unwrap().advance() - FULL_ADVANCE).abs() < 1e-6);
        assert!((ascii_uppercase('I').unwrap().advance() - NARROW_ADVANCE).abs() < 1e-6);
        assert!((ascii_uppercase('O').unwrap().advance() - FULL_ADVANCE).abs() < 1e-6);
        assert!((ascii_uppercase('T').unwrap().advance() - FULL_ADVANCE).abs() < 1e-6);
    }

    #[test]
    fn codepoints_match_dispatch() {
        assert_eq!(ascii_uppercase('A').unwrap().codepoint(), 'A');
        assert_eq!(ascii_uppercase('H').unwrap().codepoint(), 'H');
        assert_eq!(ascii_uppercase('I').unwrap().codepoint(), 'I');
        assert_eq!(ascii_uppercase('O').unwrap().codepoint(), 'O');
        assert_eq!(ascii_uppercase('T').unwrap().codepoint(), 'T');
    }

    fn regular_pen() -> PenModel {
        PenModel::from_params(&MetaFontParams::sans_regular())
    }

    #[test]
    fn all_letters_produce_non_empty_sdf2d() {
        let pen = regular_pen();
        for ch in ['A', 'H', 'I', 'O', 'T'] {
            let g = ascii_uppercase(ch).unwrap();
            assert!(
                g.to_sdf2d(&pen).is_some(),
                "letter '{ch}' should produce Some Sdf2dNode"
            );
        }
    }

    #[test]
    fn all_letters_produce_non_empty_sdf3d() {
        let pen = regular_pen();
        for ch in ['A', 'H', 'I', 'O', 'T'] {
            let g = ascii_uppercase(ch).unwrap();
            assert!(
                g.to_sdf3d(&pen, 0.1).is_some(),
                "letter '{ch}' should produce Some SdfNode"
            );
        }
    }

    #[test]
    fn letter_a_interior_at_left_leg_midpoint() {
        // A の左脚中点 (0.25, 0.35) は stroke 内部
        let pen = regular_pen();
        let g = ascii_uppercase('A').unwrap();
        let node = g.to_sdf2d(&pen).unwrap();
        let d = alice_sdf::sdf2d::eval_2d(&node, [0.25, 0.35]);
        assert!(d <= 0.05, "A left leg midpoint should be near/inside stroke, got d={d}");
    }

    #[test]
    fn letter_h_interior_at_middle_bar() {
        let pen = regular_pen();
        let g = ascii_uppercase('H').unwrap();
        let node = g.to_sdf2d(&pen).unwrap();
        let d = alice_sdf::sdf2d::eval_2d(&node, [0.5, MIDDLE_Y]);
        assert!(d <= 0.0, "H middle bar center should be inside, got d={d}");
    }

    #[test]
    fn letter_i_exterior_at_side() {
        // I の縦棒は x=0.25 の一本、x=0 は外側
        let pen = regular_pen();
        let g = ascii_uppercase('I').unwrap();
        let node = g.to_sdf2d(&pen).unwrap();
        let d = alice_sdf::sdf2d::eval_2d(&node, [0.0, 0.35]);
        assert!(d > 0.0, "I side (x=0) should be outside stroke, got d={d}");
    }

    #[test]
    fn letter_o_interior_of_curve() {
        // O 楕円の右端 stroke 上 (1.0, 0.35) 近傍は内部
        let pen = regular_pen();
        let g = ascii_uppercase('O').unwrap();
        let node = g.to_sdf2d(&pen).unwrap();
        let d = alice_sdf::sdf2d::eval_2d(&node, [1.0, 0.35]);
        assert!(d <= 0.05, "O right edge should be near stroke, got d={d}");
    }

    #[test]
    fn letter_o_center_is_hollow() {
        // O の中心 (0.5, 0.35) は空洞 (stroke なし → 外部)
        let pen = regular_pen();
        let g = ascii_uppercase('O').unwrap();
        let node = g.to_sdf2d(&pen).unwrap();
        let d = alice_sdf::sdf2d::eval_2d(&node, [0.5, MIDDLE_Y]);
        assert!(d > 0.1, "O center should be hollow (d > 0.1), got d={d}");
    }

    #[test]
    fn letter_t_interior_at_top_bar_center() {
        let pen = regular_pen();
        let g = ascii_uppercase('T').unwrap();
        let node = g.to_sdf2d(&pen).unwrap();
        let d = alice_sdf::sdf2d::eval_2d(&node, [0.5, CAP_HEIGHT]);
        assert!(d <= 0.0, "T top bar center should be inside, got d={d}");
    }
}
