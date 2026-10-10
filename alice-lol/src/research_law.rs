//! Research laws: multi-variable formulas with units, a valid range, measured
//! residuals, provenance and reference values
//!
//! This module is unrelated to [`crate::law`], which checks geometric
//! constraints on an SDF scene. A [`ResearchLaw`] is a claim about data:
//! `output = f(inputs; parameters)` written as text, together with
//!
//! | item | meaning |
//! |------|---------|
//! | [`Var`] / [`Param`] | names with units (`Pa`, `J/(mol*K)`, …); parameters carry a value and an uncertainty |
//! | [`ValidRange`] | the interval of each input the evidence covers; evaluation outside it is refused |
//! | [`ResidualStats`] | measured `observed - f(conditions)` over the stored evidence (never a reported number) |
//! | [`Provenance`] | where the law and its evidence come from |
//! | [`ResearchOracle`] | a reference value the law has to reproduce within a tolerance |
//! | [`ResearchVerdict`] | what new evidence does to the law |
//!
//! The range, residual, provenance and ingest-policy types are the ones of
//! [`alice_zip::law`], so a one-variable signal law and a research law are
//! judged with the same vocabulary.
//!
//! # Expressions
//!
//! [`LawExpr::parse`] reads numbers, identifiers, `+ - * / ^`, unary minus,
//! parentheses and the functions `exp`, `ln`, `sqrt`, `sin`, `cos` (one argument),
//! `atan2(y, x)` (the angle of the point `(x, y)`, in (−π, π]; the sign of a zero
//! argument is ignored, so the negative x axis is +π and the origin is +0) and `min` / `max`
//! (two or more arguments; of equal arguments the first is the result). `^` binds
//! tighter than unary minus (`-2^2 = -4`) and is right associative
//! (`2^3^2 = 512`); its exponent must be a constant. Expressions nest at most
//! [`MAX_DEPTH`] levels (a sum of many terms counts one level per term).
//!
//! Dimensions are checked when a law is built: `+` and `-` need equal
//! dimensions, `exp` / `ln` / `sin` / `cos` need a dimensionless argument,
//! `x^p` and `sqrt` must give integer exponents, the arguments of `atan2` /
//! `min` / `max` share one dimension (`atan2` gives a pure number, `min` / `max`
//! that dimension), and the expression must have the dimension of the output.
//! A call with a number of arguments the function does not take is
//! [`ResearchLawError::ArgumentCount`].
//!
//! # Units
//!
//! [`Unit::parse`] reads products and quotients of known unit symbols with
//! integer powers, e.g. `m/s^2`, `J/(mol*K)`, `kg*m^2`, or `1` for a
//! dimensionless quantity. Every unit is a scale factor to SI; units with an
//! offset (degrees Celsius) are not supported. Values are given and returned
//! in the declared units and evaluated in SI.
//!
//! # Evaluation
//!
//! [`ResearchLaw::evaluate`] never extrapolates: an input outside its valid
//! range, or not finite, gives [`ResearchLawError::OutOfRange`]. An input
//! without a declared range is only required to be finite. Any non-finite
//! intermediate value (division by zero, `ln` of a negative number,
//! overflow) gives [`ResearchLawError::NonFinite`] instead of a NaN or an
//! infinity. Evaluation is a fixed sequence of `f64` operations without fused
//! multiply-add; `exp`, `ln`, `sqrt`, `sin`, `cos`, `atan2` and non-integer
//! powers come from `alice-det-math`, so the same inputs give the same bits on
//! every platform. [`expression_functions_fingerprint`] hashes their behaviour
//! and is part of `law_id::LOL_SEMANTICS_ID`.
//!
//! # Judging new evidence
//!
//! [`ResearchLaw::ingest`] follows the rule order of
//! [`alice_zip::law::SignalLaw::ingest`]. With
//! `band = max(policy.abs_tolerance, residual().rms)` and `rms_new` the RMS of
//! `observed - f(conditions)` over the new observations (in the output unit):
//!
//! 1. no observations → [`ResearchVerdict::NoEvidence`]
//! 2. any observation the law cannot be evaluated at (an input outside the
//!    valid range or not finite, a missing or unknown input, a non-finite
//!    result) or with a non-finite observed value →
//!    [`ResearchVerdict::OutOfRange`]
//! 3. `rms_new ≤ band` → [`ResearchVerdict::Supports`]
//! 4. the parameters refitted on stored + new evidence give an RMS `≤ band` →
//!    [`ResearchVerdict::ParameterUpdate`] (a law without parameters cannot be
//!    updated)
//! 5. `rms_new ≤ band · policy.break_factor` → [`ResearchVerdict::ResidualGrew`]
//! 6. otherwise → [`ResearchVerdict::Breaks`]
//!
//! The refit is a least-squares fit of all parameters by Gauss–Newton:
//! starting from the current values, at most [`GN_MAX_ITERATIONS`] steps;
//! each step builds the Jacobian by central differences (step
//! `GN_FD_STEP · max(|p|, 1)` per parameter), solves the normal equations by
//! Gaussian elimination with partial pivoting, and halves the step (at most
//! [`GN_MAX_HALVINGS`] times) until the sum of squares decreases. It stops
//! when no step decreases it or when every relative parameter change is at
//! most [`GN_TOLERANCE`]. Parameter uncertainties of the refitted law are the
//! standard errors `sqrt(s² · (JᵀJ)⁻¹ᵢᵢ)` with `s² = SSR / (n - k)` when there
//! are more observations than parameters, otherwise the previous values.
//!
//! ```
//! use alice_lol::research_law::{Param, ResearchLaw, Var};
//! use alice_zip::law::{Provenance, ValidRange};
//!
//! let law = ResearchLaw::new(
//!     "free fall",
//!     "0.5*g*t^2",
//!     Var::new("s", "m"),
//!     &[Var::new("t", "s")],
//!     &[Param::new("g", 9.80665, 0.0, "m/s^2")],
//!     &[("t", ValidRange { lo: 0.0, hi: 10.0 })],
//!     Provenance::new("standard gravity", "closed form"),
//! )?;
//! let s = law.evaluate(&[("t", 2.0)])?;
//! assert!((s - 0.5 * 9.80665 * 4.0).abs() < 1e-12);
//! assert!(law.evaluate(&[("t", 11.0)]).is_err()); // outside the valid range
//! # Ok::<(), alice_lol::research_law::ResearchLawError>(())
//! ```

use std::collections::BTreeSet;
use std::fmt;

pub use alice_zip::law::{IngestPolicy, Provenance, ResidualStats, ValidRange};

/// Deepest nesting of an expression (also bounds a chain of `+` / `*` terms)
pub const MAX_DEPTH: usize = 256;
/// Most Gauss–Newton steps of a refit
pub const GN_MAX_ITERATIONS: usize = 50;
/// Most step halvings within one Gauss–Newton step
pub const GN_MAX_HALVINGS: usize = 30;
/// Relative parameter change below which a refit has converged
pub const GN_TOLERANCE: f64 = 1e-12;
/// Relative central-difference step for the Jacobian (≈ cube root of `f64::EPSILON`)
pub const GN_FD_STEP: f64 = 6.055_454_452_393_343e-6;

/// Deepest parenthesis nesting of a unit
const MAX_UNIT_DEPTH: usize = 32;

// ── errors ───────────────────────────────────────────────────────────

/// Why a law could not be built, evaluated or compared
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResearchLawError {
    /// The expression text has no tokens
    EmptyExpression,
    /// The expression ends where an operand was expected
    UnexpectedEnd,
    /// A token where it cannot appear (byte offset)
    UnexpectedToken {
        /// Byte offset in the text
        position: usize,
    },
    /// A character that is not part of the expression language (byte offset)
    UnexpectedCharacter {
        /// Byte offset in the text
        position: usize,
    },
    /// An unclosed `(` or an unmatched `)` (byte offset)
    UnbalancedParenthesis {
        /// Byte offset of the parenthesis
        position: usize,
    },
    /// A numeric literal that does not parse to a finite number (byte offset)
    InvalidNumber {
        /// Byte offset in the text
        position: usize,
    },
    /// The exponent of `^` refers to a variable
    NonConstantExponent,
    /// A call of a function other than `exp`, `ln`, `sqrt`, `sin`, `cos`, `atan2`,
    /// `min`, `max`
    UnknownFunction(String),
    /// A call with a number of arguments the function does not take
    ArgumentCount {
        /// Name of the function
        function: &'static str,
        /// Number of arguments given
        found: usize,
    },
    /// The expression nests deeper than [`MAX_DEPTH`]
    NestingTooDeep,
    /// An identifier that is neither an input nor a parameter
    UnknownIdentifier(String),
    /// A unit symbol that is not known
    UnknownUnit(String),
    /// A unit text that does not parse
    InvalidUnit(String),
    /// A name declared twice (inputs, parameters, output, ranges, conditions)
    DuplicateName(String),
    /// A name that is not an input of the law (conditions, ranges, bridge)
    UnknownVariable(String),
    /// An input that has no value in the conditions
    MissingVariable(String),
    /// An input outside its valid range, or not finite
    OutOfRange {
        /// Name of the input
        variable: String,
    },
    /// A valid range that is empty or not finite
    InvalidRange(String),
    /// A non-finite value: a parameter, an observed value, or an intermediate
    /// or final result of evaluation
    NonFinite,
    /// Two quantities that must have the same dimension do not
    IncompatibleDimensions {
        /// Dimension of the left / first quantity
        left: Dimension,
        /// Dimension of the right / second quantity
        right: Dimension,
    },
    /// The argument of `exp` / `ln` / `sin` / `cos` has a dimension
    DimensionfulArgument {
        /// Name of the function
        function: &'static str,
    },
    /// A power gives a non-integer dimension exponent
    FractionalDimension,
    /// A dimension exponent outside `i8`
    DimensionOverflow,
    /// The expression's dimension differs from the declared output's
    OutputDimension {
        /// Dimension of the output unit
        expected: Dimension,
        /// Dimension of the expression
        found: Dimension,
    },
}

impl fmt::Display for ResearchLawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyExpression => f.write_str("empty expression"),
            Self::UnexpectedEnd => f.write_str("expression ends where an operand was expected"),
            Self::UnexpectedToken { position } => write!(f, "unexpected token at byte {position}"),
            Self::UnexpectedCharacter { position } => {
                write!(f, "unexpected character at byte {position}")
            }
            Self::UnbalancedParenthesis { position } => {
                write!(f, "unbalanced parenthesis at byte {position}")
            }
            Self::InvalidNumber { position } => write!(f, "invalid number at byte {position}"),
            Self::NonConstantExponent => f.write_str("the exponent of ^ must be a constant"),
            Self::UnknownFunction(name) => write!(f, "unknown function `{name}`"),
            Self::ArgumentCount { function, found } => {
                let (lo, hi) = Func::from_name(function).map_or((0, 0), Func::arity);
                if hi == usize::MAX {
                    write!(f, "`{function}` takes {lo} or more arguments, got {found}")
                } else {
                    write!(f, "`{function}` takes {lo} argument(s), got {found}")
                }
            }
            Self::NestingTooDeep => write!(f, "expression nests deeper than {MAX_DEPTH}"),
            Self::UnknownIdentifier(name) => write!(f, "unknown identifier `{name}`"),
            Self::UnknownUnit(name) => write!(f, "unknown unit `{name}`"),
            Self::InvalidUnit(text) => write!(f, "invalid unit `{text}`"),
            Self::DuplicateName(name) => write!(f, "`{name}` is declared twice"),
            Self::UnknownVariable(name) => write!(f, "`{name}` is not an input of the law"),
            Self::MissingVariable(name) => write!(f, "no value for input `{name}`"),
            Self::OutOfRange { variable } => {
                write!(f, "`{variable}` is outside its valid range or not finite")
            }
            Self::InvalidRange(name) => write!(f, "the valid range of `{name}` is invalid"),
            Self::NonFinite => f.write_str("non-finite value"),
            Self::IncompatibleDimensions { left, right } => {
                write!(f, "incompatible dimensions {left} and {right}")
            }
            Self::DimensionfulArgument { function } => {
                write!(f, "the argument of {function} must be dimensionless")
            }
            Self::FractionalDimension => f.write_str("power gives a non-integer dimension"),
            Self::DimensionOverflow => f.write_str("dimension exponent out of range"),
            Self::OutputDimension { expected, found } => {
                write!(f, "expression has dimension {found}, output has {expected}")
            }
        }
    }
}

impl std::error::Error for ResearchLawError {}

type Result<T> = std::result::Result<T, ResearchLawError>;

// ── dimensions and units ─────────────────────────────────────────────

/// Exponents of the seven SI base dimensions: length, mass, time, electric
/// current, temperature, amount of substance, luminous intensity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Dimension([i8; 7]);

const BASE_SYMBOLS: [&str; 7] = ["L", "M", "T", "I", "Θ", "N", "J"];

impl Dimension {
    /// The dimension of a pure number
    pub const DIMENSIONLESS: Self = Self([0; 7]);

    /// Dimension from its seven exponents (`[L, M, T, I, Θ, N, J]`)
    #[must_use]
    pub const fn new(exponents: [i8; 7]) -> Self {
        Self(exponents)
    }

    /// The seven exponents `[L, M, T, I, Θ, N, J]`
    #[must_use]
    pub const fn exponents(self) -> [i8; 7] {
        self.0
    }

    /// Whether every exponent is zero
    #[must_use]
    pub fn is_dimensionless(self) -> bool {
        self == Self::DIMENSIONLESS
    }

    fn combine(self, other: Self, sign: i8) -> Result<Self> {
        let mut out = [0_i8; 7];
        for (i, slot) in out.iter_mut().enumerate() {
            let rhs = other.0[i]
                .checked_mul(sign)
                .ok_or(ResearchLawError::DimensionOverflow)?;
            *slot = self.0[i]
                .checked_add(rhs)
                .ok_or(ResearchLawError::DimensionOverflow)?;
        }
        Ok(Self(out))
    }

    fn mul(self, other: Self) -> Result<Self> {
        self.combine(other, 1)
    }

    fn div(self, other: Self) -> Result<Self> {
        self.combine(other, -1)
    }

    /// `self^p`; every resulting exponent must be an integer in `i8`
    // a dimension exponent is an exact small integer, so `x == x.round()` is
    // the exact test for "integer"; the casts are range-checked first
    #[allow(clippy::float_cmp, clippy::cast_possible_truncation)]
    fn pow(self, p: f64) -> Result<Self> {
        let mut out = [0_i8; 7];
        for (slot, &e) in out.iter_mut().zip(self.0.iter()) {
            if e == 0 {
                continue;
            }
            let x = f64::from(e) * p;
            if !x.is_finite() {
                return Err(ResearchLawError::DimensionOverflow);
            }
            if x != x.round() {
                return Err(ResearchLawError::FractionalDimension);
            }
            if x < f64::from(i8::MIN) || x > f64::from(i8::MAX) {
                return Err(ResearchLawError::DimensionOverflow);
            }
            *slot = x as i8;
        }
        Ok(Self(out))
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_dimensionless() {
            return f.write_str("1");
        }
        let mut first = true;
        for (sym, &e) in BASE_SYMBOLS.iter().zip(self.0.iter()) {
            if e == 0 {
                continue;
            }
            if !first {
                f.write_str(" ")?;
            }
            first = false;
            if e == 1 {
                f.write_str(sym)?;
            } else {
                write!(f, "{sym}^{e}")?;
            }
        }
        Ok(())
    }
}

/// A unit: a dimension and the factor that converts a value in this unit to SI
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Unit {
    dimension: Dimension,
    scale: f64,
}

/// `(symbol, [L, M, T, I, Θ, N, J], factor to SI)`
const UNIT_TABLE: &[(&str, [i8; 7], f64)] = &[
    ("m", [1, 0, 0, 0, 0, 0, 0], 1.0),
    ("km", [1, 0, 0, 0, 0, 0, 0], 1e3),
    ("cm", [1, 0, 0, 0, 0, 0, 0], 1e-2),
    ("mm", [1, 0, 0, 0, 0, 0, 0], 1e-3),
    ("kg", [0, 1, 0, 0, 0, 0, 0], 1.0),
    ("g", [0, 1, 0, 0, 0, 0, 0], 1e-3),
    ("s", [0, 0, 1, 0, 0, 0, 0], 1.0),
    ("ms", [0, 0, 1, 0, 0, 0, 0], 1e-3),
    ("min", [0, 0, 1, 0, 0, 0, 0], 60.0),
    ("h", [0, 0, 1, 0, 0, 0, 0], 3600.0),
    ("A", [0, 0, 0, 1, 0, 0, 0], 1.0),
    ("K", [0, 0, 0, 0, 1, 0, 0], 1.0),
    ("mol", [0, 0, 0, 0, 0, 1, 0], 1.0),
    ("cd", [0, 0, 0, 0, 0, 0, 1], 1.0),
    ("Hz", [0, 0, -1, 0, 0, 0, 0], 1.0),
    ("N", [1, 1, -2, 0, 0, 0, 0], 1.0),
    ("kN", [1, 1, -2, 0, 0, 0, 0], 1e3),
    ("Pa", [-1, 1, -2, 0, 0, 0, 0], 1.0),
    ("kPa", [-1, 1, -2, 0, 0, 0, 0], 1e3),
    ("MPa", [-1, 1, -2, 0, 0, 0, 0], 1e6),
    ("bar", [-1, 1, -2, 0, 0, 0, 0], 1e5),
    ("J", [2, 1, -2, 0, 0, 0, 0], 1.0),
    ("kJ", [2, 1, -2, 0, 0, 0, 0], 1e3),
    ("W", [2, 1, -3, 0, 0, 0, 0], 1.0),
    ("L", [3, 0, 0, 0, 0, 0, 0], 1e-3),
    ("mL", [3, 0, 0, 0, 0, 0, 0], 1e-6),
];

impl Unit {
    /// The unit of a pure number
    pub const ONE: Self = Self {
        dimension: Dimension::DIMENSIONLESS,
        scale: 1.0,
    };

    /// Parses a unit such as `Pa`, `m/s^2`, `J/(mol*K)` or `1`
    ///
    /// Known symbols: `m km cm mm kg g s ms min h A K mol cd Hz N kN Pa kPa
    /// MPa bar J kJ W L mL`. Powers are integers (`m^3`, `s^-2`); `*` and `/`
    /// associate to the left (`J/mol*K` is `J·K/mol`).
    ///
    /// # Errors
    ///
    /// [`ResearchLawError::UnknownUnit`] for an unknown symbol,
    /// [`ResearchLawError::InvalidUnit`] for text that does not parse,
    /// [`ResearchLawError::DimensionOverflow`] for exponents outside `i8`
    pub fn parse(text: &str) -> Result<Self> {
        let mut p = UnitParser {
            text,
            bytes: text.as_bytes(),
            pos: 0,
        };
        let unit = p.product(0)?;
        p.skip_ws();
        if p.pos != p.bytes.len() {
            return Err(p.invalid());
        }
        if !unit.scale.is_finite() || unit.scale <= 0.0 {
            return Err(p.invalid());
        }
        Ok(unit)
    }

    /// The dimension of the unit
    #[must_use]
    pub const fn dimension(self) -> Dimension {
        self.dimension
    }

    /// Factor that converts a value in this unit to SI (`1 kPa → 1000`)
    #[must_use]
    pub const fn scale(self) -> f64 {
        self.scale
    }
}

struct UnitParser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl UnitParser<'_> {
    fn invalid(&self) -> ResearchLawError {
        ResearchLawError::InvalidUnit(self.text.to_string())
    }

    const fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_ws();
        self.bytes.get(self.pos).copied()
    }

    fn product(&mut self, depth: usize) -> Result<Unit> {
        if depth > MAX_UNIT_DEPTH {
            return Err(self.invalid());
        }
        let mut acc = self.factor(depth)?;
        while let Some(op @ (b'*' | b'/')) = self.peek() {
            self.pos += 1;
            let rhs = self.factor(depth)?;
            acc = if op == b'*' {
                Unit {
                    dimension: acc.dimension.mul(rhs.dimension)?,
                    scale: acc.scale * rhs.scale,
                }
            } else {
                Unit {
                    dimension: acc.dimension.div(rhs.dimension)?,
                    scale: acc.scale / rhs.scale,
                }
            };
        }
        Ok(acc)
    }

    fn factor(&mut self, depth: usize) -> Result<Unit> {
        let base = self.atom(depth)?;
        if self.peek() != Some(b'^') {
            return Ok(base);
        }
        self.pos += 1;
        self.skip_ws();
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        let n: i32 = self.text[start..self.pos]
            .parse()
            .map_err(|_| self.invalid())?;
        let exp = i8::try_from(n).map_err(|_| ResearchLawError::DimensionOverflow)?;
        Ok(Unit {
            dimension: base.dimension.pow(f64::from(exp))?,
            scale: powi_exact(base.scale, i32::from(exp)),
        })
    }

    fn atom(&mut self, depth: usize) -> Result<Unit> {
        match self.peek() {
            Some(b'(') => {
                self.pos += 1;
                let inner = self.product(depth + 1)?;
                if self.peek() != Some(b')') {
                    return Err(self.invalid());
                }
                self.pos += 1;
                Ok(inner)
            }
            Some(b'1') => {
                self.pos += 1;
                Ok(Unit::ONE)
            }
            Some(c) if c.is_ascii_alphabetic() => {
                let start = self.pos;
                while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_alphabetic() {
                    self.pos += 1;
                }
                let sym = &self.text[start..self.pos];
                UNIT_TABLE
                    .iter()
                    .find(|(name, _, _)| *name == sym)
                    .map(|&(_, dim, scale)| Unit {
                        dimension: Dimension(dim),
                        scale,
                    })
                    .ok_or_else(|| ResearchLawError::UnknownUnit(sym.to_string()))
            }
            _ => Err(self.invalid()),
        }
    }
}

// ── expressions ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Func {
    Exp,
    Ln,
    Sqrt,
    Sin,
    Cos,
    /// `atan2(y, x)`: the angle of the point `(x, y)`, in (−π, π]
    Atan2,
    /// `min(a, b, ...)`: the smallest argument (the first of equal ones)
    Min,
    /// `max(a, b, ...)`: the largest argument (the first of equal ones)
    Max,
}

impl Func {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "exp" => Some(Self::Exp),
            "ln" => Some(Self::Ln),
            "sqrt" => Some(Self::Sqrt),
            "sin" => Some(Self::Sin),
            "cos" => Some(Self::Cos),
            "atan2" => Some(Self::Atan2),
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            _ => None,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Exp => "exp",
            Self::Ln => "ln",
            Self::Sqrt => "sqrt",
            Self::Sin => "sin",
            Self::Cos => "cos",
            Self::Atan2 => "atan2",
            Self::Min => "min",
            Self::Max => "max",
        }
    }

    /// The number of arguments the function takes (`lo..=hi`)
    const fn arity(self) -> (usize, usize) {
        match self {
            Self::Exp | Self::Ln | Self::Sqrt | Self::Sin | Self::Cos => (1, 1),
            Self::Atan2 => (2, 2),
            Self::Min | Self::Max => (2, usize::MAX),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Parsed expression tree; identifiers are names (resolved by [`compile`])
#[derive(Debug, Clone, PartialEq)]
enum Node {
    Num(f64),
    Ident(String),
    Neg(Box<Self>),
    Bin(BinOp, Box<Self>, Box<Self>),
    Pow(Box<Self>, f64),
    Call(Func, Vec<Self>),
}

/// Expression tree with identifiers resolved to input / parameter slots
#[derive(Debug, Clone, PartialEq)]
enum Code {
    Num(f64),
    Input(usize),
    Param(usize),
    Neg(Box<Self>),
    Bin(BinOp, Box<Self>, Box<Self>),
    Pow(Box<Self>, f64),
    Call(Func, Vec<Self>),
}

/// A parsed law expression
#[derive(Debug, Clone, PartialEq)]
pub struct LawExpr {
    text: String,
    root: Node,
}

impl LawExpr {
    /// Parses an expression (see the module documentation for the syntax)
    ///
    /// # Errors
    ///
    /// The syntax variants of [`ResearchLawError`]
    /// ([`ResearchLawError::EmptyExpression`],
    /// [`ResearchLawError::UnexpectedEnd`], …),
    /// [`ResearchLawError::NonConstantExponent`],
    /// [`ResearchLawError::UnknownFunction`],
    /// [`ResearchLawError::NestingTooDeep`]
    pub fn parse(text: &str) -> Result<Self> {
        let tokens = tokenize(text)?;
        if tokens.is_empty() {
            return Err(ResearchLawError::EmptyExpression);
        }
        let mut p = ExprParser { tokens, pos: 0 };
        let (root, _) = p.sum(0)?;
        if let Some(tok) = p.tokens.get(p.pos) {
            return Err(if tok.kind == Tok::RParen {
                ResearchLawError::UnbalancedParenthesis {
                    position: tok.position,
                }
            } else {
                ResearchLawError::UnexpectedToken {
                    position: tok.position,
                }
            });
        }
        Ok(Self {
            text: text.trim().to_string(),
            root,
        })
    }

    /// The expression text (trimmed)
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The identifiers the expression refers to, sorted and without repeats
    #[must_use]
    pub fn identifiers(&self) -> Vec<String> {
        let mut set = BTreeSet::new();
        collect_identifiers(&self.root, &mut set);
        set.into_iter().collect()
    }

    /// Value of an expression without identifiers
    ///
    /// # Errors
    ///
    /// [`ResearchLawError::UnknownIdentifier`] when the expression refers to a
    /// name, [`ResearchLawError::NonFinite`] when a value is not finite
    pub fn evaluate_constant(&self) -> Result<f64> {
        let code = compile(&self.root, &[], &[])?;
        eval(&code, &[], &[])
    }
}

/// The fixed expressions behind [`expression_functions_fingerprint`]: every function, the
/// quadrants and axes of `atan2`, the first-of-equal rule of `min` / `max` (the sign of
/// zero shows which argument was taken) and the argument counts that do not parse
const EXPRESSION_FUNCTION_CASES: &[&str] = &[
    "exp(1)",
    "ln(2)",
    "sqrt(2)",
    "sin(1)",
    "cos(1)",
    "atan2(1, 1)",
    "atan2(1, -1)",
    "atan2(-1, -1)",
    "atan2(-1, 1)",
    "atan2(1, 0)",
    "atan2(-1, 0)",
    "atan2(0, 1)",
    "atan2(0, -1)",
    "atan2(3, 4)",
    "atan2(0*-1, -1)",
    "atan2(0*-1, 1)",
    "atan2(0, 0)",
    "atan2(0*-1, 0)",
    "atan2(0, 0*-1)",
    "atan2(0*-1, 0*-1)",
    "atan2(1, 0*-1)",
    "min(3, 1, 2)",
    "max(3, 1, 2)",
    "min(3, 2, 1)",
    "max(1, 2, 3)",
    "min(1, 3, 2)",
    "max(2, 3, 1)",
    "min(5, 4, 3, 2, 1)",
    "max(1, 2, 3, 4, 5)",
    "min(4, 5, 1, 3, 2)",
    "max(2, 1, 5, 4, 3)",
    "min(2, 3, 1, 0*-1, 0)",
    "min(2, 3, 1, 0, 0*-1)",
    "max(-2, -3, 0, 0*-1)",
    "min(0*-1, 0)",
    "min(0, 0*-1)",
    "max(0*-1, 0)",
    "max(0, 0*-1)",
    "atan2(1)",
    "atan2(1, 2, 3)",
    "min(1)",
    "sqrt(1, 2)",
];

/// The behaviour of the expression functions, **computed** (not written down)
///
/// Each fixed expression is parsed and evaluated; the hash covers whether it parsed and
/// the bits of the value. Changing a function, its argument count, the quadrant rule of
/// `atan2` or the tie rule of `min` / `max` changes this value
#[must_use]
pub fn expression_functions_fingerprint() -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"lol.research.expression-functions");
    for text in EXPRESSION_FUNCTION_CASES {
        h.update((text.len() as u64).to_le_bytes());
        h.update(text.as_bytes());
        match LawExpr::parse(text).and_then(|e| e.evaluate_constant()) {
            Ok(v) => {
                h.update([1]);
                h.update(v.to_bits().to_le_bytes());
            }
            Err(_) => h.update([0]),
        }
    }
    h.finalize().into()
}

fn collect_identifiers(node: &Node, out: &mut BTreeSet<String>) {
    match node {
        Node::Num(_) => {}
        Node::Ident(name) => {
            out.insert(name.clone());
        }
        Node::Neg(a) | Node::Pow(a, _) => collect_identifiers(a, out),
        Node::Call(_, args) => {
            for a in args {
                collect_identifiers(a, out);
            }
        }
        Node::Bin(_, a, b) => {
            collect_identifiers(a, out);
            collect_identifiers(b, out);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    Comma,
}

#[derive(Debug, Clone, PartialEq)]
struct Token {
    kind: Tok,
    position: usize,
}

fn tokenize(text: &str) -> Result<Vec<Token>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        let kind = match c {
            b'+' => Tok::Plus,
            b'-' => Tok::Minus,
            b'*' => Tok::Star,
            b'/' => Tok::Slash,
            b'^' => Tok::Caret,
            b'(' => Tok::LParen,
            b')' => Tok::RParen,
            b',' => Tok::Comma,
            b'0'..=b'9' | b'.' => {
                i = scan_number(bytes, i);
                let value: f64 = text[start..i]
                    .parse()
                    .map_err(|_| ResearchLawError::InvalidNumber { position: start })?;
                if !value.is_finite() {
                    return Err(ResearchLawError::InvalidNumber { position: start });
                }
                out.push(Token {
                    kind: Tok::Num(value),
                    position: start,
                });
                continue;
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                out.push(Token {
                    kind: Tok::Ident(text[start..i].to_string()),
                    position: start,
                });
                continue;
            }
            _ => return Err(ResearchLawError::UnexpectedCharacter { position: start }),
        };
        out.push(Token {
            kind,
            position: start,
        });
        i += 1;
    }
    Ok(out)
}

/// End of the numeric literal starting at `i` (digits, `.`, exponent)
const fn scan_number(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
        i += 1;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        if j < bytes.len() && bytes[j].is_ascii_digit() {
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    i
}

/// Recursive-descent parser; every production returns the subtree's depth so
/// that a tree deeper than [`MAX_DEPTH`] is refused while it is built (the
/// tree walkers recurse, so the depth bound is also their stack bound)
struct ExprParser {
    tokens: Vec<Token>,
    pos: usize,
}

type Parsed = (Node, usize);

const fn deeper(depth: usize) -> Result<usize> {
    let d = depth + 1;
    if d > MAX_DEPTH {
        Err(ResearchLawError::NestingTooDeep)
    } else {
        Ok(d)
    }
}

impl ExprParser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn sum(&mut self, level: usize) -> Result<Parsed> {
        deeper(level)?;
        let (mut lhs, mut d) = self.product(level)?;
        while let Some(op @ (Tok::Plus | Tok::Minus)) = self.peek() {
            let op = if *op == Tok::Plus {
                BinOp::Add
            } else {
                BinOp::Sub
            };
            self.pos += 1;
            let (rhs, rd) = self.product(level)?;
            d = deeper(d.max(rd))?;
            lhs = Node::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok((lhs, d))
    }

    fn product(&mut self, level: usize) -> Result<Parsed> {
        let (mut lhs, mut d) = self.unary(level)?;
        while let Some(op @ (Tok::Star | Tok::Slash)) = self.peek() {
            let op = if *op == Tok::Star {
                BinOp::Mul
            } else {
                BinOp::Div
            };
            self.pos += 1;
            let (rhs, rd) = self.unary(level)?;
            d = deeper(d.max(rd))?;
            lhs = Node::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok((lhs, d))
    }

    fn unary(&mut self, level: usize) -> Result<Parsed> {
        if self.peek() == Some(&Tok::Minus) {
            deeper(level)?;
            self.pos += 1;
            let (inner, d) = self.unary(level + 1)?;
            return Ok((Node::Neg(Box::new(inner)), deeper(d)?));
        }
        self.power(level)
    }

    fn power(&mut self, level: usize) -> Result<Parsed> {
        let (base, d) = self.primary(level)?;
        if self.peek() != Some(&Tok::Caret) {
            return Ok((base, d));
        }
        self.pos += 1;
        let (exponent, _) = self.unary(level + 1)?;
        let mut names = BTreeSet::new();
        collect_identifiers(&exponent, &mut names);
        if !names.is_empty() {
            return Err(ResearchLawError::NonConstantExponent);
        }
        let p = eval(&compile(&exponent, &[], &[])?, &[], &[])?;
        Ok((Node::Pow(Box::new(base), p), deeper(d)?))
    }

    fn primary(&mut self, level: usize) -> Result<Parsed> {
        let Some(tok) = self.tokens.get(self.pos).cloned() else {
            return Err(ResearchLawError::UnexpectedEnd);
        };
        self.pos += 1;
        match tok.kind {
            Tok::Num(v) => Ok((Node::Num(v), 1)),
            Tok::Ident(name) => {
                if self.peek() != Some(&Tok::LParen) {
                    return Ok((Node::Ident(name), 1));
                }
                let func = Func::from_name(&name).ok_or(ResearchLawError::UnknownFunction(name))?;
                let open = self.tokens[self.pos].position;
                self.pos += 1;
                let mut args = Vec::new();
                let mut depth = 0;
                loop {
                    let (arg, d) = self.sum(level + 1)?;
                    args.push(arg);
                    depth = depth.max(d);
                    if self.peek() == Some(&Tok::Comma) {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                self.close(open)?;
                let (lo, hi) = func.arity();
                if args.len() < lo || args.len() > hi {
                    return Err(ResearchLawError::ArgumentCount {
                        function: func.name(),
                        found: args.len(),
                    });
                }
                Ok((Node::Call(func, args), deeper(depth)?))
            }
            Tok::LParen => {
                let (inner, d) = self.sum(level + 1)?;
                self.close(tok.position)?;
                Ok((inner, d))
            }
            Tok::RParen => Err(ResearchLawError::UnbalancedParenthesis {
                position: tok.position,
            }),
            _ => Err(ResearchLawError::UnexpectedToken {
                position: tok.position,
            }),
        }
    }

    fn close(&mut self, open: usize) -> Result<()> {
        if self.peek() == Some(&Tok::RParen) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ResearchLawError::UnbalancedParenthesis { position: open })
        }
    }
}

fn compile(node: &Node, inputs: &[&str], params: &[&str]) -> Result<Code> {
    Ok(match node {
        Node::Num(v) => Code::Num(*v),
        Node::Ident(name) => {
            if let Some(i) = inputs.iter().position(|n| n == name) {
                Code::Input(i)
            } else if let Some(j) = params.iter().position(|n| n == name) {
                Code::Param(j)
            } else {
                return Err(ResearchLawError::UnknownIdentifier(name.clone()));
            }
        }
        Node::Neg(a) => Code::Neg(Box::new(compile(a, inputs, params)?)),
        Node::Bin(op, a, b) => Code::Bin(
            *op,
            Box::new(compile(a, inputs, params)?),
            Box::new(compile(b, inputs, params)?),
        ),
        Node::Pow(a, p) => Code::Pow(Box::new(compile(a, inputs, params)?), *p),
        Node::Call(f, args) => Code::Call(
            *f,
            args.iter()
                .map(|a| compile(a, inputs, params))
                .collect::<Result<_>>()?,
        ),
    })
}

fn dimension_of(code: &Code, inputs: &[Dimension], params: &[Dimension]) -> Result<Dimension> {
    match code {
        Code::Num(_) => Ok(Dimension::DIMENSIONLESS),
        Code::Input(i) => Ok(inputs[*i]),
        Code::Param(j) => Ok(params[*j]),
        Code::Neg(a) => dimension_of(a, inputs, params),
        Code::Bin(op, a, b) => {
            let left = dimension_of(a, inputs, params)?;
            let right = dimension_of(b, inputs, params)?;
            match op {
                BinOp::Add | BinOp::Sub => {
                    if left == right {
                        Ok(left)
                    } else {
                        Err(ResearchLawError::IncompatibleDimensions { left, right })
                    }
                }
                BinOp::Mul => left.mul(right),
                BinOp::Div => left.div(right),
            }
        }
        Code::Pow(a, p) => dimension_of(a, inputs, params)?.pow(*p),
        Code::Call(f, args) => {
            let dims = args
                .iter()
                .map(|a| dimension_of(a, inputs, params))
                .collect::<Result<Vec<_>>>()?;
            match f {
                // every argument has the dimension of the first; atan2 is a ratio
                Func::Atan2 | Func::Min | Func::Max => {
                    for &d in &dims[1..] {
                        if d != dims[0] {
                            return Err(ResearchLawError::IncompatibleDimensions {
                                left: dims[0],
                                right: d,
                            });
                        }
                    }
                    Ok(if *f == Func::Atan2 {
                        Dimension::DIMENSIONLESS
                    } else {
                        dims[0]
                    })
                }
                Func::Sqrt => dims[0].pow(0.5),
                Func::Exp | Func::Ln | Func::Sin | Func::Cos => {
                    if dims[0].is_dimensionless() {
                        Ok(dims[0])
                    } else {
                        Err(ResearchLawError::DimensionfulArgument { function: f.name() })
                    }
                }
            }
        }
    }
}

const fn finite(v: f64) -> Result<f64> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(ResearchLawError::NonFinite)
    }
}

/// Evaluates in SI; every intermediate value must be finite
// an integral exponent within `i32` uses `powi` (repeated multiplication,
// independent of the platform's `pow`); the cast is range-checked
#[allow(clippy::float_cmp, clippy::cast_possible_truncation)]
fn eval(code: &Code, inputs: &[f64], params: &[f64]) -> Result<f64> {
    let v = match code {
        Code::Num(v) => *v,
        Code::Input(i) => inputs[*i],
        Code::Param(j) => params[*j],
        Code::Neg(a) => -eval(a, inputs, params)?,
        Code::Bin(op, a, b) => {
            let x = eval(a, inputs, params)?;
            let y = eval(b, inputs, params)?;
            match op {
                BinOp::Add => x + y,
                BinOp::Sub => x - y,
                BinOp::Mul => x * y,
                BinOp::Div => x / y,
            }
        }
        Code::Pow(a, p) => {
            let x = eval(a, inputs, params)?;
            if *p == p.trunc() && p.abs() <= f64::from(i32::MAX) {
                powi_exact(x, *p as i32)
            } else {
                alice_det_math::powf64(x, *p)
            }
        }
        Code::Call(f, args) => {
            let xs = args
                .iter()
                .map(|a| eval(a, inputs, params))
                .collect::<Result<Vec<_>>>()?;
            let x = xs[0];
            match f {
                Func::Exp => alice_det_math::exp64(x),
                Func::Ln => alice_det_math::ln64(x),
                Func::Sqrt => alice_det_math::sqrt64(x),
                Func::Sin => alice_det_math::sin64(x),
                Func::Cos => alice_det_math::cos64(x),
                // the sign of a zero argument is ignored: `+ 0.0` turns −0 into +0 and leaves
                // every other value as it is, so atan2(−0, x < 0) is +π and the result stays
                // in (−π, π]
                Func::Atan2 => alice_det_math::atan2_64(x + 0.0, xs[1] + 0.0),
                // the first of equal arguments, so the sign of a zero is fixed by the order
                Func::Min => xs[1..].iter().fold(x, |m, &v| if v < m { v } else { m }),
                Func::Max => xs[1..].iter().fold(x, |m, &v| if v > m { v } else { m }),
            }
        }
    };
    finite(v)
}

/// `x^n` by repeated squaring from the least significant bit of `|n|`, then a
/// reciprocal for negative `n` — a fixed sequence of IEEE multiplications, so the
/// result is the same on every platform (the operation order of `f64::powi` is
/// not specified)
fn powi_exact(x: f64, n: i32) -> f64 {
    let (mut base, mut e, mut acc) = (x, n.unsigned_abs(), 1.0_f64);
    while e > 0 {
        if e & 1 == 1 {
            acc *= base;
        }
        base *= base;
        e >>= 1;
    }
    if n < 0 {
        1.0 / acc
    } else {
        acc
    }
}

// ── declarations ─────────────────────────────────────────────────────

/// A named input or output with its unit text
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Var {
    /// Name used in the expression and in conditions
    pub name: String,
    /// Unit text, parsed by [`Unit::parse`]
    pub unit: String,
}

impl Var {
    /// Variable from a name and a unit text
    #[must_use]
    pub fn new(name: &str, unit: &str) -> Self {
        Self {
            name: name.to_string(),
            unit: unit.to_string(),
        }
    }
}

/// A named parameter with its value, uncertainty and unit text
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// Name used in the expression
    pub name: String,
    /// Value in `unit`
    pub value: f64,
    /// Standard uncertainty in `unit`
    pub uncertainty: f64,
    /// Unit text, parsed by [`Unit::parse`]
    pub unit: String,
}

impl Param {
    /// Parameter from a name, value, uncertainty and unit text
    #[must_use]
    pub fn new(name: &str, value: f64, uncertainty: f64, unit: &str) -> Self {
        Self {
            name: name.to_string(),
            value,
            uncertainty,
            unit: unit.to_string(),
        }
    }
}

fn owned_conditions(conditions: &[(&str, f64)]) -> Vec<(String, f64)> {
    conditions
        .iter()
        .map(|&(name, v)| (name.to_string(), v))
        .collect()
}

fn borrowed_conditions(conditions: &[(String, f64)]) -> Vec<(&str, f64)> {
    conditions.iter().map(|(n, v)| (n.as_str(), *v)).collect()
}

/// A reference value the law has to reproduce
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchOracle {
    /// Input values, each in its input's unit
    pub conditions: Vec<(String, f64)>,
    /// Reference value in the output unit
    pub expected: f64,
    /// Largest accepted `|f(conditions) - expected|`
    pub tolerance: f64,
    /// Where the reference value comes from
    pub source: String,
}

impl ResearchOracle {
    /// A reference value with its conditions, tolerance and source
    #[must_use]
    pub fn new(conditions: &[(&str, f64)], expected: f64, tolerance: f64, source: &str) -> Self {
        Self {
            conditions: owned_conditions(conditions),
            expected,
            tolerance,
            source: source.to_string(),
        }
    }
}

/// Result of checking one [`ResearchOracle`]
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchOracleOutcome {
    /// Value of the law at the case's conditions, or why there is none
    pub value: Result<f64>,
    /// `|value - expected|` when the law has a value
    pub error: Option<f64>,
    /// `error ≤ tolerance` (false when the law has no value)
    pub passed: bool,
}

/// One measured value of the output at given conditions
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    /// Input values, each in its input's unit
    pub conditions: Vec<(String, f64)>,
    /// Observed output in the output unit
    pub value: f64,
}

impl Observation {
    /// Observation from conditions and the observed value
    #[must_use]
    pub fn new(conditions: &[(&str, f64)], value: f64) -> Self {
        Self {
            conditions: owned_conditions(conditions),
            value,
        }
    }
}

/// What new evidence does to a law (see the module docs for the rules)
#[derive(Debug, Clone, PartialEq)]
pub enum ResearchVerdict {
    /// No evidence was given
    NoEvidence,
    /// `outside` observations cannot be judged; nothing was judged
    OutOfRange {
        /// Number of observations the law cannot be evaluated at
        outside: usize,
    },
    /// The evidence agrees with the law
    Supports {
        /// RMS of the new observations about the law
        rms: f64,
    },
    /// The same expression fits stored and new evidence with other parameters
    ParameterUpdate {
        /// RMS of the new observations about the current law
        previous_rms: f64,
        /// The law with refitted parameters, holding stored + new evidence
        updated: Box<ResearchLaw>,
    },
    /// The deviation exceeds the band but not the break threshold
    ResidualGrew {
        /// RMS of the new observations about the law
        rms: f64,
    },
    /// The evidence is not described by this law
    Breaks {
        /// RMS of the new observations about the law
        rms: f64,
    },
}

// ── the law ──────────────────────────────────────────────────────────

/// `output = f(inputs; parameters)` with units, valid ranges, evidence,
/// measured residual, provenance and oracle cases
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchLaw {
    name: String,
    expr: LawExpr,
    code: Code,
    output: Var,
    output_unit: Unit,
    inputs: Vec<Var>,
    input_units: Vec<Unit>,
    params: Vec<Param>,
    param_units: Vec<Unit>,
    /// Valid range per input (same index as `inputs`), in the input's unit
    validity: Vec<Option<ValidRange>>,
    provenance: Provenance,
    oracles: Vec<ResearchOracle>,
    evidence: Vec<Observation>,
    residual: ResidualStats,
}

const NO_RESIDUAL: ResidualStats = ResidualStats {
    n: 0,
    rms: 0.0,
    max_abs: 0.0,
};

impl ResearchLaw {
    /// Builds a law and checks its names, units and dimensions
    ///
    /// `validity` gives the valid range of some inputs in their declared
    /// units; inputs without one accept any finite value. The law starts
    /// without evidence (see [`Self::with_evidence`]).
    ///
    /// # Errors
    ///
    /// [`ResearchLawError::DuplicateName`] (a name used twice among output,
    /// inputs and parameters, or two ranges for one input),
    /// [`ResearchLawError::UnknownUnit`] / [`ResearchLawError::InvalidUnit`],
    /// [`ResearchLawError::NonFinite`] (a parameter value or uncertainty),
    /// [`ResearchLawError::UnknownVariable`] / [`ResearchLawError::InvalidRange`]
    /// (a range), the parse errors of [`LawExpr::parse`],
    /// [`ResearchLawError::UnknownIdentifier`], the dimension errors, and
    /// [`ResearchLawError::OutputDimension`]
    pub fn new(
        name: &str,
        expression: &str,
        output: Var,
        inputs: &[Var],
        params: &[Param],
        validity: &[(&str, ValidRange)],
        provenance: Provenance,
    ) -> Result<Self> {
        let mut names = BTreeSet::new();
        for n in std::iter::once(&output.name)
            .chain(inputs.iter().map(|v| &v.name))
            .chain(params.iter().map(|p| &p.name))
        {
            if !names.insert(n.as_str()) {
                return Err(ResearchLawError::DuplicateName(n.clone()));
            }
        }
        let output_unit = Unit::parse(&output.unit)?;
        let input_units = inputs
            .iter()
            .map(|v| Unit::parse(&v.unit))
            .collect::<Result<Vec<_>>>()?;
        let param_units = params
            .iter()
            .map(|p| Unit::parse(&p.unit))
            .collect::<Result<Vec<_>>>()?;
        if params
            .iter()
            .any(|p| !p.value.is_finite() || !p.uncertainty.is_finite())
        {
            return Err(ResearchLawError::NonFinite);
        }
        let mut ranges: Vec<Option<ValidRange>> = vec![None; inputs.len()];
        for &(var, range) in validity {
            let i = inputs
                .iter()
                .position(|v| v.name == var)
                .ok_or_else(|| ResearchLawError::UnknownVariable(var.to_string()))?;
            if ranges[i].is_some() {
                return Err(ResearchLawError::DuplicateName(var.to_string()));
            }
            if !range.lo.is_finite() || !range.hi.is_finite() || range.lo > range.hi {
                return Err(ResearchLawError::InvalidRange(var.to_string()));
            }
            ranges[i] = Some(range);
        }
        let expr = LawExpr::parse(expression)?;
        let input_names: Vec<&str> = inputs.iter().map(|v| v.name.as_str()).collect();
        let param_names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
        let code = compile(&expr.root, &input_names, &param_names)?;
        let in_dims: Vec<Dimension> = input_units.iter().map(|u| u.dimension).collect();
        let p_dims: Vec<Dimension> = param_units.iter().map(|u| u.dimension).collect();
        let found = dimension_of(&code, &in_dims, &p_dims)?;
        if found != output_unit.dimension {
            return Err(ResearchLawError::OutputDimension {
                expected: output_unit.dimension,
                found,
            });
        }
        Ok(Self {
            name: name.to_string(),
            expr,
            code,
            output,
            output_unit,
            inputs: inputs.to_vec(),
            input_units,
            params: params.to_vec(),
            param_units,
            validity: ranges,
            provenance,
            oracles: Vec::new(),
            evidence: Vec::new(),
            residual: NO_RESIDUAL,
        })
    }

    /// Stores `observations` as the law's evidence and measures its residual
    ///
    /// # Errors
    ///
    /// The errors of [`Self::evaluate`] for an observation the law cannot be
    /// evaluated at, [`ResearchLawError::NonFinite`] for a non-finite
    /// observed value
    pub fn with_evidence(mut self, observations: &[Observation]) -> Result<Self> {
        let values = self.param_values();
        self.residual = self.residual_over(observations, &values)?;
        self.evidence = observations.to_vec();
        Ok(self)
    }

    /// Adds a reference value the law has to reproduce
    #[must_use]
    pub fn with_oracle(mut self, oracle: ResearchOracle) -> Self {
        self.oracles.push(oracle);
        self
    }

    /// Name of the law
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The expression
    #[must_use]
    pub const fn expression(&self) -> &LawExpr {
        &self.expr
    }

    /// The output variable
    #[must_use]
    pub const fn output(&self) -> &Var {
        &self.output
    }

    /// The input variables
    #[must_use]
    pub fn inputs(&self) -> &[Var] {
        &self.inputs
    }

    /// The parameters (refitted values after a [`ResearchVerdict::ParameterUpdate`])
    #[must_use]
    pub fn params(&self) -> &[Param] {
        &self.params
    }

    /// Value of the parameter `name` in its unit
    #[must_use]
    pub fn param(&self, name: &str) -> Option<f64> {
        self.params.iter().find(|p| p.name == name).map(|p| p.value)
    }

    /// Valid range of the input `name` in its unit (`None`: unrestricted or unknown)
    #[must_use]
    pub fn validity(&self, name: &str) -> Option<ValidRange> {
        let i = self.inputs.iter().position(|v| v.name == name)?;
        self.validity[i]
    }

    /// Source and method
    #[must_use]
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// Oracle cases attached with [`Self::with_oracle`]
    #[must_use]
    pub fn oracles(&self) -> &[ResearchOracle] {
        &self.oracles
    }

    /// The observations the law was built or updated from
    #[must_use]
    pub fn evidence(&self) -> &[Observation] {
        &self.evidence
    }

    /// Residual measured over [`Self::evidence`] (zero points when there is none)
    #[must_use]
    pub const fn residual(&self) -> ResidualStats {
        self.residual
    }

    fn param_values(&self) -> Vec<f64> {
        self.params.iter().map(|p| p.value).collect()
    }

    /// Inputs in SI, in declaration order, after all condition checks
    fn inputs_si(&self, conditions: &[(&str, f64)]) -> Result<Vec<f64>> {
        let mut raw: Vec<Option<f64>> = vec![None; self.inputs.len()];
        for &(name, v) in conditions {
            let i = self
                .inputs
                .iter()
                .position(|var| var.name == name)
                .ok_or_else(|| ResearchLawError::UnknownVariable(name.to_string()))?;
            if raw[i].is_some() {
                return Err(ResearchLawError::DuplicateName(name.to_string()));
            }
            raw[i] = Some(v);
        }
        let mut si = Vec::with_capacity(raw.len());
        for (i, value) in raw.iter().enumerate() {
            let var = &self.inputs[i].name;
            let v = value.ok_or_else(|| ResearchLawError::MissingVariable(var.clone()))?;
            let inside = v.is_finite() && self.validity[i].is_none_or(|r| r.contains(v));
            if !inside {
                return Err(ResearchLawError::OutOfRange {
                    variable: var.clone(),
                });
            }
            si.push(finite(v * self.input_units[i].scale)?);
        }
        Ok(si)
    }

    /// `f(conditions)` in the output unit with the given parameter values
    fn evaluate_with(&self, conditions: &[(&str, f64)], params: &[f64]) -> Result<f64> {
        let xs = self.inputs_si(conditions)?;
        let ps: Vec<f64> = params
            .iter()
            .zip(&self.param_units)
            .map(|(v, u)| v * u.scale)
            .collect();
        let si = eval(&self.code, &xs, &ps)?;
        finite(si / self.output_unit.scale)
    }

    /// `f(conditions)` in the output unit; conditions are given in the inputs' units
    ///
    /// # Errors
    ///
    /// [`ResearchLawError::UnknownVariable`] / [`ResearchLawError::DuplicateName`]
    /// for a condition that is not an input or is given twice,
    /// [`ResearchLawError::MissingVariable`] for an input without a value,
    /// [`ResearchLawError::OutOfRange`] for an input outside its valid range or
    /// not finite, [`ResearchLawError::NonFinite`] when the evaluation produces
    /// a non-finite value
    pub fn evaluate(&self, conditions: &[(&str, f64)]) -> Result<f64> {
        self.evaluate_with(conditions, &self.param_values())
    }

    /// Checks every attached oracle case
    #[must_use]
    pub fn check_oracles(&self) -> Vec<ResearchOracleOutcome> {
        self.oracles
            .iter()
            .map(|case| {
                let value = self.evaluate(&borrowed_conditions(&case.conditions));
                let error = value.as_ref().ok().map(|v| (v - case.expected).abs());
                ResearchOracleOutcome {
                    value,
                    error,
                    passed: error.is_some_and(|e| e <= case.tolerance),
                }
            })
            .collect()
    }

    /// `observed - f(conditions)` statistics with the given parameter values
    // the number of observations converts to f64 only to average; plain
    // multiply-add (no fused `mul_add`) keeps the documented evaluation order
    #[allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]
    fn residual_over(&self, observations: &[Observation], params: &[f64]) -> Result<ResidualStats> {
        if observations.is_empty() {
            return Ok(NO_RESIDUAL);
        }
        let mut sum_sq = 0.0_f64;
        let mut max_abs = 0.0_f64;
        for o in observations {
            let observed = finite(o.value)?;
            let d = observed - self.evaluate_with(&borrowed_conditions(&o.conditions), params)?;
            sum_sq += d * d;
            max_abs = max_abs.max(d.abs());
        }
        Ok(ResidualStats {
            n: observations.len(),
            rms: finite((sum_sq / observations.len() as f64).sqrt())?,
            max_abs,
        })
    }

    /// Sum of squared residuals, `+∞` when the law has no value somewhere
    fn ssr(&self, observations: &[Observation], params: &[f64]) -> f64 {
        self.residual_over(observations, params)
            .map_or(f64::INFINITY, |r| {
                #[allow(clippy::cast_precision_loss)]
                let n = r.n as f64;
                r.rms * r.rms * n
            })
    }

    /// Residuals `observed - f` and the central-difference Jacobian `∂f/∂p`
    fn linearize(
        &self,
        observations: &[Observation],
        p: &[f64],
    ) -> Option<(Vec<f64>, Vec<Vec<f64>>)> {
        let mut r = Vec::with_capacity(observations.len());
        let mut jac = Vec::with_capacity(observations.len());
        let mut shifted = p.to_vec();
        for o in observations {
            let cond = borrowed_conditions(&o.conditions);
            r.push(o.value - self.evaluate_with(&cond, p).ok()?);
            let mut row = Vec::with_capacity(p.len());
            for j in 0..p.len() {
                let h = GN_FD_STEP * p[j].abs().max(1.0);
                shifted[j] = p[j] + h;
                let up = self.evaluate_with(&cond, &shifted).ok()?;
                shifted[j] = p[j] - h;
                let down = self.evaluate_with(&cond, &shifted).ok()?;
                shifted[j] = p[j];
                row.push((up - down) / (2.0 * h));
            }
            jac.push(row);
        }
        Some((r, jac))
    }

    /// Least-squares refit of every parameter by Gauss–Newton (module docs);
    /// `None` for a law without parameters or when the first linearization fails
    fn refit(&self, observations: &[Observation]) -> Option<Self> {
        if self.params.is_empty() {
            return None;
        }
        let k = self.params.len();
        let mut p = self.param_values();
        let mut ssr = self.ssr(observations, &p);
        let mut last_jac = None;
        for _ in 0..GN_MAX_ITERATIONS {
            let Some((r, jac)) = self.linearize(observations, &p) else {
                break;
            };
            let jtj = normal_matrix(&jac, k);
            let jtr: Vec<f64> = (0..k)
                .map(|a| jac.iter().zip(&r).map(|(row, ri)| row[a] * ri).sum())
                .collect();
            last_jac = Some(jtj.clone());
            let Some(step) = solve(jtj, jtr) else {
                break;
            };
            let mut lambda = 1.0_f64;
            let mut accepted = None;
            for _ in 0..=GN_MAX_HALVINGS {
                let cand: Vec<f64> = p
                    .iter()
                    .zip(&step)
                    .map(|(pi, si)| pi + lambda * si)
                    .collect();
                let c_ssr = self.ssr(observations, &cand);
                if c_ssr < ssr {
                    accepted = Some((cand, c_ssr));
                    break;
                }
                lambda *= 0.5;
            }
            let Some((cand, c_ssr)) = accepted else {
                break;
            };
            let converged = cand
                .iter()
                .zip(&p)
                .all(|(new, old)| (new - old).abs() <= GN_TOLERANCE * old.abs().max(1.0));
            p = cand;
            ssr = c_ssr;
            if converged {
                break;
            }
        }
        let mut updated = self.clone();
        let n = observations.len();
        let sigma = last_jac
            .filter(|_| n > k)
            .and_then(|_| self.linearize(observations, &p))
            .and_then(|(_, jac)| invert(&normal_matrix(&jac, k)))
            .map(|inv| {
                #[allow(clippy::cast_precision_loss)]
                let s2 = ssr / (n - k) as f64;
                (0..k).map(|i| (s2 * inv[i][i]).sqrt()).collect::<Vec<_>>()
            });
        for (j, param) in updated.params.iter_mut().enumerate() {
            param.value = p[j];
            if let Some(s) = sigma.as_ref().map(|s| s[j]).filter(|s| s.is_finite()) {
                param.uncertainty = s;
            }
        }
        updated.residual = updated.residual_over(observations, &p).ok()?;
        updated.evidence = observations.to_vec();
        Some(updated)
    }

    /// Judges new evidence against the law without changing it
    ///
    /// See the module documentation for the order of the rules.
    #[must_use]
    pub fn ingest(&self, observations: &[Observation], policy: &IngestPolicy) -> ResearchVerdict {
        if observations.is_empty() {
            return ResearchVerdict::NoEvidence;
        }
        let outside = observations
            .iter()
            .filter(|o| {
                !o.value.is_finite() || self.evaluate(&borrowed_conditions(&o.conditions)).is_err()
            })
            .count();
        if outside > 0 {
            return ResearchVerdict::OutOfRange { outside };
        }
        let band = policy.abs_tolerance.max(self.residual.rms);
        let Ok(stats) = self.residual_over(observations, &self.param_values()) else {
            return ResearchVerdict::OutOfRange {
                outside: observations.len(),
            };
        };
        let rms = stats.rms;
        if rms <= band {
            return ResearchVerdict::Supports { rms };
        }
        let mut combined = self.evidence.clone();
        combined.extend_from_slice(observations);
        if let Some(refit) = self.refit(&combined) {
            if refit.residual.rms <= band {
                return ResearchVerdict::ParameterUpdate {
                    previous_rms: rms,
                    updated: Box::new(refit),
                };
            }
        }
        if rms <= band * policy.break_factor {
            ResearchVerdict::ResidualGrew { rms }
        } else {
            ResearchVerdict::Breaks { rms }
        }
    }

    /// Plain-text description: expression with units, inputs and their
    /// ranges, parameters, provenance, residual and oracle count
    #[must_use]
    pub fn describe(&self) -> String {
        use fmt::Write as _;
        let mut s = String::new();
        // writing to a String cannot fail
        let _ = writeln!(s, "law: {}", self.name);
        let _ = writeln!(
            s,
            "  {} [{}] = {}",
            self.output.name,
            self.output.unit,
            self.expr.text()
        );
        for (var, range) in self.inputs.iter().zip(&self.validity) {
            match range {
                Some(r) => {
                    let _ = writeln!(
                        s,
                        "  input {} [{}] valid {}..={}",
                        var.name, var.unit, r.lo, r.hi
                    );
                }
                None => {
                    let _ = writeln!(
                        s,
                        "  input {} [{}] valid: any finite value",
                        var.name, var.unit
                    );
                }
            }
        }
        for p in &self.params {
            let _ = writeln!(
                s,
                "  param {} = {} ± {} [{}]",
                p.name, p.value, p.uncertainty, p.unit
            );
        }
        let _ = writeln!(
            s,
            "  provenance: {} (method: {})",
            self.provenance.source, self.provenance.method
        );
        let _ = writeln!(
            s,
            "  residual over {} observations: rms {} max_abs {}",
            self.residual.n, self.residual.rms, self.residual.max_abs
        );
        let _ = writeln!(s, "  oracle cases: {}", self.oracles.len());
        s
    }
}

/// `JᵀJ` (k × k)
fn normal_matrix(jac: &[Vec<f64>], k: usize) -> Vec<Vec<f64>> {
    (0..k)
        .map(|a| {
            (0..k)
                .map(|b| jac.iter().map(|row| row[a] * row[b]).sum())
                .collect()
        })
        .collect()
}

/// Solves `m x = rhs` by Gaussian elimination with partial pivoting;
/// `None` when a pivot is not larger than `1e-14 ×` the largest diagonal
// index loops mirror the textbook elimination; no fused `mul_add` (fixed
// operation order, see the module docs)
#[allow(clippy::needless_range_loop, clippy::suboptimal_flops)]
fn solve(mut m: Vec<Vec<f64>>, mut rhs: Vec<f64>) -> Option<Vec<f64>> {
    let k = rhs.len();
    let scale = (0..k).map(|i| m[i][i].abs()).fold(0.0_f64, f64::max);
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    for col in 0..k {
        let pivot = (col..k).max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))?;
        if m[pivot][col].abs() <= 1e-14 * scale {
            return None;
        }
        m.swap(col, pivot);
        rhs.swap(col, pivot);
        for row in col + 1..k {
            let f = m[row][col] / m[col][col];
            for c in col..k {
                let sub = f * m[col][c];
                m[row][c] -= sub;
            }
            let sub = f * rhs[col];
            rhs[row] -= sub;
        }
    }
    let mut x = vec![0.0; k];
    for row in (0..k).rev() {
        let mut acc = rhs[row];
        for c in row + 1..k {
            acc -= m[row][c] * x[c];
        }
        x[row] = acc / m[row][row];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// Inverse by solving against each unit vector
fn invert(m: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let k = m.len();
    let cols = (0..k)
        .map(|i| {
            let mut e = vec![0.0; k];
            e[i] = 1.0;
            solve(m.to_vec(), e)
        })
        .collect::<Option<Vec<_>>>()?;
    Some(
        (0..k)
            .map(|r| (0..k).map(|c| cols[c][r]).collect())
            .collect(),
    )
}

// ── comparison ───────────────────────────────────────────────────────

/// Maps input names of one law to input names of another
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bridge {
    pairs: Vec<(String, String)>,
}

impl Bridge {
    /// Bridge from `(input of a, input of b)` pairs
    #[must_use]
    pub fn new(pairs: &[(&str, &str)]) -> Self {
        Self {
            pairs: pairs
                .iter()
                .map(|&(a, b)| (a.to_string(), b.to_string()))
                .collect(),
        }
    }

    /// The `(input of a, input of b)` pairs
    #[must_use]
    pub fn pairs(&self) -> &[(String, String)] {
        &self.pairs
    }
}

/// Two laws evaluated under the same conditions, in the first law's output unit
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Comparison {
    /// Value of the first law
    pub a_value: f64,
    /// Value of the second law, converted to the first law's output unit
    pub b_value_in_a_unit: f64,
    /// `a_value - b_value_in_a_unit`
    pub difference: f64,
    /// `|difference| / max(|a_value|, |b_value_in_a_unit|)` (0 when both are 0)
    pub relative: f64,
}

fn unit_of(law: &ResearchLaw, name: &str) -> Result<Unit> {
    law.inputs
        .iter()
        .position(|v| v.name == name)
        .map(|i| law.input_units[i])
        .ok_or_else(|| ResearchLawError::UnknownVariable(name.to_string()))
}

/// Evaluates `a` and `b` under the same conditions and compares the results
///
/// `conditions` are `a`'s inputs in `a`'s units. Each bridge pair carries the
/// value of an input of `a` to an input of `b`, converted between their units
/// (`v_b = v_a · scale_a / scale_b`); `b`'s output is converted to `a`'s
/// output unit the same way.
///
/// # Errors
///
/// [`ResearchLawError::UnknownVariable`] for a bridge name that is not an
/// input of its law, [`ResearchLawError::IncompatibleDimensions`] for a pair
/// or the two outputs with different dimensions,
/// [`ResearchLawError::MissingVariable`] for an input of `b` the bridge does
/// not cover or an input of `a` without a value, the errors of
/// [`ResearchLaw::evaluate`] for either law, and
/// [`ResearchLawError::NonFinite`] for a non-finite converted value
pub fn compare(
    a: &ResearchLaw,
    b: &ResearchLaw,
    bridge: &Bridge,
    conditions: &[(&str, f64)],
) -> Result<Comparison> {
    let mut factors = Vec::with_capacity(bridge.pairs.len());
    for (an, bn) in &bridge.pairs {
        let ua = unit_of(a, an)?;
        let ub = unit_of(b, bn)?;
        if ua.dimension != ub.dimension {
            return Err(ResearchLawError::IncompatibleDimensions {
                left: ua.dimension,
                right: ub.dimension,
            });
        }
        factors.push((an.as_str(), bn.as_str(), ua.scale, ub.scale));
    }
    if let Some(missing) = b
        .inputs
        .iter()
        .find(|v| !bridge.pairs.iter().any(|(_, bn)| *bn == v.name))
    {
        return Err(ResearchLawError::MissingVariable(missing.name.clone()));
    }
    if a.output_unit.dimension != b.output_unit.dimension {
        return Err(ResearchLawError::IncompatibleDimensions {
            left: a.output_unit.dimension,
            right: b.output_unit.dimension,
        });
    }
    let a_value = a.evaluate(conditions)?;
    let mut b_conditions = Vec::with_capacity(factors.len());
    for &(an, bn, sa, sb) in &factors {
        let v = conditions
            .iter()
            .find(|(n, _)| *n == an)
            .map(|&(_, v)| v)
            .ok_or_else(|| ResearchLawError::MissingVariable(an.to_string()))?;
        b_conditions.push((bn, finite(v * sa / sb)?));
    }
    let b_value = b.evaluate(&b_conditions)?;
    let b_value_in_a_unit = finite(b_value * b.output_unit.scale / a.output_unit.scale)?;
    let difference = finite(a_value - b_value_in_a_unit)?;
    let denom = a_value.abs().max(b_value_in_a_unit.abs());
    let relative = if denom == 0.0 {
        0.0
    } else {
        difference.abs() / denom
    };
    Ok(Comparison {
        a_value,
        b_value_in_a_unit,
        difference,
        relative,
    })
}
