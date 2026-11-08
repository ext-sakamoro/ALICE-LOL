//! World laws (`kind world`): what things are made of and whether it rains
//!
//! A world law names its parameters and states, for each named material, how dense it is, how
//! it grips, how it bounces and how it gives, and whether it rains and how hard. The fields are
//! the parameters of `alice-physics` under plain names, in the units that crate uses, so "softer"
//! is another law text with other values, not a word this module turns into numbers. The same
//! law and the same parameter values give the same bits on every platform: the expressions are
//! the expressions of research laws (`f64` without fused multiply-add, `alice-det-math` for every
//! function).
//!
//! ```text
//! law flag_in_the_rain
//! kind world
//! claim a cotton flag that hardly bounces, under light rain
//! param softness number = 0.01 range 0 0.1
//! param raining switch = on
//! param strength number = 4 range 0 100
//! material cotton density = 1500
//! material cotton friction = 0.6
//! material cotton bounce = 0.1
//! material cotton stretch_compliance = softness
//! material cotton bend_compliance = 0.25
//! material cotton damping = 0.985
//! rain = raining
//! rain_rate = strength
//! ```
//!
//! # Lines
//!
//! - `law <name>` first, then `kind world`; `claim` / `source` lines are notes (not in the
//!   identifier); `#` starts a comment line
//! - `param <name> number = <default> range <lo> <hi>`: a named number, `lo <= default <= hi`
//! - `param <name> switch = on|off`: a named switch, read only by the `rain` line
//! - `let <name> = <expr>`: a value for the later lines
//! - `material <name> <field> = <expr>`: one of [`FIELDS`], at most once per material
//! - `material <name> <field> basis measured|assumed`: how `shear_modulus`, `poisson_ratio`,
//!   `adhesion` or `contact_area` was obtained, after its value; the law's `source` lines say where
//!   a measured value comes from. Young's modulus is never stated:
//!   [`BoundMaterial::youngs_modulus`] derives it as 2 G (1 + ν)
//! - `material <name> dynamic shear|youngs at <f> storage <expr> loss <expr>`: one row of a
//!   dynamic-modulus table, the storage and loss moduli (G' and G'' for `shear`, E' and E'' for
//!   `youngs`, Pa) measured at the frequency `f` (Hz, a plain decimal above 0), one line per
//!   frequency, one kind per material; `material <name> dynamic basis measured|assumed` after the
//!   rows. Between the stated frequencies [`DynamicModulus::at`] interpolates on log-log axes;
//!   outside them it answers nothing (no extrapolation, no relaxation model is fitted). Data
//!   measured under different conditions (storage, temperature) go in different materials
//! - `hand_youngs_modulus = <expr>` (Pa) and `hand_poisson_ratio = <expr>`: the skin of the hand
//!   that holds things, both or neither, each optionally followed by `<key> basis measured|assumed`;
//!   `hand_youngs_modulus spread <lo> <hi>` (plain decimals, `0 < lo <= value <= hi`) states the
//!   range the reported values span, and the plain text then gives the grip stiffness over it
//! - `rain = on|off|<switch>` and `rain_rate = <expr>` (millimetres an hour): both or neither;
//!   neither means the law says nothing about rain, which reads as no rain
//!
//! Expressions read `pi`, the number parameters and the earlier `let` values. Every name is
//! declared once. A material states `density`, `friction` and `bounce`; a field it leaves out of
//! the others is left as the receiving configuration has it, so this module adds no default.
//!
//! # Values
//!
//! A value outside its field's range is refused by [`WorldLaw::bind`], naming the line; nothing
//! is clamped. The defaults must bind, so a law that reads always has one instance.
//!
//! # Plain text
//!
//! [`BoundWorld::describe`] states what a value does, computed from the value. It uses no
//! thresholds of its own (no "soft" or "heavy" past a chosen number).
//!
//! # Grip
//!
//! A hand holds a material through a spring that lets go when its force passes the material's
//! `adhesion` (N). The spring's stiffness is not stated by a law; [`BoundWorld::grip`] derives it
//! so it cannot contradict the elastic constants. Model: a flat circular contact of area
//! `contact_area` (radius `a = sqrt(A / pi)`) pressed into an elastic half-space has the normal
//! stiffness `k = 2 a E*`, `E* = E / (1 - nu^2)` (Boussinesq's flat punch); the material and the
//! hand's skin are two such springs in series, `k = k_m k_h / (k_m + k_h)`. Range: the half-space
//! holds while `a` is much smaller than the thickness of the held body (a body 160 mm deep held
//! at `a` = 8 mm is 20 radii deep); a thin sheet held across its thickness is outside it. Left
//! out: friction in the contact plane, the curvature of the finger, rate dependence.

use std::fmt;
use std::fmt::Write as _;

use crate::law_id::{fold_semantics_pins, hex32, LawIdHasher};
use crate::research_law::{CompiledExpr, LawExpr};

/// The kind tag of a world law, hashed into its identifier
pub const WORLD_LAW_KIND: &[u8] = b"lol.law.world/v1";

/// The fields a material may state, in the order [`BoundMaterial`] holds them
pub const FIELDS: [&str; 15] = [
    "density",
    "friction",
    "static_friction",
    "bounce",
    "stretch_compliance",
    "bend_compliance",
    "edge_compliance",
    "volume_compliance",
    "shape_stiffness",
    "damping",
    "sdf_friction",
    "shear_modulus",
    "poisson_ratio",
    "adhesion",
    "contact_area",
];

const DENSITY: usize = 0;
const FRICTION: usize = 1;
const STATIC_FRICTION: usize = 2;
const BOUNCE: usize = 3;
const STRETCH_COMPLIANCE: usize = 4;
const BEND_COMPLIANCE: usize = 5;
const EDGE_COMPLIANCE: usize = 6;
const VOLUME_COMPLIANCE: usize = 7;
const SHAPE_STIFFNESS: usize = 8;
const DAMPING: usize = 9;
const SDF_FRICTION: usize = 10;
const SHEAR_MODULUS: usize = 11;
const POISSON_RATIO: usize = 12;
const ADHESION: usize = 13;
const CONTACT_AREA: usize = 14;

/// The largest value of an unbounded field (2^53, every integer up to it is an exact `f64`, and
/// well inside the range of `Fix128`)
pub const MAX_VALUE: f64 = 9_007_199_254_740_992.0;

/// The density of water in kg/m^3, which the plain text compares with
const WATER_DENSITY: f64 = 1000.0;

/// The most parameters
pub const MAX_PARAMS: usize = 64;
/// The most `let` lines
pub const MAX_LETS: usize = 256;
/// The most materials
pub const MAX_MATERIALS: usize = 64;
/// The most lines
pub const MAX_LINES: usize = 1024;

/// What the reading of a world law means, as pins measured from the behaviour of this module
/// ([`world_semantics_fingerprints`]; the oracle recomputes every one)
pub const WORLD_SEMANTICS_PINS: &[(&str, [u8; 32])] = &[
    (
        "world: field ranges, refused not clamped, and the plain text computed from the values",
        hex32("c577bf8750864369a83018b690202dbe75a9758164e64fe31feb10f6fd5adb6b"),
    ),
    (
        "world: the expression functions of research laws",
        hex32("ee1b39dde2b1e577892f19daee2fb514a7b43f56be7d2211061de1ddac28fad3"),
    ),
];

/// The fold of [`WORLD_SEMANTICS_PINS`] (the oracle checks the two agree)
pub const WORLD_SEMANTICS_ID: [u8; 32] =
    hex32("5a1f3dda567944e983d6160948df81aeb124c26eb44e59880aaa4a3873296f43");

/// A world law that does not read, with the line it was on (0 for the whole file)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldLawError {
    /// the 1-based line, 0 when the problem is not on one line
    pub line: usize,
    /// what is wrong
    pub message: String,
}

impl fmt::Display for WorldLawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for WorldLawError {}

fn err(line: usize, message: impl Into<String>) -> WorldLawError {
    WorldLawError {
        line,
        message: message.into(),
    }
}

/// Why a world law could not take a set of values
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindError {
    /// a parameter the law does not declare
    UnknownParam(String),
    /// a parameter given twice
    RepeatedParam(String),
    /// a number given for a switch, or a switch for a number
    WrongType(String),
    /// a number outside its declared range, or not finite
    ParamOutOfRange(String),
    /// a `let` value, a field or the rain rate is not finite
    NonFinite {
        /// the line of the law
        line: usize,
        /// what the line names
        name: String,
    },
    /// a field or the rain rate is outside the range it has in `alice-physics` or here
    OutOfRange {
        /// the line of the law
        line: usize,
        /// what the line names
        name: String,
        /// the range, in words
        range: &'static str,
    },
}

impl fmt::Display for BindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownParam(n) => write!(f, "the law has no parameter `{n}`"),
            Self::RepeatedParam(n) => write!(f, "parameter `{n}` is given twice"),
            Self::WrongType(n) => write!(f, "parameter `{n}` is of the other type"),
            Self::ParamOutOfRange(n) => write!(f, "parameter `{n}` is outside its range"),
            Self::NonFinite { line, name } => write!(f, "line {line}: `{name}` is not finite"),
            Self::OutOfRange { line, name, range } => {
                write!(f, "line {line}: `{name}` is not {range}")
            }
        }
    }
}

impl std::error::Error for BindError {}

/// A value for a parameter
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorldValue {
    /// for a `number` parameter
    Number(f64),
    /// for a `switch` parameter
    Switch(bool),
}

#[derive(Debug, Clone)]
enum ParamKind {
    Number { default: f64, lo: f64, hi: f64 },
    Switch { default: bool },
}

#[derive(Debug, Clone)]
struct Param {
    name: String,
    kind: ParamKind,
}

#[derive(Debug, Clone)]
struct Line {
    at: usize,
    name: String,
    expr: LawExpr,
    code: CompiledExpr,
}

#[derive(Debug, Clone)]
struct Material {
    name: String,
    /// sorted by slot in [`FIELDS`]
    fields: Vec<(usize, Line)>,
    /// how a field's value was obtained, by slot, sorted (`material <name> <field> basis <...>`)
    bases: Vec<(usize, Basis)>,
    /// the dynamic-modulus table, sorted by frequency
    dynamic: Option<DynamicLines>,
}

#[derive(Debug, Clone)]
struct DynamicLines {
    kind: ModulusKind,
    /// (frequency in Hz, storage, loss), sorted by frequency
    rows: Vec<(f64, Line, Line)>,
    basis: Option<Basis>,
}

/// Which modulus a dynamic table states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModulusKind {
    /// G' and G'', in shear
    Shear,
    /// E' and E'', in tension or compression
    Youngs,
}

impl ModulusKind {
    const fn word(self) -> &'static str {
        match self {
            Self::Shear => "shear",
            Self::Youngs => "youngs",
        }
    }
}

/// One stated row of a dynamic-modulus table
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DynamicRow {
    /// Hz, above 0
    pub frequency: f64,
    /// the storage modulus in Pa, above 0
    pub storage: f64,
    /// the loss modulus in Pa, at least 0
    pub loss: f64,
}

/// A material's dynamic modulus, stated at some frequencies
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicModulus {
    /// shear (G', G'') or Young's (E', E'')
    pub kind: ModulusKind,
    /// the stated rows, sorted by frequency
    pub rows: Vec<DynamicRow>,
    /// how the rows were obtained, when the law says
    pub basis: Option<Basis>,
}

impl DynamicModulus {
    /// `(storage, loss)` in Pa at `frequency` Hz: a stated row exactly, between two rows
    /// interpolated on log-log axes (the logarithm of each modulus linear in the logarithm of the
    /// frequency; a loss of 0 at either end interpolates linearly instead), and `None` outside the
    /// stated range or for a frequency that is not finite and above 0
    #[must_use]
    pub fn at(&self, frequency: f64) -> Option<(f64, f64)> {
        if !(frequency.is_finite() && frequency > 0.0) {
            return None;
        }
        if let Some(r) = self
            .rows
            .iter()
            .find(|r| r.frequency.to_bits() == frequency.to_bits())
        {
            return Some((r.storage, r.loss));
        }
        let hi = self.rows.iter().position(|r| r.frequency > frequency)?;
        let lo = self.rows.get(hi.checked_sub(1)?)?;
        let hi = &self.rows[hi];
        let ln = alice_det_math::ln64;
        let t = (ln(frequency) - ln(lo.frequency)) / (ln(hi.frequency) - ln(lo.frequency));
        let between = |a: f64, b: f64| {
            if a > 0.0 && b > 0.0 {
                alice_det_math::exp64(ln(a) + t * (ln(b) - ln(a)))
            } else {
                a + t * (b - a)
            }
        };
        Some((between(lo.storage, hi.storage), between(lo.loss, hi.loss)))
    }
}

/// How the value of an elastic constant was obtained
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Basis {
    /// measured on the material (the law's `source` lines say where)
    Measured,
    /// chosen by the law, not measured
    Assumed,
}

impl Basis {
    const fn word(self) -> &'static str {
        match self {
            Self::Measured => "measured",
            Self::Assumed => "assumed, not measured",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RainSwitch {
    Fixed(bool),
    Named(String),
}

#[derive(Debug, Clone)]
struct Rain {
    switch: RainSwitch,
    rate: Line,
}

/// A world law as read
#[derive(Debug, Clone)]
pub struct WorldLaw {
    name: String,
    params: Vec<Param>,
    lets: Vec<Line>,
    /// sorted by name
    materials: Vec<Material>,
    rain: Option<Rain>,
    hand: Option<Hand>,
}

#[derive(Debug, Clone)]
struct Hand {
    youngs_modulus: Line,
    poisson_ratio: Line,
    youngs_modulus_basis: Option<Basis>,
    poisson_ratio_basis: Option<Basis>,
    youngs_modulus_spread: Option<(f64, f64)>,
}

/// One material under one set of parameter values
///
/// The optional fields are `None` when the law does not state them.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundMaterial {
    /// the material's name
    pub name: String,
    /// kg/m^3
    pub density: f64,
    /// the dynamic friction coefficient
    pub friction: f64,
    /// the static friction coefficient (the dynamic one when the law does not state it)
    pub static_friction: f64,
    /// restitution, in `[0, 1]`
    pub bounce: f64,
    /// cloth stretch compliance
    pub stretch_compliance: Option<f64>,
    /// cloth bending compliance
    pub bend_compliance: Option<f64>,
    /// deformable edge compliance
    pub edge_compliance: Option<f64>,
    /// deformable volume compliance
    pub volume_compliance: Option<f64>,
    /// deformable shape matching stiffness, in `[0, 1]`
    pub shape_stiffness: Option<f64>,
    /// velocity kept per frame, in `[0, 1]`
    pub damping: Option<f64>,
    /// the share of its sliding speed a cloth or deformable loses on touching an SDF, in `[0, 1]`
    /// (a loss per contact, not a friction coefficient)
    pub sdf_friction: Option<f64>,
    /// the shear modulus G in Pa, above 0
    pub shear_modulus: Option<f64>,
    /// Poisson's ratio, in `[-1, 0.5)`
    pub poisson_ratio: Option<f64>,
    /// how `shear_modulus` was obtained, when the law says
    pub shear_modulus_basis: Option<Basis>,
    /// how `poisson_ratio` was obtained, when the law says
    pub poisson_ratio_basis: Option<Basis>,
    /// the pull in N at which a hand's hold on the material lets go, at least 0
    pub adhesion: Option<f64>,
    /// the area in m^2 a hand's hold touches, above 0
    pub contact_area: Option<f64>,
    /// how `adhesion` was obtained, when the law says
    pub adhesion_basis: Option<Basis>,
    /// how `contact_area` was obtained, when the law says
    pub contact_area_basis: Option<Basis>,
    /// the dynamic-modulus table, when the law states one
    pub dynamic: Option<DynamicModulus>,
}

impl BoundMaterial {
    /// Young's modulus `E = 2 G (1 + ν)` in Pa, derived from the two stated constants (never
    /// stated by a law, so it is only as good as the less certain of them); `None` unless both are
    #[must_use]
    pub fn youngs_modulus(&self) -> Option<f64> {
        Some(2.0 * self.shear_modulus? * (1.0 + self.poisson_ratio?))
    }
}

/// The skin of the hand that holds things, under one set of parameter values
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundHand {
    /// Young's modulus of the skin in Pa, above 0
    pub youngs_modulus: f64,
    /// Poisson's ratio of the skin, in `[-1, 0.5)`
    pub poisson_ratio: f64,
    /// how `youngs_modulus` was obtained, when the law says
    pub youngs_modulus_basis: Option<Basis>,
    /// how `poisson_ratio` was obtained, when the law says
    pub poisson_ratio_basis: Option<Basis>,
    /// the range `(lo, hi)` in Pa the reported skin moduli span, when the law states it
    pub youngs_modulus_spread: Option<(f64, f64)>,
}

/// A hand's hold on a material: a spring to the hand that lets go past `adhesion`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grip {
    /// the contact radius `sqrt(A / pi)` in m
    pub contact_radius: f64,
    /// the material's flat-contact stiffness `2 a E / (1 - nu^2)` in N/m
    pub material_stiffness: f64,
    /// the skin's flat-contact stiffness in N/m
    pub hand_stiffness: f64,
    /// the two in series, in N/m
    pub stiffness: f64,
    /// the pull in N at which the hold lets go, when the law states it
    pub adhesion: Option<f64>,
    /// `stiffness` with the skin's modulus at the two ends of its stated spread (it rises with the
    /// modulus), when the law states a spread
    pub stiffness_spread: Option<(f64, f64)>,
}

/// The normal stiffness `2 a E / (1 - nu^2)` of a flat circular contact of radius `a` on an elastic
/// half-space (Boussinesq)
// no fused multiply-add: the bits must be the same on every platform
#[allow(clippy::suboptimal_flops)]
fn flat_contact_stiffness(a: f64, youngs_modulus: f64, poisson_ratio: f64) -> f64 {
    2.0 * a * youngs_modulus / (1.0 - poisson_ratio * poisson_ratio)
}

/// Rain under one set of parameter values
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundRain {
    /// whether it rains
    pub on: bool,
    /// the strength stated by the law in mm/h, whether or not it rains now
    pub rate_mm_per_hour: f64,
}

impl BoundRain {
    /// Water falling on a square metre each second, in kg (a millimetre on a square metre is a
    /// litre, a kilogram of water); 0 when it does not rain
    #[must_use]
    pub fn mass_flux(&self) -> f64 {
        if self.on {
            self.rate_mm_per_hour / 3600.0
        } else {
            0.0
        }
    }

    /// Particles of `particle_mass` kg falling on a square metre each second, or `None` unless
    /// the mass is positive and finite
    #[must_use]
    pub fn particle_rate(&self, particle_mass: f64) -> Option<f64> {
        (particle_mass.is_finite() && particle_mass > 0.0).then(|| self.mass_flux() / particle_mass)
    }
}

/// A world law with parameter values
#[derive(Debug, Clone, PartialEq)]
pub struct BoundWorld {
    name: String,
    materials: Vec<BoundMaterial>,
    rain: BoundRain,
    states_rain: bool,
    hand: Option<BoundHand>,
}

const fn field_range(slot: usize) -> &'static str {
    match slot {
        DENSITY | SHEAR_MODULUS | CONTACT_AREA => "above 0 and at most 2^53",
        POISSON_RATIO => {
            "in -1..0.5 (0.5 itself is an incompressible solid, not a ratio a solid has)"
        }
        BOUNCE | SHAPE_STIFFNESS | DAMPING | SDF_FRICTION => "in 0..=1",
        _ => "in 0..=2^53",
    }
}

fn field_in_range(slot: usize, v: f64) -> bool {
    match slot {
        DENSITY | SHEAR_MODULUS | CONTACT_AREA => v > 0.0 && v <= MAX_VALUE,
        POISSON_RATIO => (-1.0..0.5).contains(&v),
        BOUNCE | SHAPE_STIFFNESS | DAMPING | SDF_FRICTION => (0.0..=1.0).contains(&v),
        _ => (0.0..=MAX_VALUE).contains(&v),
    }
}

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

fn parse_expr(line: usize, text: &str) -> Result<LawExpr, WorldLawError> {
    LawExpr::parse(text).map_err(|e| err(line, format!("`{}`: {e}", text.trim())))
}

fn on_off(s: &str) -> Option<bool> {
    match s {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

/// The text after `=` in `<left> = <right>`
fn after_equals(rest: &str) -> Option<&str> {
    rest.strip_prefix('=').map(str::trim)
}

impl WorldLaw {
    /// Reads a world law
    ///
    /// # Errors
    ///
    /// [`WorldLawError`] with the line, for anything but the form above, and when the defaults
    /// do not bind
    pub fn from_file(text: &str) -> Result<Self, WorldLawError> {
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() > MAX_LINES {
            return Err(err(0, format!("more than {MAX_LINES} lines")));
        }
        let mut name = None;
        let mut kind = false;
        let mut law = Self {
            name: String::new(),
            params: Vec::new(),
            lets: Vec::new(),
            materials: Vec::new(),
            rain: None,
            hand: None,
        };
        let mut hand = HandLines::default();
        let mut rain_switch: Option<(usize, RainSwitch)> = None;
        let mut rain_rate: Option<Line> = None;
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
                if t != "kind world" {
                    return Err(err(line, "the second line is `kind world`"));
                }
                kind = true;
                continue;
            }
            match word {
                "claim" | "source" => {}
                "param" => law.read_param(line, rest)?,
                "let" => law.read_let(line, rest)?,
                "material" => law.read_material(line, rest)?,
                "rain" => {
                    if rain_switch.is_some() {
                        return Err(err(line, "a second `rain` line"));
                    }
                    let Some(v) = after_equals(rest) else {
                        return Err(err(line, "`rain = on|off|<switch>` expected"));
                    };
                    let switch = if let Some(b) = on_off(v) {
                        RainSwitch::Fixed(b)
                    } else if law
                        .params
                        .iter()
                        .any(|p| p.name == v && matches!(p.kind, ParamKind::Switch { .. }))
                    {
                        RainSwitch::Named(v.to_owned())
                    } else {
                        return Err(err(line, format!("`{v}` is not on, off or a switch")));
                    };
                    rain_switch = Some((line, switch));
                }
                "rain_rate" => {
                    if rain_rate.is_some() {
                        return Err(err(line, "a second `rain_rate` line"));
                    }
                    let Some(v) = after_equals(rest) else {
                        return Err(err(line, "`rain_rate = <expression>` expected"));
                    };
                    rain_rate = Some(law.line(line, "rain_rate", v)?);
                }
                "hand_youngs_modulus" | "hand_poisson_ratio" => {
                    hand.read(&law, line, word, rest)?;
                }
                _ => return Err(err(line, format!("unknown line `{word}`"))),
            }
        }
        law.name = name.ok_or_else(|| err(0, "empty (no `law <name>` line)"))?;
        if !kind {
            return Err(err(0, "not a world law (no `kind world`)"));
        }
        law.hand = hand.finish()?;
        law.finish(rain_switch, rain_rate)?;
        law.bind(&[])
            .map_err(|e| err(0, format!("with the default parameters: {e}")))?;
        Ok(law)
    }

    /// Pairs the rain lines, sorts the materials and their fields, and checks the required fields
    fn finish(
        &mut self,
        rain_switch: Option<(usize, RainSwitch)>,
        rain_rate: Option<Line>,
    ) -> Result<(), WorldLawError> {
        let law = self;
        law.rain = match (rain_switch, rain_rate) {
            (Some((_, switch)), Some(rate)) => Some(Rain { switch, rate }),
            (None, None) => None,
            (Some((at, _)), None) => return Err(err(at, "`rain` without `rain_rate`")),
            (None, Some(r)) => return Err(err(r.at, "`rain_rate` without `rain`")),
        };
        if law.materials.is_empty() && law.rain.is_none() {
            return Err(err(0, "states no material and no rain"));
        }
        law.materials.sort_by(|a, b| a.name.cmp(&b.name));
        for m in &mut law.materials {
            m.fields.sort_by_key(|(s, _)| *s);
            for required in [DENSITY, FRICTION, BOUNCE] {
                if !m.fields.iter().any(|(s, _)| *s == required) {
                    return Err(err(
                        0,
                        format!("material `{}` has no `{}`", m.name, FIELDS[required]),
                    ));
                }
            }
        }
        Ok(())
    }

    fn declared(&self, n: &str) -> bool {
        n == "pi"
            || self.params.iter().any(|p| p.name == n)
            || self.lets.iter().any(|l| l.name == n)
    }

    fn read_param(&mut self, line: usize, rest: &str) -> Result<(), WorldLawError> {
        let w: Vec<&str> = rest.split_whitespace().collect();
        let (n, kind) = match w[..] {
            [n, "number", "=", d, "range", lo, hi] => {
                let (Some(default), Some(lo), Some(hi)) = (number(d), number(lo), number(hi))
                else {
                    return Err(err(line, "a parameter's numbers are plain decimals"));
                };
                if !(lo <= default && default <= hi) {
                    return Err(err(line, "the default is outside the range"));
                }
                (n, ParamKind::Number { default, lo, hi })
            }
            [n, "switch", "=", v] => {
                let Some(default) = on_off(v) else {
                    return Err(err(line, "a switch is on or off"));
                };
                (n, ParamKind::Switch { default })
            }
            _ => {
                return Err(err(
                    line,
                    "`param <name> number = <default> range <lo> <hi>` or `param <name> switch = on|off` expected",
                ))
            }
        };
        if !is_name(n) || self.declared(n) {
            return Err(err(line, format!("`{n}` is reserved or declared twice")));
        }
        if self.params.len() == MAX_PARAMS {
            return Err(err(line, format!("more than {MAX_PARAMS} parameters")));
        }
        self.params.push(Param {
            name: n.to_owned(),
            kind,
        });
        Ok(())
    }

    /// The names an expression may read, in the order [`Self::slots`] holds their values
    fn names(&self) -> Vec<&str> {
        let mut names = vec!["pi"];
        names.extend(
            self.params
                .iter()
                .filter(|p| matches!(p.kind, ParamKind::Number { .. }))
                .map(|p| p.name.as_str()),
        );
        names.extend(self.lets.iter().map(|l| l.name.as_str()));
        names
    }

    fn line(&self, at: usize, name: &str, text: &str) -> Result<Line, WorldLawError> {
        let expr = parse_expr(at, text)?;
        let code = expr
            .compile_named(&self.names())
            .map_err(|e| err(at, e.to_string()))?;
        Ok(Line {
            at,
            name: name.to_owned(),
            expr,
            code,
        })
    }

    fn read_let(&mut self, line: usize, rest: &str) -> Result<(), WorldLawError> {
        let Some((n, e)) = rest.split_once('=') else {
            return Err(err(line, "`let <name> = <expression>` expected"));
        };
        let n = n.trim();
        if !is_name(n) || self.declared(n) {
            return Err(err(line, format!("`{n}` is reserved or declared twice")));
        }
        if self.lets.len() == MAX_LETS {
            return Err(err(line, format!("more than {MAX_LETS} `let` lines")));
        }
        let l = self.line(line, n, e)?;
        self.lets.push(l);
        Ok(())
    }

    fn read_material(&mut self, line: usize, rest: &str) -> Result<(), WorldLawError> {
        let bad = || {
            err(
                line,
                "`material <name> <field> = <expression>` or `material <name> <field> basis measured|assumed` expected",
            )
        };
        if rest.split_whitespace().nth(1) == Some("dynamic") {
            return self.read_dynamic(line, rest);
        }
        if !rest.contains('=') {
            return self.read_basis(line, rest).map_err(|e| {
                if e.message.is_empty() {
                    bad()
                } else {
                    e
                }
            });
        }
        let (head, e) = rest.split_once('=').ok_or_else(bad)?;
        let w: Vec<&str> = head.split_whitespace().collect();
        let [m, field] = w[..] else {
            return Err(bad());
        };
        if !is_name(m) {
            return Err(err(line, format!("`{m}` is not a name")));
        }
        let Some(slot) = FIELDS.iter().position(|f| *f == field) else {
            return Err(err(
                line,
                format!("`{field}` is not a field ({})", FIELDS.join(", ")),
            ));
        };
        let l = self.line(line, &format!("{m} {field}"), e)?;
        let at = if let Some(at) = self.materials.iter().position(|x| x.name == m) {
            at
        } else {
            if self.materials.len() == MAX_MATERIALS {
                return Err(err(line, format!("more than {MAX_MATERIALS} materials")));
            }
            self.materials.push(Material {
                name: m.to_owned(),
                fields: Vec::new(),
                bases: Vec::new(),
                dynamic: None,
            });
            self.materials.len() - 1
        };
        let fields = &mut self.materials[at].fields;
        if fields.iter().any(|(s, _)| *s == slot) {
            return Err(err(line, format!("`{m} {field}` is given twice")));
        }
        fields.push((slot, l));
        Ok(())
    }

    /// `material <name> dynamic <kind> at <f> storage <expr> loss <expr>` or
    /// `material <name> dynamic basis measured|assumed`
    fn read_dynamic(&mut self, line: usize, rest: &str) -> Result<(), WorldLawError> {
        let form = "`material <name> dynamic shear|youngs at <f> storage <expression> loss <expression>` expected";
        let mut w = rest.splitn(3, ' ');
        let (Some(m), Some(_), Some(tail)) = (w.next(), w.next(), w.next()) else {
            return Err(err(line, form));
        };
        if !is_name(m) {
            return Err(err(line, format!("`{m}` is not a name")));
        }
        let tail = tail.trim();
        if let Some(word) = tail.strip_prefix("basis ") {
            let basis = basis_word(line, word.trim())?;
            let Some(table) = self
                .materials
                .iter_mut()
                .find(|x| x.name == m)
                .and_then(|x| x.dynamic.as_mut())
            else {
                return Err(err(
                    line,
                    format!("`{m}` has no dynamic row before its basis"),
                ));
            };
            if table.basis.is_some() {
                return Err(err(
                    line,
                    format!("the basis of `{m} dynamic` is given twice"),
                ));
            }
            table.basis = Some(basis);
            return Ok(());
        }
        let (kind, after) = if let Some(a) = tail.strip_prefix("shear at ") {
            (ModulusKind::Shear, a)
        } else if let Some(a) = tail.strip_prefix("youngs at ") {
            (ModulusKind::Youngs, a)
        } else {
            return Err(err(line, form));
        };
        let Some((f, after)) = after.split_once(' ') else {
            return Err(err(line, form));
        };
        let Some(f) = number(f).filter(|f| *f > 0.0 && *f <= MAX_VALUE) else {
            return Err(err(line, "a frequency is a plain decimal above 0"));
        };
        let Some((storage, loss)) = after
            .trim()
            .strip_prefix("storage ")
            .and_then(|s| s.split_once(" loss "))
        else {
            return Err(err(line, form));
        };
        let storage = self.line(line, &format!("{m} dynamic storage at {f}"), storage)?;
        let loss = self.line(line, &format!("{m} dynamic loss at {f}"), loss)?;
        let at = if let Some(at) = self.materials.iter().position(|x| x.name == m) {
            at
        } else {
            if self.materials.len() == MAX_MATERIALS {
                return Err(err(line, format!("more than {MAX_MATERIALS} materials")));
            }
            self.materials.push(Material {
                name: m.to_owned(),
                fields: Vec::new(),
                bases: Vec::new(),
                dynamic: None,
            });
            self.materials.len() - 1
        };
        let table = self.materials[at].dynamic.get_or_insert(DynamicLines {
            kind,
            rows: Vec::new(),
            basis: None,
        });
        if table.kind != kind {
            return Err(err(
                line,
                format!(
                    "`{m} dynamic` is {} already; one kind per material",
                    table.kind.word()
                ),
            ));
        }
        if table.basis.is_some() {
            return Err(err(line, format!("a row of `{m} dynamic` after its basis")));
        }
        if table
            .rows
            .iter()
            .any(|(g, _, _)| g.to_bits() == f.to_bits())
        {
            return Err(err(line, format!("`{m} dynamic` at {f} Hz is given twice")));
        }
        table.rows.push((f, storage, loss));
        table.rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(())
    }

    /// `material <name> <field> basis measured|assumed`, for an elastic constant the law states
    fn read_basis(&mut self, line: usize, rest: &str) -> Result<(), WorldLawError> {
        let w: Vec<&str> = rest.split_whitespace().collect();
        let [m, field, "basis", word] = w[..] else {
            return Err(err(line, ""));
        };
        let basis = basis_word(line, word)?;
        let slot = match field {
            "shear_modulus" => SHEAR_MODULUS,
            "poisson_ratio" => POISSON_RATIO,
            "adhesion" => ADHESION,
            "contact_area" => CONTACT_AREA,
            _ => {
                return Err(err(
                    line,
                    format!(
                        "`{field}` takes no basis (only shear_modulus, poisson_ratio, adhesion and contact_area)"
                    ),
                ))
            }
        };
        let Some(mat) = self.materials.iter_mut().find(|x| x.name == m) else {
            return Err(err(
                line,
                format!("`{m}` has no field yet: state the value before its basis"),
            ));
        };
        if !mat.fields.iter().any(|(s, _)| *s == slot) {
            return Err(err(
                line,
                format!("`{m} {field}` is not stated before its basis"),
            ));
        }
        if mat.bases.iter().any(|(s, _)| *s == slot) {
            return Err(err(
                line,
                format!("the basis of `{m} {field}` is given twice"),
            ));
        }
        mat.bases.push((slot, basis));
        Ok(())
    }

    /// The law's name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The identifier of the law: its kind, [`WORLD_SEMANTICS_ID`], its name, every parameter
    /// and every expression as a tree (spelling, spacing and the order of the materials and
    /// fields do not count; notes and comments are not in it)
    #[must_use]
    pub fn law_id(&self) -> [u8; 32] {
        self.law_id_under(&WORLD_SEMANTICS_ID)
    }

    fn law_id_under(&self, semantics: &[u8; 32]) -> [u8; 32] {
        let mut h = LawIdHasher::new(WORLD_LAW_KIND, semantics)
            .str(&self.name)
            .u64(self.params.len() as u64);
        for p in &self.params {
            h = h.str(&p.name);
            h = match p.kind {
                ParamKind::Number { default, lo, hi } => h.u32(0).f64(default).f64(lo).f64(hi),
                ParamKind::Switch { default } => h.u32(1).u32(u32::from(default)),
            };
        }
        h = h.u64(self.lets.len() as u64);
        for l in &self.lets {
            h = l.expr.hash_into(h.str(&l.name));
        }
        h = h.u64(self.materials.len() as u64);
        for m in &self.materials {
            h = h.str(&m.name).u64(m.fields.len() as u64);
            for (slot, l) in &m.fields {
                h = l.expr.hash_into(h.str(FIELDS[*slot]));
            }
            h = h.u64(m.bases.len() as u64);
            for (slot, b) in &m.bases {
                h = h.str(FIELDS[*slot]).u32(u32::from(*b == Basis::Measured));
            }
            h = match &m.dynamic {
                None => h.u32(0),
                Some(d) => {
                    let mut h = h
                        .u32(1)
                        .u32(u32::from(d.kind == ModulusKind::Youngs))
                        .u64(d.rows.len() as u64);
                    for (f, storage, loss) in &d.rows {
                        h = loss.expr.hash_into(storage.expr.hash_into(h.f64(*f)));
                    }
                    match d.basis {
                        None => h.u32(0),
                        Some(b) => h.u32(1).u32(u32::from(b == Basis::Measured)),
                    }
                }
            };
        }
        h = match &self.rain {
            None => h.u32(0),
            Some(r) => {
                let h = match &r.switch {
                    RainSwitch::Fixed(b) => h.u32(1).u32(u32::from(*b)),
                    RainSwitch::Named(n) => h.u32(2).str(n),
                };
                r.rate.expr.hash_into(h)
            }
        };
        h = match &self.hand {
            None => h.u32(0),
            Some(hand) => {
                let mut h = hand.youngs_modulus.expr.hash_into(h.u32(1));
                h = hand.poisson_ratio.expr.hash_into(h);
                for b in [hand.youngs_modulus_basis, hand.poisson_ratio_basis] {
                    h = match b {
                        None => h.u32(0),
                        Some(b) => h.u32(1).u32(u32::from(b == Basis::Measured)),
                    };
                }
                match hand.youngs_modulus_spread {
                    None => h.u32(0),
                    Some((lo, hi)) => h.u32(1).f64(lo).f64(hi),
                }
            }
        };
        h.finish()
    }

    /// The law with parameter values: each named one must be declared, given once, of its type
    /// and in its range; the others take their defaults
    ///
    /// # Errors
    ///
    /// [`BindError`]: the parameter at fault, or the line whose value is not finite or outside
    /// its field's range
    pub fn bind(&self, values: &[(&str, WorldValue)]) -> Result<BoundWorld, BindError> {
        let mut numbers: Vec<f64> = Vec::new();
        let mut switches: Vec<(&str, bool)> = Vec::new();
        for (j, (n, _)) in values.iter().enumerate() {
            if !self.params.iter().any(|p| p.name == *n) {
                return Err(BindError::UnknownParam((*n).to_owned()));
            }
            if values[..j].iter().any(|(m, _)| m == n) {
                return Err(BindError::RepeatedParam((*n).to_owned()));
            }
        }
        for p in &self.params {
            let given = values.iter().find(|(n, _)| *n == p.name).map(|(_, v)| *v);
            match (&p.kind, given) {
                (ParamKind::Number { default, .. }, None) => numbers.push(*default),
                (ParamKind::Number { lo, hi, .. }, Some(WorldValue::Number(v))) => {
                    if !(v.is_finite() && *lo <= v && v <= *hi) {
                        return Err(BindError::ParamOutOfRange(p.name.clone()));
                    }
                    numbers.push(v);
                }
                (ParamKind::Switch { default }, None) => switches.push((&p.name, *default)),
                (ParamKind::Switch { .. }, Some(WorldValue::Switch(b))) => {
                    switches.push((&p.name, b));
                }
                _ => return Err(BindError::WrongType(p.name.clone())),
            }
        }
        let mut slots = vec![std::f64::consts::PI];
        slots.extend_from_slice(&numbers);
        for l in &self.lets {
            let v = eval(l, &slots)?;
            slots.push(v);
        }
        let mut materials = Vec::with_capacity(self.materials.len());
        for m in &self.materials {
            materials.push(bind_material(m, &slots)?);
        }
        let (rain, states_rain) = match &self.rain {
            None => (
                BoundRain {
                    on: false,
                    rate_mm_per_hour: 0.0,
                },
                false,
            ),
            Some(r) => {
                let rate = eval(&r.rate, &slots)?;
                if !(0.0..=MAX_VALUE).contains(&rate) {
                    return Err(BindError::OutOfRange {
                        line: r.rate.at,
                        name: r.rate.name.clone(),
                        range: "in 0..=2^53",
                    });
                }
                let on = match &r.switch {
                    RainSwitch::Fixed(b) => *b,
                    RainSwitch::Named(n) => switches
                        .iter()
                        .find(|(s, _)| s == n)
                        .is_some_and(|(_, b)| *b),
                };
                (
                    BoundRain {
                        on,
                        rate_mm_per_hour: rate + 0.0,
                    },
                    true,
                )
            }
        };
        let hand = match &self.hand {
            None => None,
            Some(hand) => Some(bind_hand(hand, &slots)?),
        };
        Ok(BoundWorld {
            name: self.name.clone(),
            materials,
            rain,
            states_rain,
            hand,
        })
    }
}

fn eval(l: &Line, slots: &[f64]) -> Result<f64, BindError> {
    l.code
        .eval(slots)
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| BindError::NonFinite {
            line: l.at,
            name: l.name.clone(),
        })
}

fn bind_material(m: &Material, slots: &[f64]) -> Result<BoundMaterial, BindError> {
    let mut v = [None; FIELDS.len()];
    for (slot, l) in &m.fields {
        let x = eval(l, slots)?;
        if !field_in_range(*slot, x) {
            return Err(BindError::OutOfRange {
                line: l.at,
                name: l.name.clone(),
                range: field_range(*slot),
            });
        }
        // -0 and +0 are the same value here
        v[*slot] = Some(x + 0.0);
    }
    // read refuses a material without these
    let required = |slot: usize| v[slot].unwrap_or(0.0);
    Ok(BoundMaterial {
        name: m.name.clone(),
        density: required(DENSITY),
        friction: required(FRICTION),
        static_friction: v[STATIC_FRICTION].unwrap_or_else(|| required(FRICTION)),
        bounce: required(BOUNCE),
        stretch_compliance: v[STRETCH_COMPLIANCE],
        bend_compliance: v[BEND_COMPLIANCE],
        edge_compliance: v[EDGE_COMPLIANCE],
        volume_compliance: v[VOLUME_COMPLIANCE],
        shape_stiffness: v[SHAPE_STIFFNESS],
        damping: v[DAMPING],
        sdf_friction: v[SDF_FRICTION],
        shear_modulus: v[SHEAR_MODULUS],
        poisson_ratio: v[POISSON_RATIO],
        shear_modulus_basis: basis_of(m, SHEAR_MODULUS),
        poisson_ratio_basis: basis_of(m, POISSON_RATIO),
        adhesion: v[ADHESION],
        contact_area: v[CONTACT_AREA],
        adhesion_basis: basis_of(m, ADHESION),
        contact_area_basis: basis_of(m, CONTACT_AREA),
        dynamic: match &m.dynamic {
            None => None,
            Some(d) => Some(bind_dynamic(d, slots)?),
        },
    })
}

fn bind_dynamic(d: &DynamicLines, slots: &[f64]) -> Result<DynamicModulus, BindError> {
    let mut rows = Vec::with_capacity(d.rows.len());
    for (frequency, storage, loss) in &d.rows {
        let s = eval(storage, slots)?;
        if !(s > 0.0 && s <= MAX_VALUE) {
            return Err(BindError::OutOfRange {
                line: storage.at,
                name: storage.name.clone(),
                range: "above 0 and at most 2^53",
            });
        }
        let l = eval(loss, slots)?;
        if !(0.0..=MAX_VALUE).contains(&l) {
            return Err(BindError::OutOfRange {
                line: loss.at,
                name: loss.name.clone(),
                range: "in 0..=2^53",
            });
        }
        rows.push(DynamicRow {
            frequency: *frequency,
            storage: s,
            loss: l + 0.0,
        });
    }
    Ok(DynamicModulus {
        kind: d.kind,
        rows,
        basis: d.basis,
    })
}

fn bind_hand(hand: &Hand, slots: &[f64]) -> Result<BoundHand, BindError> {
    let e = eval(&hand.youngs_modulus, slots)?;
    if !field_in_range(SHEAR_MODULUS, e) {
        return Err(BindError::OutOfRange {
            line: hand.youngs_modulus.at,
            name: hand.youngs_modulus.name.clone(),
            range: field_range(SHEAR_MODULUS),
        });
    }
    let nu = eval(&hand.poisson_ratio, slots)?;
    if !field_in_range(POISSON_RATIO, nu) {
        return Err(BindError::OutOfRange {
            line: hand.poisson_ratio.at,
            name: hand.poisson_ratio.name.clone(),
            range: field_range(POISSON_RATIO),
        });
    }
    if let Some((lo, hi)) = hand.youngs_modulus_spread {
        if !(lo <= e && e <= hi) {
            return Err(BindError::OutOfRange {
                line: hand.youngs_modulus.at,
                name: hand.youngs_modulus.name.clone(),
                range: "inside its stated spread",
            });
        }
    }
    Ok(BoundHand {
        youngs_modulus: e,
        poisson_ratio: nu + 0.0,
        youngs_modulus_basis: hand.youngs_modulus_basis,
        poisson_ratio_basis: hand.poisson_ratio_basis,
        youngs_modulus_spread: hand.youngs_modulus_spread,
    })
}

fn basis_word(line: usize, word: &str) -> Result<Basis, WorldLawError> {
    match word {
        "measured" => Ok(Basis::Measured),
        "assumed" => Ok(Basis::Assumed),
        _ => Err(err(line, "a basis is measured or assumed")),
    }
}

/// The hand lines as read, before they are paired
#[derive(Default)]
struct HandLines {
    youngs_modulus: Option<Line>,
    poisson_ratio: Option<Line>,
    youngs_modulus_basis: Option<Basis>,
    poisson_ratio_basis: Option<Basis>,
    youngs_modulus_spread: Option<(f64, f64)>,
}

impl HandLines {
    /// `hand_youngs_modulus = <expr>`, `hand_poisson_ratio = <expr>` or `<key> basis <word>`
    fn read(
        &mut self,
        law: &WorldLaw,
        line: usize,
        key: &str,
        rest: &str,
    ) -> Result<(), WorldLawError> {
        let is_e = key == "hand_youngs_modulus";
        if let Some(v) = after_equals(rest) {
            let slot = if is_e {
                &mut self.youngs_modulus
            } else {
                &mut self.poisson_ratio
            };
            if slot.is_some() {
                return Err(err(line, format!("a second `{key}` line")));
            }
            *slot = Some(law.line(line, key, v)?);
            return Ok(());
        }
        if let Some(range) = rest.strip_prefix("spread ") {
            return self.read_spread(line, key, range);
        }
        let Some(word) = rest.strip_prefix("basis ") else {
            return Err(err(
                line,
                format!("`{key} = <expression>` or `{key} basis measured|assumed` expected"),
            ));
        };
        let basis = basis_word(line, word.trim())?;
        let (stated, slot) = if is_e {
            (
                self.youngs_modulus.is_some(),
                &mut self.youngs_modulus_basis,
            )
        } else {
            (self.poisson_ratio.is_some(), &mut self.poisson_ratio_basis)
        };
        if !stated {
            return Err(err(line, format!("`{key}` is not stated before its basis")));
        }
        if slot.is_some() {
            return Err(err(line, format!("the basis of `{key}` is given twice")));
        }
        *slot = Some(basis);
        Ok(())
    }

    /// `hand_youngs_modulus spread <lo> <hi>`, after the value
    fn read_spread(&mut self, line: usize, key: &str, range: &str) -> Result<(), WorldLawError> {
        if key != "hand_youngs_modulus" {
            return Err(err(
                line,
                format!("`{key}` takes no spread (only hand_youngs_modulus)"),
            ));
        }
        if self.youngs_modulus.is_none() {
            return Err(err(
                line,
                format!("`{key}` is not stated before its spread"),
            ));
        }
        if self.youngs_modulus_spread.is_some() {
            return Err(err(line, format!("the spread of `{key}` is given twice")));
        }
        let w: Vec<&str> = range.split_whitespace().collect();
        let [lo, hi] = w[..] else {
            return Err(err(line, format!("`{key} spread <lo> <hi>` expected")));
        };
        let (Some(lo), Some(hi)) = (number(lo), number(hi)) else {
            return Err(err(line, "a spread's numbers are plain decimals"));
        };
        if !(lo > 0.0 && lo <= hi && hi <= MAX_VALUE) {
            return Err(err(line, "a spread is 0 < lo <= hi <= 2^53"));
        }
        self.youngs_modulus_spread = Some((lo, hi));
        Ok(())
    }

    fn finish(self) -> Result<Option<Hand>, WorldLawError> {
        match (self.youngs_modulus, self.poisson_ratio) {
            (Some(youngs_modulus), Some(poisson_ratio)) => Ok(Some(Hand {
                youngs_modulus,
                poisson_ratio,
                youngs_modulus_basis: self.youngs_modulus_basis,
                poisson_ratio_basis: self.poisson_ratio_basis,
                youngs_modulus_spread: self.youngs_modulus_spread,
            })),
            (None, None) => Ok(None),
            (Some(e), None) => Err(err(
                e.at,
                "`hand_youngs_modulus` without `hand_poisson_ratio`",
            )),
            (None, Some(nu)) => Err(err(
                nu.at,
                "`hand_poisson_ratio` without `hand_youngs_modulus`",
            )),
        }
    }
}

fn basis_of(m: &Material, slot: usize) -> Option<Basis> {
    m.bases.iter().find(|(s, _)| *s == slot).map(|(_, b)| *b)
}

impl BoundWorld {
    /// The materials, sorted by name
    #[must_use]
    pub fn materials(&self) -> &[BoundMaterial] {
        &self.materials
    }

    /// One material by name
    #[must_use]
    pub fn material(&self, name: &str) -> Option<&BoundMaterial> {
        self.materials.iter().find(|m| m.name == name)
    }

    /// The rain
    #[must_use]
    pub const fn rain(&self) -> BoundRain {
        self.rain
    }

    /// The skin of the hand, when the law states it
    #[must_use]
    pub const fn hand(&self) -> Option<BoundHand> {
        self.hand
    }

    /// A hand's hold on a material, derived from the law (module docs, Grip); `None` unless the
    /// material states `shear_modulus`, `poisson_ratio` and `contact_area` and the law states the
    /// hand
    #[must_use]
    pub fn grip(&self, material: &str) -> Option<Grip> {
        grip_of(self.material(material)?, self.hand?)
    }

    /// What this world does, in plain text, computed from its values
    #[must_use]
    pub fn describe(&self) -> String {
        let mut s = String::new();
        // writing to a String cannot fail
        let _ = writeln!(s, "law: {}", self.name);
        for m in &self.materials {
            describe_material(&mut s, m, self.hand);
        }
        match self.hand {
            Some(h) => {
                let basis = |b: Option<Basis>| b.map_or("basis not stated", Basis::word);
                let spread = h.youngs_modulus_spread.map_or(String::new(), |(lo, hi)| {
                    format!(", reported values spread over {lo:.4e}..{hi:.4e} Pa")
                });
                let _ = writeln!(
                    s,
                    "hand: skin Young's modulus {:.4e} Pa ({}{spread}), Poisson's ratio {:.3} ({})",
                    h.youngs_modulus,
                    basis(h.youngs_modulus_basis),
                    h.poisson_ratio,
                    basis(h.poisson_ratio_basis)
                );
            }
            None => {
                let _ = writeln!(s, "hand: not stated");
            }
        }
        if self.states_rain {
            let r = self.rain;
            if r.on {
                let _ = writeln!(
                    s,
                    "rain: on, {:.3} mm an hour, {:.6} kg of water on each square metre each second",
                    r.rate_mm_per_hour,
                    r.mass_flux()
                );
            } else {
                let _ = writeln!(s, "rain: off");
            }
        } else {
            let _ = writeln!(s, "rain: not stated");
        }
        s
    }
}

fn grip_of(m: &BoundMaterial, hand: BoundHand) -> Option<Grip> {
    let a = (m.contact_area? / std::f64::consts::PI).sqrt();
    let material_stiffness = flat_contact_stiffness(a, m.youngs_modulus()?, m.poisson_ratio?);
    let series = |hand_e: f64| {
        let k_hand = flat_contact_stiffness(a, hand_e, hand.poisson_ratio);
        (
            k_hand,
            material_stiffness * k_hand / (material_stiffness + k_hand),
        )
    };
    let (hand_stiffness, stiffness) = series(hand.youngs_modulus);
    Some(Grip {
        contact_radius: a,
        material_stiffness,
        hand_stiffness,
        stiffness,
        adhesion: m.adhesion,
        stiffness_spread: hand
            .youngs_modulus_spread
            .map(|(lo, hi)| (series(lo).1, series(hi).1)),
    })
}

fn describe_grip(s: &mut String, m: &BoundMaterial, hand: Option<BoundHand>) {
    if m.adhesion.is_none() && m.contact_area.is_none() {
        return;
    }
    let basis = |b: Option<Basis>| b.map_or("basis not stated", Basis::word);
    match m.adhesion {
        Some(f) => {
            let _ = writeln!(
                s,
                "  adhesion {f:.4e} N ({}): the pull at which a hand's hold lets go",
                basis(m.adhesion_basis)
            );
        }
        None => {
            let _ = writeln!(s, "  adhesion: not stated");
        }
    }
    match m.contact_area {
        Some(area) => {
            let _ = writeln!(
                s,
                "  contact area {area:.4e} m^2 ({})",
                basis(m.contact_area_basis)
            );
        }
        None => {
            let _ = writeln!(s, "  contact area: not stated");
        }
    }
    match hand.and_then(|h| grip_of(m, h)) {
        Some(g) => {
            let _ = writeln!(
                s,
                "  grip stiffness {:.4e} N/m, derived: the material ({:.4e} N/m) and the hand ({:.4e} N/m) in series, each 2 a E / (1 - Poisson's ratio^2) for a flat contact of radius a = {:.4e} m on an elastic half-space (Boussinesq), valid while a is much smaller than the body's thickness",
                g.stiffness, g.material_stiffness, g.hand_stiffness, g.contact_radius
            );
            if let (Some((lo, hi)), Some((k_lo, k_hi))) = (
                hand.and_then(|h| h.youngs_modulus_spread),
                g.stiffness_spread,
            ) {
                let _ = writeln!(
                    s,
                    "  grip stiffness over the skin's stated spread {lo:.4e}..{hi:.4e} Pa: {k_lo:.4e}..{k_hi:.4e} N/m"
                );
            }
        }
        None => {
            let _ = writeln!(
                s,
                "  grip stiffness: not derived (needs the shear modulus, Poisson's ratio, contact area and the hand)"
            );
        }
    }
}

fn describe_material(s: &mut String, m: &BoundMaterial, hand: Option<BoundHand>) {
    let _ = writeln!(s, "material: {}", m.name);
    let water = if m.density > WATER_DENSITY {
        "sinks in water"
    } else if m.density < WATER_DENSITY {
        "floats on water"
    } else {
        "as dense as water"
    };
    let _ = writeln!(
        s,
        "  {:.3} kg in a cubic metre, {:.3} times water, {water}",
        m.density,
        m.density / WATER_DENSITY
    );
    let _ = writeln!(
        s,
        "  grip: sliding {:.3}, at rest {:.3}, the share of its weight it takes to move it",
        m.friction, m.static_friction
    );
    let _ = writeln!(
        s,
        "  bounce {:.3}: dropped from 1 m on the same material, it comes back up {:.3} m",
        m.bounce,
        m.bounce * m.bounce
    );
    let rows = [
        ("stretch compliance", m.stretch_compliance),
        ("bend compliance", m.bend_compliance),
        ("edge compliance", m.edge_compliance),
        ("volume compliance", m.volume_compliance),
        ("shape stiffness", m.shape_stiffness),
        ("velocity kept per frame", m.damping),
        ("sliding speed lost on touching an SDF", m.sdf_friction),
    ];
    for (what, v) in rows {
        if let Some(v) = v {
            let _ = writeln!(s, "  {what} {v:.6}");
        }
    }
    let basis = |b: Option<Basis>| b.map_or("basis not stated", Basis::word);
    match (m.shear_modulus, m.poisson_ratio) {
        (None, None) => {
            let _ = writeln!(s, "  elastic constants: not stated");
        }
        (g, nu) => {
            match g {
                Some(g) => {
                    let _ = writeln!(
                        s,
                        "  shear modulus {g:.4e} Pa ({})",
                        basis(m.shear_modulus_basis)
                    );
                }
                None => {
                    let _ = writeln!(s, "  shear modulus: not stated");
                }
            }
            match nu {
                Some(nu) => {
                    let _ = writeln!(
                        s,
                        "  Poisson's ratio {nu:.3} ({})",
                        basis(m.poisson_ratio_basis)
                    );
                }
                None => {
                    let _ = writeln!(s, "  Poisson's ratio: not stated");
                }
            }
            if let Some(e) = m.youngs_modulus() {
                let _ = writeln!(
                    s,
                    "  Young's modulus {e:.4e} Pa, derived as 2 G (1 + Poisson's ratio), not measured"
                );
            }
        }
    }
    if let Some(d) = &m.dynamic {
        describe_dynamic(s, d);
    }
    describe_grip(s, m, hand);
}

fn describe_dynamic(s: &mut String, d: &DynamicModulus) {
    let (lo, hi) = match (d.rows.first(), d.rows.last()) {
        (Some(a), Some(b)) => (a.frequency, b.frequency),
        _ => return,
    };
    let names = match d.kind {
        ModulusKind::Shear => "G' and G''",
        ModulusKind::Youngs => "E' and E''",
    };
    let basis = d.basis.map_or("basis not stated", Basis::word);
    if d.rows.len() == 1 {
        let _ = writeln!(
            s,
            "  dynamic {} modulus ({names}, {basis}) stated at one frequency, {lo:.4e} Hz; other frequencies not stated",
            d.kind.word()
        );
    } else {
        let _ = writeln!(
            s,
            "  dynamic {} modulus ({names}, {basis}) stated at {} frequencies {lo:.4e}..{hi:.4e} Hz; between them interpolated on log-log axes, outside them not stated",
            d.kind.word(),
            d.rows.len()
        );
    }
    for r in &d.rows {
        let _ = writeln!(
            s,
            "    {:.4e} Hz: storage {:.4e} Pa, loss {:.4e} Pa, loss tangent {:.4}",
            r.frequency,
            r.storage,
            r.loss,
            r.loss / r.storage
        );
    }
}

/// The values under the types of `alice-physics`
#[cfg(feature = "physics")]
mod physics {
    use alice_physics::cloth::ClothConfig;
    use alice_physics::deformable::DeformableConfig;
    use alice_physics::material::{MaterialId, PhysicsMaterial};
    use alice_physics::math::Fix128;
    use alice_physics::particle::ParticleEmitter;
    use alice_physics::sdf_sph::SphConfig;
    use alice_physics::solver::RigidBody;

    use super::{BoundMaterial, BoundRain};

    impl BoundMaterial {
        /// Density as the mass property functions of `alice-physics` take it
        #[must_use]
        pub fn density_fix(&self) -> Fix128 {
            Fix128::from_f64(self.density)
        }

        /// Friction and restitution as a material of `alice-physics`, under `id`
        #[must_use]
        pub fn physics_material(&self, id: MaterialId) -> PhysicsMaterial {
            PhysicsMaterial::new(
                id,
                Fix128::from_f64(self.friction),
                Fix128::from_f64(self.bounce),
            )
            .with_static_friction(Fix128::from_f64(self.static_friction))
        }

        /// Friction and restitution on one body (a body's own values, as a site or game sets them)
        pub fn apply_to_body(&self, body: &mut RigidBody) {
            body.friction = Fix128::from_f64(self.friction);
            body.restitution = Fix128::from_f64(self.bounce);
        }

        /// A cloth configuration with the fields this material states; the others stay
        #[must_use]
        pub fn apply_to_cloth(&self, mut config: ClothConfig) -> ClothConfig {
            if let Some(v) = self.stretch_compliance {
                config.stretch_compliance = Fix128::from_f64(v);
            }
            if let Some(v) = self.bend_compliance {
                config.bend_compliance = Fix128::from_f64(v);
            }
            if let Some(v) = self.damping {
                config.damping = Fix128::from_f64(v);
            }
            if let Some(v) = self.sdf_friction {
                config.sdf_friction = Fix128::from_f64(v);
            }
            config
        }

        /// A deformable configuration with the fields this material states; the others stay
        #[must_use]
        pub fn apply_to_deformable(&self, mut config: DeformableConfig) -> DeformableConfig {
            if let Some(v) = self.edge_compliance {
                config.edge_compliance = Fix128::from_f64(v);
            }
            if let Some(v) = self.volume_compliance {
                config.volume_compliance = Fix128::from_f64(v);
            }
            if let Some(v) = self.shape_stiffness {
                config.shape_stiffness = Fix128::from_f64(v);
            }
            if let Some(v) = self.damping {
                config.damping = Fix128::from_f64(v);
            }
            if let Some(v) = self.sdf_friction {
                config.sdf_friction = Fix128::from_f64(v);
            }
            config
        }
    }

    impl BoundRain {
        /// [`BoundRain::particle_rate`] for the particle mass of an SPH configuration
        #[must_use]
        pub fn particle_rate_for(&self, config: &SphConfig) -> Option<f64> {
            self.particle_rate(f64::from(config.particle_mass))
        }

        /// Sets how many particles an emitter gives a second, for the rain on `area` square
        /// metres at the emitter's own particle mass. `false`, with the emitter unchanged, unless
        /// the area and the mass are positive and the rate is finite.
        pub fn apply_to_emitter(&self, emitter: &mut ParticleEmitter, area: f64) -> bool {
            let rate = self
                .particle_rate(emitter.particle_mass.to_f64())
                .map(|per_square_metre| per_square_metre * area);
            match rate {
                Some(r) if area > 0.0 && r.is_finite() && r <= super::MAX_VALUE => {
                    emitter.emission_rate = Fix128::from_f64(r);
                    true
                }
                _ => false,
            }
        }
    }
}

/// The law the first world pin binds: every field, a switch, a `let` and the expression
/// functions, with values in and outside the ranges
pub const WORLD_PROBE_LAW: &str = "law world_probe
kind world
param k number = 0.5 range -2 2
param wet switch = on
let w = sin(pi * k) ^ 2
material a density = 1000 + 100 * k
material a friction = w
material a static_friction = w + 0.25
material a bounce = k
material a stretch_compliance = k / 3
material a bend_compliance = w / 7
material a edge_compliance = k * k
material a volume_compliance = exp(-k)
material a shape_stiffness = k
material a damping = 1 - k
material a sdf_friction = k * 0.5
material b density = 2500 - 1500 * k
material b friction = 0
material b bounce = 0
material b shear_modulus = 1000 + 1000 * k
material b shear_modulus basis measured
material b poisson_ratio = 0.25 * k
material b poisson_ratio basis assumed
material b adhesion = k
material b adhesion basis assumed
material b contact_area = 0.0001 + 0.0001 * k
material b dynamic youngs at 3 storage 1000 + 100 * k loss 50 * k
material b dynamic youngs at 30 storage 2000 loss 100
material b dynamic basis measured
hand_youngs_modulus = 300000
hand_youngs_modulus basis assumed
hand_youngs_modulus spread 40000 400000
hand_poisson_ratio = 0.45
rain = wet
rain_rate = 10 * k
";

/// The measured value of every pin of [`WORLD_SEMANTICS_PINS`]
///
/// # Panics
///
/// If [`WORLD_PROBE_LAW`] does not read (the oracle reads it first)
#[must_use]
pub fn world_semantics_fingerprints() -> Vec<(&'static str, [u8; 32])> {
    use sha2::{Digest as _, Sha256};
    let law = WorldLaw::from_file(WORLD_PROBE_LAW).expect("the probe law reads");
    let mut h = Sha256::new();
    for k in [-1.0, 0.0, 0.25, 0.5, 0.75, 1.0, 2.0] {
        for wet in [false, true] {
            let bound = law.bind(&[
                ("k", WorldValue::Number(k)),
                ("wet", WorldValue::Switch(wet)),
            ]);
            match bound {
                Ok(b) => {
                    h.update([1]);
                    h.update(b.describe().as_bytes());
                    for m in b.materials() {
                        h.update(m.density.to_bits().to_le_bytes());
                        h.update(m.friction.to_bits().to_le_bytes());
                        h.update(m.static_friction.to_bits().to_le_bytes());
                        h.update(m.bounce.to_bits().to_le_bytes());
                    }
                    h.update(b.rain().mass_flux().to_bits().to_le_bytes());
                }
                Err(e) => {
                    h.update([0]);
                    h.update(e.to_string().as_bytes());
                }
            }
        }
    }
    vec![
        (WORLD_SEMANTICS_PINS[0].0, h.finalize().into()),
        (
            WORLD_SEMANTICS_PINS[1].0,
            crate::research_law::expression_functions_fingerprint(),
        ),
    ]
}

/// The fold of the measured pins (what [`WORLD_SEMANTICS_ID`] must equal)
#[must_use]
pub fn measured_world_semantics_id() -> [u8; 32] {
    fold_semantics_pins(&world_semantics_fingerprints())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifier moves with the world semantics
    #[test]
    fn the_identifier_moves_with_the_world_semantics() {
        let law = WorldLaw::from_file(WORLD_PROBE_LAW).unwrap();
        assert_eq!(law.law_id(), law.law_id_under(&WORLD_SEMANTICS_ID));
        let mut other = WORLD_SEMANTICS_ID;
        other[0] ^= 1;
        assert_ne!(law.law_id_under(&other), law.law_id());
    }
}
