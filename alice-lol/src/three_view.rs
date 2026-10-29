//! Three-view measurement records (`three_view 1`) and their derivation into the avatar
//! proportion laws (`laws/avatar/body.law`, `laws/avatar/face.law`)
//!
//! A record is data: landmark positions picked on a front / side / back / face sheet, in the
//! sheet's pixels, each with its own ± or marked `not_measurable` with a reason. The picking is
//! done by a person, a model or a detector; the record keeps what was picked, and the
//! derivation from it is deterministic (the same record gives the same bits on every target:
//! IEEE `+ − × ÷`, fused multiply-add (one rounding, as IEEE fixes it) and
//! `alice_det_math::sqrt64`, no other function).
//!
//! ```text
//! three_view 1
//! image 1500 743
//! picked_by human 2026-10-29
//! stature 1.48 stated
//! footwear heel outsole 4 ankle_lift 11 ± 4
//! view front box 190 30 380 690 ground 683 centre 284.5
//! view face box 918 672 1092 930 ground - centre 1000
//! mark eye front 284 121 ± 2
//! mark knee front 248 487 ± 8 rule knee_waist
//! mark hips front not_measurable under the skirt
//! ```
//!
//! - `stature <m> stated|measured`: the height from the sole to the top of the skull, without
//!   hair (optional; a stated height is an input, not checked against the sheet)
//! - `footwear flat|boot|heel outsole <px> ankle_lift <px> ± <px>`: the drawn ground is
//!   `outsole + ankle_lift` below the barefoot sole for every landmark above the ankle
//! - `view <front|side|back|face> box x0 y0 x1 y1 ground <y|-> centre <x|->`: the sheet's
//!   views; heights are measured from a view's own ground (y grows downwards)
//! - `mark <name> <view> <x> <y> ± <px> [rule <rule>]` or `mark <name> <view> not_measurable
//!   <reason>`: a landmark; `rule` names the picking rule when the point was found by a rule
//!   whose bias was calibrated ([`Rule`]), and the calibrated correction is then applied
//!
//! Every value is derived in the laws' units (body: the eye height above the sole is 1; face:
//! the chin-to-eye distance is 1) with a ± from the landmarks' ± by linear propagation (an
//! upper bound, never smaller than the true spread of the inputs). A parameter whose
//! landmarks are not measured is not derived: it is listed as not measurable, never guessed.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use crate::json_lite::{get, parse as parse_json, J};

/// The record format this module reads and writes
pub const RECORD_FORMAT: u32 = 1;
/// The version of [`Check::to_json`]'s shape (the contract a reader of the JSON relies on)
pub const CHECK_JSON_VERSION: u32 = 1;
/// The domain of [`Record::id`]
const RECORD_ID_DOMAIN: &[u8] = b"lol.three_view.record/v1\0";

/// A view of the sheet
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum View {
    /// The body seen from the front
    Front,
    /// The body seen from its side
    Side,
    /// The body seen from behind
    Back,
    /// The face seen from the front, at its own scale
    Face,
}

impl View {
    const ALL: [Self; 4] = [Self::Front, Self::Side, Self::Back, Self::Face];

    /// The view's name in a record
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Side => "side",
            Self::Back => "back",
            Self::Face => "face",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.name() == s)
    }

    /// The landmarks this view may carry
    #[must_use]
    pub const fn landmarks(self) -> &'static [&'static str] {
        match self {
            Self::Front => &[
                "head_top",
                "eye",
                "chin",
                "shoulder",
                "elbow",
                "wrist",
                "hips",
                "hip_joint",
                "crotch",
                "knee",
                "ankle",
            ],
            Self::Side => &[
                "head_top", "eye", "chin", "shoulder", "knee", "ankle", "heel", "toe_sole",
            ],
            Self::Back => &["head_top", "shoulder", "knee"],
            Self::Face => &[
                "eye",
                "chin",
                "mouth",
                "nose",
                "brow",
                "eye_outer",
                "eye_inner",
                "eye_top",
                "eye_bottom",
                "mouth_left",
                "mouth_right",
                "brow_inner",
                "brow_outer",
                "head_left",
                "head_right",
            ],
        }
    }
}

/// A picking rule whose bias against the true landmark was calibrated on rendered reference
/// bodies (blind picks on orthographic renders, compared with the bodies' 3D skeletons)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// The knee as the narrowest leg row between the thigh and the calf's widest row: reads
    /// low; corrected by [`KNEE_WAIST_CORRECTION`]
    KneeWaist,
    /// The shoulder's x where the arm separates from the torso: reads wide; corrected by
    /// [`ARMPIT_CORRECTION`]
    Armpit,
}

/// Added to `knee` (eye-height units) for a knee picked by [`Rule::KneeWaist`] (calibrated
/// bias −0.018 and −0.024 on two reference bodies)
pub const KNEE_WAIST_CORRECTION: f64 = 0.021;
/// Added to `shoulder_x` (eye-height units) for a shoulder picked by [`Rule::Armpit`]
/// (calibrated bias +0.013 and +0.023)
pub const ARMPIT_CORRECTION: f64 = -0.018;
/// The ± of each calibrated correction (half the spread between the two references)
pub const CORRECTION_UNCERTAINTY: f64 = 0.005;

impl Rule {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "knee_waist" => Some(Self::KneeWaist),
            "armpit" => Some(Self::Armpit),
            _ => None,
        }
    }
    const fn name(self) -> &'static str {
        match self {
            Self::KneeWaist => "knee_waist",
            Self::Armpit => "armpit",
        }
    }
}

/// What a landmark is in a record
#[derive(Debug, Clone, PartialEq)]
pub enum Mark {
    /// Picked at (x, y) sheet pixels, ± `u` pixels, by `rule` if a calibrated rule was used
    At {
        x: f64,
        y: f64,
        u: f64,
        rule: Option<Rule>,
    },
    /// Not measurable on this sheet, and why
    NotMeasurable(String),
}

/// What the footwear is
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Footwear {
    /// Barefoot or a thin flat sole
    Flat,
    /// A thick sole
    Boot,
    /// A raised heel
    Heel,
}

/// One view's frame on the sheet
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub view: View,
    /// x0, y0, x1, y1 in sheet pixels
    pub area: [f64; 4],
    pub ground: Option<f64>,
    pub centre: Option<f64>,
}

/// A three-view record
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub image: (u32, u32),
    pub picked_by: String,
    /// Metres from the sole to the skull's top, and whether it was stated on the sheet
    pub stature: Option<(f64, bool)>,
    pub footwear: Footwear,
    pub outsole: f64,
    pub ankle_lift: f64,
    pub footwear_u: f64,
    pub frames: Vec<Frame>,
    pub marks: Vec<(String, View, Mark)>,
}

/// Why a record was refused
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordError {
    /// 1-based line, 0 for the whole record
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for RecordError {}

fn err(line: usize, message: impl Into<String>) -> RecordError {
    RecordError {
        line,
        message: message.into(),
    }
}

fn number(line: usize, s: &str, what: &str) -> Result<f64, RecordError> {
    match s.parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v),
        _ => Err(err(line, format!("{what}: `{s}` is not a finite number"))),
    }
}

fn non_negative(line: usize, s: &str, what: &str) -> Result<f64, RecordError> {
    let v = number(line, s, what)?;
    if v < 0.0 {
        return Err(err(line, format!("{what}: {v} is negative")));
    }
    Ok(v)
}

fn optional(line: usize, s: &str, what: &str) -> Result<Option<f64>, RecordError> {
    if s == "-" {
        Ok(None)
    } else {
        number(line, s, what).map(Some)
    }
}

/// One `mark` line's words: the landmark, its view and what was picked
fn parse_mark(n: usize, w: &[&str]) -> Result<(String, View, Mark), RecordError> {
    let view = View::parse(w[2]).ok_or_else(|| err(n, format!("unknown view `{}`", w[2])))?;
    let name = w[1];
    if !view.landmarks().contains(&name) {
        return Err(err(
            n,
            format!("`{name}` is not a landmark of the {} view", view.name()),
        ));
    }
    let mark = if w[3] == "not_measurable" {
        if w.len() < 5 {
            return Err(err(n, "not_measurable needs a reason"));
        }
        Mark::NotMeasurable(w[4..].join(" "))
    } else if (w.len() == 7 || (w.len() == 9 && w[7] == "rule")) && w[5] == "±" {
        let rule = if w.len() == 9 {
            Some(Rule::parse(w[8]).ok_or_else(|| err(n, format!("unknown rule `{}`", w[8])))?)
        } else {
            None
        };
        Mark::At {
            x: number(n, w[3], "x")?,
            y: number(n, w[4], "y")?,
            u: non_negative(n, w[6], "±")?,
            rule,
        }
    } else {
        return Err(err(n, "mark: `<name> <view> <x> <y> ± <px> [rule <rule>]` or `<name> <view> not_measurable <reason>`"));
    };
    Ok((name.to_owned(), view, mark))
}

impl Record {
    /// Read a record
    ///
    /// # Errors
    ///
    /// [`RecordError`] for a missing or second header, an unknown line, a value that is not
    /// a finite number, an unknown view or landmark, a landmark or view given twice, or a
    /// landmark on a view the record does not frame.
    #[allow(clippy::too_many_lines)] // one arm per kind of line, in file order
    pub fn parse(text: &str) -> Result<Self, RecordError> {
        let mut lines = text
            .lines()
            .enumerate()
            .map(|(i, l)| (i + 1, l.trim()))
            .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'));
        match lines.next() {
            Some((_, "three_view 1")) => {}
            Some((n, l)) => {
                return Err(err(
                    n,
                    format!("the first line must be `three_view 1`, not `{l}`"),
                ))
            }
            None => return Err(err(0, "empty record")),
        }
        let mut r = Self {
            image: (0, 0),
            picked_by: String::new(),
            stature: None,
            footwear: Footwear::Flat,
            outsole: 0.0,
            ankle_lift: 0.0,
            footwear_u: 0.0,
            frames: Vec::new(),
            marks: Vec::new(),
        };
        let (mut seen_image, mut seen_footwear) = (false, false);
        for (n, line) in lines {
            let w: Vec<&str> = line.split_whitespace().collect();
            match w[0] {
                "image" if w.len() == 3 && !seen_image => {
                    let px = |s: &str| {
                        s.parse::<u32>()
                            .map_err(|_| err(n, format!("image: `{s}` is not a pixel count")))
                    };
                    r.image = (px(w[1])?, px(w[2])?);
                    seen_image = true;
                }
                "picked_by" if w.len() >= 2 && r.picked_by.is_empty() => {
                    r.picked_by = w[1..].join(" ");
                }
                "stature" if w.len() == 3 && r.stature.is_none() => {
                    let m = number(n, w[1], "stature")?;
                    if m <= 0.0 {
                        return Err(err(n, "stature must be positive"));
                    }
                    let stated = match w[2] {
                        "stated" => true,
                        "measured" => false,
                        s => {
                            return Err(err(n, format!("stature source `{s}`: stated or measured")))
                        }
                    };
                    r.stature = Some((m, stated));
                }
                "footwear"
                    if w.len() == 8
                        && w[2] == "outsole"
                        && w[4] == "ankle_lift"
                        && w[6] == "±"
                        && !seen_footwear =>
                {
                    r.footwear = match w[1] {
                        "flat" => Footwear::Flat,
                        "boot" => Footwear::Boot,
                        "heel" => Footwear::Heel,
                        s => return Err(err(n, format!("footwear `{s}`: flat, boot or heel"))),
                    };
                    r.outsole = non_negative(n, w[3], "outsole")?;
                    r.ankle_lift = non_negative(n, w[5], "ankle_lift")?;
                    r.footwear_u = non_negative(n, w[7], "footwear ±")?;
                    seen_footwear = true;
                }
                "view"
                    if w.len() == 11 && w[2] == "box" && w[7] == "ground" && w[9] == "centre" =>
                {
                    let view = View::parse(w[1])
                        .ok_or_else(|| err(n, format!("unknown view `{}`", w[1])))?;
                    if r.frames.iter().any(|f| f.view == view) {
                        return Err(err(n, format!("view {} given twice", view.name())));
                    }
                    let a = [
                        number(n, w[3], "box")?,
                        number(n, w[4], "box")?,
                        number(n, w[5], "box")?,
                        number(n, w[6], "box")?,
                    ];
                    if a[2] <= a[0] || a[3] <= a[1] {
                        return Err(err(
                            n,
                            "box must run from its top-left to its bottom-right corner",
                        ));
                    }
                    r.frames.push(Frame {
                        view,
                        area: a,
                        ground: optional(n, w[8], "ground")?,
                        centre: optional(n, w[10], "centre")?,
                    });
                }
                "mark" if w.len() >= 4 => {
                    let (name, view, mark) = parse_mark(n, &w)?;
                    if r.marks.iter().any(|(m, v, _)| *m == name && *v == view) {
                        return Err(err(
                            n,
                            format!("{name} on the {} view given twice", view.name()),
                        ));
                    }
                    r.marks.push((name, view, mark));
                }
                _ => return Err(err(n, format!("unknown or malformed line `{line}`"))),
            }
        }
        if !seen_image {
            return Err(err(0, "missing `image <width> <height>`"));
        }
        for (name, view, _) in &r.marks {
            if !r.frames.iter().any(|f| f.view == *view) {
                return Err(err(
                    0,
                    format!(
                        "{name} is on the {} view, which the record does not frame",
                        view.name()
                    ),
                ));
            }
        }
        // canonical order (the order of the lines carries no meaning)
        r.frames.sort_by_key(|f| f.view);
        let order = |name: &str, view: View| {
            view.landmarks()
                .iter()
                .position(|n| *n == name)
                .unwrap_or(usize::MAX)
        };
        r.marks.sort_by_key(|(n, v, _)| (*v, order(n, *v)));
        Ok(r)
    }

    /// The record in its canonical text: header, image, `picked_by`, stature, footwear, then
    /// the views in front / side / back / face order and the marks in each view's landmark
    /// order (what [`Record::id`] hashes; reading it back gives an equal record)
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut s = format!(
            "three_view {RECORD_FORMAT}\nimage {} {}\n",
            self.image.0, self.image.1
        );
        if !self.picked_by.is_empty() {
            let _ = writeln!(s, "picked_by {}", self.picked_by);
        }
        if let Some((m, stated)) = self.stature {
            let _ = writeln!(
                s,
                "stature {m} {}",
                if stated { "stated" } else { "measured" }
            );
        }
        let fw = match self.footwear {
            Footwear::Flat => "flat",
            Footwear::Boot => "boot",
            Footwear::Heel => "heel",
        };
        let _ = writeln!(
            s,
            "footwear {fw} outsole {} ankle_lift {} ± {}",
            self.outsole, self.ankle_lift, self.footwear_u
        );
        let opt = |v: Option<f64>| v.map_or_else(|| "-".to_owned(), |v| v.to_string());
        for view in View::ALL {
            if let Some(f) = self.frames.iter().find(|f| f.view == view) {
                let a = f.area;
                let _ = writeln!(
                    s,
                    "view {} box {} {} {} {} ground {} centre {}",
                    view.name(),
                    a[0],
                    a[1],
                    a[2],
                    a[3],
                    opt(f.ground),
                    opt(f.centre)
                );
            }
        }
        for view in View::ALL {
            for name in view.landmarks() {
                if let Some((_, _, m)) = self.marks.iter().find(|(n, v, _)| n == name && *v == view)
                {
                    match m {
                        Mark::At { x, y, u, rule } => {
                            let _ = write!(s, "mark {name} {} {x} {y} ± {u}", view.name());
                            if let Some(r) = rule {
                                let _ = write!(s, " rule {}", r.name());
                            }
                            s.push('\n');
                        }
                        Mark::NotMeasurable(why) => {
                            let _ = writeln!(s, "mark {name} {} not_measurable {why}", view.name());
                        }
                    }
                }
            }
        }
        s
    }

    /// The record's identifier: SHA-256 of a domain tag and [`Record::to_text`]
    #[must_use]
    pub fn id(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(RECORD_ID_DOMAIN);
        h.update(self.to_text().as_bytes());
        h.finalize().into()
    }

    fn frame(&self, view: View) -> Option<&Frame> {
        self.frames.iter().find(|f| f.view == view)
    }

    fn mark(&self, name: &str, view: View) -> Option<&Mark> {
        self.marks
            .iter()
            .find(|(n, v, _)| n == name && *v == view)
            .map(|(_, _, m)| m)
    }
}

// ── derivation ──────────────────────────────────────────────────────────────────────────────

/// Which law a derived value belongs to
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Law {
    /// `avatar_body` (eye-height units)
    Body,
    /// `avatar_face` (chin-to-eye units)
    Face,
}

impl Law {
    /// The law's name
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Body => "avatar_body",
            Self::Face => "avatar_face",
        }
    }
}

/// A value derived from the record, with its ±
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub law: Law,
    pub name: &'static str,
    pub value: f64,
    pub u: f64,
    /// The calibrated correction applied, if any
    pub correction: Option<f64>,
}

/// A value the law fixes (a `let`), measured to check that the sheet agrees with it
#[derive(Debug, Clone, PartialEq)]
pub struct Fixed {
    pub law: Law,
    pub name: &'static str,
    pub value: f64,
    pub u: f64,
    pub fixed: f64,
}

/// A landmark seen on two body views at heights that disagree beyond their ±
#[derive(Debug, Clone, PartialEq)]
pub struct CrossView {
    pub landmark: String,
    pub a: View,
    pub b: View,
    /// Height above each view's own ground, in pixels
    pub height_a: f64,
    pub height_b: f64,
    pub tolerance: f64,
}

/// Everything derived from a record
#[derive(Debug, Clone, PartialEq)]
pub struct Derivation {
    pub values: Vec<Value>,
    pub fixed: Vec<Fixed>,
    /// Law parameters not derived, and why
    pub not_measurable: Vec<(Law, &'static str, String)>,
    /// Measured but in no law (e.g. the nose)
    pub extra: Vec<Value>,
    pub cross_view: Vec<CrossView>,
    /// The eye height in metres from the stature, ± (when the record has a stature and the
    /// skull top and eye are measured)
    pub eye_height_m: Option<(f64, f64)>,
}

struct Ctx<'a> {
    r: &'a Record,
    out: Derivation,
}

/// A point picked on a view: (x, y, u), or why it is missing
fn point(r: &Record, name: &str, view: View) -> Result<(f64, f64, f64), String> {
    match r.mark(name, view) {
        Some(Mark::At { x, y, u, .. }) => Ok((*x, *y, *u)),
        Some(Mark::NotMeasurable(why)) => Err(format!(
            "{name} not measurable on the {} view: {why}",
            view.name()
        )),
        None => Err(format!("{name} not picked on the {} view", view.name())),
    }
}

fn rule_of(r: &Record, name: &str, view: View) -> Option<Rule> {
    match r.mark(name, view) {
        Some(Mark::At { rule, .. }) => *rule,
        _ => None,
    }
}

/// Height above the barefoot sole on a body view, in pixels, with its ±
#[allow(clippy::many_single_char_names)] // r record, f frame, g ground, y and u: the point and its ±
fn height(r: &Record, name: &str, view: View) -> Result<(f64, f64), String> {
    let f = r
        .frame(view)
        .ok_or_else(|| format!("no {} view", view.name()))?;
    let g = f
        .ground
        .ok_or_else(|| format!("the {} view has no ground", view.name()))?;
    let (_, y, u) = point(r, name, view)?;
    Ok((g - y - r.outsole - r.ankle_lift, u + r.footwear_u))
}

/// The ratio a / b with the linear bound of its ±
fn ratio(a: f64, ua: f64, b: f64, ub: f64) -> (f64, f64) {
    let q = a / b;
    (q, q.abs().mul_add(ub, ua) / b)
}

impl Ctx<'_> {
    fn value(&mut self, law: Law, name: &'static str, v: Result<(f64, f64), String>) {
        match v {
            Ok((value, u)) => self.out.values.push(Value {
                law,
                name,
                value,
                u,
                correction: None,
            }),
            Err(why) => self.out.not_measurable.push((law, name, why)),
        }
    }

    #[allow(clippy::many_single_char_names)] // e eye height, u ±, x / y sheet pixels, as in the module doc
    fn body(&mut self) {
        let r = self.r;
        let (e, ue) = match height(r, "eye", View::Front) {
            Ok((e, ue)) if e > 0.0 => (e, ue),
            Ok(_) => return self.all_body("the eye is not above the sole"),
            Err(why) => return self.all_body(&format!("no eye height: {why}")),
        };
        let h = |name: &str| height(r, name, View::Front);
        let over_e = |hv: Result<(f64, f64), String>| hv.map(|(a, ua)| ratio(a, ua, e, ue));
        let centre = r.frame(View::Front).and_then(|f| f.centre);
        let x_from_centre = |name: &str| -> Result<(f64, f64), String> {
            let c = centre.ok_or_else(|| "the front view has no centre".to_owned())?;
            let (x, _, u) = point(r, name, View::Front)?;
            Ok(ratio((x - c).abs(), u, e, ue))
        };
        self.value(Law::Body, "head_top", over_e(h("head_top")));
        self.value(
            Law::Body,
            "face_length",
            h("eye").and_then(|(a, ua)| h("chin").map(|(b, ub)| ratio(a - b, ua + ub, e, ue))),
        );
        // knee, with the calibrated correction when it was picked by its rule
        match over_e(h("knee")) {
            Ok((v, u)) if rule_of(r, "knee", View::Front) == Some(Rule::KneeWaist) => {
                self.out.values.push(Value {
                    law: Law::Body,
                    name: "knee",
                    value: v + KNEE_WAIST_CORRECTION,
                    u: u + CORRECTION_UNCERTAINTY,
                    correction: Some(KNEE_WAIST_CORRECTION),
                });
            }
            other => self.value(Law::Body, "knee", other),
        }
        self.value(Law::Body, "hips_y", over_e(h("hips")));
        self.value(
            Law::Body,
            "hip_drop",
            h("hips")
                .and_then(|(a, ua)| h("hip_joint").map(|(b, ub)| ratio(a - b, ua + ub, e, ue))),
        );
        self.value(Law::Body, "hip_x", x_from_centre("hip_joint"));
        match x_from_centre("shoulder") {
            Ok((v, u)) if rule_of(r, "shoulder", View::Front) == Some(Rule::Armpit) => {
                self.out.values.push(Value {
                    law: Law::Body,
                    name: "shoulder_x",
                    value: v + ARMPIT_CORRECTION,
                    u: u + CORRECTION_UNCERTAINTY,
                    correction: Some(ARMPIT_CORRECTION),
                });
            }
            other => self.value(Law::Body, "shoulder_x", other),
        }
        // the arm along its own polyline (arms out or down alike)
        let seg = |a: &str, b: &str| -> Result<(f64, f64), String> {
            let (xa, ya, ua) = point(r, a, View::Front)?;
            let (xb, yb, ub) = point(r, b, View::Front)?;
            let (dx, dy) = (xb - xa, yb - ya);
            Ok((alice_det_math::sqrt64(dy.mul_add(dy, dx * dx)), ua + ub))
        };
        let upper = seg("shoulder", "elbow");
        let lower = seg("elbow", "wrist");
        match (&upper, &lower) {
            (Ok((l1, u1)), Ok((l2, u2))) => {
                let (arm, uarm) = (l1 + l2, u1 + u2);
                self.value(Law::Body, "arm", Ok(ratio(arm, uarm, e, ue)));
                self.value(Law::Body, "elbow_split", Ok(ratio(*l1, *u1, arm, uarm)));
            }
            (Err(why), _) | (_, Err(why)) => {
                self.out
                    .not_measurable
                    .push((Law::Body, "arm", why.clone()));
                self.out
                    .not_measurable
                    .push((Law::Body, "elbow_split", why.clone()));
            }
        }
        // the fixed lets, measured
        for (name, landmark, fixed) in [("shoulder", "shoulder", 0.9), ("ankle", "ankle", 0.06)] {
            if let Ok((v, u)) = over_e(h(landmark)) {
                self.out.fixed.push(Fixed {
                    law: Law::Body,
                    name,
                    value: v,
                    u,
                    fixed,
                });
            }
        }
        // stature: the skull top in metres gives the scale
        if let (Some((m, _)), Ok((top, utop))) = (r.stature, h("head_top")) {
            if top > 0.0 {
                let (q, uq) = ratio(e, ue, top, utop);
                self.out.eye_height_m = Some((q * m, uq * m));
            }
        }
    }

    fn all_body(&mut self, why: &str) {
        for name in [
            "head_top",
            "face_length",
            "knee",
            "hips_y",
            "hip_drop",
            "hip_x",
            "shoulder_x",
            "arm",
            "elbow_split",
        ] {
            self.out
                .not_measurable
                .push((Law::Body, name, why.to_owned()));
        }
    }

    #[allow(clippy::many_single_char_names)] // f chin-to-eye distance, u ±, x / y sheet pixels
    fn face(&mut self) {
        let r = self.r;
        let p = |name: &str| point(r, name, View::Face);
        let (f, uf) = match (p("eye"), p("chin")) {
            (Ok((_, ye, ue)), Ok((_, yc, uc))) if yc > ye => (yc - ye, ue + uc),
            (Ok(_), Ok(_)) => {
                return self.all_face("the chin is not below the eye on the face view")
            }
            (Err(why), _) | (_, Err(why)) => {
                return self.all_face(&format!("no chin-to-eye distance: {why}"))
            }
        };
        let chin_y = p("chin").map(|t| t.1).unwrap_or_default();
        let up = |name: &str| p(name).map(|(_, y, u)| ratio(chin_y - y, u, f, uf));
        let across = |a: &str, b: &str| -> Result<(f64, f64), String> {
            let (xa, _, ua) = p(a)?;
            let (xb, _, ub) = p(b)?;
            Ok(ratio((xb - xa).abs(), ua + ub, f, uf))
        };
        let tall = |a: &str, b: &str| -> Result<(f64, f64), String> {
            let (_, ya, ua) = p(a)?;
            let (_, yb, ub) = p(b)?;
            Ok(ratio((yb - ya).abs(), ua + ub, f, uf))
        };
        let centre = r.frame(View::Face).and_then(|fr| fr.centre);
        let from_centre = |name: &str| -> Result<(f64, f64), String> {
            let c = centre.ok_or_else(|| "the face view has no centre".to_owned())?;
            let (x, _, u) = p(name)?;
            Ok(ratio((x - c).abs(), u, f, uf))
        };
        self.value(Law::Face, "eye_width", across("eye_inner", "eye_outer"));
        self.value(Law::Face, "eye_height", tall("eye_top", "eye_bottom"));
        self.value(Law::Face, "brow_y", up("brow"));
        self.value(Law::Face, "brow_inner", from_centre("brow_inner"));
        self.value(Law::Face, "brow_outer", from_centre("brow_outer"));
        self.value(Law::Face, "mouth_y", up("mouth"));
        self.value(
            Law::Face,
            "mouth_width",
            across("mouth_left", "mouth_right"),
        );
        for name in ["eye_recess", "depth"] {
            self.out.not_measurable.push((
                Law::Face,
                name,
                "needs a side view of the face".to_owned(),
            ));
        }
        if let Ok((v, u)) = up("nose") {
            self.out.extra.push(Value {
                law: Law::Face,
                name: "nose_y",
                value: v,
                u,
                correction: None,
            });
        }
        if let Ok((v, u)) = across("head_left", "head_right") {
            self.out.fixed.push(Fixed {
                law: Law::Face,
                name: "head_width",
                value: v,
                u,
                fixed: 2.0,
            });
        }
        if let (Some(c), Ok((xo, _, uo)), Ok((xi, _, ui))) =
            (centre, p("eye_outer"), p("eye_inner"))
        {
            let (v, u) = ratio(
                (f64::midpoint(xo, xi) - c).abs(),
                f64::midpoint(uo, ui),
                f,
                uf,
            );
            self.out.fixed.push(Fixed {
                law: Law::Face,
                name: "eye_x",
                value: v,
                u,
                fixed: 0.53,
            });
        }
    }

    fn all_face(&mut self, why: &str) {
        for name in [
            "eye_width",
            "eye_height",
            "brow_y",
            "brow_inner",
            "brow_outer",
            "mouth_y",
            "mouth_width",
            "eye_recess",
            "depth",
        ] {
            self.out
                .not_measurable
                .push((Law::Face, name, why.to_owned()));
        }
    }

    /// The same landmark on two body views must sit at the same height above each view's
    /// ground, within the sum of their ±; a difference is reported, never averaged
    fn cross_view(&mut self) {
        let r = self.r;
        let body = [View::Front, View::Side, View::Back];
        for (i, &a) in body.iter().enumerate() {
            for &b in &body[i + 1..] {
                for name in a.landmarks() {
                    if !b.landmarks().contains(name) {
                        continue;
                    }
                    let ga = r.frame(a).and_then(|f| f.ground);
                    let gb = r.frame(b).and_then(|f| f.ground);
                    if let (Some(ga), Some(gb), Ok((_, ya, ua)), Ok((_, yb, ub))) =
                        (ga, gb, point(r, name, a), point(r, name, b))
                    {
                        let (ha, hb) = (ga - ya, gb - yb);
                        if (ha - hb).abs() > ua + ub {
                            self.out.cross_view.push(CrossView {
                                landmark: (*name).to_owned(),
                                a,
                                b,
                                height_a: ha,
                                height_b: hb,
                                tolerance: ua + ub,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Derive the law values a record supports
#[must_use]
pub fn derive(r: &Record) -> Derivation {
    let mut c = Ctx {
        r,
        out: Derivation {
            values: Vec::new(),
            fixed: Vec::new(),
            not_measurable: Vec::new(),
            extra: Vec::new(),
            cross_view: Vec::new(),
            eye_height_m: None,
        },
    };
    c.body();
    c.face();
    c.cross_view();
    c.out
}

// ── verdicts ────────────────────────────────────────────────────────────────────────────────

/// Where a value with its ± falls against a range
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    /// The whole interval value ± u lies in the range
    In,
    /// The value is in the range but its ± reaches past an end
    Edge,
    /// Below the range by this much (the value's distance to the lower end)
    Below(f64),
    /// Above the range by this much
    Above(f64),
}

impl Verdict {
    /// Judge `value ± u` against `[lo, hi]`
    #[must_use]
    pub fn of(value: f64, u: f64, lo: f64, hi: f64) -> Self {
        if value < lo {
            Self::Below(lo - value)
        } else if value > hi {
            Self::Above(value - hi)
        } else if value - u < lo || value + u > hi {
            Self::Edge
        } else {
            Self::In
        }
    }

    const fn word(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Edge => "edge",
            Self::Below(_) => "below",
            Self::Above(_) => "above",
        }
    }

    const fn by(self) -> f64 {
        match self {
            Self::Below(d) | Self::Above(d) => d,
            _ => 0.0,
        }
    }
}

/// One law parameter checked against its range
#[derive(Debug, Clone, PartialEq)]
pub struct Judged {
    pub value: Value,
    pub lo: f64,
    pub hi: f64,
    pub verdict: Verdict,
}

/// A record checked against the laws' ranges
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub record_id: [u8; 32],
    pub body_law_id: [u8; 32],
    pub face_law_id: [u8; 32],
    pub judged: Vec<Judged>,
    pub derivation: Derivation,
}

/// The ranges of a law's parameters: (name, lo, hi)
pub type Ranges<'a> = &'a [(String, f64, f64)];

/// Check a record's derived values against the two laws' ranges
///
/// # Errors
///
/// When a derived parameter has no range in its law (the record and the law disagree on the
/// law's parameters).
pub fn check(
    r: &Record,
    body: (&[u8; 32], Ranges<'_>),
    face: (&[u8; 32], Ranges<'_>),
) -> Result<Check, String> {
    let d = derive(r);
    let mut judged = Vec::new();
    for v in &d.values {
        let ranges = match v.law {
            Law::Body => body.1,
            Law::Face => face.1,
        };
        let (_, lo, hi) = ranges
            .iter()
            .find(|(n, _, _)| n == v.name)
            .ok_or_else(|| format!("{} has no parameter {}", v.law.name(), v.name))?;
        judged.push(Judged {
            value: v.clone(),
            lo: *lo,
            hi: *hi,
            verdict: Verdict::of(v.value, v.u, *lo, *hi),
        });
    }
    Ok(Check {
        record_id: r.id(),
        body_law_id: *body.0,
        face_law_id: *face.0,
        judged,
        derivation: d,
    })
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// Numbers in the JSON: 6 decimals, fixed (the same text on every target for the same bits)
fn num(v: f64) -> String {
    format!("{v:.6}")
}

impl Check {
    /// The check as JSON (shape version [`CHECK_JSON_VERSION`]): keys in a fixed order, one
    /// line, numbers with 6 decimals
    #[must_use]
    pub fn to_json(&self) -> String {
        let d = &self.derivation;
        let mut s = format!(
            "{{\"three_view_check\":{CHECK_JSON_VERSION},\"record_id\":\"{}\",\"laws\":{{\"avatar_body\":\"{}\",\"avatar_face\":\"{}\"}},\"params\":[",
            hex(&self.record_id),
            hex(&self.body_law_id),
            hex(&self.face_law_id)
        );
        for (i, j) in self.judged.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(
                s,
                "{{\"law\":\"{}\",\"name\":\"{}\",\"value\":{},\"u\":{},\"lo\":{},\"hi\":{},\"verdict\":\"{}\",\"by\":{},\"correction\":{}}}",
                j.value.law.name(),
                j.value.name,
                num(j.value.value),
                num(j.value.u),
                num(j.lo),
                num(j.hi),
                j.verdict.word(),
                num(j.verdict.by()),
                j.value.correction.map_or_else(|| "null".to_owned(), num)
            );
        }
        s.push_str("],\"fixed\":[");
        for (i, f) in d.fixed.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let off = (f.value - f.fixed).abs() > f.u;
            let _ = write!(s, "{{\"law\":\"{}\",\"name\":\"{}\",\"value\":{},\"u\":{},\"fixed\":{},\"agrees\":{}}}", f.law.name(), f.name, num(f.value), num(f.u), num(f.fixed), !off);
        }
        s.push_str("],\"not_measurable\":[");
        for (i, (law, name, why)) in d.not_measurable.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(
                s,
                "{{\"law\":\"{}\",\"name\":\"{name}\",\"why\":{}}}",
                law.name(),
                json_str(why)
            );
        }
        s.push_str("],\"extra\":[");
        for (i, v) in d.extra.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(
                s,
                "{{\"name\":\"{}\",\"value\":{},\"u\":{}}}",
                v.name,
                num(v.value),
                num(v.u)
            );
        }
        s.push_str("],\"cross_view\":[");
        for (i, c) in d.cross_view.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(
                s,
                "{{\"landmark\":\"{}\",\"views\":[\"{}\",\"{}\"],\"heights_px\":[{},{}],\"tolerance_px\":{}}}",
                c.landmark,
                c.a.name(),
                c.b.name(),
                num(c.height_a),
                num(c.height_b),
                num(c.tolerance)
            );
        }
        s.push_str("],\"eye_height_m\":");
        s.push_str(&d.eye_height_m.map_or_else(
            || "null".to_owned(),
            |(v, u)| format!("{{\"value\":{},\"u\":{}}}", num(v), num(u)),
        ));
        s.push('}');
        s
    }
}

// ── landmarks as JSON → record ──────────────────────────────────────────────────────────────

fn only(o: &[(String, J)], allowed: &[&str], what: &str) -> Result<(), RecordError> {
    match o.iter().find(|(k, _)| !allowed.contains(&k.as_str())) {
        Some((k, _)) => Err(err(
            0,
            format!(
                "{what}: unknown key `{k}` (allowed: {})",
                allowed.join(", ")
            ),
        )),
        None => Ok(()),
    }
}

fn jnum(v: Option<&J>, what: &str) -> Result<f64, RecordError> {
    match v {
        Some(J::Num(n)) => Ok(*n),
        _ => Err(err(0, format!("{what}: a number is required"))),
    }
}

fn jopt(v: Option<&J>, what: &str) -> Result<Option<f64>, RecordError> {
    match v {
        None | Some(J::Null) => Ok(None),
        Some(J::Num(n)) => Ok(Some(*n)),
        _ => Err(err(0, format!("{what}: a number or null"))),
    }
}

fn jstr<'a>(v: Option<&'a J>, what: &str) -> Result<&'a str, RecordError> {
    match v {
        Some(J::Str(s)) => Ok(s),
        _ => Err(err(0, format!("{what}: a string is required"))),
    }
}

/// Build a record from landmarks given as JSON (for a model or a detector that picks points)
///
/// ```json
/// {"image": [1500, 743], "picked_by": "model-x 2026-10-29", "stature": {"m": 1.48, "source": "stated"},
///  "footwear": {"kind": "heel", "outsole": 4, "ankle_lift": 11, "u": 4},
///  "views": {"front": {"box": [190, 30, 380, 690], "ground": 683, "centre": 284.5}},
///  "marks": [{"name": "eye", "view": "front", "x": 284, "y": 121, "u": 2},
///            {"name": "knee", "view": "front", "x": 248, "y": 487, "u": 8, "rule": "knee_waist"},
///            {"name": "hips", "view": "front", "not_measurable": "under the skirt"}]}
/// ```
///
/// # Errors
///
/// [`RecordError`] for JSON outside this shape (unknown keys are refused) or a record the
/// text reader would refuse (the JSON is written as record text and read back).
#[allow(clippy::many_single_char_names)] // p parser, t record text, o / m / f / v JSON objects
#[allow(clippy::too_many_lines)] // one block per top-level key, in record order
pub fn record_from_json(text: &str) -> Result<Record, RecordError> {
    let root = parse_json(text).map_err(|e| err(0, e))?;
    let J::Obj(o) = root else {
        return Err(err(0, "the JSON must be an object"));
    };
    only(
        &o,
        &[
            "image",
            "picked_by",
            "stature",
            "footwear",
            "views",
            "marks",
        ],
        "record",
    )?;
    let mut t = format!("three_view {RECORD_FORMAT}\n");
    match get(&o, "image") {
        Some(J::Arr(a)) if a.len() == 2 => {
            let w = jnum(a.first(), "image width")?;
            let h = jnum(a.get(1), "image height")?;
            let _ = writeln!(t, "image {w} {h}");
        }
        _ => return Err(err(0, "image: [width, height] is required")),
    }
    if let Some(v) = get(&o, "picked_by") {
        let _ = writeln!(t, "picked_by {}", jstr(Some(v), "picked_by")?);
    }
    if let Some(J::Obj(s)) = get(&o, "stature") {
        only(s, &["m", "source"], "stature")?;
        let _ = writeln!(
            t,
            "stature {} {}",
            jnum(get(s, "m"), "stature.m")?,
            jstr(get(s, "source"), "stature.source")?
        );
    } else if get(&o, "stature").is_some() {
        return Err(err(0, "stature: an object {\"m\": …, \"source\": …}"));
    }
    match get(&o, "footwear") {
        Some(J::Obj(f)) => {
            only(f, &["kind", "outsole", "ankle_lift", "u"], "footwear")?;
            let _ = writeln!(
                t,
                "footwear {} outsole {} ankle_lift {} ± {}",
                jstr(get(f, "kind"), "footwear.kind")?,
                jnum(get(f, "outsole"), "footwear.outsole")?,
                jnum(get(f, "ankle_lift"), "footwear.ankle_lift")?,
                jnum(get(f, "u"), "footwear.u")?
            );
        }
        None => {}
        _ => return Err(err(0, "footwear: an object")),
    }
    match get(&o, "views") {
        Some(J::Obj(vs)) => {
            for (name, v) in vs {
                let J::Obj(v) = v else {
                    return Err(err(0, format!("views.{name}: an object")));
                };
                only(v, &["box", "ground", "centre"], &format!("views.{name}"))?;
                let b = match get(v, "box") {
                    Some(J::Arr(a)) if a.len() == 4 => [
                        jnum(a.first(), "box")?,
                        jnum(a.get(1), "box")?,
                        jnum(a.get(2), "box")?,
                        jnum(a.get(3), "box")?,
                    ],
                    _ => return Err(err(0, format!("views.{name}.box: [x0, y0, x1, y1]"))),
                };
                let opt = |x: Option<f64>| x.map_or_else(|| "-".to_owned(), |x| x.to_string());
                let _ = writeln!(
                    t,
                    "view {name} box {} {} {} {} ground {} centre {}",
                    b[0],
                    b[1],
                    b[2],
                    b[3],
                    opt(jopt(get(v, "ground"), "ground")?),
                    opt(jopt(get(v, "centre"), "centre")?)
                );
            }
        }
        _ => return Err(err(0, "views: an object of view name → frame")),
    }
    match get(&o, "marks") {
        Some(J::Arr(ms)) => {
            for m in ms {
                let J::Obj(m) = m else {
                    return Err(err(0, "marks: objects"));
                };
                only(
                    m,
                    &["name", "view", "x", "y", "u", "rule", "not_measurable"],
                    "mark",
                )?;
                let (name, view) = (
                    jstr(get(m, "name"), "mark.name")?,
                    jstr(get(m, "view"), "mark.view")?,
                );
                if let Some(why) = get(m, "not_measurable") {
                    let why = jstr(Some(why), "mark.not_measurable")?;
                    let _ = writeln!(t, "mark {name} {view} not_measurable {why}");
                } else {
                    let _ = write!(
                        t,
                        "mark {name} {view} {} {} ± {}",
                        jnum(get(m, "x"), "mark.x")?,
                        jnum(get(m, "y"), "mark.y")?,
                        jnum(get(m, "u"), "mark.u")?
                    );
                    if let Some(r) = get(m, "rule") {
                        let _ = write!(t, " rule {}", jstr(Some(r), "mark.rule")?);
                    }
                    t.push('\n');
                }
            }
        }
        _ => return Err(err(0, "marks: an array")),
    }
    Record::parse(&t)
}
