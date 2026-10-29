//! The roles of bones by name: the trunk, shoulders, toes and breasts as body parts, the roots of
//! the hair, the tail and the ears as roles that are not body parts, and the helper bones of the
//! usual mixamo / Unity humanoid rigs (twist and roll bones, hip helpers, the jaw, end bones)
//!
//! The names follow the usual rig conventions: dotted sides (`upper_leg.L`), underscore sides
//! (`UpperLeg_L`), `Left` / `Right` prefixes, the VRM (`J_Bip_…`) and mixamo (`mixamorig:…`)
//! names, numbered chains (`Tail_1`, `Tail_2`) and chains whose first bone is named `…root`, and
//! copies of a bone (`.001`). The names that must not be read as a role are of the kinds a rig
//! carries besides the body: the rest of a chain, props and costume, helpers of no known kind,
//! and mesh objects that share the node list.
#![allow(clippy::float_cmp)]

use alice_lol_humanoid::measure::{Part, Side};
use alice_lol_humanoid::rig::{classify, role, Finger, Helper, Role};

const L: Option<Side> = Some(Side::Left);
const R: Option<Side> = Some(Side::Right);
const MID: Option<Side> = None;

const fn body(p: Part, s: Option<Side>) -> (Role, Option<Side>) {
    (Role::Body(p), s)
}

/// A name, the role it must have, and why
type Case = (&'static str, Option<(Role, Option<Side>)>, &'static str);

fn cases() -> Vec<Case> {
    let mut v = body_cases();
    v.extend(root_cases());
    v.extend(helper_cases());
    v.extend(no_role_cases());
    v
}

/// The body parts
fn body_cases() -> Vec<Case> {
    vec![
        // the trunk, on the midline
        ("Spine", Some(body(Part::Spine, MID)), "plain name"),
        ("J_Bip_C_Spine", Some(body(Part::Spine, MID)), "VRM spine"),
        (
            "mixamorig:Spine",
            Some(body(Part::Spine, MID)),
            "mixamo spine",
        ),
        ("Chest", Some(body(Part::Chest, MID)), "plain name"),
        ("J_Bip_C_Chest", Some(body(Part::Chest, MID)), "VRM chest"),
        ("Neck", Some(body(Part::Neck, MID)), "plain name"),
        ("J_Bip_C_Neck", Some(body(Part::Neck, MID)), "VRM neck"),
        ("mixamorig:Neck", Some(body(Part::Neck, MID)), "mixamo neck"),
        // paired parts
        ("shoulder.L", Some(body(Part::Shoulder, L)), "dotted side"),
        (
            "Shoulder_R",
            Some(body(Part::Shoulder, R)),
            "underscore side",
        ),
        (
            "J_Bip_L_Shoulder",
            Some(body(Part::Shoulder, L)),
            "VRM shoulder",
        ),
        (
            "mixamorig:RightShoulder",
            Some(body(Part::Shoulder, R)),
            "mixamo shoulder",
        ),
        ("clavicle_l", Some(body(Part::Shoulder, L)), "clavicle"),
        ("toe.R", Some(body(Part::Toe, R)), "dotted side"),
        ("Toe_L", Some(body(Part::Toe, L)), "underscore side"),
        ("J_Bip_L_ToeBase", Some(body(Part::Toe, L)), "VRM toe"),
        (
            "mixamorig:LeftToeBase",
            Some(body(Part::Toe, L)),
            "mixamo toe",
        ),
        ("leftToes", Some(body(Part::Toe, L)), "VRM humanoid name"),
        ("LeftBreast", Some(body(Part::Breast, L)), "a Left prefix"),
        (
            "Breast_R",
            Some(body(Part::Breast, R)),
            "a single breast bone",
        ),
        // the parts the measure already read, in both side conventions
        ("upper_leg.L", Some(body(Part::UpperLeg, L)), "dotted side"),
        (
            "UpperLeg_L",
            Some(body(Part::UpperLeg, L)),
            "underscore side",
        ),
        ("lower_arm.R", Some(body(Part::LowerArm, R)), "dotted side"),
        (
            "LowerArm_R",
            Some(body(Part::LowerArm, R)),
            "underscore side",
        ),
        ("LeftEye", Some(body(Part::Eye, L)), "Left prefix"),
        ("Eye_L", Some(body(Part::Eye, L)), "underscore side"),
    ]
}

/// The roots of the chains that are not the body
fn root_cases() -> Vec<Case> {
    vec![
        (
            "HairRoot",
            Some((Role::HairRoot, MID)),
            "the hair's root, named as a root",
        ),
        (
            "Hair",
            Some((Role::HairRoot, MID)),
            "the hair's root, named by the part",
        ),
        (
            "Tail_1",
            Some((Role::TailRoot, MID)),
            "the first bone of a numbered tail chain",
        ),
        (
            "tail",
            Some((Role::TailRoot, MID)),
            "a tail named by the part",
        ),
        (
            "TailRoot",
            Some((Role::TailRoot, MID)),
            "a tail chain named by its root",
        ),
        (
            "Ear_1.L",
            Some((Role::EarRoot, L)),
            "the first bone of a numbered ear chain",
        ),
        (
            "ear_root_L",
            Some((Role::EarRoot, L)),
            "an ear chain named by its root",
        ),
        (
            "Kemomimi_L",
            Some((Role::EarRoot, L)),
            "an animal ear, by its usual name",
        ),
        (
            "nekomimi.R",
            Some((Role::EarRoot, R)),
            "a cat ear, by its usual name",
        ),
    ]
}

/// The helper bones of the usual mixamo / Unity humanoid rigs
fn helper_cases() -> Vec<Case> {
    let h = |x: Helper, s: Option<Side>| Some((Role::Helper(x), s));
    vec![
        (
            "LeftArmRoll",
            h(Helper::ArmRoll, L),
            "mixamo-style arm roll",
        ),
        (
            "mixamorig:RightArmRoll",
            h(Helper::ArmRoll, R),
            "with the mixamo prefix",
        ),
        ("UpperArmRoll.R", h(Helper::ArmRoll, R), "dotted side"),
        (
            "LeftForeArmTwist",
            h(Helper::ForeArmTwist, L),
            "forearm twist",
        ),
        (
            "LowerArmTwist_R",
            h(Helper::ForeArmTwist, R),
            "lower-arm spelling",
        ),
        (
            "ForeArmRoll.L",
            h(Helper::ForeArmTwist, L),
            "forearm roll is the same helper",
        ),
        ("RightUpLegRoll", h(Helper::UpLegRoll, R), "thigh roll"),
        (
            "UpperLegRoll_L",
            h(Helper::UpLegRoll, L),
            "upper-leg spelling",
        ),
        ("Thigh_twist.R", h(Helper::UpLegRoll, R), "thigh twist"),
        (
            "LeftHip",
            h(Helper::HipHelper(0), L),
            "the first hip helper",
        ),
        (
            "LeftHip1",
            h(Helper::HipHelper(1), L),
            "the second hip helper",
        ),
        (
            "RightHip2",
            h(Helper::HipHelper(2), R),
            "the third hip helper",
        ),
        ("Jaw", h(Helper::Jaw, MID), "the jaw"),
        ("Jaw_END", h(Helper::JawEnd, MID), "the jaw's end bone"),
        ("LeftToe_End", h(Helper::ToeEnd, L), "mixamo toe end"),
        ("RightToeBase_End", h(Helper::ToeEnd, R), "toe-base end"),
        (
            "LeftHandThumb4",
            h(Helper::FingerTip(Finger::Thumb), L),
            "thumb tip",
        ),
        (
            "LeftHandIndex4",
            h(Helper::FingerTip(Finger::Index), L),
            "index tip",
        ),
        (
            "RightHandMiddle4",
            h(Helper::FingerTip(Finger::Middle), R),
            "middle tip",
        ),
        (
            "mixamorig:RightHandRing4",
            h(Helper::FingerTip(Finger::Ring), R),
            "ring tip, prefixed",
        ),
        (
            "LeftHandPinky4",
            h(Helper::FingerTip(Finger::Pinky), L),
            "pinky tip",
        ),
        (
            "BounceRoot",
            h(Helper::SwayRoot, MID),
            "the root of swinging parts",
        ),
        // the side rules hold for helpers too, and the body names are unchanged
        ("Jaw.L", None, "the jaw has no side"),
        ("ArmRoll", None, "an arm roll needs a side"),
        ("BounceRoot_R", None, "the sway root has no side"),
        ("LeftHip3", None, "a fourth hip helper is not read"),
        (
            "LeftHandIndex3",
            None,
            "a finger's third bone is not its end",
        ),
        (
            "Hips",
            Some(body(Part::Hips, MID)),
            "the pelvis stays the pelvis",
        ),
    ]
}

/// The names that are no role: the rest of a chain, props, costume and helpers of no known kind,
/// mesh objects, and a side where none belongs
fn no_role_cases() -> Vec<Case> {
    vec![
        // the rest of a chain is not its root
        ("Tail_2", None, "a tail bone after the first"),
        ("Tail_3", None, "a later tail bone"),
        ("Ear_2.L", None, "an ear bone after the first"),
        ("Ear_3.R", None, "a later ear bone"),
        ("BreastTip.L", None, "the next bone of a breast chain"),
        ("Spine.001", None, "a copy of the spine, not a second spine"),
        (
            "mixamorig:Spine1",
            None,
            "mixamo: the second spine bone is not the spine",
        ),
        (
            "J_Bip_C_UpperChest",
            None,
            "the upper chest is not the chest",
        ),
        ("Fringe_1", None, "a hair strand, not the hair's root"),
        ("BunRoot", None, "a bun's root, not the hair's root"),
        ("hair_strand.L", None, "a hair strand"),
        // a prop named after parts, each name one word, is not the body nor its ears
        ("plushear.L", None, "a toy's ear"),
        ("plushleg.R", None, "a toy's leg"),
        ("plusharm.L", None, "a toy's arm"),
        ("PonytailRoot", None, "a ponytail's root is not a tail"),
        // costume and helper bones
        ("Sleeve.L", None, "a sleeve bone"),
        ("Hand_helper.L", None, "a hand helper"),
        ("UpperLegbow", None, "a bow on the thigh"),
        ("Collar_1.L", None, "a collar bone of the costume"),
        (
            "Hip_helper.L",
            Some((Role::Helper(Helper::HipHelper(0)), L)),
            "a hip helper, named as such",
        ),
        ("Bow_root", None, "a costume root"),
        (
            "Upper_arm_twist.L",
            Some((Role::Helper(Helper::ArmRoll), L)),
            "the upper arm's twist helper",
        ),
        ("Pinky1.R", None, "a finger, not read"),
        // mesh objects that share a node list with the bones
        ("Tail_mesh", None, "a tail mesh object"),
        ("Ear_mesh", None, "an ear mesh object"),
        ("Fringe_mesh", None, "a fringe mesh object"),
        // a side on a midline role, no side on a paired one, both sides
        ("Spine_L", None, "the spine has no side"),
        ("Tail_1.L", None, "the tail has no side"),
        ("Hair_root_R", None, "the hair's root has no side"),
        ("Shoulder", None, "a shoulder needs a side"),
        ("kemomimi", None, "an ear needs a side"),
        ("Left_Toe_R", None, "two sides"),
    ]
}

#[test]
fn bone_roles_of_the_usual_rig_conventions() {
    let mut wrong = Vec::new();
    for (name, want, why) in cases() {
        let got = role(name);
        if got != want {
            wrong.push(format!("{name:?} ({why}): want {want:?}, got {got:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} names read wrong:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn classify_is_the_body_part_of_a_role() {
    let mut compared = 0;
    for (name, want, _) in cases() {
        let body = want.and_then(|(r, s)| match r {
            Role::Body(p) => Some((p, s)),
            _ => None,
        });
        assert_eq!(classify(name), body, "{name:?}: classify must be the role's body part, or None for a root that is not the body");
        compared += 1;
    }
    assert!(compared > 50, "compared {compared}");
}

/// The marks read only the parts a measure reads: a suit whose vertices follow the trunk,
/// shoulders, breasts and toes (more parts than the body has, counted with them) and one foot
/// stays a suit, and the trunk's bones are not marks
#[test]
fn the_new_parts_do_not_change_the_marks_nor_the_body_mesh() {
    use alice_lol_humanoid::measure::MeasureError;
    use alice_lol_humanoid::rig::{body_marks, Rig, RigBone, RigMesh};
    let bones: &[(&str, Option<usize>, [f64; 3])] = &[
        ("Hips", None, [0.0, 0.66, 0.0]),             // 0
        ("UpperLeg_L", Some(0), [0.05, 0.62, 0.0]),   // 1
        ("UpperLeg_R", Some(0), [-0.05, 0.62, 0.0]),  // 2
        ("LowerLeg_L", Some(1), [0.05, 0.35, 0.0]),   // 3
        ("LowerLeg_R", Some(2), [-0.05, 0.35, 0.0]),  // 4
        ("Foot_L", Some(3), [0.05, 0.06, 0.0]),       // 5
        ("Foot_R", Some(4), [-0.05, 0.06, 0.0]),      // 6
        ("Head", Some(0), [0.0, 0.95, -0.01]),        // 7
        ("Eye_L", Some(7), [0.03, 1.0, 0.03]),        // 8
        ("Eye_R", Some(7), [-0.03, 1.0, 0.03]),       // 9
        ("Spine", Some(0), [0.0, 0.72, 0.0]),         // 10
        ("Chest", Some(10), [0.0, 0.8, 0.0]),         // 11
        ("Neck", Some(11), [0.0, 0.9, 0.0]),          // 12
        ("Shoulder_L", Some(11), [0.03, 0.88, 0.0]),  // 13
        ("Shoulder_R", Some(11), [-0.03, 0.88, 0.0]), // 14
        ("Breast_L", Some(11), [0.04, 0.82, 0.05]),   // 15
        ("Breast_R", Some(11), [-0.04, 0.82, 0.05]),  // 16
        ("Toe_L", Some(5), [0.05, 0.02, 0.06]),       // 17
        ("Toe_R", Some(6), [-0.05, 0.02, 0.06]),      // 18
    ];
    let mesh = |v: &[([f64; 3], usize)]| RigMesh {
        positions: v.iter().map(|x| x.0).collect(),
        leads: v.iter().map(|x| Some(x.1)).collect(),
    };
    // the body: 8 measured parts, its sole at 0
    let body = mesh(&[
        ([0.0, 0.66, 0.0], 0),
        ([0.05, 0.62, 0.0], 1),
        ([-0.05, 0.62, 0.0], 2),
        ([0.05, 0.35, 0.0], 3),
        ([-0.05, 0.35, 0.0], 4),
        ([0.05, 0.0, 0.0], 5),
        ([-0.05, 0.0, 0.0], 6),
        ([0.0, 0.94, 0.04], 7),
    ]);
    // the suit: one measured part (a foot) and the 9 new ones, its sole lower
    let mut suit_v = vec![([0.05, -0.01, 0.0], 5)];
    suit_v.extend((10..=18).map(|b| (bones[b].2, b)));
    let suit = mesh(&suit_v);
    let rig = |meshes: Vec<RigMesh>| Rig {
        bones: bones
            .iter()
            .map(|b| RigBone {
                name: b.0.into(),
                parent: b.1,
                bind: Some(b.2),
            })
            .collect(),
        meshes,
    };
    let alone = body_marks(&rig(vec![body.clone()])).unwrap();
    for meshes in [vec![body.clone(), suit.clone()], vec![suit.clone(), body]] {
        let m = body_marks(&rig(meshes)).unwrap();
        assert_eq!(
            m.sole, 0.0,
            "the suit (1 measured part, 9 unmeasured) must not be taken for the body"
        );
        assert_eq!(m, alone, "the suit must not change any mark");
    }
    assert!(
        alone.joints.keys().all(|k| k.0.is_measured()),
        "the marks hold only measured parts: {:?}",
        alone.joints.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        alone.joints.len(),
        7,
        "hips + 2 hip joints + 2 knees + 2 ankles"
    );
    // the suit alone has a foot, so it is the body then (the rule is unchanged)
    assert!(!matches!(
        body_marks(&rig(vec![suit])),
        Err(MeasureError::Missing(_))
    ));
}
