//! A [`Rig`] from a binary FBX scene, for [`measure`](crate::measure::measure)
//!
//! Every model is a bone (its parent kept); a bone's bind position is the `TransformLink` of the
//! first skin cluster that binds it. Every skinned mesh is a rig mesh: its vertices through the
//! mesh model's world matrix, each following the cluster that weighs most on it. The scene's own
//! axes give the frame: its coord axis as x, its up axis as y, the third as z.

// no fused multiply-add: the rig is the same bits on every target
#![allow(clippy::suboptimal_flops)]

use alice_fbx::Scene;

use crate::measure::{BodyMarks, MeasureError};
use crate::rig::{Rig, RigBone, RigMesh};

fn apply(m: &[f64; 16], p: [f64; 3]) -> [f64; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

/// The scene's frame: its coord axis as x, its up axis as y, the third axis as z
fn framer(scene: &Scene) -> impl Fn([f64; 3]) -> [f64; 3] {
    let s = scene.settings;
    let third = 3 - s.up.axis - s.coord.axis;
    move |p: [f64; 3]| {
        let at = |a: u8| p.get(usize::from(a)).copied().unwrap_or(f64::NAN);
        [
            at(s.coord.axis) * f64::from(s.coord.sign),
            at(s.up.axis) * f64::from(s.up.sign),
            at(third),
        ]
    }
}

/// The bone each vertex of a mesh follows most (none for a vertex no cluster weighs on)
fn leads(m: &alice_fbx::scene::Mesh) -> Vec<Option<usize>> {
    let mut lead: Vec<(f64, Option<usize>)> = vec![(0.0, None); m.positions.len()];
    for c in m.skin.iter().flatten() {
        for (&i, &w) in c.indices.iter().zip(&c.weights) {
            if let Some(l) = lead.get_mut(i as usize) {
                if w > l.0 {
                    *l = (w, c.bone);
                }
            }
        }
    }
    lead.into_iter().map(|l| l.1).collect()
}

/// The rig of a scene
#[must_use]
pub fn rig(scene: &Scene) -> Rig {
    let frame = framer(scene);
    let mut bones: Vec<RigBone> = scene
        .models
        .iter()
        .map(|m| RigBone {
            name: m.name.clone(),
            parent: m.parent,
            bind: None,
        })
        .collect();
    let mut meshes = Vec::new();
    for m in scene.meshes.iter().filter(|m| m.skin.is_some()) {
        let clusters = m.skin.iter().flatten();
        for c in clusters.clone() {
            if let Some(bone) = c.bone.and_then(|b| bones.get_mut(b)) {
                let t = &c.transform_link;
                bone.bind
                    .get_or_insert_with(|| frame([t[12], t[13], t[14]]));
            }
        }
        let world = m.model.map(|i| scene.world_matrix(i));
        meshes.extend([RigMesh {
            positions: m
                .positions
                .iter()
                .map(|&p| frame(world.as_ref().map_or(p, |w| apply(w, p))))
                .collect(),
            leads: leads(m),
        }]);
    }
    Rig { bones, meshes }
}

/// Mark the body of a scene: [`crate::rig::body_marks`] of its [`rig`]
///
/// # Errors
///
/// As [`crate::rig::body_marks`]
pub fn body_marks(scene: &Scene) -> Result<BodyMarks, MeasureError> {
    crate::rig::body_marks(&rig(scene))
}
