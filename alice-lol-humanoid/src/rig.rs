//! A rig in no file format: named bones with their bind position, and skinned meshes
//!
//! Readers of a format (feature `fbx` here, a VRM reader elsewhere) build a [`Rig`];
//! [`body_marks`] finds the points a measure reads.
//!
//! - **joints** and **eyes**: the bind positions of the bones, named by the usual rig conventions
//!   ([`classify`]: `LeftUpperLeg`, `UpperLeg_L`, `upper_leg.L`, `J_Bip_L_UpperLeg`,
//!   `mixamorig:LeftUpLeg`, the VRM humanoid names, ...). Copies (`.001`), helper bones
//!   (`..._support`) and names with both sides do not match. [`role`] also reads the trunk,
//!   shoulders, toes and breasts, and the roots of the hair, the tail and the ears; the marks use
//!   only the parts a measure reads.
//! - **sole**: the lowest vertex of the body mesh among those that follow a foot (the foot bone or
//!   a bone below it). The body mesh is the skinned mesh whose vertices follow the most body parts
//!   (counted from the bone each vertex follows, not from the bones a skin lists: a file may list
//!   every bone in every skin, with zero weights); shoes, props and stands are other meshes and do
//!   not move it.
//! - **chin**: the lowest vertex, on any mesh, that follows the head bone itself (not hair) and lies
//!   in front of the head bone and between the eyes' outer bounds. Forward is the side of the head
//!   the eyes are on.
//!
//! Every position is in one frame: x to the body's left or right, y up, z forward or back (the
//! signs of x and z do not matter). The units cancel.

// no fused multiply-add: the marks are the same bits on every target
#![allow(clippy::suboptimal_flops)]

use crate::measure::{BodyMarks, MeasureError, Part, Side};

/// One bone of a rig
#[derive(Debug, Clone, PartialEq)]
pub struct RigBone {
    /// Its name, read by [`classify`]
    pub name: String,
    /// Its parent bone, an index into [`Rig::bones`]
    pub parent: Option<usize>,
    /// Its position in the bind pose (`None` for a bone no skin binds)
    pub bind: Option<[f64; 3]>,
}

/// One skinned mesh of a rig
#[derive(Debug, Clone, PartialEq)]
pub struct RigMesh {
    /// Its vertices in the bind pose
    pub positions: Vec<[f64; 3]>,
    /// The bone each vertex follows most (`None` for a vertex no bone weighs on)
    pub leads: Vec<Option<usize>>,
}

/// A rig: bones and skinned meshes
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rig {
    /// The bones
    pub bones: Vec<RigBone>,
    /// The skinned meshes
    pub meshes: Vec<RigMesh>,
}

/// The role of a rig bone: a body part, or the root of a chain that is not the body
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Role {
    /// A part of the body
    Body(Part),
    /// The root of the hair (one, on the midline): `HairRoot`, `Hair`
    HairRoot,
    /// The first bone of the tail (one, on the midline): `Tail_1`, `TailRoot`
    TailRoot,
    /// The first bone of an animal ear (paired): `Ear_1.L`, `Kemomimi_R`
    EarRoot,
}

/// The words of a bone name, lower case, without the side words and the prefixes of the usual
/// conventions, and its side; `None` when it names both sides
fn words(name: &str) -> Option<(Vec<String>, Option<Side>)> {
    let name = name.rsplit(':').next().unwrap_or(name);
    let mut words: Vec<String> = Vec::new();
    for chunk in name.split(|c: char| !c.is_ascii_alphanumeric()) {
        let mut word = String::new();
        let mut prev_lower = false;
        for c in chunk.chars() {
            if c.is_ascii_uppercase() && prev_lower {
                words.extend([std::mem::take(&mut word)]);
            }
            prev_lower = c.is_ascii_lowercase();
            word.extend([c.to_ascii_lowercase()]);
        }
        if !word.is_empty() {
            words.extend([word]);
        }
    }
    let mut side = None;
    let mut kept = Vec::new();
    for w in words {
        let s = match w.as_str() {
            "l" | "left" => Some(Side::Left),
            "r" | "right" => Some(Side::Right),
            "j" | "bip" | "c" | "adj" | "def" | "mixamorig" => continue,
            _ => {
                kept.extend([w]);
                None
            }
        };
        if let Some(s) = s {
            if side.is_some_and(|t| t != s) {
                return None;
            }
            side = Some(s);
        }
    }
    Some((kept, side))
}

/// Read a rig bone's name as a role and side; `None` for any other bone
///
/// The words of the name (split at non-alphanumerics and lower-to-upper case changes) without
/// the side words make a stem that must be one of the known ones exactly: a copy (`Spine.001`),
/// the next bone of a chain (`Tail_2`, `BreastTip.L` after `Breast.L`), a helper
/// (`UpperArm_twist.L`) or a prop named after a part as one word (`plushear.L`) is no
/// role. A midline role takes no side and a paired one needs one. The first bone of a numbered
/// chain (`Tail_1`, `Ear_1.L`) is its root; a numbered trunk bone is not (`Spine1` of mixamo is
/// the second spine bone).
#[must_use]
pub fn role(name: &str) -> Option<(Role, Option<Side>)> {
    let (words, side) = words(name)?;
    let stem: String = words.concat();
    let midline = |r: Role| side.is_none().then_some((r, None));
    let paired = |r: Role| side.map(|s| (r, Some(s)));
    match stem.as_str() {
        "hips" | "pelvis" => midline(Role::Body(Part::Hips)),
        "head" => midline(Role::Body(Part::Head)),
        "spine" => midline(Role::Body(Part::Spine)),
        "chest" => midline(Role::Body(Part::Chest)),
        "neck" => midline(Role::Body(Part::Neck)),
        "upperleg" | "upleg" | "thigh" => paired(Role::Body(Part::UpperLeg)),
        "lowerleg" | "leg" | "knee" | "shin" | "calf" => paired(Role::Body(Part::LowerLeg)),
        "foot" | "ankle" => paired(Role::Body(Part::Foot)),
        "toe" | "toes" | "toebase" => paired(Role::Body(Part::Toe)),
        "shoulder" | "clavicle" => paired(Role::Body(Part::Shoulder)),
        "upperarm" | "arm" => paired(Role::Body(Part::UpperArm)),
        "lowerarm" | "forearm" | "elbow" => paired(Role::Body(Part::LowerArm)),
        "hand" | "wrist" => paired(Role::Body(Part::Hand)),
        "eye" | "faceeye" => paired(Role::Body(Part::Eye)),
        "breast" => paired(Role::Body(Part::Breast)),
        "hair" | "hairroot" => midline(Role::HairRoot),
        "tail" | "tail1" | "tailroot" => midline(Role::TailRoot),
        "ear" | "ear1" | "earroot" | "kemomimi" | "nekomimi" => paired(Role::EarRoot),
        _ => None,
    }
}

/// Read a rig bone's name as a body part and side; `None` for any other bone, and for the roots
/// [`role`] reads that are not the body
#[must_use]
pub fn classify(name: &str) -> Option<(Part, Option<Side>)> {
    match role(name)? {
        (Role::Body(p), s) => Some((p, s)),
        _ => None,
    }
}

/// The mean of some values (`None` for none)
fn mean(v: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = v.fold((0.0, 0_u32), |(s, n), x| (s + x, n + 1));
    (n > 0).then(|| sum / f64::from(n))
}

/// Mark the body of a rig
///
/// # Errors
///
/// [`MeasureError::Missing`] when the rig has no bound eye bones, no bound head bone or no
/// skinned mesh that follows a foot.
pub fn body_marks(rig: &Rig) -> Result<BodyMarks, MeasureError> {
    // only the parts a measure reads: the trunk, shoulders, toes and breasts a rig also names do
    // not take part in the marks nor in choosing the body mesh
    let part = |b: usize| {
        rig.bones
            .get(b)
            .and_then(|x| classify(&x.name))
            .filter(|p| p.0.is_measured())
    };
    let is_head = |b: usize| part(b) == Some((Part::Head, None));
    // the bind position of every bound bone, by part; the first bone of a part is kept
    let mut marks = BodyMarks::new(0.0, 0.0);
    let mut eyes: Vec<(Option<Side>, [f64; 3])> = Vec::new();
    let mut head = None;
    for (b, bone) in rig.bones.iter().enumerate() {
        let Some(p) = bone.bind else { continue };
        match part(b) {
            Some((Part::Eye, side)) => {
                if !eyes.iter().any(|e| e.0 == side) {
                    eyes.extend([(side, p)]);
                }
            }
            Some((Part::Head, _)) => {
                head.get_or_insert(p);
            }
            Some(key) => {
                marks.joints.entry(key).or_insert(p);
            }
            None => {}
        }
    }
    marks.eye = mean(eyes.iter().map(|e| e.1[1])).ok_or(MeasureError::Missing("eyes"))?;
    let head: [f64; 3] = head.ok_or(MeasureError::Missing("head"))?;
    let eye_front = mean(eyes.iter().map(|e| e.1[2])).unwrap_or(head[2]);
    let forward = if eye_front >= head[2] { 1.0 } else { -1.0 };
    let eye_span = eyes
        .iter()
        .map(|e| (e.1[0] - head[0]).abs())
        .fold(0.0, |a, d| if d > a { d } else { a });
    // a bone follows a foot when the foot is on its way to the root
    let below_foot = |mut b: usize| {
        for _ in 0..rig.bones.len() {
            if matches!(part(b), Some((Part::Foot, _))) {
                return true;
            }
            match rig.bones.get(b).and_then(|x| x.parent) {
                Some(p) => b = p,
                None => return false,
            }
        }
        false
    };
    // ordered by comparison, not f64::min (whose zero sign differs between targets)
    let lowest = |a: Option<f64>, y: f64| Some(a.map_or(y, |a: f64| if y < a { y } else { a }));
    let mut body: Option<(usize, f64)> = None;
    let mut chin: Option<f64> = None;
    for m in &rig.meshes {
        let points = || m.positions.iter().zip(&m.leads);
        // the chin: in front of the head, between the eyes' outer bounds, on the head bone itself
        chin = points()
            .filter(|(p, b)| {
                b.is_some_and(is_head)
                    && (p[2] - head[2]) * forward > 0.0
                    && (p[0] - head[0]).abs() <= eye_span
            })
            .map(|(p, _)| p[1])
            .fold(chin, lowest);
        let sole = points()
            .filter(|(_, b)| b.is_some_and(below_foot))
            .map(|(p, _)| p[1])
            .fold(None, lowest);
        // the body parts its vertices follow, each counted once
        let mut parts: Vec<(Part, Option<Side>)> =
            m.leads.iter().filter_map(|b| b.and_then(part)).collect();
        parts.sort_unstable();
        parts.dedup();
        let named = parts.len();
        if let Some(sole) = sole {
            if body.is_none_or(|(n, _)| named > n) {
                body = Some((named, sole));
            }
        }
    }
    marks.sole = body
        .ok_or(MeasureError::Missing("skinned body with feet"))?
        .1;
    marks.chin = chin;
    Ok(marks)
}
