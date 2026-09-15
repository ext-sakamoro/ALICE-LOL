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
    /// - [`RobotError::InvalidLatentDim`]: [`IntentNode::LatentIntent`] の `values.len() < 4`
    ///
    /// # 前処理 (Phase G.1、Garrido latent extension)
    ///
    /// [`IntentNode::LatentIntent`] は [`intent_to_kinematics`] 呼び出し前に
    /// [`decode_latent_intents`] で discrete verb ([`IntentNode::Walk`]) に変換される
    ///
    /// [`intent_to_kinematics`]: alice_kinematics::lol_bridge::intent_to_kinematics
    #[allow(clippy::unused_self)]
    pub fn translate_with_positions(
        &self,
        intent: &IntentNode,
        positions: &[Vec3],
    ) -> Result<Vec<Intent>, RobotError> {
        let decoded = decode_latent_intents(intent)?;
        lol_bridge::intent_to_kinematics(&decoded, positions).map_err(RobotError::from)
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

/// [`IntentNode::LatentIntent`] → discrete verb の再帰的変換 (Phase G.1、Garrido 発想)
///
/// [`IntentNode::Sequence`] / [`IntentNode::Parallel`] は内部を再帰処理
/// その他 discrete verb は clone して pass-through
///
/// # Errors
///
/// - [`RobotError::InvalidLatentDim`]: `LatentIntent::values.len() < 4`
///
/// # Phase 進化
///
/// - **G.1** (本実装): hardcoded linear projection (`values[0..4]` → [`IntentNode::Walk`])
/// - **G.2** (別 sprint): learned controller (cross-attention block、Garrido 準拠、`alice-latent-controller` crate 予定)
/// - **G.3** (別 sprint): verb catalog 統一 (破壊的 root redesign 検討)
fn decode_latent_intents(intent: &IntentNode) -> Result<IntentNode, RobotError> {
    match intent {
        IntentNode::LatentIntent { values, .. } => decode_latent_projection(values),
        IntentNode::Sequence(items) => {
            let decoded = items
                .iter()
                .map(decode_latent_intents)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(IntentNode::Sequence(decoded))
        }
        IntentNode::Parallel(items) => {
            let decoded = items
                .iter()
                .map(decode_latent_intents)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(IntentNode::Parallel(decoded))
        }
        other => Ok(other.clone()),
    }
}

/// Phase G.1 hardcoded linear projection: latent slice → [`IntentNode::Walk`]
///
/// # Mapping
///
/// - `values[0..3]` → `Walk::destination` (world xyz meters)
/// - `values[3]`    → `Walk::speed` (clamp 0.1..2.0 m/s)
/// - `values[4..]`  → reserved (Phase G.2 の learned controller が消費)
///
/// # 選定理由
///
/// [`IntentNode::Walk`] は [`alice_kinematics::lol_bridge`] で単一 packet を確実生成
/// (Reach type、Kinematics scope 内)、Phase G.1 の linear projection target として最適
///
/// # Errors
///
/// `values.len() < 4` で [`RobotError::InvalidLatentDim`]
const fn decode_latent_projection(values: &[f32]) -> Result<IntentNode, RobotError> {
    const MIN_DIM: usize = 4;
    if values.len() < MIN_DIM {
        return Err(RobotError::InvalidLatentDim {
            got: values.len(),
            min: MIN_DIM,
        });
    }
    let destination = Vec3::new(values[0], values[1], values[2]);
    let speed = values[3].clamp(0.1, 2.0);
    Ok(IntentNode::Walk { destination, speed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alice_lol::intent::{grasp, latent_intent, rest, walk, HandSide, ProgramBuilder};
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

    // ── Phase G.1 LatentIntent tests (Garrido arXiv 2601.05230 発想) ──

    #[test]
    fn translate_latent_intent_min_dim_produces_walk_packet() {
        // dim = 4 (min)、[target_x, target_y, target_z, speed]
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(latent_intent(vec![1.0, 0.5, 0.0, 0.8]))
            .build();
        let executor = IntentExecutor::from_program(&program);
        let packets = executor.translate().unwrap();
        assert_eq!(packets.len(), 1); // Walk → 1 packet
    }

    #[test]
    fn translate_latent_intent_128_dim_produces_walk_packet() {
        // Garrido paper default dim = 128、Phase G.1 は先頭 4 要素のみ使用
        #[allow(clippy::cast_precision_loss)]
        let vals: Vec<f32> = (0..128).map(|i| (i as f32) * 0.001).collect();
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(latent_intent(vals))
            .build();
        let executor = IntentExecutor::from_program(&program);
        let packets = executor.translate().unwrap();
        assert_eq!(packets.len(), 1);
    }

    #[test]
    fn translate_latent_intent_below_min_dim_errors() {
        // dim < 4 → InvalidLatentDim error
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(latent_intent(vec![1.0, 0.5])) // dim = 2
            .build();
        let executor = IntentExecutor::from_program(&program);
        let result = executor.translate();
        assert!(matches!(
            result,
            Err(RobotError::InvalidLatentDim { got: 2, min: 4 })
        ));
    }

    #[test]
    fn translate_latent_intent_in_sequence_decodes_all() {
        let seq = IntentNode::Sequence(vec![
            latent_intent(vec![1.0, 0.0, 0.0, 0.5]),
            latent_intent(vec![2.0, 0.0, 0.0, 0.8]),
            latent_intent(vec![3.0, 0.0, 0.0, 1.2]),
        ]);
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(seq)
            .build();
        let executor = IntentExecutor::from_program(&program);
        let packets = executor.translate().unwrap();
        assert_eq!(packets.len(), 3); // 3 x Walk = 3 packet
    }

    #[test]
    fn translate_latent_intent_in_parallel_decodes_all() {
        let par = IntentNode::Parallel(vec![
            latent_intent(vec![1.0, 0.0, 0.0, 0.5]),
            latent_intent(vec![0.0, 1.0, 0.0, 0.5]),
        ]);
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(par)
            .build();
        let executor = IntentExecutor::from_program(&program);
        let packets = executor.translate().unwrap();
        assert_eq!(packets.len(), 2);
    }

    #[test]
    fn translate_latent_speed_clamped_to_max() {
        // values[3] = 100.0 でも Walk::speed は clamp(0.1, 2.0) で 2.0 上限
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .with_intent(latent_intent(vec![1.0, 0.0, 0.0, 100.0]))
            .build();
        let executor = IntentExecutor::from_program(&program);
        let packets = executor.translate().unwrap();
        assert_eq!(packets.len(), 1); // Kinematics 側で reject されず 1 packet 生成
    }

    #[test]
    fn translate_mixed_latent_and_discrete_in_sequence() {
        // LatentIntent と discrete verb の 混在 sequence
        let seq = IntentNode::Sequence(vec![
            latent_intent(vec![0.5, 0.0, 0.5, 0.4]),
            grasp(0, HandSide::Right, 3.0),
        ]);
        let program = make_program_with_target(Vec3::new(0.5, 0.3, 0.2), seq);
        let executor = IntentExecutor::from_program(&program);
        let packets = executor.translate().unwrap();
        assert_eq!(packets.len(), 2); // Walk (from latent) + Grasp
    }

    #[test]
    fn translate_nested_latent_in_sequence_parallel() {
        // Sequence 内に Parallel(LatentIntent) が入るネスト構造
        let complex = IntentNode::Sequence(vec![
            IntentNode::Parallel(vec![
                latent_intent(vec![1.0, 0.0, 0.0, 0.5]),
                latent_intent(vec![0.0, 1.0, 0.0, 0.5]),
            ]),
            grasp(0, HandSide::Right, 3.0),
        ]);
        let program = make_program_with_target(Vec3::new(0.5, 0.3, 0.2), complex);
        let executor = IntentExecutor::from_program(&program);
        let packets = executor.translate().unwrap();
        assert_eq!(packets.len(), 3); // 2 x Walk (parallel) + 1 Grasp
    }
}
