//! # alice-lol-humanoid
//!
//! Humanoid template for `alice_lol` DSL parametric character generation
//!
//! `~/ALICE-LOL/docs/HUMANOID_TEMPLATE_ROADMAP.md` の Phase H.0-H.6 に沿って段階実装
//! 現在 Phase = **H.4 BVH import + pose** (BVH parser + `HumanoidTemplate::with_pose` FK)
//!
//! # Roadmap
//!
//! - H.0 Scaffolding (完了)
//! - H.1 Static template (完了)
//! - H.2 Parametric morphology (完了)
//! - H.3 VRM import (完了)
//! - **H.4 BVH import + pose** (本 Phase、`bvh` module 常時 available + `with_pose` FK)
//! - H.5 ALICE-Manga との duplication 整理
//! - H.6 Intent 結線 (`apply_intent(&IntentNode)`)
//!
//! # 三相原理での位置付け
//!
//! Phase 2 Law (parametric morphology) → Phase 3 Intent (verb) の橋渡し
//! 詳細: `docs/HUMANOID_TEMPLATE_DESIGN.md` §1
//!
//! # H.1 / H.2 実装ノート (duplication)
//!
//! `Joint` enum / `Bone` struct / `MuscleWidths` / bones topology は
//! ALICE-Manga `src/skeleton.rs` + `src/skeleton3d.rs` の code copy
//! Phase H.5 で reconciliation (併存 / wrapper 化 / 段階移行) を user 判断
//!
//! # Quick start
//!
//! ```
//! use alice_lol_humanoid::{HumanoidTemplate, MorphologyParams};
//!
//! // canonical (H.1 hardcoded 相当、10 頭身)
//! let default_template = HumanoidTemplate::default();
//!
//! // chibi 3 頭身 (SD 体型)
//! let chibi = HumanoidTemplate::from_morphology(&MorphologyParams::chibi());
//!
//! // builder chain
//! let custom = HumanoidTemplate::builder()
//!     .height(1.7)
//!     .head_body_ratio(7.5)
//!     .build();
//!
//! let sdf = default_template.to_sdf(0.15);
//! ```

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;

use alice_lol::{SdfNode, Vec3};
use glam::{Mat4, Quat};

pub mod bvh;

#[cfg(feature = "vrm")]
pub mod vrm;

// ============================================================================
// Joint
// ============================================================================

/// 16 主要 joint (VRM humanoid 相当、`body_25` subset)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Joint {
    /// 頭頂
    Head,
    /// 首
    Neck,
    /// 胸中央
    Chest,
    /// 腰 (VRM `hips` 相当)
    Waist,
    /// 左肩
    LShoulder,
    /// 右肩
    RShoulder,
    /// 左肘
    LElbow,
    /// 右肘
    RElbow,
    /// 左手首
    LWrist,
    /// 右手首
    RWrist,
    /// 左股関節
    LHip,
    /// 右股関節
    RHip,
    /// 左膝
    LKnee,
    /// 右膝
    RKnee,
    /// 左足首
    LAnkle,
    /// 右足首
    RAnkle,
}

impl Joint {
    /// 全 16 joint の順序固定 array (iteration 用)
    pub const ALL: [Self; 16] = [
        Self::Head,
        Self::Neck,
        Self::Chest,
        Self::Waist,
        Self::LShoulder,
        Self::RShoulder,
        Self::LElbow,
        Self::RElbow,
        Self::LWrist,
        Self::RWrist,
        Self::LHip,
        Self::RHip,
        Self::LKnee,
        Self::RKnee,
        Self::LAnkle,
        Self::RAnkle,
    ];
}

// ============================================================================
// VRM humanoid parent-child hierarchy (pub(crate)、H.3 vrm + H.4 with_pose 共用)
// ============================================================================

/// VRM humanoid の parent-child hierarchy (Waist が root、他 15 joint は parent を持つ)
/// forward kinematics で必要
pub(crate) const VRM_PARENT_TABLE: &[(Joint, Joint)] = &[
    (Joint::Chest, Joint::Waist),
    (Joint::Neck, Joint::Chest),
    (Joint::Head, Joint::Neck),
    (Joint::LShoulder, Joint::Chest),
    (Joint::LElbow, Joint::LShoulder),
    (Joint::LWrist, Joint::LElbow),
    (Joint::RShoulder, Joint::Chest),
    (Joint::RElbow, Joint::RShoulder),
    (Joint::RWrist, Joint::RElbow),
    (Joint::LHip, Joint::Waist),
    (Joint::LKnee, Joint::LHip),
    (Joint::LAnkle, Joint::LKnee),
    (Joint::RHip, Joint::Waist),
    (Joint::RKnee, Joint::RHip),
    (Joint::RAnkle, Joint::RKnee),
];

/// FK walk 用 topological order (parent が先、child が後、BFS from Waist)
pub(crate) const VRM_TOPO_ORDER: &[Joint] = &[
    Joint::Waist,
    Joint::Chest,
    Joint::LHip,
    Joint::RHip,
    Joint::Neck,
    Joint::LShoulder,
    Joint::RShoulder,
    Joint::LKnee,
    Joint::RKnee,
    Joint::Head,
    Joint::LElbow,
    Joint::RElbow,
    Joint::LAnkle,
    Joint::RAnkle,
    Joint::LWrist,
    Joint::RWrist,
];

/// [`Joint`] → VRM bone name の逆引き
#[must_use]
pub(crate) const fn joint_to_vrm_name(j: Joint) -> &'static str {
    match j {
        Joint::Head => "head",
        Joint::Neck => "neck",
        Joint::Chest => "chest",
        Joint::Waist => "hips",
        Joint::LShoulder => "leftUpperArm",
        Joint::RShoulder => "rightUpperArm",
        Joint::LElbow => "leftLowerArm",
        Joint::RElbow => "rightLowerArm",
        Joint::LWrist => "leftHand",
        Joint::RWrist => "rightHand",
        Joint::LHip => "leftUpperLeg",
        Joint::RHip => "rightUpperLeg",
        Joint::LKnee => "leftLowerLeg",
        Joint::RKnee => "rightLowerLeg",
        Joint::LAnkle => "leftFoot",
        Joint::RAnkle => "rightFoot",
    }
}

// ============================================================================
// Bone
// ============================================================================

/// 2 つの joint を結ぶ骨
#[derive(Debug, Clone, Copy)]
pub struct Bone {
    /// 始点 joint
    pub from: Joint,
    /// 終点 joint
    pub to: Joint,
    /// bone base thickness ([`MuscleWidths`] 分類より上書きされる fallback 値)
    pub thickness: f32,
}

// ============================================================================
// MuscleWidths
// ============================================================================

/// 部位別 thickness preset (limb / torso / head)
///
/// [`HumanoidTemplate::to_sdf`] 内で [`Bone`] の from/to から部位分類し対応 thickness を
/// `bone.thickness` より優先して適用する
#[derive(Debug, Clone, Copy)]
pub struct MuscleWidths {
    /// 手足 (limb) の thickness
    pub limb: f32,
    /// 胴体 (torso) の thickness
    pub torso: f32,
    /// 頭 / 首の thickness
    pub head: f32,
}

impl MuscleWidths {
    /// 少年漫画キャラ preset (limb=0.35, torso=0.6, head=0.4)
    #[must_use]
    pub const fn shounen() -> Self {
        Self {
            limb: 0.35,
            torso: 0.6,
            head: 0.4,
        }
    }

    /// SD / chibi キャラ preset (limb=0.5, torso=0.85, head=0.5)
    #[must_use]
    pub const fn chibi() -> Self {
        Self {
            limb: 0.5,
            torso: 0.85,
            head: 0.5,
        }
    }

    /// 細身女性キャラ preset (limb=0.25, torso=0.45, head=0.3)
    #[must_use]
    pub const fn slim() -> Self {
        Self {
            limb: 0.25,
            torso: 0.45,
            head: 0.3,
        }
    }
}

impl Default for MuscleWidths {
    fn default() -> Self {
        Self::shounen()
    }
}

// ============================================================================
// MorphologyParams (H.2)
// ============================================================================

/// 骨格比パラメータ (parametric law) から humanoid joint 位置を数式導出する
///
/// # 座標系
///
/// 右手系、Y up、Waist が原点、canonical T-pose (両腕水平、Z=0)
///
/// # 各 ratio の意味
///
/// - `height`: 全身高 (Y span、canonical 5.0、real-world 1.7 m 等の任意単位)
/// - `head_body_ratio`: 頭身 (head span = height / n、chibi=3、adult=7.5、hero=10)
/// - `arm_ratio`: 腕長 / 身長 per side (肩から手首の直線距離、canonical 0.36)
/// - `shoulder_ratio`: 肩幅 / 身長 (両肩間の距離、canonical 0.24)
/// - `hip_ratio`: 骨盤幅 / 身長 (両股関節間の距離、canonical 0.16)
/// - `leg_ratio`: 脚長 / 身長 (股関節から足首、canonical 0.5)
///
/// # 導出 formula (Waist 原点、Y up)
///
/// ```text
/// head_top_y = h * 0.5
/// head_span  = h / n
/// neck_y     = head_top_y - head_span
/// chest_y    = neck_y * 0.75
/// ankle_y    = -h * leg_ratio
/// knee_y     = ankle_y * 0.52
/// shoulder_x = h * shoulder_ratio * 0.5
/// arm_len    = h * arm_ratio
/// elbow_x    = shoulder_x + arm_len * 0.5
/// wrist_x    = shoulder_x + arm_len
/// hip_x      = h * hip_ratio * 0.5
/// ```
#[derive(Debug, Clone, Copy)]
pub struct MorphologyParams {
    /// 全身高
    pub height: f32,
    /// 頭身
    pub head_body_ratio: f32,
    /// 腕長 / 身長 (per side)
    pub arm_ratio: f32,
    /// 肩幅 / 身長
    pub shoulder_ratio: f32,
    /// 骨盤幅 / 身長
    pub hip_ratio: f32,
    /// 脚長 / 身長
    pub leg_ratio: f32,
}

impl MorphologyParams {
    /// adult 7.5 頭身 preset (real body 相当、height=5.0)
    #[must_use]
    pub const fn adult() -> Self {
        Self {
            height: 5.0,
            head_body_ratio: 7.5,
            arm_ratio: 0.38,
            shoulder_ratio: 0.23,
            hip_ratio: 0.16,
            leg_ratio: 0.52,
        }
    }

    /// chibi 3 頭身 preset (SD 体型、大頭 / short limb、height=5.0)
    #[must_use]
    pub const fn chibi() -> Self {
        Self {
            height: 5.0,
            head_body_ratio: 3.0,
            arm_ratio: 0.28,
            shoulder_ratio: 0.28,
            hip_ratio: 0.22,
            leg_ratio: 0.4,
        }
    }

    /// hero 8 頭身 preset (少年漫画ヒーロー、height=5.0)
    #[must_use]
    pub const fn hero() -> Self {
        Self {
            height: 5.0,
            head_body_ratio: 8.0,
            arm_ratio: 0.4,
            shoulder_ratio: 0.25,
            hip_ratio: 0.16,
            leg_ratio: 0.52,
        }
    }
}

impl Default for MorphologyParams {
    /// canonical (H.1 hardcoded joint 位置と 100% 一致、10 頭身、height=5.0)
    fn default() -> Self {
        Self {
            height: 5.0,
            head_body_ratio: 10.0,
            arm_ratio: 0.36,
            shoulder_ratio: 0.24,
            hip_ratio: 0.16,
            leg_ratio: 0.5,
        }
    }
}

// ============================================================================
// HumanoidTemplate
// ============================================================================

/// canonical humanoid rigging (16 joint + 15 bone)
///
/// # 座標系
///
/// 右手系、Y up (alice-lol / alice-sdf 慣例と一致)
/// canonical T-pose では頭頂 `y = +h / 2`、足首 `y = -h * leg_ratio`、両腕水平 (H.2 formula)
#[derive(Debug, Clone)]
pub struct HumanoidTemplate {
    /// bone list (from → to + thickness)
    pub bones: Vec<Bone>,
    /// joint → local 3D position (Y up)
    pub joints: HashMap<Joint, [f32; 3]>,
}

impl Default for HumanoidTemplate {
    /// canonical humanoid T-pose factory ([`MorphologyParams::default`] 相当)
    fn default() -> Self {
        Self::from_morphology(&MorphologyParams::default())
    }
}

impl HumanoidTemplate {
    /// [`MorphologyParams`] から joint 位置を数式導出、bones topology は固定
    #[must_use]
    pub fn from_morphology(params: &MorphologyParams) -> Self {
        let h = params.height;
        let n = params.head_body_ratio;

        // Y 座標 (Waist が原点)
        let head_top_y = h * 0.5;
        let head_span = h / n;
        let neck_y = head_top_y - head_span;
        let chest_y = neck_y * 0.75;
        let waist_y = 0.0_f32;
        let hip_y = 0.0_f32;
        let ankle_y = -h * params.leg_ratio;
        let knee_y = ankle_y * 0.52;
        let shoulder_y = chest_y;

        // X 座標 (右側正、左側負、T-pose 水平)
        let shoulder_x = h * params.shoulder_ratio * 0.5;
        let arm_len = h * params.arm_ratio;
        let elbow_x = arm_len.mul_add(0.5, shoulder_x);
        let wrist_x = shoulder_x + arm_len;
        let hip_x = h * params.hip_ratio * 0.5;

        let mut joints = HashMap::with_capacity(16);
        joints.insert(Joint::Head, [0.0, head_top_y, 0.0]);
        joints.insert(Joint::Neck, [0.0, neck_y, 0.0]);
        joints.insert(Joint::Chest, [0.0, chest_y, 0.0]);
        joints.insert(Joint::Waist, [0.0, waist_y, 0.0]);
        joints.insert(Joint::LShoulder, [-shoulder_x, shoulder_y, 0.0]);
        joints.insert(Joint::RShoulder, [shoulder_x, shoulder_y, 0.0]);
        joints.insert(Joint::LElbow, [-elbow_x, shoulder_y, 0.0]);
        joints.insert(Joint::RElbow, [elbow_x, shoulder_y, 0.0]);
        joints.insert(Joint::LWrist, [-wrist_x, shoulder_y, 0.0]);
        joints.insert(Joint::RWrist, [wrist_x, shoulder_y, 0.0]);
        joints.insert(Joint::LHip, [-hip_x, hip_y, 0.0]);
        joints.insert(Joint::RHip, [hip_x, hip_y, 0.0]);
        joints.insert(Joint::LKnee, [-hip_x, knee_y, 0.0]);
        joints.insert(Joint::RKnee, [hip_x, knee_y, 0.0]);
        joints.insert(Joint::LAnkle, [-hip_x, ankle_y, 0.0]);
        joints.insert(Joint::RAnkle, [hip_x, ankle_y, 0.0]);

        Self {
            bones: canonical_bones(),
            joints,
        }
    }

    /// fluent builder for morphology-driven construction
    #[must_use]
    pub fn builder() -> HumanoidTemplateBuilder {
        HumanoidTemplateBuilder::default()
    }

    /// bone chain を Capsule → `SmoothUnion` tree に変換 ([`MuscleWidths::default`] = shounen 使用)
    ///
    /// # Panics
    ///
    /// - `self.bones` が空
    /// - `self.bones` のいずれかが参照する joint が `self.joints` に存在しない
    #[must_use]
    pub fn to_sdf(&self, smoothness_k: f32) -> SdfNode {
        self.to_sdf_with_widths(smoothness_k, &MuscleWidths::default())
    }

    /// bone chain を Capsule → `SmoothUnion` tree に変換 (thickness preset 明示版)
    ///
    /// # Panics
    ///
    /// - `self.bones` が空
    /// - `self.bones` のいずれかが参照する joint が `self.joints` に存在しない
    #[must_use]
    pub fn to_sdf_with_widths(&self, smoothness_k: f32, widths: &MuscleWidths) -> SdfNode {
        assert!(
            !self.bones.is_empty(),
            "HumanoidTemplate::to_sdf: empty bones"
        );

        self.bones
            .iter()
            .map(|bone| self.bone_to_capsule(*bone, widths))
            .reduce(|acc, next| SdfNode::SmoothUnion {
                a: Arc::new(acc),
                b: Arc::new(next),
                k: smoothness_k,
            })
            .expect("bones non-empty by assert above")
    }

    /// bind pose (self) + VRM-named local rotations で FK 適用、新 template を返す
    ///
    /// `vrm_named_rotations` は VRM bone name (`"leftUpperArm"` 等) → parent frame の local rotation
    /// 指定なし joint は identity 適用 (bind pose 保持)
    /// bones topology は同一、joints のみ FK で更新される
    ///
    /// # FK Algorithm
    ///
    /// 1. `self.joints` を bind pose として、各 child joint の parent 基準 local offset を計算
    /// 2. Waist を root、[`VRM_TOPO_ORDER`] で FK walk:
    ///    `world[child] = world[parent] * translate(offset) * rotate(local_rot)`
    /// 3. `world.transform_point3(Vec3::ZERO)` で新 joint 位置を抽出
    ///
    /// # Panics
    ///
    /// - `self.joints` に [`Joint::Waist`] が存在しない (canonical topology 前提)
    #[must_use]
    pub fn with_pose(&self, vrm_named_rotations: &HashMap<String, Quat>) -> Self {
        let parent_map: HashMap<Joint, Joint> = VRM_PARENT_TABLE.iter().copied().collect();

        // Step 1: bind pose local offsets
        let mut local_offsets: HashMap<Joint, Vec3> = HashMap::with_capacity(15);
        for (&child, &parent) in &parent_map {
            if let (Some(&c), Some(&p)) = (self.joints.get(&child), self.joints.get(&parent)) {
                local_offsets.insert(child, Vec3::from(c) - Vec3::from(p));
            }
        }

        // Step 2: root transform (Waist)
        let root_pos: Vec3 = self
            .joints
            .get(&Joint::Waist)
            .copied()
            .expect("HumanoidTemplate::with_pose requires Waist joint")
            .into();
        let root_rot = vrm_named_rotations
            .get(joint_to_vrm_name(Joint::Waist))
            .copied()
            .unwrap_or(Quat::IDENTITY);
        let root_transform = Mat4::from_rotation_translation(root_rot, root_pos);

        // Step 3: FK walk
        let mut world_transforms: HashMap<Joint, Mat4> = HashMap::with_capacity(16);
        world_transforms.insert(Joint::Waist, root_transform);

        for &joint in VRM_TOPO_ORDER {
            if joint == Joint::Waist {
                continue;
            }
            let Some(&parent) = parent_map.get(&joint) else {
                continue;
            };
            let Some(&parent_t) = world_transforms.get(&parent) else {
                continue;
            };
            let offset = local_offsets.get(&joint).copied().unwrap_or(Vec3::ZERO);
            let rot = vrm_named_rotations
                .get(joint_to_vrm_name(joint))
                .copied()
                .unwrap_or(Quat::IDENTITY);
            let local_t = Mat4::from_rotation_translation(rot, offset);
            world_transforms.insert(joint, parent_t * local_t);
        }

        // Step 4: extract world positions
        let joints: HashMap<Joint, [f32; 3]> = world_transforms
            .iter()
            .map(|(j, t)| {
                let p = t.transform_point3(Vec3::ZERO);
                (*j, [p.x, p.y, p.z])
            })
            .collect();

        Self {
            bones: self.bones.clone(),
            joints,
        }
    }

    fn bone_to_capsule(&self, bone: Bone, widths: &MuscleWidths) -> SdfNode {
        let from = self
            .joints
            .get(&bone.from)
            .copied()
            .unwrap_or_else(|| panic!("HumanoidTemplate: joint {:?} missing", bone.from));
        let to = self
            .joints
            .get(&bone.to)
            .copied()
            .unwrap_or_else(|| panic!("HumanoidTemplate: joint {:?} missing", bone.to));
        let radius = classify_thickness(bone, widths);
        SdfNode::Capsule {
            point_a: Vec3::from_array(from),
            point_b: Vec3::from_array(to),
            radius,
        }
    }
}

/// canonical 15-bone humanoid topology (脊柱 3 + 左腕 3 + 右腕 3 + 左脚 3 + 右脚 3)
pub(crate) fn canonical_bones() -> Vec<Bone> {
    vec![
        // 脊柱 (3)
        Bone {
            from: Joint::Head,
            to: Joint::Neck,
            thickness: 0.4,
        },
        Bone {
            from: Joint::Neck,
            to: Joint::Chest,
            thickness: 0.5,
        },
        Bone {
            from: Joint::Chest,
            to: Joint::Waist,
            thickness: 0.6,
        },
        // 左腕 (3)
        Bone {
            from: Joint::Chest,
            to: Joint::LShoulder,
            thickness: 0.4,
        },
        Bone {
            from: Joint::LShoulder,
            to: Joint::LElbow,
            thickness: 0.35,
        },
        Bone {
            from: Joint::LElbow,
            to: Joint::LWrist,
            thickness: 0.3,
        },
        // 右腕 (3)
        Bone {
            from: Joint::Chest,
            to: Joint::RShoulder,
            thickness: 0.4,
        },
        Bone {
            from: Joint::RShoulder,
            to: Joint::RElbow,
            thickness: 0.35,
        },
        Bone {
            from: Joint::RElbow,
            to: Joint::RWrist,
            thickness: 0.3,
        },
        // 左脚 (3)
        Bone {
            from: Joint::Waist,
            to: Joint::LHip,
            thickness: 0.4,
        },
        Bone {
            from: Joint::LHip,
            to: Joint::LKnee,
            thickness: 0.35,
        },
        Bone {
            from: Joint::LKnee,
            to: Joint::LAnkle,
            thickness: 0.3,
        },
        // 右脚 (3)
        Bone {
            from: Joint::Waist,
            to: Joint::RHip,
            thickness: 0.4,
        },
        Bone {
            from: Joint::RHip,
            to: Joint::RKnee,
            thickness: 0.35,
        },
        Bone {
            from: Joint::RKnee,
            to: Joint::RAnkle,
            thickness: 0.3,
        },
    ]
}

// ============================================================================
// HumanoidTemplateBuilder (H.2)
// ============================================================================

/// [`HumanoidTemplate`] の fluent builder ([`MorphologyParams`] を chain 設定)
///
/// # 例
///
/// ```
/// use alice_lol_humanoid::HumanoidTemplate;
///
/// let t = HumanoidTemplate::builder()
///     .height(1.7)
///     .head_body_ratio(7.5)
///     .build();
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct HumanoidTemplateBuilder {
    params: MorphologyParams,
}

impl HumanoidTemplateBuilder {
    /// 全 params を一括設定 (preset 経由の chain 起点用)
    #[must_use]
    pub const fn from_params(mut self, params: MorphologyParams) -> Self {
        self.params = params;
        self
    }

    /// 全身高
    #[must_use]
    pub const fn height(mut self, h: f32) -> Self {
        self.params.height = h;
        self
    }

    /// 頭身
    #[must_use]
    pub const fn head_body_ratio(mut self, n: f32) -> Self {
        self.params.head_body_ratio = n;
        self
    }

    /// 腕長 / 身長 (per side)
    #[must_use]
    pub const fn arm_ratio(mut self, r: f32) -> Self {
        self.params.arm_ratio = r;
        self
    }

    /// 肩幅 / 身長
    #[must_use]
    pub const fn shoulder_ratio(mut self, r: f32) -> Self {
        self.params.shoulder_ratio = r;
        self
    }

    /// 骨盤幅 / 身長
    #[must_use]
    pub const fn hip_ratio(mut self, r: f32) -> Self {
        self.params.hip_ratio = r;
        self
    }

    /// 脚長 / 身長
    #[must_use]
    pub const fn leg_ratio(mut self, r: f32) -> Self {
        self.params.leg_ratio = r;
        self
    }

    /// [`HumanoidTemplate`] を構築 (`from_morphology` 経由)
    #[must_use]
    pub fn build(self) -> HumanoidTemplate {
        HumanoidTemplate::from_morphology(&self.params)
    }
}

// ============================================================================
// classify_thickness helper
// ============================================================================

/// `bone.from` / `bone.to` から limb / torso / head を判別し対応 thickness を返す
fn classify_thickness(bone: Bone, widths: &MuscleWidths) -> f32 {
    use Joint::{Chest, Head, Neck, Waist};
    let both_torso = |j| matches!(j, Chest | Waist | Neck);
    if matches!(bone.from, Head) || matches!(bone.to, Head) {
        widths.head
    } else if both_torso(bone.from) && both_torso(bone.to) {
        widths.torso
    } else {
        widths.limb
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use alice_lol::eval;

    // ─── H.1 継続 test (invariance で全 pass 想定) ───

    #[test]
    fn humanoid_default_has_16_joints_15_bones() {
        let t = HumanoidTemplate::default();
        assert_eq!(t.joints.len(), 16);
        assert_eq!(t.bones.len(), 15);
    }

    #[test]
    fn humanoid_default_y_up_convention() {
        let t = HumanoidTemplate::default();
        let head_y = t.joints[&Joint::Head][1];
        let waist_y = t.joints[&Joint::Waist][1];
        let ankle_y = t.joints[&Joint::LAnkle][1];
        assert!(
            head_y > waist_y,
            "Head y ({head_y}) should exceed Waist y ({waist_y})"
        );
        assert!(
            waist_y > ankle_y,
            "Waist y ({waist_y}) should exceed LAnkle y ({ankle_y})"
        );
    }

    #[test]
    fn humanoid_default_t_pose_arms_horizontal() {
        let t = HumanoidTemplate::default();
        let ls = t.joints[&Joint::LShoulder];
        let lw = t.joints[&Joint::LWrist];
        let rs = t.joints[&Joint::RShoulder];
        let rw = t.joints[&Joint::RWrist];
        assert!((ls[1] - lw[1]).abs() < 1e-4, "L arm not horizontal");
        assert!((rs[1] - rw[1]).abs() < 1e-4, "R arm not horizontal");
        assert!(lw[0] < ls[0], "LWrist should be more -X than LShoulder");
        assert!(rw[0] > rs[0], "RWrist should be more +X than RShoulder");
    }

    #[test]
    fn to_sdf_produces_smooth_union_tree() {
        let t = HumanoidTemplate::default();
        let sdf = t.to_sdf(0.15);
        assert!(
            matches!(sdf, SdfNode::SmoothUnion { .. }),
            "top level should be SmoothUnion"
        );
    }

    #[test]
    fn to_sdf_with_wider_widths_penetrates_deeper_at_head_center() {
        let t = HumanoidTemplate::default();
        let sdf_chibi = t.to_sdf_with_widths(0.15, &MuscleWidths::chibi());
        let sdf_slim = t.to_sdf_with_widths(0.15, &MuscleWidths::slim());
        let point = Vec3::new(0.0, 2.5, 0.0); // head center
        let d_chibi = eval(&sdf_chibi, point);
        let d_slim = eval(&sdf_slim, point);
        assert!(
            d_chibi < d_slim,
            "chibi distance ({d_chibi}) should be more negative than slim ({d_slim}) at head center",
        );
    }

    #[test]
    fn muscle_widths_presets_ordering() {
        let chibi = MuscleWidths::chibi();
        let shounen = MuscleWidths::shounen();
        let slim = MuscleWidths::slim();
        assert!(chibi.limb > shounen.limb && shounen.limb > slim.limb);
        assert!(chibi.torso > shounen.torso && shounen.torso > slim.torso);
        assert!(chibi.head > shounen.head && shounen.head > slim.head);
    }

    #[test]
    fn all_bones_reference_existing_joints() {
        let t = HumanoidTemplate::default();
        for bone in &t.bones {
            assert!(
                t.joints.contains_key(&bone.from),
                "bone.from {:?} missing",
                bone.from
            );
            assert!(
                t.joints.contains_key(&bone.to),
                "bone.to {:?} missing",
                bone.to
            );
        }
    }

    #[test]
    fn classify_thickness_head_torso_limb() {
        let w = MuscleWidths::shounen();
        let head_bone = Bone {
            from: Joint::Head,
            to: Joint::Neck,
            thickness: 0.0,
        };
        assert!((classify_thickness(head_bone, &w) - w.head).abs() < 1e-6);
        let torso_bone = Bone {
            from: Joint::Chest,
            to: Joint::Waist,
            thickness: 0.0,
        };
        assert!((classify_thickness(torso_bone, &w) - w.torso).abs() < 1e-6);
        let limb_bone = Bone {
            from: Joint::LShoulder,
            to: Joint::LElbow,
            thickness: 0.0,
        };
        assert!((classify_thickness(limb_bone, &w) - w.limb).abs() < 1e-6);
    }

    #[test]
    fn joint_all_length_and_uniqueness() {
        assert_eq!(Joint::ALL.len(), 16);
        // uniqueness: HashSet 化して同数なら重複なし
        let set: std::collections::HashSet<Joint> = Joint::ALL.iter().copied().collect();
        assert_eq!(set.len(), 16);
    }

    // ─── H.2 追加 test (MorphologyParams + Builder) ───

    #[test]
    fn morphology_default_matches_h1_hardcoded_joints() {
        // H.1 で hardcoded だった canonical 座標を parametric formula が再現するか
        let t = HumanoidTemplate::default();
        let expect = [
            (Joint::Head, [0.0_f32, 2.5, 0.0]),
            (Joint::Neck, [0.0, 2.0, 0.0]),
            (Joint::Chest, [0.0, 1.5, 0.0]),
            (Joint::Waist, [0.0, 0.0, 0.0]),
            (Joint::LShoulder, [-0.6, 1.5, 0.0]),
            (Joint::RShoulder, [0.6, 1.5, 0.0]),
            (Joint::LElbow, [-1.5, 1.5, 0.0]),
            (Joint::RElbow, [1.5, 1.5, 0.0]),
            (Joint::LWrist, [-2.4, 1.5, 0.0]),
            (Joint::RWrist, [2.4, 1.5, 0.0]),
            (Joint::LHip, [-0.4, 0.0, 0.0]),
            (Joint::RHip, [0.4, 0.0, 0.0]),
            (Joint::LKnee, [-0.4, -1.3, 0.0]),
            (Joint::RKnee, [0.4, -1.3, 0.0]),
            (Joint::LAnkle, [-0.4, -2.5, 0.0]),
            (Joint::RAnkle, [0.4, -2.5, 0.0]),
        ];
        for (joint, expected) in expect {
            let actual = t.joints[&joint];
            for i in 0..3 {
                assert!(
                    (actual[i] - expected[i]).abs() < 1e-4,
                    "joint {joint:?} axis {i}: expected {}, got {}",
                    expected[i],
                    actual[i]
                );
            }
        }
    }

    #[test]
    fn morphology_chibi_has_larger_head_span_than_adult() {
        let chibi = MorphologyParams::chibi();
        let adult = MorphologyParams::adult();
        let head_span_chibi = chibi.height / chibi.head_body_ratio;
        let head_span_adult = adult.height / adult.head_body_ratio;
        assert!(
            head_span_chibi > head_span_adult,
            "chibi head span ({head_span_chibi}) should exceed adult ({head_span_adult})"
        );
    }

    #[test]
    fn morphology_taller_scales_all_positions_linearly() {
        let short = MorphologyParams::default();
        let tall = MorphologyParams {
            height: short.height * 2.0,
            ..MorphologyParams::default()
        };
        let t_s = HumanoidTemplate::from_morphology(&short);
        let t_t = HumanoidTemplate::from_morphology(&tall);
        for j in Joint::ALL {
            let s = t_s.joints[&j];
            let t = t_t.joints[&j];
            for i in 0..3 {
                assert!(
                    s[i].mul_add(-2.0, t[i]).abs() < 1e-4,
                    "joint {j:?} axis {i}: short {} → tall {}, expected 2x",
                    s[i],
                    t[i]
                );
            }
        }
    }

    #[test]
    fn builder_chain_produces_expected_joint_positions() {
        // height=10, head_body_ratio=4 → head_top y = 5, head_span = 2.5, neck y = 2.5
        let t = HumanoidTemplate::builder()
            .height(10.0)
            .head_body_ratio(4.0)
            .build();
        assert!((t.joints[&Joint::Head][1] - 5.0).abs() < 1e-4);
        assert!((t.joints[&Joint::Neck][1] - 2.5).abs() < 1e-4);
        assert!((t.joints[&Joint::Chest][1] - 1.875).abs() < 1e-4); // 2.5 * 0.75
    }

    #[test]
    fn morphology_adult_vs_hero_head_span_differs() {
        let adult = MorphologyParams::adult();
        let hero = MorphologyParams::hero();
        let head_span_adult = adult.height / adult.head_body_ratio;
        let head_span_hero = hero.height / hero.head_body_ratio;
        assert!(
            head_span_adult > head_span_hero,
            "adult 7.5 頭身 head span ({head_span_adult}) should exceed hero 8 頭身 ({head_span_hero})"
        );
    }

    #[test]
    fn humanoid_from_morphology_preserves_bone_topology_across_presets() {
        let t_chibi = HumanoidTemplate::from_morphology(&MorphologyParams::chibi());
        let t_adult = HumanoidTemplate::from_morphology(&MorphologyParams::adult());
        let t_hero = HumanoidTemplate::from_morphology(&MorphologyParams::hero());
        for t in [&t_chibi, &t_adult, &t_hero] {
            assert_eq!(t.bones.len(), 15);
            assert_eq!(t.joints.len(), 16);
        }
        // bone topology (from / to pairs) は preset に関わらず同一
        for i in 0..15 {
            assert_eq!(t_chibi.bones[i].from, t_adult.bones[i].from);
            assert_eq!(t_chibi.bones[i].to, t_adult.bones[i].to);
            assert_eq!(t_adult.bones[i].from, t_hero.bones[i].from);
        }
    }

    #[test]
    fn morphology_presets_head_body_ratio_ordering() {
        let chibi = MorphologyParams::chibi();
        let adult = MorphologyParams::adult();
        let hero = MorphologyParams::hero();
        let default = MorphologyParams::default();
        assert!(chibi.head_body_ratio < adult.head_body_ratio);
        assert!(adult.head_body_ratio < hero.head_body_ratio);
        assert!(hero.head_body_ratio < default.head_body_ratio); // default = 10 (最高)
    }

    // ─── H.4 追加 test (with_pose FK) ───

    #[test]
    fn with_pose_identity_returns_bind_pose_equivalent_positions() {
        let t = HumanoidTemplate::default();
        let rotations: HashMap<String, Quat> = HashMap::new();
        let posed = t.with_pose(&rotations);
        assert_eq!(posed.joints.len(), t.joints.len());
        for joint in Joint::ALL {
            let orig = t.joints[&joint];
            let new = posed.joints[&joint];
            for i in 0..3 {
                assert!(
                    (orig[i] - new[i]).abs() < 1e-4,
                    "joint {joint:?} axis {i}: bind {} vs posed {}",
                    orig[i],
                    new[i]
                );
            }
        }
    }

    #[test]
    fn with_pose_left_shoulder_rotation_moves_arm_up() {
        // canonical bind pose:
        //   LShoulder [-0.6, 1.5, 0]、LElbow [-1.5, 1.5, 0] (offset [-0.9, 0, 0])
        //   LWrist [-2.4, 1.5, 0] (offset [-0.9, 0, 0] from LElbow)
        // leftUpperArm に Z=-π/2 適用 → LShoulder 中心に arm が +Y に回転
        let t = HumanoidTemplate::default();
        let mut rotations = HashMap::new();
        rotations.insert(
            "leftUpperArm".to_string(),
            Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2),
        );
        let posed = t.with_pose(&rotations);

        // LShoulder 自身 は変わらず (parent = Chest、Chest に rotation なし)
        let ls = posed.joints[&Joint::LShoulder];
        assert!((ls[0] - (-0.6)).abs() < 1e-4, "LShoulder x: {}", ls[0]);
        assert!((ls[1] - 1.5).abs() < 1e-4, "LShoulder y: {}", ls[1]);

        // LElbow は Shoulder 中心 Z=-π/2 で offset (-0.9, 0, 0) が (0, 0.9, 0) に回る
        // world = LShoulder + rotated = (-0.6, 2.4, 0)
        let le = posed.joints[&Joint::LElbow];
        assert!((le[0] - (-0.6)).abs() < 1e-4, "LElbow x: {}", le[0]);
        assert!((le[1] - 2.4).abs() < 1e-4, "LElbow y: {}", le[1]);

        // 右腕 (rightUpperArm rotation なし) は影響なし
        let re = posed.joints[&Joint::RElbow];
        assert!((re[0] - 1.5).abs() < 1e-4, "RElbow x: {}", re[0]);
        assert!((re[1] - 1.5).abs() < 1e-4);
    }

    #[test]
    fn with_pose_hips_rotation_propagates_to_children() {
        // hips (Waist) の Y=π/2 rotation は全 children に伝播
        // canonical: Chest [0, 1.5, 0], LShoulder [-0.6, 1.5, 0]
        //   Chest の offset from Waist = (0, 1.5, 0)、Y rotation は Y 成分不変 → (0, 1.5, 0)
        //   LShoulder の offset from Chest = (-0.6, 0, 0)、R_y(π/2) → (0, 0, 0.6)
        //   → LShoulder world = Chest(0, 1.5, 0) + (0, 0, 0.6) = (0, 1.5, 0.6)
        let t = HumanoidTemplate::default();
        let mut rotations = HashMap::new();
        rotations.insert(
            "hips".to_string(),
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        );
        let posed = t.with_pose(&rotations);

        // Waist は原点固定 (self position)
        let waist = posed.joints[&Joint::Waist];
        assert!(waist[0].abs() < 1e-4);
        assert!(waist[1].abs() < 1e-4);

        // Chest は Y unchanged
        let chest = posed.joints[&Joint::Chest];
        assert!(chest[0].abs() < 1e-4, "Chest x: {}", chest[0]);
        assert!((chest[1] - 1.5).abs() < 1e-4);

        // LShoulder は +Z 方向に回転
        let ls = posed.joints[&Joint::LShoulder];
        assert!(ls[0].abs() < 1e-4, "LShoulder x after Y rot: {}", ls[0]);
        assert!((ls[1] - 1.5).abs() < 1e-4);
        assert!(
            (ls[2] - 0.6).abs() < 1e-4,
            "LShoulder z after Y rot: {}",
            ls[2]
        );
    }

    #[test]
    fn with_pose_unmapped_bone_names_ignored() {
        // 不明な bone name は無視される (bind pose 保持)
        let t = HumanoidTemplate::default();
        let mut rotations = HashMap::new();
        rotations.insert("unknownBone".to_string(), Quat::from_rotation_x(1.0));
        let posed = t.with_pose(&rotations);
        // 全 joint bind pose 保持
        for joint in Joint::ALL {
            let orig = t.joints[&joint];
            let new = posed.joints[&joint];
            for i in 0..3 {
                assert!(
                    (orig[i] - new[i]).abs() < 1e-4,
                    "joint {joint:?} axis {i} changed by unmapped bone",
                );
            }
        }
    }
}
