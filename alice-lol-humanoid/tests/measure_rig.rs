//! Marking a body from a rig in no file format: the parts by name, the sole, the eye line and
//! the chin
#![allow(clippy::float_cmp)]

use alice_lol_humanoid::measure::{measure, MeasureError, Part, Side};
use alice_lol_humanoid::rig::{body_marks, classify, Rig, RigBone, RigMesh};

/// name, parent, bind position (x left, y up, z forward)
type BoneRow = (&'static str, Option<usize>, Option<[f64; 3]>);

const BONES: &[BoneRow] = &[
    ("hips", None, Some([0.0, 0.66, 0.0])),               // 0
    ("leftUpperLeg", Some(0), Some([0.05, 0.62, 0.0])),   // 1
    ("rightUpperLeg", Some(0), Some([-0.05, 0.62, 0.0])), // 2
    ("leftLowerLeg", Some(1), Some([0.05, 0.35, 0.0])),   // 3
    ("rightLowerLeg", Some(2), Some([-0.05, 0.35, 0.0])), // 4
    ("leftFoot", Some(3), Some([0.05, 0.06, 0.0])),       // 5
    ("rightFoot", Some(4), Some([-0.05, 0.06, 0.0])),     // 6
    ("leftToes", Some(5), Some([0.05, 0.02, 0.06])),      // 7
    ("leftUpperArm", Some(0), Some([0.065, 0.9, 0.0])),   // 8
    ("rightUpperArm", Some(0), Some([-0.065, 0.9, 0.0])), // 9
    ("leftLowerArm", Some(8), Some([0.235, 0.9, 0.0])),   // 10
    ("rightLowerArm", Some(9), Some([-0.235, 0.9, 0.0])), // 11
    ("leftHand", Some(10), Some([0.365, 0.9, 0.0])),      // 12
    ("rightHand", Some(11), Some([-0.365, 0.9, 0.0])),    // 13
    ("head", Some(0), Some([0.0, 0.95, -0.01])),          // 14
    ("leftEye", Some(14), Some([0.03, 1.0, 0.03])),       // 15
    ("rightEye", Some(14), Some([-0.03, 1.0, 0.03])),     // 16
    ("hairFront", Some(14), Some([0.0, 1.05, 0.05])),     // 17
    ("leftFoot", Some(3), None),                          // 18: a second foot, unbound
    ("upper_leg.L", Some(0), Some([0.9, 0.9, 0.9])),      // 19: a second left hip joint, later
];

fn mesh(verts: &[([f64; 3], usize)]) -> RigMesh {
    RigMesh {
        positions: verts.iter().map(|v| v.0).collect(),
        leads: verts.iter().map(|v| Some(v.1)).collect(),
    }
}

fn body() -> RigMesh {
    let mut v: Vec<([f64; 3], usize)> = BONES
        .iter()
        .enumerate()
        .filter_map(|(i, b)| b.2.map(|p| (p, i)))
        .collect();
    v.extend([
        ([0.05, 0.0, 0.08], 7),  // the sole, under the toes
        ([0.0, 0.94, 0.04], 14), // the chin
        ([0.0, 0.9, -0.06], 14), // the nape, behind the head and lower
        ([0.2, 0.91, 0.05], 14), // a cheek wider than the eyes, lower
    ]);
    mesh(&v)
}

fn rig(meshes: Vec<RigMesh>) -> Rig {
    Rig {
        bones: BONES
            .iter()
            .map(|b| RigBone {
                name: b.0.into(),
                parent: b.1,
                bind: b.2,
            })
            .collect(),
        meshes,
    }
}

#[test]
fn the_marks_of_a_rig() {
    let m = body_marks(&rig(vec![body()])).unwrap();
    assert_eq!((m.sole, m.eye, m.chin), (0.0, 1.0, Some(0.94)));
    assert_eq!(m.joints[&(Part::Hips, None)], [0.0, 0.66, 0.0]);
    // the bound foot is kept, not the unbound one named the same; of two bound bones of one
    // part, the first
    assert_eq!(m.joints[&(Part::Foot, Some(Side::Left))], [0.05, 0.06, 0.0]);
    assert_eq!(
        m.joints[&(Part::UpperLeg, Some(Side::Left))],
        [0.05, 0.62, 0.0]
    );
    assert_eq!(measure(&m).unwrap().get("knee"), Some(0.35));
    // a vertex without a leading bone is neither sole nor chin
    let mut b = body();
    b.positions.extend([[0.0, -1.0, 0.1]]);
    b.leads.extend([None]);
    assert_eq!(body_marks(&rig(vec![b])).unwrap(), m);
}

#[test]
fn other_meshes_do_not_move_the_sole() {
    let base = body_marks(&rig(vec![body()])).unwrap();
    // shoes bind two feet, a stand binds nothing named: both below the sole
    let shoes = mesh(&[([0.05, -0.03, 0.08], 7), ([0.05, 0.1, 0.0], 5)]);
    let mut stand = mesh(&[([0.0, -0.4, 0.0], 0)]);
    stand.leads = vec![None];
    for extra in [vec![shoes.clone()], vec![stand.clone()], vec![shoes, stand]] {
        let mut before = vec![body()];
        before.extend(extra.clone());
        let mut after = extra;
        after.extend([body()]);
        for meshes in [before, after] {
            assert_eq!(body_marks(&rig(meshes)).unwrap(), base);
        }
    }
    // the body is the mesh whose vertices follow the most parts, each part once: shoes with many
    // vertices on the feet (more than the body has on named parts) stay shoes
    let mut many: Vec<([f64; 3], usize)> = (0..40)
        .map(|i| ([0.05, -0.03, 0.001 * f64::from(i)], 5))
        .collect();
    many.extend([([-0.05, -0.03, 0.08], 6), ([0.05, 0.1, 0.0], 5)]);
    assert_eq!(body_marks(&rig(vec![body(), mesh(&many)])).unwrap(), base);
    // a mesh that follows no foot is not the body, however many parts it binds
    let hair = mesh(&[([0.0, 0.5, -0.1], 17)]);
    assert_eq!(body_marks(&rig(vec![hair, body()])).unwrap(), base);
}

#[test]
fn the_chin_is_on_the_head_bone_in_front_between_the_eyes() {
    // eyes behind the head: the front is the other way, so the nape is the chin
    let mut r = rig(vec![body()]);
    for i in [15, 16] {
        if let Some(p) = &mut r.bones[i].bind {
            p[2] = -0.05;
        }
    }
    assert_eq!(body_marks(&r).unwrap().chin, Some(0.9));
    // no vertex qualifies: no chin, and the face length is not measured
    let mut b = body();
    b.leads.iter_mut().for_each(|l| {
        if *l == Some(14) {
            *l = Some(0);
        }
    });
    let m = body_marks(&rig(vec![b])).unwrap();
    assert_eq!(m.chin, None);
    assert_eq!(measure(&m).unwrap().get("face_length"), None);
}

#[test]
fn a_rig_without_eyes_head_or_feet_says_so() {
    let without = |names: &[&str]| {
        let mut r = rig(vec![body()]);
        for b in &mut r.bones {
            if names.contains(&b.name.as_str()) {
                b.name = "prop".into();
            }
        }
        body_marks(&r).unwrap_err()
    };
    assert_eq!(
        without(&["leftEye", "rightEye"]),
        MeasureError::Missing("eyes")
    );
    assert_eq!(without(&["head"]), MeasureError::Missing("head"));
    assert_eq!(
        without(&["leftFoot", "rightFoot"]),
        MeasureError::Missing("skinned body with feet")
    );
    // an unbound eye does not count
    let mut r = rig(vec![body()]);
    r.bones[15].bind = None;
    r.bones[16].bind = None;
    assert_eq!(body_marks(&r).unwrap_err(), MeasureError::Missing("eyes"));
    // a parent loop ends
    let mut r = rig(vec![body()]);
    r.bones[0].parent = Some(14);
    r.bones[14].parent = Some(0);
    let _ = body_marks(&r);
}

#[test]
fn bone_names_of_the_usual_rig_conventions() {
    let l = Some(Side::Left);
    let r = Some(Side::Right);
    for (name, want) in [
        ("Hips", Some((Part::Hips, None))),
        ("J_Bip_C_Hips", Some((Part::Hips, None))),
        ("pelvis", Some((Part::Hips, None))),
        ("LeftUpperLeg", Some((Part::UpperLeg, l))),
        ("UpperLeg_R", Some((Part::UpperLeg, r))),
        ("upper_leg.L", Some((Part::UpperLeg, l))),
        ("J_Bip_L_UpperLeg", Some((Part::UpperLeg, l))),
        ("mixamorig:LeftUpLeg", Some((Part::UpperLeg, l))),
        ("mixamorig:RightLeg", Some((Part::LowerLeg, r))),
        ("Character1:LeftUpLeg", Some((Part::UpperLeg, l))),
        ("Lower_leg.L", Some((Part::LowerLeg, l))),
        ("thigh_r", Some((Part::UpperLeg, r))),
        ("Foot.L", Some((Part::Foot, l))),
        ("mixamorig:LeftArm", Some((Part::UpperArm, l))),
        ("LeftForeArm", Some((Part::LowerArm, l))),
        ("Hand_R", Some((Part::Hand, r))),
        ("LeftEye", Some((Part::Eye, l))),
        ("leftEye", Some((Part::Eye, l))),
        ("rightUpperLeg", Some((Part::UpperLeg, r))),
        ("Eye_R", Some((Part::Eye, r))),
        ("J_Adj_L_FaceEye", Some((Part::Eye, l))),
        ("Head", Some((Part::Head, None))),
        ("J_Bip_C_Head", Some((Part::Head, None))),
        // copies, helpers, both sides, a side on a midline part, no side on a paired part
        ("LowerArm.L.001", None),
        ("Lower_arm_support.L", None),
        ("RightUpperLegsub_L", None),
        ("Hips_L", None),
        ("Head_R", None),
        ("Right_Hand_L", None),
        ("L_Hand_Left", Some((Part::Hand, l))),
        ("UpperLeg", None),
        ("Spine", Some((Part::Spine, None))),
        ("Hair_front", None),
        ("", None),
    ] {
        assert_eq!(classify(name), want, "{name:?}");
    }
}
