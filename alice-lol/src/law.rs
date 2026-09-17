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
