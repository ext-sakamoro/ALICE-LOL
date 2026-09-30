//! 法則（Law）制約チェッカー
//!
//! SDF ツリーに対して物理的・幾何学的制約を宣言し、
//! 違反領域を空間的に特定する。
//!
//! 検出方式: グリッド点サンプリング + 区間演算による AABB レポート
//!
//! ソルバーをブラックボックスにしない設計原則:
//! - 全制約の残差（violation magnitude）を公開
//! - 違反領域の AABB を空間的にレポート
//! - ハード/ソフト制約の明示的な優先度宣言
//! - **判定不能を合格にしない**: 決められなかったセルは [`LawReport::unresolved`]
//!   に載り、[`LawReport::all_passed`] は「違反なし」ではなく「全て証明済」を意味する
//!
//! # 距離依存 law の判定原理 (0.4.0、Lipschitz 非依存)
//!
//! `MinThickness` / `Stress` / `NonOverlap` / `Containment` / `Contact` は
//! 「表面までの距離」を問うが、SDF の場の値 `f(p)` は一般に距離ではない
//! (TPMS は √3〜7 倍に過大、union の内部は過小、`eval_lipschitz` の L は
//! 外部 `{f ≥ 0}` でしか保証されない) そこで場の値を距離に使わず、
//! **符号の正しさと連続性だけ**に依存する 2 つの道具で判定する:
//!
//! - [`alice_sdf::interval::eval_interval`] (区間演算の包含) — 箱の中の場が
//!   一様に同符号なら、その箱に表面はない (証明)
//! - 点評価 — `f(p) < 0` の p と `f(q) ≥ 0` の q があれば、線分 p–q 上に
//!   表面がある (中間値定理) ので `dist(p, 表面) ≤ |p − q|` (証拠)
//!
//! 半径 r の球を八分木で細分し、全ての葉が同符号なら「表面まで ≥ r」、
//! 反対符号の点が見つかれば「表面まで ≤ |p − q|」(二分探索で締める)、
//! 深さ上限 ([`BALL_PROBE_DEPTH`]) で未決定の葉が残れば **unresolved**
//! 違反の検出は健全 (偽陽性なし)、合格は標本点ごとの証明 (格子解像度に依存)

use alice_sdf::interval::{eval_lipschitz, Interval, Vec3Interval};
use alice_sdf::SdfNode;
use glam::Vec3;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 型定義
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 制約の優先度
///
/// [`Hard`](Self::Hard) を名乗れるのは **証明か反例を返せる制約だけ** で、
/// モデルによる推定しか返せない制約は [`Law::hard`] を通らない
/// ([`Constraint::evidence_class`] / [`Evidence`] 参照)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Priority {
    /// 絶対不可侵 — 違反はエラー
    Hard,
    /// エネルギー最小化 — 違反は警告 + 残差で重み付け
    Soft(f32),
}

/// 判定の根拠の種類
///
/// この検証器が「違反である」と言う時、その主張の裏付けには 3 つの強さがある
/// 区別しないと、**格子解像度に依存した推定が証明と同じ重み ([`Priority::Hard`])
/// で報告される** — 0.4.0 までの `Thermal` / `Continuity` /
/// `VolumeConservation` / `Stress` がそうだった
///
/// 強さは `Proved` > `Witnessed` > `Modelled` の順だが、**前 2 つと `Modelled`
/// の間だけが質的な差** で、そこが [`Law::hard`] の gate になる
/// ([`Self::is_proof`])
///
/// 検証器 (本 crate) が構築し、利用側は読むだけ 根拠は今後増えるので
/// `#[non_exhaustive]`、match には `_` 腕を置く
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Evidence {
    /// 区間演算の包含による証明 — 標本の取り方に依存しない
    ///
    /// 「この箱の場は一様に正なので、どんな経路もここを通れない」のように、
    /// 有限個の点を見たのではなく **領域全体** について言えている
    Proved,
    /// 点の反例 — 中間値定理 / 点評価で実際に見つけた具体的な証拠
    ///
    /// 「この 2 点で場の符号が違うので、間に表面がある」のように、
    /// 示した点そのものが主張の裏付けになっている (偽陽性なし)
    Witnessed,
    /// モデルによる推定 — 証明でも反例でもない
    ///
    /// 格子解像度・無次元の閾値・場の値の距離への流用などに依存し、
    /// **解像度を変えると結論が変わりうる** 違反を主張してよいが、
    /// [`Priority::Hard`] は名乗れない
    Modelled {
        /// 何を仮定したモデルか (報告にそのまま出る)
        model: &'static str,
    },
}

impl Evidence {
    /// 証明または反例か (= [`Priority::Hard`] を名乗ってよいか)
    #[must_use]
    pub const fn is_proof(self) -> bool {
        !matches!(self, Self::Modelled { .. })
    }

    /// モデル推定ならその説明
    #[must_use]
    pub const fn model(self) -> Option<&'static str> {
        match self {
            Self::Modelled { model } => Some(model),
            _ => None,
        }
    }
}

/// [`Law::hard`] を、モデル推定しか返せない制約に対して呼んだ
///
/// 降格するかを検証器が黙って決めると「証明なしの Hard 違反」が別の形で復活
/// するので、呼び出し側に返して選ばせる ([`Law::soft`] で重みを付けるのが既定
/// の対処)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotProvable {
    /// 対象の制約 ([`Constraint::name`])
    pub constraint: &'static str,
    /// その制約が依存しているモデル
    pub model: &'static str,
}

impl std::fmt::Display for NotProvable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} は証明も反例も返せない (根拠: {}) ので Priority::Hard を名乗れない — Law::soft で重みを付けて使う",
            self.constraint, self.model
        )
    }
}

impl std::error::Error for NotProvable {}

/// 制約の種類
///
/// **variant ごとに主張の強さが違う** ([`Self::evidence_class`]):
///
/// - 区間演算の包含で証明する ([`Evidence::Proved`]): `Reachable`
/// - 点の反例を返す ([`Evidence::Witnessed`]): `NonOverlap` / `Containment` /
///   `MinThickness` / `Contact` / `GradientBound`
/// - モデル推定にとどまる ([`Evidence::Modelled`]): `Stress` / `Thermal` /
///   `Continuity` / `VolumeConservation` — [`Law::hard`] を通らない
///
/// 0.5.0 まで、この 4 つは幾何 proxy のまま [`Priority::Hard`] を名乗れた
/// (`LawSet` の convenience が `Hard` を hardcode していた) 物理 backend
/// (`alice-physics`) を繋いでも `Modelled` は `Modelled` のままで、
/// 「モデルである」ことと「モデルが良い」ことは別の話
#[derive(Debug, Clone)]
pub enum Constraint {
    /// 2 つの SDF が重ならない（distance > 0）
    NonOverlap {
        /// 対象 A
        a: SdfNode,
        /// 対象 B
        b: SdfNode,
    },
    /// inner が outer の内部に完全に収まる
    Containment {
        /// 内側のオブジェクト
        inner: SdfNode,
        /// 外側の境界
        outer: SdfNode,
    },
    /// SDF 形状の最小肉厚を保証
    MinThickness {
        /// 対象ノード
        node: SdfNode,
        /// 最小肉厚
        min_thickness: f32,
    },
    /// 荷重点近傍の応力集中を geometric proxy で検出
    ///
    /// 各 load point について、その近傍の内部セルで
    /// 肉厚 (|sdf|) が force × `min_thickness_factor` を下回れば violation
    /// 探索範囲は max(1.0, force) 半径 (heuristic)
    Stress {
        /// 対象ノード
        node: SdfNode,
        /// 荷重点リスト (位置, force 大きさ)
        load_points: Vec<(Vec3, f32)>,
        /// force に比例した必要肉厚係数 (例: 0.2 なら force=5 で肉厚 1.0 要求)
        min_thickness_factor: f32,
    },
    /// 熱源近傍の放熱面積比を geometric proxy で検出
    ///
    /// 各 heat source について、半径 `search_radius` 以内で
    /// 表面近傍セル (|sdf| < step) 数 / 内部セル (sdf < 0) 数 の ratio を計算
    /// ratio が `min_surface_ratio` を下回れば violation (放熱面積不足)
    Thermal {
        /// 対象ノード
        node: SdfNode,
        /// 熱源点リスト
        heat_sources: Vec<Vec3>,
        /// 探索半径
        search_radius: f32,
        /// 表面積 / 体積 比の下限
        min_surface_ratio: f32,
    },
    /// 2 面の接触可能距離範囲を検証 (assembly / mating check)
    ///
    /// A と B の最小表面間距離が \[`min_distance`, `max_distance`\] の範囲に収まれば pass
    /// interfering (両 sdf < 0 の cell あり) or 距離が範囲外なら violation
    Contact {
        /// 対象 A
        a: SdfNode,
        /// 対象 B
        b: SdfNode,
        /// 接触可能とみなす最小距離
        min_distance: f32,
        /// 接触可能とみなす最大距離
        max_distance: f32,
    },
    /// SDF が単一連結領域であることを検証
    ///
    /// `seed_point` (内部、sdf < 0) から 6-connected flood fill で到達可能な内部セル数と
    /// 全内部セル数を比較 到達できない内部セルがあれば violation (disjoint region 存在)
    Continuity {
        /// 対象ノード
        node: SdfNode,
        /// flood fill の起点 (内部点、sdf(seed) < 0 でなければ invalid)
        seed_point: Vec3,
    },
    /// 領域上で場の勾配が上界を超えないことを検証
    ///
    /// 距離場の勾配が 1 を超えるとき、場は真の距離より大きい値を申告する
    /// (`|f(p)| ≤ L · dist(p)`)。sphere tracing が `f(p)` だけ進むと面を
    /// 踏み越えるので、薄い形状が貫通する。逆に 1 を下回るのは緩みで、
    /// 正しさは保たれたまま step 数だけが増える。
    ///
    /// 三値の出方が他の法則と違う: 合格は**静的上界による証明**
    /// (`eval_lipschitz(node) ≤ max_gradient`)、違反は**証拠の 2 点**
    /// (差分商が上界を超える標本対)、そのどちらも出なければ未定
    /// (標本は「無いこと」を証明できない)。
    GradientBound {
        /// 対象ノード
        node: SdfNode,
        /// 許容する最大勾配 (1.0 = 距離を過大申告しない)
        max_gradient: f32,
        /// 差分商を取る 2 点の間隔
        probe: f32,
    },
    /// `from` から `to` へ内部領域を通って到達できることを検証
    ///
    /// [`Continuity`](Self::Continuity) が「内部が 1 つながりか」を見るのに
    /// 対し、こちらは「この 2 点が繋がっているか」を見る。詰み (行きたい
    /// 場所へ到達できない状態) の検出はこちらでしかできない。
    ///
    /// 三値はすべて証明になっている: 区間演算で**内部と確定した**セルだけを
    /// 辿って到達できれば合格、**外部と確定した**セルが 2 点を隔てていれば
    /// 違反 (どんな経路も外部を通らざるを得ない)、判定できないセルを通らな
    /// ければ繋がらない場合は未定。
    Reachable {
        /// 対象ノード
        node: SdfNode,
        /// 出発点 (内部)
        from: Vec3,
        /// 目的点 (内部)
        to: Vec3,
    },
    /// morph 前後の体積保存を検証
    ///
    /// grid 上で before / after 各 SDF の内部セル数を count
    /// 相対差 |`V_before` - `V_after`| / `max(V_before`, 1) が `relative_tolerance` 超過で violation
    VolumeConservation {
        /// 変形前 SDF
        before: SdfNode,
        /// 変形後 SDF
        after: SdfNode,
        /// 相対許容誤差 (例: 0.05 = 5% 以内なら pass)
        relative_tolerance: f32,
    },
    /// 熱源を与えた時の **実温度場** の最高温度が上限を超えないことを検証
    ///
    /// [`Thermal`](Self::Thermal) が「放熱面積比」という幾何 proxy なのに対し、
    /// こちらは `alice_physics` の熱伝導 solver
    /// ([`transient_step_3d`](alice_physics::transient_thermal::transient_step_3d))
    /// で温度場そのものを解く。
    ///
    /// # 境界条件を 1 つに決めず、上下から挟む
    ///
    /// 表面からどれだけ熱が逃げるか (対流熱伝達率 `h`) は形状と設置環境で
    /// 決まり、設計時には分からない。`h` を 1 つ決め打ちすると、その値が
    /// 外れた分だけ判定が嘘になる。最高温度は `h` に対して**単調に減少**
    /// するので、両端で解いて挟む:
    ///
    /// - `h = 0` (断熱、熱が一切逃げない) → 最高温度の **上界**
    /// - `h = ∞` (表面が周囲温度に固定) → 最高温度の **下界**
    ///
    /// 下界 > 上限なら違反 (どれだけ冷やしても超える)、上界 ≤ 上限なら合格
    /// (一切冷えなくても収まる)、挟んだら未定
    /// ([`UnresolvedReason::TemperatureUnbracketed`])。
    ///
    /// # 形状も同じく挟む
    ///
    /// 区間演算で内外が決まらないセルがあるので、真の材料 `M` は
    /// `Inside ⊆ M ⊆ Inside ∪ Undecided` の範囲にある。材料を増やした時に
    /// 最高温度が動く向きは **境界条件によって逆**で、断熱は下がり
    /// (質量が増えて同じ熱量を吸う)、等温は上がる (周囲温度に固定される面が
    /// 熱源から遠のく)。したがって**両端とも最小材料 = `Inside` で極値を取る**
    /// ので、どちらの run も `Inside` だけで解く。
    ///
    /// `Inside` に絞ると材料が消える / 熱源が乗らない / 格子の都合で島が
    /// 千切れる形状が出る。いずれも黙って通さず
    /// [`UnresolvedReason::ThermalGridUnusable`] にする。
    ///
    /// # 単位
    ///
    /// `alice_physics` の材料定数は SI (`W/(m·K)` / `J/(kg·K)` / `kg/m³`) なので、
    /// SDF の座標が何 m かを [`metres_per_unit`](Self::ThermalField::metres_per_unit)
    /// で明示する。mm で設計した形状にそのまま SI を当てると拡散率の効き方が
    /// 10⁶ ずれる。
    ///
    /// `physics` feature が要る (`alice-physics` は **AGPL-3.0-or-later** なので
    /// 有効化すると下流にも伝播する)。
    #[cfg(feature = "physics")]
    ThermalField {
        /// 対象ノード
        node: SdfNode,
        /// 熱源リスト (位置 \[SDF 単位\], 発熱 \[W\])
        sources: Vec<(Vec3, f32)>,
        /// 材料 (`ThermalMaterial::pla_polymer()` 等)
        material: alice_physics::transient_thermal::ThermalMaterial,
        /// SDF の 1 単位が何 m か (mm 設計なら 0.001)
        metres_per_unit: f32,
        /// 周囲温度 \[°C\]
        ambient_c: f32,
        /// 許容する最高温度 \[°C\]
        max_temperature_c: f32,
        /// 何秒後の温度場を見るか \[s\]
        duration_s: f32,
    },
}

impl Constraint {
    /// variant 名 (報告 / エラー用)
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::NonOverlap { .. } => "NonOverlap",
            Self::Containment { .. } => "Containment",
            Self::MinThickness { .. } => "MinThickness",
            Self::Stress { .. } => "Stress",
            Self::Thermal { .. } => "Thermal",
            Self::Contact { .. } => "Contact",
            Self::Continuity { .. } => "Continuity",
            Self::GradientBound { .. } => "GradientBound",
            Self::Reachable { .. } => "Reachable",
            Self::VolumeConservation { .. } => "VolumeConservation",
            #[cfg(feature = "physics")]
            Self::ThermalField { .. } => "ThermalField",
        }
    }

    /// この制約が返せる **最も強い** 根拠
    ///
    /// [`Law::hard`] の gate に使う 個々の違反が実際に何を根拠にしたかは
    /// [`Violation::evidence`] 側に載る (例: `Contact` は干渉なら
    /// [`Evidence::Witnessed`]、離れすぎの証明なら [`Evidence::Proved`] を
    /// 返すが、ここでは弱い方の `Witnessed` を class として申告する)
    #[must_use]
    pub const fn evidence_class(&self) -> Evidence {
        match self {
            // 区間演算で内部確定セルの経路 / 外部確定セルの隔離を示す
            Self::Reachable { .. } => Evidence::Proved,
            // 反例の点そのものが証拠になる
            Self::NonOverlap { .. }
            | Self::Containment { .. }
            | Self::MinThickness { .. }
            | Self::Contact { .. }
            | Self::GradientBound { .. } => Evidence::Witnessed,
            // 以下 4 つは推定にとどまる (Hard を名乗れない)
            Self::Stress { .. } => Evidence::Modelled {
                model: MODEL_STRESS,
            },
            Self::Thermal { .. } => Evidence::Modelled {
                model: MODEL_THERMAL,
            },
            Self::Continuity { .. } => Evidence::Modelled {
                model: MODEL_CONTINUITY,
            },
            Self::VolumeConservation { .. } => Evidence::Modelled {
                model: MODEL_VOLUME,
            },
            // 実温度場だが数値解 + 格子離散化 + 境界条件の両端しか挟まない
            #[cfg(feature = "physics")]
            Self::ThermalField { .. } => Evidence::Modelled {
                model: MODEL_THERMAL_FIELD,
            },
        }
    }
}

/// `Stress` が仮定しているモデル
///
/// 表面までの距離そのものは [`probe_ball`] で健全に測るが、**要求値**
/// `force × min_thickness_factor` が無次元の heuristic で、探索半径
/// `max(1.0, force)` も同様
const MODEL_STRESS: &str =
    "肉厚 ≥ force × min_thickness_factor の幾何 proxy (材料 / 断面係数 / 降伏応力を持たない)";

/// `Thermal` が仮定しているモデル
///
/// 放熱面積比という proxy に加え、表面近傍の判定が `|f(p)| < step` = **場の値を
/// 表面までの距離として流用** している (TPMS では場が真の距離の 1.7〜7.0 倍)
const MODEL_THERMAL: &str =
    "熱源近傍の 表面近傍セル数 / 内部セル数 比 (|f| を距離として流用、格子解像度依存)";

/// `Continuity` が仮定しているモデル
const MODEL_CONTINUITY: &str =
    "格子セル中心の点標本 + 6-connected flood fill (格子より細い接続 / 分離を取りこぼす)";

/// `ThermalField` が仮定しているモデル
///
/// 幾何 proxy ではなく実温度場だが、**証明ではない**: 陽解法の数値解 /
/// 格子離散化 / 挟んでいるのは対流熱伝達率と形状の 2 軸だけ
///
/// 形状は `Inside ⊆ 真の材料 ⊆ Inside ∪ Undecided` の範囲にあり、両端とも
/// 最小材料 (`Inside`) で極値を取る (材料を増やすと断熱は下がり等温は上がる)
///
/// `ThermalField` は `physics` feature 限定なので、この定数もそれに合わせる
/// (gate を忘れると既定 build で dead code になる CI の clippy は
/// `--all-features` で回るので、既定 build だけの警告は捕まらない)
#[cfg(feature = "physics")]
const MODEL_THERMAL_FIELD: &str = "alice_physics の陽解法 3D 熱伝導 (格子離散化、\
境界条件 h と形状を両端で挟む: 上界 = 断熱 × 内部確定セルのみ、下界 = 等温 × 同)";

/// `VolumeConservation` が仮定しているモデル
const MODEL_VOLUME: &str =
    "格子セル中心の点標本による内部セル数 count (格子より細い体積差を取りこぼす)";

/// 法則の定義
#[derive(Debug, Clone)]
pub struct Law {
    /// 法則名
    pub name: String,
    /// 優先度
    pub priority: Priority,
    /// 制約の内容
    pub constraint: Constraint,
}

impl Law {
    /// ハード制約の法則を作成 — **証明か反例を返せる制約に限る**
    ///
    /// # Errors
    ///
    /// モデル推定しか返せない制約 (`Stress` / `Thermal` / `Continuity` /
    /// `VolumeConservation`) には [`NotProvable`] を返す 黙って
    /// [`Priority::Soft`] に降格すると「証明なしの Hard 違反」が別の形で
    /// 復活するので、降格は呼び出し側が明示的に選ぶ
    ///
    /// ```
    /// use alice_lol::law::{Constraint, Law};
    /// use alice_sdf::SdfNode;
    /// use glam::Vec3;
    ///
    /// let node = SdfNode::sphere(1.0);
    /// // 反例を返せるので Hard を名乗れる
    /// let c = Constraint::MinThickness { node: node.clone(), min_thickness: 0.5 };
    /// assert!(Law::hard("thick", c).is_ok());
    ///
    /// // 格子解像度依存の推定なので名乗れない
    /// let c = Constraint::Continuity { node, seed_point: Vec3::ZERO };
    /// assert_eq!(Law::hard("split", c).unwrap_err().constraint, "Continuity");
    /// ```
    pub fn hard(name: impl Into<String>, constraint: Constraint) -> Result<Self, NotProvable> {
        if let Some(model) = constraint.evidence_class().model() {
            return Err(NotProvable {
                constraint: constraint.name(),
                model,
            });
        }
        Ok(Self::hard_unchecked(name, constraint))
    }

    /// gate を通さず Hard を作る (呼び出し側が provable を保証する内部用)
    fn hard_unchecked(name: impl Into<String>, constraint: Constraint) -> Self {
        debug_assert!(
            constraint.evidence_class().is_proof(),
            "hard_unchecked にモデル推定の制約 {} が渡された",
            constraint.name()
        );
        Self {
            name: name.into(),
            priority: Priority::Hard,
            constraint,
        }
    }

    /// ソフト制約の法則を作成（weight: 0.0〜1.0）— 任意の制約に使える
    #[must_use]
    pub fn soft(name: impl Into<String>, weight: f32, constraint: Constraint) -> Self {
        Self {
            name: name.into(),
            priority: Priority::Soft(weight),
            constraint,
        }
    }
}

/// 違反レポート
#[derive(Debug, Clone)]
pub struct Violation {
    /// 違反した法則名
    pub law_name: String,
    /// 優先度
    pub priority: Priority,
    /// 残差（違反の大きさ、負の値 = 侵入深さ）
    pub residual: f32,
    /// 違反が検出された点
    pub point: Vec3,
    /// 違反点を含むセルの AABB
    pub region: Vec3Interval,
    /// **この違反の** 裏付け
    ///
    /// [`Constraint::evidence_class`] が「その制約が返せる最強の根拠」なのに
    /// 対し、こちらは実際にこの 1 件が何に依っているか 同じ `Contact` でも
    /// 干渉は [`Evidence::Witnessed`]、離れすぎは [`Evidence::Proved`] になる
    pub evidence: Evidence,
}

/// 判定不能の理由
///
/// 検証器 (本 crate) が構築し、利用側は読むだけ 理由は今後増えるので
/// `#[non_exhaustive]`、match には `_` 腕を置く
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum UnresolvedReason {
    /// 半径 `radius` の球内に表面があるか、区間演算でも点探索でも決まらなかった
    SurfaceProximity {
        /// 探索半径
        radius: f32,
    },
    /// セル内で 2 つの場の符号の組合せが区間演算で確定しなかった
    SignUndecided,
    /// 表面間距離を上下から挟めなかった (`upper` = 見つかった上界、無ければ +∞)
    GapUnbracketed {
        /// 表面間距離の上界
        upper: f32,
    },
    /// 勾配の上界超過を示す標本対が見つからなかったが、静的上界も
    /// 上界以下を保証しない (標本は「無いこと」を証明できない)
    GradientUnwitnessed {
        /// 静的上界が主張する値 (保証できなかった側)
        claimed: f32,
        /// 標本で見つかった最大の差分商
        worst_sampled: f32,
    },
    /// 内部と確定したセルだけでは到達できず、外部と確定したセルだけでは
    /// 隔てられてもいない (判定できないセルを経由すれば繋がる)
    ReachabilityUndecided {
        /// 判定できなかったセルを経由した経路の長さ (セル数)
        undecided_cells: usize,
    },
    /// 表面比が下限を挟んでしまい、上回るとも下回るとも決まらなかった
    ///
    /// 区間演算で内外が確定しないセルは「表面を含みうる」かつ「内部かもしれ
    /// ない」の両方に効くので、比は 1 点でなく幅を持つ
    SurfaceRatioUnbracketed {
        /// 表面比の下界
        lo: f32,
        /// 表面比の上界
        hi: f32,
    },
    /// 最高温度が上限を挟んでしまい、超えるとも収まるとも決まらなかった
    ///
    /// 対流熱伝達率 `h` は設計時に決まらないので両端で挟む — `h = ∞`
    /// (表面が周囲温度、最大冷却) が下界、`h = 0` (断熱) が上界
    TemperatureUnbracketed {
        /// 最高温度の下界 \[°C\] (h = ∞)
        lo_c: f32,
        /// 最高温度の上界 \[°C\] (h = 0)
        hi_c: f32,
    },
    /// 熱伝導 solver が要求する格子の前提を満たさず、解けなかった
    ///
    /// `transient_step_3d` は 1 つの `dx` しか取らない (= 立方セル必須) / 各軸
    /// 3 セル以上必要 / 陽解法の CFL 条件で刻みが決まるので、評価時間に対して
    /// step 数が多すぎると解ききれない
    ThermalGridUnusable {
        /// 何が足りなかったか
        why: &'static str,
    },
    /// 体積の相対差が許容値を挟んでしまい、超えるとも収まるとも決まらなかった
    VolumeUnbracketed {
        /// 相対差の下界
        lo: f32,
        /// 相対差の上界
        hi: f32,
    },
}

/// 判定不能レポート — 違反でも合格でもない (検証器の解像度 / 区間演算の
/// 包含精度が足りなかった) 標本点 合格扱いにしてはいけない
#[derive(Debug, Clone)]
pub struct Unresolved {
    /// 法則名
    pub law_name: String,
    /// 優先度
    pub priority: Priority,
    /// 判定できなかった標本点
    pub point: Vec3,
    /// 判定できなかった領域 (球探索なら未決定の葉、セル判定ならセル)
    pub region: Vec3Interval,
    /// 理由
    pub reason: UnresolvedReason,
}

/// [`Priority::Hard`] 制約の 3 値判定 ([`LawReport::hard_verdict`] の結果)
///
/// doctrine「不確かさが嘘をつけない」の実体化 — **`Undecided` は合格ではない**
/// 合否 1 bit だけが欲しい caller は [`Self::is_proven`] を使う
/// ([`LawReport::has_hard_violations`] を `!` で否定すると、判定できなかった
/// 標本が合格側へ倒れる)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardVerdict {
    /// Hard 制約に違反も判定不能も無い (証明付きの合格)
    Proven,
    /// Hard 制約に証明か反例のある違反がある
    Violated,
    /// Hard 制約に違反は無いが、判定できなかった標本がある
    ///
    /// 検証器の解像度 / 区間演算の包含精度が足りなかった状態
    /// **合格扱いにしてはいけない**
    Undecided,
}

impl HardVerdict {
    /// 証明付きで合格したか (`Proven` だけが `true`)
    #[must_use]
    pub const fn is_proven(self) -> bool {
        matches!(self, Self::Proven)
    }
}

/// 法則検証の結果
#[derive(Debug, Clone)]
pub struct LawReport {
    /// 全法則数
    pub total_laws: usize,
    /// パスした法則数 (違反も未決定もない法則)
    pub passed: usize,
    /// 違反リスト（残差の絶対値の大きい順）
    pub violations: Vec<Violation>,
    /// 判定不能リスト (法則ごとに最初の 1 件)
    pub unresolved: Vec<Unresolved>,
}

impl LawReport {
    /// 全法則が **証明付きで** パスしたか (違反なし かつ 判定不能なし)
    #[must_use]
    pub const fn all_passed(&self) -> bool {
        self.violations.is_empty() && self.unresolved.is_empty()
    }

    /// ハード制約の違反があるか
    ///
    /// 0.5.0 以降、`true` は **証明か反例のある違反** だけを意味する
    /// ([`Law::hard`] がモデル推定の制約を弾くため)
    #[must_use]
    pub fn has_hard_violations(&self) -> bool {
        self.violations.iter().any(|v| v.priority == Priority::Hard)
    }

    /// モデル推定でなく証明 / 反例に裏付けられた違反だけを返す
    ///
    /// [`Priority`] と直交する軸 — Soft で足したモデル推定の法則と、
    /// Soft で足した反例つきの法則を、報告側で分けたい時に使う
    #[must_use]
    pub fn proven_violations(&self) -> Vec<&Violation> {
        self.violations
            .iter()
            .filter(|v| v.evidence.is_proof())
            .collect()
    }

    /// 判定不能の法則があるか
    #[must_use]
    pub const fn has_unresolved(&self) -> bool {
        !self.unresolved.is_empty()
    }

    /// [`Priority::Hard`] 制約の 3 値判定 — **判定不能を合格にしない**
    ///
    /// gate (「先へ進んでよいか」) にはこちらを使う
    /// [`Self::has_hard_violations`] は `violations` だけを見るので、
    /// `!has_hard_violations()` を合格の判定に使うと **Hard が判定不能の時に
    /// 合格へ倒れる**
    ///
    /// [`Self::all_passed`] との違いは対象範囲 — `all_passed` は
    /// [`Priority::Soft`] の判定不能でも `false` になるので、Hard 制約だけの
    /// 主張には過剰
    ///
    /// 違反と判定不能が同時にある時は [`HardVerdict::Violated`]
    /// (反例のある側が証拠として強いので、そちらを捨てない)
    #[must_use]
    pub fn hard_verdict(&self) -> HardVerdict {
        if self.has_hard_violations() {
            HardVerdict::Violated
        } else if self.unresolved.iter().any(|u| u.priority == Priority::Hard) {
            HardVerdict::Undecided
        } else {
            HardVerdict::Proven
        }
    }
}

/// 1 法則の判定結果 (内部)
struct Verdict {
    violation: Option<Violation>,
    unresolved: Option<Unresolved>,
}

/// 球探索 / セル細分の八分木深さ上限 (葉の辺 = 元の辺 / 2^depth)
pub const BALL_PROBE_DEPTH: u32 = 4;

/// 交点の二分探索反復回数 (距離の上界を `|p − q| / 2^n` まで締める)
const BISECT_ITERS: u32 = 12;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 検証設定
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 法則検証の設定
#[derive(Debug, Clone)]
pub struct CheckConfig {
    /// 検査範囲の AABB 最小点
    pub aabb_min: Vec3,
    /// 検査範囲の AABB 最大点
    pub aabb_max: Vec3,
    /// グリッド解像度（各軸のサンプル点数）
    pub resolution: usize,
}

impl Default for CheckConfig {
    fn default() -> Self {
        Self {
            aabb_min: Vec3::splat(-5.0),
            aabb_max: Vec3::splat(5.0),
            resolution: 8,
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 検証エンジン
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// SDF を点で評価するヘルパー
fn sdf_eval(node: &SdfNode, point: Vec3) -> f32 {
    alice_sdf::eval(node, point)
}

/// 区間演算で箱の場を包含評価するヘルパー
fn sdf_interval(node: &SdfNode, bounds: Vec3Interval) -> Interval {
    alice_sdf::interval::eval_interval(node, bounds)
}

/// 中心 `c` 半幅 `h` の立方体
fn cube(c: Vec3, h: f32) -> Vec3Interval {
    Vec3Interval {
        x: Interval::new(c.x - h, c.x + h),
        y: Interval::new(c.y - h, c.y + h),
        z: Interval::new(c.z - h, c.z + h),
    }
}

/// 箱の中心
const fn box_center(b: Vec3Interval) -> Vec3 {
    Vec3::new(
        b.x.lo.midpoint(b.x.hi),
        b.y.lo.midpoint(b.y.hi),
        b.z.lo.midpoint(b.z.hi),
    )
}

/// 箱の各軸を `amount` だけ広げる
fn box_expand(b: Vec3Interval, amount: f32) -> Vec3Interval {
    Vec3Interval {
        x: b.x.expand(amount),
        y: b.y.expand(amount),
        z: b.z.expand(amount),
    }
}

/// 箱の中で `p` に最も近い点
const fn box_nearest(b: Vec3Interval, p: Vec3) -> Vec3 {
    Vec3::new(
        p.x.clamp(b.x.lo, b.x.hi),
        p.y.clamp(b.y.lo, b.y.hi),
        p.z.clamp(b.z.lo, b.z.hi),
    )
}

/// 箱を 8 分割
fn box_children(b: Vec3Interval) -> [Vec3Interval; 8] {
    let c = box_center(b);
    let split = |iv: Interval, m: f32, hi: bool| {
        if hi {
            Interval::new(m, iv.hi)
        } else {
            Interval::new(iv.lo, m)
        }
    };
    let mut out = [b; 8];
    for (i, child) in out.iter_mut().enumerate() {
        *child = Vec3Interval {
            x: split(b.x, c.x, i & 1 != 0),
            y: split(b.y, c.y, i & 2 != 0),
            z: split(b.z, c.z, i & 4 != 0),
        };
    }
    out
}

/// 区間の符号: `Some(true)` = 全て非負 (外側 or 表面)、`Some(false)` = 全て負 (内側)、
/// `None` = 混在 (表面を含みうる)
fn interval_sign(iv: Interval) -> Option<bool> {
    if iv.lo >= 0.0 {
        Some(true)
    } else if iv.hi < 0.0 {
        Some(false)
    } else {
        None
    }
}

/// `p` (`f(p)` の符号 `outside_p`) と `q` (反対符号) の線分上の交点を二分探索し、
/// `p` から交点までの距離の上界を返す
fn bisect_crossing(node: &SdfNode, p: Vec3, outside_p: bool, mut q: Vec3) -> f32 {
    let mut a = p;
    for _ in 0..BISECT_ITERS {
        let m = (a + q) * 0.5;
        if (sdf_eval(node, m) >= 0.0) == outside_p {
            a = m;
        } else {
            q = m;
        }
    }
    // 交点は [a, q] の間 → 上界は q
    p.distance(q)
}

/// 球探索の結果
///
/// `Debug` は判定器の核の unit test (`core_probe_tests`) が「期待した variant で
/// なかった時に何が返ったか」を出すために必要 (private enum なので API 影響なし)
#[derive(Debug)]
enum BallProbe {
    /// 球内は全て `p` と同符号 = 表面まで ≥ r
    Clear,
    /// 反対符号の点を発見 = 表面まで ≤ `distance` (二分探索で締めた上界)
    Crossing { distance: f32 },
    /// 深さ上限まで細分しても未決定の葉が残った
    Undecided { region: Vec3Interval },
}

/// 点 `p` を中心とする半径 `r` の球に `f(p)` と反対符号の点 (= 表面) があるか
///
/// 八分木で細分し、`p` に近い箱から訪問、見つかった交点より遠い箱は刈る
/// 深さ上限で未決定の葉は中心を点評価 (反対符号なら証拠) してから Undecided
fn probe_ball(node: &SdfNode, centre: Vec3, radius: f32) -> BallProbe {
    let f_centre = sdf_eval(node, centre);
    if f_centre == 0.0 {
        return BallProbe::Crossing { distance: 0.0 };
    }
    let outside = f_centre > 0.0;

    let mut best: Option<f32> = None;
    let mut undecided: Option<Vec3Interval> = None;
    // Lipschitz 定数は node ごとに定まるので八分木を降りる前に 1 度だけ求める
    let l = eval_lipschitz(node);
    // (箱, 深さ) の明示 stack — centre に近い順に処理するため子は遠い順に push
    let mut stack: Vec<(Vec3Interval, u32)> = vec![(cube(centre, radius), 0)];

    while let Some((bx, depth)) = stack.pop() {
        let near = box_nearest(bx, centre);
        let near_dist = centre.distance(near);
        if near_dist > radius {
            continue; // 球と交わらない
        }
        if best.is_some_and(|d| near_dist >= d) {
            continue; // これより近い交点は出ない
        }

        // 区間で符号が決まらない箱は Lipschitz 包囲で締め直してもう一度見る
        // (第 2 の証明経路、[`refine`]) ⚠️ 包囲が矛盾した箱では符号を主張しない
        let iv = sdf_interval(node, bx);
        let sign = interval_sign(iv).or_else(|| match refine(node, bx, iv, l) {
            Refined::Enclosure(tight) => interval_sign(tight),
            Refined::Contradiction => None,
        });
        match sign {
            // 一様に同符号: 表面なし
            Some(sign) if sign == outside => {}
            // 一様に反対符号: 箱の最近点が反対符号 → 交点は centre–near 線分上
            Some(_) => {
                let d = bisect_crossing(node, centre, outside, near);
                if best.is_none_or(|bd| d < bd) {
                    best = Some(d);
                }
            }
            None if depth >= BALL_PROBE_DEPTH => {
                let leaf_centre = box_center(bx);
                if centre.distance(leaf_centre) <= radius
                    && (sdf_eval(node, leaf_centre) >= 0.0) != outside
                {
                    let d = bisect_crossing(node, centre, outside, leaf_centre);
                    if best.is_none_or(|bd| d < bd) {
                        best = Some(d);
                    }
                } else if undecided.is_none() {
                    undecided = Some(bx);
                }
            }
            None => {
                let mut children = box_children(bx);
                children.sort_by(|x, y| {
                    let dx = centre.distance(box_nearest(*x, centre));
                    let dy = centre.distance(box_nearest(*y, centre));
                    dy.partial_cmp(&dx).unwrap_or(std::cmp::Ordering::Equal)
                });
                for child in children {
                    stack.push((child, depth + 1));
                }
            }
        }
    }

    match (best, undecided) {
        (Some(distance), _) => BallProbe::Crossing { distance },
        (None, Some(region)) => BallProbe::Undecided { region },
        (None, None) => BallProbe::Clear,
    }
}

/// `p` から表面までの距離の上界を求める
///
/// [`probe_ball`] は「半径 r の球の中に表面があるか」を返すので、交点が出る
/// まで半径を倍にしていく 場の値 `|f(p)|` を初期半径に使うが、**これは
/// 当たりを付けるためだけ** で、返す値は二分探索で締めた実距離の上界
/// (場が距離を過大申告する node でも過小申告する node でも正しい)
///
/// 上限まで探して見つからなければ `None`
///
/// 上限は `floor` (検査 AABB の対角) と **場の値の定数倍** の大きい方
/// 検査範囲を 1 セルに絞った使い方だと AABB の対角が表面まで届かないので、
/// 場の値を距離のスケールの当たりとして併用する (場が距離を数倍ずらしても
/// 届く倍率を取る)
fn surface_distance(node: &SdfNode, p: Vec3, floor: f32) -> Option<f32> {
    let field = sdf_eval(node, p).abs();
    let cap = floor.max(field * SURFACE_SEARCH_SLACK).max(f32::EPSILON);
    let mut r = field.max(cap * 1.0e-3);
    // 回数で切る (`cap` までの倍増は高々 log2(SURFACE_SEARCH_SLACK) + 余裕、
    // 浮動小数を while の条件にすると NaN で止まらなくなる)
    for _ in 0..SURFACE_SEARCH_STEPS {
        if let BallProbe::Crossing { distance } = probe_ball(node, p, r.min(cap)) {
            return Some(distance);
        }
        if r >= cap {
            break;
        }
        r *= 2.0;
    }
    None
}

/// 距離探索の倍増回数の上限
const SURFACE_SEARCH_STEPS: u32 = 10;

/// 場の値から距離探索の上限を作る時の倍率
///
/// 場が真の距離を過小申告する合成 (union の内部 / intersection の外部) でも
/// 届くだけの余裕 実測で必要なのは 2 倍程度 (union 内部 0.5 → 0.866、
/// 薄いレンズ外部 1.193 → 1.564)
const SURFACE_SEARCH_SLACK: f32 = 16.0;

/// 検査 AABB の対角 (距離探索の下限側の上限)
fn search_cap(config: &CheckConfig) -> f32 {
    (config.aabb_max - config.aabb_min).length()
}

/// 「表面まで ≥ `required`」を標本点 `center` で判定し、違反 / 未決定を集約する
///
/// `MinThickness` と `Stress` の共通部分 `worst` は最も負の residual、
/// `undecided` は最初の未決定
fn accumulate_thickness(
    node: &SdfNode,
    center: Vec3,
    bounds: Vec3Interval,
    required: f32,
    worst: &mut Option<(f32, Vec3, Vec3Interval)>,
    undecided: &mut Option<(Vec3, Vec3Interval, f32)>,
) {
    match probe_ball(node, center, required) {
        BallProbe::Clear => {}
        BallProbe::Crossing { distance } => {
            let residual = distance - required; // ≤ 0 = 不足量 (上界側)
            match worst {
                Some((w, _, _)) if residual >= *w => {}
                _ => *worst = Some((residual, center, bounds)),
            }
        }
        BallProbe::Undecided { region } => {
            if undecided.is_none() {
                *undecided = Some((center, region, required));
            }
        }
    }
}

fn thickness_verdict(
    law_name: &str,
    priority: Priority,
    evidence: Evidence,
    worst: Option<(f32, Vec3, Vec3Interval)>,
    undecided: Option<(Vec3, Vec3Interval, f32)>,
) -> Verdict {
    Verdict {
        violation: worst.map(|(residual, point, region)| Violation {
            law_name: law_name.to_string(),
            priority,
            residual,
            point,
            region,
            evidence,
        }),
        unresolved: undecided.map(|(point, region, radius)| Unresolved {
            law_name: law_name.to_string(),
            priority,
            point,
            region,
            reason: UnresolvedReason::SurfaceProximity { radius },
        }),
    }
}

/// 2 つの場の符号条件をセル内で判定する (八分木細分付き)
///
/// `decided_ok(ia, ib)`: 箱全体で条件が成立し得ないことの証明
/// `witness(fa, fb)`: 点評価で条件が成立するか (箱全体で成立する場合も
/// 中心点が witness になるので、区間での「certain」判定は不要)
/// `residual(fa, fb)`: witness の残差 (負、小さいほど悪い) — 最初の witness で
/// 打ち切らず、未決定の箱を全て掘って **最悪** の witness を返す
enum PairProbe {
    Clear,
    Witness { point: Vec3, residual: f32 },
    Undecided { region: Vec3Interval },
}

/// **Lipschitz 包囲** — 区間演算と並列の第 2 の証明経路 (2026-09-30、doctrine §5)
///
/// 真の距離場は `L`-Lipschitz なので、箱の中心 `c` とその半対角 `ρ` に対して
/// `f` は `[f(c) − L·ρ, f(c) + L·ρ]` に収まる これは区間演算と**独立に健全な
/// 包囲**なので、両者の共通部分を取ればより締まる
///
/// # なぜ必要か (実測、2026-09-30)
///
/// ⚠️ 区間演算は**健全だが依存性問題でセル幅の 300 倍に膨らむ** 真の核 5 件
/// (`heart` / `cut_sphere` / `link` / `capped_torus` / `death_star`) では
/// **区間 `[-5.5, +6.53]` に対し真の範囲が `[+1.58, +1.62]`** で、
/// 解像度をいくら上げても区間幅は縮まらない (= 未決定が「解像度に無反応」の正体)
///
/// 一方 Lipschitz 側は `|f(c)| = 1.60` / `L = 1.0` / `L·ρ = 0.027` で
/// **59 倍の余裕で符号が確定する** ⇒ **「決定不能」ではなく「区間が緩い」** だった
///
/// # 健全性の前提 — ⚠️ **外部 (`f ≥ 0`) 限定**
///
/// [`alice_sdf::interval::eval_lipschitz`] の契約は
/// **`f(p) ≥ 0` または `f(q) ≥ 0` である対に限って** `|f(p) − f(q)| ≤ L·|p − q|`
/// (場は実体の**内部で不連続でもよい** — IQ の楕円体は中心で跳ぶ)
///
/// ⇒ **箱の中心の場が `f(c) ≥ 0` の時だけ包囲を主張できる** 中心を対の片端に
/// 取れば契約が満たされるので、箱の任意の点 `q` に対して
/// `|f(q) − f(c)| ≤ L·|q − c| ≤ L·ρ` が言える
/// ⚠️ **`f(c) < 0` では契約が何も言わない** (内部同士の対は無制約) ので `None`
///
/// この gate を省くと **実体の深い内部で偽の包囲**を作る 実測 (2026-09-30):
/// `tests/analytic_law.rs::undecidable_node_is_reported_not_passed` が
/// `InfiniteCone` の内部点 `(0, −10, 0)` (区間は `Interval::EVERYTHING` = 包含を
/// 諦める node) を**決着させてしまい red になった** = oracle が gate 漏れを捉えた
///
/// `L` が非有限 (上界を主張できない形) の時も `None` を返して使わない
///
/// 誤った `proven` は総当たり反証器
/// (`tests/law_corpus_oracle.rs::judge_never_proves_a_pass_the_brute_force_refutes`)
/// が独立に検出する前提で運用する 加えて [`refine_with_lipschitz`] が
/// **2 つの包囲の矛盾**を機械的に捕らえる (下記)
fn lipschitz_enclosure(bx: Vec3Interval, f_centre: f32, l: f32) -> Option<Interval> {
    // ⚠️ 外部限定の契約 — 内部 (`f(c) < 0`) では何も主張できない
    if !l.is_finite() || l <= 0.0 || !f_centre.is_finite() || f_centre < 0.0 {
        return None;
    }
    // 半対角 ρ
    let (dx, dy, dz) = (bx.x.hi - bx.x.lo, bx.y.hi - bx.y.lo, bx.z.hi - bx.z.lo);
    let rho = 0.5 * dx.mul_add(dx, dy.mul_add(dy, dz * dz)).sqrt();
    if !rho.is_finite() {
        return None;
    }
    let slack = l * rho;
    Some(Interval::new(f_centre - slack, f_centre + slack))
}

/// [`refine_with_lipschitz`] の結果
#[derive(Debug, Clone, Copy)]
enum Refined {
    /// 使える範囲で最も締まった包囲 (Lipschitz を主張できない形では区間そのまま)
    Enclosure(Interval),
    /// ⚠️ **2 つの包囲が矛盾した** = どちらかが健全でない この cell は何も証明しない
    Contradiction,
}

/// 区間包囲を Lipschitz 包囲との共通部分で締める
///
/// どちらも同じ `f(box)` の包囲なので、健全なら共通部分も健全で、より締まる
///
/// # ⚠️ 共通部分が空になった時
///
/// 健全な包囲同士は **同じ `f(box)` を両方が含む**ので交差は空にならない
/// 空になったなら **どちらかが健全でない**ので、[`Refined::Contradiction`] を返して
/// 呼び手にその cell を未決定として扱わせる (証明も反証もしない)
/// ⚠️ **どちらを信じても根拠が無い**ので、片方を採って続けてはいけない
///
/// これは決着率のための機構だが、**第 1 の経路の健全性の検査器**にもなっている
/// 実測 (2026-09-30): corpus 237 construct のうち `rounded_cone` の 1 件で空交差が出て、
/// 総当たりの真値と突き合わせた結果 **`alice_sdf::interval::eval_interval` 側が
/// 真の最小値を包囲から外していた** (`ia_bsphere` に渡す外接半径が球キャップ分
/// 足りない) `NonOverlap` の決着条件は `ia.lo >= 0.0` を見るので、これは
/// **偽の `proven` を生む向き**の誤り 詳細は ALICE-SDF 側の Backlog
fn refine_with_lipschitz(bx: Vec3Interval, iv: Interval, f_centre: f32, l: f32) -> Refined {
    let Some(e) = lipschitz_enclosure(bx, f_centre, l) else {
        return Refined::Enclosure(iv);
    };
    let (lo, hi) = (iv.lo.max(e.lo), iv.hi.min(e.hi));
    if lo > hi {
        return Refined::Contradiction;
    }
    Refined::Enclosure(Interval::new(lo, hi))
}

/// 既に求めた区間 `iv` を Lipschitz 包囲で締め直す ([`refine_with_lipschitz`] の
/// 中心評価込みの版)
///
/// ⚠️ **区間だけで決着した箱では呼ばない** 中心の点評価 1 回が増えるので、
/// 「区間で決まらなかった箱」に限って払う (八分木の葉だけが対象になる)
fn refine(node: &SdfNode, bx: Vec3Interval, iv: Interval, l: f32) -> Refined {
    if !l.is_finite() || l <= 0.0 {
        return Refined::Enclosure(iv);
    }
    refine_with_lipschitz(bx, iv, sdf_eval(node, box_center(bx)), l)
}

fn probe_pair(
    a: &SdfNode,
    b: &SdfNode,
    cell: Vec3Interval,
    decided_ok: &dyn Fn(Interval, Interval) -> bool,
    witness: &dyn Fn(f32, f32) -> bool,
    residual: &dyn Fn(f32, f32) -> f32,
) -> PairProbe {
    let mut worst: Option<(f32, Vec3)> = None;
    let mut undecided: Option<Vec3Interval> = None;
    let mut stack: Vec<(Vec3Interval, u32)> = vec![(cell, 0)];
    // Lipschitz 定数は node ごとに定まるので八分木を降りる前に 1 度だけ求める
    let (l_a, l_b) = (eval_lipschitz(a), eval_lipschitz(b));

    while let Some((bx, depth)) = stack.pop() {
        let ia = sdf_interval(a, bx);
        let ib = sdf_interval(b, bx);
        if decided_ok(ia, ib) {
            continue;
        }
        let c = box_center(bx);
        let (fa, fb) = (sdf_eval(a, c), sdf_eval(b, c));
        if witness(fa, fb) {
            let r = residual(fa, fb).min(-f32::EPSILON);
            if worst.is_none_or(|(w, _)| r < w) {
                worst = Some((r, c));
            }
        }
        // ⚠️ 第 2 の証明経路 — 区間が緩くて決まらない箱を Lipschitz 包囲で締める
        // (区間演算と独立に健全なので共通部分を取れる、[`refine_with_lipschitz`])
        if let (Refined::Enclosure(ta), Refined::Enclosure(tb)) = (
            refine_with_lipschitz(bx, ia, fa, l_a),
            refine_with_lipschitz(bx, ib, fb, l_b),
        ) {
            if decided_ok(ta, tb) {
                continue;
            }
        } else {
            // ⚠️ 包囲が矛盾した cell は掘らずに未決定にする
            // (どちらの包囲も信用できないので証明も反証もしない)
            if undecided.is_none() {
                undecided = Some(bx);
            }
            continue;
        }
        if depth >= BALL_PROBE_DEPTH {
            if undecided.is_none() {
                undecided = Some(bx);
            }
        } else {
            for child in box_children(bx) {
                stack.push((child, depth + 1));
            }
        }
    }

    match (worst, undecided) {
        (Some((residual, point)), _) => PairProbe::Witness { point, residual },
        (None, Some(region)) => PairProbe::Undecided { region },
        (None, None) => PairProbe::Clear,
    }
}

fn pair_verdict(
    law_name: &str,
    priority: Priority,
    worst: Option<(f32, Vec3, Vec3Interval)>,
    undecided: Option<(Vec3, Vec3Interval)>,
) -> Verdict {
    Verdict {
        violation: worst.map(|(residual, point, region)| Violation {
            law_name: law_name.to_string(),
            priority,
            residual,
            point,
            region,
            // 両方の場が負 / はみ出しの点を実際に見つけている
            evidence: Evidence::Witnessed,
        }),
        unresolved: undecided.map(|(point, region)| Unresolved {
            law_name: law_name.to_string(),
            priority,
            point,
            region,
            reason: UnresolvedReason::SignUndecided,
        }),
    }
}

/// `NonOverlap`: 両 SDF が負（内部）の点があれば重なり
///
/// 証明: 箱で `a ≥ 0` or `b ≥ 0` が一様 → その箱に重なりなし
/// 証拠: 点で両方負 (residual = 浅い方の侵入深さ)
///
/// 0.5.0 まで残差に **場の値** `max(fa, fb)` を使っていたが、場の値は距離では
/// ない (union の内部は距離を過小申告する) ので「侵入深さ」を名乗れない
/// 判定は符号だけを見るので健全だったが、**残差の数値と
/// [`top_violations`] の順位が信用できなかった** 証拠の点が決まってから
/// [`surface_distance`] で実距離に直す
fn check_non_overlap(
    a: &SdfNode,
    b: &SdfNode,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Verdict {
    let mut worst: Option<(f32, Vec3, Vec3Interval)> = None;
    let mut undecided: Option<(Vec3, Vec3Interval)> = None;

    for (center, bounds) in GridSampler::new(config) {
        match probe_pair(
            a,
            b,
            bounds,
            &|ia, ib| ia.lo >= 0.0 || ib.lo >= 0.0,
            &|fa, fb| fa < 0.0 && fb < 0.0,
            // 証拠を選ぶ順位付けにだけ場の値を使う (安い) 残差は後で実距離に直す
            &|fa, fb| fa.max(fb),
        ) {
            PairProbe::Clear => {}
            PairProbe::Witness { point, residual } => match &worst {
                Some((w, _, _)) if residual >= *w => {}
                _ => worst = Some((residual, point, bounds)),
            },
            PairProbe::Undecided { region } => {
                if undecided.is_none() {
                    undecided = Some((center, region));
                }
            }
        }
    }

    // 侵入深さ = 証拠の点から「浅い方」の表面までの実距離
    let cap = search_cap(config);
    let worst = worst.map(|(field, point, region)| {
        let depth = surface_distance(a, point, cap)
            .into_iter()
            .chain(surface_distance(b, point, cap))
            .fold(f32::INFINITY, f32::min);
        let residual = if depth.is_finite() {
            -depth
        } else {
            field // どちらの表面も検査範囲内で見つからない時だけ場の値に戻す
        };
        (residual.min(-f32::EPSILON), point, region)
    });

    pair_verdict(law_name, priority, worst, undecided)
}

/// Containment: inner が内部（< 0）かつ outer が外部（> 0）の点があればはみ出し
///
/// 証明: 箱で `inner ≥ 0` (inner が無い) or `outer ≤ 0` (outer の中) が一様
/// 証拠: 点で `inner < 0 && outer > 0` (residual = はみ出し量)
///
/// 0.5.0 まで残差に **場の値** `−outer` を使っていたが、intersection の外側の
/// 場は真の距離を過小申告するので「はみ出し量」を名乗れない
/// ([`check_non_overlap`] と同軸)
fn check_containment(
    inner: &SdfNode,
    outer: &SdfNode,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Verdict {
    let mut worst: Option<(f32, Vec3, Vec3Interval)> = None;
    let mut undecided: Option<(Vec3, Vec3Interval)> = None;

    for (center, bounds) in GridSampler::new(config) {
        match probe_pair(
            inner,
            outer,
            bounds,
            &|ii, io| ii.lo >= 0.0 || io.hi <= 0.0,
            &|fi, fo| fi < 0.0 && fo > 0.0,
            // 証拠を選ぶ順位付けにだけ場の値を使う 残差は後で実距離に直す
            &|_, fo| -fo,
        ) {
            PairProbe::Clear => {}
            PairProbe::Witness { point, residual } => match &worst {
                Some((w, _, _)) if residual >= *w => {}
                _ => worst = Some((residual, point, bounds)),
            },
            PairProbe::Undecided { region } => {
                if undecided.is_none() {
                    undecided = Some((center, region));
                }
            }
        }
    }

    // はみ出し量 = 証拠の点から outer の表面までの実距離
    let cap = search_cap(config);
    let worst = worst.map(|(field, point, region)| {
        let residual = surface_distance(outer, point, cap).map_or(field, |d| -d);
        (residual.min(-f32::EPSILON), point, region)
    });

    pair_verdict(law_name, priority, worst, undecided)
}

/// `MinThickness`: 内部標本点から表面までの距離が `min_thickness` 未満なら肉厚不足
///
/// 場の値 `|f|` は距離ではないので使わない ([`probe_ball`] で球内の表面を
/// 証明 / 証拠 / 未決定に三分する)
fn check_min_thickness(
    node: &SdfNode,
    min_thickness: f32,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Verdict {
    let mut worst = None;
    let mut undecided = None;

    for (center, bounds) in GridSampler::new(config) {
        if sdf_eval(node, center) >= 0.0 {
            continue; // 内部標本点のみ対象
        }
        accumulate_thickness(
            node,
            center,
            bounds,
            min_thickness,
            &mut worst,
            &mut undecided,
        );
    }

    // 表面までの距離は probe_ball が交点を見つけて上から押さえる
    thickness_verdict(law_name, priority, Evidence::Witnessed, worst, undecided)
}

/// Stress: 各 load point 近傍の内部標本点で、表面までの距離 < force × factor なら violation
///
/// 探索半径 = max(1.0, force) の球内セル対象 (heuristic)
fn check_stress(
    node: &SdfNode,
    load_points: &[(Vec3, f32)],
    min_thickness_factor: f32,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Verdict {
    let mut worst = None;
    let mut undecided = None;

    for (center, bounds) in GridSampler::new(config) {
        if sdf_eval(node, center) >= 0.0 {
            continue; // 内部セルのみ対象
        }

        for &(lp, force) in load_points {
            let search_radius = force.max(1.0);
            if center.distance(lp) > search_radius {
                continue;
            }
            let required = force * min_thickness_factor;
            if required <= 0.0 {
                continue;
            }
            accumulate_thickness(node, center, bounds, required, &mut worst, &mut undecided);
        }
    }

    // 距離そのものは反例で押さえているが、要求値 force × factor が無次元の
    // heuristic なので、違反という結論はモデルに依る (MODEL_STRESS)
    thickness_verdict(
        law_name,
        priority,
        Evidence::Modelled {
            model: MODEL_STRESS,
        },
        worst,
        undecided,
    )
}

/// 表面間距離 (gap) が `m` を超えることの証明
///
/// gap ≤ m なら中点 p に `dist(p, A) ≤ m/2 && dist(p, B) ≤ m/2` の点がある
/// (p が検査 AABB 内にある前提) 各セルを m/2 広げた箱で A か B が一様に
/// 正なら、そのセルにそんな p は無い 全セルで示せれば gap > m
fn gap_exceeds(a: &SdfNode, b: &SdfNode, m: f32, config: &CheckConfig) -> bool {
    let half = m * 0.5;
    // Lipschitz 定数は node ごとに定まるので走査の前に 1 度だけ求める
    let (l_a, l_b) = (eval_lipschitz(a), eval_lipschitz(b));
    // 「膨張した箱が完全に外側」を証明できるか 区間で足りなければ Lipschitz 包囲で
    // 締め直して再判定する (第 2 の証明経路、[`refine`])
    // ⚠️ 刈る = その箱に中点が無いことを**証明する**ので、包囲が矛盾した箱は刈らない
    let provably_outside = |node: &SdfNode, e: Vec3Interval, l: f32| {
        let iv = sdf_interval(node, e);
        if iv.lo > 0.0 {
            return true;
        }
        matches!(refine(node, e, iv, l), Refined::Enclosure(tight) if tight.lo > 0.0)
    };
    for (_, cell) in GridSampler::new(config) {
        let mut stack: Vec<(Vec3Interval, u32)> = vec![(cell, 0)];
        while let Some((bx, depth)) = stack.pop() {
            let e = box_expand(bx, half);
            if provably_outside(a, e, l_a) || provably_outside(b, e, l_b) {
                continue;
            }
            if depth >= BALL_PROBE_DEPTH {
                return false;
            }
            for child in box_children(bx) {
                stack.push((child, depth + 1));
            }
        }
    }
    true
}

/// Contact の gap 上界: 両方の外側にある標本点 c から A / B それぞれの表面への
/// 交点距離の和 (三角不等式で gap ≤ 和) の最小
///
/// gap の中点は何れかの cell 中心から cell 半対角以内にあるので、探索半径は
/// max(min, max) + cell 対角で両側に届く (遠いセルは区間で除外)
fn contact_upper_bound(
    a: &SdfNode,
    b: &SdfNode,
    min_distance: f32,
    max_distance: f32,
    config: &CheckConfig,
) -> Option<(f32, Vec3, Vec3Interval)> {
    #[allow(clippy::cast_precision_loss)]
    let cell_diag =
        ((config.aabb_max - config.aabb_min) / config.resolution.max(1) as f32).length();
    let radius = max_distance.max(min_distance) + cell_diag;
    let mut upper: Option<(f32, Vec3, Vec3Interval)> = None;
    for (center, bounds) in GridSampler::new(config) {
        let (fa, fb) = (sdf_eval(a, center), sdf_eval(b, center));
        if fa <= 0.0 || fb <= 0.0 {
            continue;
        }
        let e = box_expand(bounds, radius);
        if sdf_interval(a, e).lo > 0.0 || sdf_interval(b, e).lo > 0.0 {
            continue;
        }
        let (BallProbe::Crossing { distance: da }, BallProbe::Crossing { distance: db }) =
            (probe_ball(a, center, radius), probe_ball(b, center, radius))
        else {
            continue;
        };
        let ub = da + db;
        match &upper {
            Some((u, _, _)) if ub >= *u => {}
            _ => upper = Some((ub, center, bounds)),
        }
    }
    upper
}

/// Contact: A と B の表面間距離 (gap) が \[min, max\] 範囲外なら violation
///
/// 1. interfering (両 sdf < 0 の点) → residual = 侵入深さ (負)
/// 2. 上界 UB: 両方の外側の標本点 c から A / B それぞれへの交点距離の和
///    (三角不等式で gap ≤ UB)、UB < min なら近すぎ (residual = UB − min)
/// 3. 下界: [`gap_exceeds`] で gap > max を証明できれば遠すぎ
///    (residual = max − UB、UB 無しなら −∞)
/// 4. gap > min の証明 かつ UB ≤ max → pass、それ以外は未決定
fn check_contact(
    a: &SdfNode,
    b: &SdfNode,
    min_distance: f32,
    max_distance: f32,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Verdict {
    // 1. 干渉
    let interference = check_non_overlap(a, b, law_name, priority, config);
    if interference.violation.is_some() {
        return interference;
    }

    // 2. 上界
    let upper = contact_upper_bound(a, b, min_distance, max_distance, config);

    let make_violation =
        |residual: f32, point: Vec3, region: Vec3Interval, evidence: Evidence| Violation {
            law_name: law_name.to_string(),
            priority,
            residual,
            point,
            region,
            evidence,
        };

    if let Some((ub, point, region)) = upper {
        if ub < min_distance {
            return Verdict {
                // 交点を実際に見つけて三角不等式で上から押さえた
                violation: Some(make_violation(
                    ub - min_distance,
                    point,
                    region,
                    Evidence::Witnessed,
                )),
                unresolved: None,
            };
        }
    }

    // 3. 下界
    if gap_exceeds(a, b, max_distance, config) {
        let (residual, point, region) = upper.map_or_else(
            || {
                (
                    f32::NEG_INFINITY,
                    config.aabb_min,
                    Vec3Interval::from_bounds(config.aabb_min, config.aabb_max),
                )
            },
            |(ub, p, r)| (max_distance - ub, p, r),
        );
        return Verdict {
            // gap_exceeds は全セルを区間演算で潰して「gap > max」を証明する
            violation: Some(make_violation(residual, point, region, Evidence::Proved)),
            unresolved: None,
        };
    }

    // 4. pass には gap > min の証明 と UB ≤ max の両方が要る
    if let Some((ub, _, _)) = upper {
        if ub <= max_distance && gap_exceeds(a, b, min_distance, config) {
            return interference; // 干渉側の未決定があればそれを引き継ぐ
        }
    }

    Verdict {
        violation: None,
        unresolved: Some(Unresolved {
            law_name: law_name.to_string(),
            priority,
            point: upper.map_or(config.aabb_min, |(_, p, _)| p),
            region: upper.map_or_else(|| cube(config.aabb_min, 0.0), |(_, _, r)| r),
            reason: UnresolvedReason::GapUnbracketed {
                upper: upper.map_or(f32::INFINITY, |(u, _, _)| u),
            },
        }),
    }
}

/// グリッド上のサンプル点を生成するイテレータ
struct GridSampler {
    aabb_min: Vec3,
    step: Vec3,
    half_step: Vec3,
    n: usize,
    ix: usize,
    iy: usize,
    iz: usize,
}

impl GridSampler {
    fn new(config: &CheckConfig) -> Self {
        let n = config.resolution;
        let extent = config.aabb_max - config.aabb_min;
        #[allow(clippy::cast_precision_loss)]
        let step = extent / n as f32;
        Self {
            aabb_min: config.aabb_min,
            step,
            half_step: step * 0.5,
            n,
            ix: 0,
            iy: 0,
            iz: 0,
        }
    }
}

impl Iterator for GridSampler {
    /// (セル中心の座標, セルの AABB)
    type Item = (Vec3, Vec3Interval);

    fn next(&mut self) -> Option<Self::Item> {
        if self.iz >= self.n {
            return None;
        }

        #[allow(clippy::cast_precision_loss)]
        let lo =
            self.aabb_min + self.step * Vec3::new(self.ix as f32, self.iy as f32, self.iz as f32);
        let center = lo + self.half_step;
        let hi = lo + self.step;

        let bounds = Vec3Interval {
            x: Interval { lo: lo.x, hi: hi.x },
            y: Interval { lo: lo.y, hi: hi.y },
            z: Interval { lo: lo.z, hi: hi.z },
        };

        // 次のセルへ進む
        self.ix += 1;
        if self.ix >= self.n {
            self.ix = 0;
            self.iy += 1;
            if self.iy >= self.n {
                self.iy = 0;
                self.iz += 1;
            }
        }

        Some((center, bounds))
    }
}

/// 1 法則を判定する
fn check_one(law: &Law, config: &CheckConfig) -> Verdict {
    match &law.constraint {
        Constraint::NonOverlap { a, b } => check_non_overlap(a, b, &law.name, law.priority, config),
        Constraint::Containment { inner, outer } => {
            check_containment(inner, outer, &law.name, law.priority, config)
        }
        Constraint::MinThickness {
            node,
            min_thickness,
        } => check_min_thickness(node, *min_thickness, &law.name, law.priority, config),
        Constraint::Stress {
            node,
            load_points,
            min_thickness_factor,
        } => check_stress(
            node,
            load_points,
            *min_thickness_factor,
            &law.name,
            law.priority,
            config,
        ),
        Constraint::Thermal {
            node,
            heat_sources,
            search_radius,
            min_surface_ratio,
        } => check_thermal(
            node,
            heat_sources,
            *search_radius,
            *min_surface_ratio,
            &law.name,
            law.priority,
            config,
        ),
        Constraint::Contact {
            a,
            b,
            min_distance,
            max_distance,
        } => check_contact(
            a,
            b,
            *min_distance,
            *max_distance,
            &law.name,
            law.priority,
            config,
        ),
        Constraint::Continuity { node, seed_point } => {
            check_continuity(node, *seed_point, &law.name, law.priority, config)
        }
        Constraint::GradientBound {
            node,
            max_gradient,
            probe,
        } => check_gradient_bound(node, *max_gradient, *probe, &law.name, law.priority, config),
        Constraint::Reachable { node, from, to } => {
            check_reachable(node, *from, *to, &law.name, law.priority, config)
        }
        Constraint::VolumeConservation {
            before,
            after,
            relative_tolerance,
        } => check_volume_conservation(
            before,
            after,
            *relative_tolerance,
            &law.name,
            law.priority,
            config,
        ),
        #[cfg(feature = "physics")]
        Constraint::ThermalField {
            node,
            sources,
            material,
            metres_per_unit,
            ambient_c,
            max_temperature_c,
            duration_s,
        } => check_thermal_field(
            node,
            sources,
            material,
            *metres_per_unit,
            *ambient_c,
            *max_temperature_c,
            *duration_s,
            &law.name,
            law.priority,
            config,
        ),
    }
}

/// 法則リストを一括検証
#[must_use]
pub fn check_laws(laws: &[Law], config: &CheckConfig) -> LawReport {
    let mut violations = Vec::new();
    let mut unresolved = Vec::new();

    for law in laws {
        let verdict = check_one(law, config);
        if let Some(v) = verdict.violation {
            violations.push(v);
        }
        if let Some(u) = verdict.unresolved {
            unresolved.push(u);
        }
    }

    // 残差の絶対値が大きい順にソート
    violations.sort_by(|a, b| {
        b.residual
            .abs()
            .partial_cmp(&a.residual.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 違反と未決定の両方を持つ法則を二重に引かない
    let mut undecided_names: Vec<&str> = violations.iter().map(|v| v.law_name.as_str()).collect();
    undecided_names.extend(unresolved.iter().map(|u| u.law_name.as_str()));
    undecided_names.sort_unstable();
    undecided_names.dedup();
    let passed = laws.len().saturating_sub(undecided_names.len());

    LawReport {
        total_laws: laws.len(),
        passed,
        violations,
        unresolved,
    }
}

/// 1 熱源の表面比を区間で挟む — `(lo, hi, 代表セル)`
///
/// 未定セルは「表面を含みうる」と「内部かもしれない」の両方に効くので、
/// 比を最小にする取り方 (表面は確定分だけ / 内部は未定も足す) と最大にする
/// 取り方 (表面は未定も足す / 内部は確定分だけ) で上下から挟む
fn surface_ratio_bracket(
    node: &SdfNode,
    source: Vec3,
    search_radius: f32,
    config: &CheckConfig,
    signs: &[CellSign],
) -> Option<(f32, f32, (Vec3, Vec3Interval))> {
    let n = config.resolution;
    let step = grid_step(config);
    let (mut surface_lo, mut surface_hi) = (0usize, 0usize);
    let (mut interior_lo, mut interior_hi) = (0usize, 0usize);
    let mut representative: Option<(Vec3, Vec3Interval)> = None;

    for (ix, iy, iz, idx) in grid_indices(n) {
        let center = cell_center(config, step, ix, iy, iz);
        if center.distance(source) > search_radius {
            continue;
        }
        let region = cell_box(config, step, ix, iy, iz);
        match signs[idx] {
            CellSign::Inside => {
                interior_lo += 1;
                interior_hi += 1;
                if representative.is_none() {
                    representative = Some((center, region));
                }
            }
            CellSign::Outside => {}
            CellSign::Undecided => {
                // 表面を含みうる = 上界に効く / 内部かもしれない = 内部の上界に効く
                surface_hi += 1;
                interior_hi += 1;
                // 符号が実際に割れていれば、中間値定理で表面の存在が確定する
                if cell_straddles_surface(node, region) {
                    surface_lo += 1;
                }
            }
        }
    }

    if interior_hi == 0 {
        return None; // 熱源近傍に内部セルなし = 対象外
    }
    let representative = representative.or_else(|| {
        // 内部確定セルが無い時は探索球の中心を代表にする
        Some((source, cube(source, step.length() * 0.5)))
    })?;

    #[allow(clippy::cast_precision_loss)]
    let lo = surface_lo as f32 / interior_hi as f32;
    #[allow(clippy::cast_precision_loss)]
    let hi = if interior_lo == 0 {
        f32::INFINITY // 内部の下界が 0 なら比の上界は押さえられない
    } else {
        surface_hi as f32 / interior_lo as f32
    };
    Some((lo, hi, representative))
}

/// セル内の標本点で場の符号が割れるか (中間値定理で表面の存在が確定する)
fn cell_straddles_surface(node: &SdfNode, region: Vec3Interval) -> bool {
    let lo = Vec3::new(region.x.lo, region.y.lo, region.z.lo);
    let hi = Vec3::new(region.x.hi, region.y.hi, region.z.hi);
    let first = sdf_eval(node, lo) >= 0.0;
    for c in 1..8u32 {
        let p = Vec3::new(
            if c & 1 == 0 { lo.x } else { hi.x },
            if c & 2 == 0 { lo.y } else { hi.y },
            if c & 4 == 0 { lo.z } else { hi.z },
        );
        if (sdf_eval(node, p) >= 0.0) != first {
            return true;
        }
    }
    (sdf_eval(node, box_center(region)) >= 0.0) != first
}

/// Thermal: 各 heat source 近傍の 表面セル数 / 内部セル数 の比が下限未満なら violation
///
/// 0.5.0 まで表面近傍を `|f(center)| < step` で取っていたが、これは **場の値を
/// 表面までの距離として流用** している 場が真の距離を過大申告する node
/// (TPMS は 1.7〜7.0 倍) では帯がその分痩せ、表面セルを取りこぼして比を
/// 過小評価し、**満たしている形状を違反と断言していた**
///
/// 場の値は使わず、区間演算の三分類だけで比を上下から挟む
/// ([`surface_ratio_bracket`])
/// - 上界 < 下限 → 違反 (どう取っても足りない)
/// - 下界 ≥ 下限 → 合格
/// - 下限を挟む → 未定 ([`UnresolvedReason::SurfaceRatioUnbracketed`])
fn check_thermal(
    node: &SdfNode,
    heat_sources: &[Vec3],
    search_radius: f32,
    min_surface_ratio: f32,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Verdict {
    let step = grid_step(config);
    let signs = classify_cells(node, config, step);

    let mut worst: Option<(f32, Vec3, Vec3Interval)> = None;
    let mut undecided: Option<(Vec3, Vec3Interval, f32, f32)> = None;

    for &source in heat_sources {
        let Some((lo, hi, (point, region))) =
            surface_ratio_bracket(node, source, search_radius, config, &signs)
        else {
            continue;
        };

        if hi < min_surface_ratio {
            let residual = hi - min_surface_ratio; // 負 = 上界でも不足
            match &worst {
                Some((w, _, _)) if residual >= *w => {}
                _ => worst = Some((residual, point, region)),
            }
        } else if lo < min_surface_ratio && undecided.is_none() {
            undecided = Some((point, region, lo, hi));
        }
    }

    Verdict {
        violation: worst.map(|(residual, point, region)| Violation {
            law_name: law_name.to_string(),
            priority,
            residual,
            point,
            region,
            evidence: Evidence::Modelled {
                model: MODEL_THERMAL,
            },
        }),
        unresolved: undecided.map(|(point, region, lo, hi)| Unresolved {
            law_name: law_name.to_string(),
            priority,
            point,
            region,
            reason: UnresolvedReason::SurfaceRatioUnbracketed { lo, hi },
        }),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 応力特異点の **計測** — 判定器はまだ無い
//
// 線形弾性では材料側の内角 `α > π` の入隅で応力が `σ ~ r^(λ−1)` (λ < 1) で
// 発散する (Williams、α = 2π のき裂で λ = 1/2) つまりその形状に対して
// **「最大応力」という量が存在しない** ので、応力を見る法則はそこで判定を
// 降りる必要がある
//
// ⚠️ **ここにあるのは計測だけで、判定器 (形状を走査して入隅を探す層) は
// 未実装** 走査層は 2026-09-29 に一度書いたが、(1) 尺度不変性の判定がどの
// test でも load-bearing でない (判定を丸ごと外しても green) (2) 「決められ
// ない」分岐が 20 通りの scene で一度も発火しない (3) フィレット済の形状に
// 偽陽性が残り、報告値が最悪点でない (最初の一致で返すため) の 3 点が
// 破壊試験で出たので破棄した 再設計は Backlog
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 占有率を測る球面標本の数 (Fibonacci 格子、決定論)
///
/// 標本は球 **面** に置く — 体積で測ると表面を跨ぐセルが 2 割近く出て、
/// 内外が確定しない分が占有率の幅を 0.4〜0.6 に広げてしまい、内角 3π/2
/// (占有率 0.75) すら決められなくなる (2026-09-29 実測)
/// 立体角なら各標本が内か外かに確定し、楔に対しては厳密に `α / 2π` になる
/// 判定器 (走査層) が入るまで production からは呼ばれない
#[allow(dead_code)]
const FRACTION_SAMPLES: usize = 512;

/// 点 `p` を中心とする半径 `r` の **球面** 上で材料が占める割合
///
/// 材料側の内角 `α` に対して `α / 2π` に収束する (平面なら 0.5) ので、
/// **Williams の特異条件 `α > π` は「占有率が 0.5 を超えるか」で測れる**
///
/// # 曲率 API を使わない理由
///
/// `alice_sdf::autodiff::principal_curvatures` は `eval_hessian` = 勾配の
/// 有限差分に依るので、**勾配が不連続な鋭い入隅では `|k| ~ 1/ε` で発散** する
/// (滑らかな面では正しく収束するので API が壊れているわけではない)
/// 「曲率 < −閾値なら入隅」は閾値が実質 length scale になり、`ε` を変えると
/// 結論が変わる 占有率は逆に `r → 0` で内角そのものに収束する
///
/// # 実測 (2026-09-29)
///
/// 内角 3π/2 の鋭い入隅の稜線上で、`r` を 8 倍変えても **0.752 で不変**
/// (`α / 2π = 0.75` と一致) 曲率有限のフィレット面では `0.5` からの隔たりが
/// `r` に比例して縮む (`r` を半分にすると隔たりも 0.50〜0.63 倍)
/// **この差が「閾値でなく尺度不変性で判定する」根拠**
///
/// # これは計測であって判定ではない
///
/// 球面上の点標本なので区間演算のような上下界ではない。形状を走査して
/// 「入隅があるか」を答える層は未実装 (module 冒頭の comment 参照) なので、
/// production からはまだ呼ばれない (下の oracle 2 本が唯一の呼び出し元)
#[allow(dead_code)]
fn material_fraction(node: &SdfNode, p: Vec3, r: f32) -> f32 {
    // Fibonacci 格子 (決定論、乱数を使わないので落ちた時に再現できる)
    const GOLDEN_ANGLE: f32 = 2.399_963_2; // π (3 − √5)
    let mut inside = 0_u32;
    for i in 0..FRACTION_SAMPLES {
        #[allow(clippy::cast_precision_loss)]
        let t = (i as f32 + 0.5) / FRACTION_SAMPLES as f32;
        let z = 2.0f32.mul_add(-t, 1.0);
        let rho = (1.0 - z * z).max(0.0).sqrt();
        #[allow(clippy::cast_precision_loss)]
        let theta = GOLDEN_ANGLE * i as f32;
        let dir = Vec3::new(rho * theta.cos(), rho * theta.sin(), z);
        if sdf_eval(node, p + dir * r) < 0.0 {
            inside += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let frac = f32::from(u16::try_from(inside).unwrap_or(u16::MAX)) / FRACTION_SAMPLES as f32;
    frac
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ThermalField — 実温度場 (`physics` feature、AGPL 伝播あり)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 陽解法の step 数上限 (超えたら解かずに未定を返す)
///
/// CFL は `dt ≤ dx² / (6α)` なので、拡散率の高い材料 (Al は PLA の 700 倍) を
/// 細かい格子で長時間回すと step が爆発する 黙って粗い `dt` で回すと解が
/// 発散するので、解けないことを報告する側に倒す
#[cfg(feature = "physics")]
const THERMAL_MAX_STEPS: u32 = 6000;

/// 表面から熱が逃げる度合い (対流熱伝達率) の両端
///
/// `Debug` は bracket の test が「どちらの端で外れたか」を出すために要る
#[cfg(feature = "physics")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurfaceCooling {
    /// `h = 0` — 熱が一切逃げない (最高温度の上界)
    Adiabatic,
    /// `h = ∞` — 表面が周囲温度に固定される (最高温度の下界)
    Isothermal,
}

/// 熱伝導を解くのに要る入力一式 (引数の取り違えを型で防ぐ)
#[cfg(feature = "physics")]
struct ThermalScene<'a> {
    /// セルが材料か (格子と同じ並び)
    solid: &'a [bool],
    /// 熱源 (位置 \[SDF 単位\], 発熱 \[W\])
    sources: &'a [(Vec3, f32)],
    /// 材料
    material: &'a alice_physics::transient_thermal::ThermalMaterial,
    /// SDF の 1 単位が何 m か
    metres_per_unit: f32,
    /// 周囲温度 \[°C\]
    ambient_c: f32,
    /// 評価する経過時間 \[s\]
    duration_s: f32,
}

/// 熱源 / 材料 / 境界条件を与えて `duration_s` 後の最高温度 \[°C\] を返す
///
/// `alice_physics::transient_thermal::transient_step_3d` は **熱源項を持たず、
/// 箱の 6 面が Neumann 固定** なので、熱源の注入と形状表面の境界条件は
/// step の合間にこちら側で与える (演算子分離):
///
/// 1. 熱源セルに `P·dt / (ρ·cp·dx³)` を加える
/// 2. 拡散を 1 step 進める
/// 3. 形状の外のセルを境界条件に従って上書きする
///    - 断熱: 隣接する材料セルの値で埋める (勾配 0 = 流束 0 の ghost cell)
///    - 等温: 周囲温度に固定する
#[cfg(feature = "physics")]
fn solve_peak_temperature(
    scene: &ThermalScene<'_>,
    cooling: SurfaceCooling,
    config: &CheckConfig,
) -> Option<f32> {
    use alice_physics::transient_thermal::{stable_dt_3d, transient_step_3d};

    let ThermalScene {
        solid,
        sources,
        material,
        metres_per_unit,
        ambient_c,
        duration_s,
    } = *scene;

    let n = config.resolution;
    let step = grid_step(config);
    let dx_m = step.x * metres_per_unit;
    let cell_volume = dx_m * dx_m * dx_m;

    let mut t = vec![ambient_c; n * n * n];

    // 熱源をセルに割り付ける (形状の外の熱源は効かない)
    let mut source_cells: Vec<(usize, f32)> = Vec::new();
    for &(p, watts) in sources {
        let rel = (p - config.aabb_min) / step;
        if rel.x < 0.0 || rel.y < 0.0 || rel.z < 0.0 {
            continue;
        }
        let (ix, iy, iz) = (
            grid_index(rel.x, n),
            grid_index(rel.y, n),
            grid_index(rel.z, n),
        );
        let idx = ix + iy * n + iz * n * n;
        if solid[idx] {
            source_cells.push((idx, watts));
        }
    }

    // CFL で刻みを決める (温度が上がると拡散率が変わるので、まず周囲温度で見積る)
    let dt_max = stable_dt_3d(&t, material, dx_m);
    if !dt_max.is_finite() || dt_max <= 0.0 {
        // 拡散率 0 = 熱が動かない 熱源セルだけが上がる
        return None;
    }
    // 安全率 0.5 (温度上昇で α が増えても CFL を割らないため)
    let dt = (dt_max * 0.5).min(duration_s);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = (duration_s / dt).ceil() as u32;
    if steps > THERMAL_MAX_STEPS {
        return None;
    }

    // 断熱境界で ghost cell を埋める時の「隣の材料セル」
    let neighbours = |idx: usize| -> [Option<usize>; 6] {
        let (ix, iy, iz) = (idx % n, (idx / n) % n, idx / (n * n));
        [
            ix.checked_sub(1).map(|v| v + iy * n + iz * n * n),
            (ix + 1 < n).then_some((ix + 1) + iy * n + iz * n * n),
            iy.checked_sub(1).map(|v| ix + v * n + iz * n * n),
            (iy + 1 < n).then_some(ix + (iy + 1) * n + iz * n * n),
            iz.checked_sub(1).map(|v| ix + iy * n + v * n * n),
            (iz + 1 < n).then_some(ix + iy * n + (iz + 1) * n * n),
        ]
    };

    for _ in 0..steps {
        for &(idx, watts) in &source_cells {
            let rho_cp = material.heat_capacity_at(t[idx]);
            if rho_cp > 0.0 && rho_cp.is_finite() {
                t[idx] += watts * dt / (rho_cp * cell_volume);
            }
        }

        transient_step_3d(&mut t, n, n, n, material, dx_m, dt);

        // 形状の外を境界条件で上書きする
        match cooling {
            SurfaceCooling::Isothermal => {
                for (idx, &is_solid) in solid.iter().enumerate() {
                    if !is_solid {
                        t[idx] = ambient_c;
                    }
                }
            }
            SurfaceCooling::Adiabatic => {
                // 材料セルに隣接する外側セルを、その材料セルの値で埋める
                // = 境界を跨ぐ勾配が 0 になり流束が消える
                let snapshot = t.clone();
                for idx in 0..solid.len() {
                    if solid[idx] {
                        continue;
                    }
                    let mut sum = 0.0_f32;
                    let mut count = 0_u32;
                    for nb in neighbours(idx).into_iter().flatten() {
                        if solid[nb] {
                            sum += snapshot[nb];
                            count += 1;
                        }
                    }
                    t[idx] = if count == 0 {
                        ambient_c
                    } else {
                        #[allow(clippy::cast_precision_loss)]
                        let c = f32::from(u16::try_from(count).unwrap_or(u16::MAX));
                        sum / c
                    };
                }
            }
        }
    }

    // 最高温度は材料セルだけで取る (外側は境界条件の産物で物理量ではない)
    let peak = t
        .iter()
        .zip(solid)
        .filter_map(|(v, &is_solid)| is_solid.then_some(*v))
        .fold(f32::NEG_INFINITY, f32::max);
    peak.is_finite().then_some(peak)
}

/// 熱源が `Inside` の材料に乗っているか、乗っている島が離散化で千切れていないか
///
/// 返り値が `Some` なら解けない理由
///
/// - **熱源が材料に乗らない**: 0.4.0 は「形状の外の熱源は効かない」と黙って
///   捨てていた = 熱が入らず上界も周囲温度になり **偽の合格**になる経路
///   (`Inside ∪ Undecided` で解いていたので踏みにくかっただけで、元からある穴)
/// - **熱源の島が離散化で千切れている**: `Inside` に絞ると薄い部分がバラバラの
///   島になり、熱源の乗った島だけが加熱されて上界が跳ね上がる 判定としては
///   安全側だが「解像度を上げれば直る」ことが利用者に伝わらない
///
///   本当に分かれている部品と区別するために、**`Inside` の連結成分と
///   `Inside ∪ Undecided` の連結成分を比べる** — 後者で繋がっているのに前者で
///   切れているなら、それは形状でなく格子の都合 (閾値を置かずに区別できる)
///
/// # これは soundness の要件ではなく可用性の仕組み
///
/// 材料単調性は連結性と無関係に成り立つ (`Inside ⊆ M` である限り
/// `T_断熱(Inside) ≥ T_断熱(M)`)。分断は **bracket を緩める (上界が上がる)
/// だけで、破る方向には効かない** ので、この検出が無くても判定は嘘をつかず
/// 未定が増えるだけ。**消しても unsound にはならない**ので、動作を変える時に
/// ここを「触れない前提」として扱わなくてよい。
///
/// 逆に、`Inside` と `Inside ∪ Undecided` の **両方が分断されている** 形状は
/// 「別部品の assembly」として通すが、ここには **解像度が全域で不足して首が
/// 切れた連結形状** も混ざる。その場合も上界が過大になって未定へ倒れるので
/// 判定は安全側だが、利用者からは別部品扱いされたことが見えない。
#[cfg(feature = "physics")]
fn thermal_source_placement_problem(
    sources: &[(Vec3, f32)],
    solid: &[bool],
    loose: &[bool],
    config: &CheckConfig,
    step: Vec3,
) -> Option<&'static str> {
    let n = config.resolution;
    for &(p, watts) in sources {
        if watts == 0.0 {
            continue; // 発熱しない source は配置を問わない
        }
        let rel = (p - config.aabb_min) / step;
        if rel.x < 0.0 || rel.y < 0.0 || rel.z < 0.0 {
            return Some("熱源が検査 AABB の外にある");
        }
        let (ix, iy, iz) = (
            grid_index(rel.x, n),
            grid_index(rel.y, n),
            grid_index(rel.z, n),
        );
        let start = (ix, iy, iz, ix + iy * n + iz * n * n);
        if !solid[start.3] {
            return Some("熱源が内部と確定したセルに乗っていない (解像度を上げる)");
        }
        let strict = flood_mask(n, start, &|i| solid[i]);
        let relaxed = flood_mask(n, start, &|i| loose[i]);
        if (0..solid.len()).any(|i| solid[i] && relaxed[i] && !strict[i]) {
            return Some(
                "熱源のある材料が格子の都合で分断されている \
                 (形状としては繋がっているので解像度を上げる)",
            );
        }
    }
    None
}

/// `ThermalField`: 最高温度を境界条件の両端で挟み、3 値で判定する
#[cfg(feature = "physics")]
#[allow(clippy::too_many_arguments)]
fn check_thermal_field(
    node: &SdfNode,
    sources: &[(Vec3, f32)],
    material: &alice_physics::transient_thermal::ThermalMaterial,
    metres_per_unit: f32,
    ambient_c: f32,
    max_temperature_c: f32,
    duration_s: f32,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Verdict {
    let step = grid_step(config);
    let unusable = |why: &'static str| Verdict {
        violation: None,
        unresolved: Some(Unresolved {
            law_name: law_name.to_string(),
            priority,
            point: box_center(Vec3Interval::from_bounds(config.aabb_min, config.aabb_max)),
            region: Vec3Interval::from_bounds(config.aabb_min, config.aabb_max),
            reason: UnresolvedReason::ThermalGridUnusable { why },
        }),
    };

    if config.resolution < 3 {
        return unusable("格子が各軸 3 セル未満 (transient_step_3d が解かない)");
    }
    // `transient_step_3d` は dx を 1 つしか取らない = 立方セルが前提
    let aniso = (step.x - step.y).abs().max((step.x - step.z).abs());
    if aniso > step.x.abs() * 1.0e-4 {
        return unusable("セルが立方でない (検査 AABB を立方体にする)");
    }
    if !(metres_per_unit > 0.0 && duration_s > 0.0) {
        return unusable("metres_per_unit / duration_s が正でない");
    }

    // **両端とも「内部と確定したセルだけ」で解く**
    //
    // 真の材料 `M` は `Inside ⊆ M ⊆ Inside ∪ Undecided` の範囲にあるが、材料を
    // 増やした時に最高温度が動く向きは境界条件で逆になる:
    //
    // - 断熱 (`h = 0`): 質量が増えて同じ熱量を吸うので **下がる**
    // - 等温 (`h = ∞`): 周囲温度に固定される面が熱源から遠のくので **上がる**
    //
    // したがって `T(M, h_true) ≤ T_断熱(M) ≤ T_断熱(Inside)` かつ
    // `T(M, h_true) ≥ T_等温(M) ≥ T_等温(Inside)` で、**両端とも最小材料 =
    // `Inside` で極値を取る** (`Inside ∪ Undecided` で解くと上界は下がり下界は
    // 上がって、bracket が両側から食われる = 0.4.0 の実装の誤り)
    let signs = classify_cells(node, config, step);
    let solid: Vec<bool> = signs.iter().map(|s| *s == CellSign::Inside).collect();
    let loose: Vec<bool> = signs.iter().map(|s| *s != CellSign::Outside).collect();
    if !solid.iter().any(|b| *b) {
        return unusable("内部と確定したセルが 1 つも無い (解像度を上げる)");
    }
    if let Some(why) = thermal_source_placement_problem(sources, &solid, &loose, config, step) {
        return unusable(why);
    }

    let scene = ThermalScene {
        solid: &solid,
        sources,
        material,
        metres_per_unit,
        ambient_c,
        duration_s,
    };
    let solve = |cooling| solve_peak_temperature(&scene, cooling, config);
    let (Some(hi_c), Some(lo_c)) = (
        solve(SurfaceCooling::Adiabatic),
        solve(SurfaceCooling::Isothermal),
    ) else {
        return unusable("CFL 条件で step 数が上限を超えた (解像度を下げるか時間を短く)");
    };

    // どれだけ冷やしても超える = 違反
    if lo_c > max_temperature_c {
        return Verdict {
            violation: Some(Violation {
                law_name: law_name.to_string(),
                priority,
                residual: max_temperature_c - lo_c, // 負 = 下界でも超過 \[K\]
                point: box_center(Vec3Interval::from_bounds(config.aabb_min, config.aabb_max)),
                region: Vec3Interval::from_bounds(config.aabb_min, config.aabb_max),
                evidence: Evidence::Modelled {
                    model: MODEL_THERMAL_FIELD,
                },
            }),
            unresolved: None,
        };
    }
    // 一切冷えなくても収まる = 合格
    if hi_c <= max_temperature_c {
        return Verdict {
            violation: None,
            unresolved: None,
        };
    }
    Verdict {
        violation: None,
        unresolved: Some(Unresolved {
            law_name: law_name.to_string(),
            priority,
            point: box_center(Vec3Interval::from_bounds(config.aabb_min, config.aabb_max)),
            region: Vec3Interval::from_bounds(config.aabb_min, config.aabb_max),
            reason: UnresolvedReason::TemperatureUnbracketed { lo_c, hi_c },
        }),
    }
}

/// grid 座標 (cell 単位、f32) → `[0, n)` の cell index
///
/// `max(0.0)` で負と NaN を 0 に寄せてから truncation、上限は `min(n - 1)` で clamp
/// (`n >= 1` 前提、`n` は `CheckConfig` の grid 解像度で小さい)
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn grid_index(coord: f32, n: usize) -> usize {
    (coord.max(0.0) as usize).min(n - 1)
}

/// `idx` から全セルを 6-connected で flood fill し、到達したセルの mask を返す
fn flood_mask(
    n: usize,
    start: (usize, usize, usize, usize),
    allowed: &dyn Fn(usize) -> bool,
) -> Vec<bool> {
    let mut visited = vec![false; n * n * n];
    if !allowed(start.3) {
        return visited;
    }
    let mut queue = std::collections::VecDeque::new();
    visited[start.3] = true;
    queue.push_back((start.0, start.1, start.2));
    while let Some((x, y, z)) = queue.pop_front() {
        let neighbors = [
            x.checked_sub(1).map(|v| (v, y, z)),
            (x + 1 < n).then_some((x + 1, y, z)),
            y.checked_sub(1).map(|v| (x, v, z)),
            (y + 1 < n).then_some((x, y + 1, z)),
            z.checked_sub(1).map(|v| (x, y, v)),
            (z + 1 < n).then_some((x, y, z + 1)),
        ];
        for (nx, ny, nz) in neighbors.into_iter().flatten() {
            let nidx = nx + ny * n + nz * n * n;
            if !visited[nidx] && allowed(nidx) {
                visited[nidx] = true;
                queue.push_back((nx, ny, nz));
            }
        }
    }
    visited
}

/// Continuity: 内部が 1 つながりかを 3 値で判定する
///
/// 0.5.0 まで **セル中心の点標本だけ** で内部 mask を作っていたため、格子より
/// 細い接続 (首 / 薄板) は中心が 1 つも内部に落ちず、繋がっている形状を
/// **分離と断言していた** ([`check_reachable`] と同じ区間演算の三分類に移す)
///
/// - 違反: 内部と確定したセルが、**外部確定でないセル全部を使っても** seed から
///   到達できない (どんな経路も外部確定セルを通る = 分離が確定)
/// - 合格: 内部確定セルが全て内部確定セルだけで到達でき、かつ未定セルも
///   seed の塊と繋がっている (未定の島に別成分が隠れていない)
/// - それ以外: 未定 ([`UnresolvedReason::ReachabilityUndecided`])
fn check_continuity(
    node: &SdfNode,
    seed_point: Vec3,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Verdict {
    let violation = |residual: f32, point: Vec3, region: Vec3Interval| Violation {
        law_name: law_name.to_string(),
        priority,
        residual,
        point,
        region,
        evidence: Evidence::Modelled {
            model: MODEL_CONTINUITY,
        },
    };
    let undecided_verdict = |point: Vec3, region: Vec3Interval, cells: usize| Verdict {
        violation: None,
        unresolved: Some(Unresolved {
            law_name: law_name.to_string(),
            priority,
            point,
            region,
            reason: UnresolvedReason::ReachabilityUndecided {
                undecided_cells: cells,
            },
        }),
    };

    let seed_dist = sdf_eval(node, seed_point);
    if seed_dist >= 0.0 {
        // seed 点そのものを潰れた箱として報告する (領域ではなく 1 点が原因)
        return Verdict {
            violation: Some(violation(seed_dist, seed_point, point_box(seed_point))),
            unresolved: None,
        };
    }

    let n = config.resolution;
    let step = grid_step(config);
    let signs = classify_cells(node, config, step);
    let undecided_cells = signs.iter().filter(|s| **s == CellSign::Undecided).count();

    let rel = (seed_point - config.aabb_min) / step;
    let (sx, sy, sz) = (
        grid_index(rel.x, n),
        grid_index(rel.y, n),
        grid_index(rel.z, n),
    );
    let seed = (sx, sy, sz, sx + sy * n + sz * n * n);
    let seed_region = cell_box(config, step, sx, sy, sz);

    let inside_total = signs.iter().filter(|s| **s == CellSign::Inside).count();
    if inside_total == 0 {
        // 内部と確定したセルが 1 つも無い = この解像度では何も言えない
        return undecided_verdict(seed_point, seed_region, undecided_cells);
    }

    // (1) 外部確定でないセルを全部使って到達できない内部確定セル = 分離の証明
    let loose = flood_mask(n, seed, &|i| signs[i] != CellSign::Outside);
    let stranded =
        grid_indices(n).find(|&(_, _, _, idx)| signs[idx] == CellSign::Inside && !loose[idx]);
    if let Some((ix, iy, iz, _)) = stranded {
        let unreachable = grid_indices(n)
            .filter(|&(_, _, _, idx)| signs[idx] == CellSign::Inside && !loose[idx])
            .count();
        #[allow(clippy::cast_precision_loss)]
        let ratio = unreachable as f32 / inside_total as f32;
        let region = cell_box(config, step, ix, iy, iz);
        return Verdict {
            violation: Some(violation(-ratio, box_center(region), region)),
            unresolved: None,
        };
    }

    // (2) 内部確定セルだけで全部届き、未定セルも seed の塊に繋がっていれば合格
    let strict = flood_mask(n, seed, &|i| signs[i] == CellSign::Inside);
    let all_inside_linked = (0..signs.len()).all(|i| signs[i] != CellSign::Inside || strict[i]);
    let no_detached_undecided =
        (0..signs.len()).all(|i| signs[i] != CellSign::Undecided || loose[i]);
    if all_inside_linked && no_detached_undecided {
        return Verdict {
            violation: None,
            unresolved: None,
        };
    }

    undecided_verdict(box_center(seed_region), seed_region, undecided_cells)
}

/// 1 点を潰れた箱として表す (領域ではなく点が原因だと報告する時)
const fn point_box(p: Vec3) -> Vec3Interval {
    Vec3Interval {
        x: Interval { lo: p.x, hi: p.x },
        y: Interval { lo: p.y, hi: p.y },
        z: Interval { lo: p.z, hi: p.z },
    }
}

/// セルの箱を `Vec3Interval` にする
fn cell_box(config: &CheckConfig, step: Vec3, ix: usize, iy: usize, iz: usize) -> Vec3Interval {
    #[allow(clippy::cast_precision_loss)]
    let lo = config.aabb_min + step * Vec3::new(ix as f32, iy as f32, iz as f32);
    let hi = lo + step;
    Vec3Interval {
        x: Interval { lo: lo.x, hi: hi.x },
        y: Interval { lo: lo.y, hi: hi.y },
        z: Interval { lo: lo.z, hi: hi.z },
    }
}

/// 勾配上界の検証 — 合格は静的上界の証明、違反は証拠の 2 点、他は未定
fn check_gradient_bound(
    node: &SdfNode,
    max_gradient: f32,
    probe: f32,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Verdict {
    // 静的上界が上界以下なら、標本を 1 点も取らずに合格が証明される
    let claimed = alice_sdf::interval::eval_lipschitz(node);
    if claimed <= max_gradient {
        return Verdict {
            violation: None,
            unresolved: None,
        };
    }

    let n = config.resolution;
    let extent = config.aabb_max - config.aabb_min;
    #[allow(clippy::cast_precision_loss)]
    let step = extent / (n as f32);
    let h = if probe > 0.0 { probe } else { step.x * 0.25 };

    let mut worst = 0.0_f32;
    let mut worst_at = config.aabb_min;
    let mut worst_cell = (0usize, 0usize, 0usize);

    for (ix, iy, iz, _) in grid_indices(n) {
        #[allow(clippy::cast_precision_loss)]
        let center =
            config.aabb_min + step * Vec3::new(ix as f32, iy as f32, iz as f32) + step * 0.5;
        let fp = sdf_eval(node, center);
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let q = center + axis * h;
            let fq = sdf_eval(node, q);
            // eval_lipschitz の保証は外部の主張なので、両端が内部の対は見ない
            if fp < 0.0 && fq < 0.0 {
                continue;
            }
            if !fp.is_finite() || !fq.is_finite() {
                continue;
            }
            let quotient = (fp - fq).abs() / h;
            if quotient > worst {
                worst = quotient;
                worst_at = center;
                worst_cell = (ix, iy, iz);
            }
        }
    }

    if worst > max_gradient {
        // 証拠が出た = 違反が証明された
        return Verdict {
            violation: Some(Violation {
                law_name: law_name.to_string(),
                priority,
                residual: worst - max_gradient,
                point: worst_at,
                region: cell_box(config, step, worst_cell.0, worst_cell.1, worst_cell.2),
                // 上界を超える差分商を取る 2 点が、そのまま反例になっている
                evidence: Evidence::Witnessed,
            }),
            unresolved: None,
        };
    }

    // 標本では超過が出なかったが、静的上界も上界以下を保証しない
    Verdict {
        violation: None,
        unresolved: Some(Unresolved {
            law_name: law_name.to_string(),
            priority,
            point: worst_at,
            region: cell_box(config, step, worst_cell.0, worst_cell.1, worst_cell.2),
            reason: UnresolvedReason::GradientUnwitnessed {
                claimed,
                worst_sampled: worst,
            },
        }),
    }
}

/// `a` から `b` へ、`allowed` なセルだけを 6-connected で辿れるか
///
/// 「どのセルを通ってよいか」を差し替えて 2 回呼ぶのがこの法則の要点:
/// 内部と確定したセルだけで届けば合格の証拠、外部と確定していないセル
/// 全部を使っても届かなければ到達不能の証拠になる。
fn flood_reaches(
    n: usize,
    a: (usize, usize, usize, usize),
    b: (usize, usize, usize),
    allowed: &dyn Fn(usize) -> bool,
) -> bool {
    if !allowed(a.3) {
        return false;
    }
    let mut visited = vec![false; n * n * n];
    let mut queue = std::collections::VecDeque::new();
    visited[a.3] = true;
    queue.push_back((a.0, a.1, a.2));
    while let Some((x, y, z)) = queue.pop_front() {
        if (x, y, z) == b {
            return true;
        }
        let neighbors = [
            x.checked_sub(1).map(|v| (v, y, z)),
            (x + 1 < n).then_some((x + 1, y, z)),
            y.checked_sub(1).map(|v| (x, v, z)),
            (y + 1 < n).then_some((x, y + 1, z)),
            z.checked_sub(1).map(|v| (x, y, v)),
            (z + 1 < n).then_some((x, y, z + 1)),
        ];
        for (nx, ny, nz) in neighbors.into_iter().flatten() {
            let nidx = nx + ny * n + nz * n * n;
            if !visited[nidx] && allowed(nidx) {
                visited[nidx] = true;
                queue.push_back((nx, ny, nz));
            }
        }
    }
    false
}

/// セルを「内部確定 / 外部確定 / 未定」に分ける時の余裕
///
/// 区間演算に外側丸めが無いので、包含が真の値域より **狭くなる** 方向の drift
/// が実測で相対 2.086e-4 ある (`stairs_intersection` で子の区間が親の外に出た、
/// 2026-09-27)。`lo >= 0` をそのまま「外部確定」に使うと、その分だけ偽の
/// 到達不能 (偽の詰み) を主張しうる。判定を `lo > eps` / `hi < -eps` に狭める
/// と、境界のセルは確定でなく **未定**に倒れる — 合格も違反も緩まず、判定
/// できない側に寄るだけなので健全性は保たれる。
const INTERVAL_SLACK: f32 = 1.0e-3;

/// 区間演算によるセルの三分類
///
/// `Undecided` は「表面を含みうる」と「内部かもしれない」の両方を意味する
/// ので、数え上げ系の法則ではこのセルが上界・下界の両方に効く
/// `Debug` は帯の白箱 test が「どの分類になったか」を出すために要る
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CellSign {
    /// セル全体が内部と確定
    Inside,
    /// セル全体が外部と確定
    Outside,
    /// 確定しない (表面を含みうる)
    Undecided,
}

/// 格子の全セルを区間演算で三分類する
///
/// 境界は [`INTERVAL_SLACK`] の余裕で **未定側に倒す** — 合格も違反も緩めず、
/// 判定できない側に寄せるだけなので健全性は保たれる ([`check_reachable`] と
/// 同じ契約) `interval_sign` と違い `d = 0` の扱いで悩まなくて済むのは、
/// slack が 0 の両側を未定にするため
fn classify_cells(node: &SdfNode, config: &CheckConfig, step: Vec3) -> Vec<CellSign> {
    let n = config.resolution;
    let slack = INTERVAL_SLACK * step.length().max(1.0);
    let mut out = vec![CellSign::Undecided; n * n * n];
    for (ix, iy, iz, idx) in grid_indices(n) {
        let iv = sdf_interval(node, cell_box(config, step, ix, iy, iz));
        out[idx] = if iv.lo > slack {
            CellSign::Outside
        } else if iv.hi < -slack {
            CellSign::Inside
        } else {
            CellSign::Undecided
        };
    }
    out
}

/// `config` の格子 1 セルの辺
fn grid_step(config: &CheckConfig) -> Vec3 {
    #[allow(clippy::cast_precision_loss)]
    let n = config.resolution as f32;
    (config.aabb_max - config.aabb_min) / n
}

/// セル index から中心座標
fn cell_center(config: &CheckConfig, step: Vec3, ix: usize, iy: usize, iz: usize) -> Vec3 {
    #[allow(clippy::cast_precision_loss)]
    let lo = config.aabb_min + step * Vec3::new(ix as f32, iy as f32, iz as f32);
    lo + step * 0.5
}

/// 2 点間の到達性 — 3 値すべてが証明になる
///
/// セルを区間演算で「内部と確定」「外部と確定」「未定」に分け、内部確定
/// セルだけの flood fill で届けば合格、外部確定でないセル全体の flood fill
/// でも届かなければ違反 (どんな経路も外部確定セルを通る)、その間なら未定。
fn check_reachable(
    node: &SdfNode,
    from: Vec3,
    to: Vec3,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Verdict {
    let n = config.resolution;
    let extent = config.aabb_max - config.aabb_min;
    #[allow(clippy::cast_precision_loss)]
    let step = extent / (n as f32);

    // 区間演算による 3 分類 (INTERVAL_SLACK の余裕つき、境界は未定に倒す)
    let slack = INTERVAL_SLACK * step.length().max(1.0);
    let mut proven_inside = vec![false; n * n * n];
    let mut proven_outside = vec![false; n * n * n];
    let mut undecided: usize = 0;
    for (ix, iy, iz, idx) in grid_indices(n) {
        let iv = alice_sdf::interval::eval_interval(node, cell_box(config, step, ix, iy, iz));
        if iv.lo > slack {
            proven_outside[idx] = true;
        } else if iv.hi < -slack {
            proven_inside[idx] = true;
        } else {
            undecided += 1;
        }
    }

    let cell_of = |p: Vec3| -> Option<(usize, usize, usize, usize)> {
        let rel = (p - config.aabb_min) / step;
        if rel.x < 0.0 || rel.y < 0.0 || rel.z < 0.0 {
            return None;
        }
        let (x, y, z) = (
            grid_index(rel.x, n),
            grid_index(rel.y, n),
            grid_index(rel.z, n),
        );
        Some((x, y, z, x + y * n + z * n * n))
    };

    let (Some(a), Some(b)) = (cell_of(from), cell_of(to)) else {
        return Verdict {
            violation: None,
            unresolved: Some(Unresolved {
                law_name: law_name.to_string(),
                priority,
                point: from,
                region: cell_box(config, step, 0, 0, 0),
                reason: UnresolvedReason::ReachabilityUndecided {
                    undecided_cells: undecided,
                },
            }),
        };
    };

    // 端点が外部と確定していれば、到達以前に前提が破れている
    for (cell, p) in [(a, from), (b, to)] {
        if proven_outside[cell.3] {
            return Verdict {
                violation: Some(Violation {
                    law_name: law_name.to_string(),
                    priority,
                    residual: sdf_eval(node, p),
                    point: p,
                    region: cell_box(config, step, cell.0, cell.1, cell.2),
                    // 端点のセルが区間演算で「外部」と確定している
                    evidence: Evidence::Proved,
                }),
                unresolved: None,
            };
        }
    }

    // 内部と確定したセルだけで届けば、経路そのものが合格の証拠
    if flood_reaches(n, a, (b.0, b.1, b.2), &|i| proven_inside[i]) {
        return Verdict {
            violation: None,
            unresolved: None,
        };
    }
    // 外部と確定していないセルを全部使っても届かなければ、どんな経路も
    // 外部確定セルを通る = 到達不能が証明された
    if !flood_reaches(n, a, (b.0, b.1, b.2), &|i| !proven_outside[i]) {
        return Verdict {
            violation: Some(Violation {
                law_name: law_name.to_string(),
                priority,
                residual: -(from - to).length(),
                point: to,
                region: cell_box(config, step, b.0, b.1, b.2),
                // 外部確定でないセルを全部使っても届かない = どんな経路も
                // 外部確定セルを通る、という区間演算による証明
                evidence: Evidence::Proved,
            }),
            unresolved: None,
        };
    }

    Verdict {
        violation: None,
        unresolved: Some(Unresolved {
            law_name: law_name.to_string(),
            priority,
            point: to,
            region: cell_box(config, step, b.0, b.1, b.2),
            reason: UnresolvedReason::ReachabilityUndecided {
                undecided_cells: undecided,
            },
        }),
    }
}

/// grid の (ix, iy, iz, `flat_idx`) を返すヘルパー
fn grid_indices(n: usize) -> impl Iterator<Item = (usize, usize, usize, usize)> {
    (0..n).flat_map(move |iz| {
        (0..n).flat_map(move |iy| (0..n).map(move |ix| (ix, iy, iz, ix + iy * n + iz * n * n)))
    })
}

/// `VolumeConservation`: 体積の相対差を区間で挟み、3 値で判定する
///
/// 0.5.0 まで **セル中心の点標本** で内部セル数を数えていたため、どの中心が
/// 内側に落ちるかで count が動き、**体積が厳密に保存される平行移動に対して
/// 数 % 〜 12% の差を申告していた**
///
/// 区間演算の三分類で、セルごとに「両方内部 / 両方外部 (差に効かない)」
/// 「片方が内部と確定し他方が外部と確定 (必ず 1 セル分の差)」「それ以外
/// (±1 セル分まで動きうる)」を分け、差と体積の両方を上下から挟む
///
/// - 差の下界 > 許容 × 体積の上界 → 違反 (どう取っても超える)
/// - 差の上界 ≤ 許容 × 体積の下界 → 合格
/// - 挟む → 未定 ([`UnresolvedReason::VolumeUnbracketed`])
fn check_volume_conservation(
    before: &SdfNode,
    after: &SdfNode,
    relative_tolerance: f32,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Verdict {
    let n = config.resolution;
    let step = grid_step(config);
    let sb = classify_cells(before, config, step);
    let sa = classify_cells(after, config, step);

    // V_after − V_before をセル単位で上下から積む
    //
    // セルが内部を占める割合は Inside = 1 / Outside = 0 / Undecided = [0, 1]
    // なので、1 セルの寄与は (after の下界 − before の上界, after の上界 −
    // before の下界) 未定セルを一律 ±1 にすると、**向きが決まっているのに
    // 両振れ扱い**になって検出力を落とす (例: before 外部確定 × after 未定は
    // 増える側にしか振れない)
    let (mut delta_lo, mut delta_hi) = (0i64, 0i64);
    // 変形前の体積の下界 / 上界 (セル数)
    let (mut vol_lo, mut vol_hi) = (0usize, 0usize);
    let mut witness: Option<(Vec3, Vec3Interval)> = None;

    let frac_bounds = |s: CellSign| match s {
        CellSign::Inside => (1i64, 1i64),
        CellSign::Outside => (0, 0),
        CellSign::Undecided => (0, 1),
    };

    for (ix, iy, iz, idx) in grid_indices(n) {
        let (b_lo, b_hi) = frac_bounds(sb[idx]);
        let (a_lo, a_hi) = frac_bounds(sa[idx]);
        // 変形前の体積は占有割合の下界 / 上界をそのまま積む (0 か 1 のみ)
        match sb[idx] {
            CellSign::Inside => {
                vol_lo += 1;
                vol_hi += 1;
            }
            CellSign::Undecided => vol_hi += 1,
            CellSign::Outside => {}
        }
        delta_lo += a_lo - b_hi;
        delta_hi += a_hi - b_lo;

        // 片方だけが内部と確定したセル = 差があることが確定した場所
        if witness.is_none()
            && matches!(
                (sb[idx], sa[idx]),
                (CellSign::Outside, CellSign::Inside) | (CellSign::Inside, CellSign::Outside)
            )
        {
            let r = cell_box(config, step, ix, iy, iz);
            witness = Some((box_center(r), r));
        }
    }

    if vol_hi == 0 {
        return Verdict {
            violation: None,
            unresolved: None, // 変形前が空 = 対象外
        };
    }

    // |V_after − V_before| の範囲 — delta が 0 を跨ぐなら差 0 があり得る
    let (abs_lo, abs_hi) = if delta_lo <= 0 && delta_hi >= 0 {
        (0i64, delta_lo.abs().max(delta_hi))
    } else {
        let (x, y) = (delta_lo.abs(), delta_hi.abs());
        (x.min(y), x.max(y))
    };

    #[allow(clippy::cast_precision_loss)]
    let (diff_lo, diff_hi) = (abs_lo as f32, abs_hi as f32);
    #[allow(clippy::cast_precision_loss)]
    let (v_lo, v_hi) = (vol_lo as f32, vol_hi as f32);

    // 相対差の下界は体積を最大に取った時、上界は体積を最小に取った時
    let rel_lo = diff_lo / v_hi;
    let rel_hi = if v_lo > 0.0 {
        diff_hi / v_lo
    } else {
        f32::INFINITY
    };

    let (point, region) = witness.unwrap_or_else(|| {
        let r = Vec3Interval::from_bounds(config.aabb_min, config.aabb_max);
        (box_center(r), r)
    });

    if rel_lo > relative_tolerance {
        return Verdict {
            violation: Some(Violation {
                law_name: law_name.to_string(),
                priority,
                residual: relative_tolerance - rel_lo, // 負 = 下界でも超過
                point,
                region,
                evidence: Evidence::Modelled {
                    model: MODEL_VOLUME,
                },
            }),
            unresolved: None,
        };
    }
    if rel_hi <= relative_tolerance {
        return Verdict {
            violation: None,
            unresolved: None,
        };
    }
    Verdict {
        violation: None,
        unresolved: Some(Unresolved {
            law_name: law_name.to_string(),
            priority,
            point,
            region,
            reason: UnresolvedReason::VolumeUnbracketed {
                lo: rel_lo,
                hi: rel_hi,
            },
        }),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 制約合成
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 複数法則をまとめるビルダー
#[derive(Debug, Clone, Default)]
pub struct LawSet {
    laws: Vec<Law>,
}

impl LawSet {
    /// 空の法則セットを作成
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// ハード制約を追加 — **証明か反例を返せる制約に限る**
    ///
    /// # Errors
    ///
    /// モデル推定しか返せない制約には [`NotProvable`] を返す ([`Law::hard`])
    #[allow(clippy::missing_const_for_fn)]
    pub fn hard(
        mut self,
        name: impl Into<String>,
        constraint: Constraint,
    ) -> Result<Self, NotProvable> {
        self.laws.push(Law::hard(name, constraint)?);
        Ok(self)
    }

    /// ソフト制約を追加 — 任意の制約に使える
    #[must_use]
    pub fn soft(mut self, name: impl Into<String>, weight: f32, constraint: Constraint) -> Self {
        self.laws.push(Law::soft(name, weight, constraint));
        self
    }

    /// 証明 / 反例を返せる制約を Hard で積む内部用 (gate を通す必要がない)
    fn push_provable(mut self, name: impl Into<String>, constraint: Constraint) -> Self {
        self.laws.push(Law::hard_unchecked(name, constraint));
        self
    }

    /// 法則リストの参照を返す
    #[must_use]
    pub fn laws(&self) -> &[Law] {
        &self.laws
    }

    /// Stress 制約を **soft で** 追加する convenience
    ///
    /// モデル推定 ([`Evidence::Modelled`]) なので Hard では積めない
    /// 重み付けの根拠は [`Constraint::evidence_class`] の doc を参照
    #[must_use]
    pub fn stress(
        self,
        name: impl Into<String>,
        weight: f32,
        node: SdfNode,
        load_points: Vec<(Vec3, f32)>,
        min_thickness_factor: f32,
    ) -> Self {
        self.soft(
            name,
            weight,
            Constraint::Stress {
                node,
                load_points,
                min_thickness_factor,
            },
        )
    }

    /// Thermal 制約を **soft で** 追加する convenience
    ///
    /// モデル推定 ([`Evidence::Modelled`]) なので Hard では積めない
    #[must_use]
    pub fn thermal(
        self,
        name: impl Into<String>,
        weight: f32,
        node: SdfNode,
        heat_sources: Vec<Vec3>,
        search_radius: f32,
        min_surface_ratio: f32,
    ) -> Self {
        self.soft(
            name,
            weight,
            Constraint::Thermal {
                node,
                heat_sources,
                search_radius,
                min_surface_ratio,
            },
        )
    }

    /// Contact 制約を hard で追加する convenience (反例を返せるので gate 不要)
    #[must_use]
    pub fn contact(
        self,
        name: impl Into<String>,
        a: SdfNode,
        b: SdfNode,
        min_distance: f32,
        max_distance: f32,
    ) -> Self {
        self.push_provable(
            name,
            Constraint::Contact {
                a,
                b,
                min_distance,
                max_distance,
            },
        )
    }

    /// Continuity 制約を **soft で** 追加する convenience
    ///
    /// モデル推定 ([`Evidence::Modelled`]) なので Hard では積めない
    /// 「この 2 点が繋がるか」を証明つきで問いたいなら [`Self::reachable`]
    #[must_use]
    pub fn continuity(
        self,
        name: impl Into<String>,
        weight: f32,
        node: SdfNode,
        seed_point: Vec3,
    ) -> Self {
        self.soft(name, weight, Constraint::Continuity { node, seed_point })
    }

    /// `GradientBound` 制約を hard で追加する convenience
    ///
    /// `max_gradient = 1.0` が「距離を過大申告しない」の意味。
    #[must_use]
    pub fn gradient_bound(
        self,
        name: impl Into<String>,
        node: SdfNode,
        max_gradient: f32,
        probe: f32,
    ) -> Self {
        self.push_provable(
            name,
            Constraint::GradientBound {
                node,
                max_gradient,
                probe,
            },
        )
    }

    /// `Reachable` 制約を hard で追加する convenience (3 値とも証明なので gate 不要)
    #[must_use]
    pub fn reachable(self, name: impl Into<String>, node: SdfNode, from: Vec3, to: Vec3) -> Self {
        self.push_provable(name, Constraint::Reachable { node, from, to })
    }

    /// `VolumeConservation` 制約を **soft で** 追加する convenience
    ///
    /// モデル推定 ([`Evidence::Modelled`]) なので Hard では積めない
    #[must_use]
    pub fn volume_conservation(
        self,
        name: impl Into<String>,
        weight: f32,
        before: SdfNode,
        after: SdfNode,
        relative_tolerance: f32,
    ) -> Self {
        self.soft(
            name,
            weight,
            Constraint::VolumeConservation {
                before,
                after,
                relative_tolerance,
            },
        )
    }

    /// `ThermalField` 制約を **soft で** 追加する convenience
    ///
    /// 実温度場でも数値解 + 離散化 + 境界条件の両端しか挟んでいないので
    /// [`Evidence::Modelled`]、つまり Hard では積めない
    #[cfg(feature = "physics")]
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn thermal_field(
        self,
        name: impl Into<String>,
        weight: f32,
        node: SdfNode,
        sources: Vec<(Vec3, f32)>,
        material: alice_physics::transient_thermal::ThermalMaterial,
        metres_per_unit: f32,
        ambient_c: f32,
        max_temperature_c: f32,
        duration_s: f32,
    ) -> Self {
        self.soft(
            name,
            weight,
            Constraint::ThermalField {
                node,
                sources,
                material,
                metres_per_unit,
                ambient_c,
                max_temperature_c,
                duration_s,
            },
        )
    }

    /// 一括検証
    #[must_use]
    pub fn check(&self, config: &CheckConfig) -> LawReport {
        check_laws(&self.laws, config)
    }

    /// 静的矛盾検出: 同一ノードペアに対する矛盾制約を検出
    ///
    /// 矛盾例: 同じ (A, B) ペアに `NonOverlap` と `Containment`(inner=A, outer=B) を同時適用
    /// → A が B の中にあるのに重ならないのは矛盾
    #[must_use]
    pub fn detect_contradictions(&self) -> Vec<Contradiction> {
        let mut contradictions = Vec::new();

        for (i, law_i) in self.laws.iter().enumerate() {
            for law_j in &self.laws[i + 1..] {
                if let Some(reason) = check_contradiction(&law_i.constraint, &law_j.constraint) {
                    contradictions.push(Contradiction {
                        law_a: law_i.name.clone(),
                        law_b: law_j.name.clone(),
                        reason,
                    });
                }
            }
        }

        contradictions
    }
}

/// 静的矛盾の記述
#[derive(Debug, Clone)]
pub struct Contradiction {
    /// 矛盾する法則 A の名前
    pub law_a: String,
    /// 矛盾する法則 B の名前
    pub law_b: String,
    /// 矛盾の理由
    pub reason: String,
}

/// 2 つの制約が矛盾するかチェック
fn check_contradiction(a: &Constraint, b: &Constraint) -> Option<String> {
    match (a, b) {
        // NonOverlap(X, Y) + Containment(inner=X, outer=Y) → 矛盾
        (Constraint::NonOverlap { a: na, b: nb }, Constraint::Containment { inner, outer })
        | (Constraint::Containment { inner, outer }, Constraint::NonOverlap { a: na, b: nb }) => {
            let dbg_a = format!("{na:?}");
            let dbg_b = format!("{nb:?}");
            let dbg_inner = format!("{inner:?}");
            let dbg_outer = format!("{outer:?}");

            if (dbg_a == dbg_inner && dbg_b == dbg_outer)
                || (dbg_a == dbg_outer && dbg_b == dbg_inner)
            {
                Some(
                    "NonOverlap と Containment が同一ノードペアに適用: 内包されるならば必ず重なる"
                        .to_string(),
                )
            } else {
                None
            }
        }
        // NonOverlap(X, Y) + Contact(X, Y, min, max) with min <= 0 → 矛盾
        // Contact が interfering を許容する (min<=0) のに NonOverlap は禁じている
        (
            Constraint::NonOverlap { a: na, b: nb },
            Constraint::Contact {
                a: ca,
                b: cb,
                min_distance,
                ..
            },
        )
        | (
            Constraint::Contact {
                a: ca,
                b: cb,
                min_distance,
                ..
            },
            Constraint::NonOverlap { a: na, b: nb },
        ) => {
            let dbg_a = format!("{na:?}");
            let dbg_b = format!("{nb:?}");
            let dbg_constraint_a = format!("{ca:?}");
            let dbg_constraint_b = format!("{cb:?}");
            let same_pair = (dbg_a == dbg_constraint_a && dbg_b == dbg_constraint_b)
                || (dbg_a == dbg_constraint_b && dbg_b == dbg_constraint_a);
            if same_pair && *min_distance <= 0.0 {
                Some(
                    "NonOverlap と Contact(min<=0) が同一ノードペアに適用: 接触を許容と禁止が同時"
                        .to_string(),
                )
            } else {
                None
            }
        }
        _ => None,
    }
}

/// 違反レポートから上位 N 件を取得
#[must_use]
pub fn top_violations(report: &LawReport, n: usize) -> Vec<&Violation> {
    report.violations.iter().take(n).collect()
}

/// ハード違反のみを抽出
#[must_use]
pub fn hard_violations(report: &LawReport) -> Vec<&Violation> {
    report
        .violations
        .iter()
        .filter(|v| v.priority == Priority::Hard)
        .collect()
}

/// ソフト違反のみを抽出
#[must_use]
pub fn soft_violations(report: &LawReport) -> Vec<&Violation> {
    report
        .violations
        .iter()
        .filter(|v| matches!(v.priority, Priority::Soft(_)))
        .collect()
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// レポート出力
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 違反レポートのフォーマット済み文字列
#[must_use]
pub fn format_report(report: &LawReport) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "Law Check: {}/{} passed",
        report.passed, report.total_laws
    );

    if report.all_passed() {
        let _ = writeln!(out, "  All laws satisfied (proven at every sample point).");
        return out;
    }

    for u in &report.unresolved {
        let _ = writeln!(
            out,
            "  [UNDECIDED] {}: {:?} at=({:.2},{:.2},{:.2}), region=[{:.2}..{:.2}]x[{:.2}..{:.2}]x[{:.2}..{:.2}]",
            u.law_name,
            u.reason,
            u.point.x, u.point.y, u.point.z,
            u.region.x.lo, u.region.x.hi,
            u.region.y.lo, u.region.y.hi,
            u.region.z.lo, u.region.z.hi,
        );
    }

    for v in &report.violations {
        let severity = match v.priority {
            Priority::Hard => "ERROR",
            Priority::Soft(_) => "WARN ",
        };
        let basis = match v.evidence {
            Evidence::Proved => "proved",
            Evidence::Modelled { .. } => "modelled",
            // 反例つきが既定なので、既定でない 2 つだけを目立たせる
            _ => "witnessed",
        };
        let _ = writeln!(
            out,
            "  [{severity}] {}: residual={:.4}, basis={basis}, at=({:.2},{:.2},{:.2}), region=[{:.2}..{:.2}]x[{:.2}..{:.2}]x[{:.2}..{:.2}]",
            v.law_name,
            v.residual,
            v.point.x, v.point.y, v.point.z,
            v.region.x.lo, v.region.x.hi,
            v.region.y.lo, v.region.y.hi,
            v.region.z.lo, v.region.z.hi,
        );
        // モデルに依る結論は、何を仮定したかまで出さないと読み手が重みを
        // 判断できない
        if let Some(model) = v.evidence.model() {
            let _ = writeln!(out, "            model: {model}");
        }
    }

    out
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 判定器の核の unit test (2026-09-27、cargo-mutants の生存変異を殺すため追加)
//
// `cargo mutants --in-place -p alice-lol --file alice-lol/src/law.rs
//  -- --test analytic_law --test law_tests` = 433 mutant / 104 missed
//
// missed のうち 61 件は sound 化対象外の 3 law (`check_continuity` 30 /
// `check_thermal` 16 / `check_volume_conservation` 15) に集中しており、これは
// 既知の弱点 (flood fill が格子解像度依存、Backlog 221)
//
// 残りは sound 化した 5 law の核に生き残ったもので、本 module はそのうち
// **実害のある変異** を殺す 核の関数は private なので integration test
// (`tests/analytic_law.rs`) からは到達できない
//
// ## 実測で生存が確認されている変異 (2026-09-27、run 36320622842)
//
// 各行に **どの run で生存を観測したか** を書く 分類だけ書いて理由を検証
// できない形にしない (下の「訂正」の原因がそれだった)
//
// * `box_children` の `i & 1 != 0` → `== 0` (×3、`law.rs:362-364`) — i を 0..8 で
//   全走するので 8 個の箱の集合は不変、順序だけ入れ替わる **等価**
// * `probe_ball` の `f_centre > 0.0` → `>=` (`law.rs:421`) — 直前に
//   `f_centre == 0.0` で return しているので同じ分岐になる **等価**
// * `probe_ball` の `near_dist > radius` → `>=` (`law.rs:431`) — 箱の最近点が
//   **厳密に radius** の距離になる配置が要る 下の `gap_exceeds` の訂正を踏まえる
//   と「作れない」とは言い切れないので、**未達成**と書く (octree の分割点と
//   `cube(centre, radius)` の面が一致する配置を作れば殺せる可能性がある)
// * `probe_ball` / `probe_pair` の `d < bd` → `<=` — 等しい時に更新するか
//   しないかの違いで、保持される値は同じ **等価**
// * `probe_ball` の `d < bd` → `>` / `==` (×4) — stack は centre に近い箱から
//   訪問し、比較の直前に `best.is_some_and(|d| near_dist >= d)` で刈るので、
//   `best` の比較が 2 回以上効く状況を作れなかった **未達成** (2 球 union の
//   test では殺せず)
// * `probe_pair` の `residual(..).min(-f32::EPSILON)` の `-` 削除 — 差が出るのは
//   residual が (-EPSILON, 0) に入る場合だけ **未達成**
// * `contact_upper_bound` の**区間による事前 filter** (`sdf_interval(..).lo > 0.0
//   || ...`、`L808`) — 刈らなくても後段の `probe_ball` が `Crossing` を返さず
//   `continue` するので上界は変わらない (仕事量が増えるだけ) **等価**、
//   `mutants.toml` に理由付きで除外を書いてある
//
// ## 2026-09-27 の訂正 (「等価 / 到達困難」と書いたが実際は殺せた 3 件)
//
// 分類だけ書いて理由を検証可能な形で残さなかったため、**誤った断定が doc に
// 残っていた** 実測で覆ったものを記録する:
//
// * `gap_exceeds` の `sdf_interval(..).lo > 0.0` → `>= 0.0` (×2) — 「interval
//   演算を通して浮動小数で厳密 0 を作れない」と書いたが **作れる**:
//   `Box3d` の面に膨張箱の面が乗る配置 (`box3d(2,2,2)` の半幅 1.0 に対し
//   cell `[1.5, 2.0]³` を `half = 0.5` 膨張 → `[1.0, 2.5]³`) で `lo == 0.0`
//   ちょうどになる `gap_exceeds_does_not_clear_a_box_whose_interval_touches_zero`
// * `contact_upper_bound` の `fa <= 0.0 || fb <= 0.0` (`L804`) — 「性能のみ」と
//   書いたが **意味を変える**: `&&` にすると「a の内部だが b の外部」の cell が
//   処理され、内部の点から測った距離の和が上界として返る (契約は「両方の外側の
//   標本点から」) `contact_upper_bound_only_samples_points_outside_both`
//
// ## 本 module で caught に変えた変異 (2026-09-27 実測)
//
// * `interval_sign` の `iv.hi < 0.0` → `<= 0.0` — 表面ちょうどの箱を内側と断定
//   する偽陽性を作る変異
// * `gap_exceeds` の `depth >= BALL_PROBE_DEPTH` → `<` — 深さ 0 で即
//   `return false` になる変異 **殺すには「細分に入る」配置が必須**で、配置を
//   2 回外してから 3 回目で通した (経緯は該当 test の doc)
// * `gap_exceeds` の `lo > 0.0` → `>= 0.0` (×2、上の訂正)
// * `contact_upper_bound` の cell 対角 (`(aabb_max − aabb_min)` の `−` → `+` / `/`)
//   と `L804` の `||` → `&&` (上の訂正)
//
// 測定は `.github/workflows/quality-deep.yml` (週次 + law.rs 系を触った PR)
// 手元で回す時は `--in-place` 必須 (tree copy だと sibling path dep が切れる)、
// `--jobs` とは併用不可、**中断すると変異がソースに残る** ので
// `grep -rn "changed by cargo-mutants" alice-lol/src/` で確認すること
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod core_probe_tests {
    use super::*;

    /// `hi == 0.0` の区間は「表面を含む」ので符号は一様でない
    ///
    /// oracle: 区間 `[-1, 0]` は 0 を含むので内側と断定できない
    /// `iv.hi < 0.0` を `<= 0.0` に変えるとこれが `Some(false)` になり、
    /// **表面ちょうどの箱を「表面なし」として刈る** = 三値判定の健全性が崩れる
    #[test]
    fn an_interval_touching_zero_from_below_is_not_uniformly_negative() {
        // 検査対象は `interval_sign` の契約なので、境界を動かさないように
        // **struct literal で組む**。`Interval::new` は alice-sdf 4.0 の
        // `ba8b967` から外側丸め (`next_down` / `next_up`) を掛けるので、
        // `new(0.0, 1.0)` の lo は 0 のわずか下になり「外側と断定できない」
        // = `None` が正しい答えになってしまい、この test が見たい性質
        // (lo が厳密に 0 なら外側と断定する) を測れなくなる。
        assert_eq!(interval_sign(Interval { lo: -1.0, hi: 0.0 }), None);
        // 両端が厳密に負 / 非負なら断定できる
        assert_eq!(interval_sign(Interval { lo: -1.0, hi: -0.5 }), Some(false));
        assert_eq!(interval_sign(Interval { lo: 0.0, hi: 1.0 }), Some(true));
        // 外側丸めを通した区間は、境界に触れていれば断定しない (健全側)
        assert_eq!(interval_sign(Interval::new(0.0, 1.0)), None);
    }

    /// 球面より内側に入った表面は交点として拾う
    ///
    /// oracle: 半径 1 の球の表面は原点から 1.0 中心を `(2, 0, 0)`、探索半径を
    /// 1.5 に取ると表面は中心から 1.0 = 球面より内側にあり、距離 1.0 が返る
    /// `near_dist > radius` を `>=` に変えると、**箱の最近点が球面ちょうどに
    /// 乗る箱** (= 表面を含む側の箱) が刈られて距離が悪化するか見失う
    ///
    /// なお `radius` を厳密に 1.0 (表面が球面上) にすると [`BallProbe::Clear`]
    /// が正しい: [`interval_sign`] の契約では `d = 0` は「外側 or 表面」side な
    /// ので、区間 `[0, …]` は一様に非負 = 表面なしと判定される
    #[test]
    fn probe_ball_finds_a_surface_inside_the_ball() {
        let sphere = SdfNode::sphere(1.0);
        let probe = probe_ball(&sphere, Vec3::new(2.0, 0.0, 0.0), 1.5);
        match probe {
            BallProbe::Crossing { distance } => {
                assert!(
                    (distance - 1.0).abs() < 1e-1,
                    "表面までの距離は 1.0 のはずが {distance}"
                );
            }
            other => panic!("球内の表面を拾えていない: {other:?}"),
        }
    }

    /// 交点が複数あるときは **近い側** を返す
    ///
    /// oracle: 半径 1 の球を `x = ±3` に置いた union を原点から見ると、表面は
    /// `x = ±2` (距離 2) と `x = ±4` (距離 4) にある 返る値は近い側でなければ
    /// ならない (`best` 更新の `d < bd` を `>` / `==` に変えると 4 側が残る)
    ///
    /// 厳密な最近距離は 2.0 だが、`probe_ball` は octree の箱の最近点に向けて
    /// 二分探索するので `BALL_PROBE_DEPTH = 4` の分割精度で上振れする (実測
    /// 2.14) 上界として保守的な側にずれるので law の健全性には影響しない
    /// ここで見るのは「**2 と 4 のどちらを選んだか**」であって小数第 2 位ではない
    #[test]
    fn probe_ball_returns_the_nearer_crossing_not_the_far_one() {
        let left = SdfNode::sphere(1.0).translate(-3.0, 0.0, 0.0);
        let right = SdfNode::sphere(1.0).translate(3.0, 0.0, 0.0);
        let pair = SdfNode::union(left, right);
        let probe = probe_ball(&pair, Vec3::ZERO, 5.0);
        match probe {
            BallProbe::Crossing { distance } => {
                assert!(
                    (2.0..3.0).contains(&distance),
                    "近い側 (厳密 2.0、octree 精度で 2.1 前後) を選ぶべきが {distance} \
                     — 4.0 付近なら遠い交点を採用している"
                );
            }
            other => panic!("交点を拾えていない: {other:?}"),
        }
    }

    /// gap が m を上回る 2 形状で真になる — **細分に入る配置で**
    ///
    /// oracle: 半径 1 の球を `x = ±2` に置くと向かい合う表面は `x = ∓1` なので
    /// gap は 2.0 `m = 1.5` (`half = 0.75`) は満たす
    ///
    /// **配置の要件 (ここが本質)**: [`GridSampler`] の cell 境界は
    /// `aabb_min + k · (extent / resolution)` なので、**resolution を奇数にすると
    /// 中央 cell が原点をまたぐ** `aabb −3..3` / `resolution 3` なら step 2.0 で
    /// 中央 cell は `[−1, 1]³`
    ///
    /// その cell を `half = 0.75` 膨張すると `[−1.75, 1.75]³` で、両球の最近点
    /// (`x = ∓1.75`、中心から 0.25) の SDF は `−0.75 < 0` = **両方に触れるので
    /// `continue` されず細分に入る** 子 (`[−1, 0]³` 等) を膨張した
    /// `[−1.75, 0.75]³` は右球まで 1.25 (SDF `+0.25`) で離れるため `continue` し、
    /// 最終的に真が返る
    ///
    /// この「細分に入る」ことが `depth >= BALL_PROBE_DEPTH` を `<` に変えた変異を
    /// 殺す条件になる (変異後は深さ 0 の中央 cell で即 `return false`)
    ///
    /// 2026-09-27 に配置を 2 回外している: `x = ±5` (gap 8) は最初の cell で
    /// `continue` されて細分に入らず、既定 config (`−5..5` / `resolution 8`、
    /// cell 境界が原点に乗る) では `[0, 1.25]` を 0.75 膨張しても左球の表面
    /// `x = −1` に 0.25 届かず、どちらも変異を殺せなかった
    #[test]
    fn gap_exceeds_is_true_and_subdivides_for_a_gap_just_above_m() {
        let a = SdfNode::sphere(1.0).translate(-2.0, 0.0, 0.0);
        let b = SdfNode::sphere(1.0).translate(2.0, 0.0, 0.0);
        // 中央 cell を原点にまたがせるため resolution は奇数
        let config = CheckConfig {
            aabb_min: Vec3::splat(-3.0),
            aabb_max: Vec3::splat(3.0),
            resolution: 3,
        };
        assert!(
            gap_exceeds(&a, &b, 1.5, &config),
            "gap 2.0 の 2 球で m = 1.5 が満たされない"
        );
    }

    /// 区間の下界が **厳密に 0.0** の箱を「離れている」と刈ってはいけない
    ///
    /// oracle: `Box3d` の半幅 1 と、膨張後の面が `x = 1.0` にちょうど乗る cell
    /// を取ると区間の下界は 0.0 ちょうどになる 下界 0.0 は「表面に接する」
    /// = その箱に中点があり得るので、証明にはならない
    ///
    /// `sdf_interval(..).lo > 0.0` を `>= 0.0` に変えると深さ 0 の cell が即
    /// 刈られて `gap_exceeds` が真を返す (= gap を証明していないのに証明済と
    /// 言う) cell の低 x 側の子は膨張後も同じ面に乗り続けるので、正しい実装
    /// では深さ上限まで決まらず偽になる a 側 / b 側の両方の比較を見るため
    /// 役割を入れ替えた 2 例を assert する (2026-09-27 mutants 生存 2 件)
    #[test]
    fn gap_exceeds_does_not_clear_a_box_whose_interval_touches_zero() {
        // `box3d` は **全長** を取るので半幅 1.0 は `box3d(2.0, …)`
        let boxy = SdfNode::box3d(2.0, 2.0, 2.0);
        // 区間が常に負 = 「この箱は決着しない」側を固定する相方
        let everywhere = SdfNode::sphere(50.0);
        // cell [1.5, 2.0]³ を half = 0.5 膨張すると [1.0, 2.5]³ で箱の面に接する
        let config = CheckConfig {
            aabb_min: Vec3::splat(1.5),
            aabb_max: Vec3::splat(2.0),
            resolution: 1,
        };
        assert!(
            !gap_exceeds(&boxy, &everywhere, 1.0, &config),
            "a 側の区間下界 0.0 (表面に接する) を「離れている」と刈っている"
        );
        assert!(
            !gap_exceeds(&everywhere, &boxy, 1.0, &config),
            "b 側の区間下界 0.0 (表面に接する) を「離れている」と刈っている"
        );
    }

    /// 探索半径は **cell 対角を含む** — 含まないと上界を見失う
    ///
    /// oracle: 半径 1 の球を `x = 0` と `x = 4` に置き `aabb −2..2` /
    /// `resolution 2` で見ると、cell 中心 `(±1, ±1, ±1)` から各表面までは
    /// 0.73 と 2.32 `min/max_distance` は 0.1 / 0.2 なので、cell 対角 3.46 が
    /// 半径に入って初めて両表面に届く
    ///
    /// `(aabb_max − aabb_min)` の `−` を `+` に変えると対称 aabb では 0 に、
    /// `/` に変えると 0.87 になり、どちらも届かず `None` が返る
    /// (2026-09-27 mutants 生存 2 件)
    #[test]
    fn contact_upper_bound_radius_includes_the_cell_diagonal() {
        let a = SdfNode::sphere(1.0);
        let b = SdfNode::sphere(1.0).translate(4.0, 0.0, 0.0);
        let config = CheckConfig {
            aabb_min: Vec3::splat(-2.0),
            aabb_max: Vec3::splat(2.0),
            resolution: 2,
        };
        let found = contact_upper_bound(&a, &b, 0.1, 0.2, &config);
        let Some((ub, point, _)) = found else {
            panic!("cell 対角を含む半径なら上界が取れるはずが None");
        };
        // 三角不等式の上界なので真の gap 2.0 を下回ってはいけない
        assert!(ub >= 2.0, "上界 {ub} が真の gap 2.0 を下回っている");
        assert!(
            sdf_eval(&a, point) > 0.0 && sdf_eval(&b, point) > 0.0,
            "上界の標本点 {point:?} が形状の内部にある"
        );
    }

    /// 上界の標本点は **両方の外側** に限る
    ///
    /// oracle: 検査範囲を球 a の内部だけ (`−1..1`、cell 中心 `(±0.5)³` は
    /// `|p| = 0.87 < 1` で内部) に取ると、外側の標本点は 1 つも無いので上界は
    /// 取れない = `None`
    ///
    /// `fa <= 0.0 || fb <= 0.0` を `&&` に変えると「a の内部だが b の外部」の
    /// cell が処理され、内部の点から測った距離の和が上界として返る
    /// (2026-09-27 mutants 生存 1 件)
    #[test]
    fn contact_upper_bound_only_samples_points_outside_both() {
        let a = SdfNode::sphere(1.0);
        let b = SdfNode::sphere(1.0).translate(2.6, 0.0, 0.0);
        let config = CheckConfig {
            aabb_min: Vec3::splat(-1.0),
            aabb_max: Vec3::splat(1.0),
            resolution: 2,
        };
        assert!(
            contact_upper_bound(&a, &b, 0.1, 0.2, &config).is_none(),
            "標本点が全て a の内部なのに上界を返した"
        );
    }

    /// Containment も境界の点を witness にしてはいけない
    ///
    /// witness は `fi < 0.0 && fo > 0.0` (inner の**内部**かつ outer の**外部**)
    /// `fi <= 0.0` にすると inner の表面上の点、`fo >= 0.0` にすると outer の
    /// 表面上の点が「はみ出し」として報告される
    ///
    /// oracle: はみ出しの厚みを ε = 1e-3 にすると、深さ 4 の標本間隔 0.0625 で
    /// 当たるのは境界ちょうどの 1 点だけになる (2026-09-27 run 36325718354 の
    /// 生存変異 `law.rs:675` ×2)
    // 厳密比較は **検査対象そのもの**: 軸平行な箱の面上 / 中点なので f32 でも
    // 厳密に 0 / 0.5 になり、その厳密性が前提の oracle である
    #[allow(clippy::float_cmp)]
    #[test]
    fn a_point_on_either_boundary_is_not_a_containment_witness() {
        const EPS: f32 = 1.0e-3;
        let config = CheckConfig {
            aabb_min: Vec3::new(0.5, -0.5, -0.5),
            aabb_max: Vec3::new(1.5, 0.5, 0.5),
            resolution: 1,
        };
        let boundary = Vec3::new(1.0, 0.0, 0.0);

        // (1) inner の表面 (fi = 0) かつ outer の外部 (fo > 0) — `fi <= 0.0` 変異用
        let inner = SdfNode::box3d(2.0, 2.0, 2.0);
        let outer = SdfNode::box3d(2.0f32.mul_add(-EPS, 2.0), 2.0, 2.0);
        assert_eq!(
            sdf_eval(&inner, boundary),
            0.0,
            "inner の表面が厳密 0 でない"
        );
        assert!(
            sdf_eval(&outer, boundary) > 0.0,
            "outer の外部になっていない"
        );
        let report = check_laws(
            &[
                Law::hard("surface_of_inner", Constraint::Containment { inner, outer })
                    .expect("provable constraint"),
            ],
            &config,
        );
        assert!(
            !report.has_hard_violations(),
            "inner の表面上の点 (fi = 0) をはみ出しの witness にした\n{}",
            format_report(&report)
        );

        // (2) inner の内部 (fi < 0) かつ outer の表面 (fo = 0) — `fo >= 0.0` 変異用
        let inner2 = SdfNode::box3d(2.0f32.mul_add(EPS, 2.0), 2.0, 2.0);
        let outer2 = SdfNode::box3d(2.0, 2.0, 2.0);
        assert!(
            sdf_eval(&inner2, boundary) < 0.0,
            "inner の内部になっていない"
        );
        assert_eq!(
            sdf_eval(&outer2, boundary),
            0.0,
            "outer の表面が厳密 0 でない"
        );
        let report2 = check_laws(
            &[Law::hard(
                "surface_of_outer",
                Constraint::Containment {
                    inner: inner2,
                    outer: outer2,
                },
            )
            .expect("provable constraint")],
            &config,
        );
        assert!(
            !report2.has_hard_violations(),
            "outer の表面上の点 (fo = 0) をはみ出しの witness にした\n{}",
            format_report(&report2)
        );
    }

    /// gap が下限ちょうどの scene を「近すぎ」と報告しない
    ///
    /// oracle: 半幅 1 の箱 (`box3d` は全長を取る) を `x = 0` と `x = 3` に置くと
    /// gap は厳密に 1.0 で、`min_distance = 1.0` を満たしている
    ///
    /// **この test は `ub < min_distance` → `<=` の変異を殺さない** (2026-09-27
    /// 実測): 上界は `probe_ball` の八分木降下 + 二分探索が返す**緩い**上界で、
    /// この scene では真の gap 1.0 に対し **1.1056** を返す 等号を踏ませるには
    /// 上界の値を厳密に `min_distance` に一致させる必要があり、その値は探索順に
    /// 依存する人工物なので構成しても意味がない 変異は `mutants.toml` に
    /// **未達成** として理由付きで除外してある (等価ではない)
    // 厳密比較は **検査対象そのもの**: 軸平行な箱の面上 / 中点なので f32 でも
    // 厳密に 0 / 0.5 になり、その厳密性が前提の oracle である
    #[allow(clippy::float_cmp)]
    #[test]
    fn contact_upper_bound_equal_to_min_distance_is_not_too_close() {
        let a = SdfNode::box3d(2.0, 2.0, 2.0);
        let b = SdfNode::box3d(2.0, 2.0, 2.0).translate(3.0, 0.0, 0.0);
        // cell 中心が 2 表面の中点に乗る単一 cell
        let config = CheckConfig {
            aabb_min: Vec3::new(1.0, -0.5, -0.5),
            aabb_max: Vec3::new(2.0, 0.5, 0.5),
            resolution: 1,
        };
        let mid = Vec3::new(1.5, 0.0, 0.0);
        assert_eq!(sdf_eval(&a, mid), 0.5, "a までの距離が 0.5 でない");
        assert_eq!(sdf_eval(&b, mid), 0.5, "b までの距離が 0.5 でない");

        let report = check_laws(
            &[Law::hard(
                "gap_at_the_lower_bound",
                Constraint::Contact {
                    a,
                    b,
                    min_distance: 1.0,
                    max_distance: 2.0,
                },
            )
            .expect("provable constraint")],
            &config,
        );
        assert!(
            !report.has_hard_violations(),
            "gap が下限ちょうど (1.0) の scene を「近すぎ」と報告した\n{}",
            format_report(&report)
        );
    }

    /// 接している 2 形状では「gap が m を超える」が偽になる
    ///
    /// oracle: 半径 1 の球を `x = ±1` に置くと表面は原点で接し gap は 0
    /// どんな正の m も満たさない
    #[test]
    fn gap_exceeds_is_false_for_touching_shapes() {
        let a = SdfNode::sphere(1.0).translate(-1.0, 0.0, 0.0);
        let b = SdfNode::sphere(1.0).translate(1.0, 0.0, 0.0);
        let config = CheckConfig::default();
        assert!(
            !gap_exceeds(&a, &b, 0.5, &config),
            "原点で接している 2 球が gap 0.5 を満たしてしまっている"
        );
    }

    /// 片方の場が **厳密に 0** の標本点を witness にしてはいけない
    ///
    /// `interval_sign` の契約どおり `d = 0` は「外側 or 表面」側なので、witness は
    /// `fa < 0.0 && fb < 0.0` (両方とも厳密に負) でなければならない `<=` に
    /// 変えると **表面上の点で侵入を報告する**
    ///
    /// oracle: 半幅 1 の箱 (`box3d` は全長を取る) を ε = 1e-3 だけ重ねて置くと、
    /// 面 `x = 1` は a の表面 (fa = 0) かつ b の内部 (fb = −ε) になる 真の重なりは
    /// 厚さ ε しかなく、cell 1.0 を深さ 4 まで割った標本間隔 0.0625 では
    /// **x = 1 ちょうどの標本しか当たらない** = 正しい実装は witness 無し (未決定)、
    /// `<=` 変異だけが違反を報告する 役割を入れ替えた 2 例で a 側 / b 側の両方の
    /// 比較を見る (2026-09-27 run 36325718354 の生存変異 `law.rs:636` ×2)
    // 厳密比較は **検査対象そのもの**: 軸平行な箱の面上 / 中点なので f32 でも
    // 厳密に 0 / 0.5 になり、その厳密性が前提の oracle である
    #[allow(clippy::float_cmp)]
    #[test]
    fn a_point_on_one_surface_is_not_an_overlap_witness() {
        const EPS: f32 = 1.0e-3;
        // cell 中心が x = 1 に乗る単一 cell
        let config = CheckConfig {
            aabb_min: Vec3::new(0.5, -0.5, -0.5),
            aabb_max: Vec3::new(1.5, 0.5, 0.5),
            resolution: 1,
        };
        let touch = Vec3::new(1.0, 0.0, 0.0);

        // (1) a の表面 (fa = 0) かつ b の内部 (fb < 0) — `fa <= 0.0` 変異が発火する
        let a = SdfNode::box3d(2.0, 2.0, 2.0);
        let b = SdfNode::box3d(2.0, 2.0, 2.0).translate(2.0 - EPS, 0.0, 0.0);
        assert_eq!(sdf_eval(&a, touch), 0.0, "a の表面の場が厳密 0 でない");
        assert!(sdf_eval(&b, touch) < 0.0, "b の内部になっていない");
        let report = check_laws(
            &[Law::hard("surface_of_a", Constraint::NonOverlap { a, b })
                .expect("provable constraint")],
            &config,
        );
        assert!(
            !report.has_hard_violations(),
            "a の表面上の点 (fa = 0) を侵入の witness にした\n{}",
            format_report(&report)
        );

        // (2) 役割を入れ替え: b の表面 (fb = 0) かつ a の内部 — `fb <= 0.0` 変異用
        let a2 = SdfNode::box3d(2.0f32.mul_add(EPS, 2.0), 2.0, 2.0);
        let b2 = SdfNode::box3d(2.0, 2.0, 2.0).translate(2.0, 0.0, 0.0);
        assert_eq!(sdf_eval(&b2, touch), 0.0, "b の表面の場が厳密 0 でない");
        assert!(sdf_eval(&a2, touch) < 0.0, "a の内部になっていない");
        let report2 = check_laws(
            &[
                Law::hard("surface_of_b", Constraint::NonOverlap { a: a2, b: b2 })
                    .expect("provable constraint"),
            ],
            &config,
        );
        assert!(
            !report2.has_hard_violations(),
            "b の表面上の点 (fb = 0) を侵入の witness にした\n{}",
            format_report(&report2)
        );
    }

    /// 半対角と余裕の算術を厳密な数値で固定する
    ///
    /// oracle: 辺長 `(2, 2, 1)` の箱の半対角は `0.5·√(4+4+1) = 1.5` (厳密)
    /// `L = 2` なら余裕は `L·ρ = 3.0` なので、`f(c) = 10` の包囲は `[7, 13]`
    ///
    /// 辺長を `hi − lo` 以外 (`hi + lo` / `hi / lo`) にすると半対角が変わり、
    /// `0.5 * Σ` や `dz * dz` や `L * ρ` を `+` にすると余裕が変わるので、
    /// ⚠️ **この 1 本で `lipschitz_enclosure` の算術 6 箇所すべてが動く**
    ///
    /// ⚠️ `Interval::new` は外側丸めを掛けるので厳密一致では見ない
    /// (変異は 0.5 以上ずれるので 1e-3 で十分に分離する)
    #[test]
    fn the_lipschitz_enclosure_uses_the_half_diagonal_and_the_product_with_l() {
        let bx = Vec3Interval {
            // ⚠️ 3 軸すべて **非対称** に取る — 対称区間 (`[-a, a]`) だと
            // `hi - lo` を `hi / lo` に変えても `-1` になり、**二乗で符号が消えて
            // 辺長の二乗が変わらない**ので変異が見えなくなる (2026-10-01 実測、
            // `z: [-0.5, 0.5]` で `law.rs:1094:71` の `-` → `/` を殺せなかった)
            // 辺長は (2, 2, 1) を保つので半対角は `0.5·√9 = 1.5` で f32 厳密
            x: Interval { lo: 0.5, hi: 2.5 },
            y: Interval { lo: 1.0, hi: 3.0 },
            z: Interval { lo: 0.25, hi: 1.25 },
        };
        let e = lipschitz_enclosure(bx, 10.0, 2.0)
            .expect("外部 (f(c) > 0) で上界も有限なので包囲が出る");
        assert!(
            (e.lo - 7.0).abs() < 1e-3 && (e.hi - 13.0).abs() < 1e-3,
            "半対角 1.5 × L 2.0 = 余裕 3.0 で [7, 13] のはずが [{}, {}]",
            e.lo,
            e.hi
        );
    }

    /// 契約 (外部限定 / 有限な上界) を外れた入力では何も主張しない
    ///
    /// oracle: `eval_lipschitz` の契約は `{f ≥ 0}` 限定なので、`f(c) < 0` の箱で
    /// 包囲を作ってはいけない `L` が非有限 / 非正の時も同じ
    ///
    /// ⚠️ `rho.is_finite()` の `!` を消すと **有限な ρ で `None` を返す**ように
    /// なるので、上の test が `expect` で落ちてこの分岐の変異も殺される
    #[test]
    fn the_lipschitz_enclosure_refuses_interior_boxes_and_useless_bounds() {
        let bx = Vec3Interval {
            // ⚠️ 3 軸すべて **非対称** に取る — 対称区間 (`[-a, a]`) だと
            // `hi - lo` を `hi / lo` に変えても `-1` になり、**二乗で符号が消えて
            // 辺長の二乗が変わらない**ので変異が見えなくなる (2026-10-01 実測、
            // `z: [-0.5, 0.5]` で `law.rs:1094:71` の `-` → `/` を殺せなかった)
            // 辺長は (2, 2, 1) を保つので半対角は `0.5·√9 = 1.5` で f32 厳密
            x: Interval { lo: 0.5, hi: 2.5 },
            y: Interval { lo: 1.0, hi: 3.0 },
            z: Interval { lo: 0.25, hi: 1.25 },
        };
        // 内部点 (契約外)
        assert!(
            lipschitz_enclosure(bx, -0.1, 2.0).is_none(),
            "f(c) < 0 で主張した"
        );
        // 上界が非正 / 非有限
        assert!(
            lipschitz_enclosure(bx, 10.0, 0.0).is_none(),
            "L = 0 で主張した"
        );
        assert!(
            lipschitz_enclosure(bx, 10.0, -1.0).is_none(),
            "L < 0 で主張した"
        );
        assert!(
            lipschitz_enclosure(bx, 10.0, f32::INFINITY).is_none(),
            "L が非有限で主張した"
        );
        // f(c) が非有限
        assert!(
            lipschitz_enclosure(bx, f32::NAN, 2.0).is_none(),
            "f(c) が NaN で主張した"
        );
        // 表面ちょうど (f(c) = 0) は契約内なので主張してよい
        assert!(
            lipschitz_enclosure(bx, 0.0, 2.0).is_some(),
            "f(c) = 0 は契約内 (`f < 0` で弾く) なのに主張しなかった"
        );
    }

    /// 交差が 1 点に潰れるのは矛盾ではない
    ///
    /// oracle: 2 つの健全な包囲が 1 点で接する時、その点は両方に含まれるので
    /// **`f(box)` はその値しか取れない**という最も締まった情報になる
    /// 空 (`lo > hi`) とは意味が違う
    ///
    /// ⚠️ `if lo > hi` を `>=` に変えるとこの 1 点が [`Refined::Contradiction`] に
    /// 化け、**決着できる cell を未決定に落とす** (健全だが決着率が下がる向き)
    /// `==` に変えると下の空交差が矛盾として扱われず、⚠️ **健全でない包囲を
    /// 信じて進む**ので向きが逆に危ない
    ///
    /// ⚠️ 区間側は外側丸めを避けるため struct literal で組む (1 点交差を作るため)
    #[test]
    fn a_single_point_intersection_is_the_tightest_enclosure_not_a_contradiction() {
        let bx = Vec3Interval {
            // ⚠️ 3 軸すべて **非対称** に取る — 対称区間 (`[-a, a]`) だと
            // `hi - lo` を `hi / lo` に変えても `-1` になり、**二乗で符号が消えて
            // 辺長の二乗が変わらない**ので変異が見えなくなる (2026-10-01 実測、
            // `z: [-0.5, 0.5]` で `law.rs:1094:71` の `-` → `/` を殺せなかった)
            // 辺長は (2, 2, 1) を保つので半対角は `0.5·√9 = 1.5` で f32 厳密
            x: Interval { lo: 0.5, hi: 2.5 },
            y: Interval { lo: 1.0, hi: 3.0 },
            z: Interval { lo: 0.25, hi: 1.25 },
        };
        // Lipschitz 包囲は [7, 13] (外側丸めで両端が 1 ulp 広がる)
        // 区間側を [10, 10] にすると交差は 1 点 10 になる
        let iv = Interval { lo: 10.0, hi: 10.0 };
        match refine_with_lipschitz(bx, iv, 10.0, 2.0) {
            Refined::Enclosure(r) => assert!(
                (r.lo - 10.0).abs() < 1e-3 && (r.hi - 10.0).abs() < 1e-3,
                "1 点交差が [{}, {}] になった",
                r.lo,
                r.hi
            ),
            Refined::Contradiction => panic!("1 点で接する交差を矛盾として扱った"),
        }
    }

    /// 交差が空なら矛盾として報告する (どちらも信じない)
    ///
    /// oracle: `[100, 200]` と Lipschitz 包囲 `[7, 13]` は重ならない
    /// 健全な包囲同士なら同じ `f(box)` を両方が含むので空にならないので、
    /// 空になった時点でどちらかが健全でない
    ///
    /// ⚠️ `lo > hi` を `==` に変えるとここが [`Refined::Enclosure`] になり、
    /// **`lo > hi` の壊れた区間を下流に渡す**
    #[test]
    fn an_empty_intersection_is_reported_as_a_contradiction() {
        let bx = Vec3Interval {
            // ⚠️ 3 軸すべて **非対称** に取る — 対称区間 (`[-a, a]`) だと
            // `hi - lo` を `hi / lo` に変えても `-1` になり、**二乗で符号が消えて
            // 辺長の二乗が変わらない**ので変異が見えなくなる (2026-10-01 実測、
            // `z: [-0.5, 0.5]` で `law.rs:1094:71` の `-` → `/` を殺せなかった)
            // 辺長は (2, 2, 1) を保つので半対角は `0.5·√9 = 1.5` で f32 厳密
            x: Interval { lo: 0.5, hi: 2.5 },
            y: Interval { lo: 1.0, hi: 3.0 },
            z: Interval { lo: 0.25, hi: 1.25 },
        };
        let iv = Interval {
            lo: 100.0,
            hi: 200.0,
        };
        assert!(
            matches!(
                refine_with_lipschitz(bx, iv, 10.0, 2.0),
                Refined::Contradiction
            ),
            "重ならない 2 包囲を矛盾として扱わなかった"
        );
    }

    /// Lipschitz を主張できない形では区間をそのまま返す
    ///
    /// oracle: 契約外 (`f(c) < 0`) では包囲が出ないので、締める材料が無い
    /// ⚠️ ここで区間を捨てると第 1 の経路の決着も失う
    #[test]
    fn without_a_lipschitz_bound_the_interval_passes_through_unchanged() {
        let bx = Vec3Interval {
            // ⚠️ 3 軸すべて **非対称** に取る — 対称区間 (`[-a, a]`) だと
            // `hi - lo` を `hi / lo` に変えても `-1` になり、**二乗で符号が消えて
            // 辺長の二乗が変わらない**ので変異が見えなくなる (2026-10-01 実測、
            // `z: [-0.5, 0.5]` で `law.rs:1094:71` の `-` → `/` を殺せなかった)
            // 辺長は (2, 2, 1) を保つので半対角は `0.5·√9 = 1.5` で f32 厳密
            x: Interval { lo: 0.5, hi: 2.5 },
            y: Interval { lo: 1.0, hi: 3.0 },
            z: Interval { lo: 0.25, hi: 1.25 },
        };
        let iv = Interval { lo: -5.0, hi: 5.0 };
        match refine_with_lipschitz(bx, iv, -0.1, 2.0) {
            Refined::Enclosure(r) => {
                assert!(
                    (r.lo - iv.lo).abs() < 1e-6 && (r.hi - iv.hi).abs() < 1e-6,
                    "契約外なのに区間が変わった [{}, {}]",
                    r.lo,
                    r.hi
                );
            }
            Refined::Contradiction => panic!("締める材料が無い時に矛盾を報告した"),
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 根拠 gate (0.5.0) — 「証明なしの Hard 違反」が復活していないことの機械検査
//
// `Constraint::evidence_class` は手で書いた表なので、**実装が実際に返す
// `Violation::evidence` と食い違っても誰も気付かない** ここが drift すると
// gate は形だけ残って意味を失うので、両者を突き合わせる test を置く
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
#[cfg(test)]
mod evidence_gate_tests {
    use super::*;

    /// 全 10 variant を 1 つずつ (網羅は `all_variants_are_classified` で固定)
    fn every_constraint() -> Vec<Constraint> {
        let n = || SdfNode::sphere(1.0);
        vec![
            Constraint::NonOverlap { a: n(), b: n() },
            Constraint::Containment {
                inner: n(),
                outer: n(),
            },
            Constraint::MinThickness {
                node: n(),
                min_thickness: 0.5,
            },
            Constraint::Stress {
                node: n(),
                load_points: vec![(Vec3::ZERO, 1.0)],
                min_thickness_factor: 0.2,
            },
            Constraint::Thermal {
                node: n(),
                heat_sources: vec![Vec3::ZERO],
                search_radius: 1.0,
                min_surface_ratio: 0.8,
            },
            Constraint::Contact {
                a: n(),
                b: n(),
                min_distance: 0.1,
                max_distance: 1.0,
            },
            Constraint::Continuity {
                node: n(),
                seed_point: Vec3::ZERO,
            },
            Constraint::GradientBound {
                node: n(),
                max_gradient: 1.0,
                probe: 0.01,
            },
            Constraint::Reachable {
                node: n(),
                from: Vec3::ZERO,
                to: Vec3::ZERO,
            },
            Constraint::VolumeConservation {
                before: n(),
                after: n(),
                relative_tolerance: 0.05,
            },
        ]
    }

    /// variant を足したら必ずここに落ちる (`name` の網羅が抜けを教える)
    #[test]
    fn all_variants_are_classified() {
        assert_eq!(
            every_constraint().len(),
            10,
            "variant を足したら every_constraint にも足す"
        );
        let modelled: Vec<&str> = every_constraint()
            .iter()
            .filter(|c| !c.evidence_class().is_proof())
            .map(Constraint::name)
            .collect();
        assert_eq!(
            modelled,
            ["Stress", "Thermal", "Continuity", "VolumeConservation"],
            "モデル推定の顔ぶれが変わった — 意図した変更なら CHANGELOG に書く"
        );
    }

    /// モデル推定は `Law::hard` を通らない / 証明・反例つきは通る
    #[test]
    fn hard_is_gated_by_evidence_class() {
        for c in every_constraint() {
            let (name, class) = (c.name(), c.evidence_class());
            let got = Law::hard("x", c);
            assert_eq!(
                got.is_ok(),
                class.is_proof(),
                "{name} の gate が evidence_class と食い違う"
            );
            if let Err(e) = got {
                assert_eq!(e.constraint, name);
                assert_eq!(Some(e.model), class.model(), "{name} の model 説明が違う");
                assert!(
                    e.to_string().contains("Law::soft"),
                    "{name}: 対処法が message に無い"
                );
            }
        }
    }

    /// モデル推定の制約は `Priority::Hard` の違反を出せない
    ///
    /// これが 0.5.0 の要点 — gate を外すとここが落ちる
    #[test]
    fn modelled_constraints_cannot_produce_hard_violations() {
        // 離れた 2 球 = Continuity / VolumeConservation が違反を出す形
        let split = SdfNode::sphere(0.8).union(SdfNode::sphere(0.8).translate(4.0, 0.0, 0.0));
        let config = CheckConfig {
            aabb_min: Vec3::new(-2.0, -2.0, -2.0),
            aabb_max: Vec3::new(6.0, 2.0, 2.0),
            resolution: 16,
        };
        let report = LawSet::new()
            .continuity("split", 1.0, split.clone(), Vec3::ZERO)
            .volume_conservation("shrunk", 1.0, split, SdfNode::sphere(0.8), 0.05)
            .check(&config);

        assert!(
            !report.violations.is_empty(),
            "検査対象が違反を出していない (test が無意味になっている)"
        );
        assert!(
            !report.has_hard_violations(),
            "モデル推定が Hard 違反として出た\n{}",
            format_report(&report)
        );
        assert!(
            report.proven_violations().is_empty(),
            "モデル推定が proven_violations に混ざった"
        );
        for v in &report.violations {
            assert!(
                v.evidence.model().is_some(),
                "{} の違反が Modelled でない",
                v.law_name
            );
        }
    }

    /// 実装が返す `Violation::evidence` が `evidence_class` より強くなっていない
    ///
    /// class は「返せる最強の根拠」なので、実際の違反がそれを **超えて**
    /// いたら表が古い (逆に弱いのは Contact の干渉側などで正常)
    #[test]
    fn reported_evidence_never_exceeds_the_declared_class() {
        let config = CheckConfig {
            aabb_min: Vec3::splat(-2.0),
            aabb_max: Vec3::splat(2.0),
            resolution: 8,
        };
        // 重なる 2 球 / 薄い板 / 潰れた熱源 — 各法則が違反を出す形を 1 つずつ
        let overlap = SdfNode::sphere(1.0);
        let cases: Vec<Law> = vec![
            Law::hard(
                "overlap",
                Constraint::NonOverlap {
                    a: overlap.clone(),
                    b: overlap.clone(),
                },
            )
            .expect("provable constraint"),
            Law::soft(
                "heat",
                1.0,
                Constraint::Thermal {
                    node: overlap.clone(),
                    heat_sources: vec![Vec3::ZERO],
                    search_radius: 1.5,
                    min_surface_ratio: 0.99,
                },
            ),
            Law::soft(
                "load",
                1.0,
                Constraint::Stress {
                    node: overlap,
                    load_points: vec![(Vec3::ZERO, 5.0)],
                    min_thickness_factor: 0.5,
                },
            ),
        ];
        let classes: Vec<(String, Evidence)> = cases
            .iter()
            .map(|l| (l.name.clone(), l.constraint.evidence_class()))
            .collect();
        let report = check_laws(&cases, &config);
        assert!(
            !report.violations.is_empty(),
            "どの法則も違反を出さなかった"
        );

        for v in &report.violations {
            let (_, class) = classes
                .iter()
                .find(|(n, _)| *n == v.law_name)
                .expect("報告された法則名が入力に無い");
            assert!(
                !v.evidence.is_proof() || class.is_proof(),
                "{}: 実際の根拠 {:?} が申告した class {:?} より強い — \
                 evidence_class の表が実装から遅れている",
                v.law_name,
                v.evidence,
                class
            );
        }
    }

    /// 表面帯の判定基準が **場の値に依存していない** こと (0.5.0)
    ///
    /// `|f(center)| < step` で帯を取ると、場が真の距離を過大申告する node
    /// (TPMS は 1.7〜7.0 倍) では帯がその分痩せ、**表面を含むセルを帯から
    /// 落とす** = `surface_hi` が上界でなくなる
    ///
    /// `analytic_law::thermal_surface_ratio_must_not_shrink_with_the_field_scale`
    /// は三値化の側を pin していて、**この基準の差までは捕まえられなかった**
    /// (`|f| < step` に戻しても比の上界が閾値を上回るので verdict が動かない、
    /// 2026-09-28 実測) ので、基準そのものを白箱で固定する
    ///
    /// scene は「区間演算で内部と確定するセルが多い」(= 比の上界が発散しない)
    /// かつ「場の帯が表面セルを取りこぼす」を満たす必要があり、薄い
    /// `gyroid(1.0, 0.3)` では内部確定セルが 0 になって測れない
    #[test]
    fn surface_band_does_not_depend_on_the_field_scale() {
        let node = SdfNode::gyroid(0.6, 1.2);
        let config = CheckConfig {
            aabb_min: Vec3::splat(-2.0),
            aabb_max: Vec3::splat(2.0),
            resolution: 24,
        };
        let (src, radius) = (Vec3::ZERO, 1.5_f32);
        let step = grid_step(&config);
        let signs = classify_cells(&node, &config, step);

        let (mut straddling, mut missed_by_field, mut inside) = (0usize, 0usize, 0usize);
        for (ix, iy, iz, idx) in grid_indices(config.resolution) {
            let center = cell_center(&config, step, ix, iy, iz);
            if center.distance(src) > radius {
                continue;
            }
            if signs[idx] == CellSign::Inside {
                inside += 1;
            }
            let region = cell_box(&config, step, ix, iy, iz);
            if !cell_straddles_surface(&node, region) {
                continue;
            }
            straddling += 1;
            // 表面を含むセルは区間演算では必ず未定 = 帯が上界であることの担保
            assert_eq!(
                signs[idx],
                CellSign::Undecided,
                "表面を含むセルが内外確定になった (帯が上界でない)"
            );
            if sdf_eval(&node, center).abs() >= step.x {
                missed_by_field += 1;
            }
        }

        assert!(
            straddling > 0 && inside > 0,
            "検査対象が空 (straddling={straddling} inside={inside})"
        );
        assert!(
            missed_by_field > 0,
            "場の帯と区間の帯が一致してしまい基準の差を測れない — \
             場が過大申告する node に変える"
        );

        // `check_thermal` が実際にその帯を使っていることを比の上界経由で固定
        // 旧基準に戻すと `surface_hi` が straddling を下回り、ここが落ちる
        let (_, hi, _) = surface_ratio_bracket(&node, src, radius, &config, &signs)
            .expect("内部確定セルがあるので bracket が取れる");
        #[allow(clippy::cast_precision_loss)]
        let needed = straddling as f32 / inside as f32;
        assert!(
            hi >= needed,
            "表面比の上界 {hi:.4} が、表面を含むと確定したセル分 {needed:.4} に \
             届いていない = 帯が場の値で痩せている \
             (straddling={straddling} inside={inside} missed_by_field={missed_by_field})"
        );
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // ThermalField の物理 oracle (`physics` feature)
    //
    // 期待値は**閉形式**から出す (実装の出力を pin しない):
    // エネルギー保存 / 境界条件の単調性 / 熱源ゼロの恒等性
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// 温度依存のない材料 (エネルギー収支を閉形式で書けるようにする)
    ///
    /// α が一様なら 7 点 Laplacian は Σ T を厳密に保存するので、
    /// 断熱系では「入れた熱 = 上がった温度 × 熱容量」が丸め誤差の範囲で成り立つ
    #[cfg(feature = "physics")]
    fn constant_material(
        k: f32,
        cp: f32,
        rho: f32,
    ) -> alice_physics::transient_thermal::ThermalMaterial {
        use alice_physics::transient_thermal::{TemperatureDependence as Dep, ThermalMaterial};
        ThermalMaterial {
            name: "oracle_constant",
            conductivity: Dep::Constant(k),
            specific_heat: Dep::Constant(cp),
            density: Dep::Constant(rho),
            reference_temperature: 293.15,
        }
    }

    /// 検査範囲を丸ごと材料で埋めた立方格子 (外側セルが無い = 純粋な断熱系)
    #[cfg(feature = "physics")]
    fn filled_cube(n: usize, half: f32) -> (CheckConfig, Vec<bool>) {
        let config = CheckConfig {
            aabb_min: Vec3::splat(-half),
            aabb_max: Vec3::splat(half),
            resolution: n,
        };
        (config, vec![true; n * n * n])
    }

    /// 断熱系は **入れた熱をすべて保持する** — 最高温度がエネルギー収支で挟まれる
    ///
    /// 熱源 `P` を `t` 秒入れた時、系の内部エネルギーは厳密に `P·t` 増える
    /// (断熱 = 流出なし)。平均温度上昇は `ΔT̄ = P·t / (ρ·cp·V)` で、最高温度は
    ///
    /// - 平均以上 (最大 ≥ 平均)
    /// - 全部が 1 セルに溜まった場合以下 (`P·t / (ρ·cp·V_cell)`)
    ///
    /// の間に必ず入る。さらに **時間を延ばすほど場は均され**、最高温度は
    /// 平均に寄る — 絶対値の勘ではなく「比が単調に 1 へ近づく」で見る
    /// (拡散長 √(αt) が領域寸法に届くかは材料と時間の組合せ次第なので、
    /// 決め打ちの係数を書くと oracle が実装でなく私の見積もりを測る)
    #[cfg(feature = "physics")]
    #[test]
    fn adiabatic_peak_is_bounded_by_the_energy_balance() {
        let (k, cp, rho) = (200.0_f32, 900.0_f32, 2700.0_f32); // Al 相当の定数近似
        let material = constant_material(k, cp, rho);
        let n = 9;
        let half = 0.5_f32; // 検査範囲 1 m 角 (metres_per_unit = 1)
        let (config, solid) = filled_cube(n, half);
        let (watts, ambient) = (50.0_f32, 20.0_f32);

        let volume = (2.0 * half).powi(3);
        #[allow(clippy::cast_precision_loss)]
        let cell_volume = volume / (n * n * n) as f32;

        let peak_of = |duration: f32| {
            let scene = ThermalScene {
                solid: &solid,
                sources: &[(Vec3::ZERO, watts)],
                material: &material,
                metres_per_unit: 1.0,
                ambient_c: ambient,
                duration_s: duration,
            };
            solve_peak_temperature(&scene, SurfaceCooling::Adiabatic, &config).expect("解けるはず")
        };

        let mut previous_ratio = f32::INFINITY;
        let mut excess = Vec::new();
        for duration in [200.0_f32, 1000.0, 5000.0] {
            let peak = peak_of(duration);
            let mean_rise = watts * duration / (rho * cp * volume);
            let single_cell_rise = watts * duration / (rho * cp * cell_volume);

            // (1) 断熱系は熱を失わない
            assert!(
                peak >= ambient + mean_rise * 0.999,
                "t={duration}: 最高温度 {peak:.4} が平均 {:.4} 未満 = 熱が消えている",
                ambient + mean_rise
            );
            // (2) 熱は湧かない (全部 1 セルに溜まった場合が上限)
            assert!(
                peak <= ambient + single_cell_rise,
                "t={duration}: 最高温度 {peak:.4} が 1 セル集中の上限 {:.4} 超 = 熱が湧いている",
                ambient + single_cell_rise
            );
            // (3) 平均に対する比は時間とともに 1 へ向かう
            let ratio = (peak - ambient) / mean_rise;
            assert!(
                ratio < previous_ratio,
                "t={duration}: 最高/平均 の比 {ratio:.3} が前回 {previous_ratio:.3} から \
                 下がらない = 拡散が進んでいない"
            );
            previous_ratio = ratio;
            excess.push(peak - ambient - mean_rise);
        }

        // (4) 熱源セルの **超過温度は定常値に収束する** (平均は時間に比例して
        //     上がるが、熱源と周囲の温度差は一定に落ち着く)
        let (d1, d2) = ((excess[1] - excess[0]).abs(), (excess[2] - excess[1]).abs());
        assert!(
            d2 * 5.0 < d1,
            "超過温度が収束しない (差 {d1:.6} → {d2:.6}) = 定常状態に入っていない"
        );

        // (5) その定常超過は **熱伝導率で決まる**: 熱源セルが P を 6 面から
        //     逃がすので ΔT ≈ P / (6·k·dx) 離散 stencil の幾何係数の分だけ
        //     連続体の見積もりとずれるが、桁と k 依存性はここで固定される
        //     (k を 2 倍 / 半分にすると範囲から外れる)
        #[allow(clippy::cast_precision_loss)]
        let dx = 2.0 * half / n as f32;
        let continuum = watts / (6.0 * k * dx);
        let rel = excess[2] / continuum;
        assert!(
            (0.6..=1.4).contains(&rel),
            "定常超過 {:.4} が連続体の見積もり {continuum:.4} の {rel:.3} 倍 = \
             熱伝導率か単位の扱いがずれている",
            excess[2]
        );
    }

    /// 熱源が無ければ温度は周囲のまま (定数場の Laplacian は 0、厳密)
    #[cfg(feature = "physics")]
    #[test]
    fn no_source_keeps_the_field_at_ambient() {
        let material = constant_material(200.0, 900.0, 2700.0);
        let (config, solid) = filled_cube(9, 0.5);
        for cooling in [SurfaceCooling::Adiabatic, SurfaceCooling::Isothermal] {
            let scene = ThermalScene {
                solid: &solid,
                sources: &[],
                material: &material,
                metres_per_unit: 1.0,
                ambient_c: 20.0,
                duration_s: 100.0,
            };
            let peak = solve_peak_temperature(&scene, cooling, &config).expect("解けるはず");
            assert!(
                (peak - 20.0).abs() < 1.0e-3,
                "熱源なしで温度が {peak:.6} に動いた"
            );
        }
    }

    /// 冷却が強いほど最高温度は下がる — 両端の順序が壊れたら bracket が嘘になる
    #[cfg(feature = "physics")]
    #[test]
    fn isothermal_never_exceeds_adiabatic() {
        let material = constant_material(200.0, 900.0, 2700.0);
        let n = 9;
        let config = CheckConfig {
            aabb_min: Vec3::splat(-0.5),
            aabb_max: Vec3::splat(0.5),
            resolution: n,
        };
        // 箱の中に球 = 外側セルがあるので境界条件が効く
        let signs = classify_cells(&SdfNode::sphere(0.35), &config, grid_step(&config));
        let solid: Vec<bool> = signs.iter().map(|s| *s != CellSign::Outside).collect();
        assert!(solid.iter().any(|b| *b), "材料セルが無い");
        assert!(
            solid.iter().any(|b| !*b),
            "外側セルが無い (境界条件が効かない)"
        );

        let solve = |c| {
            let scene = ThermalScene {
                solid: &solid,
                sources: &[(Vec3::ZERO, 50.0)],
                material: &material,
                metres_per_unit: 1.0,
                ambient_c: 20.0,
                duration_s: 200.0,
            };
            solve_peak_temperature(&scene, c, &config).expect("解けるはず")
        };
        let (hot, cold) = (
            solve(SurfaceCooling::Adiabatic),
            solve(SurfaceCooling::Isothermal),
        );
        assert!(
            cold <= hot,
            "等温 (h = ∞) の最高温度 {cold:.4} が断熱 (h = 0) の {hot:.4} を超えた \
             = 上下界が逆転している"
        );
        assert!(
            cold < hot,
            "境界条件を変えても最高温度が動かない ({cold:.4}) = 表面が効いていない"
        );
    }

    /// 球を箱に入れた scene (外側セルがあり境界条件が効く)
    #[cfg(feature = "physics")]
    fn sphere_in_box(n: usize) -> (CheckConfig, Vec<bool>) {
        let config = CheckConfig {
            aabb_min: Vec3::splat(-0.5),
            aabb_max: Vec3::splat(0.5),
            resolution: n,
        };
        let signs = classify_cells(&SdfNode::sphere(0.35), &config, grid_step(&config));
        let solid: Vec<bool> = signs.iter().map(|s| *s != CellSign::Outside).collect();
        assert!(solid.iter().any(|b| *b), "材料セルが無い");
        assert!(solid.iter().any(|b| !*b), "外側セルが無い");
        (config, solid)
    }

    /// **等温境界は定常状態を持ち、断熱境界は持たない**
    ///
    /// `h = ∞` では表面から出る熱が発熱と釣り合った時点で温度が止まる。
    /// `h = 0` では熱が一切逃げないので平均温度は時間に比例して上がり続ける。
    /// この差が bracket の下界を「下界」たらしめている性質で、
    /// [`isothermal_never_exceeds_adiabatic`] の大小比較では捕まらない
    /// (外側セルを素通しにしても大小は保たれてしまう、2026-09-29 実測)
    #[cfg(feature = "physics")]
    #[test]
    fn isothermal_saturates_but_adiabatic_keeps_rising() {
        let material = constant_material(200.0, 900.0, 2700.0);
        let (config, solid) = sphere_in_box(9);
        let rise = |cooling, duration| {
            let scene = ThermalScene {
                solid: &solid,
                sources: &[(Vec3::ZERO, 50.0)],
                material: &material,
                metres_per_unit: 1.0,
                ambient_c: 20.0,
                duration_s: duration,
            };
            solve_peak_temperature(&scene, cooling, &config).expect("解けるはず") - 20.0
        };

        // 拡散時間 L²/α ≈ 0.35²/8.2e-5 ≈ 1500 s なので 3000 s で定常に入る
        let iso_short = rise(SurfaceCooling::Isothermal, 3000.0);
        let iso_long = rise(SurfaceCooling::Isothermal, 30000.0);
        let adi_short = rise(SurfaceCooling::Adiabatic, 3000.0);
        let adi_long = rise(SurfaceCooling::Adiabatic, 30000.0);

        assert!(
            iso_long < iso_short * 1.2,
            "等温境界で時間を 10 倍にしたら温度上昇が {iso_short:.4} → {iso_long:.4} \
             (1.2 倍超) = 定常に入っていない = 表面が周囲温度に固定されていない"
        );
        assert!(
            adi_long > adi_short * 3.0,
            "断熱境界で時間を 10 倍にしても温度上昇が {adi_short:.4} → {adi_long:.4} \
             (3 倍未満) = 熱が逃げている"
        );
    }

    /// 材料の増減に対する最高温度の向き — bracket の両端が最小材料に来る根拠
    ///
    /// **この 2 本が設計の前提そのもの**。向きが逆だったり動かなかったりしたら
    /// `Inside` で挟む方式が成立しないので、実装より先に実測する。
    ///
    /// - 断熱: 材料を増やす = 同じ熱量をより多い質量が吸う → **下がる**
    /// - 等温: 材料を増やす = 周囲温度に固定される面が熱源から遠のく → **上がる**
    #[cfg(feature = "physics")]
    #[test]
    fn material_moves_the_peak_in_opposite_directions_per_boundary() {
        let material = constant_material(200.0, 900.0, 2700.0);
        let config = CheckConfig {
            aabb_min: Vec3::splat(-0.5),
            aabb_max: Vec3::splat(0.5),
            resolution: 9,
        };
        let signs = classify_cells(&SdfNode::sphere(0.35), &config, grid_step(&config));
        let strict: Vec<bool> = signs.iter().map(|s| *s == CellSign::Inside).collect();
        let loose: Vec<bool> = signs.iter().map(|s| *s != CellSign::Outside).collect();
        let extra = (0..signs.len()).filter(|&i| loose[i] && !strict[i]).count();
        assert!(
            strict.iter().any(|b| *b) && extra > 0,
            "内部確定セルと未確定セルが両方要る (strict={} extra={extra})",
            strict.iter().filter(|b| **b).count()
        );

        let sources = [(Vec3::ZERO, 50.0)];
        let peak = |solid: &[bool], cooling| {
            let scene = ThermalScene {
                solid,
                sources: &sources,
                material: &material,
                metres_per_unit: 1.0,
                ambient_c: 20.0,
                duration_s: 2000.0,
            };
            solve_peak_temperature(&scene, cooling, &config).expect("解けるはず")
        };

        let adi_small = peak(&strict, SurfaceCooling::Adiabatic);
        let adi_big = peak(&loose, SurfaceCooling::Adiabatic);
        assert!(
            adi_big < adi_small,
            "断熱で材料を増やしたのに最高温度が下がらない ({adi_small:.4} → {adi_big:.4}) \
             = 上界を最小材料に置く根拠が崩れる"
        );

        let iso_small = peak(&strict, SurfaceCooling::Isothermal);
        let iso_big = peak(&loose, SurfaceCooling::Isothermal);
        assert!(
            iso_big > iso_small,
            "等温で材料を増やしたのに最高温度が上がらない ({iso_small:.4} → {iso_big:.4}) \
             = 下界を最小材料に置く根拠が崩れる"
        );
    }

    /// `Undecided` セルの任意の部分集合を材料に足しても bracket の内側に入る
    ///
    /// 真の材料 `M` は `Inside ⊆ M ⊆ Inside ∪ Undecided` のどれかなので、
    /// **その範囲の `M` を実際に何通りも作って**、どれも
    /// `[T_等温(Inside), T_断熱(Inside)]` に収まることを見る。
    /// 形状 (半径) を変えるのではなく `Undecided` の部分集合を直接作るのが要点で、
    /// 範囲を外れた形状を混ぜると「入らなくて当然」になり oracle が嘘の red を出す。
    #[cfg(feature = "physics")]
    #[test]
    fn bracket_contains_every_admissible_material_set() {
        let material = constant_material(200.0, 900.0, 2700.0);
        let config = CheckConfig {
            aabb_min: Vec3::splat(-0.5),
            aabb_max: Vec3::splat(0.5),
            resolution: 9,
        };
        let signs = classify_cells(&SdfNode::sphere(0.35), &config, grid_step(&config));
        let strict: Vec<bool> = signs.iter().map(|s| *s == CellSign::Inside).collect();
        let undecided: Vec<usize> = (0..signs.len())
            .filter(|&i| signs[i] == CellSign::Undecided)
            .collect();
        assert!(!undecided.is_empty(), "未確定セルが無いと挟む意味が無い");

        let sources = [(Vec3::ZERO, 50.0)];
        let peak = |solid: &[bool], cooling| {
            let scene = ThermalScene {
                solid,
                sources: &sources,
                material: &material,
                metres_per_unit: 1.0,
                ambient_c: 20.0,
                duration_s: 2000.0,
            };
            solve_peak_temperature(&scene, cooling, &config).expect("解けるはず")
        };

        // bracket は **法則が報告した値** を読む (solver を直接呼ぶと
        // `check_thermal_field` がどの形状を選んでいるかを見ないので、
        // 形状選択を戻しても落ちない test になってしまう)
        let probe = peak(&strict, SurfaceCooling::Adiabatic)
            .midpoint(peak(&strict, SurfaceCooling::Isothermal));
        let report = check_laws(
            &[Law::soft(
                "heat",
                1.0,
                Constraint::ThermalField {
                    node: SdfNode::sphere(0.35),
                    sources: vec![(Vec3::ZERO, 50.0)],
                    material,
                    metres_per_unit: 1.0,
                    ambient_c: 20.0,
                    max_temperature_c: probe,
                    duration_s: 2000.0,
                },
            )],
            &config,
        );
        let (lo, hi) = match report.unresolved.first().map(|u| &u.reason) {
            Some(UnresolvedReason::TemperatureUnbracketed { lo_c, hi_c }) => (*lo_c, *hi_c),
            other => panic!("bracket の中点なら未定になるはず (実際 {other:?} / {report:?})"),
        };
        assert!(lo < hi, "bracket が潰れている ({lo:.4}, {hi:.4})");

        // 未確定セルを 0 / 25 / 50 / 75 / 100 % 採用した材料集合を作る
        // (決定論的に間引く — 乱数を使うと落ちた時に再現できない)
        for numerator in [0_usize, 1, 2, 3, 4] {
            let mut m = strict.clone();
            for (rank, &idx) in undecided.iter().enumerate() {
                if rank * 4 < numerator * undecided.len() {
                    m[idx] = true;
                }
            }
            for cooling in [SurfaceCooling::Adiabatic, SurfaceCooling::Isothermal] {
                let t = peak(&m, cooling);
                assert!(
                    (lo..=hi).contains(&t),
                    "未確定セル {numerator}/4 採用 / {cooling:?} の最高温度 {t:.4} が \
                     bracket [{lo:.4}, {hi:.4}] の外 = 挟めていない"
                );
            }
        }
    }

    /// 同じ入力は同じ温度場を返す (f32 の + - * / のみなので bit 一致するはず)
    #[cfg(feature = "physics")]
    #[test]
    fn thermal_solve_is_bit_reproducible() {
        let material = constant_material(200.0, 900.0, 2700.0);
        let (config, solid) = filled_cube(9, 0.5);
        let run = || {
            let scene = ThermalScene {
                solid: &solid,
                sources: &[(Vec3::new(0.1, -0.2, 0.05), 50.0)],
                material: &material,
                metres_per_unit: 1.0,
                ambient_c: 20.0,
                duration_s: 150.0,
            };
            solve_peak_temperature(&scene, SurfaceCooling::Adiabatic, &config).expect("解けるはず")
        };
        assert_eq!(run().to_bits(), run().to_bits(), "同じ入力で結果が揺れた");
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 応力特異点の計測 — Williams の条件が占有率で測れることの実証
    //
    // 判定器 (走査層) は未実装なので、ここで固定するのは **計測の物理** だけ
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// L 字の 2 腕 (稜線は z 軸に平行、材料側の内角は 3π/2)
    ///
    /// `arm_x` は y ∈ \[−0.3, 0\]、`arm_y` は x ∈ \[−0.3, 0\] なので、
    /// 空気側の象限 (x < −0.3 かつ y < −0.3) が π/2 = 材料 3π/2
    /// 入隅の稜線は (−0.3, −0.3) 上
    fn l_arms() -> (SdfNode, SdfNode) {
        (
            SdfNode::box3d_half_extents(0.5, 0.15, 1.0).translate(-0.5, -0.15, 0.0),
            SdfNode::box3d_half_extents(0.15, 0.5, 1.0).translate(-0.15, -0.5, 0.0),
        )
    }

    /// 鋭い入隅の占有率は **尺度を変えても動かない** = `α / 2π` に等しい
    ///
    /// これが「閾値でなく尺度不変性で判定する」の根拠で、Williams の特異条件
    /// (`α > π`) が占有率で測れることの実証
    #[test]
    fn solid_angle_fraction_is_scale_invariant_at_a_sharp_corner() {
        let (arm_x, arm_y) = l_arms();
        let node = arm_x.union(arm_y);
        let corner = Vec3::new(-0.3, -0.3, 0.0);

        let seen: Vec<f32> = [0.26_f32, 0.13, 0.065, 0.032]
            .iter()
            .map(|&r| material_fraction(&node, corner, r))
            .collect();

        for f in &seen {
            assert!(
                (0.72..=0.78).contains(f),
                "占有率 {f:.3} が内角 3π/2 の α/2π = 0.75 近傍にない: {seen:?}"
            );
        }
        let spread = seen.iter().copied().fold(f32::MIN, f32::max)
            - seen.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            spread < 0.02,
            "尺度を 8 倍変えて占有率が {spread:.3} 動いた = 尺度不変でない: {seen:?}"
        );
    }

    /// 曲率有限の面では 0.5 からの隔たりが **半径に比例して縮む**
    ///
    /// 鋭い入隅 (比 1.0) との差がそのまま判定基準になる
    /// 実測 (2026-09-29): `smooth_union` のフィレット面で比 0.50〜0.63 (理論 0.5)
    #[test]
    fn solid_angle_fraction_decays_with_radius_on_a_fillet() {
        let (arm_x, arm_y) = l_arms();
        let node = arm_x.smooth_union(arm_y, 0.35);
        // フィレット面上の点へ Newton 射影 (空気側の bisector から入る)
        let mut surf = Vec3::new(-0.45, -0.45, 0.0);
        for _ in 0..12 {
            let value = sdf_eval(&node, surf);
            let grad = alice_sdf::eval::eval_gradient(&node, surf);
            let norm_sq = grad.length_squared();
            assert!(norm_sq > 1.0e-12, "勾配が消えた = 射影できない");
            surf -= grad * (value / norm_sq);
        }
        assert!(
            sdf_eval(&node, surf).abs() < 1.0e-3,
            "フィレット面へ射影できていない (f = {})",
            sdf_eval(&node, surf)
        );

        for radius in [0.26_f32, 0.173, 0.13] {
            let d_far = material_fraction(&node, surf, radius) - 0.5;
            let d_near = material_fraction(&node, surf, radius * 0.5) - 0.5;
            assert!(
                d_far > 0.01,
                "r={radius}: フィレット面なのに隔たり {d_far:.3} が小さすぎて比を測れない"
            );
            let ratio = d_near / d_far;
            assert!(
                (0.35..=0.75).contains(&ratio),
                "r={radius}: 比 {ratio:.3} が理論値 0.5 の近傍にない \
                 (鋭い入隅なら 1.0 になるので、0.75 以上だと区別が付かない)"
            );
        }
    }

    /// `LawSet` の convenience が Hard を作り直していないか
    #[test]
    fn lawset_convenience_keeps_modelled_laws_soft() {
        let n = || SdfNode::sphere(1.0);
        let set = LawSet::new()
            .stress("s", 0.5, n(), vec![(Vec3::ZERO, 1.0)], 0.2)
            .thermal("t", 0.5, n(), vec![Vec3::ZERO], 1.0, 0.8)
            .continuity("c", 0.5, n(), Vec3::ZERO)
            .volume_conservation("v", 0.5, n(), n(), 0.05);
        assert_eq!(set.laws().len(), 4);
        for law in set.laws() {
            assert_eq!(
                law.priority,
                Priority::Soft(0.5),
                "{} が Hard で積まれた",
                law.name
            );
        }

        // 証明・反例つきの 3 つは Hard のまま
        let provable = LawSet::new()
            .contact("k", n(), n(), 0.1, 1.0)
            .gradient_bound("g", n(), 1.0, 0.01)
            .reachable("r", n(), Vec3::ZERO, Vec3::ZERO);
        for law in provable.laws() {
            assert_eq!(law.priority, Priority::Hard, "{} が降格した", law.name);
        }
    }
}
