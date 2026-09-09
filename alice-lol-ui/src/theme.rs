//! Theme — color palette + golden ratio spacing / radius scale
//!
//! # golden ratio
//!
//! spacing / radius は φ = 1.6180339887 の冪乗を base に乗算 (`golden-ratio-design` skill 準拠)
//! `spacing_base * φⁿ` で n-th step、視覚的に調和のとれた scale が得られる

/// 黄金比 φ ≈ 1.6180339887
pub const PHI: f32 = 1.618_034;

/// Theme — 色 + spacing / radius base scale
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// 背景色 sRGB [0, 1]
    pub bg: [f32; 3],
    /// 前景色 (テキスト) sRGB [0, 1]
    pub fg: [f32; 3],
    /// アクセント色 sRGB [0, 1]
    pub accent: [f32; 3],
    /// spacing base [CSS px 相当]、`spacing(n) = base * φⁿ`
    pub spacing_base: f32,
    /// radius base [CSS px 相当]、`radius(n) = base * φⁿ`
    pub radius_base: f32,
}

impl Theme {
    /// Light theme preset (WCAG AA 準拠、white bg + near-black fg + brand blue accent)
    #[must_use]
    pub const fn light() -> Self {
        Self {
            bg: [1.0, 1.0, 1.0],                       // #ffffff
            fg: [0.117_647, 0.117_647, 0.117_647],     // #1e1e1e
            accent: [0.156_863, 0.470_588, 0.933_333], // #2878ee
            spacing_base: 4.0,
            radius_base: 4.0,
        }
    }

    /// Dark theme preset (WCAG AA 準拠、near-black bg + light-gray fg + cyan accent)
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            bg: [0.070_588, 0.070_588, 0.070_588],     // #121212
            fg: [0.878_431, 0.878_431, 0.878_431],     // #e0e0e0
            accent: [0.156_863, 0.803_922, 0.905_882], // #28cde7
            spacing_base: 4.0,
            radius_base: 4.0,
        }
    }

    /// High-contrast theme preset (WCAG AAA 準拠、pure black bg + pure white fg + yellow accent)
    #[must_use]
    pub const fn high_contrast() -> Self {
        Self {
            bg: [0.0, 0.0, 0.0],
            fg: [1.0, 1.0, 1.0],
            accent: [1.0, 0.933_333, 0.0], // #ffee00
            spacing_base: 4.0,
            radius_base: 4.0,
        }
    }

    /// n-th spacing step (`spacing_base * φⁿ`)
    ///
    /// - n=0 → 4 px
    /// - n=1 → 6.5 px
    /// - n=2 → 10.5 px
    /// - n=3 → 17 px
    /// - n=-1 → 2.5 px
    #[must_use]
    pub fn spacing(&self, n: i32) -> f32 {
        self.spacing_base * PHI.powi(n)
    }

    /// n-th radius step
    #[must_use]
    pub fn radius(&self, n: i32) -> f32 {
        self.radius_base * PHI.powi(n)
    }
}

/// golden ratio step 単独 helper (`PHI.powi(n)` の shorthand)
#[must_use]
pub fn golden_ratio_step(n: i32) -> f32 {
    PHI.powi(n)
}

/// WCAG relative luminance (sRGB → luminance) — a11y contrast 計算に使う
///
/// 参照: <https://www.w3.org/WAI/GL/wiki/Relative_luminance>
#[must_use]
pub fn relative_luminance(color: [f32; 3]) -> f32 {
    fn channel(c: f32) -> f32 {
        if c <= 0.039_285 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    let r = channel(color[0]);
    let g = channel(color[1]);
    let b = channel(color[2]);
    0.2126_f32.mul_add(r, 0.7152_f32.mul_add(g, 0.0722 * b))
}

/// WCAG contrast ratio (fg vs bg、1:1 〜 21:1 の範囲)
///
/// - 4.5 以上 = WCAG AA (normal text)
/// - 3.0 以上 = WCAG AA (large text ≥ 18pt / 14pt bold)
/// - 7.0 以上 = WCAG AAA (normal text)
#[must_use]
pub fn contrast_ratio(fg: [f32; 3], bg: [f32; 3]) -> f32 {
    let l1 = relative_luminance(fg);
    let l2 = relative_luminance(bg);
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phi_value_is_golden_ratio() {
        assert!((PHI - 1.618_034).abs() < 1e-5);
    }

    #[test]
    fn light_theme_has_white_bg_and_dark_fg() {
        let t = Theme::light();
        assert!(t.bg[0] > 0.9);
        assert!(t.fg[0] < 0.2);
    }

    #[test]
    fn dark_theme_has_dark_bg_and_light_fg() {
        let t = Theme::dark();
        assert!(t.bg[0] < 0.2);
        assert!(t.fg[0] > 0.8);
    }

    fn arr3_close(a: [f32; 3], b: [f32; 3], eps: f32) -> bool {
        (a[0] - b[0]).abs() < eps && (a[1] - b[1]).abs() < eps && (a[2] - b[2]).abs() < eps
    }

    #[test]
    fn high_contrast_theme_is_black_and_white() {
        let t = Theme::high_contrast();
        assert!(arr3_close(t.bg, [0.0, 0.0, 0.0], 1e-6));
        assert!(arr3_close(t.fg, [1.0, 1.0, 1.0], 1e-6));
    }

    #[test]
    fn spacing_scales_by_phi() {
        let t = Theme::light();
        let expect_1 = PHI * 4.0;
        let expect_2 = PHI * PHI * 4.0;
        assert!((t.spacing(0) - 4.0).abs() < 1e-5);
        assert!((t.spacing(1) - expect_1).abs() < 1e-5);
        assert!((t.spacing(2) - expect_2).abs() < 1e-4);
    }

    #[test]
    fn radius_scales_by_phi() {
        let t = Theme::light();
        assert!((t.radius(-1) - 4.0 / PHI).abs() < 1e-5);
        assert!((t.radius(0) - 4.0).abs() < 1e-5);
    }

    #[test]
    fn golden_ratio_step_helper_matches_phi_powi() {
        assert!((golden_ratio_step(0) - 1.0).abs() < 1e-5);
        assert!((golden_ratio_step(1) - PHI).abs() < 1e-5);
        assert!((golden_ratio_step(-1) - 1.0 / PHI).abs() < 1e-5);
    }

    // ─── luminance / contrast ratio ───

    #[test]
    fn white_luminance_is_one() {
        assert!((relative_luminance([1.0, 1.0, 1.0]) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn black_luminance_is_zero() {
        assert!(relative_luminance([0.0, 0.0, 0.0]).abs() < 1e-6);
    }

    #[test]
    fn white_black_contrast_is_21() {
        let ratio = contrast_ratio([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]);
        assert!((ratio - 21.0).abs() < 0.1);
    }

    #[test]
    fn same_color_contrast_is_one() {
        let ratio = contrast_ratio([0.5, 0.5, 0.5], [0.5, 0.5, 0.5]);
        assert!((ratio - 1.0).abs() < 1e-5);
    }

    #[test]
    fn light_theme_meets_wcag_aa() {
        let t = Theme::light();
        let ratio = contrast_ratio(t.fg, t.bg);
        assert!(
            ratio >= 4.5,
            "light theme fg/bg contrast {ratio:.2} < AA 4.5"
        );
    }

    #[test]
    fn dark_theme_meets_wcag_aa() {
        let t = Theme::dark();
        let ratio = contrast_ratio(t.fg, t.bg);
        assert!(
            ratio >= 4.5,
            "dark theme fg/bg contrast {ratio:.2} < AA 4.5"
        );
    }

    #[test]
    fn high_contrast_theme_meets_wcag_aaa() {
        let t = Theme::high_contrast();
        let ratio = contrast_ratio(t.fg, t.bg);
        assert!(
            ratio >= 7.0,
            "high_contrast theme fg/bg {ratio:.2} < AAA 7.0"
        );
    }
}
