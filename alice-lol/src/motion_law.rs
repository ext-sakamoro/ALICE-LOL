//! Motion laws (`kind motion`): how a run of glyphs moves over time
//!
//! A motion law names its parameters, the length of its loop and, for each glyph at each
//! instant, a fixed set of channels (offset, scale, rotation, dilation, alpha, luminance)
//! that a renderer applies to the glyph's shape. The same law, the same parameter values and
//! the same instant give the same bits on every platform: time is an integer number of
//! milliseconds, the loop position is computed from integers, and the expressions are the
//! expressions of research laws (`f64` without fused multiply-add, `alice-det-math` for
//! every function), each channel converted to `f32` once at the end.
//!
//! ```text
//! law heartbeat
//! kind motion
//! claim two quick swells per beat
//! param bpm number = 72 range 40 180
//! param depth number = 0.06 range 0 0.2
//! period 60000 / bpm
//! stagger 40
//! let pulse = max(0, sin(2 * pi * phase)) ^ 8
//! channel scale = 1 + depth * pulse
//! ```
//!
//! # Lines
//!
//! - `law <name>` first, then `kind motion`; `claim` / `source` lines are notes (not in the
//!   identifier); `#` starts a comment line
//! - `param <name> number = <default> range <lo> <hi>`: a named number, `lo <= default <= hi`
//! - `period <expr>`: the length of the loop in milliseconds, from the parameters only; the
//!   value is rounded to the nearest integer (halves up) and must be in `1..=3_600_000`
//! - `stagger <expr>`: how many milliseconds each glyph runs behind the one before it, from
//!   the parameters only, rounded the same way, in `0..=3_600_000` (default 0)
//! - `let <name> = <expr>`: a value for the later lines
//! - `channel <name> = <expr>`: one of [`CHANNELS`], at most once each; a channel not given
//!   keeps its identity value
//!
//! # Time
//!
//! For glyph `i` of `n` at time `t` (milliseconds, an integer), the glyph's own time is
//! `ti = t - i * stagger`; `cycle = floor(ti / period)` and `phase = (ti - cycle * period) /
//! period`, both from integers (so `phase` is in `[0, 1)` and exact to the division). The
//! expressions read `t` (as `ti`), `phase`, `cycle`, `i`, `n`, `pi`, the parameters and the
//! earlier `let` values. There is no clock: the caller says which instant.
//!
//! # Channels
//!
//! Each channel value must be finite; `scale` must be positive; `alpha` and `luminance` are
//! clamped to `[0, 1]` after the conversion to `f32`.

use std::fmt;

use crate::law_id::{fold_semantics_pins, hex32, LawIdHasher};
use crate::research_law::{CompiledExpr, LawExpr};

/// The kind tag of a motion law, hashed into its identifier
pub const MOTION_LAW_KIND: &[u8] = b"lol.law.motion/v1";

/// The channels a motion law may set, in the order [`Channels`] holds them
pub const CHANNELS: [&str; 7] = [
    "offset_x",
    "offset_y",
    "scale",
    "rotate",
    "dilate",
    "alpha",
    "luminance",
];

/// The longest loop and the largest stagger (one hour, in milliseconds)
pub const MAX_PERIOD_MS: u64 = 3_600_000;

/// The most parameters, `let` lines and lines of a motion law
pub const MAX_PARAMS: usize = 64;
/// The most `let` lines
pub const MAX_LETS: usize = 256;
/// The most lines
pub const MAX_LINES: usize = 1024;

/// What the reading of a motion law means, as pins measured from the behaviour of this
/// module ([`motion_semantics_fingerprints`]; the oracle recomputes every one)
pub const MOTION_SEMANTICS_PINS: &[(&str, [u8; 32])] = &[
    (
        "motion: integer time, loop position from integers, channels converted once",
        hex32("d543189088b1956aa4e9a1afcd9baaa25818a5f676d7e3e53e759de40c67c19b"),
    ),
    (
        "motion: the expression functions of research laws",
        hex32("ee1b39dde2b1e577892f19daee2fb514a7b43f56be7d2211061de1ddac28fad3"),
    ),
];

/// The fold of [`MOTION_SEMANTICS_PINS`] (the oracle checks the two agree)
pub const MOTION_SEMANTICS_ID: [u8; 32] =
    hex32("dfcf2c6cd34b5b43f2503a17283307b9cb27a8163b6970b1c961888ee18278b8");

/// A motion law that does not read, with the line it was on (0 for the whole file)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotionLawError {
    /// the 1-based line, 0 when the problem is not on one line
    pub line: usize,
    /// what is wrong
    pub message: String,
}

impl fmt::Display for MotionLawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "motion law: {}", self.message)
        } else {
            write!(f, "motion law, line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for MotionLawError {}

fn err(line: usize, message: impl Into<String>) -> MotionLawError {
    MotionLawError {
        line,
        message: message.into(),
    }
}

/// Why a sample could not be computed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SampleError {
    /// a parameter the law does not declare
    UnknownParam(String),
    /// a parameter given twice
    RepeatedParam(String),
    /// a parameter value outside its declared range, or not finite
    ParamOutOfRange {
        /// the parameter
        name: String,
    },
    /// `period` or `stagger` is not finite or outside its bounds for these parameters
    Timing {
        /// `period` or `stagger`
        which: &'static str,
    },
    /// an intermediate or channel value is not finite
    NonFinite {
        /// the `let` or channel whose value is not finite
        name: String,
    },
    /// `scale` is not positive
    ScaleNotPositive,
    /// the glyph index is not below the glyph count
    GlyphIndex,
    /// the glyph's own time is outside the exactly representable range
    TimeOutOfRange,
    /// a proportion law's `require` line does not hold for these values (the line, then both
    /// sides' values)
    Refused {
        /// the line as `<left> <= <right>`
        require: String,
        /// `<left value> > <right value>`
        values: String,
    },
}

impl fmt::Display for SampleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParam(n) => write!(f, "the law has no parameter `{n}`"),
            Self::RepeatedParam(n) => write!(f, "parameter `{n}` is given twice"),
            Self::ParamOutOfRange { name } => write!(f, "parameter `{name}` is outside its range"),
            Self::Timing { which } => write!(f, "`{which}` is outside its bounds"),
            Self::NonFinite { name } => write!(f, "`{name}` is not finite"),
            Self::ScaleNotPositive => write!(f, "`scale` is not positive"),
            Self::GlyphIndex => write!(f, "the glyph index is not below the glyph count"),
            Self::TimeOutOfRange => write!(f, "the glyph's time is out of range"),
            Self::Refused { require, values } => {
                write!(f, "`require {require}` does not hold ({values})")
            }
        }
    }
}

impl std::error::Error for SampleError {}

/// A declared parameter
#[derive(Debug, Clone, PartialEq)]
pub struct MotionParam {
    /// its name
    pub name: String,
    /// the value used when none is given
    pub default: f64,
    /// the smallest value
    pub lo: f64,
    /// the largest value
    pub hi: f64,
}

/// What a motion law sets on one glyph at one instant
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Channels {
    /// horizontal offset (the glyph's units)
    pub offset_x: f32,
    /// vertical offset
    pub offset_y: f32,
    /// uniform scale about the glyph's origin, positive
    pub scale: f32,
    /// rotation about the glyph's origin, radians
    pub rotate: f32,
    /// how far the shape grows outward (a negative value shrinks it)
    pub dilate: f32,
    /// opacity in `[0, 1]`
    pub alpha: f32,
    /// brightness in `[0, 1]`
    pub luminance: f32,
}

impl Channels {
    /// The channels of a glyph the law leaves as it is
    pub const IDENTITY: Self = Self {
        offset_x: 0.0,
        offset_y: 0.0,
        scale: 1.0,
        rotate: 0.0,
        dilate: 0.0,
        alpha: 1.0,
        luminance: 1.0,
    };

    /// The channels as their bit patterns, in the order of [`CHANNELS`]
    #[must_use]
    pub const fn to_bits(&self) -> [u32; 7] {
        [
            self.offset_x.to_bits(),
            self.offset_y.to_bits(),
            self.scale.to_bits(),
            self.rotate.to_bits(),
            self.dilate.to_bits(),
            self.alpha.to_bits(),
            self.luminance.to_bits(),
        ]
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Line {
    name: String,
    expr: LawExpr,
    code: CompiledExpr,
}

/// A motion law (`kind motion`)
#[derive(Debug, Clone, PartialEq)]
pub struct MotionLaw {
    name: String,
    params: Vec<MotionParam>,
    period: LawExpr,
    period_code: CompiledExpr,
    stagger: Option<(LawExpr, CompiledExpr)>,
    lets: Vec<Line>,
    channels: Vec<(usize, Line)>,
}

/// The names every expression after the timing lines can read, before parameters and lets
const TIME_NAMES: [&str; 6] = ["t", "phase", "cycle", "i", "n", "pi"];

fn is_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

fn number(s: &str) -> Option<f64> {
    let v: f64 = s.parse().ok()?;
    let plain = s
        .bytes()
        .all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'-'));
    (plain && v.is_finite()).then_some(v)
}

fn expr(line: usize, text: &str) -> Result<LawExpr, MotionLawError> {
    LawExpr::parse(text).map_err(|e| err(line, format!("`{}`: {e}", text.trim())))
}

/// `(v + 0.5).floor()` as an integer in `lo..=hi`
fn rounded_ms(v: f64, lo: u64, hi: u64) -> Option<u64> {
    if !v.is_finite() {
        return None;
    }
    let r = (v + 0.5).floor();
    #[allow(clippy::cast_precision_loss)] // the bounds are below 2^53
    let in_bounds = r >= lo as f64 && r <= hi as f64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // checked just above
    in_bounds.then_some(r as u64)
}

/// The timing of a motion law under one set of parameter values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// the loop length in milliseconds, `1..=MAX_PERIOD_MS`
    pub period_ms: u64,
    /// how far each glyph runs behind the one before it, `0..=MAX_PERIOD_MS`
    pub stagger_ms: u64,
}

/// A motion law with its parameter values fixed
#[derive(Debug, Clone)]
pub struct BoundMotion<'a> {
    law: &'a MotionLaw,
    values: Vec<f64>,
    timing: Timing,
}

impl MotionLaw {
    /// Reads a motion law file
    ///
    /// # Errors
    ///
    /// [`MotionLawError`] naming the line: not `kind motion`, an unknown line, a name
    /// declared twice or reserved, a parameter whose default is outside its range, a
    /// `period` that is missing or reads a non-parameter, an expression that does not parse
    /// or names something not declared before it, a channel not in [`CHANNELS`] or given
    /// twice, more lines, parameters or lets than the limits
    #[allow(clippy::too_many_lines)] // one arm per kind of line, in file order
    pub fn from_file(text: &str) -> Result<Self, MotionLawError> {
        let mut name = None;
        let mut kind = false;
        let mut params: Vec<MotionParam> = Vec::new();
        let mut period: Option<LawExpr> = None;
        let mut stagger: Option<LawExpr> = None;
        let mut lets: Vec<(String, LawExpr)> = Vec::new();
        let mut channels: Vec<(usize, String, LawExpr)> = Vec::new();
        let used = |n: &str, params: &[MotionParam], lets: &[(String, LawExpr)]| {
            TIME_NAMES.contains(&n)
                || params.iter().any(|p| p.name == n)
                || lets.iter().any(|(l, _)| l == n)
        };
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() > MAX_LINES {
            return Err(err(0, format!("more than {MAX_LINES} lines")));
        }
        for (k, raw) in lines.iter().enumerate() {
            let line = k + 1;
            if raw.chars().any(|c| c.is_control() && c != '\t') {
                return Err(err(line, "a control character"));
            }
            let t = raw.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let (word, rest) = t.split_once(' ').map_or((t, ""), |(w, r)| (w, r.trim()));
            if name.is_none() {
                if word != "law" || !is_name(rest) {
                    return Err(err(line, "the first line is `law <name>`"));
                }
                name = Some(rest.to_owned());
                continue;
            }
            if !kind {
                if t != "kind motion" {
                    return Err(err(line, "the second line is `kind motion`"));
                }
                kind = true;
                continue;
            }
            match word {
                "claim" | "source" => {}
                "param" => {
                    let w: Vec<&str> = rest.split_whitespace().collect();
                    let [n, "number", "=", d, "range", lo, hi] = w[..] else {
                        return Err(err(
                            line,
                            "`param <name> number = <default> range <lo> <hi>` expected",
                        ));
                    };
                    if !is_name(n) || used(n, &params, &lets) {
                        return Err(err(line, format!("`{n}` is reserved or declared twice")));
                    }
                    let (Some(d), Some(lo), Some(hi)) = (number(d), number(lo), number(hi)) else {
                        return Err(err(line, "a parameter's numbers are plain decimals"));
                    };
                    if !(lo <= d && d <= hi) {
                        return Err(err(line, "the default is outside the range"));
                    }
                    if params.len() == MAX_PARAMS {
                        return Err(err(line, format!("more than {MAX_PARAMS} parameters")));
                    }
                    params.push(MotionParam {
                        name: n.to_owned(),
                        default: d,
                        lo,
                        hi,
                    });
                }
                "period" | "stagger" => {
                    let slot = if word == "period" {
                        &mut period
                    } else {
                        &mut stagger
                    };
                    if slot.is_some() {
                        return Err(err(line, format!("a second `{word}` line")));
                    }
                    if !lets.is_empty() || !channels.is_empty() {
                        return Err(err(
                            line,
                            format!("`{word}` comes before `let` and `channel`"),
                        ));
                    }
                    let e = expr(line, rest)?;
                    let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                    e.compile_named(&names)
                        .map_err(|_| err(line, format!("`{word}` reads only parameters")))?;
                    *slot = Some(e);
                }
                "let" | "channel" => {
                    let Some((n, e)) = rest.split_once('=') else {
                        return Err(err(
                            line,
                            format!("`{word} <name> = <expression>` expected"),
                        ));
                    };
                    let n = n.trim();
                    if period.is_none() {
                        return Err(err(line, "`period` comes first"));
                    }
                    let e = expr(line, e)?;
                    let mut names: Vec<&str> = TIME_NAMES.to_vec();
                    names.extend(params.iter().map(|p| p.name.as_str()));
                    names.extend(lets.iter().map(|(l, _)| l.as_str()));
                    e.compile_named(&names)
                        .map_err(|e| err(line, e.to_string()))?;
                    if word == "let" {
                        if !is_name(n) || used(n, &params, &lets) {
                            return Err(err(line, format!("`{n}` is reserved or declared twice")));
                        }
                        if lets.len() == MAX_LETS {
                            return Err(err(line, format!("more than {MAX_LETS} `let` lines")));
                        }
                        if !channels.is_empty() {
                            return Err(err(line, "`let` comes before `channel`"));
                        }
                        lets.push((n.to_owned(), e));
                    } else {
                        let Some(slot) = CHANNELS.iter().position(|c| *c == n) else {
                            return Err(err(
                                line,
                                format!("`{n}` is not a channel ({})", CHANNELS.join(", ")),
                            ));
                        };
                        if channels.iter().any(|(s, _, _)| *s == slot) {
                            return Err(err(line, format!("channel `{n}` is given twice")));
                        }
                        channels.push((slot, n.to_owned(), e));
                    }
                }
                _ => return Err(err(line, format!("unknown line `{word}`"))),
            }
        }
        let name = name.ok_or_else(|| err(0, "empty (no `law <name>` line)"))?;
        if !kind {
            return Err(err(0, "not a motion law (no `kind motion`)"));
        }
        let period = period.ok_or_else(|| err(0, "no `period` line"))?;
        let pnames: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
        let compile_timing =
            |e: &LawExpr| e.compile_named(&pnames).map_err(|e| err(0, e.to_string()));
        let period_code = compile_timing(&period)?;
        let stagger = match stagger {
            Some(e) => {
                let c = compile_timing(&e)?;
                Some((e, c))
            }
            None => None,
        };
        let mut names: Vec<&str> = TIME_NAMES.to_vec();
        names.extend(pnames.iter().copied());
        let mut compiled_lets = Vec::new();
        for (n, e) in &lets {
            let code = e.compile_named(&names).map_err(|e| err(0, e.to_string()))?;
            compiled_lets.push(Line {
                name: n.clone(),
                expr: e.clone(),
                code,
            });
            names.push(n);
        }
        let mut compiled_channels = Vec::new();
        for (slot, n, e) in channels {
            let code = e.compile_named(&names).map_err(|e| err(0, e.to_string()))?;
            compiled_channels.push((
                slot,
                Line {
                    name: n,
                    expr: e,
                    code,
                },
            ));
        }
        compiled_channels.sort_by_key(|(s, _)| *s);
        let law = Self {
            name,
            params,
            period,
            period_code,
            stagger,
            lets: compiled_lets,
            channels: compiled_channels,
        };
        // the defaults must give a valid timing, so a law that reads always has one instance
        law.bind(&[])
            .map_err(|e| err(0, format!("with the default parameters: {e}")))?;
        Ok(law)
    }

    /// The law's name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The declared parameters, in file order
    #[must_use]
    pub fn params(&self) -> &[MotionParam] {
        &self.params
    }

    /// The identifier of the law: its kind, [`MOTION_SEMANTICS_ID`], its name, every
    /// parameter (name, default, range) and every expression as a tree (spelling and
    /// spacing do not count; notes and comments are not in it)
    #[must_use]
    pub fn law_id(&self) -> [u8; 32] {
        self.law_id_under(&MOTION_SEMANTICS_ID)
    }

    fn law_id_under(&self, semantics: &[u8; 32]) -> [u8; 32] {
        let mut h = LawIdHasher::new(MOTION_LAW_KIND, semantics)
            .str(&self.name)
            .u64(self.params.len() as u64);
        for p in &self.params {
            h = h.str(&p.name).f64(p.default).f64(p.lo).f64(p.hi);
        }
        h = self.period.hash_into(h.u32(0));
        h = match &self.stagger {
            Some((e, _)) => e.hash_into(h.u32(1)),
            None => h.u32(2),
        };
        h = h.u64(self.lets.len() as u64);
        for l in &self.lets {
            h = l.expr.hash_into(h.str(&l.name));
        }
        h = h.u64(self.channels.len() as u64);
        for (_, c) in &self.channels {
            h = c.expr.hash_into(h.str(&c.name));
        }
        h.finish()
    }

    /// The law with parameter values: each named one must be declared, given once and in its
    /// range; the others take their defaults
    ///
    /// # Errors
    ///
    /// [`SampleError::UnknownParam`], [`SampleError::RepeatedParam`],
    /// [`SampleError::ParamOutOfRange`], or [`SampleError::Timing`] when `period` or
    /// `stagger` is outside its bounds for these values
    pub fn bind(&self, values: &[(&str, f64)]) -> Result<BoundMotion<'_>, SampleError> {
        let mut out: Vec<f64> = self.params.iter().map(|p| p.default).collect();
        let mut seen = vec![false; self.params.len()];
        for (n, v) in values {
            let Some(j) = self.params.iter().position(|p| p.name == *n) else {
                return Err(SampleError::UnknownParam((*n).to_owned()));
            };
            if seen[j] {
                return Err(SampleError::RepeatedParam((*n).to_owned()));
            }
            seen[j] = true;
            let p = &self.params[j];
            if !(v.is_finite() && p.lo <= *v && *v <= p.hi) {
                return Err(SampleError::ParamOutOfRange {
                    name: p.name.clone(),
                });
            }
            out[j] = *v;
        }
        let period_ms = self
            .period_code
            .eval(&out)
            .ok()
            .and_then(|v| rounded_ms(v, 1, MAX_PERIOD_MS))
            .ok_or(SampleError::Timing { which: "period" })?;
        let stagger_ms = match &self.stagger {
            Some((_, c)) => c
                .eval(&out)
                .ok()
                .and_then(|v| rounded_ms(v, 0, MAX_PERIOD_MS))
                .ok_or(SampleError::Timing { which: "stagger" })?,
            None => 0,
        };
        Ok(BoundMotion {
            law: self,
            values: out,
            timing: Timing {
                period_ms,
                stagger_ms,
            },
        })
    }
}

/// The largest glyph time a sample reads (2^53: every integer up to it is an exact `f64`)
const MAX_TIME: i128 = 1 << 53;

impl BoundMotion<'_> {
    /// The loop length and the stagger under these parameter values
    #[must_use]
    pub const fn timing(&self) -> Timing {
        self.timing
    }

    /// The value of every parameter, in the law's declared order
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// The channels of glyph `i` of `n` at `t_ms` milliseconds
    ///
    /// # Errors
    ///
    /// [`SampleError::GlyphIndex`] when `i >= n`, [`SampleError::TimeOutOfRange`] when the
    /// glyph's own time is beyond `±2^53`, [`SampleError::NonFinite`] naming the `let` or
    /// channel whose value is not finite, [`SampleError::ScaleNotPositive`]
    #[allow(clippy::cast_precision_loss)] // every integer converted is below 2^53 (checked)
    pub fn sample(&self, t_ms: u64, i: u32, n: u32) -> Result<Channels, SampleError> {
        if i >= n {
            return Err(SampleError::GlyphIndex);
        }
        let period = i128::from(self.timing.period_ms);
        let own_time = i128::from(t_ms) - i128::from(i) * i128::from(self.timing.stagger_ms);
        if !(-MAX_TIME..=MAX_TIME).contains(&own_time) {
            return Err(SampleError::TimeOutOfRange);
        }
        let cycle = own_time.div_euclid(period);
        let rem = own_time.rem_euclid(period);
        let mut slots: Vec<f64> = vec![
            own_time as f64,
            rem as f64 / period as f64,
            cycle as f64,
            f64::from(i),
            f64::from(n),
            std::f64::consts::PI,
        ];
        slots.extend_from_slice(&self.values);
        for l in &self.law.lets {
            let v = l.code.eval(&slots).map_err(|_| SampleError::NonFinite {
                name: l.name.clone(),
            })?;
            slots.push(v);
        }
        let mut out = Channels::IDENTITY;
        for (slot, c) in &self.law.channels {
            let v = c.code.eval(&slots).map_err(|_| SampleError::NonFinite {
                name: c.name.clone(),
            })?;
            #[allow(clippy::cast_possible_truncation)] // the one conversion to f32, by design
            let f = v as f32;
            if !f.is_finite() {
                return Err(SampleError::NonFinite {
                    name: c.name.clone(),
                });
            }
            match slot {
                0 => out.offset_x = f,
                1 => out.offset_y = f,
                2 => out.scale = f,
                3 => out.rotate = f,
                4 => out.dilate = f,
                5 => out.alpha = f.clamp(0.0, 1.0),
                _ => out.luminance = f.clamp(0.0, 1.0),
            }
        }
        if out.scale <= 0.0 {
            return Err(SampleError::ScaleNotPositive);
        }
        Ok(out)
    }
}

/// The law the first motion pin samples: every time name, a parameter, a stagger and every
/// channel, with the clamps and a rounding of the period reached
pub const MOTION_PROBE_LAW: &str = "law motion_probe
kind motion
param k number = 3 range 1 9
period 1000 / k + 0.5
stagger 125
let w = 2 * pi * phase
let s = sin(w)
channel offset_x = t / 1000 + cycle
channel offset_y = s * i / n
channel scale = 1 + 0.25 * cos(w) ^ 2
channel rotate = atan2(s, cos(w))
channel dilate = 0.1 * k - phase
channel alpha = 2 * phase - 0.5
channel luminance = exp(-phase) * 1.5 - 0.25
";

/// The measured value of every pin of [`MOTION_SEMANTICS_PINS`]
///
/// # Panics
///
/// If [`MOTION_PROBE_LAW`] does not read (the oracle reads it first)
#[must_use]
pub fn motion_semantics_fingerprints() -> Vec<(&'static str, [u8; 32])> {
    use sha2::{Digest as _, Sha256};
    let law = MotionLaw::from_file(MOTION_PROBE_LAW).expect("the probe law reads");
    let mut h = Sha256::new();
    for k in [1.0, 3.0, 7.0] {
        let bound = law.bind(&[("k", k)]).expect("the probe binds");
        let t = bound.timing();
        h.update(t.period_ms.to_le_bytes());
        h.update(t.stagger_ms.to_le_bytes());
        for t_ms in [0, 1, 332, 333, 334, 999, 1000, 1001, 4321, 1 << 40] {
            for i in 0..4 {
                match bound.sample(t_ms, i, 4) {
                    Ok(c) => {
                        h.update([1]);
                        for b in c.to_bits() {
                            h.update(b.to_le_bytes());
                        }
                    }
                    Err(e) => {
                        h.update([0]);
                        h.update(e.to_string().as_bytes());
                    }
                }
            }
        }
    }
    vec![
        (MOTION_SEMANTICS_PINS[0].0, h.finalize().into()),
        (
            MOTION_SEMANTICS_PINS[1].0,
            crate::research_law::expression_functions_fingerprint(),
        ),
    ]
}

/// The fold of the measured pins (what [`MOTION_SEMANTICS_ID`] must equal)
#[must_use]
pub fn measured_motion_semantics_id() -> [u8; 32] {
    fold_semantics_pins(&motion_semantics_fingerprints())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifier moves with the motion semantics
    #[test]
    fn the_identifier_moves_with_the_motion_semantics() {
        let law = MotionLaw::from_file(MOTION_PROBE_LAW).unwrap();
        assert_eq!(law.law_id(), law.law_id_under(&MOTION_SEMANTICS_ID));
        let mut other = MOTION_SEMANTICS_ID;
        other[0] ^= 1;
        assert_ne!(law.law_id_under(&other), law.law_id());
    }
}
