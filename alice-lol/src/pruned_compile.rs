//! 空間枝刈りコンパイラ
//!
//! AABB を N×N×N グリッドに分割し、セルごとに `eval_interval` で
//! 不要なサブツリーを除去した軽量 GLSL を生成する。

use std::sync::Arc;

use alice_sdf::interval::{eval_interval, Interval, Vec3Interval};
use alice_sdf::SdfNode;
use glam::Vec3;

/// グリッドセルの分類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    /// `lo > 0` — 完全に外側（描画不要）
    Outside,
    /// `hi < 0` — 完全に内側
    Inside,
    /// 表面が通過する可能性あり
    Crossing,
}

/// セルごとの解析結果
#[derive(Debug, Clone)]
pub struct CellInfo {
    /// グリッド座標
    pub ix: usize,
    pub iy: usize,
    pub iz: usize,
    /// 空間的な範囲
    pub bounds: Vec3Interval,
    /// セル分類
    pub kind: CellKind,
    /// 枝刈り済み `SdfNode`（Crossing セルのみ `Some`）
    pub pruned_node: Option<SdfNode>,
}

/// 空間枝刈りの設定
#[derive(Debug, Clone)]
pub struct PruneConfig {
    /// AABB 最小点
    pub aabb_min: Vec3,
    /// AABB 最大点
    pub aabb_max: Vec3,
    /// 各軸の分割数
    pub grid_resolution: usize,
}

impl Default for PruneConfig {
    fn default() -> Self {
        Self {
            aabb_min: Vec3::splat(-5.0),
            aabb_max: Vec3::splat(5.0),
            grid_resolution: 4,
        }
    }
}

/// 空間枝刈り結果
#[derive(Debug, Clone)]
pub struct PruneResult {
    /// セル情報の 3D 配列（row-major: iz * ny * nx + iy * nx + ix）
    pub cells: Vec<CellInfo>,
    /// 設定
    pub config: PruneConfig,
    /// 統計: 外側セル数
    pub outside_count: usize,
    /// 統計: 内側セル数
    pub inside_count: usize,
    /// 統計: 交差セル数
    pub crossing_count: usize,
}

/// AABB を N×N×N に分割してセルごとに解析
#[must_use]
pub fn analyze_grid(node: &SdfNode, config: &PruneConfig) -> PruneResult {
    let n = config.grid_resolution;
    let total = n * n * n;
    let mut cells = Vec::with_capacity(total);
    let mut outside_count = 0usize;
    let mut inside_count = 0usize;
    let mut crossing_count = 0usize;

    let extent = config.aabb_max - config.aabb_min;
    #[allow(clippy::cast_precision_loss)]
    let inv_n = 1.0 / n as f32;

    for iz in 0..n {
        for iy in 0..n {
            for ix in 0..n {
                #[allow(clippy::cast_precision_loss)]
                let lo =
                    config.aabb_min + extent * Vec3::new(ix as f32, iy as f32, iz as f32) * inv_n;
                #[allow(clippy::cast_precision_loss)]
                let hi = config.aabb_min
                    + extent * Vec3::new((ix + 1) as f32, (iy + 1) as f32, (iz + 1) as f32) * inv_n;

                let bounds = Vec3Interval {
                    x: Interval { lo: lo.x, hi: hi.x },
                    y: Interval { lo: lo.y, hi: hi.y },
                    z: Interval { lo: lo.z, hi: hi.z },
                };

                let iv = eval_interval(node, bounds);

                let kind = if iv.lo > 0.0 {
                    outside_count += 1;
                    CellKind::Outside
                } else if iv.hi < 0.0 {
                    inside_count += 1;
                    CellKind::Inside
                } else {
                    crossing_count += 1;
                    CellKind::Crossing
                };

                let pruned_node = if kind == CellKind::Crossing {
                    Some(prune_subtree(node, bounds))
                } else {
                    None
                };

                cells.push(CellInfo {
                    ix,
                    iy,
                    iz,
                    bounds,
                    kind,
                    pruned_node,
                });
            }
        }
    }

    PruneResult {
        cells,
        config: config.clone(),
        outside_count,
        inside_count,
        crossing_count,
    }
}

/// サブツリー枝刈り: Union / `SmoothUnion` で片方が確実に遠ければ除去
#[must_use]
#[allow(clippy::too_many_lines)]
fn prune_subtree(node: &SdfNode, bounds: Vec3Interval) -> SdfNode {
    match node {
        // ── Union: min(a, b) → a が確実に遠ければ b のみ ──
        SdfNode::Union { a, b } => {
            let iv_a = eval_interval(a, bounds);
            let iv_b = eval_interval(b, bounds);
            if iv_a.lo >= iv_b.hi {
                // a は b より確実に遠い → b だけで十分
                prune_subtree(b, bounds)
            } else if iv_b.lo >= iv_a.hi {
                prune_subtree(a, bounds)
            } else {
                SdfNode::Union {
                    a: Arc::new(prune_subtree(a, bounds)),
                    b: Arc::new(prune_subtree(b, bounds)),
                }
            }
        }

        // ── SmoothUnion: k の影響範囲を考慮 ──
        SdfNode::SmoothUnion { a, b, k } => {
            let iv_a = eval_interval(a, bounds);
            let iv_b = eval_interval(b, bounds);
            if iv_a.lo >= iv_b.hi + k {
                prune_subtree(b, bounds)
            } else if iv_b.lo >= iv_a.hi + k {
                prune_subtree(a, bounds)
            } else {
                SdfNode::SmoothUnion {
                    a: Arc::new(prune_subtree(a, bounds)),
                    b: Arc::new(prune_subtree(b, bounds)),
                    k: *k,
                }
            }
        }

        // ── Intersection / SmoothIntersection: 両方必要なのでそのまま再帰 ──
        SdfNode::Intersection { a, b } => SdfNode::Intersection {
            a: Arc::new(prune_subtree(a, bounds)),
            b: Arc::new(prune_subtree(b, bounds)),
        },
        SdfNode::SmoothIntersection { a, b, k } => SdfNode::SmoothIntersection {
            a: Arc::new(prune_subtree(a, bounds)),
            b: Arc::new(prune_subtree(b, bounds)),
            k: *k,
        },

        // ── Subtraction / SmoothSubtraction: 両方必要 ──
        SdfNode::Subtraction { a, b } => SdfNode::Subtraction {
            a: Arc::new(prune_subtree(a, bounds)),
            b: Arc::new(prune_subtree(b, bounds)),
        },
        SdfNode::SmoothSubtraction { a, b, k } => SdfNode::SmoothSubtraction {
            a: Arc::new(prune_subtree(a, bounds)),
            b: Arc::new(prune_subtree(b, bounds)),
            k: *k,
        },

        // ── Translate: bounds をシフトして子を枝刈り ──
        SdfNode::Translate { child, offset } => {
            let shifted = Vec3Interval {
                x: Interval {
                    lo: bounds.x.lo - offset.x,
                    hi: bounds.x.hi - offset.x,
                },
                y: Interval {
                    lo: bounds.y.lo - offset.y,
                    hi: bounds.y.hi - offset.y,
                },
                z: Interval {
                    lo: bounds.z.lo - offset.z,
                    hi: bounds.z.hi - offset.z,
                },
            };
            SdfNode::Translate {
                child: Arc::new(prune_subtree(child, shifted)),
                offset: *offset,
            }
        }

        // ── その他のトランスフォーム・モディファイア: 子を再帰枝刈り ──
        SdfNode::Rotate { child, rotation } => SdfNode::Rotate {
            child: Arc::new(prune_subtree(child, bounds)),
            rotation: *rotation,
        },
        SdfNode::Scale { child, factor } => SdfNode::Scale {
            child: Arc::new(prune_subtree(child, bounds)),
            factor: *factor,
        },
        SdfNode::Twist { child, strength } => SdfNode::Twist {
            child: Arc::new(prune_subtree(child, bounds)),
            strength: *strength,
        },
        SdfNode::Bend { child, curvature } => SdfNode::Bend {
            child: Arc::new(prune_subtree(child, bounds)),
            curvature: *curvature,
        },
        SdfNode::Round { child, radius } => SdfNode::Round {
            child: Arc::new(prune_subtree(child, bounds)),
            radius: *radius,
        },
        SdfNode::Onion { child, thickness } => SdfNode::Onion {
            child: Arc::new(prune_subtree(child, bounds)),
            thickness: *thickness,
        },
        SdfNode::Mirror { child, axes } => SdfNode::Mirror {
            child: Arc::new(prune_subtree(child, bounds)),
            axes: *axes,
        },
        SdfNode::RepeatInfinite { child, spacing } => SdfNode::RepeatInfinite {
            child: Arc::new(prune_subtree(child, bounds)),
            spacing: *spacing,
        },

        // ── Time ──
        SdfNode::Animated {
            child,
            speed,
            amplitude,
        } => SdfNode::Animated {
            child: Arc::new(prune_subtree(child, bounds)),
            speed: *speed,
            amplitude: *amplitude,
        },
        SdfNode::Morph { a, b, t } => SdfNode::Morph {
            a: Arc::new(prune_subtree(a, bounds)),
            b: Arc::new(prune_subtree(b, bounds)),
            t: *t,
        },

        // ── プリミティブ / 未対応バリアント: そのまま返す ──
        other => other.clone(),
    }
}

/// `SdfNode` ツリーのノード数を数える
#[must_use]
pub fn count_nodes(node: &SdfNode) -> usize {
    match node {
        SdfNode::Union { a, b }
        | SdfNode::Intersection { a, b }
        | SdfNode::Subtraction { a, b }
        | SdfNode::SmoothUnion { a, b, .. }
        | SdfNode::SmoothIntersection { a, b, .. }
        | SdfNode::SmoothSubtraction { a, b, .. }
        | SdfNode::Morph { a, b, .. } => 1 + count_nodes(a) + count_nodes(b),
        SdfNode::Translate { child, .. }
        | SdfNode::Rotate { child, .. }
        | SdfNode::Scale { child, .. }
        | SdfNode::Twist { child, .. }
        | SdfNode::Bend { child, .. }
        | SdfNode::Round { child, .. }
        | SdfNode::Onion { child, .. }
        | SdfNode::Mirror { child, .. }
        | SdfNode::RepeatInfinite { child, .. }
        | SdfNode::Animated { child, .. } => 1 + count_nodes(child),
        _ => 1,
    }
}

/// 枝刈り結果から GLSL コードを生成
///
/// - Crossing セルごとに `float sdf_cell_X_Y_Z(vec3 p)` を生成
/// - Outside/Inside セルはスキップ
/// - ディスパッチャ `float sdf_pruned(vec3 p)` で座標→セル→関数呼び出し
#[must_use]
#[cfg(feature = "glsl")]
pub fn to_pruned_glsl(node: &SdfNode, result: &PruneResult) -> String {
    use std::fmt::Write;

    use alice_sdf::compiled::glsl::{GlslShader, GlslTranspileMode};

    let mut out = String::with_capacity(4096);

    // フルシーン関数（Outside/Inside セル用フォールバック）
    let full = GlslShader::transpile(node, GlslTranspileMode::Hardcoded);
    out.push_str("// ── Full scene (fallback) ──\n");
    out.push_str(&full.source);
    out.push('\n');

    // セルごとの枝刈り関数
    for cell in &result.cells {
        if cell.kind != CellKind::Crossing {
            continue;
        }
        let Some(ref pruned) = cell.pruned_node else {
            continue;
        };
        let cell_glsl = GlslShader::transpile(pruned, GlslTranspileMode::Hardcoded);

        let _ = write!(
            out,
            "\n// ── Cell ({}, {}, {}) ──\n",
            cell.ix, cell.iy, cell.iz
        );

        // 関数名をリネーム: sdf( → sdf_cell_X_Y_Z(
        let cell_fn = format!("sdf_cell_{}_{}_{}", cell.ix, cell.iy, cell.iz);
        let renamed = cell_glsl
            .source
            .replace("float sdf(", &format!("float {cell_fn}("));
        out.push_str(&renamed);
        out.push('\n');
    }

    // ディスパッチャ
    let n = result.config.grid_resolution;
    let mn = result.config.aabb_min;
    let mx = result.config.aabb_max;

    out.push_str("\n// ── Dispatcher ──\n");
    out.push_str("float sdf_pruned(vec3 p) {\n");
    let _ = writeln!(
        out,
        "  vec3 rel = (p - vec3({:.6}, {:.6}, {:.6})) / vec3({:.6}, {:.6}, {:.6});",
        mn.x,
        mn.y,
        mn.z,
        mx.x - mn.x,
        mx.y - mn.y,
        mx.z - mn.z,
    );
    #[allow(clippy::cast_precision_loss)]
    let _ = writeln!(
        out,
        "  ivec3 idx = clamp(ivec3(rel * {:.1}), ivec3(0), ivec3({}));",
        n as f32,
        n - 1,
    );
    let _ = writeln!(
        out,
        "  int cell = idx.z * {} + idx.y * {} + idx.x;",
        n * n,
        n
    );
    out.push_str("  switch(cell) {\n");

    for cell in &result.cells {
        if cell.kind != CellKind::Crossing {
            continue;
        }
        let idx = cell.iz * n * n + cell.iy * n + cell.ix;
        let _ = writeln!(
            out,
            "    case {idx}: return sdf_cell_{}_{}_{vz}(p);",
            cell.ix,
            cell.iy,
            vz = cell.iz,
        );
    }

    out.push_str("    default: return sdf(p);\n");
    out.push_str("  }\n");
    out.push_str("}\n");

    out
}

/// 枝刈り統計のサマリ文字列
#[must_use]
pub fn summary(result: &PruneResult) -> String {
    let total = result.cells.len();
    format!(
        "Grid {}^3 = {} cells: {} outside, {} inside, {} crossing",
        result.config.grid_resolution,
        total,
        result.outside_count,
        result.inside_count,
        result.crossing_count,
    )
}
