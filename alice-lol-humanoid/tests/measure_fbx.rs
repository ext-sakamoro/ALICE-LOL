//! Marking a body read from an FBX scene: bone names, the sole, the eye line and the chin
//!
//! The scenes are built in code: a small rig, a body mesh skinned to it, and the meshes that must
//! not move the measurement (shoes, a stand below the sole, a face cut at the hairline, hair).
#![cfg(feature = "fbx")]
#![allow(clippy::unwrap_used, clippy::float_cmp)]

use alice_fbx::scene::{Axis, Cluster, Mesh, Model, RotationOrder, Settings};
use alice_fbx::Scene;
use alice_lol_humanoid::fbx::body_marks;
use alice_lol_humanoid::measure::{measure, MeasureError, Part, Side};

fn model(name: &str, parent: Option<usize>) -> Model {
    Model {
        id: 0,
        name: name.into(),
        kind: "LimbNode".into(),
        parent,
        translation: [0.0; 3],
        rotation: [0.0; 3],
        scaling: [1.0; 3],
        pre_rotation: [0.0; 3],
        post_rotation: [0.0; 3],
        rotation_offset: [0.0; 3],
        rotation_pivot: [0.0; 3],
        scaling_offset: [0.0; 3],
        scaling_pivot: [0.0; 3],
        rotation_order: RotationOrder::Xyz,
    }
}

fn translate(p: [f64; 3]) -> [f64; 16] {
    let mut m = [0.0; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m[12..15].copy_from_slice(&p);
    m
}

/// A y-up rig: name, parent, bind position (x left, y up, z forward)
const RIG: &[(&str, Option<usize>, [f64; 3])] = &[
    ("Hips", None, [0.0, 0.66, 0.0]),                   // 0
    ("LeftUpperLeg", Some(0), [0.05, 0.62, 0.0]),       // 1
    ("RightUpperLeg", Some(0), [-0.05, 0.62, 0.0]),     // 2
    ("LeftLowerLeg", Some(1), [0.05, 0.35, 0.0]),       // 3
    ("RightLowerLeg", Some(2), [-0.05, 0.35, 0.0]),     // 4
    ("LeftFoot", Some(3), [0.05, 0.06, 0.0]),           // 5
    ("RightFoot", Some(4), [-0.05, 0.06, 0.0]),         // 6
    ("LeftToes", Some(5), [0.05, 0.02, 0.06]),          // 7
    ("Spine", Some(0), [0.0, 0.75, 0.0]),               // 8
    ("LeftUpperArm", Some(8), [0.065, 0.9, 0.0]),       // 9
    ("RightUpperArm", Some(8), [-0.065, 0.9, 0.0]),     // 10
    ("LeftLowerArm", Some(9), [0.235, 0.9, 0.0]),       // 11
    ("RightLowerArm", Some(10), [-0.235, 0.9, 0.0]),    // 12
    ("LeftHand", Some(11), [0.365, 0.9, 0.0]),          // 13
    ("RightHand", Some(12), [-0.365, 0.9, 0.0]),        // 14
    ("Head", Some(8), [0.0, 0.95, -0.01]),              // 15
    ("LeftEye", Some(15), [0.03, 1.0, 0.03]),           // 16
    ("RightEye", Some(15), [-0.03, 1.0, 0.03]),         // 17
    ("Hair_front", Some(15), [0.0, 1.05, 0.05]),        // 18
    ("Lower_arm_support.L", Some(11), [0.3, 0.8, 0.0]), // 19: a helper, not a joint
];

/// A skinned mesh: each vertex follows one bone fully
fn mesh(name: &str, verts: &[([f64; 3], usize)]) -> Mesh {
    let mut skin: Vec<Cluster> = Vec::new();
    for (i, &(_, b)) in verts.iter().enumerate() {
        match skin.iter_mut().find(|c| c.bone == Some(b)) {
            Some(c) => {
                c.indices.extend([u32::try_from(i).unwrap()]);
                c.weights.extend([1.0]);
            }
            None => skin.extend([Cluster {
                bone: Some(b),
                indices: vec![u32::try_from(i).unwrap()],
                weights: vec![1.0],
                transform: translate([0.0; 3]),
                transform_link: translate(RIG[b].2),
            }]),
        }
    }
    Mesh {
        id: 0,
        name: name.into(),
        model: None,
        positions: verts.iter().map(|v| v.0).collect(),
        polygon_vertices: Vec::new(),
        polygon_starts: Vec::new(),
        normals: None,
        uvs: Vec::new(),
        polygon_materials: None,
        materials: Vec::new(),
        skin: Some(skin),
        blend_channels: Vec::new(),
    }
}

const fn y_up() -> Settings {
    Settings {
        up: Axis { axis: 1, sign: 1 },
        front: Axis { axis: 2, sign: 1 },
        coord: Axis { axis: 0, sign: 1 },
        unit_scale: 1.0,
    }
}

/// The body: one vertex on every named bone, the sole under the toes, the chin in front of the head
fn body() -> Mesh {
    let mut v: Vec<([f64; 3], usize)> = RIG.iter().enumerate().map(|(i, r)| (r.2, i)).collect();
    v.extend([
        ([0.05, 0.0, 0.08], 7),  // the sole, under the toes
        ([0.0, 0.94, 0.04], 15), // the chin
        ([0.0, 0.9, -0.06], 15), // the nape, behind the head and lower than the chin
        ([0.2, 0.91, 0.05], 15), // a cheek wider than the eyes, lower than the chin
    ]);
    mesh("body", &v)
}

fn scene(meshes: Vec<Mesh>) -> Scene {
    Scene {
        settings: y_up(),
        models: RIG.iter().map(|r| model(r.0, r.1)).collect(),
        meshes,
        materials: Vec::new(),
    }
}

#[test]
fn the_marks_come_from_the_bones_the_feet_and_the_face() {
    let m = body_marks(&scene(vec![body()])).unwrap();
    assert_eq!(m.sole, 0.0);
    assert_eq!(m.eye, 1.0);
    assert_eq!(m.chin, Some(0.94));
    assert_eq!(m.joints[&(Part::Hips, None)], [0.0, 0.66, 0.0]);
    assert_eq!(
        m.joints[&(Part::LowerArm, Some(Side::Left))],
        [0.235, 0.9, 0.0]
    );
    assert_eq!(
        m.joints[&(Part::Hand, Some(Side::Right))],
        [-0.365, 0.9, 0.0]
    );
    let got = measure(&m).unwrap();
    assert_eq!(got.get("knee"), Some(0.35));
    assert_eq!(got.get("hips_y"), Some(0.66));
    assert_eq!(got.get("face_length"), Some(1.0 - 0.94));
}

#[test]
fn shoes_and_a_stand_below_the_sole_do_not_move_it() {
    let base = measure(&body_marks(&scene(vec![body()])).unwrap()).unwrap();
    // shoes follow the feet but only the legs' bones; a stand follows nothing named
    let shoes = mesh("shoes", &[([0.05, -0.03, 0.08], 7), ([0.05, 0.1, 0.0], 5)]);
    let stand = mesh("stand", &[([0.0, -0.4, 0.0], 0)]);
    let mut unskinned = mesh("plate", &[([0.0, -0.6, 0.0], 0)]);
    unskinned.skin = None;
    for extra in [
        vec![shoes.clone()],
        vec![stand.clone(), unskinned.clone()],
        vec![shoes, stand, unskinned],
    ] {
        // the body before and after the other meshes
        let mut after = extra.clone();
        after.extend([body()]);
        let mut before = vec![body()];
        before.extend(extra);
        for meshes in [after, before] {
            let got = measure(&body_marks(&scene(meshes)).unwrap()).unwrap();
            assert_eq!(got, base);
        }
    }
}

#[test]
fn a_face_cut_at_the_hairline_and_the_hair_do_not_move_the_measure() {
    let base = measure(&body_marks(&scene(vec![body()])).unwrap()).unwrap();
    // a face mesh that ends at the hairline, one that runs over the head, and hair hanging low
    let cut = mesh("face", &[([0.0, 0.95, 0.05], 15), ([0.0, 1.06, 0.05], 15)]);
    let full = mesh("face", &[([0.0, 0.95, 0.05], 15), ([0.0, 1.2, 0.0], 15)]);
    let hair = mesh("hair", &[([0.0, 0.5, -0.1], 18), ([0.0, 0.8, 0.06], 18)]);
    for extra in [cut, full, hair] {
        let got = measure(&body_marks(&scene(vec![body(), extra])).unwrap()).unwrap();
        assert_eq!(got, base);
    }
}

#[test]
fn a_z_up_scene_measures_the_same() {
    let base = measure(&body_marks(&scene(vec![body()])).unwrap()).unwrap();
    // y up, z forward  ->  z up, y forward (negated), with the bind poses moved alike
    let swap = |p: [f64; 3]| [p[0], -p[2], p[1]];
    let mut s = scene(vec![body()]);
    s.settings.up = Axis { axis: 2, sign: 1 };
    s.settings.front = Axis { axis: 1, sign: -1 };
    for m in &mut s.meshes {
        for p in &mut m.positions {
            *p = swap(*p);
        }
        for c in m.skin.iter_mut().flatten() {
            let t = swap([
                c.transform_link[12],
                c.transform_link[13],
                c.transform_link[14],
            ]);
            c.transform_link[12..15].copy_from_slice(&t);
        }
    }
    assert_eq!(measure(&body_marks(&s).unwrap()).unwrap(), base);
}

#[test]
fn an_up_axis_pointing_down_the_file_measures_the_same() {
    let base = measure(&body_marks(&scene(vec![body()])).unwrap()).unwrap();
    let flip = |p: [f64; 3]| [p[0], -p[1], p[2]];
    let mut s = scene(vec![body()]);
    s.settings.up = Axis { axis: 1, sign: -1 };
    for m in &mut s.meshes {
        for p in &mut m.positions {
            *p = flip(*p);
        }
        for c in m.skin.iter_mut().flatten() {
            let t = flip([
                c.transform_link[12],
                c.transform_link[13],
                c.transform_link[14],
            ]);
            c.transform_link[12..15].copy_from_slice(&t);
        }
    }
    assert_eq!(measure(&body_marks(&s).unwrap()).unwrap(), base);
}

#[test]
fn a_vertex_follows_the_bone_that_weighs_most() {
    // a vertex below the sole that the shin weighs 0.7 and the toes 0.3: not part of the foot
    let mut b = body();
    let i = u32::try_from(b.positions.len()).unwrap();
    b.positions.extend([[0.05, -0.02, 0.0]]);
    for c in b.skin.iter_mut().flatten() {
        let w = match c.bone {
            Some(3) => 0.7,
            Some(7) => 0.3,
            _ => continue,
        };
        c.indices.extend([i]);
        c.weights.extend([w]);
    }
    assert_eq!(body_marks(&scene(vec![b])).unwrap().sole, 0.0);
}

#[test]
fn a_scene_without_eyes_head_or_feet_says_so() {
    let without = |names: &[&str]| {
        let mut s = scene(vec![body()]);
        for m in &mut s.models {
            if names.contains(&m.name.as_str()) {
                m.name = "Prop".into();
            }
        }
        body_marks(&s).unwrap_err()
    };
    assert_eq!(
        without(&["LeftEye", "RightEye"]),
        MeasureError::Missing("eyes")
    );
    assert_eq!(without(&["Head"]), MeasureError::Missing("head"));
    assert_eq!(
        without(&["LeftFoot", "RightFoot"]),
        MeasureError::Missing("skinned body with feet")
    );
    // one eye is enough for the eye line
    assert_eq!(
        body_marks(&{
            let mut s = scene(vec![body()]);
            s.models[17].name = "Prop".into();
            s
        })
        .unwrap()
        .eye,
        1.0
    );
}

#[test]
fn a_bone_is_bound_where_it_is_first_bound_and_a_mesh_sits_where_its_model_puts_it() {
    let base = body_marks(&scene(vec![body()])).unwrap();
    // a later mesh binds the hips somewhere else: the first binding stands
    let mut odd = mesh("odd", &[([0.0, 0.66, 0.0], 0)]);
    for c in odd.skin.iter_mut().flatten() {
        c.transform_link = translate([0.3, 0.2, 0.1]);
    }
    assert_eq!(body_marks(&scene(vec![body(), odd])).unwrap(), base);
    // the body mesh under a model moved up by 0.5: its vertices move, the bones do not
    let mut s = scene(vec![body()]);
    let mut lift = model("BodyMesh", None);
    lift.kind = "Mesh".into();
    lift.translation = [0.0, 0.5, 0.0];
    s.models.extend([lift]);
    s.meshes[0].model = Some(s.models.len() - 1);
    let m = body_marks(&s).unwrap();
    assert_eq!(m.sole, 0.5);
    assert_eq!(m.chin, Some(0.94 + 0.5));
    assert_eq!(m.eye, base.eye);
}

#[test]
fn a_skin_that_lists_every_bone_with_zero_weights_does_not_make_shoes_the_body() {
    let base = body_marks(&scene(vec![body()])).unwrap();
    // shoes below the sole whose skin lists every bone of the rig, all but the feet at weight 0
    let mut shoes = mesh(
        "shoes",
        &[([0.05, -0.03, 0.08], 7), ([-0.05, -0.03, 0.08], 6)],
    );
    let skin = shoes.skin.as_mut().unwrap();
    for (b, row) in RIG.iter().enumerate() {
        if !skin.iter().any(|c| c.bone == Some(b)) {
            skin.extend([Cluster {
                bone: Some(b),
                indices: vec![0, 1],
                weights: vec![0.0, 0.0],
                transform: translate([0.0; 3]),
                transform_link: translate(row.2),
            }]);
        }
    }
    for meshes in [vec![shoes.clone(), body()], vec![body(), shoes]] {
        assert_eq!(body_marks(&scene(meshes)).unwrap(), base);
    }
}
