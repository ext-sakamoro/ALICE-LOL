//! Robot error 型
//!
//! `IntentExecutor` / `HumanoidIntent` / `SafetyLaw` の 3 経路共通 error 型
//! `alice_kinematics::lol_bridge::TranslationError` を wrap する (feature = kinematics)

use alice_lol::intent::NodeId;

/// Robot 実装層の error
///
/// LOL Intent → Kinematics packet / humanoid pose / safety 検査 の 3 経路 共通
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RobotError {
    /// LOL verb が Kinematics scope 外で翻訳不能
    ///
    /// `Gaze` / `Rotate` / `Align` は body kinematics ではなく物体操作 or 視線制御なので
    /// 8-byte packet 経路では表現できない (Foundry / Social Intent 経路で表現される予定)
    Unmappable(&'static str),

    /// [`NodeId`] が `Program::sdf_registry` の range 外
    OutOfRange {
        /// 参照された [`NodeId`]
        id: NodeId,
        /// 実際の registry 長
        len: usize,
    },

    /// humanoid pose 適用失敗 (bone name mismatch / joint 未定義 等)
    PoseFail(&'static str),

    /// safety 検査で verb が禁止条件に該当
    SafetyViolated {
        /// 違反した rule 名
        rule: &'static str,
        /// 詳細 (例: "joint velocity 4.2 rad/s > max 3.0 rad/s")
        detail: String,
    },
}

impl core::fmt::Display for RobotError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unmappable(verb) => write!(f, "verb '{verb}' unmappable to kinematics packet"),
            Self::OutOfRange { id, len } => {
                write!(f, "NodeId {id} out of range (registry len = {len})")
            }
            Self::PoseFail(msg) => write!(f, "humanoid pose apply failed: {msg}"),
            Self::SafetyViolated { rule, detail } => {
                write!(f, "safety rule '{rule}' violated: {detail}")
            }
        }
    }
}

impl std::error::Error for RobotError {}

#[cfg(feature = "kinematics")]
impl From<alice_kinematics::lol_bridge::TranslationError> for RobotError {
    fn from(err: alice_kinematics::lol_bridge::TranslationError) -> Self {
        use alice_kinematics::lol_bridge::TranslationError;
        match err {
            TranslationError::Unmappable(verb) => Self::Unmappable(verb),
            TranslationError::OutOfRange { id, len } => Self::OutOfRange { id, len },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmappable_display() {
        let e = RobotError::Unmappable("Gaze");
        assert_eq!(
            format!("{e}"),
            "verb 'Gaze' unmappable to kinematics packet"
        );
    }

    #[test]
    fn out_of_range_display() {
        let e = RobotError::OutOfRange { id: 5, len: 2 };
        assert_eq!(format!("{e}"), "NodeId 5 out of range (registry len = 2)");
    }

    #[test]
    fn pose_fail_display() {
        let e = RobotError::PoseFail("LShoulder joint missing");
        assert_eq!(
            format!("{e}"),
            "humanoid pose apply failed: LShoulder joint missing"
        );
    }

    #[test]
    fn safety_violated_display() {
        let e = RobotError::SafetyViolated {
            rule: "Overspeed",
            detail: "joint velocity 4.2 rad/s > max 3.0 rad/s".to_string(),
        };
        assert!(format!("{e}").contains("Overspeed"));
        assert!(format!("{e}").contains("4.2 rad/s"));
    }
}
