//! Reference frames of a moving word: the glyph shapes, a motion law at one instant, pixels
//!
//! A word is a list of glyph shapes (2D distance fields, [`alice_sdf::Sdf2dNode`]) placed at
//! pen positions. [`render_frame`] applies each glyph's motion channels and rasterizes the
//! word at a fixed density; [`frame_hash`] names the pixels. The same word, law, parameter
//! values and instant give the same frame and hash on every platform: every step is a fixed
//! sequence of `f32` operations without fused multiply-add, the rotation comes from
//! `alice-det-math`, and the rules below are what [`FRAME_SEMANTICS_ID`] measures.
//!
//! A GPU renderer draws the same word for display; its pixels are compared with these within
//! a tolerance, never by hash (GPU arithmetic is not specified to the bit).
//!
//! # Rules
//!
//! - density: [`PX_PER_EM`] pixels per em; the frame is `ceil(width_em * PX_PER_EM)` by
//!   `ceil(height_em * PX_PER_EM)` pixels, each at most [`MAX_SIDE`]
//! - pixel `(x, y)` (row 0 at the top) samples the point
//!   `((x + 0.5) / PX_PER_EM, height_em - (y + 0.5) / PX_PER_EM)` in em, y up
//! - glyph `i`'s point in its own frame: `q = p - origin - (offset_x, offset_y)`, rotated by
//!   `-rotate` and divided by `scale`; its distance is `eval_2d(shape, q) * scale - dilate`
//! - coverage `clamp(0.5 - d * PX_PER_EM, 0, 1)` (a one-pixel edge), times `alpha` and
//!   `luminance`; a pixel is the largest value over the glyphs (so their order does not
//!   matter)
//! - the value `v` is stored as `floor(v * 255 + 0.5)`; a value that is not finite is stored
//!   as 0

use std::fmt;

use alice_sdf::sdf2d::{eval_2d, Sdf2dNode};
use sha2::{Digest as _, Sha256};

use crate::law_id::{fold_semantics_pins, hex32};
use crate::motion_law::{BoundMotion, Channels, SampleError};

/// Pixels per em of a reference frame
pub const PX_PER_EM: u32 = 128;
/// The largest width or height of a reference frame, in pixels
pub const MAX_SIDE: u32 = 4096;
/// The most glyphs of a word
pub const MAX_GLYPHS: usize = 256;

/// What the reference raster means, as pins measured from its behaviour
/// ([`frame_semantics_fingerprints`]; the oracle recomputes every one)
pub const FRAME_SEMANTICS_PINS: &[(&str, [u8; 32])] = &[(
    "frame: 128 px per em, pixel centres, one-pixel edge, max over glyphs, u8 halves up, NaN 0",
    hex32("ddda47062a75e946783b6977f0ea1b715df6745db3f8c6743f48dda9a9197d65"),
)];

/// The fold of [`FRAME_SEMANTICS_PINS`], the first thing [`frame_hash`] hashes
pub const FRAME_SEMANTICS_ID: [u8; 32] =
    hex32("111edba290bdd5997a0afef3418471009be11abc2a7d575b3cbeb3b854180ea0");

/// One glyph of a word
#[derive(Debug, Clone)]
pub struct Glyph {
    /// the glyph's shape in em, its origin at `(0, 0)`
    pub shape: Sdf2dNode,
    /// where the glyph's origin is in the word, in em (y up)
    pub origin: [f32; 2],
}

/// A word to rasterize: its glyphs and the box it is drawn in (em)
#[derive(Debug, Clone)]
pub struct Word {
    /// the glyphs, in reading order (the motion law's glyph index)
    pub glyphs: Vec<Glyph>,
    /// the width of the frame, in em
    pub width_em: f32,
    /// the height of the frame, in em
    pub height_em: f32,
}

/// A reference frame: rows from the top, one byte per pixel
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// pixels per row
    pub width: u32,
    /// rows
    pub height: u32,
    /// `width * height` values, row-major
    pub pixels: Vec<u8>,
}

/// Why a frame cannot be drawn
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// the box is not finite and positive, or a side is above [`MAX_SIDE`] pixels
    Size,
    /// no glyph, or more than [`MAX_GLYPHS`]
    GlyphCount,
    /// a glyph's channels could not be sampled
    Motion(SampleError),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size => write!(
                f,
                "the frame size is not positive, finite and at most {MAX_SIDE} px"
            ),
            Self::GlyphCount => write!(f, "a word has 1 to {MAX_GLYPHS} glyphs"),
            Self::Motion(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for FrameError {}

fn side(em: f32) -> Option<u32> {
    #[allow(clippy::cast_precision_loss)] // 128 is exact
    let px = (em * PX_PER_EM as f32).ceil();
    #[allow(clippy::cast_precision_loss)]
    let ok = em.is_finite() && em > 0.0 && px <= MAX_SIDE as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // in 1..=MAX_SIDE
    ok.then_some(px as u32)
}

// no fused multiply-add: the stored value is part of FRAME_SEMANTICS_ID and must round the
// same on every target
#[allow(clippy::suboptimal_flops)]
fn quantize(v: f32) -> u8 {
    if v.is_finite() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..=255
        let q = (v.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8;
        q
    } else {
        0
    }
}

/// The frame of `word` under `motion` at `t_ms` milliseconds
///
/// # Errors
///
/// [`FrameError::Size`], [`FrameError::GlyphCount`], or [`FrameError::Motion`] when a
/// glyph's channels cannot be sampled
#[allow(clippy::cast_precision_loss)] // pixel indices are below MAX_SIDE, exact in f32
pub fn render_frame(word: &Word, motion: &BoundMotion<'_>, t_ms: u64) -> Result<Frame, FrameError> {
    let (Some(width), Some(height)) = (side(word.width_em), side(word.height_em)) else {
        return Err(FrameError::Size);
    };
    if word.glyphs.is_empty() || word.glyphs.len() > MAX_GLYPHS {
        return Err(FrameError::GlyphCount);
    }
    let n = u32::try_from(word.glyphs.len()).map_err(|_| FrameError::GlyphCount)?;
    let mut placed: Vec<(&Glyph, Channels, f32, f32)> = Vec::with_capacity(word.glyphs.len());
    for (i, g) in (0..n).zip(&word.glyphs) {
        let c = motion.sample(t_ms, i, n).map_err(FrameError::Motion)?;
        let (s, co) = alice_det_math::sin_cos(c.rotate);
        placed.push((g, c, s, co));
    }
    let density = PX_PER_EM as f32;
    let mut pixels = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        let py = word.height_em - (y as f32 + 0.5) / density;
        for x in 0..width {
            let px = (x as f32 + 0.5) / density;
            let mut best = 0.0_f32;
            for (g, c, s, co) in &placed {
                let qx = px - g.origin[0] - c.offset_x;
                let qy = py - g.origin[1] - c.offset_y;
                // rotate by -rotate: (x cos + y sin, -x sin + y cos)
                let rx = qx * co + qy * s;
                let ry = (-qx) * s + qy * co;
                let d = eval_2d(&g.shape, [rx / c.scale, ry / c.scale]) * c.scale - c.dilate;
                let coverage = (0.5 - d * density).clamp(0.0, 1.0);
                let v = coverage * c.alpha * c.luminance;
                // a NaN never wins (it compares false), so the pixel stays finite
                if v > best {
                    best = v;
                }
            }
            pixels.push(quantize(best));
        }
    }
    Ok(Frame {
        width,
        height,
        pixels,
    })
}

/// The mean of a frame's stored values, scaled to `[0, 1]` (the sum is exact: an integer)
#[must_use]
#[allow(clippy::cast_precision_loss)] // the sum and the count are below 2^53
pub fn frame_luminance(frame: &Frame) -> f64 {
    let sum: u64 = frame.pixels.iter().map(|&p| u64::from(p)).sum();
    let n = frame.pixels.len() as f64;
    if n == 0.0 {
        0.0
    } else {
        sum as f64 / (n * 255.0)
    }
}

/// The luminance of `count` frames sampled `hz` times a second from `from_ms` (frame `k` at
/// `from_ms + floor(k * 1000 / hz)` milliseconds): the series a flash audit law reads
///
/// # Errors
///
/// [`FrameError`] of the first frame that cannot be drawn, or [`FrameError::Size`] when `hz`
/// is 0
pub fn luminance_series(
    word: &Word,
    motion: &BoundMotion<'_>,
    from_ms: u64,
    count: u32,
    hz: u32,
) -> Result<Vec<f64>, FrameError> {
    if hz == 0 {
        return Err(FrameError::Size);
    }
    (0..count)
        .map(|k| {
            let t = from_ms + u64::from(k) * 1000 / u64::from(hz);
            render_frame(word, motion, t).map(|f| frame_luminance(&f))
        })
        .collect()
}

/// The name of a frame: SHA-256 of [`FRAME_SEMANTICS_ID`], the width and height
/// (little-endian `u32`) and the pixels
#[must_use]
pub fn frame_hash(frame: &Frame) -> [u8; 32] {
    frame_hash_under(&FRAME_SEMANTICS_ID, frame)
}

fn frame_hash_under(semantics: &[u8; 32], frame: &Frame) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(semantics);
    h.update(frame.width.to_le_bytes());
    h.update(frame.height.to_le_bytes());
    h.update(&frame.pixels);
    h.finalize().into()
}

/// The word the frame pin draws: a ring, a rounded bar and a stroke, so the edge, the
/// overlap rule and every shape kind of a glyph are reached
#[must_use]
pub fn probe_word() -> Word {
    Word {
        glyphs: vec![
            Glyph {
                shape: Sdf2dNode::circle(0.3).onion(0.06),
                origin: [0.4, 0.5],
            },
            Glyph {
                shape: Sdf2dNode::rounded_rect(0.15, 0.35, 0.05),
                origin: [0.95, 0.5],
            },
            Glyph {
                shape: Sdf2dNode::line([-0.2, -0.3], [0.2, 0.3], 0.04),
                origin: [1.45, 0.5],
            },
        ],
        width_em: 1.8,
        height_em: 1.0,
    }
}

/// The motion the frame pin applies: every channel moves
pub const FRAME_PROBE_LAW: &str = "law frame_probe
kind motion
period 1000
stagger 250
channel offset_x = 0.05 * sin(2 * pi * phase)
channel offset_y = -0.03 * phase
channel scale = 1 + 0.2 * phase
channel rotate = 0.4 * phase
channel dilate = 0.01 * cos(2 * pi * phase)
channel alpha = 0.5 + phase
channel luminance = 1 - 0.3 * phase
";

/// The measured value of every pin of [`FRAME_SEMANTICS_PINS`]
///
/// # Panics
///
/// If [`FRAME_PROBE_LAW`] or [`probe_word`] does not draw (the oracle checks they do)
#[must_use]
pub fn frame_semantics_fingerprints() -> Vec<(&'static str, [u8; 32])> {
    let law =
        crate::motion_law::MotionLaw::from_file(FRAME_PROBE_LAW).expect("the probe law reads");
    let bound = law.bind(&[]).expect("the probe binds");
    let word = probe_word();
    let mut h = Sha256::new();
    for t in [0, 125, 500, 875] {
        let f = render_frame(&word, &bound, t).expect("the probe draws");
        h.update(frame_hash_under(&[0; 32], &f));
    }
    // the edge cases of the stored value
    for v in [
        f32::NAN,
        f32::INFINITY,
        -1.0,
        0.0,
        0.5 / 255.0,
        0.501_960_8,
        1.0,
        2.0,
    ] {
        h.update([quantize(v)]);
    }
    vec![(FRAME_SEMANTICS_PINS[0].0, h.finalize().into())]
}

/// The fold of the measured pins (what [`FRAME_SEMANTICS_ID`] must equal)
#[must_use]
pub fn measured_frame_semantics_id() -> [u8; 32] {
    fold_semantics_pins(&frame_semantics_fingerprints())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_moves_with_the_frame_semantics() {
        let f = Frame {
            width: 1,
            height: 1,
            pixels: vec![7],
        };
        assert_eq!(frame_hash(&f), frame_hash_under(&FRAME_SEMANTICS_ID, &f));
        let mut other = FRAME_SEMANTICS_ID;
        other[0] ^= 1;
        assert_ne!(frame_hash_under(&other, &f), frame_hash(&f));
    }

    #[test]
    fn stored_values_round_halves_up_and_not_finite_is_zero() {
        assert_eq!(quantize(f32::NAN), 0);
        assert_eq!(quantize(f32::INFINITY), 0);
        assert_eq!(quantize(-1.0), 0);
        assert_eq!(quantize(1.0), 255);
        assert_eq!(quantize(2.0), 255);
        assert_eq!(quantize(0.5 / 255.0), 1);
    }
}
