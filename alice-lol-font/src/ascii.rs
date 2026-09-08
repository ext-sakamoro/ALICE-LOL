//! ASCII glyph library — 26 uppercase letter (A..=Z)
//!
//! Phase 1-5.2 実装 26 letter 全対応 (2026-09-08、Phase 1-5 MVP 5 letter 拡張)
//! 各 letter は Line / Bezier stroke の組合せで em 座標系に定義される
//! `Glyph::to_sdf3d` の Bezier tessellation 経路と `Glyph::to_sdf2d` の
//! 直接経路の両方で全 letter が可視化可能
//!
//! # em 座標系規約
//!
//! - baseline: `y = 0`
//! - cap height: `y = 0.7` (`MetaFontParams::sans_regular().cap_height = 0.72` に近い)
//! - letter width: `x = 0` から `advance` まで
//! - stroke 位置は em 単位で hardcode (parametric variation は Phase 1.6+ で追加)
//!
//! # letter design summary
//!
//! | letter | strokes | 特徴 |
//! |---|---|---|
//! | A | 3 Line | 左脚 + 右脚 + 中央横棒 |
//! | B | 1 Line + 2 Bezier | 左縦棒 + 上下 2 bump |
//! | C | 2 Bezier | 右開き楕円弧 |
//! | D | 1 Line + 1 Bezier | 左縦棒 + 右半楕円 |
//! | E | 4 Line | 左縦棒 + 上/中/下 3 横棒 |
//! | F | 3 Line | 左縦棒 + 上/中 2 横棒 |
//! | G | 2 Bezier + 2 Line | C + 内部横棒 + 短縦棒 |
//! | H | 3 Line | 左右縦棒 + 中央横棒 |
//! | I | 1 Line | 単一縦棒 (advance 0.5) |
//! | J | 2 Line + 1 Bezier | 上部横棒 + 縦棒 + 下部フック |
//! | K | 3 Line | 左縦棒 + 2 斜線 (中点から上右/下右) |
//! | L | 2 Line | 左縦棒 + 下部横棒 |
//! | M | 4 Line | 左縦棒 + V 字 + 右縦棒 |
//! | N | 3 Line | 左右縦棒 + 対角線 |
//! | O | 2 Bezier | 楕円 |
//! | P | 1 Line + 1 Bezier | 左縦棒 + 上半 bump |
//! | Q | 2 Bezier + 1 Line | O + 右下 tail |
//! | R | 1 Line + 1 Bezier + 1 Line | P + 右下 対角線 |
//! | S | 2 Bezier | 上下 2 曲線 |
//! | T | 2 Line | 上部横棒 + 中央縦棒 |
//! | U | 2 Line + 1 Bezier | 左右縦棒 + 下部半円 |
//! | V | 2 Line | 2 対角線 (V 字) |
//! | W | 4 Line | 2 V 字連結 |
//! | X | 2 Line | 2 対角線 (X 字) |
//! | Y | 3 Line | 上部 V + 中央縦棒 |
//! | Z | 3 Line | 上/下横棒 + 対角線 |

use crate::glyph::Glyph;
use crate::stroke::Stroke;

/// Cap height in em units (glyph 上端の y 座標)
const CAP_HEIGHT: f32 = 0.7;

/// Middle bar y position (A / H の中央横棒用)
const MIDDLE_Y: f32 = 0.35;

/// Standard advance for full-width letters
const FULL_ADVANCE: f32 = 1.0;

/// Narrow advance for I letter
const NARROW_ADVANCE: f32 = 0.5;

/// Dispatch to per-letter parametric definition for an ASCII uppercase letter
///
/// Phase 1-5.2 完全対応: A..=Z の 26 letter 全て Unsupported letter (lowercase /
/// digit / 記号) は `None` を返す `to_ascii_uppercase` は caller 側で適用推奨
/// (本 fn は case-sensitive)
#[must_use]
pub fn ascii_uppercase(ch: char) -> Option<Glyph> {
    match ch {
        'A' => Some(letter_a()),
        'B' => Some(letter_b()),
        'C' => Some(letter_c()),
        'D' => Some(letter_d()),
        'E' => Some(letter_e()),
        'F' => Some(letter_f()),
        'G' => Some(letter_g()),
        'H' => Some(letter_h()),
        'I' => Some(letter_i()),
        'J' => Some(letter_j()),
        'K' => Some(letter_k()),
        'L' => Some(letter_l()),
        'M' => Some(letter_m()),
        'N' => Some(letter_n()),
        'O' => Some(letter_o()),
        'P' => Some(letter_p()),
        'Q' => Some(letter_q()),
        'R' => Some(letter_r()),
        'S' => Some(letter_s()),
        'T' => Some(letter_t()),
        'U' => Some(letter_u()),
        'V' => Some(letter_v()),
        'W' => Some(letter_w()),
        'X' => Some(letter_x()),
        'Y' => Some(letter_y()),
        'Z' => Some(letter_z()),
        _ => None,
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Letter 定義群 (Phase 1-5.2: 26 letter 全実装)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

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

/// Letter 'B' — 左縦棒 + 上下 2 bump (Bezier)
fn letter_b() -> Glyph {
    Glyph::new(
        'B',
        vec![
            // 左縦棒
            Stroke::line([0.0, 0.0], [0.0, CAP_HEIGHT]),
            // 上 bump: 左上 → 右上 → 中央右 → 中央左
            Stroke::bezier(
                [0.0, CAP_HEIGHT],
                [0.85, CAP_HEIGHT],
                [0.85, MIDDLE_Y],
                [0.0, MIDDLE_Y],
            ),
            // 下 bump: 中央左 → 中央右 → 右下 → 左下
            Stroke::bezier([0.0, MIDDLE_Y], [0.95, MIDDLE_Y], [0.95, 0.0], [0.0, 0.0]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'C' — 右開き楕円弧 (2 Bezier)
fn letter_c() -> Glyph {
    Glyph::new(
        'C',
        vec![
            // 上半: 右上 (0.9, 0.55) → 上 → 左 (0, MIDDLE_Y)
            Stroke::bezier(
                [0.9, 0.55],
                [0.9, CAP_HEIGHT],
                [0.0, CAP_HEIGHT],
                [0.0, MIDDLE_Y],
            ),
            // 下半: 左 → 下 → 右下 (0.9, 0.15)
            Stroke::bezier([0.0, MIDDLE_Y], [0.0, 0.0], [0.9, 0.0], [0.9, 0.15]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'D' — 左縦棒 + 右半楕円
fn letter_d() -> Glyph {
    Glyph::new(
        'D',
        vec![
            Stroke::line([0.0, 0.0], [0.0, CAP_HEIGHT]),
            Stroke::bezier(
                [0.0, CAP_HEIGHT],
                [FULL_ADVANCE, CAP_HEIGHT],
                [FULL_ADVANCE, 0.0],
                [0.0, 0.0],
            ),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'E' — 左縦棒 + 上/中/下 3 横棒
fn letter_e() -> Glyph {
    Glyph::new(
        'E',
        vec![
            Stroke::line([0.0, 0.0], [0.0, CAP_HEIGHT]),
            Stroke::line([0.0, CAP_HEIGHT], [0.9, CAP_HEIGHT]),
            Stroke::line([0.0, MIDDLE_Y], [0.75, MIDDLE_Y]),
            Stroke::line([0.0, 0.0], [0.9, 0.0]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'F' — E の下横棒なし
fn letter_f() -> Glyph {
    Glyph::new(
        'F',
        vec![
            Stroke::line([0.0, 0.0], [0.0, CAP_HEIGHT]),
            Stroke::line([0.0, CAP_HEIGHT], [0.9, CAP_HEIGHT]),
            Stroke::line([0.0, MIDDLE_Y], [0.75, MIDDLE_Y]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'G' — C + 内部横棒 + 短縦棒
fn letter_g() -> Glyph {
    Glyph::new(
        'G',
        vec![
            // C 部分 (上半)
            Stroke::bezier(
                [0.9, 0.55],
                [0.9, CAP_HEIGHT],
                [0.0, CAP_HEIGHT],
                [0.0, MIDDLE_Y],
            ),
            // C 部分 (下半)
            Stroke::bezier([0.0, MIDDLE_Y], [0.0, 0.0], [0.9, 0.0], [0.9, 0.2]),
            // 内部横棒 (右側中央から左へ)
            Stroke::line([0.5, MIDDLE_Y], [0.9, MIDDLE_Y]),
            // 短縦棒 (内部横棒から右下端まで)
            Stroke::line([0.9, MIDDLE_Y], [0.9, 0.2]),
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

/// Letter 'J' — 上部横棒 + 縦棒 + 下部フック (Bezier)
fn letter_j() -> Glyph {
    Glyph::new(
        'J',
        vec![
            // 上部横棒
            Stroke::line([0.2, CAP_HEIGHT], [FULL_ADVANCE, CAP_HEIGHT]),
            // 縦棒 (右寄り)
            Stroke::line([0.65, CAP_HEIGHT], [0.65, 0.2]),
            // 下部フック (縦棒下 → 下 → 左)
            Stroke::bezier([0.65, 0.2], [0.65, 0.0], [0.1, 0.0], [0.1, 0.15]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'K' — 左縦棒 + 2 斜線 (中点から上右 / 下右)
fn letter_k() -> Glyph {
    Glyph::new(
        'K',
        vec![
            Stroke::line([0.0, 0.0], [0.0, CAP_HEIGHT]),
            Stroke::line([0.0, MIDDLE_Y], [0.9, CAP_HEIGHT]),
            Stroke::line([0.0, MIDDLE_Y], [0.9, 0.0]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'L' — 左縦棒 + 下部横棒
fn letter_l() -> Glyph {
    Glyph::new(
        'L',
        vec![
            Stroke::line([0.0, 0.0], [0.0, CAP_HEIGHT]),
            Stroke::line([0.0, 0.0], [0.85, 0.0]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'M' — 左縦棒 + V 字 + 右縦棒
fn letter_m() -> Glyph {
    Glyph::new(
        'M',
        vec![
            Stroke::line([0.0, 0.0], [0.0, CAP_HEIGHT]),
            Stroke::line([0.0, CAP_HEIGHT], [0.5, 0.2]),
            Stroke::line([0.5, 0.2], [FULL_ADVANCE, CAP_HEIGHT]),
            Stroke::line([FULL_ADVANCE, 0.0], [FULL_ADVANCE, CAP_HEIGHT]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'N' — 左右縦棒 + 対角線
fn letter_n() -> Glyph {
    Glyph::new(
        'N',
        vec![
            Stroke::line([0.0, 0.0], [0.0, CAP_HEIGHT]),
            Stroke::line([0.0, CAP_HEIGHT], [FULL_ADVANCE, 0.0]),
            Stroke::line([FULL_ADVANCE, 0.0], [FULL_ADVANCE, CAP_HEIGHT]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'O' — 上半 + 下半 の 2 Bezier で楕円 approximation
fn letter_o() -> Glyph {
    let mid_y = CAP_HEIGHT * 0.5;
    Glyph::new(
        'O',
        vec![
            Stroke::bezier(
                [FULL_ADVANCE, mid_y],
                [FULL_ADVANCE, CAP_HEIGHT],
                [0.0, CAP_HEIGHT],
                [0.0, mid_y],
            ),
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

/// Letter 'P' — 左縦棒 + 上半 bump (Bezier)
fn letter_p() -> Glyph {
    Glyph::new(
        'P',
        vec![
            Stroke::line([0.0, 0.0], [0.0, CAP_HEIGHT]),
            Stroke::bezier(
                [0.0, CAP_HEIGHT],
                [0.9, CAP_HEIGHT],
                [0.9, MIDDLE_Y],
                [0.0, MIDDLE_Y],
            ),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'Q' — O + 右下 tail
fn letter_q() -> Glyph {
    let mid_y = CAP_HEIGHT * 0.5;
    Glyph::new(
        'Q',
        vec![
            Stroke::bezier(
                [FULL_ADVANCE, mid_y],
                [FULL_ADVANCE, CAP_HEIGHT],
                [0.0, CAP_HEIGHT],
                [0.0, mid_y],
            ),
            Stroke::bezier(
                [0.0, mid_y],
                [0.0, 0.0],
                [FULL_ADVANCE, 0.0],
                [FULL_ADVANCE, mid_y],
            ),
            // tail (右下)
            Stroke::line([0.6, 0.15], [FULL_ADVANCE, -0.05]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'R' — P + 右下 対角線
fn letter_r() -> Glyph {
    Glyph::new(
        'R',
        vec![
            Stroke::line([0.0, 0.0], [0.0, CAP_HEIGHT]),
            Stroke::bezier(
                [0.0, CAP_HEIGHT],
                [0.9, CAP_HEIGHT],
                [0.9, MIDDLE_Y],
                [0.0, MIDDLE_Y],
            ),
            // 右下対角線 (中央から右下端)
            Stroke::line([0.35, MIDDLE_Y], [FULL_ADVANCE, 0.0]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'S' — 上下 2 曲線 (Bezier)
fn letter_s() -> Glyph {
    Glyph::new(
        'S',
        vec![
            // 上半: 右上 → 上 → 左 → 中央左
            Stroke::bezier(
                [0.9, 0.55],
                [0.9, CAP_HEIGHT],
                [0.1, CAP_HEIGHT],
                [0.1, MIDDLE_Y],
            ),
            // 下半: 中央左 → 中央右 → 下 → 左下 (S の下半)
            Stroke::bezier([0.1, MIDDLE_Y], [0.9, MIDDLE_Y], [0.9, 0.0], [0.1, 0.15]),
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

/// Letter 'U' — 左右縦棒 + 下部半円 (Bezier)
fn letter_u() -> Glyph {
    Glyph::new(
        'U',
        vec![
            Stroke::line([0.0, CAP_HEIGHT], [0.0, 0.15]),
            Stroke::bezier(
                [0.0, 0.15],
                [0.0, 0.0],
                [FULL_ADVANCE, 0.0],
                [FULL_ADVANCE, 0.15],
            ),
            Stroke::line([FULL_ADVANCE, 0.15], [FULL_ADVANCE, CAP_HEIGHT]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'V' — 2 対角線 (V 字)
fn letter_v() -> Glyph {
    Glyph::new(
        'V',
        vec![
            Stroke::line([0.0, CAP_HEIGHT], [0.5, 0.0]),
            Stroke::line([0.5, 0.0], [FULL_ADVANCE, CAP_HEIGHT]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'W' — 4 対角線 (2 V 字連結)
fn letter_w() -> Glyph {
    Glyph::new(
        'W',
        vec![
            Stroke::line([0.0, CAP_HEIGHT], [0.25, 0.0]),
            Stroke::line([0.25, 0.0], [0.5, 0.4]),
            Stroke::line([0.5, 0.4], [0.75, 0.0]),
            Stroke::line([0.75, 0.0], [FULL_ADVANCE, CAP_HEIGHT]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'X' — 2 対角線 (X 字)
fn letter_x() -> Glyph {
    Glyph::new(
        'X',
        vec![
            Stroke::line([0.0, 0.0], [FULL_ADVANCE, CAP_HEIGHT]),
            Stroke::line([0.0, CAP_HEIGHT], [FULL_ADVANCE, 0.0]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'Y' — 上部 V + 中央縦棒
fn letter_y() -> Glyph {
    Glyph::new(
        'Y',
        vec![
            Stroke::line([0.0, CAP_HEIGHT], [0.5, MIDDLE_Y]),
            Stroke::line([FULL_ADVANCE, CAP_HEIGHT], [0.5, MIDDLE_Y]),
            Stroke::line([0.5, MIDDLE_Y], [0.5, 0.0]),
        ],
        FULL_ADVANCE,
    )
}

/// Letter 'Z' — 上/下横棒 + 対角線
fn letter_z() -> Glyph {
    Glyph::new(
        'Z',
        vec![
            Stroke::line([0.0, CAP_HEIGHT], [FULL_ADVANCE, CAP_HEIGHT]),
            Stroke::line([FULL_ADVANCE, CAP_HEIGHT], [0.0, 0.0]),
            Stroke::line([0.0, 0.0], [FULL_ADVANCE, 0.0]),
        ],
        FULL_ADVANCE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MetaFontParams, PenModel};

    /// 全 26 uppercase letter (A..=Z) の順序固定 array (iteration test 用)
    const ALL_UPPERCASE: [char; 26] = [
        'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R',
        'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
    ];

    #[test]
    fn all_26_letters_supported() {
        for ch in ALL_UPPERCASE {
            assert!(
                ascii_uppercase(ch).is_some(),
                "letter '{ch}' should be supported (Phase 1-5.2)"
            );
        }
    }

    #[test]
    fn unsupported_letters_return_none() {
        assert!(ascii_uppercase('a').is_none()); // lowercase
        assert!(ascii_uppercase('z').is_none());
        assert!(ascii_uppercase('1').is_none()); // digit
        assert!(ascii_uppercase(' ').is_none()); // space
        assert!(ascii_uppercase('!').is_none()); // 記号
        assert!(ascii_uppercase('あ').is_none()); // 日本語
    }

    #[test]
    fn codepoint_matches_dispatch_for_all_26() {
        for ch in ALL_UPPERCASE {
            let g = ascii_uppercase(ch).unwrap();
            assert_eq!(g.codepoint(), ch, "codepoint mismatch for '{ch}'");
        }
    }

    #[test]
    fn stroke_counts_match_design() {
        // MVP 5 letter
        assert_eq!(ascii_uppercase('A').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('H').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('I').unwrap().stroke_count(), 1);
        assert_eq!(ascii_uppercase('O').unwrap().stroke_count(), 2);
        assert_eq!(ascii_uppercase('T').unwrap().stroke_count(), 2);
        // Phase 1-5.2 追加 21 letter
        assert_eq!(ascii_uppercase('B').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('C').unwrap().stroke_count(), 2);
        assert_eq!(ascii_uppercase('D').unwrap().stroke_count(), 2);
        assert_eq!(ascii_uppercase('E').unwrap().stroke_count(), 4);
        assert_eq!(ascii_uppercase('F').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('G').unwrap().stroke_count(), 4);
        assert_eq!(ascii_uppercase('J').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('K').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('L').unwrap().stroke_count(), 2);
        assert_eq!(ascii_uppercase('M').unwrap().stroke_count(), 4);
        assert_eq!(ascii_uppercase('N').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('P').unwrap().stroke_count(), 2);
        assert_eq!(ascii_uppercase('Q').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('R').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('S').unwrap().stroke_count(), 2);
        assert_eq!(ascii_uppercase('U').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('V').unwrap().stroke_count(), 2);
        assert_eq!(ascii_uppercase('W').unwrap().stroke_count(), 4);
        assert_eq!(ascii_uppercase('X').unwrap().stroke_count(), 2);
        assert_eq!(ascii_uppercase('Y').unwrap().stroke_count(), 3);
        assert_eq!(ascii_uppercase('Z').unwrap().stroke_count(), 3);
    }

    #[test]
    fn advance_widths_correct() {
        // I は narrow (0.5)、他 25 letter は full (1.0)
        for ch in ALL_UPPERCASE {
            let g = ascii_uppercase(ch).unwrap();
            let expected = if ch == 'I' {
                NARROW_ADVANCE
            } else {
                FULL_ADVANCE
            };
            assert!(
                (g.advance() - expected).abs() < 1e-6,
                "advance mismatch for '{ch}': got {}, expected {expected}",
                g.advance()
            );
        }
    }

    fn regular_pen() -> PenModel {
        PenModel::from_params(&MetaFontParams::sans_regular())
    }

    #[test]
    fn all_26_letters_produce_non_empty_sdf2d() {
        let pen = regular_pen();
        for ch in ALL_UPPERCASE {
            let g = ascii_uppercase(ch).unwrap();
            assert!(
                g.to_sdf2d(&pen).is_some(),
                "letter '{ch}' should produce Some Sdf2dNode"
            );
        }
    }

    #[test]
    fn all_26_letters_produce_non_empty_sdf3d() {
        let pen = regular_pen();
        for ch in ALL_UPPERCASE {
            let g = ascii_uppercase(ch).unwrap();
            assert!(
                g.to_sdf3d(&pen, 0.1).is_some(),
                "letter '{ch}' should produce Some SdfNode"
            );
        }
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // Per-letter interior/exterior verification (代表 letter で pattern 網羅)
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// interior 判定: `d ≤ 0.05` (thin stroke で境界近傍を許容)
    fn assert_interior(ch: char, point: [f32; 2]) {
        let pen = regular_pen();
        let g = ascii_uppercase(ch).unwrap();
        let node = g.to_sdf2d(&pen).unwrap();
        let d = alice_sdf::sdf2d::eval_2d(&node, point);
        assert!(
            d <= 0.05,
            "'{ch}' interior point {point:?} should be inside stroke (d ≤ 0.05), got d={d}"
        );
    }

    /// exterior 判定: `d > 0.05` (thin stroke 影響から離れた点)
    fn assert_exterior(ch: char, point: [f32; 2]) {
        let pen = regular_pen();
        let g = ascii_uppercase(ch).unwrap();
        let node = g.to_sdf2d(&pen).unwrap();
        let d = alice_sdf::sdf2d::eval_2d(&node, point);
        assert!(
            d > 0.05,
            "'{ch}' exterior point {point:?} should be outside stroke (d > 0.05), got d={d}"
        );
    }

    #[test]
    fn letter_a_interior_and_exterior() {
        assert_interior('A', [0.25, MIDDLE_Y]); // 左脚中点
        assert_exterior('A', [0.5, 0.5]); // 三角形内部空洞 (peak 直下)
    }

    #[test]
    fn letter_b_interior_and_exterior() {
        assert_interior('B', [0.0, MIDDLE_Y]); // 左縦棒中点
        assert_exterior('B', [FULL_ADVANCE, MIDDLE_Y]); // 右外側
    }

    #[test]
    fn letter_e_interior_and_exterior() {
        assert_interior('E', [0.0, MIDDLE_Y]); // 左縦棒中点
        assert_interior('E', [0.4, MIDDLE_Y]); // 中央横棒
        assert_exterior('E', [0.5, 0.55]); // 上部空洞
    }

    #[test]
    fn letter_h_interior_and_exterior() {
        assert_interior('H', [0.5, MIDDLE_Y]); // 中央横棒
        assert_exterior('H', [0.5, 0.6]); // 上部空洞
    }

    #[test]
    fn letter_i_interior_and_exterior() {
        assert_interior('I', [NARROW_ADVANCE * 0.5, MIDDLE_Y]); // 縦棒中点
        assert_exterior('I', [0.0, MIDDLE_Y]); // 左側 (I 外)
    }

    #[test]
    fn letter_l_interior_and_exterior() {
        assert_interior('L', [0.0, MIDDLE_Y]); // 左縦棒中点
        assert_interior('L', [0.4, 0.0]); // 下部横棒
        assert_exterior('L', [0.5, 0.5]); // 右上空洞
    }

    #[test]
    fn letter_o_interior_and_exterior() {
        assert_interior('O', [FULL_ADVANCE, MIDDLE_Y]); // 右端 stroke
        assert_exterior('O', [0.5, MIDDLE_Y]); // 楕円中心 hollow
    }

    #[test]
    fn letter_t_interior_and_exterior() {
        assert_interior('T', [0.5, CAP_HEIGHT]); // 上部横棒中央
        assert_exterior('T', [0.8, MIDDLE_Y]); // 右下 (中央縦棒より右)
    }

    #[test]
    fn letter_x_interior_and_exterior() {
        assert_interior('X', [0.5, MIDDLE_Y]); // 中心 (2 対角線 intersect)
        assert_exterior('X', [0.5, 0.6]); // 上部空洞
    }

    #[test]
    fn letter_z_interior_and_exterior() {
        assert_interior('Z', [0.5, CAP_HEIGHT]); // 上横棒中央
        assert_interior('Z', [0.5, 0.0]); // 下横棒中央
        assert_exterior('Z', [0.0, 0.6]); // 左上空洞
    }
}
