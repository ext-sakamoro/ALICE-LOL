//! # alice-lol-humanoid
//!
//! Humanoid template for `alice_lol` DSL parametric character generation
//!
//! `~/ALICE-LOL/docs/HUMANOID_TEMPLATE_ROADMAP.md` の Phase H.0-H.6 に沿って段階実装
//! 現在 Phase = **H.1 Static template** (canonical T-pose + `to_sdf`、morphology は H.2)
//!
//! # Roadmap
//!
//! - H.0 Scaffolding (完了)
//! - **H.1 Static template** (本 Phase、`HumanoidTemplate::default()` + `to_sdf(k)`)
//! - H.2 Parametric morphology (`MorphologyParams` + builder)
//! - H.3 VRM import (feature `vrm`、gltf optional dep)
//! - H.4 BVH import + pose (`apply_pose`)
//! - H.5 ALICE-Manga との duplication 整理
//! - H.6 Intent 結線 (`apply_intent(&IntentNode)`)
//!
//! # 三相原理での位置付け
//!
//! Phase 2 Law (parametric morphology) → Phase 3 Intent (verb) の橋渡し
//! 詳細: `docs/HUMANOID_TEMPLATE_DESIGN.md` §1
//!
//! # H.1 実装ノート (duplication)
//!
//! `Joint` enum / `Bone` struct / `MuscleWidths` / `HumanoidTemplate::default()` /
//! `to_sdf(k)` は ALICE-Manga `src/skeleton.rs` + `src/skeleton3d.rs` の code copy
//! Phase H.5 で reconciliation (併存 / wrapper 化 / 段階移行) を user 判断
//!
//! # Quick start
//!
//! ```
//! use alice_lol_humanoid::HumanoidTemplate;
//!
//! let template = HumanoidTemplate::default();
//! let sdf = template.to_sdf(0.15);
//! // sdf は 15-Capsule chain の SmoothUnion tree
//! ```

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::sync::Arc;

use alice_lol::{SdfNode, Vec3};

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
// HumanoidTemplate
// ============================================================================

/// canonical humanoid rigging (16 joint + 15 bone)
///
/// # 座標系
///
/// 右手系、Y up (alice-lol / alice-sdf 慣例と一致)
/// canonical T-pose では頭頂 y=+2.5、足首 y=-2.5、両腕水平
#[derive(Debug, Clone)]
pub struct HumanoidTemplate {
    /// bone list (from → to + thickness)
    pub bones: Vec<Bone>,
    /// joint → local 3D position (Y up)
    pub joints: HashMap<Joint, [f32; 3]>,
}

impl Default for HumanoidTemplate {
    /// canonical humanoid T-pose factory (16 joint / 15 bone、Y up、頭頂 y=+2.5、足首 y=-2.5)
    fn default() -> Self {
        let mut joints = HashMap::with_capacity(16);
        // 脊柱軸 (x=0, z=0)
        joints.insert(Joint::Head, [0.0, 2.5, 0.0]);
        joints.insert(Joint::Neck, [0.0, 2.0, 0.0]);
        joints.insert(Joint::Chest, [0.0, 1.5, 0.0]);
        joints.insert(Joint::Waist, [0.0, 0.0, 0.0]);
        // 両肩 / 肘 / 手首 (T-pose)
        joints.insert(Joint::LShoulder, [-0.6, 1.5, 0.0]);
        joints.insert(Joint::RShoulder, [0.6, 1.5, 0.0]);
        joints.insert(Joint::LElbow, [-1.5, 1.5, 0.0]);
        joints.insert(Joint::RElbow, [1.5, 1.5, 0.0]);
        joints.insert(Joint::LWrist, [-2.4, 1.5, 0.0]);
        joints.insert(Joint::RWrist, [2.4, 1.5, 0.0]);
        // 両脚
        joints.insert(Joint::LHip, [-0.4, 0.0, 0.0]);
        joints.insert(Joint::RHip, [0.4, 0.0, 0.0]);
        joints.insert(Joint::LKnee, [-0.4, -1.3, 0.0]);
        joints.insert(Joint::RKnee, [0.4, -1.3, 0.0]);
        joints.insert(Joint::LAnkle, [-0.4, -2.5, 0.0]);
        joints.insert(Joint::RAnkle, [0.4, -2.5, 0.0]);

        let bones = vec![
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
        ];

        Self { bones, joints }
    }
}

impl HumanoidTemplate {
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
}
