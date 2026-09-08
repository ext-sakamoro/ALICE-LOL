//! `IntentExecutor` — LOL `IntentNode` を 8-byte Kinematics packet に翻訳
//!
//! [`alice_kinematics::lol_bridge::intent_to_kinematics`] を wrap し、
//! [`Program::sdf_registry`] から [`NodeId`](alice_lol::intent::NodeId) → world Vec3
//! の resolution を自動化する

use alice_kinematics::intent::Intent;
use alice_kinematics::lol_bridge;
use alice_lol::intent::{IntentNode, Program};
use alice_lol::SdfNode;
use glam::Vec3;

use crate::error::RobotError;

/// LOL Intent → Kinematics 8-byte packet 翻訳器
///
/// # 設計方針
///
/// - [`Program::sdf_registry`] の各 [`SdfNode`] から centroid (position) を **heuristic 抽出**
/// - [`IntentNode`] を [`alice_kinematics::lol_bridge::intent_to_kinematics`] に流す
/// - 結果は [`Vec`]<[`alice_kinematics::intent::Intent`]> (8-byte packet 群)
///
/// # centroid heuristic
///
/// MVP scope では以下 2 pattern のみ handle:
///
/// - [`SdfNode::Translate`] `{ child, offset }` → `offset` を position とする
/// - 他 primitive ([`SdfNode::Sphere`] / [`SdfNode::Box3d`] / etc) → [`Vec3::ZERO`] (原点想定)
///
/// より精密な centroid が必要な場合は [`IntentExecutor::translate_with_positions`] を使用
///
/// # Examples
///
/// ```ignore
/// use alice_lol::intent::{grasp, HandSide, ProgramBuilder};
/// use alice_lol_robot::IntentExecutor;
/// use alice_lol::SdfNode;
/// use glam::Vec3;
/// use std::sync::Arc;
///
/// // 対象を (1, 0, 0.5) に置く
/// let target = SdfNode::Translate {
///     child: Arc::new(SdfNode::Sphere { radius: 0.05 }),
///     offset: Vec3::new(1.0, 0.0, 0.5),
/// };
/// let mut builder = ProgramBuilder::new().with_sdf(SdfNode::Sphere { radius: 0.5 });
/// let id = builder.register(target);
/// let program = builder.with_intent(grasp(id, HandSide::Right, 3.0)).build();
///
/// let executor = IntentExecutor::from_program(&program);
/// let packets = executor.translate().unwrap();
/// assert_eq!(packets.len(), 1);
/// ```
#[derive(Debug, Clone)]
pub struct IntentExecutor<'a> {
    /// 参照する LOL [`Program`]
    program: &'a Program,
}

impl<'a> IntentExecutor<'a> {
    /// [`Program`] の参照から executor を作成
    #[must_use]
    pub const fn from_program(program: &'a Program) -> Self {
        Self { program }
    }

    /// `sdf_registry` から heuristic に position を抽出
    ///
    /// [`SdfNode::Translate`] は `offset` を採用、他 primitive は原点
    #[must_use]
    pub fn resolve_positions(&self) -> Vec<Vec3> {
        self.program
            .sdf_registry
            .iter()
            .map(centroid_heuristic)
            .collect()
    }

    /// [`IntentNode`] → 8-byte packet 群 に翻訳
    ///
    /// # Errors
    ///
    /// - [`RobotError::Unmappable`]: `Gaze` / `Rotate` / `Align` verb
    /// - [`RobotError::OutOfRange`]: [`NodeId`](alice_lol::intent::NodeId) が
    ///   `sdf_registry` の range 外
    ///
    /// # Semantics
    ///
    /// - [`IntentNode::Rest`] verb は空 [`Vec`] を返す (motion なし)
    /// - [`IntentNode::Sequence`] / [`IntentNode::Parallel`] は flatten される
    ///   ([`IntentNode::Parallel`] の concurrency は失われる)
    pub fn translate(&self) -> Result<Vec<Intent>, RobotError> {
        let Some(intent) = self.program.intent.as_ref() else {
            return Ok(Vec::new());
        };
        let positions = self.resolve_positions();
        self.translate_with_positions(intent, &positions)
    }

    /// 明示的な `positions` slice を使って翻訳
    ///
    /// centroid heuristic を bypass したい (実座標を別経路で持っている) 場合に使う
    ///
    /// # Errors
    ///
    /// - [`RobotError::Unmappable`]: verb が Kinematics scope 外
    /// - [`RobotError::OutOfRange`]: [`NodeId`](alice_lol::intent::NodeId) が
    ///   `positions` slice の range 外
    #[allow(clippy::unused_self)]
    pub fn translate_with_positions(
        &self,
        intent: &IntentNode,
        positions: &[Vec3],
    ) -> Result<Vec<Intent>, RobotError> {
        lol_bridge::intent_to_kinematics(intent, positions).map_err(RobotError::from)
    }
}

/// [`SdfNode`] の centroid heuristic (MVP scope)
///
/// - [`SdfNode::Translate`] `{ offset, .. }` → `offset`
/// - 他 → [`Vec3::ZERO`]
#[inline]
const fn centroid_heuristic(node: &SdfNode) -> Vec3 {
    match node {
        SdfNode::Translate { offset, .. } => *offset,
        _ => Vec3::ZERO,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alice_lol::intent::{grasp, rest, walk, HandSide, ProgramBuilder};
    use std::sync::Arc;

    fn make_program_with_target(target_pos: Vec3, intent: IntentNode) -> Program {
        let target = SdfNode::Translate {
            child: Arc::new(SdfNode::Sphere { radius: 0.05 }),
            offset: target_pos,
        };
        let mut builder = ProgramBuilder::new().with_sdf(SdfNode::Sphere { radius: 0.5 });
        let _id = builder.register(target);
        builder.with_intent(intent).build()
    }

    #[test]
    fn resolve_positions_extracts_translate_offset() {
        let target = SdfNode::Translate {
            child: Arc::new(SdfNode::Sphere { radius: 0.05 }),
            offset: Vec3::new(1.0, 2.0, 3.0),
        };
        let mut builder = ProgramBuilder::new().with_sdf(SdfNode::Sphere { radius: 0.5 });
        let _id = builder.register(target);
        let program = builder.build();
        let executor = IntentExecutor::from_program(&program);
        let positions = executor.resolve_positions();
        assert_eq!(positions.len(), 1);
        assert!((positions[0] - Vec3::new(1.0, 2.0, 3.0)).length() < 1e-6);
    }

    #[test]
    fn resolve_positions_bare_sphere_yields_origin() {
        let mut builder = ProgramBuilder::new().with_sdf(SdfNode::Sphere { radius: 0.5 });
        let _id = builder.register(SdfNode::Sphere { radius: 0.1 });
        let program = builder.build();
        let executor = IntentExecutor::from_program(&program);
        let positions = executor.resolve_positions();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0], Vec3::ZERO);
    }

    #[test]
    fn translate_grasp_produces_one_packet() {
        let program =
            make_program_with_target(Vec3::new(0.5, 0.3, 0.2), grasp(0, HandSide::Right, 3.0));
        let executor = IntentExecutor::from_program(&program);
        let packets = executor.translate().unwrap();
        assert_eq!(packets.len(), 1);
    }

    #[test]
    fn translate_walk_produces_reach_packet() {
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(walk(Vec3::new(5.0, 0.0, 0.0), 1.5))
            .build();
        let executor = IntentExecutor::from_program(&program);
        let packets = executor.translate().unwrap();
        assert_eq!(packets.len(), 1);
    }

    #[test]
    fn translate_rest_produces_empty_vec() {
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(rest(1000))
            .build();
        let executor = IntentExecutor::from_program(&program);
        let packets = executor.translate().unwrap();
        assert_eq!(packets.len(), 0);
    }

    #[test]
    fn translate_no_intent_yields_empty() {
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .build();
        let executor = IntentExecutor::from_program(&program);
        let packets = executor.translate().unwrap();
        assert!(packets.is_empty());
    }

    #[test]
    fn translate_out_of_range_node_id_errors() {
        // registry 空、NodeId 5 は out of range
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(grasp(5, HandSide::Right, 3.0))
            .build();
        let executor = IntentExecutor::from_program(&program);
        let result = executor.translate();
        assert!(matches!(
            result,
            Err(RobotError::OutOfRange { id: 5, len: 0 })
        ));
    }

    #[test]
    fn translate_gaze_is_unmappable() {
        use alice_lol::intent::gaze;
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(gaze(Vec3::new(0.0, 1.5, 2.0), 500))
            .build();
        let executor = IntentExecutor::from_program(&program);
        let result = executor.translate();
        assert!(matches!(result, Err(RobotError::Unmappable(_))));
    }

    #[test]
    fn translate_sequence_flattens_verbs() {
        let seq = IntentNode::Sequence(vec![
            grasp(0, HandSide::Right, 3.0),
            walk(Vec3::new(1.0, 0.0, 0.0), 1.0),
        ]);
        let program = make_program_with_target(Vec3::new(0.5, 0.3, 0.2), seq);
        let executor = IntentExecutor::from_program(&program);
        let packets = executor.translate().unwrap();
        assert_eq!(packets.len(), 2);
    }
}
