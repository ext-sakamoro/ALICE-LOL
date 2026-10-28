//! Material laws (`kind material`): the height, colour and roughness of a surface at a point,
//! computed from named parameters
//!
//! A material law declares parameters (each with a default and a range), intermediate values, the
//! tile the surface repeats over, an optional cell pattern, named noises and three fields:
//! `height` (metres), `albedo` (linear RGB, each in 0..=1) and `roughness` (0..=1). The fields
//! are expressions of the point's place `u`, `v` (metres), the distance `edge` (metres) to the
//! nearest edge of its cell, three numbers `cell0`, `cell1`, `cell2` that the cell pattern gives
//! each cell, and the value of every noise at the point.
//!
//! The law does not evaluate the cells or the noises: the evaluator of the surface does (it owns
//! the cell pattern and the noise), and gives this law their values at the point
//! ([`MaterialPoint`]). What the law fixes is how they are combined, and that they tile: a bound
//! law is refused unless the tile holds every cell period and every noise frequency a whole
//! number of times, so a surface made from it repeats without a seam. The normal of the surface
//! is the gradient of `height` by central difference at the law's `normal_step` (metres); the
//! evaluator takes the four samples.
//!
//! The same law, parameter values and point give the same bits on every platform: the
//! expressions are those of research laws (`f64` without fused multiply-add, `alice-det-math`
//! for every function) and the fields stay `f64`.
//!
//! ```text
//! law pavers
//! kind material
//! claim square pavers with a joint and a fine grain
//! param size number = 0.25 range 0.1 0.5
//! param joint number = 0.008 range 0 0.02
//! let half = joint / 2
//! tile = (1, 1)
//! cell grid = (size, size, 0)
//! noise grain = (16, 0.002, 7)
//! normal_step = 0.001
//! field height = min(edge / half, 1) * 0.004 + grain
//! field albedo = (0.30 + 0.05 * cell0, 0.28, 0.25)
//! field roughness = 0.6
//! ```
//!
//! # Lines
//!
//! - `law <name>` first, then `kind material`; `claim` / `source` lines are notes (not in the
//!   identifier); `#` starts a comment line
//! - `param <name> number = <default> range <lo> <hi>`: a named number, `lo <= default <= hi`
//! - `let <name> = <expr>`: a value for the later lines, before every other line below
//! - `tile = (<tu>, <tv>)`: the size (metres, positive) of the tile the surface repeats over, once
//! - `cell grid = (<pu>, <pv>, <shift>)`: rectangular cells of `pu` × `pv` metres, each row
//!   shifted by `shift` (a fraction of a cell, `0 <= shift < 1`) from the row before; or
//!   `cell voronoi = (<period>, <jitter>)`: cells around one point per `period` × `period`
//!   square, moved by up to `jitter` (`0 <= jitter <= 1`) of it; at most one cell line
//! - `noise <name> = (<frequency>, <amplitude>, <seed>)`: a noise of that many cycles per metre
//!   (positive), scaled by the amplitude, with an integer seed in `0 ..= u32::MAX`; at most 16
//! - `normal_step = <expr>`: the step (metres, positive) of the central difference, once
//! - `point <name> = <expr>`: a named value at the point, read by the later point lines and the
//!   fields; it reads what a field reads and the point lines before it. Its meaning is exact
//!   substitution (the law samples bit for bit as with the expression written out where it is
//!   read), so it names a part of the fields once instead of repeating it
//! - `warp = (<a>, <b>)`: the cell of a point is looked up at `(u + a, v + b)`, `a` and `b`
//!   two declared noises (the same one may be named twice) — the outlines of the cells follow
//!   the noises while the fields still read the point's own `u`, `v`; at most once, and only with
//!   a cell line. The warp's noises are noises like the others, so the tile holds their
//!   frequencies a whole number of times and a warped pattern repeats with the tile
//! - `field height = <expr>`, `field albedo = (<expr>, <expr>, <expr>)`,
//!   `field roughness = <expr>`: each once
//!
//! The `tile`, `cell`, `noise` and `normal_step` expressions read `pi`, the parameters and the
//! `let` values; the fields read those and `u`, `v`, `edge`, `cell0`, `cell1`, `cell2` and the
//! noises by name. Every name is declared once across parameters, lets and noises; `pi`, `u`,
//! `v`, `edge` and `cell0` .. `cell2` are reserved. A tile holds a cell period or a noise
//! frequency a whole number of times when the ratio is within 1e-6 (of a cycle or a cell) of an
//! integer from 1 to 2^20 (a larger count would let the rounding of the ratio itself hide a seam);
//! a grid's row shift tiles when the rows of the tile times the shift is such a whole number. The defaults must tile and give an in-range sample at the point `u = v = edge = 0`
//! with every cell number and noise 0.

use std::fmt;

use alice_sdf::surface::{grid_cell, hash_noise_2d_periodic, voronoi_cell};

use crate::law_id::{fold_semantics_pins, hex32, LawIdHasher};
use crate::law_residual::{compensated_add, Block, LawResidual, ResidualOwner};
use crate::research_law::{CompiledExpr, LawExpr};

/// The seed of the cell numbers when a bound law is evaluated on a surface
/// ([`BoundMaterial::at`])
pub const MATERIAL_CELL_SEED: u32 = 0;

/// The kind tag of a material law, hashed into its identifier
pub const MATERIAL_LAW_KIND: &[u8] = MATERIAL_LAW_KIND_STR.as_bytes();
/// [`MATERIAL_LAW_KIND`] as text (the `for` line of a residual)
pub const MATERIAL_LAW_KIND_STR: &str = "lol.law.material/v1";

/// The most parameters
pub const MAX_PARAMS: usize = 64;
/// The most `let` lines
pub const MAX_LETS: usize = 256;
/// The most noises
pub const MAX_NOISES: usize = 16;
/// The most lines
pub const MAX_LINES: usize = 4096;

/// How close (in cycles, or cells) a ratio must be to a whole number for a tile to hold it
pub const TILE_TOLERANCE: f64 = 1e-6;
/// The most times a tile side may hold a cell period or a noise cycle: beyond it the ratio is not
/// computed finely enough to tell a whole number from one that leaves a seam
pub const MAX_REPEATS: f64 = 1_048_576.0;

/// What the reading of a material law means, as pins measured from the behaviour of this module
/// ([`material_semantics_fingerprints`]; the oracle recomputes every one)
pub const MATERIAL_SEMANTICS_PINS: &[(&str, [u8; 32])] = &[
    (
        "material: f64 fields of research-law expressions over u, v, edge, cell numbers and noises, tiling by whole-number ratios within 1e-6 of a cycle and at most 2^20 repeats",
        hex32("fd750c4dbe67af67a704694f04271680a220ccc353a19b5481f3e9402de17dcd"),
    ),
    (
        "material: the expression functions of research laws",
        hex32("ee1b39dde2b1e577892f19daee2fb514a7b43f56be7d2211061de1ddac28fad3"),
    ),
    (
        "material: warp = the cell of (u, v) looked up at (u + a, v + b) for two declared noises a, b, only with a cell line",
        hex32("403b0e5130d2a1c7f3ffc3a0ed945cb51bf918b656ffd977b70b644b2dab4ba4"),
    ),
    (
        "material: on a surface (BoundMaterial::at) the point is reduced into the tile, each noise is alice_sdf::surface::hash_noise_2d_periodic with its lattice wrapping once per tile, the cell is alice_sdf::surface grid_cell / voronoi_cell at the warped place with seed 0",
        hex32("ed464bafbfb25a30f9b3b3382371bf53208bb8f0c254165fabe3e0a15a9c13e4"),
    ),
];

/// The fold of [`MATERIAL_SEMANTICS_PINS`] (the oracle checks the two agree)
pub const MATERIAL_SEMANTICS_ID: [u8; 32] =
    hex32("91d9e4d933da982139abfceeaf4af71f980133a69517f244c4b83f1d5aa5f708");

/// A material law that does not read, with the line it was on (0 for the whole file)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialLawError {
    /// the 1-based line, 0 when the problem is not on one line
    pub line: usize,
    /// what is wrong
    pub message: String,
}

impl fmt::Display for MaterialLawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "material law: {}", self.message)
        } else {
            write!(f, "material law, line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for MaterialLawError {}

fn err(line: usize, message: impl Into<String>) -> MaterialLawError {
    MaterialLawError {
        line,
        message: message.into(),
    }
}

/// Why a material law cannot be bound or sampled
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterialError {
    /// a residual that does not belong to this bound law or does not lay out as a material's
    Residual(String),
    /// a parameter the law does not declare
    UnknownParam(String),
    /// a parameter given twice
    RepeatedParam(String),
    /// a parameter value outside its declared range, or not finite
    ParamOutOfRange {
        /// the parameter
        name: String,
    },
    /// the tile does not hold a cell period, a row shift or a noise frequency a whole number of
    /// times
    NotTiling {
        /// what does not fit, with the numbers
        what: String,
    },
    /// a tile, cell, noise or step value outside what its line allows
    InvalidValue {
        /// the value and what it must be
        what: String,
    },
    /// an input or a computed value is not finite
    NonFinite {
        /// the input (`u`, `v`, `edge`, a cell number, a noise) or the `let` / field
        field: String,
    },
    /// `albedo` or `roughness` outside 0..=1
    OutOfRange {
        /// `albedo` or `roughness`
        field: String,
    },
    /// the point does not give one value per declared noise
    NoiseCount {
        /// the noises the law declares
        expected: usize,
        /// the values the point gives
        got: usize,
    },
}

impl fmt::Display for MaterialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParam(n) => write!(f, "unknown parameter `{n}`"),
            Self::RepeatedParam(n) => write!(f, "parameter `{n}` given twice"),
            Self::ParamOutOfRange { name } => write!(f, "parameter `{name}` outside its range"),
            Self::NotTiling { what } | Self::InvalidValue { what } => f.write_str(what),
            Self::NonFinite { field } => write!(f, "`{field}` is not finite"),
            Self::Residual(m) => write!(f, "residual: {m}"),
            Self::OutOfRange { field } => write!(f, "`{field}` is outside 0..=1"),
            Self::NoiseCount { expected, got } => {
                write!(
                    f,
                    "the point gives {got} noise value(s), the law declares {expected}"
                )
            }
        }
    }
}

impl std::error::Error for MaterialError {}

/// A declared parameter
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialParam {
    /// its name
    pub name: String,
    /// the value used when none is given
    pub default: f64,
    /// the smallest value
    pub lo: f64,
    /// the largest value
    pub hi: f64,
}

/// The cell pattern of a bound law
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoundCell {
    /// rectangular cells, each row shifted by `row_shift` of a cell
    Grid {
        /// the cell's size along u and v (metres)
        period: [f64; 2],
        /// the shift of a row from the one before, a fraction of a cell
        row_shift: f64,
    },
    /// cells around one point per `period` square, moved by up to `jitter` of it
    Voronoi {
        /// the side of the square (metres)
        period: f64,
        /// how far a point moves, a fraction of the square
        jitter: f64,
    },
}

/// What the evaluator of the surface gives the law at one point
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialPoint {
    /// the place along the surface (metres)
    pub u: f64,
    /// the place across the surface (metres)
    pub v: f64,
    /// the distance to the nearest edge of the point's cell (metres; 0 without a cell line)
    pub edge: f64,
    /// the three numbers of the point's cell (as the evaluator's cell pattern gives them)
    pub cell: [f64; 3],
    /// the value of every declared noise at the point, in declared order
    pub noise: Vec<f64>,
}

/// The fields at one point
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialSample {
    /// metres
    pub height: f64,
    /// linear RGB, each in 0..=1
    pub albedo: [f64; 3],
    /// 0..=1
    pub roughness: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct Line {
    name: String,
    exprs: Vec<LawExpr>,
    codes: Vec<CompiledExpr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellKind {
    Grid,
    Voronoi,
}

/// A material law (`kind material`)
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialLaw {
    name: String,
    params: Vec<MaterialParam>,
    lets: Vec<Line>,
    tile: Line,
    cell: Option<(CellKind, Line)>,
    noises: Vec<Line>,
    /// the places, in `noises`, of the noises that move the cell lookup along u and v
    warp: Option<[usize; 2]>,
    /// the point lines, in file order, read after the noises
    points: Vec<Line>,
    normal_step: Line,
    height: Line,
    albedo: Line,
    roughness: Line,
}

/// A material law with its parameter values fixed: its tile, cells, noises and step, and the
/// fields at any point
#[derive(Debug, Clone)]
pub struct BoundMaterial<'a> {
    law: &'a MaterialLaw,
    values: Vec<f64>,
    /// `pi`, the parameters and the lets, the first slots of every field expression
    slots: Vec<f64>,
    tile: [f64; 2],
    cell: Option<BoundCell>,
    noises: Vec<(f64, f64, u32)>,
    warp: Option<[usize; 2]>,
    normal_step: f64,
}

const RESERVED: [&str; 7] = ["pi", "u", "v", "edge", "cell0", "cell1", "cell2"];
/// the inputs a field reads after `pi`, the parameters and the lets
const POINT_INPUTS: [&str; 6] = ["u", "v", "edge", "cell0", "cell1", "cell2"];

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

fn expr(line: usize, text: &str) -> Result<LawExpr, MaterialLawError> {
    LawExpr::parse(text).map_err(|e| err(line, format!("`{}`: {e}", text.trim())))
}

/// The `n` expressions of `(a, b, ...)`, split at the commas outside any parentheses
fn tuple<'t>(
    line: usize,
    text: &'t str,
    n: usize,
    shape: &str,
) -> Result<Vec<&'t str>, MaterialLawError> {
    let t = text.trim();
    let inner = t
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .ok_or_else(|| err(line, format!("`{shape}` expected")))?;
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    for (k, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&inner[start..k]);
                start = k + 1;
            }
            _ => {}
        }
        if depth < 0 {
            return Err(err(line, "unbalanced parentheses"));
        }
    }
    parts.push(&inner[start..]);
    if parts.len() != n {
        return Err(err(
            line,
            format!("`{shape}` has {n} values, not {}", parts.len()),
        ));
    }
    Ok(parts)
}

/// Parses the expressions and checks them against the names they may read
fn line_of(
    line: usize,
    name: &str,
    texts: &[&str],
    names: &[&str],
) -> Result<Line, MaterialLawError> {
    let mut exprs = Vec::new();
    let mut codes = Vec::new();
    for t in texts {
        let e = expr(line, t)?;
        codes.push(
            e.compile_named(names)
                .map_err(|e| err(line, e.to_string()))?,
        );
        exprs.push(e);
    }
    Ok(Line {
        name: name.to_owned(),
        exprs,
        codes,
    })
}

/// The whole number `ratio` is close to, if it is within [`TILE_TOLERANCE`] of it and from 1 to
/// [`MAX_REPEATS`]
fn whole(ratio: f64) -> Option<f64> {
    let r = ratio.round();
    (ratio.is_finite() && (1.0..=MAX_REPEATS).contains(&r) && (ratio - r).abs() <= TILE_TOLERANCE)
        .then_some(r)
}

impl MaterialLaw {
    /// Reads a material law file
    ///
    /// # Errors
    ///
    /// [`MaterialLawError`] naming the line: not `kind material`, an unknown line, a name
    /// declared twice or reserved, a parameter whose default is outside its range, a `let` after
    /// another kind of line, a `tile`, `cell`, `normal_step` or field line given twice, a cell
    /// that is neither `grid` nor `voronoi`, a tuple with the wrong number of values, more noises
    /// than [`MAX_NOISES`], an expression that does not parse or names something it may not
    /// read, a missing field, `tile` or `normal_step` line, more lines, parameters or lets than
    /// the limits, or defaults that do not tile or do not give an in-range sample at the zero
    /// point
    #[allow(clippy::too_many_lines)] // one arm per kind of line, in file order
    pub fn from_file(text: &str) -> Result<Self, MaterialLawError> {
        let mut name = None;
        let mut kind = false;
        let mut params: Vec<MaterialParam> = Vec::new();
        let mut lets: Vec<Line> = Vec::new();
        let mut tile: Option<Line> = None;
        let mut cell: Option<(CellKind, Line)> = None;
        let mut noises: Vec<Line> = Vec::new();
        let mut normal_step: Option<Line> = None;
        // the warp's two names and its line, resolved once every noise is known
        let mut warp: Option<(usize, [String; 2])> = None;
        // the point lines as text until every noise is known
        let mut point_texts: Vec<(usize, &str, &str)> = Vec::new();
        let mut height: Option<Line> = None;
        let mut albedo: Option<Line> = None;
        let mut roughness: Option<Line> = None;
        // the field lines as text until every noise is known (a field may read a later noise)
        let mut fields: Vec<(usize, &str, Vec<&str>)> = Vec::new();
        let mut after_lets = false;
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
                if t != "kind material" {
                    return Err(err(line, "the second line is `kind material`"));
                }
                kind = true;
                continue;
            }
            let taken = |n: &str, params: &[MaterialParam], lets: &[Line], noises: &[Line]| {
                RESERVED.contains(&n)
                    || params.iter().any(|p| p.name == n)
                    || lets.iter().any(|l| l.name == n)
                    || noises.iter().any(|l| l.name == n)
            };
            // what the tile, cell, noise and step lines read
            let mut config: Vec<&str> = vec!["pi"];
            config.extend(params.iter().map(|p| p.name.as_str()));
            config.extend(lets.iter().map(|l| l.name.as_str()));
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
                    if after_lets {
                        return Err(err(line, "`param` comes before the other lines"));
                    }
                    if !is_name(n) || taken(n, &params, &lets, &noises) {
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
                    params.push(MaterialParam {
                        name: n.to_owned(),
                        default: d,
                        lo,
                        hi,
                    });
                }
                "let" => {
                    if after_lets {
                        return Err(err(
                            line,
                            "`let` comes before the tile, cell, noise, step and field lines",
                        ));
                    }
                    let Some((n, e)) = rest.split_once('=') else {
                        return Err(err(line, "`let <name> = <expr>` expected"));
                    };
                    let n = n.trim();
                    if !is_name(n) || taken(n, &params, &lets, &noises) {
                        return Err(err(line, format!("`{n}` is reserved or declared twice")));
                    }
                    if lets.len() == MAX_LETS {
                        return Err(err(line, format!("more than {MAX_LETS} `let` lines")));
                    }
                    let l = line_of(line, n, &[e], &config)?;
                    lets.push(l);
                }
                "tile" | "normal_step" | "=" => {
                    after_lets = true;
                    // `tile = (...)` and `normal_step = <expr>` split as word `tile`, rest `= ...`
                    let Some(e) = rest.strip_prefix('=') else {
                        return Err(err(line, format!("`{word} = ...` expected")));
                    };
                    let slot = if word == "tile" {
                        &mut tile
                    } else {
                        &mut normal_step
                    };
                    if slot.is_some() {
                        return Err(err(line, format!("`{word}` given twice")));
                    }
                    let texts = if word == "tile" {
                        tuple(line, e, 2, "tile = (<tu>, <tv>)")?
                    } else {
                        vec![e]
                    };
                    *slot = Some(line_of(line, word, &texts, &config)?);
                }
                "cell" => {
                    after_lets = true;
                    if cell.is_some() {
                        return Err(err(line, "`cell` given twice"));
                    }
                    let (k, e) = rest
                        .split_once('=')
                        .map_or(("", ""), |(k, e)| (k.trim(), e));
                    let (ck, texts) = match k {
                        "grid" => (
                            CellKind::Grid,
                            tuple(line, e, 3, "cell grid = (<pu>, <pv>, <shift>)")?,
                        ),
                        "voronoi" => (
                            CellKind::Voronoi,
                            tuple(line, e, 2, "cell voronoi = (<period>, <jitter>)")?,
                        ),
                        _ => {
                            return Err(err(
                                line,
                                "`cell grid = (...)` or `cell voronoi = (...)` expected",
                            ))
                        }
                    };
                    cell = Some((ck, line_of(line, k, &texts, &config)?));
                }
                "noise" => {
                    after_lets = true;
                    let Some((n, e)) = rest.split_once('=') else {
                        return Err(err(
                            line,
                            "`noise <name> = (<frequency>, <amplitude>, <seed>)` expected",
                        ));
                    };
                    let n = n.trim();
                    if !is_name(n) || taken(n, &params, &lets, &noises) {
                        return Err(err(line, format!("`{n}` is reserved or declared twice")));
                    }
                    if noises.len() == MAX_NOISES {
                        return Err(err(line, format!("more than {MAX_NOISES} noises")));
                    }
                    let texts = tuple(
                        line,
                        e,
                        3,
                        "noise <name> = (<frequency>, <amplitude>, <seed>)",
                    )?;
                    noises.push(line_of(line, n, &texts, &config)?);
                }
                "point" => {
                    after_lets = true;
                    let Some((n, e)) = rest.split_once('=') else {
                        return Err(err(line, "`point <name> = <expr>` expected"));
                    };
                    let n = n.trim();
                    if !is_name(n)
                        || taken(n, &params, &lets, &noises)
                        || point_texts.iter().any(|(_, m, _)| *m == n)
                    {
                        return Err(err(line, format!("`{n}` is reserved or declared twice")));
                    }
                    if point_texts.len() == MAX_LETS {
                        return Err(err(line, format!("more than {MAX_LETS} `point` lines")));
                    }
                    point_texts.push((line, n, e));
                }
                "warp" => {
                    after_lets = true;
                    let Some(e) = rest.strip_prefix('=') else {
                        return Err(err(line, "`warp = (<noise>, <noise>)` expected"));
                    };
                    if warp.is_some() {
                        return Err(err(line, "`warp` given twice"));
                    }
                    let names = tuple(line, e, 2, "warp = (<noise>, <noise>)")?;
                    let [a, b] = [names[0].trim(), names[1].trim()];
                    if !is_name(a) || !is_name(b) {
                        return Err(err(line, "a warp names two noises"));
                    }
                    warp = Some((line, [a.to_owned(), b.to_owned()]));
                }
                "field" => {
                    after_lets = true;
                    let Some((f, e)) = rest.split_once('=') else {
                        return Err(err(line, "`field <name> = ...` expected"));
                    };
                    let f = f.trim();
                    let texts = match f {
                        "height" | "roughness" => vec![e],
                        "albedo" => tuple(line, e, 3, "field albedo = (<r>, <g>, <b>)")?,
                        _ => {
                            return Err(err(
                                line,
                                format!("unknown field `{f}` (height, albedo or roughness)"),
                            ))
                        }
                    };
                    if fields.iter().any(|(_, g, _)| *g == f) {
                        return Err(err(line, format!("`field {f}` given twice")));
                    }
                    fields.push((line, f, texts));
                }
                _ => return Err(err(line, format!("unknown line `{word}`"))),
            }
        }
        let name = name.ok_or_else(|| err(0, "empty (no `law <name>` line)"))?;
        if !kind {
            return Err(err(0, "not a material law (no `kind material`)"));
        }
        let mut names: Vec<&str> = vec!["pi"];
        names.extend(params.iter().map(|p| p.name.as_str()));
        names.extend(lets.iter().map(|l| l.name.as_str()));
        names.extend(POINT_INPUTS);
        names.extend(noises.iter().map(|l| l.name.as_str()));
        // each point reads the point lines before it; a noise declared after a point line may not
        // take its name
        let mut points: Vec<Line> = Vec::with_capacity(point_texts.len());
        for (line, n, e) in &point_texts {
            if noises.iter().any(|l| l.name == *n) {
                return Err(err(*line, format!("`{n}` is reserved or declared twice")));
            }
            points.push(line_of(*line, n, &[e], &names)?);
            names.push(n);
        }
        for (line, f, texts) in &fields {
            let l = line_of(*line, f, texts, &names)?;
            match *f {
                "height" => height = Some(l),
                "albedo" => albedo = Some(l),
                _ => roughness = Some(l),
            }
        }
        let height = height.ok_or_else(|| err(0, "no `field height` line"))?;
        let albedo = albedo.ok_or_else(|| err(0, "no `field albedo` line"))?;
        let roughness = roughness.ok_or_else(|| err(0, "no `field roughness` line"))?;
        let tile = tile.ok_or_else(|| err(0, "no `tile` line"))?;
        let normal_step = normal_step.ok_or_else(|| err(0, "no `normal_step` line"))?;
        let warp = match warp {
            None => None,
            Some((line, names)) => {
                if cell.is_none() {
                    return Err(err(
                        line,
                        "a warp moves the cell lookup: it needs a cell line",
                    ));
                }
                let place = |n: &str| {
                    noises
                        .iter()
                        .position(|l| l.name == n)
                        .ok_or_else(|| err(line, format!("`{n}` is not a declared noise")))
                };
                Some([place(&names[0])?, place(&names[1])?])
            }
        };
        let law = Self {
            name,
            params,
            lets,
            tile,
            cell,
            noises,
            warp,
            points,
            normal_step,
            height,
            albedo,
            roughness,
        };
        // the defaults must tile and give an in-range sample, so a law that reads always has one
        // instance
        let zero = MaterialPoint {
            u: 0.0,
            v: 0.0,
            edge: 0.0,
            cell: [0.0; 3],
            noise: vec![0.0; law.noises.len()],
        };
        law.bind(&[])
            .and_then(|b| b.sample(&zero).map(|_| ()))
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
    pub fn params(&self) -> &[MaterialParam] {
        &self.params
    }

    /// The names of the noises, in declared order (the order of [`MaterialPoint::noise`])
    pub fn noise_names(&self) -> impl Iterator<Item = &str> {
        self.noises.iter().map(|n| n.name.as_str())
    }

    /// The identifier of the law: its kind, [`MATERIAL_SEMANTICS_ID`], its name, every parameter
    /// (name, default, range), every `let`, the tile, the cell line (its kind), every noise, the
    /// step and the three fields, each expression as a tree (spelling and spacing do not count;
    /// notes and comments are not in it)
    #[must_use]
    pub fn law_id(&self) -> [u8; 32] {
        self.law_id_under(&MATERIAL_SEMANTICS_ID)
    }

    fn law_id_under(&self, semantics: &[u8; 32]) -> [u8; 32] {
        let hash_line = |mut h: LawIdHasher, l: &Line| {
            h = h.str(&l.name).u64(l.exprs.len() as u64);
            for e in &l.exprs {
                h = e.hash_into(h);
            }
            h
        };
        let mut h = LawIdHasher::new(MATERIAL_LAW_KIND, semantics)
            .str(&self.name)
            .u64(self.params.len() as u64);
        for p in &self.params {
            h = h.str(&p.name).f64(p.default).f64(p.lo).f64(p.hi);
        }
        h = h.u64(self.lets.len() as u64);
        for l in &self.lets {
            h = hash_line(h, l);
        }
        h = hash_line(h, &self.tile);
        h = match &self.cell {
            None => h.u64(0),
            Some((CellKind::Grid, l)) => hash_line(h.u64(1), l),
            Some((CellKind::Voronoi, l)) => hash_line(h.u64(2), l),
        };
        h = h.u64(self.noises.len() as u64);
        for l in &self.noises {
            h = hash_line(h, l);
        }
        if let Some([a, b]) = self.warp {
            h = h.str("warp").u64(a as u64).u64(b as u64);
        }
        for l in &self.points {
            h = hash_line(h.str("point"), l);
        }
        h = hash_line(h, &self.normal_step);
        h = hash_line(h, &self.height);
        h = hash_line(h, &self.albedo);
        hash_line(h, &self.roughness).finish()
    }

    /// The law with parameter values: each named value must be declared, given once and in its
    /// range; the others take their defaults. The tile, cells, noises and step are computed and
    /// checked
    ///
    /// # Errors
    ///
    /// [`MaterialError::UnknownParam`], [`MaterialError::RepeatedParam`],
    /// [`MaterialError::ParamOutOfRange`], [`MaterialError::NonFinite`] naming the `let` or line
    /// whose value is not finite, [`MaterialError::InvalidValue`] for a tile, period, frequency
    /// or step that is not positive, a shift outside `0..1`, a jitter outside `0..=1` or a seed
    /// that is not an integer in `0 ..= u32::MAX`, and [`MaterialError::NotTiling`] when the tile
    /// does not hold a cell period, a row shift or a noise frequency a whole number of times
    #[allow(clippy::too_many_lines)] // one check per value, in line order
    pub fn bind(&self, values: &[(&str, f64)]) -> Result<BoundMaterial<'_>, MaterialError> {
        let mut vals: Vec<f64> = self.params.iter().map(|p| p.default).collect();
        let mut seen = vec![false; self.params.len()];
        for (n, v) in values {
            let Some(j) = self.params.iter().position(|p| p.name == *n) else {
                return Err(MaterialError::UnknownParam((*n).to_owned()));
            };
            if seen[j] {
                return Err(MaterialError::RepeatedParam((*n).to_owned()));
            }
            seen[j] = true;
            let p = &self.params[j];
            if !(v.is_finite() && p.lo <= *v && *v <= p.hi) {
                return Err(MaterialError::ParamOutOfRange {
                    name: p.name.clone(),
                });
            }
            vals[j] = *v;
        }
        let mut slots = vec![std::f64::consts::PI];
        slots.extend_from_slice(&vals);
        // the evaluator refuses a value that is not finite (it never returns one)
        let finite = |field: &str, v: Result<f64, _>| -> Result<f64, MaterialError> {
            v.map_err(|_| MaterialError::NonFinite {
                field: field.to_owned(),
            })
        };
        for l in &self.lets {
            let v = finite(&l.name, l.codes[0].eval(&slots))?;
            slots.push(v);
        }
        let eval = |l: &Line, slots: &[f64]| -> Result<Vec<f64>, MaterialError> {
            l.codes
                .iter()
                .map(|c| finite(&l.name, c.eval(slots)))
                .collect()
        };
        let invalid = |what: String| MaterialError::InvalidValue { what };
        let not_tiling = |what: String| MaterialError::NotTiling { what };

        let t = eval(&self.tile, &slots)?;
        let tile = [t[0], t[1]];
        if !(tile[0] > 0.0 && tile[1] > 0.0) {
            return Err(invalid(format!(
                "tile {} x {} m: both sides must be positive",
                tile[0], tile[1]
            )));
        }
        let fits = |side: f64, period: f64, what: &str| -> Result<f64, MaterialError> {
            whole(side / period).ok_or_else(|| {
                not_tiling(format!(
                    "the tile side {side} m holds {} {what} (not a whole number)",
                    side / period
                ))
            })
        };
        let cell = match &self.cell {
            None => None,
            Some((CellKind::Grid, l)) => {
                let c = eval(l, &slots)?;
                let (pu, pv, shift) = (c[0], c[1], c[2]);
                if !(pu > 0.0 && pv > 0.0) {
                    return Err(invalid(format!(
                        "cell grid {pu} x {pv} m: both sides must be positive"
                    )));
                }
                if !(0.0..1.0).contains(&shift) {
                    return Err(invalid(format!(
                        "cell grid row shift {shift}: must be in 0..1"
                    )));
                }
                fits(tile[0], pu, "cells along u")?;
                let rows = fits(tile[1], pv, "cells along v")?;
                if shift != 0.0 && whole(rows * shift).is_none() {
                    return Err(not_tiling(format!(
                        "{rows} rows of the tile shifted by {shift} of a cell each move {} cells (not a whole number)",
                        rows * shift
                    )));
                }
                Some(BoundCell::Grid {
                    period: [pu, pv],
                    row_shift: shift,
                })
            }
            Some((CellKind::Voronoi, l)) => {
                let c = eval(l, &slots)?;
                let (period, jitter) = (c[0], c[1]);
                if period <= 0.0 {
                    return Err(invalid(format!(
                        "cell voronoi period {period} m: must be positive"
                    )));
                }
                if !(0.0..=1.0).contains(&jitter) {
                    return Err(invalid(format!(
                        "cell voronoi jitter {jitter}: must be in 0..=1"
                    )));
                }
                fits(tile[0], period, "cells along u")?;
                fits(tile[1], period, "cells along v")?;
                Some(BoundCell::Voronoi { period, jitter })
            }
        };
        let mut noises = Vec::with_capacity(self.noises.len());
        for l in &self.noises {
            let c = eval(l, &slots)?;
            let (freq, amp, noise_seed) = (c[0], c[1], c[2]);
            if freq <= 0.0 {
                return Err(invalid(format!(
                    "noise `{}` frequency {freq}: must be positive",
                    l.name
                )));
            }
            if !(noise_seed >= 0.0
                && noise_seed <= f64::from(u32::MAX)
                && noise_seed.fract() == 0.0)
            {
                return Err(invalid(format!(
                    "noise `{}` seed {noise_seed}: must be an integer in 0..=4294967295",
                    l.name
                )));
            }
            for side in tile {
                if whole(freq * side).is_none() {
                    return Err(not_tiling(format!(
                        "noise `{}` at {freq} cycles per metre makes {} cycles across the tile side {side} m (not a whole number)",
                        l.name,
                        freq * side
                    )));
                }
            }
            // the seed is an integer in u32 range (checked above), so the cast is exact
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            noises.push((freq, amp, noise_seed as u32));
        }
        let step = eval(&self.normal_step, &slots)?[0];
        if step <= 0.0 {
            return Err(invalid(format!("normal_step {step} m: must be positive")));
        }
        Ok(BoundMaterial {
            law: self,
            values: vals,
            slots,
            tile,
            cell,
            noises,
            warp: self.warp,
            normal_step: step,
        })
    }
}

impl BoundMaterial<'_> {
    /// The value of every parameter, in the law's declared order
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// The tile's sides along u and v (metres)
    #[must_use]
    pub const fn tile(&self) -> [f64; 2] {
        self.tile
    }

    /// The cell pattern, if the law has a cell line
    #[must_use]
    pub const fn cell(&self) -> Option<BoundCell> {
        self.cell
    }

    /// Every noise: its name, frequency (cycles per metre), amplitude and seed, in declared order
    pub fn noises(&self) -> impl Iterator<Item = (&str, f64, f64, u32)> {
        self.law
            .noises
            .iter()
            .zip(&self.noises)
            .map(|(l, (f, a, s))| (l.name.as_str(), *f, *a, *s))
    }

    /// The step (metres) of the central difference that gives the normal
    #[must_use]
    pub const fn normal_step(&self) -> f64 {
        self.normal_step
    }

    /// What the surface gives the law at `(u, v)` (metres), from the law's own cells and noises
    /// (`alice_sdf::surface`): the point is reduced into the tile (`u - tu·floor(u / tu)`, and
    /// v) for every lookup; each noise `(f, amp, seed)` is
    /// `amp · hash_noise_2d_periodic(u'·f, v'·f, [tu·f, tv·f], seed)`, its lattice wrapping once
    /// per tile; the cell is looked up at the reduced point moved by the warp's noises (the point
    /// itself without a warp), with the tile's cell counts and [`MATERIAL_CELL_SEED`]; `u` and
    /// `v` stay the point's own, as the fields read them
    ///
    /// # Errors
    ///
    /// [`MaterialError::NonFinite`] naming `u` or `v` when it is not finite
    #[allow(clippy::suboptimal_flops)] // unfused: the law's values are f64 without fused multiply-add
    pub fn point_at(&self, u: f64, v: f64) -> Result<MaterialPoint, MaterialError> {
        self.point_with(u, v, None)
    }

    /// [`Self::point_at`], the cell from `map` instead of the law's cell line when given
    #[allow(clippy::suboptimal_flops)] // unfused: the law's values are f64 without fused multiply-add
    #[allow(clippy::option_if_let_else)] // the table's cells, else the law's own: read in that order
    fn point_with(
        &self,
        u: f64,
        v: f64,
        map: Option<&CellMap>,
    ) -> Result<MaterialPoint, MaterialError> {
        for (name, x) in [("u", u), ("v", v)] {
            if !x.is_finite() {
                return Err(MaterialError::NonFinite {
                    field: name.to_owned(),
                });
            }
        }
        let [tu, tv] = self.tile;
        let (ur, vr) = (u - tu * (u / tu).floor(), v - tv * (v / tv).floor());
        // the bind checked that the tile holds every frequency and period a whole number of
        // times (at most 2^20), so the rounded counts are those whole numbers
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = |x: f64| x.round() as u32;
        #[allow(clippy::cast_possible_truncation)]
        let noise: Vec<f64> = self
            .noises
            .iter()
            .map(|&(f, amp, seed)| {
                let n = hash_noise_2d_periodic(
                    (ur * f) as f32,
                    (vr * f) as f32,
                    [count(tu * f), count(tv * f)],
                    seed,
                );
                amp * f64::from(n)
            })
            .collect();
        let (cu, cv) = match self.warp {
            Some([a, b]) => (ur + noise[a], vr + noise[b]),
            None => (ur, vr),
        };
        let (edge, cell) = if let Some(m) = map {
            m.lookup(cu, cv, self.tile)
        } else {
            match self.cell {
                None => (0.0, [0.0; 3]),
                Some(BoundCell::Grid { period, row_shift }) => {
                    let tiles = [count(tu / period[0]), count(tv / period[1])];
                    let c = grid_cell(cu, cv, period, row_shift, tiles, MATERIAL_CELL_SEED);
                    (c.edge, c.numbers)
                }
                Some(BoundCell::Voronoi { period, jitter }) => {
                    let tiles = [count(tu / period), count(tv / period)];
                    let c = voronoi_cell(cu, cv, period, jitter, tiles, MATERIAL_CELL_SEED);
                    (c.edge, c.numbers)
                }
            }
        };
        Ok(MaterialPoint {
            u,
            v,
            edge,
            cell,
            noise,
        })
    }

    /// The fields at `(u, v)` on the surface: [`Self::sample`] of [`Self::point_at`]
    ///
    /// # Errors
    ///
    /// As [`Self::point_at`] and [`Self::sample`]
    pub fn at(&self, u: f64, v: f64) -> Result<MaterialSample, MaterialError> {
        self.sample(&self.point_at(u, v)?)
    }

    /// The unit normal of the surface at `(u, v)`: `(-∂h/∂u, -∂h/∂v, 1)` normalised, the
    /// derivatives of `height` by central difference at [`Self::normal_step`]
    ///
    /// # Errors
    ///
    /// As [`Self::at`] at the four points
    pub fn normal_at(&self, u: f64, v: f64) -> Result<[f64; 3], MaterialError> {
        let s = self.normal_step;
        let h = |x: f64, y: f64| self.at(x, y).map(|p| p.height);
        let gu = (h(u + s, v)? - h(u - s, v)?) / (2.0 * s);
        let gv = (h(u, v + s)? - h(u, v - s)?) / (2.0 * s);
        let len = (gu * gu + gv * gv + 1.0).sqrt();
        Ok([-gu / len, -gv / len, 1.0 / len])
    }

    /// The places, in [`MaterialPoint::noise`] order, of the two noises that move the cell
    /// lookup: the cell of `(u, v)` is the cell of `(u + noise[a], v + noise[b])`; `None`
    /// without a `warp` line (the cell of the point itself)
    #[must_use]
    pub const fn warp(&self) -> Option<[usize; 2]> {
        self.warp
    }

    /// The fields at a point
    ///
    /// # Errors
    ///
    /// [`MaterialError::NoiseCount`] when the point does not give one value per noise,
    /// [`MaterialError::NonFinite`] naming an input that is not finite or a field whose value is
    /// not, and [`MaterialError::OutOfRange`] for an `albedo` channel or `roughness` outside
    /// 0..=1
    pub fn sample(&self, p: &MaterialPoint) -> Result<MaterialSample, MaterialError> {
        if p.noise.len() != self.noises.len() {
            return Err(MaterialError::NoiseCount {
                expected: self.noises.len(),
                got: p.noise.len(),
            });
        }
        let mut slots = self.slots.clone();
        let inputs = [p.u, p.v, p.edge, p.cell[0], p.cell[1], p.cell[2]];
        for (n, v) in POINT_INPUTS.iter().zip(inputs) {
            if !v.is_finite() {
                return Err(MaterialError::NonFinite {
                    field: (*n).to_owned(),
                });
            }
            slots.push(v);
        }
        for (l, v) in self.law.noises.iter().zip(&p.noise) {
            if !v.is_finite() {
                return Err(MaterialError::NonFinite {
                    field: l.name.clone(),
                });
            }
            slots.push(*v);
        }
        for l in &self.law.points {
            let v = l.codes[0]
                .eval(&slots)
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| MaterialError::NonFinite {
                    field: l.name.clone(),
                })?;
            slots.push(v);
        }
        let field = |l: &Line| -> Result<Vec<f64>, MaterialError> {
            l.codes
                .iter()
                .map(|c| {
                    c.eval(&slots).map_err(|_| MaterialError::NonFinite {
                        field: l.name.clone(),
                    })
                })
                .collect()
        };
        let height = field(&self.law.height)?[0];
        let a = field(&self.law.albedo)?;
        let roughness = field(&self.law.roughness)?[0];
        let unit = |v: f64| (0.0..=1.0).contains(&v);
        if !a.iter().all(|v| unit(*v)) {
            return Err(MaterialError::OutOfRange {
                field: "albedo".to_owned(),
            });
        }
        if !unit(roughness) {
            return Err(MaterialError::OutOfRange {
                field: "roughness".to_owned(),
            });
        }
        Ok(MaterialSample {
            height,
            albedo: [a[0], a[1], a[2]],
            roughness,
        })
    }
}

/// The law the first material pin evaluates: parameters, lets, a brick grid, two noises and
/// fields that read every input and the expression functions
pub const MATERIAL_PROBE_LAW: &str = "law material_probe
kind material
param a number = 0.25 range 0.125 0.5
param b number = 0.3 range 0 1
let c = a / 2
tile = (1, 2)
cell grid = (a, a, 0.5)
noise n1 = (8, 0.01, 3)
noise n2 = (2, 0.5, 4294967295)
normal_step = c / 100
field height = min(edge / c, 1) * 0.004 + n1 * sin(u * pi) + exp(-v) * 0.001
field albedo = (0.2 + 0.1 * cell0, max(0, 0.3 - b * n2 * 0.1), min(1, sqrt(cell2 + 0.01)))
field roughness = max(0, min(1, 0.5 + atan2(n2, 1 + cell1) * b / pi))
";

/// The fields and normals [`BoundMaterial::at`] gives the probe law, with a warp, at points in
/// and beyond the tile
#[allow(clippy::suboptimal_flops)] // unfused, as the values it pins
fn surface_fingerprint() -> [u8; 32] {
    use sha2::{Digest as _, Sha256};
    let text = MATERIAL_PROBE_LAW.replace("normal_step", "warp = (n1, n2)\nnormal_step");
    let law = MaterialLaw::from_file(&text).expect("the probe law with a warp reads");
    let bound = law.bind(&[]).expect("the probe binds");
    let mut h = Sha256::new();
    for k in 0..12u8 {
        let step = f64::from(k) * 0.37 - 1.1;
        let (pu, pv) = (step, 2.3 - step * 1.7);
        let sample = bound.at(pu, pv).expect("the probe samples on its surface");
        let normal = bound.normal_at(pu, pv).expect("the probe has a normal");
        for value in [
            sample.height,
            sample.albedo[0],
            sample.albedo[1],
            sample.albedo[2],
            sample.roughness,
            normal[0],
            normal[1],
            normal[2],
        ] {
            h.update(value.to_bits().to_le_bytes());
        }
    }
    h.finalize().into()
}

/// The measured value of every pin of [`MATERIAL_SEMANTICS_PINS`]
///
/// # Panics
///
/// If [`MATERIAL_PROBE_LAW`] does not read (the oracle reads it first)
#[must_use]
pub fn material_semantics_fingerprints() -> Vec<(&'static str, [u8; 32])> {
    use sha2::{Digest as _, Sha256};
    let law = MaterialLaw::from_file(MATERIAL_PROBE_LAW).expect("the probe law reads");
    let mut h = Sha256::new();
    for a in [0.125, 0.25, 0.5] {
        for b in [0.0, 0.3, 1.0] {
            let bound = law.bind(&[("a", a), ("b", b)]).expect("the probe binds");
            for v in bound.tile() {
                h.update(v.to_bits().to_le_bytes());
            }
            if let Some(BoundCell::Grid { period, row_shift }) = bound.cell() {
                for v in [period[0], period[1], row_shift] {
                    h.update(v.to_bits().to_le_bytes());
                }
            }
            for (n, f, amp, s) in bound.noises() {
                h.update(n.as_bytes());
                h.update(f.to_bits().to_le_bytes());
                h.update(amp.to_bits().to_le_bytes());
                h.update(s.to_le_bytes());
            }
            h.update(bound.normal_step().to_bits().to_le_bytes());
            for k in 0..5u8 {
                let x = f64::from(k) * 0.37;
                let point = MaterialPoint {
                    u: x,
                    v: 1.5 - x,
                    edge: x * 0.01,
                    cell: [x / 2.0, 1.0 - x / 3.0, x / 4.0],
                    noise: vec![x * 0.1 - 0.2, 0.9 - x],
                };
                let s = bound.sample(&point).expect("the probe samples");
                for v in [s.height, s.albedo[0], s.albedo[1], s.albedo[2], s.roughness] {
                    h.update(v.to_bits().to_le_bytes());
                }
            }
        }
    }
    // the tiling rule itself: which ratios a tile holds
    for ratio in [
        1.0,
        16.0,
        16.000_000_5,
        16.000_002,
        0.5,
        1_048_576.0,
        1_048_577.0,
        1e9 + 0.5,
    ] {
        match whole(ratio) {
            Some(r) => {
                h.update([1]);
                h.update(r.to_bits().to_le_bytes());
            }
            None => h.update([0]),
        }
    }
    // the warp: which noises a warp line names, and that a warp needs a cell line
    let mut w = Sha256::new();
    for line in ["", "warp = (n1, n2)", "warp = (n2, n1)", "warp = (n2, n2)"] {
        let text = MATERIAL_PROBE_LAW.replace("normal_step", &format!("{line}\nnormal_step"));
        match MaterialLaw::from_file(&text).map(|l| l.warp) {
            Ok(None) => w.update([0]),
            Ok(Some([a, b])) => {
                w.update([1]);
                w.update((a as u64).to_le_bytes());
                w.update((b as u64).to_le_bytes());
            }
            Err(_) => w.update([2]),
        }
    }
    let no_cell = MATERIAL_PROBE_LAW
        .replace("cell grid = (a, a, 0.5)\n", "")
        .replace("normal_step", "warp = (n1, n2)\nnormal_step");
    w.update([u8::from(MaterialLaw::from_file(&no_cell).is_err())]);
    vec![
        (MATERIAL_SEMANTICS_PINS[0].0, h.finalize().into()),
        (
            MATERIAL_SEMANTICS_PINS[1].0,
            crate::research_law::expression_functions_fingerprint(),
        ),
        (MATERIAL_SEMANTICS_PINS[2].0, w.finalize().into()),
        (MATERIAL_SEMANTICS_PINS[3].0, surface_fingerprint()),
    ]
}

/// The fold of the measured pins (what [`MATERIAL_SEMANTICS_ID`] must equal)
#[must_use]
pub fn measured_material_semantics_id() -> [u8; 32] {
    fold_semantics_pins(&material_semantics_fingerprints())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifier moves with the material semantics
    #[test]
    fn the_identifier_moves_with_the_material_semantics() {
        let law = MaterialLaw::from_file(MATERIAL_PROBE_LAW).unwrap();
        assert_eq!(law.law_id(), law.law_id_under(&MATERIAL_SEMANTICS_ID));
        let mut other = MATERIAL_SEMANTICS_ID;
        other[0] ^= 1;
        assert_ne!(law.law_id_under(&other), law.law_id());
    }
}

/// A material law's residual: `texels <W> <H>` over the tile (any whole positive W, H), blocks
/// `height` (W × H), `albedo` (W × H × 3) and `roughness` (W × H), row-major with the column
/// fastest; the bound hash is the law identifier and every parameter value but the excluded ones
impl ResidualOwner for BoundMaterial<'_> {
    fn kind_tag(&self) -> &str {
        MATERIAL_LAW_KIND_STR
    }
    fn law_id(&self) -> [u8; 32] {
        self.law.law_id()
    }
    fn canonical_bound(&self, excluded: &[&str]) -> Vec<u8> {
        let mut out = self.law.law_id().to_vec();
        for (p, v) in self.law.params.iter().zip(&self.values) {
            if !excluded.contains(&p.name.as_str()) {
                out.extend((p.name.len() as u64).to_le_bytes());
                out.extend(p.name.as_bytes());
                out.extend(v.to_bits().to_le_bytes());
            }
        }
        out
    }
    fn index(&self, _domain: &str) -> Option<Vec<String>> {
        None
    }
    fn selector(&self, _param: &str) -> Option<String> {
        None
    }
    fn additive_block(&self, name: &str) -> bool {
        matches!(name, "height" | "albedo" | "roughness")
    }
    fn accepts_index(&self, domain: &str, words: &[String]) -> bool {
        let counts = |n: usize| {
            words.len() == n
                && words
                    .iter()
                    .all(|w| w.parse::<usize>().is_ok_and(|k| k > 0))
        };
        match domain {
            "texels" => counts(2),
            "rows" | "stones" => counts(1),
            _ => false,
        }
    }
}

/// A bound material law with its texel residual: the fields of the law plus the residual,
/// bilinear between texel centres and wrapping around the tile
#[derive(Debug, Clone)]
pub struct ResidualMaterial<'r, 'a> {
    bound: &'r BoundMaterial<'a>,
    /// the cells from the residual's table, if it has one
    map: Option<CellMap>,
    /// the texel grid's columns and rows, [0, 0] without one
    size: [usize; 2],
    /// each grid as (hi, lo) per value; a single-f64 grid (with a quantum) has lo 0
    height: (Vec<f64>, Vec<f64>),
    albedo: (Vec<f64>, Vec<f64>),
    roughness: (Vec<f64>, Vec<f64>),
}

impl<'a> BoundMaterial<'a> {
    /// This bound law with a residual
    ///
    /// # Errors
    ///
    /// [`MaterialError::Residual`] when the residual belongs to another law or instance, has no
    /// `texels` domain, or its blocks are not `height`, `albedo` and `roughness` of the texel
    /// grid's shape
    pub fn with_residual<'r>(
        &'r self,
        residual: &LawResidual,
    ) -> Result<ResidualMaterial<'r, 'a>, MaterialError> {
        let no = |m: String| MaterialError::Residual(m);
        residual
            .check_belongs(self)
            .map_err(|e| no(e.to_string()))?;
        let size = residual.index("texels").map_or([0, 0], |w| {
            [
                w[0].parse::<usize>().unwrap_or(0),
                w[1].parse::<usize>().unwrap_or(0),
            ]
        });
        let take = |name: &str, comps: usize| -> Result<(Vec<f64>, Vec<f64>), MaterialError> {
            match residual.block(name) {
                Some(Block::Dense {
                    domain,
                    shape,
                    values,
                    pairs,
                    when: None,
                    ..
                }) if domain == "texels"
                    && (shape[..] == [size[0], size[1]] && comps == 1
                        || shape[..] == [size[0], size[1], comps]) =>
                {
                    Ok(if *pairs {
                        (
                            values.iter().step_by(2).copied().collect(),
                            values.iter().skip(1).step_by(2).copied().collect(),
                        )
                    } else {
                        (values.clone(), vec![0.0; values.len()])
                    })
                }
                Some(_) => Err(no(format!(
                    "block `{name}` is not a texel grid of {comps} per texel"
                ))),
                None => Err(no(format!("no `{name}` block"))),
            }
        };
        if let Some(b) = residual.blocks().iter().find(|b| {
            !matches!(
                b.name(),
                "height" | "albedo" | "roughness" | "row_bounds" | "stones"
            )
        }) {
            return Err(no(format!(
                "a material residual has no block `{}`",
                b.name()
            )));
        }
        let map = CellMap::from_residual(residual, self.tile).map_err(no)?;
        let grids = if size == [0, 0] {
            if ["height", "albedo", "roughness"]
                .iter()
                .any(|n| residual.block(n).is_some())
            {
                return Err(no("texel blocks without a `texels` domain".into()));
            }
            let none = || (Vec::new(), Vec::new());
            (none(), none(), none())
        } else {
            (
                take("height", 1)?,
                take("albedo", 3)?,
                take("roughness", 1)?,
            )
        };
        Ok(ResidualMaterial {
            bound: self,
            map,
            size,
            height: grids.0,
            albedo: grids.1,
            roughness: grids.2,
        })
    }
}

impl ResidualMaterial<'_, '_> {
    /// The fields at `(u, v)`: the law's ([`BoundMaterial::at`]) plus the residual there, the
    /// albedo and roughness ranges checked after it (a residual of 0 adds nothing)
    ///
    /// # Errors
    ///
    /// As [`BoundMaterial::at`], and [`MaterialError::OutOfRange`] for a field the residual takes
    /// out of 0..=1
    #[allow(clippy::suboptimal_flops)] // unfused, as the law's values
    pub fn at(&self, u: f64, v: f64) -> Result<MaterialSample, MaterialError> {
        let law = self
            .bound
            .sample(&self.bound.point_with(u, v, self.map.as_ref())?)?;
        if self.size == [0, 0] {
            return Ok(law);
        }
        let [tu, tv] = self.bound.tile;
        let (ur, vr) = (u - tu * (u / tu).floor(), v - tv * (v / tv).floor());
        let [cols, rows] = self.size;
        #[allow(clippy::cast_precision_loss)]
        let (gx, gy) = (ur / tu * cols as f64 - 0.5, vr / tv * rows as f64 - 0.5);
        let (fx, fy) = (gx - gx.floor(), gy - gy.floor());
        #[allow(clippy::cast_possible_truncation)]
        let (i0, j0) = (gx.floor() as i64, gy.floor() as i64);
        #[allow(
            clippy::cast_possible_wrap,
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation
        )]
        let wrap = |k: i64, n: usize| k.rem_euclid(n as i64) as usize;
        let (i1, j1) = (wrap(i0 + 1, cols), wrap(j0 + 1, rows));
        let (i0, j0) = (wrap(i0, cols), wrap(j0, rows));
        let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
        let at = |grid: &(Vec<f64>, Vec<f64>), comps: usize, c: usize| {
            let one = |g: &[f64]| {
                let v = |i: usize, j: usize| g[comps * (j * cols + i) + c];
                lerp(
                    lerp(v(i0, j0), v(i1, j0), fx),
                    lerp(v(i0, j1), v(i1, j1), fx),
                    fy,
                )
            };
            (one(&grid.0), one(&grid.1))
        };
        let add = |field: f64, (hi, lo): (f64, f64)| {
            if hi == 0.0 && lo == 0.0 {
                field
            } else {
                compensated_add(field, hi, lo)
            }
        };
        let out = MaterialSample {
            height: add(law.height, at(&self.height, 1, 0)),
            albedo: [
                add(law.albedo[0], at(&self.albedo, 3, 0)),
                add(law.albedo[1], at(&self.albedo, 3, 1)),
                add(law.albedo[2], at(&self.albedo, 3, 2)),
            ],
            roughness: add(law.roughness, at(&self.roughness, 1, 0)),
        };
        let unit = |x: f64| (0.0..=1.0).contains(&x);
        if !out.albedo.iter().all(|a| unit(*a)) {
            return Err(MaterialError::OutOfRange {
                field: "albedo".to_owned(),
            });
        }
        if !unit(out.roughness) {
            return Err(MaterialError::OutOfRange {
                field: "roughness".to_owned(),
            });
        }
        if !out.height.is_finite() {
            return Err(MaterialError::NonFinite {
                field: "height".to_owned(),
            });
        }
        Ok(out)
    }
}

/// A material residual's cell table: rows of stones with rounded corners, replacing the law's
/// procedural cells (see `with_residual`)
#[derive(Debug, Clone, PartialEq)]
struct CellMap {
    /// (v0, v1) per row
    rows: Vec<[f64; 2]>,
    /// (row, u0, u1, cell0, cell1, cell2, radius) per stone
    stones: Vec<[f64; 7]>,
}

impl CellMap {
    /// The table of `row_bounds` (R × 2) and `stones` (N × 7), `None` when the residual has
    /// neither; refused unless rows are ordered within the tile, every stone is in a row, starts
    /// before it ends, is no wider than the tile, has a radius within half its smaller side, and
    /// no two stones of a row overlap (around the tile)
    #[allow(clippy::suboptimal_flops)] // unfused, as the law's values
    fn from_residual(r: &LawResidual, tile: [f64; 2]) -> Result<Option<Self>, String> {
        let table = |name: &str, domain: &str, cols: usize| -> Result<Option<Vec<f64>>, String> {
            match r.block(name) {
                None => Ok(None),
                Some(Block::Dense {
                    domain: d,
                    shape,
                    values,
                    pairs: false,
                    when: None,
                    ..
                }) if d == domain && shape.len() == 2 && shape[1] == cols => {
                    Ok(Some(values.clone()))
                }
                Some(_) => Err(format!(
                    "block `{name}` is not a `{domain}` table of {cols} columns"
                )),
            }
        };
        let (rows, stones) = match (
            table("row_bounds", "rows", 2)?,
            table("stones", "stones", 7)?,
        ) {
            (None, None) => return Ok(None),
            (Some(a), Some(b)) => (a, b),
            _ => return Err("a cell map needs both `row_bounds` and `stones`".into()),
        };
        let [tu, tv] = tile;
        let rows: Vec<[f64; 2]> = rows.as_chunks::<2>().0.to_vec();
        for (k, r) in rows.iter().enumerate() {
            let next = rows.get(k + 1).map_or(tv, |n| n[0]);
            if !(0.0 <= r[0] && r[0] < r[1] && r[1] <= next) {
                return Err(format!(
                    "row {k}: ({}, {}) is not ordered within the tile",
                    r[0], r[1]
                ));
            }
        }
        let stones: Vec<[f64; 7]> = stones.as_chunks::<7>().0.to_vec();
        let mut by_row: Vec<Vec<(f64, f64)>> = vec![Vec::new(); rows.len()];
        for (k, st) in stones.iter().enumerate() {
            let row = st[0];
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let ri = row as usize;
            if row.fract() != 0.0 || row < 0.0 || ri >= rows.len() {
                return Err(format!("stone {k}: row {row} is not a row of the table"));
            }
            let (w, h) = (st[2] - st[1], rows[ri][1] - rows[ri][0]);
            if !(w > 0.0 && w <= tu) {
                return Err(format!(
                    "stone {k}: [{}, {}) is not a span within the tile",
                    st[1], st[2]
                ));
            }
            if !(0.0 <= st[6] && 2.0 * st[6] <= w.min(h)) {
                return Err(format!(
                    "stone {k}: radius {} past half its smaller side",
                    st[6]
                ));
            }
            by_row[ri].push((st[1] - tu * (st[1] / tu).floor(), w));
        }
        for (ri, spans) in by_row.iter_mut().enumerate() {
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            for k in 0..spans.len() {
                let (s0, w0) = spans[k];
                let next = if k + 1 < spans.len() {
                    spans[k + 1].0
                } else {
                    spans[0].0 + tu
                };
                if spans.len() > 1 && s0 + w0 > next || spans.len() == 1 && w0 > tu {
                    return Err(format!("row {ri}: stones overlap"));
                }
            }
        }
        Ok(Some(Self { rows, stones }))
    }

    /// The edge distance and cell numbers at the lookup place `(cu, cv)` (any place; reduced into
    /// the tile): inside a stone's rounded rectangle, the distance to its outline and its numbers;
    /// elsewhere 0 and 0
    // unfused, and the square root of the sum (not libm's hypot), so every platform agrees
    #[allow(clippy::suboptimal_flops, clippy::imprecise_flops)]
    fn lookup(&self, cu: f64, cv: f64, tile: [f64; 2]) -> (f64, [f64; 3]) {
        let [tu, tv] = tile;
        let (u, v) = (cu - tu * (cu / tu).floor(), cv - tv * (cv / tv).floor());
        let joint = (0.0, [0.0; 3]);
        let Some(ri) = self.rows.iter().position(|r| r[0] <= v && v < r[1]) else {
            return joint;
        };
        let [v0, v1] = self.rows[ri];
        // a stone's row was checked to be a whole number below the row count
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        for st in self.stones.iter().filter(|st| st[0] as usize == ri) {
            let (u0, u1, radius) = (st[1], st[2], st[6]);
            let du = (u - u0) - tu * ((u - u0) / tu).floor();
            if du >= u1 - u0 {
                continue;
            }
            let x = u0 + du;
            let (hx, hy) = ((u1 - u0) / 2.0, (v1 - v0) / 2.0);
            let qx = (x - (u0 + hx)).abs() - (hx - radius);
            let qy = (v - (v0 + hy)).abs() - (hy - radius);
            let (ox, oy) = (qx.max(0.0), qy.max(0.0));
            let sdf = (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - radius;
            return if sdf < 0.0 {
                (-sdf, [st[3], st[4], st[5]])
            } else {
                joint
            };
        }
        joint
    }
}
