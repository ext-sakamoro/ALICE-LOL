//! `UiLaw` — WCAG AA + iOS/Material touch target + radius sanity 静的検査
//!
//! # 3 rule
//!
//! - **`TouchTargetTooSmall`**: 最小 tap 面積 (default 44 CSS px、iOS HIG / Material 48 dp)
//! - **`ContrastTooLow`**: fg/bg WCAG contrast ratio < threshold (default AA 4.5)
//! - **`RadiusOversized`**: `corner_radius` > `min(w, h)/2` (視覚破綻 = 円形化しすぎ)

use crate::error::UiViolation;
use crate::theme::{contrast_ratio, Theme};

/// UI law rule set の閾値
#[derive(Debug, Clone, Copy)]
pub struct UiLaw {
    /// 最小 tap 面積 [CSS px] (default 44、iOS HIG)
    pub min_touch_target: f32,
    /// 最小 contrast ratio (default 4.5、WCAG AA normal text)
    pub min_contrast_normal: f32,
    /// 最小 contrast ratio (large text、default 3.0、WCAG AA large text ≥ 18pt / 14pt bold)
    pub min_contrast_large: f32,
}

impl UiLaw {
    /// WCAG AA + iOS HIG canonical default
    #[must_use]
    pub const fn default_wcag_aa() -> Self {
        Self {
            min_touch_target: 44.0,
            min_contrast_normal: 4.5,
            min_contrast_large: 3.0,
        }
    }

    /// WCAG AAA + Material 48 dp preset (higher bar)
    #[must_use]
    pub const fn strict_wcag_aaa() -> Self {
        Self {
            min_touch_target: 48.0,
            min_contrast_normal: 7.0,
            min_contrast_large: 4.5,
        }
    }

    /// Button 単体検査 (dimension + optional theme fg/bg contrast)
    #[must_use]
    pub fn check_button(&self, width: f32, height: f32, corner_radius: f32) -> Vec<UiViolation> {
        let mut violations = Vec::new();
        self.check_touch_target(width, height, "Button", &mut violations);
        self.check_radius(width, height, corner_radius, "Button", &mut violations);
        violations
    }

    /// `InputField` 単体検査
    #[must_use]
    pub fn check_input_field(
        &self,
        width: f32,
        height: f32,
        corner_radius: f32,
    ) -> Vec<UiViolation> {
        let mut violations = Vec::new();
        self.check_touch_target(width, height, "InputField", &mut violations);
        self.check_radius(width, height, corner_radius, "InputField", &mut violations);
        violations
    }

    /// Card 単体検査 (touch target は cardsize 通常大なので skip、radius のみ)
    #[must_use]
    pub fn check_card(&self, width: f32, height: f32, corner_radius: f32) -> Vec<UiViolation> {
        let mut violations = Vec::new();
        self.check_radius(width, height, corner_radius, "Card", &mut violations);
        violations
    }

    /// Chip 単体検査 (pill 形、touch target 44 min の場合小型 chip は fail 想定)
    #[must_use]
    pub fn check_chip(&self, width: f32, height: f32) -> Vec<UiViolation> {
        let mut violations = Vec::new();
        self.check_touch_target(width, height, "Chip", &mut violations);
        violations
    }

    /// Theme contrast 検査 (fg vs bg normal text)
    #[must_use]
    pub fn check_theme_contrast(&self, theme: &Theme, is_large_text: bool) -> Vec<UiViolation> {
        let mut violations = Vec::new();
        let ratio = contrast_ratio(theme.fg, theme.bg);
        let min = if is_large_text {
            self.min_contrast_large
        } else {
            self.min_contrast_normal
        };
        if ratio < min {
            violations.push(UiViolation {
                rule: "ContrastTooLow",
                detail: format!(
                    "fg/bg contrast {ratio:.2}:1 < min {min:.1}:1 ({} text)",
                    if is_large_text { "large" } else { "normal" }
                ),
            });
        }
        violations
    }

    /// accent 色 contrast 検査 (accent vs bg、button surface 等の visibility)
    #[must_use]
    pub fn check_accent_contrast(&self, theme: &Theme, is_large_text: bool) -> Vec<UiViolation> {
        let mut violations = Vec::new();
        let ratio = contrast_ratio(theme.accent, theme.bg);
        let min = if is_large_text {
            self.min_contrast_large
        } else {
            self.min_contrast_normal
        };
        if ratio < min {
            violations.push(UiViolation {
                rule: "ContrastTooLow",
                detail: format!(
                    "accent/bg contrast {ratio:.2}:1 < min {min:.1}:1 ({} text)",
                    if is_large_text { "large" } else { "normal" }
                ),
            });
        }
        violations
    }

    // ─── internal check helpers ───

    fn check_touch_target(
        &self,
        width: f32,
        height: f32,
        label: &'static str,
        violations: &mut Vec<UiViolation>,
    ) {
        let min_side = width.min(height);
        if min_side < self.min_touch_target {
            violations.push(UiViolation {
                rule: "TouchTargetTooSmall",
                detail: format!(
                    "{label} min side {min_side:.1} px < min {} px",
                    self.min_touch_target
                ),
            });
        }
    }

    #[allow(clippy::unused_self)] // self は使わないが `UiLaw` の method 一貫性のため
    fn check_radius(
        &self,
        width: f32,
        height: f32,
        corner_radius: f32,
        label: &'static str,
        violations: &mut Vec<UiViolation>,
    ) {
        let max_r = width.min(height) * 0.5;
        // pill 形 (radius = height/2) は正当なので strict overflow のみ拒否
        if corner_radius > max_r + 1e-3 {
            violations.push(UiViolation {
                rule: "RadiusOversized",
                detail: format!(
                    "{label} corner_radius {corner_radius:.1} > min(w,h)/2 = {max_r:.1}"
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── default_wcag_aa ───

    #[test]
    fn default_wcag_aa_uses_44px_and_4_5_contrast() {
        let law = UiLaw::default_wcag_aa();
        assert!((law.min_touch_target - 44.0).abs() < 1e-5);
        assert!((law.min_contrast_normal - 4.5).abs() < 1e-5);
        assert!((law.min_contrast_large - 3.0).abs() < 1e-5);
    }

    #[test]
    fn strict_wcag_aaa_uses_48px_and_7_0_contrast() {
        let law = UiLaw::strict_wcag_aaa();
        assert!((law.min_touch_target - 48.0).abs() < 1e-5);
        assert!((law.min_contrast_normal - 7.0).abs() < 1e-5);
    }

    // ─── Button touch target ───

    #[test]
    fn button_44px_min_side_passes() {
        let law = UiLaw::default_wcag_aa();
        let violations = law.check_button(120.0, 44.0, 8.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn button_32px_min_side_triggers_touch_target() {
        let law = UiLaw::default_wcag_aa();
        let violations = law.check_button(120.0, 32.0, 8.0);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "TouchTargetTooSmall");
    }

    #[test]
    fn button_48px_passes_strict_aaa() {
        let law = UiLaw::strict_wcag_aaa();
        let violations = law.check_button(120.0, 48.0, 8.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn button_44px_fails_strict_aaa() {
        let law = UiLaw::strict_wcag_aaa();
        let violations = law.check_button(120.0, 44.0, 8.0);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "TouchTargetTooSmall");
    }

    // ─── Button radius sanity ───

    #[test]
    fn button_moderate_radius_passes() {
        let law = UiLaw::default_wcag_aa();
        let violations = law.check_button(120.0, 44.0, 8.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn button_pill_shape_radius_passes() {
        // pill 形 = corner_radius = height/2 は境界内、violation なし
        let law = UiLaw::default_wcag_aa();
        let violations = law.check_button(120.0, 44.0, 22.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn button_over_pill_radius_triggers_radius_oversized() {
        let law = UiLaw::default_wcag_aa();
        let violations = law.check_button(120.0, 44.0, 30.0);
        assert!(violations.iter().any(|v| v.rule == "RadiusOversized"));
    }

    // ─── InputField ───

    #[test]
    fn input_field_44px_passes() {
        let law = UiLaw::default_wcag_aa();
        assert!(law.check_input_field(200.0, 44.0, 6.0).is_empty());
    }

    #[test]
    fn input_field_20px_triggers_touch_target() {
        let law = UiLaw::default_wcag_aa();
        let violations = law.check_input_field(200.0, 20.0, 6.0);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "TouchTargetTooSmall");
    }

    // ─── Card ───

    #[test]
    fn card_only_checks_radius_not_touch_target() {
        let law = UiLaw::default_wcag_aa();
        // 20 px card は Button なら touch target fail するが Card は skip
        let violations = law.check_card(20.0, 20.0, 4.0);
        assert!(violations.is_empty());
    }

    #[test]
    fn card_over_radius_triggers_radius_oversized() {
        let law = UiLaw::default_wcag_aa();
        let violations = law.check_card(100.0, 60.0, 50.0);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "RadiusOversized");
    }

    // ─── Chip ───

    #[test]
    fn small_chip_triggers_touch_target() {
        let law = UiLaw::default_wcag_aa();
        let violations = law.check_chip(80.0, 24.0);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "TouchTargetTooSmall");
    }

    // ─── Theme contrast ───

    #[test]
    fn light_theme_meets_normal_text_contrast() {
        let law = UiLaw::default_wcag_aa();
        assert!(law.check_theme_contrast(&Theme::light(), false).is_empty());
    }

    #[test]
    fn dark_theme_meets_normal_text_contrast() {
        let law = UiLaw::default_wcag_aa();
        assert!(law.check_theme_contrast(&Theme::dark(), false).is_empty());
    }

    #[test]
    fn high_contrast_theme_meets_aaa_normal_text() {
        let law = UiLaw::strict_wcag_aaa();
        assert!(law
            .check_theme_contrast(&Theme::high_contrast(), false)
            .is_empty());
    }

    #[test]
    fn same_color_bg_fg_fails_contrast() {
        let law = UiLaw::default_wcag_aa();
        let bad_theme = Theme {
            bg: [0.5, 0.5, 0.5],
            fg: [0.5, 0.5, 0.5],
            accent: [0.0, 0.0, 0.0],
            spacing_base: 4.0,
            radius_base: 4.0,
        };
        let violations = law.check_theme_contrast(&bad_theme, false);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "ContrastTooLow");
    }

    // ─── Accent contrast ───

    #[test]
    fn light_theme_accent_meets_large_text_contrast() {
        let law = UiLaw::default_wcag_aa();
        assert!(law.check_accent_contrast(&Theme::light(), true).is_empty());
    }
}
