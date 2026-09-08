//! `PenModel` — stroke thickness + italic slant を [`MetaFontParams`] から抽出する adapter
//!
//! Phase 2 Law の 10 axis から、描画時に必要な 4 scalar (`half_width` /
//! `thick_half_width` / `thin_half_width` / `slant_tan`) を事前計算して保持する
//! `Stroke::to_sdf2d` から高頻度に参照されるため、`tan(slant)` は 1 回だけ計算
//!
//! [`MetaFontParams`]: crate::MetaFontParams

use crate::MetaFontParams;

/// Stroke pen configuration derived from `MetaFontParams`
///
/// 全 field は em unit の f32 `PenModel::from_params` で `MetaFontParams` から
/// 一度計算し、その後は immutable に glyph 描画全域で共有する
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PenModel {
    /// Base stroke half-width (weight-driven、regular stroke に使う)
    pub half_width: f32,
    /// Thick stroke half-width (contrast > 0 で `half_width` より太い、serif 縦棒等)
    pub thick_half_width: f32,
    /// Thin stroke half-width (contrast > 0 で `half_width` より細い、serif 横棒等)
    pub thin_half_width: f32,
    /// Precomputed `tan(slant)` — italic skew の horizontal shift 用
    ///
    /// 実用範囲 (slant ≤ 0.5 rad ≈ 28°) では `tan` は有限、`f32` で表現可
    pub slant_tan: f32,
}

impl PenModel {
    /// `MetaFontParams` から `PenModel` を構築
    ///
    /// `tan(slant)` は 1 回だけ計算し、以後の skew 適用で reuse する
    #[must_use]
    pub fn from_params(params: &MetaFontParams) -> Self {
        Self {
            half_width: params.stroke_half_width(),
            thick_half_width: params.thick_half_width(),
            thin_half_width: params.thin_half_width(),
            slant_tan: params.slant.tan(),
        }
    }

    /// Apply italic skew to a 2D point
    ///
    /// 高さ (y) に比例した horizontal shift を適用 slant が 0 なら no-op
    /// upright (`slant = 0.0`) では 常に `p` をそのまま返す
    #[must_use]
    pub fn skew_point(&self, p: [f32; 2]) -> [f32; 2] {
        let shift = self.slant_tan * p[1];
        [p[0] + shift, p[1]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_sans_regular_preserves_half_width() {
        let params = MetaFontParams::sans_regular();
        let pen = PenModel::from_params(&params);
        assert!(
            (pen.half_width - params.stroke_half_width()).abs() < 1e-6,
            "PenModel half_width must match params.stroke_half_width()"
        );
    }

    #[test]
    fn from_serif_regular_thick_exceeds_thin() {
        let params = MetaFontParams::serif_regular();
        let pen = PenModel::from_params(&params);
        assert!(
            pen.thick_half_width > pen.thin_half_width,
            "serif high-contrast: thick ({}) must exceed thin ({})",
            pen.thick_half_width,
            pen.thin_half_width
        );
    }

    #[test]
    fn from_mono_regular_thick_equals_thin() {
        let params = MetaFontParams::mono_regular();
        let pen = PenModel::from_params(&params);
        assert!(
            (pen.thick_half_width - pen.thin_half_width).abs() < 1e-4,
            "mono zero-contrast: thick ({}) must equal thin ({})",
            pen.thick_half_width,
            pen.thin_half_width
        );
    }

    #[test]
    fn upright_pen_has_zero_slant_tan() {
        let params = MetaFontParams::sans_regular();
        let pen = PenModel::from_params(&params);
        assert!(
            pen.slant_tan.abs() < 1e-6,
            "upright pen must have slant_tan ≈ 0"
        );
    }

    #[test]
    fn italic_pen_has_positive_slant_tan() {
        let params = MetaFontParams::serif_italic();
        let pen = PenModel::from_params(&params);
        assert!(
            pen.slant_tan > 0.15,
            "italic pen slant_tan ({}) must be > 0.15 (params.slant ≈ 0.21 rad)",
            pen.slant_tan
        );
    }

    #[test]
    fn skew_point_at_baseline_unchanged() {
        let pen = PenModel::from_params(&MetaFontParams::serif_italic());
        let skewed = pen.skew_point([0.5, 0.0]);
        assert!((skewed[0] - 0.5).abs() < 1e-6);
        assert!((skewed[1] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn skew_point_at_cap_height_shifts_horizontally() {
        let pen = PenModel::from_params(&MetaFontParams::serif_italic());
        let skewed = pen.skew_point([0.0, 1.0]);
        // slant ≈ 0.21 rad → tan ≈ 0.213
        assert!(
            skewed[0] > 0.15 && skewed[0] < 0.25,
            "italic skew at y=1.0 should shift x by ~0.21, got {}",
            skewed[0]
        );
        assert!((skewed[1] - 1.0).abs() < 1e-6, "y must be preserved");
    }

    #[test]
    fn skew_point_upright_pen_no_shift() {
        let pen = PenModel::from_params(&MetaFontParams::sans_regular());
        let skewed = pen.skew_point([0.3, 0.7]);
        assert!((skewed[0] - 0.3).abs() < 1e-6);
        assert!((skewed[1] - 0.7).abs() < 1e-6);
    }

    #[test]
    fn hairline_thinner_than_regular_pen() {
        let hairline = PenModel::from_params(&MetaFontParams::sans_hairline());
        let regular = PenModel::from_params(&MetaFontParams::sans_regular());
        assert!(
            hairline.half_width < regular.half_width,
            "hairline pen half_width ({}) must be < regular ({})",
            hairline.half_width,
            regular.half_width
        );
    }

    #[test]
    fn bold_thicker_than_regular_pen() {
        let bold = PenModel::from_params(&MetaFontParams::sans_bold());
        let regular = PenModel::from_params(&MetaFontParams::sans_regular());
        assert!(
            bold.half_width > regular.half_width,
            "bold pen half_width ({}) must be > regular ({})",
            bold.half_width,
            regular.half_width
        );
    }

    #[test]
    fn display_heavy_thickest_pen() {
        let display = PenModel::from_params(&MetaFontParams::display_heavy());
        let bold = PenModel::from_params(&MetaFontParams::sans_bold());
        assert!(
            display.half_width > bold.half_width,
            "display_heavy pen ({}) must be > sans_bold ({})",
            display.half_width,
            bold.half_width
        );
    }
}
