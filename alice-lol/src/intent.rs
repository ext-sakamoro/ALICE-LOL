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
//! - LOL text 構文 (B.4 / A0、2026-09-14): `crate::runtime_parser::parse_program` ↔ [`IntentNode::to_lol`]
//! - 未実装: 8-byte packet serialize (C.1)、Kinematics 解釈器 (B.3)

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

/// L1 Physical Intent verb catalog (14 discrete verb + 1 latent (G.1) + 2 合成 = 17 variant)
///
/// Q2 決定通り: grasp / release / walk / gaze / point / throw / catch / push / pull / rotate / align / follow / avoid / rest
/// + `LatentIntent` (Phase G.1、Garrido 発想 constrained continuous latent action)
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

    // ── 連続 latent (1 個、Phase G.1、Garrido arXiv 2601.05230 準拠) ──
    /// Constrained continuous latent action (Garrido arXiv 2601.05230, ICML 2026)
    ///
    /// # 目的
    ///
    /// 14 discrete verb では表現困難な nuanced motion 用の連続 latent slot
    /// Garrido の in-the-wild video 実測 = Noisy / Sparse continuous latent が
    /// vector quantization (discrete codebook) を上回った
    ///
    /// # dim 推奨
    ///
    /// - **32**: compact first-strike (128 byte / intent、既存 8-byte packet の 16x)
    /// - **64**: medium
    /// - **128**: Garrido paper default (512 byte / intent、high-fidelity nuance)
    ///
    /// # 制約 (constrained latent の由来)
    ///
    /// `values` の L2 norm ≤ 1.0 を想定
    /// この制約が Garrido の「future frame を単純 copy 予防」に相当
    /// `alice_lol_robot::law::SafetyLaw` で validation (別 crate 参照、docs.rs cross-link 不可)
    ///
    /// # 実行
    ///
    /// `alice_lol_robot::IntentExecutor` が hardcoded linear projection (Phase G.1)
    /// または learned controller (Phase G.2、cross-attention block) で kinematics packet 群に翻訳
    /// Phase G.1 の projection: `values[0..3]` → target xyz、`values[3]` → speed
    LatentIntent {
        /// constrained continuous latent (dim = `values.len()`、min 4、推奨 32/64/128)
        values: Box<[f32]>,
        /// debug 用 human-readable label ("grasp-like" 等、production は None 推奨)
        semantic_hint: Option<String>,
    },

    // ── 合成 (2 個) ──
    /// 直列合成: 内部の Intent を順に実行
    Sequence(Vec<IntentNode>),
    /// 並列合成: 内部の Intent を同時実行
    Parallel(Vec<IntentNode>),

    // ── L1 Musical Intent (1 個、2026-09-13 Phase 3.1 追加) ──
    /// L1 Musical Intent — 8-byte 音楽的意図 packet
    ///
    /// packet の内訳は `alice_synth::intent::MusicIntent` (byte 0: genre / 1: mood /
    /// 2: length_bars / 3: tempo_bpm_offset / 4: key / 5: mode / 6-7: variation_seed LE)
    /// で正式定義される 本 crate は依存を持たず opaque `[u8; 8]` として保持し、
    /// 解釈は consumer (`alice-synth::intent::MusicIntent::from_bytes(packet)` を叩く
    /// interpreter、または将来の LLM plan head) 側で行う
    ///
    /// ALICE 三相原理の Phase 3 (Intent) を Physical Intent の隣に置く音楽 variant
    /// Kinematics packet と同じ 8-byte サイズで、network 帯域 / storage 効率も同等
    Music {
        /// 8-byte MusicIntent packet (alice-synth `intent` module 参照)
        packet: [u8; 8],
    },
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
            sdf: self
                .sdf
                .expect("ProgramBuilder::build には with_sdf が必須"),
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

/// L1 Musical Intent constructor (Phase 3.1、2026-09-13)
///
/// 8-byte packet を `IntentNode::Music` variant に wrap する
/// packet 内訳は `alice_synth::intent::MusicIntent` 側で正式定義、本 crate は
/// opaque payload として持つ
///
/// ```
/// use alice_lol::intent::music_intent;
///
/// // alice-synth 側で作った 8-byte packet を LOL Intent tree に埋め込む
/// let packet = [0u8, 0, 4, 80, 0, 0, 0xDE, 0xC0]; // C major Folk / Happy, seed 0xC0DE
/// let node = music_intent(packet);
/// ```
#[must_use]
pub const fn music_intent(packet: [u8; 8]) -> IntentNode {
    IntentNode::Music { packet }
}

/// `LatentIntent` constructor (Phase G.1、Garrido 発想 constrained continuous latent action)
///
/// # 引数
///
/// - `values`: min 4 dim、推奨 32/64/128 dim
///   先頭 4 要素は decoder が `[target_x, target_y, target_z, speed]` として解釈
///   残り (dim > 4) は Phase G.2 の learned controller が消費する reserved 領域
///
/// # 制約 (Garrido 由来)
///
/// L2 norm ≤ 1.0 を想定 (`alice_lol_robot::law::SafetyLaw::max_latent_norm` で validation、別 crate)
/// この制約が Garrido の「future frame を単純 copy 予防」の意味を持つ
#[must_use]
pub fn latent_intent(values: impl Into<Box<[f32]>>) -> IntentNode {
    IntentNode::LatentIntent {
        values: values.into(),
        semantic_hint: None,
    }
}

/// `LatentIntent` constructor with debug hint
///
/// production では [`latent_intent`] (hint なし) を推奨、hint は debug / demo 用
#[must_use]
pub fn latent_intent_hinted(values: impl Into<Box<[f32]>>, hint: impl Into<String>) -> IntentNode {
    IntentNode::LatentIntent {
        values: values.into(),
        semantic_hint: Some(hint.into()),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// LOL text 出力 (runtime_parser::parse_program の逆変換)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

impl HandSide {
    /// LOL text 表記 (`left` / `right` / `both`)
    #[must_use]
    pub const fn as_lol(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Both => "both",
        }
    }
}

fn fmt_vec3(v: Vec3) -> String {
    format!("{}, {}, {}", fmt_f32(v.x), fmt_f32(v.y), fmt_f32(v.z))
}

/// `f32` を LOL 数値リテラルとして出力 (`1` → `1.0` で整数と区別、`NaN` / `inf` は不許可)
fn fmt_f32(v: f32) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

impl IntentNode {
    /// Intent tree を LOL text に変換する
    ///
    /// [`crate::runtime_parser::parse_program`] が読み戻せる形式 (`lol.gbnf` の `intent` rule)
    /// `Rotate` は SDF transform の `rotate` と衝突するため verb 名 `turn` で出力
    /// `LatentIntent::semantic_hint` は text 構文に存在しないため落とす
    #[must_use]
    pub fn to_lol(&self) -> String {
        match self {
            Self::Grasp {
                target_id,
                hand,
                force,
            } => format!("grasp({target_id}, {}, {})", hand.as_lol(), fmt_f32(*force)),
            Self::Release { target_id } => format!("release({target_id})"),
            Self::Catch { object_id } => format!("catch({object_id})"),
            Self::Walk { destination, speed } => {
                format!("walk({}, {})", fmt_vec3(*destination), fmt_f32(*speed))
            }
            Self::Gaze {
                target,
                duration_ms,
            } => format!("gaze({}, {duration_ms})", fmt_vec3(*target)),
            Self::Point { target, hand } => {
                format!("point({}, {})", fmt_vec3(*target), hand.as_lol())
            }
            Self::Throw {
                target,
                force,
                hand,
            } => format!(
                "throw({}, {}, {})",
                fmt_vec3(*target),
                fmt_f32(*force),
                hand.as_lol()
            ),
            Self::Push {
                target_id,
                direction,
                force,
            } => format!(
                "push({target_id}, {}, {})",
                fmt_vec3(*direction),
                fmt_f32(*force)
            ),
            Self::Pull {
                target_id,
                direction,
                force,
            } => format!(
                "pull({target_id}, {}, {})",
                fmt_vec3(*direction),
                fmt_f32(*force)
            ),
            Self::Rotate {
                target_id,
                axis,
                angle_rad,
            } => format!(
                "turn({target_id}, {}, {})",
                fmt_vec3(*axis),
                fmt_f32(*angle_rad)
            ),
            Self::Align {
                target_id,
                reference,
            } => format!("align({target_id}, {})", fmt_vec3(*reference)),
            Self::Follow {
                target_id,
                distance,
            } => format!("follow({target_id}, {})", fmt_f32(*distance)),
            Self::Avoid {
                target_id,
                min_distance,
            } => format!("avoid({target_id}, {})", fmt_f32(*min_distance)),
            Self::Rest { duration_ms } => format!("rest({duration_ms})"),
            Self::LatentIntent { values, .. } => {
                let inner: Vec<String> = values.iter().map(|v| fmt_f32(*v)).collect();
                format!("latent({})", inner.join(", "))
            }
            Self::Sequence(items) => {
                let inner: Vec<String> = items.iter().map(Self::to_lol).collect();
                format!("seq({})", inner.join(", "))
            }
            Self::Parallel(items) => {
                let inner: Vec<String> = items.iter().map(Self::to_lol).collect();
                format!("par({})", inner.join(", "))
            }
            Self::Music { packet } => {
                let inner: Vec<String> = packet.iter().map(u8::to_string).collect();
                format!("music({})", inner.join(", "))
            }
        }
    }
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
        let prog = ProgramBuilder::new().with_sdf(SdfNode::sphere(1.0)).build();
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

    // ── Phase G.1 LatentIntent tests (Garrido arXiv 2601.05230 発想) ──

    #[test]
    fn latent_intent_construction_min_dim() {
        let li = latent_intent(vec![0.5, 0.3, 0.2, 0.8]);
        match &li {
            IntentNode::LatentIntent {
                values,
                semantic_hint,
            } => {
                assert_eq!(values.len(), 4);
                assert!(semantic_hint.is_none());
                assert!((values[0] - 0.5).abs() < 1e-6);
                assert!((values[3] - 0.8).abs() < 1e-6);
            }
            _ => panic!("LatentIntent を期待"),
        }
    }

    #[test]
    fn latent_intent_with_hint() {
        let li = latent_intent_hinted(vec![0.1, 0.2, 0.3, 0.4], "grasp-like");
        match li {
            IntentNode::LatentIntent {
                values,
                semantic_hint,
            } => {
                assert_eq!(values.len(), 4);
                assert_eq!(semantic_hint.as_deref(), Some("grasp-like"));
            }
            _ => panic!("LatentIntent を期待"),
        }
    }

    #[test]
    fn latent_intent_dims_variety_32_64_128() {
        // Phase G.1 推奨 3 dim (compact / medium / Garrido default) 全て正常構築
        for dim in [32_usize, 64, 128] {
            #[allow(clippy::cast_precision_loss)]
            let vals: Vec<f32> = (0..dim).map(|i| (i as f32) * 0.01).collect();
            let li = latent_intent(vals);
            if let IntentNode::LatentIntent { values, .. } = li {
                assert_eq!(values.len(), dim);
            } else {
                panic!("LatentIntent を期待 (dim = {dim})");
            }
        }
    }

    #[test]
    fn latent_intent_composable_in_sequence() {
        let seq = sequence(vec![
            latent_intent(vec![1.0, 0.0, 0.0, 0.5]),
            grasp(0, HandSide::Right, 3.0),
            latent_intent_hinted(vec![0.5, 0.5, 0.0, 0.3], "release-approach"),
        ]);
        if let IntentNode::Sequence(items) = &seq {
            assert_eq!(items.len(), 3);
            assert!(matches!(items[0], IntentNode::LatentIntent { .. }));
            assert!(matches!(items[2], IntentNode::LatentIntent { .. }));
        } else {
            panic!("Sequence を期待");
        }
    }

    #[test]
    fn latent_intent_composable_in_parallel() {
        let par = parallel(vec![
            latent_intent(vec![0.1, 0.2, 0.3, 0.5]),
            latent_intent(vec![0.4, 0.5, 0.6, 0.7]),
        ]);
        if let IntentNode::Parallel(items) = par {
            assert_eq!(items.len(), 2);
            assert!(matches!(items[0], IntentNode::LatentIntent { .. }));
        } else {
            panic!("Parallel を期待");
        }
    }

    #[test]
    fn latent_intent_clone_and_eq() {
        let a = latent_intent(vec![1.0, 2.0, 3.0, 4.0]);
        let b = a.clone();
        assert_eq!(a, b);
        let c = latent_intent(vec![1.0, 2.0, 3.0, 5.0]); // 最後の要素 差
        assert_ne!(a, c);
    }

    // ── L1 Musical Intent (Phase 3.1、2026-09-13) ──

    #[test]
    fn music_intent_constructor_wraps_packet() {
        let packet = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let node = music_intent(packet);
        match node {
            IntentNode::Music { packet: p } => assert_eq!(p, packet),
            _ => panic!("Music variant を期待"),
        }
    }

    #[test]
    fn music_intent_roundtrip_via_variant() {
        // alice-synth の canonical 8-byte layout に対応する packet を wrap して
        // Match で取り出し、byte 一致することを確認する
        let packet = [0u8, 0, 4, 80, 0, 0, 0xDE, 0xC0]; // C major Folk / Happy, seed 0xC0DE
        let node = music_intent(packet);
        if let IntentNode::Music { packet: extracted } = node {
            assert_eq!(extracted[0], 0); // genre::FOLK
            assert_eq!(extracted[3], 80); // tempo offset → 120 BPM
            assert_eq!(u16::from_le_bytes([extracted[6], extracted[7]]), 0xC0DE);
        } else {
            panic!("Music variant を期待");
        }
    }

    #[test]
    fn music_intent_composable_in_sequence() {
        let packet_a = [0u8, 0, 4, 80, 0, 0, 0x11, 0x11];
        let packet_b = [1u8, 4, 8, 60, 4, 5, 0x22, 0x22];
        let seq = sequence(vec![music_intent(packet_a), music_intent(packet_b)]);
        if let IntentNode::Sequence(items) = &seq {
            assert_eq!(items.len(), 2);
            assert!(matches!(items[0], IntentNode::Music { .. }));
            assert!(matches!(items[1], IntentNode::Music { .. }));
        } else {
            panic!("Sequence を期待");
        }
    }

    #[test]
    fn music_intent_composable_with_physical_verbs() {
        // Musical + Physical Intent を並列に走らせるシナリオ (演奏中の身体動作等)
        let par = parallel(vec![
            music_intent([0u8, 0, 4, 80, 0, 0, 0xC0, 0xDE]),
            grasp(0, HandSide::Right, 5.0),
        ]);
        if let IntentNode::Parallel(items) = par {
            assert_eq!(items.len(), 2);
            assert!(matches!(items[0], IntentNode::Music { .. }));
            assert!(matches!(items[1], IntentNode::Grasp { .. }));
        } else {
            panic!("Parallel を期待");
        }
    }

    #[test]
    fn music_intent_clone_and_eq() {
        let a = music_intent([1u8, 2, 3, 4, 5, 6, 7, 8]);
        let b = a.clone();
        assert_eq!(a, b);
        let c = music_intent([1u8, 2, 3, 4, 5, 6, 7, 9]); // 末尾 1 byte 差
        assert_ne!(a, c);
    }
}
