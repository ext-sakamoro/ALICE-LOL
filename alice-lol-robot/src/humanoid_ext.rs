//! humanoid extension trait — LOL [`IntentNode`] を [`HumanoidTemplate`] に反映
//!
//! **Additive adapter pattern**: [`alice_lol_humanoid`] crate は触らず、
//! [`HumanoidIntent`] trait を本 crate 側で `impl` することで、
//! `HumanoidTemplate::apply_intent` に相当する Phase H.6 機能を追加
//!
//! # MVP scope (Phase R.2)
//!
//! - [`IntentNode::Walk`] `{ destination, speed }` → 全 joint を
//!   `(destination - waist_pos)` 分 translate
//! - [`IntentNode::Rest`] `{ duration_ms }` → 変化なし (clone)
//! - [`IntentNode::Sequence`] → 順次 apply の fold
//! - [`IntentNode::Parallel`] → 先頭 verb のみ apply (concurrency は semantic loss)
//! - 他 verb (`Grasp` / `Point` / `Push` / `Pull` / `Follow` / `Avoid` 等) → 変化なし
//!
//! # Phase R.6+ 計画
//!
//! - `Grasp` / `Point` / `Throw`: `alice_kinematics::predictor` の jerk-min 軌道 solver を
//!   経由して arm joint (`Shoulder` / `Elbow` / `Wrist`) の quat を FK で更新
//! - `Follow` / `Avoid`: navigation controller (別 crate) と結線
//! - `Gaze` / `Rotate` / `Align` は Kinematics scope 外、Foundry / Social Intent 経路で表現

use alice_lol::intent::IntentNode;
use alice_lol_humanoid::{HumanoidTemplate, Joint};

use crate::error::RobotError;

/// [`HumanoidTemplate`] に LOL [`IntentNode`] を反映する trait
///
/// 本 trait は adapter pattern で [`alice_lol_humanoid`] crate に破壊変更を加えずに
/// Phase 3 Intent 経路を実現する Phase H.6 の canonical impl として機能する
pub trait HumanoidIntent: Sized {
    /// Intent を humanoid に反映し、新しい template を返す
    ///
    /// # Errors
    ///
    /// - [`RobotError::PoseFail`]: `Waist` joint が template から missing
    fn apply_intent(&self, intent: &IntentNode) -> Result<Self, RobotError>;
}

impl HumanoidIntent for HumanoidTemplate {
    fn apply_intent(&self, intent: &IntentNode) -> Result<Self, RobotError> {
        apply_intent_impl(self, intent)
    }
}

/// Internal impl (recursive [`IntentNode::Sequence`] / [`IntentNode::Parallel`] handling
/// を分離)
fn apply_intent_impl(
    template: &HumanoidTemplate,
    intent: &IntentNode,
) -> Result<HumanoidTemplate, RobotError> {
    match intent {
        IntentNode::Walk { destination, .. } => translate_to_destination(template, *destination),

        // 変化なし verb 群 (MVP scope)
        IntentNode::Rest { .. }
        | IntentNode::Grasp { .. }
        | IntentNode::Release { .. }
        | IntentNode::Point { .. }
        | IntentNode::Throw { .. }
        | IntentNode::Catch { .. }
        | IntentNode::Push { .. }
        | IntentNode::Pull { .. }
        | IntentNode::Follow { .. }
        | IntentNode::Avoid { .. }
        | IntentNode::Gaze { .. }
        | IntentNode::Rotate { .. }
        | IntentNode::Align { .. } => Ok(template.clone()),

        // 合成 verb
        IntentNode::Sequence(children) => {
            let mut acc = template.clone();
            for child in children {
                acc = apply_intent_impl(&acc, child)?;
            }
            Ok(acc)
        }
        IntentNode::Parallel(children) => {
            // MVP: 先頭のみ apply (concurrency semantic loss、doc に明記済)
            children.first().map_or_else(
                || Ok(template.clone()),
                |first| apply_intent_impl(template, first),
            )
        }
    }
}

/// [`Joint::Waist`] を destination まで translate、他 joint も同量 shift (rigid body
/// translation)
///
/// # Errors
///
/// - [`RobotError::PoseFail`]: [`Joint::Waist`] が `template.joints` に missing
fn translate_to_destination(
    template: &HumanoidTemplate,
    destination: glam::Vec3,
) -> Result<HumanoidTemplate, RobotError> {
    let current_waist = template
        .joints
        .get(&Joint::Waist)
        .ok_or(RobotError::PoseFail("Waist joint missing"))?;
    let delta = destination - glam::Vec3::new(current_waist[0], current_waist[1], current_waist[2]);

    let mut new_template = template.clone();
    for pos in new_template.joints.values_mut() {
        pos[0] += delta.x;
        pos[1] += delta.y;
        pos[2] += delta.z;
    }
    Ok(new_template)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alice_lol::intent::{grasp, parallel, point, rest, sequence, walk, HandSide};
    use glam::Vec3;

    /// f32 array 3 要素の epsilon 比較 helper (`clippy::float_cmp` 回避)
    fn joints_close(a: [f32; 3], b: [f32; 3], eps: f32) -> bool {
        (a[0] - b[0]).abs() < eps && (a[1] - b[1]).abs() < eps && (a[2] - b[2]).abs() < eps
    }

    #[test]
    fn walk_translates_waist_to_destination() {
        let t = HumanoidTemplate::default();
        let original_waist = t.joints[&Joint::Waist];
        let dest = Vec3::new(2.0, 0.0, 1.0);
        let posed = t.apply_intent(&walk(dest, 1.0)).unwrap();
        let new_waist = posed.joints[&Joint::Waist];
        assert!((new_waist[0] - dest.x).abs() < 1e-5);
        assert!((new_waist[1] - dest.y).abs() < 1e-5);
        assert!((new_waist[2] - dest.z).abs() < 1e-5);
        // 元の waist と違うことを確認 (translation 効いている)
        assert!((new_waist[0] - original_waist[0]).abs() > 1e-3);
    }

    #[test]
    fn walk_translates_all_joints_uniformly() {
        let t = HumanoidTemplate::default();
        let original_head = t.joints[&Joint::Head];
        let original_waist = t.joints[&Joint::Waist];
        let dest = Vec3::new(3.0, 0.0, 0.0);
        let posed = t.apply_intent(&walk(dest, 1.0)).unwrap();
        let new_head = posed.joints[&Joint::Head];
        let new_waist = posed.joints[&Joint::Waist];

        // head と waist の relative offset は translation で変わらない (rigid body)
        let orig_delta_x = original_head[0] - original_waist[0];
        let new_delta_x = new_head[0] - new_waist[0];
        assert!((orig_delta_x - new_delta_x).abs() < 1e-5);
    }

    #[test]
    fn rest_returns_unchanged_clone() {
        let t = HumanoidTemplate::default();
        let posed = t.apply_intent(&rest(500)).unwrap();
        assert_eq!(t.joints.len(), posed.joints.len());
        for joint in &[Joint::Head, Joint::Waist, Joint::LWrist, Joint::RWrist] {
            assert!(joints_close(t.joints[joint], posed.joints[joint], 1e-6));
        }
    }

    #[test]
    fn grasp_returns_unchanged_clone_mvp() {
        let t = HumanoidTemplate::default();
        let posed = t.apply_intent(&grasp(0, HandSide::Right, 3.0)).unwrap();
        // MVP: pose 変更なし (Phase R.6+ で IK 実装)
        for joint in &[Joint::LWrist, Joint::RWrist] {
            assert!(joints_close(t.joints[joint], posed.joints[joint], 1e-6));
        }
    }

    #[test]
    fn point_returns_unchanged_clone_mvp() {
        let t = HumanoidTemplate::default();
        let posed = t
            .apply_intent(&point(Vec3::new(2.0, 1.0, 0.0), HandSide::Right))
            .unwrap();
        for joint in &[Joint::LWrist, Joint::RWrist] {
            assert!(joints_close(t.joints[joint], posed.joints[joint], 1e-6));
        }
    }

    #[test]
    fn sequence_applies_verbs_in_order() {
        let t = HumanoidTemplate::default();
        let seq = sequence(vec![
            walk(Vec3::new(1.0, 0.0, 0.0), 1.0),
            walk(Vec3::new(3.0, 0.0, 0.0), 1.0),
        ]);
        let posed = t.apply_intent(&seq).unwrap();
        let final_waist = posed.joints[&Joint::Waist];
        // 最後の Walk (3.0, 0, 0) が勝つ
        assert!((final_waist[0] - 3.0).abs() < 1e-5);
    }

    #[test]
    fn parallel_applies_first_verb_only_mvp() {
        let t = HumanoidTemplate::default();
        let par = parallel(vec![
            walk(Vec3::new(1.0, 0.0, 0.0), 1.0),
            walk(Vec3::new(5.0, 0.0, 0.0), 1.0),
        ]);
        let posed = t.apply_intent(&par).unwrap();
        let final_waist = posed.joints[&Joint::Waist];
        // MVP: 先頭のみ (1.0, 0, 0)
        assert!((final_waist[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn empty_parallel_returns_clone() {
        let t = HumanoidTemplate::default();
        let par = parallel(vec![]);
        let posed = t.apply_intent(&par).unwrap();
        assert!(joints_close(
            t.joints[&Joint::Waist],
            posed.joints[&Joint::Waist],
            1e-6
        ));
    }
}
