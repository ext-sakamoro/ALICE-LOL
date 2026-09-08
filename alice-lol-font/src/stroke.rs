//! `Stroke` — glyph skeleton primitive (Line / Cubic Bezier) + `Sdf2dNode` 変換
//!
//! Glyph 構築 helper 各 stroke は `StrokeWeight` (Regular / Thick / Thin) を持ち、
//! `PenModel` 経由で対応する half-width を選択して `alice_sdf::Sdf2dNode::Line` /
//! `Sdf2dNode::Bezier` に変換する italic skew は `PenModel::skew_point` で
//! 全 endpoint / control point に適用される
//!
//! # 設計判断
//!
//! - `StrokeWeight` を stroke に持たせる (pen 側でなく) → 同一 glyph 内で
//!   Thick / Thin を混在できる (serif の縦棒太 + 横棒細を 1 glyph 内で表現)
//! - `Union` fold は左結合 (`Sdf2dNode::Union(a, b)`) balanced tree 化は
//!   glyph 単位 (数十 stroke 上限) では不要、shallow recursion のみ
//!
//! # 使用例
//!
//! ```
//! use alice_lol_font::{MetaFontParams, PenModel};
//! use alice_lol_font::stroke::{Stroke, strokes_to_sdf2d};
//!
//! let pen = PenModel::from_params(&MetaFontParams::sans_regular());
//! let strokes = [
//!     Stroke::line([0.0, 0.0], [0.5, 0.7]),
//!     Stroke::line([0.5, 0.7], [1.0, 0.0]),
//!     Stroke::line([0.2, 0.3], [0.8, 0.3]),
//! ]; // 'A' の 3 stroke
//! let node = strokes_to_sdf2d(&strokes, &pen).expect("non-empty strokes → Some");
//! # let _ = node;
//! ```

use crate::pen::PenModel;
use alice_sdf::sdf2d::Sdf2dNode;

/// `PenModel` のどの half-width を使うかを選択する variant
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeWeight {
    /// Default — `PenModel::half_width` を使う (contrast 影響なし)
    Regular,
    /// Thick — `PenModel::thick_half_width` を使う (高 contrast font の縦棒等)
    Thick,
    /// Thin — `PenModel::thin_half_width` を使う (高 contrast font の横棒等)
    Thin,
}

/// A single stroke primitive (glyph building block)
///
/// `Line` は直線分、`Bezier` は cubic Bezier curve `weight` で
/// `PenModel` の どの half-width を使うかが決まる
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stroke {
    /// Line segment from `a` to `b`
    Line {
        /// Start point (em coordinate)
        a: [f32; 2],
        /// End point (em coordinate)
        b: [f32; 2],
        /// Weight class (pen half-width selection)
        weight: StrokeWeight,
    },
    /// Cubic Bezier from `p0` to `p3` with control points `p1`, `p2`
    Bezier {
        /// Start point
        p0: [f32; 2],
        /// First control point
        p1: [f32; 2],
        /// Second control point
        p2: [f32; 2],
        /// End point
        p3: [f32; 2],
        /// Weight class
        weight: StrokeWeight,
    },
}

impl Stroke {
    /// Regular-weight line convenience constructor
    #[must_use]
    pub const fn line(a: [f32; 2], b: [f32; 2]) -> Self {
        Self::Line {
            a,
            b,
            weight: StrokeWeight::Regular,
        }
    }

    /// Thick-weight line convenience constructor (高 contrast 縦棒用)
    #[must_use]
    pub const fn line_thick(a: [f32; 2], b: [f32; 2]) -> Self {
        Self::Line {
            a,
            b,
            weight: StrokeWeight::Thick,
        }
    }

    /// Thin-weight line convenience constructor (高 contrast 横棒用)
    #[must_use]
    pub const fn line_thin(a: [f32; 2], b: [f32; 2]) -> Self {
        Self::Line {
            a,
            b,
            weight: StrokeWeight::Thin,
        }
    }

    /// Regular-weight cubic Bezier convenience constructor
    #[must_use]
    pub const fn bezier(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2]) -> Self {
        Self::Bezier {
            p0,
            p1,
            p2,
            p3,
            weight: StrokeWeight::Regular,
        }
    }

    /// Thick-weight cubic Bezier convenience constructor
    #[must_use]
    pub const fn bezier_thick(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2]) -> Self {
        Self::Bezier {
            p0,
            p1,
            p2,
            p3,
            weight: StrokeWeight::Thick,
        }
    }

    /// Thin-weight cubic Bezier convenience constructor
    #[must_use]
    pub const fn bezier_thin(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2]) -> Self {
        Self::Bezier {
            p0,
            p1,
            p2,
            p3,
            weight: StrokeWeight::Thin,
        }
    }

    /// Get this stroke's weight class
    #[must_use]
    pub const fn weight(&self) -> StrokeWeight {
        match *self {
            Self::Line { weight, .. } | Self::Bezier { weight, .. } => weight,
        }
    }

    /// Resolve this stroke's half-width in em units through the pen
    #[must_use]
    pub const fn thickness(&self, pen: &PenModel) -> f32 {
        match self.weight() {
            StrokeWeight::Regular => pen.half_width,
            StrokeWeight::Thick => pen.thick_half_width,
            StrokeWeight::Thin => pen.thin_half_width,
        }
    }

    /// Convert this stroke to a `Sdf2dNode`, applying `pen`'s slant to all points
    ///
    /// Italic (`pen.slant_tan > 0`) では全 endpoint / control point が
    /// `PenModel::skew_point` で horizontally shift される
    #[must_use]
    pub fn to_sdf2d(&self, pen: &PenModel) -> Sdf2dNode {
        let thickness = self.thickness(pen);
        match *self {
            Self::Line { a, b, .. } => Sdf2dNode::Line {
                a: pen.skew_point(a),
                b: pen.skew_point(b),
                thickness,
            },
            Self::Bezier { p0, p1, p2, p3, .. } => Sdf2dNode::Bezier {
                p0: pen.skew_point(p0),
                p1: pen.skew_point(p1),
                p2: pen.skew_point(p2),
                p3: pen.skew_point(p3),
                thickness,
            },
        }
    }

    /// Tessellate this stroke into a chain of line segments (for 3D extrude path)
    ///
    /// `Line` は自身の 1 segment を返す `Bezier` は cubic curve を `n_samples`
    /// segment (端点 `n_samples + 1` 点) に分割する `n_samples` は最低 2 に clamp
    ///
    /// 返される点は em coordinate (skew 未適用) skew は caller 側で
    /// `PenModel::skew_point` を適用する
    #[must_use]
    pub fn sample_segments(&self, n_samples: u32) -> Vec<([f32; 2], [f32; 2])> {
        match *self {
            Self::Line { a, b, .. } => vec![(a, b)],
            Self::Bezier { p0, p1, p2, p3, .. } => {
                let n = n_samples.max(2);
                let mut segs = Vec::with_capacity(n as usize);
                let mut prev = p0;
                for i in 1..=n {
                    #[allow(clippy::cast_precision_loss)]
                    let t = f32::from(u16::try_from(i).unwrap_or(u16::MAX))
                        / f32::from(u16::try_from(n).unwrap_or(u16::MAX));
                    let curr = cubic_bezier_at(p0, p1, p2, p3, t);
                    segs.push((prev, curr));
                    prev = curr;
                }
                segs
            }
        }
    }
}

/// Evaluate cubic Bezier at parameter `t` (`0.0 ≤ t ≤ 1.0`)
///
/// Standard cubic Bernstein basis:
/// `B(t) = (1-t)³ P₀ + 3(1-t)² t P₁ + 3(1-t) t² P₂ + t³ P₃`
#[must_use]
pub fn cubic_bezier_at(
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    p3: [f32; 2],
    t: f32,
) -> [f32; 2] {
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    let t2 = t * t;
    let b0 = mt2 * mt;
    let b1 = 3.0 * mt2 * t;
    let b2 = 3.0 * mt * t2;
    let b3 = t2 * t;
    [
        b3.mul_add(
            p3[0],
            b2.mul_add(p2[0], b1.mul_add(p1[0], b0 * p0[0])),
        ),
        b3.mul_add(
            p3[1],
            b2.mul_add(p2[1], b1.mul_add(p1[1], b0 * p0[1])),
        ),
    ]
}

/// Convert a slice of `Stroke` to a single `Sdf2dNode` via left-fold `Union`
///
/// 空 slice は `None` を返す (empty glyph、caller が明示的に扱う責務)
/// 単一 stroke なら `Union` で wrap せず、その stroke の `to_sdf2d` をそのまま返す
#[must_use]
pub fn strokes_to_sdf2d(strokes: &[Stroke], pen: &PenModel) -> Option<Sdf2dNode> {
    let mut iter = strokes.iter().map(|s| s.to_sdf2d(pen));
    let first = iter.next()?;
    Some(iter.fold(first, |acc, next| {
        Sdf2dNode::Union(Box::new(acc), Box::new(next))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetaFontParams;

    fn regular_pen() -> PenModel {
        PenModel::from_params(&MetaFontParams::sans_regular())
    }

    fn italic_pen() -> PenModel {
        PenModel::from_params(&MetaFontParams::serif_italic())
    }

    fn high_contrast_pen() -> PenModel {
        PenModel::from_params(&MetaFontParams::serif_regular())
    }

    #[test]
    fn line_convenience_has_regular_weight() {
        let s = Stroke::line([0.0, 0.0], [1.0, 1.0]);
        assert_eq!(s.weight(), StrokeWeight::Regular);
    }

    #[test]
    fn line_thick_convenience_has_thick_weight() {
        let s = Stroke::line_thick([0.0, 0.0], [1.0, 1.0]);
        assert_eq!(s.weight(), StrokeWeight::Thick);
    }

    #[test]
    fn line_thin_convenience_has_thin_weight() {
        let s = Stroke::line_thin([0.0, 0.0], [1.0, 1.0]);
        assert_eq!(s.weight(), StrokeWeight::Thin);
    }

    #[test]
    fn bezier_convenience_variants_have_correct_weights() {
        assert_eq!(
            Stroke::bezier([0.0, 0.0], [0.3, 0.3], [0.7, 0.3], [1.0, 0.0]).weight(),
            StrokeWeight::Regular
        );
        assert_eq!(
            Stroke::bezier_thick([0.0, 0.0], [0.3, 0.3], [0.7, 0.3], [1.0, 0.0]).weight(),
            StrokeWeight::Thick
        );
        assert_eq!(
            Stroke::bezier_thin([0.0, 0.0], [0.3, 0.3], [0.7, 0.3], [1.0, 0.0]).weight(),
            StrokeWeight::Thin
        );
    }

    #[test]
    fn thickness_regular_matches_pen_half_width() {
        let pen = high_contrast_pen();
        let s = Stroke::line([0.0, 0.0], [1.0, 0.0]);
        assert!((s.thickness(&pen) - pen.half_width).abs() < 1e-6);
    }

    #[test]
    fn thickness_thick_matches_pen_thick_half_width() {
        let pen = high_contrast_pen();
        let s = Stroke::line_thick([0.0, 0.0], [1.0, 0.0]);
        assert!((s.thickness(&pen) - pen.thick_half_width).abs() < 1e-6);
        assert!(
            s.thickness(&pen) > pen.half_width,
            "thick ({}) must exceed regular ({}) for high-contrast pen",
            s.thickness(&pen),
            pen.half_width
        );
    }

    #[test]
    fn thickness_thin_matches_pen_thin_half_width() {
        let pen = high_contrast_pen();
        let s = Stroke::line_thin([0.0, 0.0], [1.0, 0.0]);
        assert!((s.thickness(&pen) - pen.thin_half_width).abs() < 1e-6);
        assert!(
            s.thickness(&pen) < pen.half_width,
            "thin ({}) must be < regular ({}) for high-contrast pen",
            s.thickness(&pen),
            pen.half_width
        );
    }

    #[test]
    fn to_sdf2d_line_upright_pen_preserves_endpoints() {
        let pen = regular_pen();
        let s = Stroke::line([0.1, 0.2], [0.8, 0.9]);
        let node = s.to_sdf2d(&pen);
        match node {
            Sdf2dNode::Line { a, b, thickness } => {
                assert!((a[0] - 0.1).abs() < 1e-6);
                assert!((a[1] - 0.2).abs() < 1e-6);
                assert!((b[0] - 0.8).abs() < 1e-6);
                assert!((b[1] - 0.9).abs() < 1e-6);
                assert!((thickness - pen.half_width).abs() < 1e-6);
            }
            other => panic!("expected Sdf2dNode::Line, got {other:?}"),
        }
    }

    #[test]
    fn to_sdf2d_line_italic_pen_skews_endpoints() {
        let pen = italic_pen();
        let s = Stroke::line([0.0, 0.0], [0.0, 1.0]);
        let node = s.to_sdf2d(&pen);
        match node {
            Sdf2dNode::Line { a, b, .. } => {
                // start at y=0 → no shift
                assert!(a[0].abs() < 1e-6, "italic pen at y=0 should not shift x");
                // end at y=1 → shift by tan(0.21) ≈ 0.213
                assert!(
                    b[0] > 0.15 && b[0] < 0.25,
                    "italic pen at y=1 should shift x by ~0.21, got {}",
                    b[0]
                );
            }
            other => panic!("expected Sdf2dNode::Line, got {other:?}"),
        }
    }

    #[test]
    fn to_sdf2d_bezier_upright_pen_preserves_control_points() {
        let pen = regular_pen();
        let s = Stroke::bezier([0.0, 0.0], [0.3, 0.5], [0.7, 0.5], [1.0, 0.0]);
        let node = s.to_sdf2d(&pen);
        match node {
            Sdf2dNode::Bezier {
                p0,
                p1,
                p2,
                p3,
                thickness,
            } => {
                assert!((p0[0] - 0.0).abs() < 1e-6);
                assert!((p1[0] - 0.3).abs() < 1e-6);
                assert!((p2[0] - 0.7).abs() < 1e-6);
                assert!((p3[0] - 1.0).abs() < 1e-6);
                assert!((thickness - pen.half_width).abs() < 1e-6);
            }
            other => panic!("expected Sdf2dNode::Bezier, got {other:?}"),
        }
    }

    #[test]
    fn to_sdf2d_bezier_italic_pen_skews_all_points() {
        let pen = italic_pen();
        // All at y=1.0 → all shift by same amount
        let s = Stroke::bezier([0.0, 1.0], [0.25, 1.0], [0.5, 1.0], [1.0, 1.0]);
        let node = s.to_sdf2d(&pen);
        match node {
            Sdf2dNode::Bezier { p0, p1, p2, p3, .. } => {
                let shift = pen.slant_tan; // y=1 shift
                assert!((p0[0] - (0.0 + shift)).abs() < 1e-5);
                assert!((p1[0] - (0.25 + shift)).abs() < 1e-5);
                assert!((p2[0] - (0.5 + shift)).abs() < 1e-5);
                assert!((p3[0] - (1.0 + shift)).abs() < 1e-5);
            }
            other => panic!("expected Sdf2dNode::Bezier, got {other:?}"),
        }
    }

    #[test]
    fn strokes_to_sdf2d_empty_returns_none() {
        let pen = regular_pen();
        assert!(strokes_to_sdf2d(&[], &pen).is_none());
    }

    #[test]
    fn strokes_to_sdf2d_single_no_wrap() {
        let pen = regular_pen();
        let s = [Stroke::line([0.0, 0.0], [1.0, 0.0])];
        let node = strokes_to_sdf2d(&s, &pen).expect("non-empty");
        // single stroke should not be wrapped in Union
        assert!(matches!(node, Sdf2dNode::Line { .. }));
    }

    #[test]
    fn strokes_to_sdf2d_two_returns_union() {
        let pen = regular_pen();
        let s = [
            Stroke::line([0.0, 0.0], [0.5, 1.0]),
            Stroke::line([0.5, 1.0], [1.0, 0.0]),
        ];
        let node = strokes_to_sdf2d(&s, &pen).expect("non-empty");
        assert!(matches!(node, Sdf2dNode::Union(_, _)));
    }

    #[test]
    fn strokes_to_sdf2d_three_left_folded() {
        let pen = regular_pen();
        // 'A' の 3 stroke: 左脚 + 右脚 + 横棒
        let s = [
            Stroke::line([0.0, 0.0], [0.5, 0.7]),
            Stroke::line([0.5, 0.7], [1.0, 0.0]),
            Stroke::line([0.2, 0.3], [0.8, 0.3]),
        ];
        let node = strokes_to_sdf2d(&s, &pen).expect("non-empty");
        // 左結合: Union(Union(l1, l2), l3)
        match node {
            Sdf2dNode::Union(inner, last) => {
                assert!(matches!(*inner, Sdf2dNode::Union(_, _)));
                assert!(matches!(*last, Sdf2dNode::Line { .. }));
            }
            other => panic!("expected Union, got {other:?}"),
        }
    }

    #[test]
    fn eval_at_stroke_center_is_near_zero() {
        // sanity: 実 SDF 評価で stroke 中心が hollow でないことを verify
        let pen = regular_pen();
        let s = Stroke::line([0.0, 0.0], [1.0, 0.0]);
        let node = s.to_sdf2d(&pen);
        let d = alice_sdf::sdf2d::eval_2d(&node, [0.5, 0.0]);
        assert!(
            d <= 0.0,
            "stroke center (0.5, 0.0) should be inside SDF (d ≤ 0), got {d}"
        );
    }

    #[test]
    fn eval_far_from_stroke_is_positive() {
        let pen = regular_pen();
        let s = Stroke::line([0.0, 0.0], [1.0, 0.0]);
        let node = s.to_sdf2d(&pen);
        let d = alice_sdf::sdf2d::eval_2d(&node, [0.5, 5.0]);
        assert!(
            d > 0.0,
            "far from stroke (0.5, 5.0) should be outside SDF (d > 0), got {d}"
        );
    }

    #[test]
    fn sample_segments_line_returns_single() {
        let s = Stroke::line([0.0, 0.0], [1.0, 0.5]);
        let segs = s.sample_segments(10);
        assert_eq!(segs.len(), 1);
        assert!((segs[0].0[0] - 0.0).abs() < 1e-6);
        assert!((segs[0].1[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn sample_segments_bezier_returns_n_segments() {
        let s = Stroke::bezier([0.0, 0.0], [0.3, 0.5], [0.7, 0.5], [1.0, 0.0]);
        let segs = s.sample_segments(8);
        assert_eq!(segs.len(), 8);
    }

    #[test]
    fn sample_segments_bezier_endpoints_match_p0_p3() {
        let s = Stroke::bezier([0.1, 0.2], [0.3, 0.9], [0.7, 0.9], [0.9, 0.2]);
        let segs = s.sample_segments(16);
        // first segment starts at p0
        assert!((segs[0].0[0] - 0.1).abs() < 1e-4);
        assert!((segs[0].0[1] - 0.2).abs() < 1e-4);
        // last segment ends at p3
        let last = segs.last().expect("non-empty");
        assert!((last.1[0] - 0.9).abs() < 1e-4);
        assert!((last.1[1] - 0.2).abs() < 1e-4);
    }

    #[test]
    fn sample_segments_bezier_chain_is_continuous() {
        // 各 segment の終点 = 次 segment の始点
        let s = Stroke::bezier([0.0, 0.0], [0.5, 1.0], [1.0, 1.0], [1.5, 0.0]);
        let segs = s.sample_segments(6);
        for w in segs.windows(2) {
            assert!(
                (w[0].1[0] - w[1].0[0]).abs() < 1e-5
                    && (w[0].1[1] - w[1].0[1]).abs() < 1e-5,
                "chain discontinuity: seg[i].end {:?} != seg[i+1].start {:?}",
                w[0].1,
                w[1].0
            );
        }
    }

    #[test]
    fn sample_segments_bezier_clamps_n_below_2() {
        let s = Stroke::bezier([0.0, 0.0], [0.3, 0.5], [0.7, 0.5], [1.0, 0.0]);
        assert_eq!(s.sample_segments(0).len(), 2);
        assert_eq!(s.sample_segments(1).len(), 2);
    }

    #[test]
    fn cubic_bezier_at_endpoints() {
        let p0 = [0.0, 0.0];
        let p1 = [0.3, 0.5];
        let p2 = [0.7, 0.5];
        let p3 = [1.0, 0.0];
        let at0 = cubic_bezier_at(p0, p1, p2, p3, 0.0);
        let at1 = cubic_bezier_at(p0, p1, p2, p3, 1.0);
        assert!((at0[0] - 0.0).abs() < 1e-6);
        assert!((at0[1] - 0.0).abs() < 1e-6);
        assert!((at1[0] - 1.0).abs() < 1e-6);
        assert!((at1[1] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn cubic_bezier_at_midpoint_is_symmetric_curve() {
        let mid = cubic_bezier_at([0.0, 0.0], [0.3, 0.5], [0.7, 0.5], [1.0, 0.0], 0.5);
        // symmetric curve: mid.x = 0.5, mid.y = 3/8
        assert!(
            (mid[0] - 0.5).abs() < 1e-5,
            "symmetric bezier at t=0.5: mid.x = 0.5, got {}",
            mid[0]
        );
        assert!(mid[1] > 0.3 && mid[1] < 0.4, "mid.y ≈ 0.375, got {}", mid[1]);
    }
}
