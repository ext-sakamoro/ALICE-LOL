//! Measure a body in the units of the avatar body law (`laws/avatar/body.law`)
//!
//! The unit is the height of the eyes above the sole: the sole is at 0 and the eyes at 1. Only
//! points that every rig has and that hair, ears and props cannot move are read: the sole (the
//! lowest point of the foot on the body mesh), the eye line (the eye bones) and the joints. The
//! top of the head is not measured, it is a parameter of the law.
//!
//! The input is a [`BodyMarks`] in a fixed frame (x to the body's left, y up, z forward); the
//! readers of file formats (feature `fbx`) build it.
//!
//! ```
//! use alice_lol_humanoid::measure::{measure, BodyMarks, Part, Side};
//!
//! let mut marks = BodyMarks::new(0.0, 1.0);
//! for (part, x, y) in [
//!     (Part::UpperLeg, 0.05, 0.6),
//!     (Part::LowerLeg, 0.05, 0.35),
//!     (Part::UpperArm, 0.06, 0.9),
//!     (Part::LowerArm, 0.23, 0.9),
//!     (Part::Hand, 0.36, 0.9),
//! ] {
//!     marks.set(part, Some(Side::Left), [x, y, 0.0]);
//!     marks.set(part, Some(Side::Right), [-x, y, 0.0]);
//! }
//! marks.set(Part::Hips, None, [0.0, 0.65, 0.0]);
//! let m = measure(&marks).unwrap();
//! assert_eq!(m.get("knee"), Some(0.35));
//! assert_eq!(m.get("face_length"), None); // no chin was marked
//! ```

// no fused multiply-add: a measure gives the same bits on every target
#![allow(clippy::suboptimal_flops)]

use std::collections::BTreeMap;

/// The parts of the body: the ones a measurement reads, and the trunk, shoulders, toes and breasts
/// a rig names (read by [`crate::rig::classify`], not by [`measure`])
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Part {
    /// The pelvis (one, on the midline)
    Hips,
    /// The hip joint
    UpperLeg,
    /// The knee
    LowerLeg,
    /// The ankle
    Foot,
    /// The shoulder joint
    UpperArm,
    /// The elbow
    LowerArm,
    /// The wrist
    Hand,
    /// The centre of the eye
    Eye,
    /// The head (one, on the midline)
    Head,
    /// The spine, the trunk's lowest bone above the hips (one, on the midline)
    Spine,
    /// The chest (one, on the midline)
    Chest,
    /// The neck (one, on the midline)
    Neck,
    /// The shoulder (the clavicle, inside the shoulder joint)
    Shoulder,
    /// The toes
    Toe,
    /// The breast (its first bone)
    Breast,
}

impl Part {
    /// Whether [`measure`] reads this part (the others are named by a rig but not measured)
    #[must_use]
    pub const fn is_measured(self) -> bool {
        matches!(
            self,
            Self::Hips
                | Self::UpperLeg
                | Self::LowerLeg
                | Self::Foot
                | Self::UpperArm
                | Self::LowerArm
                | Self::Hand
                | Self::Eye
                | Self::Head
        )
    }
}

/// The side of a paired part
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    /// The body's left
    Left,
    /// The body's right
    Right,
}

/// The marked points of one body, x to its left, y up, z forward, in any unit
#[derive(Debug, Clone, PartialEq)]
pub struct BodyMarks {
    /// The height of the sole
    pub sole: f64,
    /// The height of the eye line
    pub eye: f64,
    /// The height of the chin, when the face was read
    pub chin: Option<f64>,
    /// The joints: paired parts carry a side, the hips none
    pub joints: BTreeMap<(Part, Option<Side>), [f64; 3]>,
}

impl BodyMarks {
    /// Marks with only the sole and the eye line
    #[must_use]
    pub const fn new(sole: f64, eye: f64) -> Self {
        Self {
            sole,
            eye,
            chin: None,
            joints: BTreeMap::new(),
        }
    }

    /// Mark one point
    pub fn set(&mut self, part: Part, side: Option<Side>, p: [f64; 3]) {
        self.joints.insert((part, side), p);
    }
}

/// Why a body could not be measured
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MeasureError {
    /// A part the law needs was not marked (its name and side)
    Missing(&'static str),
    /// The eyes are not above the sole, or a mark is not finite
    BadUnit,
    /// A length the law divides by is zero (its name)
    Degenerate(&'static str),
}

impl std::fmt::Display for MeasureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(n) => write!(f, "the body has no {n}"),
            Self::BadUnit => write!(
                f,
                "the eyes are not above the sole, or a mark is not finite"
            ),
            Self::Degenerate(n) => write!(f, "the {n} has no length"),
        }
    }
}

impl std::error::Error for MeasureError {}

/// The parameters of the body law measured on one body, by name
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    values: Vec<(&'static str, f64)>,
}

impl Measured {
    /// The measured value of a parameter
    #[must_use]
    pub fn get(&self, name: &str) -> Option<f64> {
        self.values
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| *v)
    }

    /// Every measured parameter, in the order of the law
    #[must_use]
    pub fn values(&self) -> &[(&'static str, f64)] {
        &self.values
    }

    /// The values as the law's parameter list, for `ProportionLaw::bind`
    #[must_use]
    pub fn as_params(&self) -> Vec<(&str, f64)> {
        self.values.iter().map(|(n, v)| (*n, *v)).collect()
    }
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

/// The left point and the right one (the measures read heights, |x| and lengths, so neither is
/// mirrored)
fn pair(marks: &BodyMarks, part: Part, name: &'static str) -> Result<[[f64; 3]; 2], MeasureError> {
    let get = |s| marks.joints.get(&(part, Some(s))).copied();
    match (get(Side::Left), get(Side::Right)) {
        (Some(l), Some(r)) => Ok([l, r]),
        _ => Err(MeasureError::Missing(name)),
    }
}

/// Measure the body law's parameters on a body
///
/// Paired parts are the mean of the left and the right. The arm is measured along the
/// bones (shoulder to elbow to wrist), so a body in an A pose measures as in a T pose. The face
/// length needs the chin; without it the value is absent and the law's default stands. Values are
/// not clamped to the law's ranges: binding them says which are outside.
///
/// # Errors
///
/// [`MeasureError::BadUnit`] when the eyes are not above the sole or a mark is not finite,
/// [`MeasureError::Missing`] when a part the law needs was not marked,
/// [`MeasureError::Degenerate`] when the arm has no length.
pub fn measure(marks: &BodyMarks) -> Result<Measured, MeasureError> {
    let e = marks.eye - marks.sole;
    let finite = marks.sole.is_finite()
        && marks.eye.is_finite()
        && marks.chin.is_none_or(f64::is_finite)
        && marks.joints.values().flatten().all(|v| v.is_finite());
    if !finite || e.is_nan() || e <= 0.0 {
        return Err(MeasureError::BadUnit);
    }
    let up = |y: f64| (y - marks.sole) / e;
    let hips = marks
        .joints
        .get(&(Part::Hips, None))
        .copied()
        .ok_or(MeasureError::Missing("hips"))?;
    let [hl, hr] = pair(marks, Part::UpperLeg, "hip joints")?;
    let [kl, kr] = pair(marks, Part::LowerLeg, "knees")?;
    let [sl, sr] = pair(marks, Part::UpperArm, "shoulder joints")?;
    let [el, er] = pair(marks, Part::LowerArm, "elbows")?;
    let [wl, wr] = pair(marks, Part::Hand, "wrists")?;
    let mean = f64::midpoint;
    let hip_y = mean(hl[1], hr[1]);
    let upper = mean(dist(sl, el), dist(sr, er));
    let lower = mean(dist(el, wl), dist(er, wr));
    if upper + lower <= 0.0 {
        return Err(MeasureError::Degenerate("arm"));
    }
    let mut values = vec![
        ("hips_y", up(hips[1])),
        ("hip_drop", (hips[1] - hip_y) / e),
        ("hip_x", mean(hl[0].abs(), hr[0].abs()) / e),
        ("knee", up(mean(kl[1], kr[1]))),
        ("shoulder_x", mean(sl[0].abs(), sr[0].abs()) / e),
        ("arm", (upper + lower) / e),
        ("elbow_split", upper / (upper + lower)),
    ];
    if let Some(chin) = marks.chin {
        values.push(("face_length", (marks.eye - chin) / e));
    }
    Ok(Measured { values })
}
