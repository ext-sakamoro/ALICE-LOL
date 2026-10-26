//! Proportion laws (`kind proportion`): named measurements computed from named parameters
//!
//! A proportion law declares parameters (each with a default and a range), intermediate values
//! and named outputs: scalars (`out`) and points (`point`, three scalars). It knows nothing of
//! what the numbers measure; a consumer reads the outputs by name. The same law and the same
//! parameter values give the same bits on every platform: the expressions are the expressions
//! of research laws (`f64` without fused multiply-add, `alice-det-math` for every function) and
//! the outputs stay `f64`.
//!
//! ```text
//! law limb
//! kind proportion
//! claim a limb of two segments, the upper a fixed share of the whole
//! param length number = 0.3 range 0.2 0.4
//! let upper = 0.55 * length
//! out lower = length - upper
//! point joint = (0, -upper, 0)
//! ```
//!
//! # Lines
//!
//! - `law <name>` first, then `kind proportion`; `claim` / `source` lines are notes (not in the
//!   identifier); `#` starts a comment line
//! - `param <name> number = <default> range <lo> <hi>`: a named number, `lo <= default <= hi`
//! - `let <name> = <expr>`: a value for the later lines, before any `out` or `point`
//! - `out <name> = <expr>`: a named output
//! - `point <name> = (<expr>, <expr>, <expr>)`: a named output of three numbers
//!
//! Expressions read `pi`, the parameters and the earlier `let` values (not outputs). Every name
//! is declared once across parameters, lets and outputs. A law has at least one output, and its
//! defaults must give finite outputs.

use std::fmt;

use crate::law_id::{fold_semantics_pins, hex32, LawIdHasher};
use crate::motion_law::SampleError;
use crate::research_law::{CompiledExpr, LawExpr};

/// The kind tag of a proportion law, hashed into its identifier
pub const PROPORTION_LAW_KIND: &[u8] = b"lol.law.proportion/v1";

/// The most parameters
pub const MAX_PARAMS: usize = 64;
/// The most `let` lines
pub const MAX_LETS: usize = 256;
/// The most outputs (`out` and `point` lines together)
pub const MAX_OUTPUTS: usize = 256;
/// The most lines
pub const MAX_LINES: usize = 1024;

/// What the reading of a proportion law means, as pins measured from the behaviour of this
/// module ([`proportion_semantics_fingerprints`]; the oracle recomputes every one)
pub const PROPORTION_SEMANTICS_PINS: &[(&str, [u8; 32])] = &[
    (
        "proportion: f64 outputs of research-law expressions, a point as three outputs",
        hex32("bce1bc51a00e87e95b767ecb8a67e909ba9b107c5636ec388d9e604315a5f03d"),
    ),
    (
        "proportion: the expression functions of research laws",
        hex32("ee1b39dde2b1e577892f19daee2fb514a7b43f56be7d2211061de1ddac28fad3"),
    ),
];

/// The fold of [`PROPORTION_SEMANTICS_PINS`] (the oracle checks the two agree)
pub const PROPORTION_SEMANTICS_ID: [u8; 32] =
    hex32("af7e33a7437f2a5478f620bc7d02f6a8b242090754d23d66b03cdd11472e975d");

/// A proportion law that does not read, with the line it was on (0 for the whole file)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProportionLawError {
    /// the 1-based line, 0 when the problem is not on one line
    pub line: usize,
    /// what is wrong
    pub message: String,
}

impl fmt::Display for ProportionLawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "proportion law: {}", self.message)
        } else {
            write!(f, "proportion law, line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for ProportionLawError {}

fn err(line: usize, message: impl Into<String>) -> ProportionLawError {
    ProportionLawError {
        line,
        message: message.into(),
    }
}

/// A declared parameter
#[derive(Debug, Clone, PartialEq)]
pub struct ProportionParam {
    /// its name
    pub name: String,
    /// the value used when none is given
    pub default: f64,
    /// the smallest value
    pub lo: f64,
    /// the largest value
    pub hi: f64,
}

#[derive(Debug, Clone, PartialEq)]
struct Line {
    name: String,
    exprs: Vec<LawExpr>,
    codes: Vec<CompiledExpr>,
}

/// A proportion law (`kind proportion`)
#[derive(Debug, Clone, PartialEq)]
pub struct ProportionLaw {
    name: String,
    params: Vec<ProportionParam>,
    lets: Vec<Line>,
    /// `out` lines have one expression, `point` lines three, in file order
    outputs: Vec<Line>,
}

/// One output of a bound proportion law
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Output {
    /// an `out` line
    Scalar(f64),
    /// a `point` line
    Point([f64; 3]),
}

/// A proportion law with its parameter values fixed, and its outputs
#[derive(Debug, Clone)]
pub struct BoundProportion<'a> {
    law: &'a ProportionLaw,
    values: Vec<f64>,
    outputs: Vec<Output>,
}

const RESERVED: [&str; 1] = ["pi"];

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

fn expr(line: usize, text: &str) -> Result<LawExpr, ProportionLawError> {
    LawExpr::parse(text).map_err(|e| err(line, format!("`{}`: {e}", text.trim())))
}

/// The three expressions of `(a, b, c)`, split at the commas outside any parentheses
fn triple(line: usize, text: &str) -> Result<[&str; 3], ProportionLawError> {
    let t = text.trim();
    let inner = t
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .ok_or_else(|| err(line, "`point <name> = (<expr>, <expr>, <expr>)` expected"))?;
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
    <[&str; 3]>::try_from(parts)
        .map_err(|p| err(line, format!("a point has 3 coordinates, not {}", p.len())))
}

impl ProportionLaw {
    /// Reads a proportion law file
    ///
    /// # Errors
    ///
    /// [`ProportionLawError`] naming the line: not `kind proportion`, an unknown line, a name
    /// declared twice or reserved, a parameter whose default is outside its range, a `let`
    /// after an output, an expression that does not parse or names something not declared
    /// before it, a point without three coordinates, no output, more lines, parameters, lets
    /// or outputs than the limits, or outputs that are not finite with the defaults
    #[allow(clippy::too_many_lines)] // one arm per kind of line, in file order
    pub fn from_file(text: &str) -> Result<Self, ProportionLawError> {
        let mut name = None;
        let mut kind = false;
        let mut params: Vec<ProportionParam> = Vec::new();
        let mut lets: Vec<(String, LawExpr)> = Vec::new();
        let mut outputs: Vec<(String, Vec<LawExpr>)> = Vec::new();
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
                if t != "kind proportion" {
                    return Err(err(line, "the second line is `kind proportion`"));
                }
                kind = true;
                continue;
            }
            let taken = |n: &str,
                         params: &[ProportionParam],
                         lets: &[(String, LawExpr)],
                         outputs: &[(String, Vec<LawExpr>)]| {
                RESERVED.contains(&n)
                    || params.iter().any(|p| p.name == n)
                    || lets.iter().any(|(l, _)| l == n)
                    || outputs.iter().any(|(o, _)| o == n)
            };
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
                    if !is_name(n) || taken(n, &params, &lets, &outputs) {
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
                    params.push(ProportionParam {
                        name: n.to_owned(),
                        default: d,
                        lo,
                        hi,
                    });
                }
                "let" | "out" | "point" => {
                    let Some((n, e)) = rest.split_once('=') else {
                        return Err(err(line, format!("`{word} <name> = ...` expected")));
                    };
                    let n = n.trim();
                    if !is_name(n) || taken(n, &params, &lets, &outputs) {
                        return Err(err(line, format!("`{n}` is reserved or declared twice")));
                    }
                    let texts: Vec<&str> = if word == "point" {
                        triple(line, e)?.to_vec()
                    } else {
                        vec![e]
                    };
                    let mut names: Vec<&str> = RESERVED.to_vec();
                    names.extend(params.iter().map(|p| p.name.as_str()));
                    names.extend(lets.iter().map(|(l, _)| l.as_str()));
                    let mut exprs = Vec::new();
                    for text in texts {
                        let e = expr(line, text)?;
                        e.compile_named(&names)
                            .map_err(|e| err(line, e.to_string()))?;
                        exprs.push(e);
                    }
                    if word == "let" {
                        if !outputs.is_empty() {
                            return Err(err(line, "`let` comes before `out` and `point`"));
                        }
                        if lets.len() == MAX_LETS {
                            return Err(err(line, format!("more than {MAX_LETS} `let` lines")));
                        }
                        lets.push((n.to_owned(), exprs.remove(0)));
                    } else {
                        if outputs.len() == MAX_OUTPUTS {
                            return Err(err(line, format!("more than {MAX_OUTPUTS} outputs")));
                        }
                        outputs.push((n.to_owned(), exprs));
                    }
                }
                _ => return Err(err(line, format!("unknown line `{word}`"))),
            }
        }
        let name = name.ok_or_else(|| err(0, "empty (no `law <name>` line)"))?;
        if !kind {
            return Err(err(0, "not a proportion law (no `kind proportion`)"));
        }
        if outputs.is_empty() {
            return Err(err(0, "no `out` or `point` line"));
        }
        let mut names: Vec<&str> = RESERVED.to_vec();
        names.extend(params.iter().map(|p| p.name.as_str()));
        let compile =
            |exprs: &[LawExpr], names: &[&str]| -> Result<Vec<CompiledExpr>, ProportionLawError> {
                exprs
                    .iter()
                    .map(|e| e.compile_named(names).map_err(|e| err(0, e.to_string())))
                    .collect()
            };
        let mut compiled_lets = Vec::new();
        for (n, e) in &lets {
            let exprs = vec![e.clone()];
            let codes = compile(&exprs, &names)?;
            compiled_lets.push(Line {
                name: n.clone(),
                exprs,
                codes,
            });
            names.push(n);
        }
        let mut compiled_outputs = Vec::new();
        for (n, exprs) in outputs {
            let codes = compile(&exprs, &names)?;
            compiled_outputs.push(Line {
                name: n,
                exprs,
                codes,
            });
        }
        let law = Self {
            name,
            params,
            lets: compiled_lets,
            outputs: compiled_outputs,
        };
        // the defaults must give finite outputs, so a law that reads always has one instance
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
    pub fn params(&self) -> &[ProportionParam] {
        &self.params
    }

    /// The names of the outputs, in file order
    pub fn output_names(&self) -> impl Iterator<Item = &str> {
        self.outputs.iter().map(|o| o.name.as_str())
    }

    /// The identifier of the law: its kind, [`PROPORTION_SEMANTICS_ID`], its name, every
    /// parameter (name, default, range) and every expression as a tree (spelling and spacing
    /// do not count; notes and comments are not in it)
    #[must_use]
    pub fn law_id(&self) -> [u8; 32] {
        self.law_id_under(&PROPORTION_SEMANTICS_ID)
    }

    fn law_id_under(&self, semantics: &[u8; 32]) -> [u8; 32] {
        let mut h = LawIdHasher::new(PROPORTION_LAW_KIND, semantics)
            .str(&self.name)
            .u64(self.params.len() as u64);
        for p in &self.params {
            h = h.str(&p.name).f64(p.default).f64(p.lo).f64(p.hi);
        }
        h = h.u64(self.lets.len() as u64);
        for l in &self.lets {
            h = l.exprs[0].hash_into(h.str(&l.name));
        }
        h = h.u64(self.outputs.len() as u64);
        for o in &self.outputs {
            h = h.str(&o.name);
            for e in &o.exprs {
                h = e.hash_into(h);
            }
        }
        h.finish()
    }

    /// The law with parameter values and its outputs: each named value must be declared,
    /// given once and in its range; the others take their defaults
    ///
    /// # Errors
    ///
    /// [`SampleError::UnknownParam`], [`SampleError::RepeatedParam`],
    /// [`SampleError::ParamOutOfRange`], or [`SampleError::NonFinite`] naming the `let` or
    /// output whose value is not finite
    pub fn bind(&self, values: &[(&str, f64)]) -> Result<BoundProportion<'_>, SampleError> {
        let mut vals: Vec<f64> = self.params.iter().map(|p| p.default).collect();
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
            vals[j] = *v;
        }
        let mut slots = vec![std::f64::consts::PI];
        slots.extend_from_slice(&vals);
        // the evaluator refuses a value that is not finite (it never returns one)
        let finite = |name: &str, v: Result<f64, _>| -> Result<f64, SampleError> {
            v.map_err(|_| SampleError::NonFinite {
                name: name.to_owned(),
            })
        };
        for l in &self.lets {
            let v = finite(&l.name, l.codes[0].eval(&slots))?;
            slots.push(v);
        }
        let mut outputs = Vec::with_capacity(self.outputs.len());
        for o in &self.outputs {
            let mut xs = [0.0; 3];
            for (x, c) in xs.iter_mut().zip(&o.codes) {
                *x = finite(&o.name, c.eval(&slots))?;
            }
            outputs.push(if o.codes.len() == 3 {
                Output::Point(xs)
            } else {
                Output::Scalar(xs[0])
            });
        }
        Ok(BoundProportion {
            law: self,
            values: vals,
            outputs,
        })
    }
}

impl BoundProportion<'_> {
    /// The value of every parameter, in the law's declared order
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// Every output with its name, in file order
    pub fn outputs(&self) -> impl Iterator<Item = (&str, Output)> {
        self.law
            .outputs
            .iter()
            .map(|o| o.name.as_str())
            .zip(self.outputs.iter().copied())
    }

    /// The `out` line of this name
    #[must_use]
    pub fn scalar(&self, name: &str) -> Option<f64> {
        self.outputs().find_map(|(n, o)| match o {
            Output::Scalar(v) if n == name => Some(v),
            _ => None,
        })
    }

    /// The `point` line of this name
    #[must_use]
    pub fn point(&self, name: &str) -> Option<[f64; 3]> {
        self.outputs().find_map(|(n, o)| match o {
            Output::Point(p) if n == name => Some(p),
            _ => None,
        })
    }
}

/// The law the first proportion pin evaluates: a parameter, lets, a scalar and a point that
/// read the expression functions
pub const PROPORTION_PROBE_LAW: &str = "law proportion_probe
kind proportion
param a number = 0.3 range 0.1 0.9
param b number = 2 range 1 4
let c = a * b - 0.125
let d = sqrt(c * c + 1)
out e = c / d
out f = atan2(a, b) + exp(-a) - pi / 7
point g = (a, max(c, 0.25) ^ 2, min(d, b) - 0.5 * a)
";

/// The measured value of every pin of [`PROPORTION_SEMANTICS_PINS`]
///
/// # Panics
///
/// If [`PROPORTION_PROBE_LAW`] does not read (the oracle reads it first)
#[must_use]
pub fn proportion_semantics_fingerprints() -> Vec<(&'static str, [u8; 32])> {
    use sha2::{Digest as _, Sha256};
    let law = ProportionLaw::from_file(PROPORTION_PROBE_LAW).expect("the probe law reads");
    let mut h = Sha256::new();
    for a in [0.1, 0.3, 0.5, 0.9] {
        for b in [1.0, 2.0, 3.5] {
            let bound = law.bind(&[("a", a), ("b", b)]).expect("the probe binds");
            for (n, o) in bound.outputs() {
                h.update(n.as_bytes());
                match o {
                    Output::Scalar(v) => {
                        h.update([0]);
                        h.update(v.to_bits().to_le_bytes());
                    }
                    Output::Point(p) => {
                        h.update([1]);
                        for v in p {
                            h.update(v.to_bits().to_le_bytes());
                        }
                    }
                }
            }
        }
    }
    vec![
        (PROPORTION_SEMANTICS_PINS[0].0, h.finalize().into()),
        (
            PROPORTION_SEMANTICS_PINS[1].0,
            crate::research_law::expression_functions_fingerprint(),
        ),
    ]
}

/// The fold of the measured pins (what [`PROPORTION_SEMANTICS_ID`] must equal)
#[must_use]
pub fn measured_proportion_semantics_id() -> [u8; 32] {
    fold_semantics_pins(&proportion_semantics_fingerprints())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifier moves with the proportion semantics
    #[test]
    fn the_identifier_moves_with_the_proportion_semantics() {
        let law = ProportionLaw::from_file(PROPORTION_PROBE_LAW).unwrap();
        assert_eq!(law.law_id(), law.law_id_under(&PROPORTION_SEMANTICS_ID));
        let mut other = PROPORTION_SEMANTICS_ID;
        other[0] ^= 1;
        assert_ne!(law.law_id_under(&other), law.law_id());
    }
}
