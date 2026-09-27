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

use alice_sdf::interval::{Interval, Vec3Interval};
use alice_sdf::SdfNode;
use glam::Vec3;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 型定義
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 制約の優先度
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Priority {
    /// 絶対不可侵 — 違反はエラー
    Hard,
    /// エネルギー最小化 — 違反は警告 + 残差で重み付け
    Soft(f32),
}

/// 制約の種類
///
/// A.1.0 (2026-08-06) 時点で 3 variant (`NonOverlap` / Containment / `MinThickness`)
/// A.2 (2026-08-06) で 5 variant 追加 (Stress / Thermal / Contact / Continuity / `VolumeConservation`)
/// 新 5 variant は geometric proxy 評価 (grid + `sdf_eval)、精密` physics-backed 評価は A.2.1 で追加予定
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
}

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
    /// ハード制約の法則を作成
    #[must_use]
    pub fn hard(name: impl Into<String>, constraint: Constraint) -> Self {
        Self {
            name: name.into(),
            priority: Priority::Hard,
            constraint,
        }
    }

    /// ソフト制約の法則を作成（weight: 0.0〜1.0）
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
    #[must_use]
    pub fn has_hard_violations(&self) -> bool {
        self.violations.iter().any(|v| v.priority == Priority::Hard)
    }

    /// 判定不能の法則があるか
    #[must_use]
    pub const fn has_unresolved(&self) -> bool {
        !self.unresolved.is_empty()
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

        match interval_sign(sdf_interval(node, bx)) {
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
/// 証拠: 点で両方負 (residual = 浅い方の侵入深さ、点評価なので exact)
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
            &|fa, fb| fa.max(fb), // 浅い方の侵入深さ (点評価なので exact)
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

    pair_verdict(law_name, priority, worst, undecided)
}

/// Containment: inner が内部（< 0）かつ outer が外部（> 0）の点があればはみ出し
///
/// 証明: 箱で `inner ≥ 0` (inner が無い) or `outer ≤ 0` (outer の中) が一様
/// 証拠: 点で `inner < 0 && outer > 0` (residual = −outer、点評価なので exact)
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
            &|_, fo| -fo, // 負 = はみ出し量 (点評価なので exact)
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

    thickness_verdict(law_name, priority, worst, undecided)
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

    thickness_verdict(law_name, priority, worst, undecided)
}

/// 表面間距離 (gap) が `m` を超えることの証明
///
/// gap ≤ m なら中点 p に `dist(p, A) ≤ m/2 && dist(p, B) ≤ m/2` の点がある
/// (p が検査 AABB 内にある前提) 各セルを m/2 広げた箱で A か B が一様に
/// 正なら、そのセルにそんな p は無い 全セルで示せれば gap > m
fn gap_exceeds(a: &SdfNode, b: &SdfNode, m: f32, config: &CheckConfig) -> bool {
    let half = m * 0.5;
    for (_, cell) in GridSampler::new(config) {
        let mut stack: Vec<(Vec3Interval, u32)> = vec![(cell, 0)];
        while let Some((bx, depth)) = stack.pop() {
            let e = box_expand(bx, half);
            if sdf_interval(a, e).lo > 0.0 || sdf_interval(b, e).lo > 0.0 {
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

    let make_violation = |residual: f32, point: Vec3, region: Vec3Interval| Violation {
        law_name: law_name.to_string(),
        priority,
        residual,
        point,
        region,
    };

    if let Some((ub, point, region)) = upper {
        if ub < min_distance {
            return Verdict {
                violation: Some(make_violation(ub - min_distance, point, region)),
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
            violation: Some(make_violation(residual, point, region)),
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
        } => Verdict {
            violation: check_thermal(
                node,
                heat_sources,
                *search_radius,
                *min_surface_ratio,
                &law.name,
                law.priority,
                config,
            ),
            unresolved: None,
        },
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
        Constraint::Continuity { node, seed_point } => Verdict {
            violation: check_continuity(node, *seed_point, &law.name, law.priority, config),
            unresolved: None,
        },
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
        } => Verdict {
            violation: check_volume_conservation(
                before,
                after,
                *relative_tolerance,
                &law.name,
                law.priority,
                config,
            ),
            unresolved: None,
        },
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

/// Thermal: 各 heat source 近傍の 表面近傍セル数 / 内部セル数 の ratio が下限未満なら violation
///
/// step (グリッド 1 セル辺) を「表面近傍」判定に流用
/// residual = `actual_ratio` - `min_surface_ratio` (負 = 不足)
fn check_thermal(
    node: &SdfNode,
    heat_sources: &[Vec3],
    search_radius: f32,
    min_surface_ratio: f32,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Option<Violation> {
    let extent = config.aabb_max - config.aabb_min;
    #[allow(clippy::cast_precision_loss)]
    let step = extent.x / (config.resolution as f32);
    let surface_threshold = step;

    let mut worst: Option<(f32, Vec3, Vec3Interval)> = None;

    for &source in heat_sources {
        let mut surface_count: usize = 0;
        let mut interior_count: usize = 0;
        let mut worst_cell: Option<(Vec3, Vec3Interval)> = None;

        for (center, bounds) in GridSampler::new(config) {
            if center.distance(source) > search_radius {
                continue;
            }
            let d = sdf_eval(node, center);
            if d < 0.0 {
                interior_count += 1;
                if worst_cell.is_none() {
                    worst_cell = Some((center, bounds));
                }
            }
            if d.abs() < surface_threshold {
                surface_count += 1;
            }
        }

        if interior_count == 0 {
            continue; // 熱源近傍に内部セルなし = 対象外
        }

        #[allow(clippy::cast_precision_loss)]
        let ratio = (surface_count as f32) / (interior_count as f32);
        if ratio < min_surface_ratio {
            let residual = ratio - min_surface_ratio; // 負 = 不足
            if let Some((point, region)) = worst_cell {
                match &worst {
                    Some((w, _, _)) if residual >= *w => {}
                    _ => worst = Some((residual, point, region)),
                }
            }
        }
    }

    worst.map(|(residual, point, region)| Violation {
        law_name: law_name.to_string(),
        priority,
        residual,
        point,
        region,
    })
}

/// grid 座標 (cell 単位、f32) → `[0, n)` の cell index
///
/// `max(0.0)` で負と NaN を 0 に寄せてから truncation、上限は `min(n - 1)` で clamp
/// (`n >= 1` 前提、`n` は `CheckConfig` の grid 解像度で小さい)
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn grid_index(coord: f32, n: usize) -> usize {
    (coord.max(0.0) as usize).min(n - 1)
}

/// Continuity: seed から 6-connected flood fill で 到達不能な内部セルがあれば violation
///
/// seed が内部でない (sdf(seed) >= 0) なら violation として即報告
fn check_continuity(
    node: &SdfNode,
    seed_point: Vec3,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Option<Violation> {
    let seed_dist = sdf_eval(node, seed_point);
    if seed_dist >= 0.0 {
        return Some(Violation {
            law_name: law_name.to_string(),
            priority,
            residual: seed_dist, // 正値 = seed 外部
            point: seed_point,
            region: Vec3Interval {
                x: Interval {
                    lo: seed_point.x,
                    hi: seed_point.x,
                },
                y: Interval {
                    lo: seed_point.y,
                    hi: seed_point.y,
                },
                z: Interval {
                    lo: seed_point.z,
                    hi: seed_point.z,
                },
            },
        });
    }

    let n = config.resolution;
    let extent = config.aabb_max - config.aabb_min;
    #[allow(clippy::cast_precision_loss)]
    let step = extent / (n as f32);

    // grid 上の interior mask を構築
    let mut interior = vec![false; n * n * n];
    let mut total_interior: usize = 0;
    for (ix, iy, iz, idx) in grid_indices(n) {
        #[allow(clippy::cast_precision_loss)]
        let center =
            config.aabb_min + step * Vec3::new(ix as f32, iy as f32, iz as f32) + step * 0.5;
        if sdf_eval(node, center) < 0.0 {
            interior[idx] = true;
            total_interior += 1;
        }
    }

    if total_interior == 0 {
        // 内部セルなし = 対象外 (seed が内部だが grid 解像度で拾えず)
        return None;
    }

    // seed セルの grid index
    let rel = (seed_point - config.aabb_min) / step;
    let sx = grid_index(rel.x, n);
    let sy = grid_index(rel.y, n);
    let sz = grid_index(rel.z, n);
    let seed_idx = sx + sy * n + sz * n * n;

    if !interior[seed_idx] {
        // seed 点は内部でも該当セル中心が内部でない = 精度不足で seed セルが空
        return None;
    }

    // BFS flood fill
    let mut visited = vec![false; n * n * n];
    let mut queue = std::collections::VecDeque::new();
    queue.push_back((sx, sy, sz));
    visited[seed_idx] = true;
    let mut reachable: usize = 1;

    while let Some((x, y, z)) = queue.pop_front() {
        // 6-connected 隣接 (grid 外は None、符号付き演算なし)
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
            if !visited[nidx] && interior[nidx] {
                visited[nidx] = true;
                reachable += 1;
                queue.push_back((nx, ny, nz));
            }
        }
    }

    if reachable < total_interior {
        // 到達不能な内部セルの 1 つを見つけて point / region 化
        for (ix, iy, iz, idx) in grid_indices(n) {
            if interior[idx] && !visited[idx] {
                #[allow(clippy::cast_precision_loss)]
                let lo = config.aabb_min + step * Vec3::new(ix as f32, iy as f32, iz as f32);
                let center = lo + step * 0.5;
                let hi = lo + step;
                #[allow(clippy::cast_precision_loss)]
                let unreachable_ratio =
                    ((total_interior - reachable) as f32) / (total_interior as f32);
                return Some(Violation {
                    law_name: law_name.to_string(),
                    priority,
                    residual: -unreachable_ratio, // 負値 (到達不能 fraction)
                    point: center,
                    region: Vec3Interval {
                        x: Interval { lo: lo.x, hi: hi.x },
                        y: Interval { lo: lo.y, hi: hi.y },
                        z: Interval { lo: lo.z, hi: hi.z },
                    },
                });
            }
        }
    }

    None
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

/// `VolumeConservation`: before / after の内部セル数を count、相対差が tolerance 超過で violation
fn check_volume_conservation(
    before: &SdfNode,
    after: &SdfNode,
    relative_tolerance: f32,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Option<Violation> {
    let mut before_count: usize = 0;
    let mut after_count: usize = 0;
    let mut sample_region: Option<(Vec3, Vec3Interval)> = None;

    for (center, bounds) in GridSampler::new(config) {
        let db = sdf_eval(before, center);
        let da = sdf_eval(after, center);
        if db < 0.0 {
            before_count += 1;
        }
        if da < 0.0 {
            after_count += 1;
            if sample_region.is_none() {
                sample_region = Some((center, bounds));
            }
        } else if db < 0.0 && sample_region.is_none() {
            sample_region = Some((center, bounds));
        }
    }

    if before_count == 0 {
        return None; // 変形前が空 = 対象外
    }

    let diff = after_count.abs_diff(before_count);
    #[allow(clippy::cast_precision_loss)]
    let relative_diff = (diff as f32) / (before_count as f32);

    if relative_diff > relative_tolerance {
        let (point, region) = sample_region.unwrap_or_else(|| {
            (
                (config.aabb_min + config.aabb_max) * 0.5,
                Vec3Interval {
                    x: Interval {
                        lo: config.aabb_min.x,
                        hi: config.aabb_max.x,
                    },
                    y: Interval {
                        lo: config.aabb_min.y,
                        hi: config.aabb_max.y,
                    },
                    z: Interval {
                        lo: config.aabb_min.z,
                        hi: config.aabb_max.z,
                    },
                },
            )
        });
        Some(Violation {
            law_name: law_name.to_string(),
            priority,
            residual: relative_tolerance - relative_diff, // 負 = 超過量
            point,
            region,
        })
    } else {
        None
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

    /// ハード制約を追加
    #[must_use]
    pub fn hard(mut self, name: impl Into<String>, constraint: Constraint) -> Self {
        self.laws.push(Law::hard(name, constraint));
        self
    }

    /// ソフト制約を追加
    #[must_use]
    pub fn soft(mut self, name: impl Into<String>, weight: f32, constraint: Constraint) -> Self {
        self.laws.push(Law::soft(name, weight, constraint));
        self
    }

    /// 法則リストの参照を返す
    #[must_use]
    pub fn laws(&self) -> &[Law] {
        &self.laws
    }

    /// Stress 制約を hard で追加する convenience
    #[must_use]
    pub fn stress(
        self,
        name: impl Into<String>,
        node: SdfNode,
        load_points: Vec<(Vec3, f32)>,
        min_thickness_factor: f32,
    ) -> Self {
        self.hard(
            name,
            Constraint::Stress {
                node,
                load_points,
                min_thickness_factor,
            },
        )
    }

    /// Thermal 制約を hard で追加する convenience
    #[must_use]
    pub fn thermal(
        self,
        name: impl Into<String>,
        node: SdfNode,
        heat_sources: Vec<Vec3>,
        search_radius: f32,
        min_surface_ratio: f32,
    ) -> Self {
        self.hard(
            name,
            Constraint::Thermal {
                node,
                heat_sources,
                search_radius,
                min_surface_ratio,
            },
        )
    }

    /// Contact 制約を hard で追加する convenience
    #[must_use]
    pub fn contact(
        self,
        name: impl Into<String>,
        a: SdfNode,
        b: SdfNode,
        min_distance: f32,
        max_distance: f32,
    ) -> Self {
        self.hard(
            name,
            Constraint::Contact {
                a,
                b,
                min_distance,
                max_distance,
            },
        )
    }

    /// Continuity 制約を hard で追加する convenience
    #[must_use]
    pub fn continuity(self, name: impl Into<String>, node: SdfNode, seed_point: Vec3) -> Self {
        self.hard(name, Constraint::Continuity { node, seed_point })
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
        self.hard(
            name,
            Constraint::GradientBound {
                node,
                max_gradient,
                probe,
            },
        )
    }

    /// `Reachable` 制約を hard で追加する convenience
    #[must_use]
    pub fn reachable(self, name: impl Into<String>, node: SdfNode, from: Vec3, to: Vec3) -> Self {
        self.hard(name, Constraint::Reachable { node, from, to })
    }

    /// `VolumeConservation` 制約を hard で追加する convenience
    #[must_use]
    pub fn volume_conservation(
        self,
        name: impl Into<String>,
        before: SdfNode,
        after: SdfNode,
        relative_tolerance: f32,
    ) -> Self {
        self.hard(
            name,
            Constraint::VolumeConservation {
                before,
                after,
                relative_tolerance,
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
        let _ = writeln!(
            out,
            "  [{severity}] {}: residual={:.4}, at=({:.2},{:.2},{:.2}), region=[{:.2}..{:.2}]x[{:.2}..{:.2}]x[{:.2}..{:.2}]",
            v.law_name,
            v.residual,
            v.point.x, v.point.y, v.point.z,
            v.region.x.lo, v.region.x.hi,
            v.region.y.lo, v.region.y.hi,
            v.region.z.lo, v.region.z.hi,
        );
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
        assert_eq!(interval_sign(Interval::new(-1.0, 0.0)), None);
        // 両端が厳密に負 / 非負なら断定できる
        assert_eq!(interval_sign(Interval::new(-1.0, -0.5)), Some(false));
        assert_eq!(interval_sign(Interval::new(0.0, 1.0)), Some(true));
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
            &[Law::hard(
                "surface_of_inner",
                Constraint::Containment { inner, outer },
            )],
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
            )],
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
            )],
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
            &[Law::hard("surface_of_a", Constraint::NonOverlap { a, b })],
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
            &[Law::hard(
                "surface_of_b",
                Constraint::NonOverlap { a: a2, b: b2 },
            )],
            &config,
        );
        assert!(
            !report2.has_hard_violations(),
            "b の表面上の点 (fb = 0) を侵入の witness にした\n{}",
            format_report(&report2)
        );
    }
}
