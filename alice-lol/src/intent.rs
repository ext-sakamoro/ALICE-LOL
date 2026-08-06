//! Intent Layer (Milestone B.1: IntentNode 独立 enum + Program 構造)
//!
//! ALICE 三相原理 (Data → Law → Intent) の **Phase 3 Intent** 相の IR skeleton
//! `SdfNode` (Data + Law の IR) とは **独立 enum** として並列に設計
//! `Program { sdf, intent }` で 型分離、GPU backend が誤って Intent を解釈する事故を型で防ぐ
//!
//! # 3 層 Intent 階層
//!
//! - **L1 Physical Intent** (本 module): 身体運動 verb、`IntentNode` 14 variant + Sequence/Parallel
//! - **L2 Social Intent**: 未定義 (Foundry / Anima 成熟後に別 module)
//! - **L3 Architectural Intent**: `ALICE-Cognitive` crate (別 meta-agent)
//!
//! # 実装 scope (B.1 = skeleton のみ)
//!
//! - Enum + struct 定義 + builder + convenience helper
//! - 未実装: 8-byte packet serialize (C.1)、Kinematics 解釈器 (B.3)、LOL DSL intent block 構文 (B.4)

use crate::SdfNode;
use glam::Vec3;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Node reference (SdfNode registry の index)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// `Program::sdf_registry` の index として使う identifier
///
/// `Vec<SdfNode>` の index 相当 (Q_B1_registry (a) 決定)
/// Program が single owner のため単純な u32 で表現
pub type NodeId = u32;

/// 手 / 腕の指定 (verb 引数として使う)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandSide {
    /// 左手
    Left,
    /// 右手
    Right,
    /// 両手
    Both,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// IntentNode enum
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// L1 Physical Intent verb catalog (B.1 MVP scope = 14 verb + 2 合成 = 16 variant)
///
/// Q2 決定通り: grasp / release / walk / gaze / point / throw / catch / push / pull / rotate / align / follow / avoid / rest
/// + `Sequence` (直列合成) + `Parallel` (並列合成)
#[derive(Debug, Clone, PartialEq)]
pub enum IntentNode {
    // ── 単一 verb (14 個) ──
    /// 対象を掴む
    Grasp {
        /// 対象 `NodeId`
        target_id: NodeId,
        /// 使う手
        hand: HandSide,
        /// 把持力
        force: f32,
    },
    /// 対象を離す
    Release {
        /// 対象 `NodeId`
        target_id: NodeId,
    },
    /// 目的地へ歩く
    Walk {
        /// 目的地座標
        destination: Vec3,
        /// 移動速度 (m/s)
        speed: f32,
    },
    /// 目標を注視する
    Gaze {
        /// 注視点座標
        target: Vec3,
        /// 注視時間 (ミリ秒)
        duration_ms: u32,
    },
    /// 目標を指す
    Point {
        /// 指し示す座標
        target: Vec3,
        /// 使う手
        hand: HandSide,
    },
    /// 対象を投げる
    Throw {
        /// 投擲目標座標
        target: Vec3,
        /// 投擲力
        force: f32,
        /// 使う手
        hand: HandSide,
    },
    /// 飛来する対象を捕らえる
    Catch {
        /// 対象 `NodeId`
        object_id: NodeId,
    },
    /// 対象を押す
    Push {
        /// 対象 `NodeId`
        target_id: NodeId,
        /// 押す方向 (単位ベクトル想定)
        direction: Vec3,
        /// 押す力
        force: f32,
    },
    /// 対象を引く
    Pull {
        /// 対象 `NodeId`
        target_id: NodeId,
        /// 引く方向 (単位ベクトル想定)
        direction: Vec3,
        /// 引く力
        force: f32,
    },
    /// 対象を回す
    Rotate {
        /// 対象 `NodeId`
        target_id: NodeId,
        /// 回転軸 (単位ベクトル想定)
        axis: Vec3,
        /// 回転角 (ラジアン)
        angle_rad: f32,
    },
    /// 対象を基準方向に整列
    Align {
        /// 対象 `NodeId`
        target_id: NodeId,
        /// 揃える基準方向 (単位ベクトル想定)
        reference: Vec3,
    },
    /// 対象を追跡する
    Follow {
        /// 追跡対象 `NodeId`
        target_id: NodeId,
        /// 保つ距離
        distance: f32,
    },
    /// 対象を避ける
    Avoid {
        /// 回避対象 `NodeId`
        target_id: NodeId,
        /// 保つ最小距離
        min_distance: f32,
    },
    /// 休止する
    Rest {
        /// 休止時間 (ミリ秒)
        duration_ms: u32,
    },

    // ── 合成 (2 個) ──
    /// 直列合成: 内部の Intent を順に実行
    Sequence(Vec<IntentNode>),
    /// 並列合成: 内部の Intent を同時実行
    Parallel(Vec<IntentNode>),
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Program: SDF + Intent の統合表現
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// SDF (Phase 1 Data + Phase 2 Law の IR) と Intent (Phase 3) の統合構造
///
/// **重要な設計判断 (Q1 (b))**: `sdf` と `intent` は物理的に分離
/// GPU backend は `sdf` field のみ view 可能な API を提供、`intent` は 誤読不能
/// これにより「GPU shader が誤って Intent を解釈する事故」を型システムで防ぐ
#[derive(Debug, Clone)]
pub struct Program {
    /// メインの visual / geometric SDF tree (backend transpile 対象)
    pub sdf: SdfNode,
    /// `IntentNode::*` から `NodeId` で参照される SdfNode の集合 (Intent の対象エンティティ)
    ///
    /// Intent の verb 引数 (`target_id` / `object_id`) は本 registry の index
    pub sdf_registry: Vec<SdfNode>,
    /// この scene で実行すべき Intent tree (`None` = geometry のみ、Intent なし)
    pub intent: Option<IntentNode>,
}

impl Program {
    /// SDF のみの Program を作成 (Intent なし、pure geometry)
    #[must_use]
    pub fn sdf_only(sdf: SdfNode) -> Self {
        Self {
            sdf,
            sdf_registry: Vec::new(),
            intent: None,
        }
    }

    /// Backend 安全 view: GPU 側は本 method 経由でのみ Program にアクセス
    ///
    /// `intent` field は返さない (型で hide)、Data/Law path 専用
    #[must_use]
    pub const fn as_sdf(&self) -> &SdfNode {
        &self.sdf
    }

    /// Intent 有無の判定
    #[must_use]
    pub const fn has_intent(&self) -> bool {
        self.intent.is_some()
    }

    /// registry サイズ
    #[must_use]
    pub fn registry_len(&self) -> usize {
        self.sdf_registry.len()
    }

    /// `NodeId` から SDF を取得
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&SdfNode> {
        self.sdf_registry.get(id as usize)
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ProgramBuilder
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Program 構築のための builder
#[derive(Debug, Default, Clone)]
pub struct ProgramBuilder {
    sdf: Option<SdfNode>,
    registry: Vec<SdfNode>,
    intent: Option<IntentNode>,
}

impl ProgramBuilder {
    /// 空 builder を作成
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// メイン SDF tree を設定
    #[must_use]
    pub fn with_sdf(mut self, sdf: SdfNode) -> Self {
        self.sdf = Some(sdf);
        self
    }

    /// registry にエンティティ追加、割り当てた `NodeId` を返す
    ///
    /// # Panics
    ///
    /// registry が `u32::MAX` を超えると panic (現実的にはあり得ない)
    #[must_use]
    pub fn register(&mut self, node: SdfNode) -> NodeId {
        let id = self.registry.len();
        assert!(
            id <= u32::MAX as usize,
            "sdf_registry index が u32::MAX を超えた"
        );
        self.registry.push(node);
        #[allow(clippy::cast_possible_truncation)]
        (id as NodeId)
    }

    /// intent tree を設定
    #[must_use]
    pub fn with_intent(mut self, intent: IntentNode) -> Self {
        self.intent = Some(intent);
        self
    }

    /// Program を build
    ///
    /// # Panics
    ///
    /// `with_sdf` で SDF が設定されていないと panic
    #[must_use]
    pub fn build(self) -> Program {
        Program {
            sdf: self.sdf.expect("ProgramBuilder::build には with_sdf が必須"),
            sdf_registry: self.registry,
            intent: self.intent,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Convenience helpers (verb constructor 群)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Grasp verb constructor
#[must_use]
pub const fn grasp(target_id: NodeId, hand: HandSide, force: f32) -> IntentNode {
    IntentNode::Grasp {
        target_id,
        hand,
        force,
    }
}

/// Release verb constructor
#[must_use]
pub const fn release(target_id: NodeId) -> IntentNode {
    IntentNode::Release { target_id }
}

/// Walk verb constructor
#[must_use]
pub const fn walk(destination: Vec3, speed: f32) -> IntentNode {
    IntentNode::Walk { destination, speed }
}

/// Gaze verb constructor
#[must_use]
pub const fn gaze(target: Vec3, duration_ms: u32) -> IntentNode {
    IntentNode::Gaze {
        target,
        duration_ms,
    }
}

/// Point verb constructor
#[must_use]
pub const fn point(target: Vec3, hand: HandSide) -> IntentNode {
    IntentNode::Point { target, hand }
}

/// Throw verb constructor
#[must_use]
pub const fn throw(target: Vec3, force: f32, hand: HandSide) -> IntentNode {
    IntentNode::Throw {
        target,
        force,
        hand,
    }
}

/// Catch verb constructor
#[must_use]
pub const fn catch(object_id: NodeId) -> IntentNode {
    IntentNode::Catch { object_id }
}

/// Push verb constructor
#[must_use]
pub const fn push(target_id: NodeId, direction: Vec3, force: f32) -> IntentNode {
    IntentNode::Push {
        target_id,
        direction,
        force,
    }
}

/// Pull verb constructor
#[must_use]
pub const fn pull(target_id: NodeId, direction: Vec3, force: f32) -> IntentNode {
    IntentNode::Pull {
        target_id,
        direction,
        force,
    }
}

/// Rotate verb constructor
#[must_use]
pub const fn rotate(target_id: NodeId, axis: Vec3, angle_rad: f32) -> IntentNode {
    IntentNode::Rotate {
        target_id,
        axis,
        angle_rad,
    }
}

/// Align verb constructor
#[must_use]
pub const fn align(target_id: NodeId, reference: Vec3) -> IntentNode {
    IntentNode::Align {
        target_id,
        reference,
    }
}

/// Follow verb constructor
#[must_use]
pub const fn follow(target_id: NodeId, distance: f32) -> IntentNode {
    IntentNode::Follow {
        target_id,
        distance,
    }
}

/// Avoid verb constructor
#[must_use]
pub const fn avoid(target_id: NodeId, min_distance: f32) -> IntentNode {
    IntentNode::Avoid {
        target_id,
        min_distance,
    }
}

/// Rest verb constructor
#[must_use]
pub const fn rest(duration_ms: u32) -> IntentNode {
    IntentNode::Rest { duration_ms }
}

/// Sequence 合成
#[must_use]
pub fn sequence(intents: Vec<IntentNode>) -> IntentNode {
    IntentNode::Sequence(intents)
}

/// Parallel 合成
#[must_use]
pub fn parallel(intents: Vec<IntentNode>) -> IntentNode {
    IntentNode::Parallel(intents)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;
    // 注: lol! macro は proc_macro が `alice_lol::` を参照するため self crate 内では使えず
    // 代わりに `SdfNode::sphere(r)` 等の direct constructor を使用

    #[test]
    fn program_sdf_only() {
        let sdf = SdfNode::sphere(1.0);
        let prog = Program::sdf_only(sdf);
        assert!(!prog.has_intent());
        assert_eq!(prog.registry_len(), 0);
    }

    #[test]
    fn program_builder_full() {
        let main = SdfNode::sphere(1.0).union(SdfNode::sphere(0.5).translate(2.0, 0.0, 0.0));
        let mut builder = ProgramBuilder::new().with_sdf(main);
        let cup_id = builder.register(SdfNode::sphere(0.3));
        let table_id = builder.register(SdfNode::box3d(1.0, 0.1, 1.0));

        assert_eq!(cup_id, 0);
        assert_eq!(table_id, 1);

        let intent = sequence(vec![
            walk(Vec3::new(1.0, 0.0, 0.0), 0.5),
            grasp(cup_id, HandSide::Right, 5.0),
        ]);

        let prog = builder.with_intent(intent).build();
        assert!(prog.has_intent());
        assert_eq!(prog.registry_len(), 2);
        assert!(prog.get(cup_id).is_some());
        assert!(prog.get(table_id).is_some());
        assert!(prog.get(99).is_none());
    }

    #[test]
    fn all_14_verb_constructors() {
        // 全 14 verb + 2 合成 が問題なく構築できる
        let intents = vec![
            grasp(0, HandSide::Left, 3.0),
            release(0),
            walk(Vec3::new(1.0, 0.0, 0.0), 0.5),
            gaze(Vec3::new(0.0, 1.0, 0.0), 1000),
            point(Vec3::new(2.0, 0.0, 0.0), HandSide::Right),
            throw(Vec3::new(5.0, 3.0, 0.0), 10.0, HandSide::Right),
            catch(1),
            push(0, Vec3::new(1.0, 0.0, 0.0), 2.0),
            pull(0, Vec3::new(-1.0, 0.0, 0.0), 2.0),
            rotate(0, Vec3::new(0.0, 1.0, 0.0), std::f32::consts::FRAC_PI_2),
            align(0, Vec3::new(0.0, 0.0, 1.0)),
            follow(1, 2.0),
            avoid(2, 1.5),
            rest(500),
        ];
        assert_eq!(intents.len(), 14);
    }

    #[test]
    fn sequence_composition() {
        let seq = sequence(vec![
            grasp(0, HandSide::Right, 3.0),
            walk(Vec3::ZERO, 1.0),
            release(0),
        ]);
        match seq {
            IntentNode::Sequence(items) => assert_eq!(items.len(), 3),
            _ => panic!("Sequence を期待"),
        }
    }

    #[test]
    fn parallel_composition() {
        let par = parallel(vec![
            gaze(Vec3::new(1.0, 0.0, 0.0), 500),
            walk(Vec3::new(2.0, 0.0, 0.0), 0.5),
        ]);
        match par {
            IntentNode::Parallel(items) => assert_eq!(items.len(), 2),
            _ => panic!("Parallel を期待"),
        }
    }

    #[test]
    fn nested_composition() {
        // 直列の中に並列 (歩きながら見る + 手を上げる)
        let complex = sequence(vec![
            parallel(vec![
                walk(Vec3::new(3.0, 0.0, 0.0), 0.5),
                gaze(Vec3::new(3.0, 1.0, 0.0), 1500),
            ]),
            grasp(0, HandSide::Right, 4.0),
        ]);
        if let IntentNode::Sequence(items) = &complex {
            assert_eq!(items.len(), 2);
            assert!(matches!(items[0], IntentNode::Parallel(_)));
        } else {
            panic!("Sequence を期待");
        }
    }

    #[test]
    fn backend_safety_gpu_view_only_sdf() {
        // Q1 (b) 型分離の verify: as_sdf() は intent を露出しない
        let sdf = SdfNode::sphere(1.0);
        let prog = ProgramBuilder::new()
            .with_sdf(sdf)
            .with_intent(rest(100))
            .build();

        // GPU backend が想定する形の access — Intent が見えない
        let sdf_ref: &SdfNode = prog.as_sdf();
        assert!(matches!(sdf_ref, SdfNode::Sphere { .. }));
        // intent field への直接 access は Program 型に対する field access が必要 = backend でない前提
    }

    #[test]
    fn hand_side_equality() {
        assert_eq!(HandSide::Left, HandSide::Left);
        assert_ne!(HandSide::Left, HandSide::Right);
        assert_ne!(HandSide::Both, HandSide::Left);
    }

    #[test]
    fn program_get_out_of_range() {
        let prog = ProgramBuilder::new()
            .with_sdf(SdfNode::sphere(1.0))
            .build();
        assert!(prog.get(0).is_none());
    }

    #[test]
    fn intent_clone_and_eq() {
        let g1 = grasp(0, HandSide::Right, 3.0);
        let g2 = g1.clone();
        assert_eq!(g1, g2);

        let g3 = grasp(0, HandSide::Left, 3.0); // hand が違う
        assert_ne!(g1, g3);
    }
}
