//! [`SafetyLaw`] — ISO 10218 参考の robot safety 静的検査
//!
//! LOL [`Program`] の Intent tree を走査し、以下 3 rule で verb 単位に safety 判定:
//!
//! - **Overspeed**: `Walk::speed` > `max_linear_velocity`、`Rotate::angle_rad` >
//!   `max_angular_step`
//! - **`OutOfWorkspace`**: `Walk::destination` / `Point::target` / `Throw::target` /
//!   `Gaze::target` が workspace AABB を超える
//! - **Overforce**: `Grasp::force` / `Push::force` / `Pull::force` / `Throw::force` >
//!   `max_force`
//!
//! # ISO 10218 参考
//!
//! - Part 1: 産業用マニピュレータ本体の safety 要求
//! - Part 2: robot system 統合の safety 要求
//! - 本 crate は **静的仕様検査のみ**、実機 safety-rated stop / speed monitoring は
//!   下流 controller (ROS2 industrial / URDF / メーカー独自 firmware) に委任
//!
//! # MVP scope 制限
//!
//! - **`Sequence` 内の joint velocity 累積** は考慮しない (Phase R.6+ で timing model 導入)
//! - **Collision** は [`Program`] 単一で判断不能 (env geometry 別途) なので本 rule set 対象外
//! - **`Grasp` / `Point` の hand target 距離** は `sdf_registry` 参照時のみ判定可

use alice_lol::intent::{IntentNode, Program};
use glam::Vec3;

use crate::error::RobotError;

/// Safety rule set の閾値 (ISO 10218 参考値 + robot arm 一般値)
#[derive(Debug, Clone, Copy)]
pub struct SafetyLaw {
    /// 最大線速度 [m/s] (ISO 10218-1: 手動誘導時 0.25 m/s、collab robot 1.5 m/s 前後)
    pub max_linear_velocity: f32,
    /// 最大回転角 step [rad] (1 step Intent 内での回転量、π/2 = 90° を default)
    pub max_angular_step: f32,
    /// workspace AABB min [m]、範囲外 target は `OutOfWorkspace`
    pub workspace_min: Vec3,
    /// workspace AABB max
    pub workspace_max: Vec3,
    /// 最大 verb 印加力 [N] (`Grasp` / `Push` / `Pull` / `Throw`)
    pub max_force: f32,
}

impl SafetyLaw {
    /// canonical default (産業用 collab robot 想定、緩めの値)
    ///
    /// - `max_linear_velocity`: 1.5 m/s
    /// - `max_angular_step`: π/2 (90°)
    /// - workspace: ±3 m cube (肩基準の一般的 reach + buffer)
    /// - `max_force`: 150 N (人間衝突 pain threshold 目安)
    #[must_use]
    pub const fn default_collab() -> Self {
        Self {
            max_linear_velocity: 1.5,
            max_angular_step: core::f32::consts::FRAC_PI_2,
            workspace_min: Vec3::new(-3.0, -1.0, -3.0),
            workspace_max: Vec3::new(3.0, 3.0, 3.0),
            max_force: 150.0,
        }
    }

    /// 静的検査: [`Program`] 全体の Intent tree を走査
    ///
    /// # Returns
    ///
    /// 見つかった全 violation を Vec で返す (空 Vec = 安全)
    #[must_use]
    pub fn check_program(&self, program: &Program) -> Vec<SafetyViolation> {
        let mut violations = Vec::new();
        if let Some(intent) = program.intent.as_ref() {
            self.walk_intent(intent, &mut violations);
        }
        violations
    }

    /// 単一 verb の検査 (`Sequence` / `Parallel` は展開)
    #[must_use]
    pub fn check_intent(&self, intent: &IntentNode) -> Vec<SafetyViolation> {
        let mut violations = Vec::new();
        self.walk_intent(intent, &mut violations);
        violations
    }

    fn walk_intent(&self, intent: &IntentNode, violations: &mut Vec<SafetyViolation>) {
        match intent {
            IntentNode::Walk { destination, speed } => {
                if *speed > self.max_linear_velocity {
                    violations.push(SafetyViolation {
                        rule: "Overspeed",
                        detail: format!(
                            "Walk::speed {speed:.2} m/s > max {:.2} m/s",
                            self.max_linear_velocity
                        ),
                    });
                }
                self.check_workspace("Walk::destination", *destination, violations);
            }
            IntentNode::Point { target, .. }
            | IntentNode::Throw { target, .. }
            | IntentNode::Gaze { target, .. } => {
                self.check_workspace(intent_verb_label(intent), *target, violations);
            }
            IntentNode::Rotate { angle_rad, .. } => {
                if angle_rad.abs() > self.max_angular_step {
                    violations.push(SafetyViolation {
                        rule: "Overspeed",
                        detail: format!(
                            "Rotate::angle_rad {:.2} rad > max {:.2} rad step",
                            angle_rad.abs(),
                            self.max_angular_step
                        ),
                    });
                }
            }
            IntentNode::Grasp { force, .. }
            | IntentNode::Push { force, .. }
            | IntentNode::Pull { force, .. } => {
                if *force > self.max_force {
                    violations.push(SafetyViolation {
                        rule: "Overforce",
                        detail: format!(
                            "{verb}::force {force:.1} N > max {:.1} N",
                            self.max_force,
                            verb = intent_verb_label(intent)
                        ),
                    });
                }
            }
            IntentNode::Sequence(children) | IntentNode::Parallel(children) => {
                for child in children {
                    self.walk_intent(child, violations);
                }
            }
            IntentNode::Release { .. }
            | IntentNode::Catch { .. }
            | IntentNode::Align { .. }
            | IntentNode::Follow { .. }
            | IntentNode::Avoid { .. }
            | IntentNode::Rest { .. } => {
                // safety rule なし (MVP scope)
            }
        }
    }

    fn check_workspace(
        &self,
        label: &'static str,
        target: Vec3,
        violations: &mut Vec<SafetyViolation>,
    ) {
        let out_of_bounds = target.x < self.workspace_min.x
            || target.x > self.workspace_max.x
            || target.y < self.workspace_min.y
            || target.y > self.workspace_max.y
            || target.z < self.workspace_min.z
            || target.z > self.workspace_max.z;

        if out_of_bounds {
            violations.push(SafetyViolation {
                rule: "OutOfWorkspace",
                detail: format!(
                    "{label} target ({:.2}, {:.2}, {:.2}) outside workspace ({:.1},{:.1},{:.1})-({:.1},{:.1},{:.1})",
                    target.x,
                    target.y,
                    target.z,
                    self.workspace_min.x,
                    self.workspace_min.y,
                    self.workspace_min.z,
                    self.workspace_max.x,
                    self.workspace_max.y,
                    self.workspace_max.z
                ),
            });
        }
    }

    /// 便利関数: `check_program` の結果を `Result` に変換
    ///
    /// # Errors
    ///
    /// 最初の violation を [`RobotError::SafetyViolated`] で返す
    pub fn guard_program(&self, program: &Program) -> Result<(), RobotError> {
        let violations = self.check_program(program);
        violations.into_iter().next().map_or(Ok(()), |v| {
            Err(RobotError::SafetyViolated {
                rule: v.rule,
                detail: v.detail,
            })
        })
    }
}

/// 個別の safety 違反
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafetyViolation {
    /// 違反した rule 名 (`Overspeed` / `OutOfWorkspace` / `Overforce`)
    pub rule: &'static str,
    /// 詳細メッセージ
    pub detail: String,
}

/// verb → label 変換 (violation メッセージ用)
#[allow(clippy::match_same_arms)]
const fn intent_verb_label(intent: &IntentNode) -> &'static str {
    match intent {
        IntentNode::Grasp { .. } => "Grasp",
        IntentNode::Release { .. } => "Release",
        IntentNode::Walk { .. } => "Walk",
        IntentNode::Gaze { .. } => "Gaze::target",
        IntentNode::Point { .. } => "Point::target",
        IntentNode::Throw { .. } => "Throw::target",
        IntentNode::Catch { .. } => "Catch",
        IntentNode::Push { .. } => "Push",
        IntentNode::Pull { .. } => "Pull",
        IntentNode::Rotate { .. } => "Rotate",
        IntentNode::Align { .. } => "Align",
        IntentNode::Follow { .. } => "Follow",
        IntentNode::Avoid { .. } => "Avoid",
        IntentNode::Rest { .. } => "Rest",
        IntentNode::Sequence(_) => "Sequence",
        IntentNode::Parallel(_) => "Parallel",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alice_lol::intent::{
        gaze, grasp, point, push, rotate, sequence, walk, HandSide, ProgramBuilder,
    };
    use alice_lol::SdfNode;

    #[test]
    fn default_collab_has_reasonable_values() {
        let law = SafetyLaw::default_collab();
        assert!((law.max_linear_velocity - 1.5).abs() < 1e-5);
        assert!(law.max_force > 100.0);
    }

    #[test]
    fn walk_within_speed_limit_passes() {
        let law = SafetyLaw::default_collab();
        let intent = walk(Vec3::new(1.0, 0.0, 0.0), 1.0);
        let violations = law.check_intent(&intent);
        assert!(violations.is_empty());
    }

    #[test]
    fn walk_over_speed_limit_triggers_overspeed() {
        let law = SafetyLaw::default_collab();
        let intent = walk(Vec3::new(1.0, 0.0, 0.0), 5.0);
        let violations = law.check_intent(&intent);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "Overspeed");
    }

    #[test]
    fn walk_out_of_workspace_triggers_bounds_violation() {
        let law = SafetyLaw::default_collab();
        let intent = walk(Vec3::new(100.0, 0.0, 0.0), 0.5);
        let violations = law.check_intent(&intent);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "OutOfWorkspace");
    }

    #[test]
    fn point_target_out_of_workspace_triggers() {
        let law = SafetyLaw::default_collab();
        let intent = point(Vec3::new(0.0, 50.0, 0.0), HandSide::Right);
        let violations = law.check_intent(&intent);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "OutOfWorkspace");
    }

    #[test]
    fn gaze_target_bounds_check() {
        let law = SafetyLaw::default_collab();
        let intent = gaze(Vec3::new(0.0, 0.0, -100.0), 500);
        let violations = law.check_intent(&intent);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "OutOfWorkspace");
    }

    #[test]
    fn rotate_over_angular_step_triggers_overspeed() {
        let law = SafetyLaw::default_collab();
        let intent = rotate(0, Vec3::Y, std::f32::consts::PI);
        let violations = law.check_intent(&intent);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "Overspeed");
    }

    #[test]
    fn grasp_over_force_triggers_overforce() {
        let law = SafetyLaw::default_collab();
        let intent = grasp(0, HandSide::Right, 500.0);
        let violations = law.check_intent(&intent);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "Overforce");
    }

    #[test]
    fn push_over_force_triggers_overforce() {
        let law = SafetyLaw::default_collab();
        let intent = push(0, Vec3::X, 300.0);
        let violations = law.check_intent(&intent);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "Overforce");
    }

    #[test]
    fn safe_grasp_passes() {
        let law = SafetyLaw::default_collab();
        let intent = grasp(0, HandSide::Right, 10.0);
        let violations = law.check_intent(&intent);
        assert!(violations.is_empty());
    }

    #[test]
    fn sequence_with_mixed_verbs_detects_all_violations() {
        let law = SafetyLaw::default_collab();
        let intent = sequence(vec![
            walk(Vec3::new(1.0, 0.0, 0.0), 5.0), // Overspeed
            grasp(0, HandSide::Right, 500.0),    // Overforce
            walk(Vec3::new(1.0, 0.0, 0.0), 0.5), // OK
        ]);
        let violations = law.check_intent(&intent);
        assert_eq!(violations.len(), 2);
    }

    #[test]
    fn check_program_walks_intent_tree() {
        let law = SafetyLaw::default_collab();
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(walk(Vec3::new(0.0, 0.0, 0.0), 10.0))
            .build();
        let violations = law.check_program(&program);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "Overspeed");
    }

    #[test]
    fn program_without_intent_has_no_violations() {
        let law = SafetyLaw::default_collab();
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .build();
        let violations = law.check_program(&program);
        assert!(violations.is_empty());
    }

    #[test]
    fn guard_program_returns_ok_when_safe() {
        let law = SafetyLaw::default_collab();
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(walk(Vec3::new(1.0, 0.0, 0.0), 1.0))
            .build();
        assert!(law.guard_program(&program).is_ok());
    }

    #[test]
    fn guard_program_returns_first_violation() {
        let law = SafetyLaw::default_collab();
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(walk(Vec3::new(1.0, 0.0, 0.0), 100.0))
            .build();
        let result = law.guard_program(&program);
        assert!(matches!(
            result,
            Err(RobotError::SafetyViolated {
                rule: "Overspeed",
                ..
            })
        ));
    }
}
