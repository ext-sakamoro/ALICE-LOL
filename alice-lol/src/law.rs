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
    // NonOverlap(X, Y) + Containment(inner=X, outer=Y) → 矛盾
    // X が Y の中にある（Containment）のに X と Y が重ならない（NonOverlap）は不可能
    match (a, b) {
        (Constraint::NonOverlap { a: na, b: nb }, Constraint::Containment { inner, outer })
        | (Constraint::Containment { inner, outer }, Constraint::NonOverlap { a: na, b: nb }) => {
            // SdfNode は Debug 形式で比較（構造的等価）
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
