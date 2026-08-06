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
/// A.1.0 (2026-08-06) 時点で 3 variant (NonOverlap / Containment / MinThickness)
/// A.2 (2026-08-06) で 5 variant 追加 (Stress / Thermal / Contact / Continuity / `VolumeConservation`)
/// 新 5 variant は geometric proxy 評価 (grid + sdf_eval)、精密 physics-backed 評価は A.2.1 で追加予定
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
    /// 相対差 |V_before - V_after| / max(V_before, 1) が `relative_tolerance` 超過で violation
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

/// 法則検証の結果
#[derive(Debug, Clone)]
pub struct LawReport {
    /// 全法則数
    pub total_laws: usize,
    /// パスした法則数
    pub passed: usize,
    /// 違反リスト（残差の絶対値の大きい順）
    pub violations: Vec<Violation>,
}

impl LawReport {
    /// 全法則がパスしたか
    #[must_use]
    pub const fn all_passed(&self) -> bool {
        self.violations.is_empty()
    }

    /// ハード制約の違反があるか
    #[must_use]
    pub fn has_hard_violations(&self) -> bool {
        self.violations.iter().any(|v| v.priority == Priority::Hard)
    }
}

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

/// 法則リストを一括検証
#[must_use]
pub fn check_laws(laws: &[Law], config: &CheckConfig) -> LawReport {
    let mut violations = Vec::new();

    for law in laws {
        let violation = match &law.constraint {
            Constraint::NonOverlap { a, b } => {
                check_non_overlap(a, b, &law.name, law.priority, config)
            }
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
        };
        if let Some(v) = violation {
            violations.push(v);
        }
    }

    // 残差の絶対値が大きい順にソート
    violations.sort_by(|a, b| {
        b.residual
            .abs()
            .partial_cmp(&a.residual.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let passed = laws.len() - violations.len();

    LawReport {
        total_laws: laws.len(),
        passed,
        violations,
    }
}

/// `NonOverlap`: セル中心で両 SDF が負（内部）なら重なり
fn check_non_overlap(
    a: &SdfNode,
    b: &SdfNode,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Option<Violation> {
    let mut worst: Option<(f32, Vec3, Vec3Interval)> = None;

    for (center, bounds) in GridSampler::new(config) {
        let da = sdf_eval(a, center);
        let db = sdf_eval(b, center);

        // 両方が負 → 重なっている
        if da < 0.0 && db < 0.0 {
            let residual = da.max(db); // 浅い方の侵入深さ（min overlap）
            match &worst {
                Some((w, _, _)) if residual >= *w => {}
                _ => worst = Some((residual, center, bounds)),
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

/// Containment: inner が内部（< 0）かつ outer が外部（> 0）→ はみ出し
fn check_containment(
    inner: &SdfNode,
    outer: &SdfNode,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Option<Violation> {
    let mut worst: Option<(f32, Vec3, Vec3Interval)> = None;

    for (center, bounds) in GridSampler::new(config) {
        let d_inner = sdf_eval(inner, center);
        let d_outer = sdf_eval(outer, center);

        // inner の内部かつ outer の外部 → はみ出し
        if d_inner < 0.0 && d_outer > 0.0 {
            let residual = -d_outer; // 負の値（はみ出し量）
            match &worst {
                Some((w, _, _)) if residual >= *w => {}
                _ => worst = Some((residual, center, bounds)),
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

/// `MinThickness`: 内部点で SDF 値の絶対値が `min_thickness` 未満なら肉厚不足
fn check_min_thickness(
    node: &SdfNode,
    min_thickness: f32,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Option<Violation> {
    let mut worst: Option<(f32, Vec3, Vec3Interval)> = None;

    for (center, bounds) in GridSampler::new(config) {
        let d = sdf_eval(node, center);

        // 内部（d < 0）かつ表面に近すぎる（|d| < min_thickness）
        if d < 0.0 && d.abs() < min_thickness {
            let residual = d.abs() - min_thickness; // 負 = 不足量
            match &worst {
                Some((w, _, _)) if residual >= *w => {}
                _ => worst = Some((residual, center, bounds)),
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

/// Stress: 各 load point 近傍の内部セルで肉厚 < force × factor なら violation
///
/// 探索半径 = max(1.0, force) の球内セル対象 (heuristic)
fn check_stress(
    node: &SdfNode,
    load_points: &[(Vec3, f32)],
    min_thickness_factor: f32,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Option<Violation> {
    let mut worst: Option<(f32, Vec3, Vec3Interval)> = None;

    for (center, bounds) in GridSampler::new(config) {
        let d = sdf_eval(node, center);
        if d >= 0.0 {
            continue; // 内部セルのみ対象
        }

        let thickness = d.abs();

        for &(lp, force) in load_points {
            let search_radius = force.max(1.0);
            if center.distance(lp) > search_radius {
                continue;
            }

            let required_thickness = force * min_thickness_factor;
            if thickness < required_thickness {
                let residual = thickness - required_thickness; // 負 = 不足量
                match &worst {
                    Some((w, _, _)) if residual >= *w => {}
                    _ => worst = Some((residual, center, bounds)),
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

/// Thermal: 各 heat source 近傍の 表面近傍セル数 / 内部セル数 の ratio が下限未満なら violation
///
/// step (グリッド 1 セル辺) を「表面近傍」判定に流用
/// residual = actual_ratio - `min_surface_ratio` (負 = 不足)
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

/// Contact: A と B の最小表面間距離が \[min, max\] 範囲外なら violation
///
/// 両 sdf < 0 の cell (interfering) は residual = 侵入深さ (負) で返す
/// それ以外は min over (両 sdf > 0 の cell) of (sdf_a + sdf_b) を surface 間距離として使用
fn check_contact(
    a: &SdfNode,
    b: &SdfNode,
    min_distance: f32,
    max_distance: f32,
    law_name: &str,
    priority: Priority,
    config: &CheckConfig,
) -> Option<Violation> {
    let mut min_surface_distance: Option<(f32, Vec3, Vec3Interval)> = None;
    let mut interfering: Option<(f32, Vec3, Vec3Interval)> = None;

    for (center, bounds) in GridSampler::new(config) {
        let da = sdf_eval(a, center);
        let db = sdf_eval(b, center);

        if da < 0.0 && db < 0.0 {
            // 両方内部 = interfering
            let residual = da.max(db); // 浅い方の侵入深さ
            match &interfering {
                Some((w, _, _)) if residual >= *w => {}
                _ => interfering = Some((residual, center, bounds)),
            }
        } else if da > 0.0 && db > 0.0 {
            // 両方外部 = 表面間距離の候補
            let dist = da + db;
            match &min_surface_distance {
                Some((w, _, _)) if dist >= *w => {}
                _ => min_surface_distance = Some((dist, center, bounds)),
            }
        }
    }

    if let Some((residual, point, region)) = interfering {
        return Some(Violation {
            law_name: law_name.to_string(),
            priority,
            residual,
            point,
            region,
        });
    }

    let (min_dist, point, region) = min_surface_distance?;

    if min_dist < min_distance {
        // 近すぎる (violation: 過密接触)
        Some(Violation {
            law_name: law_name.to_string(),
            priority,
            residual: min_dist - min_distance, // 負 = 距離不足
            point,
            region,
        })
    } else if min_dist > max_distance {
        // 遠すぎる (violation: 接触不能)
        Some(Violation {
            law_name: law_name.to_string(),
            priority,
            residual: max_dist_residual(min_dist, max_distance), // 負 = 距離過多
            point,
            region,
        })
    } else {
        None
    }
}

/// max_distance 超過時の residual (負値で「どれだけ超過したか」を表現)
fn max_dist_residual(actual: f32, limit: f32) -> f32 {
    limit - actual
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
    let sx = (rel.x as i32).clamp(0, (n as i32) - 1) as usize;
    let sy = (rel.y as i32).clamp(0, (n as i32) - 1) as usize;
    let sz = (rel.z as i32).clamp(0, (n as i32) - 1) as usize;
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
        for (dx, dy, dz) in [
            (-1_i32, 0_i32, 0_i32),
            (1, 0, 0),
            (0, -1, 0),
            (0, 1, 0),
            (0, 0, -1),
            (0, 0, 1),
        ] {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            let nz = z as i32 + dz;
            if nx < 0 || nx >= n as i32 || ny < 0 || ny >= n as i32 || nz < 0 || nz >= n as i32 {
                continue;
            }
            let nx = nx as usize;
            let ny = ny as usize;
            let nz = nz as usize;
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

/// grid の (ix, iy, iz, flat_idx) を返すヘルパー
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

    #[allow(clippy::cast_precision_loss)]
    let diff = ((after_count as i64) - (before_count as i64)).unsigned_abs();
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
            let dbg_ca = format!("{ca:?}");
            let dbg_cb = format!("{cb:?}");
            let same_pair =
                (dbg_a == dbg_ca && dbg_b == dbg_cb) || (dbg_a == dbg_cb && dbg_b == dbg_ca);
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
        let _ = writeln!(out, "  All laws satisfied.");
        return out;
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
