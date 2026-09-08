//! `MetaFontParams` — 40-byte parametric font descriptor
//!
//! 10 f32 axes that fully describe a typeface Any weight / width / serif /
//! italic can be expressed as a point in this 10-dim space (Phase 2 Law)
//!
//! ALICE-Font v0.7.5 `param.rs` (MIT) の independent MIT re-implementation
//! 目的: `alice-lol-font` を Foundry (AGPL) に依存させずに parametric font
//! を LOL DSL に注入できるようにする Font との API 相互互換を維持し、後段の
//! Foundry bridge (opt-in) 有効化時に params を pass-through で送れる

/// Parametric font descriptor (40 bytes、10 f32 axis)
///
/// Encodes a complete typeface as 10 f32 parameters wire-transmittable
/// as 40-byte packet (三相原理 Phase 2 Law: 「data でなく law を送る」)
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct MetaFontParams {
    /// Stroke weight: 0.0 = hairline, 0.5 = regular, 1.0 = black
    pub weight: f32,
    /// Horizontal width: 0.5 = condensed, 1.0 = normal, 1.5 = extended
    pub width: f32,
    /// Serif amount: 0.0 = sans-serif, 1.0 = full serif
    pub serif: f32,
    /// Thick/thin contrast: 0.0 = monoweight, 1.0 = high contrast
    pub contrast: f32,
    /// Italic slant angle in radians (0.0 = upright, ~0.2 = italic)
    pub slant: f32,
    /// x-height ratio relative to em (0.4-0.6 typical)
    pub x_height: f32,
    /// Capital height ratio relative to em
    pub cap_height: f32,
    /// Ascender ratio relative to em
    pub ascender: f32,
    /// Descender ratio relative to em (positive value = below baseline depth)
    pub descender: f32,
    /// Corner roundness: 0.0 = sharp, 1.0 = fully rounded
    pub roundness: f32,
}

impl MetaFontParams {
    /// Wire size in bytes (`10 axis × 4 byte = 40`)
    pub const SIZE: usize = 40;

    /// Sans-serif regular (like Helvetica / Arial)
    #[must_use]
    pub const fn sans_regular() -> Self {
        Self {
            weight: 0.45,
            width: 1.0,
            serif: 0.0,
            contrast: 0.15,
            slant: 0.0,
            x_height: 0.52,
            cap_height: 0.72,
            ascender: 0.80,
            descender: 0.22,
            roundness: 0.3,
        }
    }

    /// Sans-serif hairline — shader path で細線が視覚的に merge しない thin preset
    ///
    /// `sans_regular` (`stroke_half_width` ≈ 0.046 em) は高解像度描画で
    /// 三 (3 本水平線 spacing 0.3) が merge しがち 本 preset は
    /// `stroke_half_width` ≈ 0.022 em で gap を確保する
    #[must_use]
    pub const fn sans_hairline() -> Self {
        Self {
            weight: 0.15,
            width: 1.0,
            serif: 0.0,
            contrast: 0.15,
            slant: 0.0,
            x_height: 0.52,
            cap_height: 0.72,
            ascender: 0.80,
            descender: 0.22,
            roundness: 0.3,
        }
    }

    /// Sans-serif bold
    #[must_use]
    pub const fn sans_bold() -> Self {
        Self {
            weight: 0.75,
            width: 1.02,
            serif: 0.0,
            contrast: 0.10,
            slant: 0.0,
            x_height: 0.54,
            cap_height: 0.72,
            ascender: 0.80,
            descender: 0.22,
            roundness: 0.3,
        }
    }

    /// Serif regular (like Times New Roman)
    #[must_use]
    pub const fn serif_regular() -> Self {
        Self {
            weight: 0.42,
            width: 1.0,
            serif: 0.8,
            contrast: 0.55,
            slant: 0.0,
            x_height: 0.45,
            cap_height: 0.68,
            ascender: 0.78,
            descender: 0.22,
            roundness: 0.1,
        }
    }

    /// Serif italic
    #[must_use]
    pub const fn serif_italic() -> Self {
        Self {
            weight: 0.42,
            width: 0.98,
            serif: 0.7,
            contrast: 0.55,
            slant: 0.21,
            x_height: 0.45,
            cap_height: 0.68,
            ascender: 0.78,
            descender: 0.22,
            roundness: 0.1,
        }
    }

    /// Monospace regular (like Courier)
    #[must_use]
    pub const fn mono_regular() -> Self {
        Self {
            weight: 0.40,
            width: 0.6,
            serif: 0.5,
            contrast: 0.0,
            slant: 0.0,
            x_height: 0.53,
            cap_height: 0.70,
            ascender: 0.80,
            descender: 0.25,
            roundness: 0.0,
        }
    }

    /// Gothic / display (like Impact)
    #[must_use]
    pub const fn display_heavy() -> Self {
        Self {
            weight: 0.95,
            width: 0.7,
            serif: 0.0,
            contrast: 0.05,
            slant: 0.0,
            x_height: 0.60,
            cap_height: 0.75,
            ascender: 0.80,
            descender: 0.18,
            roundness: 0.1,
        }
    }

    /// Linearly interpolate between two parameter sets
    ///
    /// `t = 0.0` returns `self`, `t = 1.0` returns `other`
    #[must_use]
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self {
            weight: (other.weight - self.weight).mul_add(t, self.weight),
            width: (other.width - self.width).mul_add(t, self.width),
            serif: (other.serif - self.serif).mul_add(t, self.serif),
            contrast: (other.contrast - self.contrast).mul_add(t, self.contrast),
            slant: (other.slant - self.slant).mul_add(t, self.slant),
            x_height: (other.x_height - self.x_height).mul_add(t, self.x_height),
            cap_height: (other.cap_height - self.cap_height).mul_add(t, self.cap_height),
            ascender: (other.ascender - self.ascender).mul_add(t, self.ascender),
            descender: (other.descender - self.descender).mul_add(t, self.descender),
            roundness: (other.roundness - self.roundness).mul_add(t, self.roundness),
        }
    }

    /// Encode to 40-byte wire format (little-endian、Phase 2 Law transmission)
    #[must_use]
    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        let fields = [
            self.weight,
            self.width,
            self.serif,
            self.contrast,
            self.slant,
            self.x_height,
            self.cap_height,
            self.ascender,
            self.descender,
            self.roundness,
        ];
        for (i, f) in fields.iter().enumerate() {
            let bytes = f.to_le_bytes();
            buf[i * 4..i * 4 + 4].copy_from_slice(&bytes);
        }
        buf
    }

    /// Decode from 40-byte wire format (little-endian)
    #[must_use]
    pub fn decode(data: &[u8; Self::SIZE]) -> Self {
        let mut fields = [0.0f32; 10];
        for (i, field) in fields.iter_mut().enumerate() {
            let mut bytes = [0u8; 4];
            bytes.copy_from_slice(&data[i * 4..i * 4 + 4]);
            *field = f32::from_le_bytes(bytes);
        }
        Self {
            weight: fields[0],
            width: fields[1],
            serif: fields[2],
            contrast: fields[3],
            slant: fields[4],
            x_height: fields[5],
            cap_height: fields[6],
            ascender: fields[7],
            descender: fields[8],
            roundness: fields[9],
        }
    }

    /// Actual stroke half-width for rendering (in em units)
    #[must_use]
    pub fn stroke_half_width(&self) -> f32 {
        self.weight.mul_add(0.08, 0.01)
    }

    /// Thick stroke half-width (contrast がある高 axis で thin より太くなる)
    #[must_use]
    pub fn thick_half_width(&self) -> f32 {
        self.stroke_half_width() * self.contrast.mul_add(0.8, 1.0)
    }

    /// Thin stroke half-width (contrast がある高 axis で thick より細くなる)
    #[must_use]
    pub fn thin_half_width(&self) -> f32 {
        self.stroke_half_width() * (-self.contrast).mul_add(0.5, 1.0)
    }

    /// Serif bracket length (`serif = 0.0` で 0、`serif = 1.0` で 0.06 em)
    #[must_use]
    pub fn serif_length(&self) -> f32 {
        self.serif * 0.06
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_size_is_40_bytes() {
        assert_eq!(core::mem::size_of::<MetaFontParams>(), MetaFontParams::SIZE);
    }

    #[test]
    fn encode_decode_roundtrip_sans_regular() {
        let params = MetaFontParams::sans_regular();
        let encoded = params.encode();
        assert_eq!(encoded.len(), MetaFontParams::SIZE);
        let decoded = MetaFontParams::decode(&encoded);
        assert!((decoded.weight - params.weight).abs() < 1e-6);
        assert!((decoded.serif - params.serif).abs() < 1e-6);
        assert!((decoded.slant - params.slant).abs() < 1e-6);
    }

    #[test]
    fn presets_have_correct_axis_dominance() {
        let sans = MetaFontParams::sans_regular();
        assert!(sans.serif < 0.01, "sans_regular must be zero-serif");

        let serif = MetaFontParams::serif_regular();
        assert!(serif.serif > 0.5, "serif_regular must be strong-serif");

        let bold = MetaFontParams::sans_bold();
        assert!(
            bold.weight > sans.weight,
            "sans_bold weight ({}) must exceed sans_regular ({})",
            bold.weight,
            sans.weight
        );
    }

    #[test]
    fn hairline_thinner_than_regular_for_shader_path() {
        let hairline = MetaFontParams::sans_hairline();
        let regular = MetaFontParams::sans_regular();
        assert!(
            hairline.weight < regular.weight,
            "hairline weight {} must be < regular {}",
            hairline.weight,
            regular.weight
        );
        assert!(
            hairline.stroke_half_width() < regular.stroke_half_width(),
            "hairline stroke_half_width {} must be < regular {}",
            hairline.stroke_half_width(),
            regular.stroke_half_width()
        );
        assert!(
            hairline.stroke_half_width() * 2.0 < 0.15,
            "hairline stroke width {} must be < 0.15 em (三 3 本線 gap 確保)",
            hairline.stroke_half_width() * 2.0
        );
    }

    #[test]
    fn lerp_midpoint_between_regular_and_bold() {
        let a = MetaFontParams::sans_regular();
        let b = MetaFontParams::sans_bold();
        let mid = a.lerp(&b, 0.5);
        assert!((mid.weight - f32::midpoint(a.weight, b.weight)).abs() < 0.01);
    }

    #[test]
    fn lerp_identity_at_endpoints() {
        let a = MetaFontParams::sans_regular();
        let b = MetaFontParams::serif_regular();

        let at_zero = a.lerp(&b, 0.0);
        assert!((at_zero.weight - a.weight).abs() < 1e-6);
        assert!((at_zero.serif - a.serif).abs() < 1e-6);
        assert!((at_zero.contrast - a.contrast).abs() < 1e-6);

        let at_one = a.lerp(&b, 1.0);
        assert!((at_one.weight - b.weight).abs() < 1e-6);
        assert!((at_one.serif - b.serif).abs() < 1e-6);
        assert!((at_one.contrast - b.contrast).abs() < 1e-6);
    }

    #[test]
    fn stroke_widths_ordered_by_contrast() {
        let params = MetaFontParams::serif_regular();
        let thick = params.thick_half_width();
        let thin = params.thin_half_width();
        assert!(
            thick > thin,
            "high-contrast serif: thick ({thick}) must exceed thin ({thin})"
        );
    }

    #[test]
    fn mono_has_no_stroke_contrast() {
        let mono = MetaFontParams::mono_regular();
        assert!(mono.contrast < 0.01, "mono_regular must be zero-contrast");
        let thick = mono.thick_half_width();
        let thin = mono.thin_half_width();
        assert!(
            (thick - thin).abs() < 0.01,
            "mono thick ({thick}) and thin ({thin}) must be near-equal"
        );
    }

    #[test]
    fn serif_length_zero_for_sans_positive_for_serif() {
        let sans = MetaFontParams::sans_regular();
        assert!(sans.serif_length().abs() < 0.001);
        let serif = MetaFontParams::serif_regular();
        assert!(serif.serif_length() > 0.01);
    }

    #[test]
    fn display_heavy_is_thick_and_condensed() {
        let d = MetaFontParams::display_heavy();
        assert!(d.weight > 0.9, "display_heavy must be very bold");
        assert!(d.width < 0.8, "display_heavy must be condensed");
    }

    #[test]
    fn encode_decode_roundtrip_all_presets() {
        let presets = [
            MetaFontParams::sans_regular(),
            MetaFontParams::sans_hairline(),
            MetaFontParams::sans_bold(),
            MetaFontParams::serif_regular(),
            MetaFontParams::serif_italic(),
            MetaFontParams::mono_regular(),
            MetaFontParams::display_heavy(),
        ];
        for p in &presets {
            let decoded = MetaFontParams::decode(&p.encode());
            assert!((decoded.weight - p.weight).abs() < 1e-6);
            assert!((decoded.slant - p.slant).abs() < 1e-6);
            assert!((decoded.roundness - p.roundness).abs() < 1e-6);
        }
    }
}
