//! `Glyph` — 単一文字の Stroke 集合 + advance metric + `Sdf2dNode` / `SdfNode` 変換
//!
//! Font Phase 2 Law → SDF conversion の中核 単一文字を em 座標系の Stroke 列で
//! 表現し、`PenModel` と組合せて 2D SDF (`to_sdf2d`) or 3D 押出 SDF (`to_sdf3d`) を出力
//!
//! # 3D 押出戦略
//!
//! Bezier curve の 3D 押出には alice-sdf に native primitive がないため、
//! `sample_segments` で Bezier を N line segment に細分化し、各 segment を
//! `SdfNode::Segment2D { half_height }` (= 2D 押出 primitive) に変換して
//! Union fold する `DEFAULT_BEZIER_SAMPLES = 12` は視覚的に滑らかな下限
//!
//! # 使用例
//!
//! ```
//! use alice_lol_font::{Glyph, MetaFontParams, PenModel};
//! use alice_lol_font::stroke::Stroke;
//!
//! let params = MetaFontParams::sans_regular();
//! let pen = PenModel::from_params(&params);
//!
//! // 'A' 3 stroke で構築
//! let glyph_a = Glyph::new('A', vec![
//!     Stroke::line([0.0, 0.0], [0.5, 0.7]),
//!     Stroke::line([0.5, 0.7], [1.0, 0.0]),
//!     Stroke::line([0.2, 0.3], [0.8, 0.3]),
//! ], 1.0);
//!
//! let sdf2d = glyph_a.to_sdf2d(&pen).expect("non-empty glyph → Some");
//! let sdf3d = glyph_a.to_sdf3d(&pen, 0.1).expect("non-empty glyph → Some");
//! # let (_, _) = (sdf2d, sdf3d);
//! ```

use crate::pen::PenModel;
use crate::stroke::{strokes_to_sdf2d, Stroke};
use alice_sdf::sdf2d::Sdf2dNode;
use alice_sdf::types::SdfNode;
use glam::Vec2;
use std::sync::Arc;

/// Default cubic Bezier subdivision count for 3D extrude path
///
/// 12 subdivision で em unit スケール (< 1.0) の Bezier は肉眼で smooth
/// 高解像度が必要なら `to_sdf3d_with_samples` で override 可
pub const DEFAULT_BEZIER_SAMPLES: u32 = 12;

/// Font glyph — Unicode codepoint + Stroke 列 + advance metric
///
/// em 座標系: baseline `y=0`、`cap_height y=1.0` 想定 x は 0 から advance まで
#[derive(Debug, Clone)]
pub struct Glyph {
    codepoint: char,
    strokes: Vec<Stroke>,
    advance: f32,
}

impl Glyph {
    /// Build a `Glyph` from codepoint / strokes / advance width
    #[must_use]
    pub const fn new(codepoint: char, strokes: Vec<Stroke>, advance: f32) -> Self {
        Self {
            codepoint,
            strokes,
            advance,
        }
    }

    /// Unicode codepoint of this glyph
    #[must_use]
    pub const fn codepoint(&self) -> char {
        self.codepoint
    }

    /// Horizontal advance width in em units (次 glyph の x 位置 offset)
    #[must_use]
    pub const fn advance(&self) -> f32 {
        self.advance
    }

    /// Read-only view of this glyph's stroke skeleton
    #[must_use]
    pub fn strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    /// Stroke 数 (empty glyph check 用)
    #[must_use]
    pub const fn stroke_count(&self) -> usize {
        self.strokes.len()
    }

    /// Build a `Glyph` for an ASCII uppercase letter using built-in parametric definitions
    ///
    /// Phase 1-5 MVP scope: `A` / `H` / `I` / `O` / `T` (5 letter)
    /// Unsupported letters return `None`
    /// Case は preserve される (`'a'` は None)、`to_ascii_uppercase` は caller 側で
    ///
    /// # 使用例
    ///
    /// ```
    /// use alice_lol_font::{Glyph, MetaFontParams, PenModel};
    ///
    /// let pen = PenModel::from_params(&MetaFontParams::sans_bold());
    /// let letter = Glyph::ascii('A').expect("A supported");
    /// let sdf = letter.to_sdf3d(&pen, 0.1).expect("non-empty");
    /// # let _ = sdf;
    /// ```
    #[must_use]
    pub fn ascii(ch: char) -> Option<Self> {
        crate::ascii::ascii_uppercase(ch)
    }

    /// Convert to 2D SDF (`alice_sdf::Sdf2dNode`)
    ///
    /// Empty glyph (stroke 0) は `None` を返す italic slant は自動適用
    #[must_use]
    pub fn to_sdf2d(&self, pen: &PenModel) -> Option<Sdf2dNode> {
        strokes_to_sdf2d(&self.strokes, pen)
    }

    /// Convert to 3D extruded SDF (`alice_sdf::SdfNode`)
    ///
    /// `extrude_depth` は Z 軸方向の厚み (`half_height = extrude_depth * 0.5`)
    /// Bezier stroke は `DEFAULT_BEZIER_SAMPLES` line segment に細分化される
    /// Empty glyph は `None` を返す
    #[must_use]
    pub fn to_sdf3d(&self, pen: &PenModel, extrude_depth: f32) -> Option<SdfNode> {
        self.to_sdf3d_with_samples(pen, extrude_depth, DEFAULT_BEZIER_SAMPLES)
    }

    /// Convert to 3D with explicit Bezier subdivision count
    ///
    /// `bezier_samples` は minimum 2 に clamp される (`Stroke::sample_segments` 内)
    /// 高解像度が必要な glyph (curved script 等) では 24-32 推奨
    #[must_use]
    pub fn to_sdf3d_with_samples(
        &self,
        pen: &PenModel,
        extrude_depth: f32,
        bezier_samples: u32,
    ) -> Option<SdfNode> {
        let half_depth = extrude_depth * 0.5;
        let mut nodes: Vec<SdfNode> = Vec::new();

        for stroke in &self.strokes {
            let thickness = stroke.thickness(pen);
            for (a_pre, b_pre) in stroke.sample_segments(bezier_samples) {
                let a = pen.skew_point(a_pre);
                let b = pen.skew_point(b_pre);
                nodes.push(SdfNode::Segment2D {
                    a: Vec2::new(a[0], a[1]),
                    b: Vec2::new(b[0], b[1]),
                    thickness,
                    half_height: half_depth,
                });
            }
        }

        fold_union_sdf3d(nodes)
    }
}

/// Left-fold `Vec<SdfNode>` into a single `SdfNode::Union` tree
///
/// 空 vec は `None`、単一要素はそのまま返す (Union で wrap しない)
#[must_use]
fn fold_union_sdf3d(mut nodes: Vec<SdfNode>) -> Option<SdfNode> {
    if nodes.is_empty() {
        return None;
    }
    if nodes.len() == 1 {
        return nodes.pop();
    }
    let mut iter = nodes.into_iter();
    let first = iter.next()?;
    Some(iter.fold(first, |acc, next| SdfNode::Union {
        a: Arc::new(acc),
        b: Arc::new(next),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetaFontParams;

    fn regular_pen() -> PenModel {
        PenModel::from_params(&MetaFontParams::sans_regular())
    }

    fn glyph_a() -> Glyph {
        // 'A' の 3 stroke minimal skeleton
        Glyph::new(
            'A',
            vec![
                Stroke::line([0.0, 0.0], [0.5, 0.7]),
                Stroke::line([0.5, 0.7], [1.0, 0.0]),
                Stroke::line([0.2, 0.3], [0.8, 0.3]),
            ],
            1.0,
        )
    }

    fn glyph_o() -> Glyph {
        // 'O' の 4 Bezier で近似
        Glyph::new(
            'O',
            vec![
                // 上半 Bezier (左→右)
                Stroke::bezier([0.0, 0.35], [0.0, 0.7], [1.0, 0.7], [1.0, 0.35]),
                // 下半 Bezier (右→左)
                Stroke::bezier([1.0, 0.35], [1.0, 0.0], [0.0, 0.0], [0.0, 0.35]),
            ],
            1.0,
        )
    }

    fn empty_glyph() -> Glyph {
        Glyph::new(' ', Vec::new(), 0.5)
    }

    #[test]
    fn new_preserves_all_fields() {
        let g = glyph_a();
        assert_eq!(g.codepoint(), 'A');
        assert!((g.advance() - 1.0).abs() < 1e-6);
        assert_eq!(g.stroke_count(), 3);
    }

    #[test]
    fn empty_glyph_stroke_count_zero() {
        let g = empty_glyph();
        assert_eq!(g.stroke_count(), 0);
    }

    #[test]
    fn to_sdf2d_non_empty_returns_some() {
        let g = glyph_a();
        let pen = regular_pen();
        assert!(g.to_sdf2d(&pen).is_some());
    }

    #[test]
    fn to_sdf2d_empty_returns_none() {
        let g = empty_glyph();
        let pen = regular_pen();
        assert!(g.to_sdf2d(&pen).is_none());
    }

    #[test]
    fn to_sdf3d_non_empty_returns_some() {
        let g = glyph_a();
        let pen = regular_pen();
        assert!(g.to_sdf3d(&pen, 0.1).is_some());
    }

    #[test]
    fn to_sdf3d_empty_returns_none() {
        let g = empty_glyph();
        let pen = regular_pen();
        assert!(g.to_sdf3d(&pen, 0.1).is_none());
    }

    #[test]
    fn to_sdf3d_line_glyph_uses_segment2d() {
        let g = glyph_a(); // 3 lines
        let pen = regular_pen();
        let node = g.to_sdf3d(&pen, 0.1).expect("non-empty");
        // 3 stroke × 1 segment each = 3 Segment2D fold: Union(Union(s1, s2), s3)
        // 少なくとも root は Union
        assert!(matches!(node, SdfNode::Union { .. }));
    }

    #[test]
    fn to_sdf3d_bezier_glyph_expands_via_samples() {
        let g = glyph_o(); // 2 Bezier × 12 samples = 24 Segment2D
        let pen = regular_pen();
        let node = g.to_sdf3d(&pen, 0.1).expect("non-empty");
        // 24 segments Union fold = deep Union tree, root は Union
        assert!(matches!(node, SdfNode::Union { .. }));
    }

    #[test]
    fn to_sdf3d_extrude_depth_half_matches_half_height() {
        let g = glyph_a();
        let pen = regular_pen();
        let node = g.to_sdf3d(&pen, 0.2).expect("non-empty");
        // deep 内の Segment2D の half_height を pluck して verify
        // root は Union、最深部は Segment2D
        let mut cursor = &node;
        while let SdfNode::Union { a, .. } = cursor {
            cursor = a.as_ref();
        }
        if let SdfNode::Segment2D { half_height, .. } = cursor {
            assert!(
                (half_height - 0.1).abs() < 1e-6,
                "extrude_depth 0.2 → half_height 0.1, got {half_height}"
            );
        } else {
            panic!("expected leaf Segment2D, got {cursor:?}");
        }
    }

    fn count_segments(n: &SdfNode) -> u32 {
        match n {
            SdfNode::Union { a, b } => count_segments(a) + count_segments(b),
            SdfNode::Segment2D { .. } => 1,
            _ => 0,
        }
    }

    #[test]
    fn to_sdf3d_with_samples_higher_produces_more_nodes() {
        let g = glyph_o();
        let pen = regular_pen();
        let low = g.to_sdf3d_with_samples(&pen, 0.1, 4).expect("non-empty");
        let high = g.to_sdf3d_with_samples(&pen, 0.1, 32).expect("non-empty");
        let low_count = count_segments(&low);
        let high_count = count_segments(&high);
        assert!(
            high_count > low_count,
            "higher sample count ({high_count}) must exceed lower ({low_count})"
        );
    }

    #[test]
    fn to_sdf3d_eval_inside_stroke_gives_negative_distance() {
        let g = glyph_a();
        let pen = regular_pen();
        let node = g.to_sdf3d(&pen, 0.1).expect("non-empty");
        // stroke 1 の中点 (0.25, 0.35, 0.0) は 'A' 左脚上 = 内部
        let d = alice_sdf::eval(&node, glam::Vec3::new(0.25, 0.35, 0.0));
        assert!(d.is_finite(), "eval must be finite");
        // stroke thickness = pen.half_width ≈ 0.046 em → 中点近傍は SDF ≤ 0 or 極小
        assert!(d < 0.05, "point near stroke center should be near/inside (d < 0.05), got {d}");
    }

    #[test]
    fn to_sdf3d_eval_far_from_glyph_positive() {
        let g = glyph_a();
        let pen = regular_pen();
        let node = g.to_sdf3d(&pen, 0.1).expect("non-empty");
        let d = alice_sdf::eval(&node, glam::Vec3::new(5.0, 5.0, 5.0));
        assert!(d > 0.0, "far from glyph must have d > 0, got {d}");
    }

    #[test]
    fn advance_zero_glyph_still_encodable() {
        // combining diacritic 等: advance 0 でも valid
        let g = Glyph::new('\u{0301}', Vec::new(), 0.0);
        assert!((g.advance() - 0.0).abs() < 1e-6);
        assert_eq!(g.stroke_count(), 0);
    }
}
