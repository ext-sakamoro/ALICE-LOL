//! # `skadis_sdf` — SKADIS panel の純 SDF 表現 (Phase 3''、ALICE way 回帰)
//!
//! Phase A.5.2 で `thin::skadis_panel_2d` を追加した (`Polygon2D` + earcutr 経路)、しかし
//! これは ALICE 三相原理 Phase 1 Data 相当 = ALICE 違反であることが Phase B.1.c 議論で
//! 判明
//!
//! 本 module は SKADIS panel を **純 SDF (`SdfNode`)** で表現する Phase 2 Law 経路
//! mesh 化は `alice_lol::print_export::node_to_3mf_dual_contouring` (SDF+DC) 経由
//! Marching Cubes の非多様体多発問題は Dual Contouring の Hermite data で解決
//!
//! ## SKADIS panel SDF spec (pipeline `formulas::skadis` 準拠)
//!
//! - 外形: `RoundedBox { size × size × thickness, corner_r }`
//! - peg 穴 (千鳥): base grid + stagger grid の 2 系統
//!   - base: 原点中心の `Box3d(PEG_W × thickness+margin × PEG_H)` を `RepeatFinite` で pitch=40
//!   - stagger: base を `(GRID_OFFSET, 0, GRID_OFFSET) = (20, 0, 20)` shift
//! - panel - (base ∪ stagger) の Subtraction で完成
//!
//! ## Phase 3'' 検証項目
//!
//! example `skadis_panel_dc_vs_mc.rs` で:
//! - 同 SDF を MC / DC 両方で mesh 化
//! - triangle 数 / vertex 数 比較
//! - pipeline 実測「SDF+MC で 6177 non-manifold edges」を DC が回避できるか実証
//!
//! ## Phase 3''.3.1 追加 (hook 3 種)
//!
//! pipeline Python `python generator` の shape を
//! Rust SDF に翻訳:
//!
//! | primitive | pipeline canonical | reach | load | `root_t` |
//! |-----------|-----------------|-------|------|--------|
//! | [`skadis_hook_l_sdf`] | `skadis-hook-l/generate.py` | 75mm | 5kgf (2-peg 分散) | 7mm |
//! | [`skadis_hook_j_sdf`] | `skadis-hook-j/generate.py` | 25mm (reach) + 70mm (drop) | 3kgf | 7.5mm |
//! | [`skadis_hook_s_sdf`] | `skadis-hook-s/generate.py` | 22mm (reach) + 45mm (drop) | 1kgf | 5.5mm |
//!
//! 実装: peg blade + shoulder + centerline sweep (Capsule 連結)
//! Python `LineString.buffer(R)` = SDF では連続 `Capsule` の `Union` で表現
//! `buffer(R).buffer(-R)` fillet は本 module では省略 (DC の Hermite で自然に滑らか)

use alice_sdf::SdfNode;
use glam::Vec3;
use std::sync::Arc;

// ────────────────────────────────────────────────────────
// SKADIS hook 定数 (pipeline `PEG_BLADE_W/T`, `SHOULDER_DEPTH/H` 準拠)
// ────────────────────────────────────────────────────────

/// SKADIS peg blade 幅 (mm、pipeline `PEG_BLADE_W`)
pub const PEG_BLADE_W: f32 = 5.0;

/// SKADIS peg blade 厚 (mm、pipeline `PEG_BLADE_T`)
pub const PEG_BLADE_T: f32 = 4.5;

/// SKADIS 板厚 (mm、pipeline `BOARD_T`)
pub const BOARD_T: f32 = 5.0;

/// SKADIS shoulder 追加深 (mm、pipeline `SHOULDER_DEPTH`)
pub const SHOULDER_DEPTH: f32 = 2.0;

/// SKADIS shoulder 高 (mm、pipeline `SHOULDER_H`)
pub const SHOULDER_H: f32 = 8.0;

// ────────────────────────────────────────────────────────
// helper — peg blade + shoulder (hook 3 種共通)
// ────────────────────────────────────────────────────────

/// SKADIS peg blade + shoulder の `SdfNode` (hook 系 3 accessory 共通の peg 部)
///
/// 座標系: pipeline Python と同期
/// - X 軸方向 = 板厚方向 (peg は X = -`BOARD_T` から 0 まで)
/// - Y 軸方向 = 上下 (pipeline Python では Y up / down)
/// - Z 軸方向 = hook 幅方向 (extrude direction、Python では `extrude_polygon` が Z 押出)
///
/// pipeline Python:
/// - `blade = box(-BOARD_T, -PEG_BLADE_T/2, 0, PEG_BLADE_T/2)` (X-Y 平面 rect)
/// - `shoulder = box(-BOARD_T-SHOULDER_DEPTH, -SHOULDER_H/2, -BOARD_T, SHOULDER_H/2)`
///
/// SDF 版は `hook_width` (Z 方向厚) を引数に取り、Box3d で 3D 化
///
/// # 引数
///
/// - `hook_width`: hook 部の Z 方向厚 (mm、通常 `hook_l/j=8mm、hook_s=5mm`)
#[must_use]
pub fn skadis_peg_and_shoulder(hook_width: f32) -> SdfNode {
    // Blade (X = -BOARD_T .. 0)
    let blade = SdfNode::Box3d {
        half_extents: Vec3::new(BOARD_T * 0.5, PEG_BLADE_T * 0.5, hook_width * 0.5),
    };
    let blade_placed = SdfNode::Translate {
        child: Arc::new(blade),
        offset: Vec3::new(-BOARD_T * 0.5, 0.0, 0.0),
    };
    // Shoulder (X = -BOARD_T-SHOULDER_DEPTH .. -BOARD_T)
    let shoulder = SdfNode::Box3d {
        half_extents: Vec3::new(SHOULDER_DEPTH * 0.5, SHOULDER_H * 0.5, hook_width * 0.5),
    };
    let shoulder_placed = SdfNode::Translate {
        child: Arc::new(shoulder),
        offset: Vec3::new(SHOULDER_DEPTH.mul_add(-0.5, -BOARD_T), 0.0, 0.0),
    };
    SdfNode::Union {
        a: Arc::new(blade_placed),
        b: Arc::new(shoulder_placed),
    }
}

/// 背面 (-Z 側) を取付面とする accessory 用の peg + shoulder
///
/// [`skadis_peg_and_shoulder`] は X 軸方向 (X = -`BOARD_T` .. 0) に伸びるので、Y 軸まわりに
/// -90 度回して -Z 方向へ向ける (blade が Z = -`BOARD_T` .. 0、shoulder がその奥) 原点は
/// 取付面 (背面の外面) の peg 中心に置く container / shelf のように取付面が Z 方向の
/// accessory で使う (hook 3 種と cord は取付面が X 方向なので回さない)
fn peg_facing_back(hook_width: f32) -> SdfNode {
    SdfNode::Rotate {
        child: Arc::new(skadis_peg_and_shoulder(hook_width)),
        rotation: glam::Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
    }
}

// ────────────────────────────────────────────────────────
// helper — 2D polyline を平らな帯 (flat strip) で SDF 化
// ────────────────────────────────────────────────────────

/// 2D polyline `pts` を、面内の半径 `tube_radius`・厚み (Z) `hook_width` の平らな帯で SDF 表現
///
/// pipeline Python `LineString.buffer(R, cap_style='round')` + `extrude_polygon` の SDF 相当:
/// 断面は面内で 2R、Z 方向に `hook_width` の長方形で、端は面内で丸い (stadium の押出)
/// 各 edge `(pts[i], pts[i+1])` を、Z 軸の円柱 (半径 R、高さ `hook_width`) を edge の向きに
/// `Elongate` した stadium 押出にし、全て `Union` で結合する (円柱は厳密な距離場で、
/// elongate は厳密性を保つので、各 edge の場も厳密)
///
/// 旧実装は 3D の `Capsule` 連結で、断面が直径 2R の丸い管になり、`hook_width` は管より
/// 狭くしか効かなかった (幅 8 の hook が直径 7 の管になった)
///
/// # Panics
///
/// なし `pts.len() < 2` の場合、空 SDF `Sphere { radius: 0.0 }` を返す
#[must_use]
pub fn capsule_polyline_sdf(pts: &[glam::Vec2], tube_radius: f32, hook_width: f32) -> SdfNode {
    if pts.len() < 2 {
        return SdfNode::Sphere { radius: 0.0 };
    }
    // 2026-08-07 fix: 線形左入れ子 fold → balanced fold で eval recursion 削減
    let strips: Vec<SdfNode> = pts
        .windows(2)
        .map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            let edge = b - a;
            let len = edge.length();
            // Z 軸の円柱 (Y 軸の円柱を X 軸まわりに 90 度回す)
            let puck = SdfNode::Rotate {
                child: Arc::new(SdfNode::Cylinder {
                    radius: tube_radius,
                    half_height: hook_width * 0.5,
                }),
                rotation: glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            };
            // edge の向き (局所 X) に len/2 だけ elongate → stadium の押出
            let elongated = SdfNode::Elongate {
                child: Arc::new(puck),
                amount: Vec3::new(len * 0.5, 0.0, 0.0),
            };
            let angle = edge.y.atan2(edge.x);
            let rotated = SdfNode::Rotate {
                child: Arc::new(elongated),
                rotation: glam::Quat::from_rotation_z(angle),
            };
            let mid = (a + b) * 0.5;
            SdfNode::Translate {
                child: Arc::new(rotated),
                offset: Vec3::new(mid.x, mid.y, 0.0),
            }
        })
        .collect();
    super::balanced_union_fold(strips).unwrap_or(SdfNode::Sphere { radius: 0.0 })
}

// ────────────────────────────────────────────────────────
// hook 3 種 (Phase 3''.3.1)
// ────────────────────────────────────────────────────────

/// SKADIS L 型 hook (2 peg、水平 arm + 上向き 1/4 円 tip、pipeline `skadis-hook-l`)
///
/// 想定荷重: 5kgf (2-peg 分散)、reach `75mm、root_t` 7mm
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::skadis_sdf::skadis_hook_l_sdf;
/// let h = skadis_hook_l_sdf();
/// // node_to_3mf_dual_contouring(&h, "hook_l.3mf", &config)
/// ```
#[must_use]
pub fn skadis_hook_l_sdf() -> SdfNode {
    let reach: f32 = 75.0;
    let root_t: f32 = 7.0;
    let hook_width: f32 = 8.0; // HOOK_WIDTH not standard PEG width, hook 部拡幅
    let radius = root_t * 0.5;

    // Centerline: [(0,0), (reach-8, 0)] + 1/4 arc from (reach-8, 0) → (reach, 8)
    let mut pts: Vec<glam::Vec2> =
        vec![glam::Vec2::new(0.0, 0.0), glam::Vec2::new(reach - 8.0, 0.0)];
    let n = 12;
    for i in 1..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        pts.push(glam::Vec2::new(
            8.0f32.mul_add(a.sin(), reach - 8.0),
            8.0 * (1.0 - a.cos()),
        ));
    }

    let peg_shoulder = skadis_peg_and_shoulder(hook_width);
    let body = capsule_polyline_sdf(&pts, radius, hook_width);
    SdfNode::Union {
        a: Arc::new(peg_shoulder),
        b: Arc::new(body),
    }
}

/// SKADIS J 型 hook (1 peg、深い J 字、pipeline `skadis-hook-j`)
///
/// 想定荷重: 3kgf、reach 25mm + drop `70mm、root_t` `7.5mm、hook_width` 8mm
#[must_use]
pub fn skadis_hook_j_sdf() -> SdfNode {
    let reach: f32 = 25.0;
    let drop: f32 = 70.0;
    let root_t: f32 = 7.5;
    let hook_width: f32 = 8.0;
    let radius = root_t * 0.5;

    // J 字 centerline: 前方 1/4 arc + 下方 straight + tip カーブ (0.75π)
    // 2026-08-07 NME fix: n=16 → 32 で adjacent capsule 間隔短縮、self-intersect 減、
    // DC watertight 破綻 (NME 42) 解消狙い hook_l (n=12) / hook_s (n=16) は tip angle
    // が 0.7π 以下で NME 0 保証済、hook_j の 0.75π tip のみ密度不足だった
    let mut pts: Vec<glam::Vec2> = vec![glam::Vec2::new(0.0, 0.0)];
    let n = 32;
    // 前方 1/4 arc (0 → π/2)、reach 方向に S(sin) 進み、下方に -R(1-cos)
    for i in 0..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        pts.push(glam::Vec2::new(reach * a.sin(), -reach * (1.0 - a.cos())));
    }
    // 下方 straight (Y = curve_end_y から drop 分下がる)
    let curve_end_y = pts.last().map_or(0.0, |v| v.y);
    let tip_y = curve_end_y - (drop - reach);
    pts.push(glam::Vec2::new(reach, tip_y));
    // 先端 J tip (0.75π カーブ、tip_r=8)
    let tip_r: f32 = 8.0;
    for i in 0..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * 0.75 * i as f32 / n as f32;
        pts.push(glam::Vec2::new(
            tip_r.mul_add(a.cos(), reach - tip_r),
            tip_r.mul_add(a.sin(), tip_y),
        ));
    }

    let peg_shoulder = skadis_peg_and_shoulder(hook_width);
    let body = capsule_polyline_sdf(&pts, radius, hook_width);
    SdfNode::Union {
        a: Arc::new(peg_shoulder),
        b: Arc::new(body),
    }
}

/// SKADIS S 型 hook (1 peg、汎用フック、pipeline `skadis-hook-s`)
///
/// 想定荷重: 1kgf、reach 22mm + drop `45mm、root_t` `5.5mm、hook_width` 5mm (peg 幅と同)
/// テーパー root→tip (5.5→3mm) は本 SDF では省略 (等幅 Capsule で近似、DC 実測で誤差確認予定)
#[must_use]
pub fn skadis_hook_s_sdf() -> SdfNode {
    let reach: f32 = 22.0;
    let drop: f32 = 45.0;
    let root_t: f32 = 5.5;
    let hook_width: f32 = 5.0; // peg blade 幅と同
    let radius = root_t * 0.5;

    let mut pts: Vec<glam::Vec2> = vec![glam::Vec2::new(0.0, 0.0)];
    let n = 16;
    for i in 0..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        pts.push(glam::Vec2::new(reach * a.sin(), -reach * (1.0 - a.cos())));
    }
    let curve_end_y = pts.last().map_or(0.0, |v| v.y);
    let tip_base_y = curve_end_y - (drop - reach);
    pts.push(glam::Vec2::new(reach, tip_base_y));
    // 先端 (0.7π tip カーブ、tip_r=6)
    let tip_r: f32 = 6.0;
    for i in 0..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * 0.7 * i as f32 / n as f32;
        pts.push(glam::Vec2::new(
            tip_r.mul_add(a.cos(), reach - tip_r),
            tip_r.mul_add(a.sin(), tip_base_y),
        ));
    }

    let peg_shoulder = skadis_peg_and_shoulder(hook_width);
    let body = capsule_polyline_sdf(&pts, radius, hook_width);
    SdfNode::Union {
        a: Arc::new(peg_shoulder),
        b: Arc::new(body),
    }
}

// ────────────────────────────────────────────────────────
// SKADIS 定数 (pipeline `formulas::skadis` と同期)
// ────────────────────────────────────────────────────────

/// SKADIS peg 幅 (mm、pipeline `PEG_W`)
pub const SKADIS_PEG_W: f32 = 5.0;

/// SKADIS peg 高 (mm、pipeline `PEG_H`)
pub const SKADIS_PEG_H: f32 = 15.0;

/// SKADIS grid pitch (mm、pipeline `GRID_PITCH`)
pub const SKADIS_GRID_PITCH: f32 = 40.0;

/// SKADIS grid offset (mm、千鳥、pipeline `GRID_OFFSET`)
pub const SKADIS_GRID_OFFSET: f32 = 20.0;

/// SKADIS edge margin (mm、pipeline `EDGE_MARGIN`)
pub const SKADIS_EDGE_MARGIN: f32 = 20.0;

/// SKADIS panel 標準厚 (mm、実プリント検証済)
pub const SKADIS_PANEL_THICKNESS: f32 = 5.0;

/// 貫通穴 depth margin (mm、subtract 用に peg 穴を板より少し長く取る)
pub const HOLE_THROUGH_MARGIN: f32 = 0.5;

/// SKADIS connector ネジ穴径 (mm、M2.5 = Ø2.7mm、pipeline `CONN_SCREW_D`)
/// production `skadis_connector_2x2.3mf` で 2 枚 panel を連結するネジ用
pub const SKADIS_CONN_SCREW_D: f32 = 2.7;

/// SKADIS connector/mount 穴の縁からの inset 距離 (mm、pipeline `CONN_INSET`)
/// = `OUTER_FRAME` / 2 = 6.0mm (frame 中央、板の縁から 6mm 内側)
pub const SKADIS_CONN_INSET: f32 = 6.0;

/// SKADIS 壁掛けマウント穴半径 (mm、Ø5mm、pipeline `MOUNT_HOLE_R`)
/// 上下辺に 3 穴ずつ、壁ネジ用
pub const SKADIS_MOUNT_HOLE_R: f32 = 2.5;

/// SKADIS 外周フレーム幅 (mm、pipeline `OUTER_FRAME`)
/// peg 穴が侵入しない reserved 領域 (`EDGE_MARGIN ≥ 18mm` 制約と対)
pub const SKADIS_OUTER_FRAME: f32 = 12.0;

// ────────────────────────────────────────────────────────
// SDF spec function
// ────────────────────────────────────────────────────────

/// SKADIS panel の `SdfNode` を生成する (千鳥 peg 穴付き)
///
/// # 引数
///
/// - `size`: 一辺 (mm、通常 300)
/// - `thickness`: 板厚 (mm、通常 5、[`SKADIS_PANEL_THICKNESS`])
/// - `corner_radius`: 外周角丸 R (mm、通常 5)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::skadis_sdf::{skadis_panel_sdf, SKADIS_PANEL_THICKNESS};
/// let panel = skadis_panel_sdf(300.0, SKADIS_PANEL_THICKNESS, 5.0);
/// // 純 SDF、DC 経路で watertight mesh 化推奨:
/// // alice_lol::print_export::node_to_3mf_dual_contouring(&panel, "panel.3mf", &config)
/// ```
///
/// # 検証
///
/// 本 SDF を MC (`node_to_3mf`) で mesh 化すると pipeline 実測相当の非多様体エッジが発生
/// DC (`node_to_3mf_dual_contouring`) で mesh 化すると Hermite data により watertight 保証
/// example `skadis_panel_dc_vs_mc.rs` で実測比較
#[must_use]
#[allow(clippy::too_many_lines)] // 1 枚の板の SDF 構成を直列に記述 (分割すると寸法の対応が追いにくい)
pub fn skadis_panel_sdf(size: f32, thickness: f32, corner_radius: f32) -> SdfNode {
    // 外形 (原点中心の RoundedBox、Y 軸方向 = 板厚)
    // 2026-08-08 fix: RoundedBox は 6 面全てに round_radius を追加する仕様のため、
    // 素朴に使うと Y 方向にも corner_radius が加算されて板厚が (thickness + 2*corner_radius)
    // になる (例: thickness=5, corner_radius=6 で Y=17mm、pipeline production 5mm と 3.4x 齟齬)
    // 対策: Y 方向のみ Box3d で cut して真の thickness に強制 (X/Z の 4 corner fillet は保持)
    // `RoundedBox` の外寸は `half_extents + round_radius` なので、X/Z は内側の寸法
    // `size/2 - R` を渡して外寸をちょうど `size` にする (旧実装は `size/2` を渡して一辺が
    // `size + 2R` になっていた)  Y は下の cutter で `thickness` に切り戻す
    let r = corner_radius.clamp(0.0, size * 0.5);
    let panel_infl = SdfNode::RoundedBox {
        half_extents: Vec3::new(
            size.mul_add(0.5, -r),
            thickness * 0.5,
            size.mul_add(0.5, -r),
        ),
        round_radius: r,
    };
    let y_cutter = SdfNode::Box3d {
        half_extents: Vec3::new(
            size.mul_add(0.5, 1.0), // X/Z は panel_infl 全体を包含 (fillet 保持)
            thickness * 0.5,        // Y は正確に thickness に制限
            size.mul_add(0.5, 1.0),
        ),
    };
    let panel = SdfNode::Intersection {
        a: Arc::new(panel_infl),
        b: Arc::new(y_cutter),
    };

    // Peg 穴 = Stadium 形状 (pipeline `SKADIS_SPEC.md` §1 準拠、5×15mm、round 2.5mm 半円 ends)
    // 2026-08-08 fix: 旧実装は Box3d rectangle だったが production は stadium (semicircular ends)
    // Stadium 構成 = 中央 Box (5 × T+2m × 10、Z 方向 10mm) + 端 Cylinder 2 個 (radius 2.5、Y 軸)
    let t_pass = 2.0f32.mul_add(HOLE_THROUGH_MARGIN, thickness);
    let stadium_middle_z = (SKADIS_PEG_H - SKADIS_PEG_W) * 0.5; // (15-5)/2 = 5mm
    let peg_middle = SdfNode::Box3d {
        half_extents: Vec3::new(SKADIS_PEG_W * 0.5, t_pass * 0.5, stadium_middle_z),
    };
    let peg_end_cyl = SdfNode::Cylinder {
        radius: SKADIS_PEG_W * 0.5, // 2.5mm = 半円 end
        half_height: t_pass * 0.5,
    };
    let peg_end_top = SdfNode::Translate {
        child: Arc::new(peg_end_cyl.clone()),
        offset: Vec3::new(0.0, 0.0, stadium_middle_z),
    };
    let peg_end_bot = SdfNode::Translate {
        child: Arc::new(peg_end_cyl),
        offset: Vec3::new(0.0, 0.0, -stadium_middle_z),
    };
    let peg_hole = SdfNode::Union {
        a: Arc::new(SdfNode::Union {
            a: Arc::new(peg_middle),
            b: Arc::new(peg_end_top),
        }),
        b: Arc::new(peg_end_bot),
    };

    // 穴の中心は使用可能範囲 |c| <= (size - 2 * EDGE_MARGIN) / 2 に収める
    // base 格子 (原点中心) と stagger 格子 (base を GRID_OFFSET ずらす) のそれぞれで、
    // 範囲に入る添字だけを並べる (旧実装は両方とも ix ∈ [-3, 3] で、stagger の +140 が余白に
    // 入り、-140 は無い左右非対称だった)
    let usable_half = size.mul_add(0.5, -SKADIS_EDGE_MARGIN);
    #[allow(clippy::cast_possible_truncation)]
    let index_range = |offset: f32| {
        let lo = ((-usable_half - offset) / SKADIS_GRID_PITCH).ceil() as i32;
        let hi = ((usable_half - offset) / SKADIS_GRID_PITCH).floor() as i32;
        lo..=hi
    };

    // 2026-08-07 fix: RepeatFinite → 明示 Union へ置換 (Phase 5.8 gridfinity 同 pattern)
    // 理由: RepeatFinite の distance field は要素間 bound-only 保証で exact metric ではない
    // DC の Hermite data sampling で boundary 精度不足による watertight 破綻 (NME 163) 発生
    // 明示 Union で真の distance field を得て DC で完全 watertight 達成 (NME 0 実測)
    //
    // 2026-08-07 fix 2: 線形左入れ子 fold → balanced fold で eval recursion depth 削減
    // (98-deep 線形 fold は test thread 2 MB stack を超過して stack overflow の実測 CI 事故)
    let mut hole_list: Vec<SdfNode> = Vec::new();
    for (grid_x, grid_z) in [(0.0, 0.0), (SKADIS_GRID_OFFSET, SKADIS_GRID_OFFSET)] {
        for ix in index_range(grid_x) {
            for iz in index_range(grid_z) {
                #[allow(clippy::cast_precision_loss)]
                let cx = (ix as f32).mul_add(SKADIS_GRID_PITCH, grid_x);
                #[allow(clippy::cast_precision_loss)]
                let cz = (iz as f32).mul_add(SKADIS_GRID_PITCH, grid_z);
                hole_list.push(SdfNode::Translate {
                    child: Arc::new(peg_hole.clone()),
                    offset: Vec3::new(cx, 0.0, cz),
                });
            }
        }
    }

    // 2026-08-08 add: Connector holes (M2.5 Ø2.7mm、4 辺 + 4 角、pipeline canonical 準拠)
    // production `python generator::get_conn_positions`
    // 板 origin=中央のため、Python `(x, y)` (板 origin 左下) を Rust `(cx, cz)` に座標変換:
    // cx = x - size/2、cz = y - size/2 (Python Y-up plane = Rust X-Z plane)
    let conn_cyl = SdfNode::Cylinder {
        radius: SKADIS_CONN_SCREW_D * 0.5, // 1.35mm
        half_height: t_pass * 0.5,
    };
    let half_size = size * 0.5;
    let conn_inset_from_center = half_size - SKADIS_CONN_INSET; // 中央から縁-inset 距離

    // 4 辺 (Python np.arange(GRID_PITCH, PANEL_W, GRID_PITCH) = 40, 80, ..., <size)
    let mut i = 1i32;
    loop {
        #[allow(clippy::cast_precision_loss)]
        let pos = i as f32 * SKADIS_GRID_PITCH;
        if pos >= size {
            break;
        }
        let center_shift = pos - half_size; // Python x → Rust cx

        // 上辺 (Y=CONN_INSET 相当 = Z=-conn_inset_from_center) と 下辺
        hole_list.push(SdfNode::Translate {
            child: Arc::new(conn_cyl.clone()),
            offset: Vec3::new(center_shift, 0.0, -conn_inset_from_center),
        });
        hole_list.push(SdfNode::Translate {
            child: Arc::new(conn_cyl.clone()),
            offset: Vec3::new(center_shift, 0.0, conn_inset_from_center),
        });
        // 左辺 / 右辺
        hole_list.push(SdfNode::Translate {
            child: Arc::new(conn_cyl.clone()),
            offset: Vec3::new(-conn_inset_from_center, 0.0, center_shift),
        });
        hole_list.push(SdfNode::Translate {
            child: Arc::new(conn_cyl.clone()),
            offset: Vec3::new(conn_inset_from_center, 0.0, center_shift),
        });
        i += 1;
    }
    // 4 角
    for &(sx, sz) in &[
        (-conn_inset_from_center, -conn_inset_from_center),
        (conn_inset_from_center, -conn_inset_from_center),
        (-conn_inset_from_center, conn_inset_from_center),
        (conn_inset_from_center, conn_inset_from_center),
    ] {
        hole_list.push(SdfNode::Translate {
            child: Arc::new(conn_cyl.clone()),
            offset: Vec3::new(sx, 0.0, sz),
        });
    }

    // 2026-08-08 add: Mount holes (Ø5mm 壁掛け、上下辺 3 穴、pipeline canonical 準拠)
    // production `get_mount_positions`: X=60/150/220 (peg X=20+40k と非重複)、Y=CONN_INSET/PANEL_H-CONN_INSET
    // 板 origin=中央: cx = x - size/2 (Python origin 左下)
    let mount_cyl = SdfNode::Cylinder {
        radius: SKADIS_MOUNT_HOLE_R, // 2.5mm
        half_height: t_pass * 0.5,
    };
    let mount_x_positions = [60.0, 150.0, 220.0];
    for &x in &mount_x_positions {
        let cx = x - half_size;
        // 上辺 (Python Y=CONN_INSET → Rust Z=-conn_inset_from_center)
        hole_list.push(SdfNode::Translate {
            child: Arc::new(mount_cyl.clone()),
            offset: Vec3::new(cx, 0.0, -conn_inset_from_center),
        });
        // 下辺
        hole_list.push(SdfNode::Translate {
            child: Arc::new(mount_cyl.clone()),
            offset: Vec3::new(cx, 0.0, conn_inset_from_center),
        });
    }

    let all_holes = super::balanced_union_fold(hole_list);

    // panel - holes
    match all_holes {
        Some(holes) => SdfNode::Subtraction {
            a: Arc::new(panel),
            b: Arc::new(holes),
        },
        None => panel,
    }
}

// ────────────────────────────────────────────────────────
// Phase 5.2: SKADIS 残 4 accessory SDF (container/clip/shelf/elastic_cord)
//
// pipeline Python `python generator`
// の実プリント合格 spec を SDF に近似移植 装飾 (fillet / 肉抜き穴 / テーパー) は省略、
// DC の Hermite で自然滑らか化 実プリント品質は Phase 5.6 user 検証で判定
// ────────────────────────────────────────────────────────

// ── container 定数 (pipeline skadis-container/generate.py 準拠) ──
/// container 内幅 (mm)
pub const CONTAINER_W: f32 = 65.0;
/// container 内奥行 (mm)
pub const CONTAINER_D: f32 = 75.0;
/// container 内高 (mm)
pub const CONTAINER_H: f32 = 70.0;
/// container 壁厚 (mm、4 perimeters)
pub const CONTAINER_WALL_T: f32 = 1.6;
/// container 底厚 (mm)
pub const CONTAINER_BOTTOM_T: f32 = 1.6;

/// SKADIS container SDF (小物入れ、2 peg、pipeline `skadis-container` 実プリント合格 spec)
///
/// 構造: 外形 Box3d - 内部 Box3d + 底 Box3d + 背面 ペグ 2 個
/// 装飾 (肉抜き穴 4 個 / R フィレット / ガセット補強) は省略 (近似実装)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::skadis_sdf::skadis_container_sdf;
/// let c = skadis_container_sdf();
/// // node_to_3mf_dual_contouring(&c, path, config) で 3MF 化推奨
/// ```
#[must_use]
pub fn skadis_container_sdf() -> SdfNode {
    let outer_w = 2.0f32.mul_add(CONTAINER_WALL_T, CONTAINER_W);
    let outer_d = 2.0f32.mul_add(CONTAINER_WALL_T, CONTAINER_D);
    let total_h = CONTAINER_H + CONTAINER_BOTTOM_T;

    // 外形 (X 幅 × Y 高 × Z 奥行)
    let outer = SdfNode::Box3d {
        half_extents: Vec3::new(outer_w * 0.5, total_h * 0.5, outer_d * 0.5),
    };
    // 内部 (刳り抜き、底より上、上面は開口 = Y=+total_h/2 + margin)
    // 空洞は床 (Y = -total_h/2 + 底厚) から上面の外 (+1mm の punch margin) まで
    let inner_half_h = f32::midpoint(CONTAINER_H, 1.0);
    let inner = SdfNode::Box3d {
        half_extents: Vec3::new(CONTAINER_W * 0.5, inner_half_h, CONTAINER_D * 0.5),
    };
    let inner_placed = SdfNode::Translate {
        child: Arc::new(inner),
        // 中心 y = 床の高さ + 半高 (旧実装は半高に H/2 を使い、底が 0.5mm 薄くなっていた)
        offset: Vec3::new(
            0.0,
            total_h.mul_add(-0.5, CONTAINER_BOTTOM_T) + inner_half_h,
            0.0,
        ),
    };
    let hollow = SdfNode::Subtraction {
        a: Arc::new(outer),
        b: Arc::new(inner_placed),
    };

    // 背面ペグ 2 個 (Z = -outer_d/2 の壁面の裏側へ BOARD_T 突き出す、Y = 上部 35%)
    // 2 個の間隔は SKADIS の格子 (GRID_PITCH = 40) = X = ±20
    let peg_at = |x: f32| SdfNode::Translate {
        child: Arc::new(peg_facing_back(PEG_BLADE_W)),
        offset: Vec3::new(x, total_h * 0.35, -outer_d * 0.5),
    };
    let pegs = SdfNode::Union {
        a: Arc::new(peg_at(-SKADIS_GRID_PITCH * 0.5)),
        b: Arc::new(peg_at(SKADIS_GRID_PITCH * 0.5)),
    };

    SdfNode::Union {
        a: Arc::new(hollow),
        b: Arc::new(pegs),
    }
}

// ── clip 定数 (pipeline skadis-clip/generate.py 準拠) ──
/// clip 横幅 (mm、pipeline `CLIP_WIDTH`)
pub const CLIP_WIDTH: f32 = 15.0;
/// clip 全長 (mm、ペグ下、pipeline `CLIP_LENGTH`)
pub const CLIP_LENGTH: f32 = 55.0;
/// clip 本体厚 (片側 mm)
pub const CLIP_BODY_T: f32 = 3.0;
/// clip スロット幅 (mm、紙 1-3 枚)
pub const CLIP_SLOT_W: f32 = 1.2;

/// SKADIS clip SDF (単 peg、差込 slot 式クリップ、pipeline `skadis-clip` 実プリント合格 spec)
///
/// 構造: 前板 + 背板 (`SLOT_W` 離間) + root 結合部 + 先端凸 + ペグ
/// PLA 弾性限界内で使う想定 (0.1kgf メモ・写真、バネなし)
#[must_use]
pub fn skadis_clip_sdf() -> SdfNode {
    let gap = CLIP_SLOT_W * 0.5;

    // root (peg 根元結合、Y = 上部、slot 全幅)
    let root = SdfNode::Box3d {
        half_extents: Vec3::new(f32::midpoint(gap, CLIP_BODY_T), 4.0, CLIP_WIDTH * 0.5),
    };
    let root_placed = SdfNode::Translate {
        child: Arc::new(root),
        offset: Vec3::new(gap * 0.5, PEG_BLADE_T * 0.25, 0.0),
    };
    // 前板 (Y = 下方向、CLIP_LENGTH)
    let front = SdfNode::Box3d {
        half_extents: Vec3::new(CLIP_BODY_T * 0.5, CLIP_LENGTH * 0.5, CLIP_WIDTH * 0.5),
    };
    let front_placed = SdfNode::Translate {
        child: Arc::new(front),
        offset: Vec3::new(CLIP_BODY_T.mul_add(0.5, gap), -CLIP_LENGTH * 0.5, 0.0),
    };
    // 背板 (対称位置)
    let back = SdfNode::Box3d {
        half_extents: Vec3::new(CLIP_BODY_T * 0.5, CLIP_LENGTH * 0.5, CLIP_WIDTH * 0.5),
    };
    let back_placed = SdfNode::Translate {
        child: Arc::new(back),
        offset: Vec3::new(CLIP_BODY_T.mul_add(-0.5, -gap), -CLIP_LENGTH * 0.5, 0.0),
    };
    // 先端凸 (保持力向上)
    let tip = SdfNode::Box3d {
        half_extents: Vec3::new(0.4, 2.5, CLIP_WIDTH * 0.5),
    };
    let tip_placed = SdfNode::Translate {
        child: Arc::new(tip),
        offset: Vec3::new(gap + 0.1, -CLIP_LENGTH + 2.5, 0.0),
    };
    // ペグ
    let peg = skadis_peg_and_shoulder(CLIP_WIDTH);

    let body_upper = SdfNode::Union {
        a: Arc::new(root_placed),
        b: Arc::new(peg),
    };
    let body_front = SdfNode::Union {
        a: Arc::new(body_upper),
        b: Arc::new(front_placed),
    };
    let body_back = SdfNode::Union {
        a: Arc::new(body_front),
        b: Arc::new(back_placed),
    };
    SdfNode::Union {
        a: Arc::new(body_back),
        b: Arc::new(tip_placed),
    }
}

// ── shelf 定数 (pipeline skadis-shelf/generate.py 準拠) ──
/// shelf 幅 (mm、W 方向、6 grid × 40mm)
pub const SHELF_W: f32 = 260.0;
/// shelf 奥行 (mm)
pub const SHELF_D: f32 = 80.0;
/// shelf lip 高 (前面リップ mm)
pub const SHELF_LIP_H: f32 = 20.0;
/// shelf 背面高 (mm)
pub const SHELF_BACK_H: f32 = 25.0;
/// shelf 底厚 (mm、曲げ計算 1.5 + マージン)
pub const SHELF_BOTTOM_T: f32 = 2.0;
/// shelf ペグ間隔 (mm、6 × `GRID_PITCH`)
pub const SHELF_PEG_SPACING: f32 = 240.0;

/// SKADIS shelf SDF (2 peg 棚、pipeline `skadis-shelf` 実プリント合格 spec)
///
/// 構造: U 字断面 (底 + 背 + 前リップ) を W 方向に extrude + 2 peg (両端)
/// 底面リブ 3 本は省略 (近似実装、DC で watertight 保証)
#[must_use]
pub fn skadis_shelf_sdf() -> SdfNode {
    // 底板 (X = W 方向、Y = 底厚、Z = 奥行)
    let bottom = SdfNode::Box3d {
        half_extents: Vec3::new(SHELF_W * 0.5, SHELF_BOTTOM_T * 0.5, SHELF_D * 0.5),
    };
    // 背面 (Y = back_h、Z = -SHELF_D/2 位置、厚 = wall_t)
    let back = SdfNode::Box3d {
        half_extents: Vec3::new(SHELF_W * 0.5, SHELF_BACK_H * 0.5, 1.6 * 0.5),
    };
    let back_placed = SdfNode::Translate {
        child: Arc::new(back),
        offset: Vec3::new(0.0, SHELF_BACK_H * 0.5, (-SHELF_D).mul_add(0.5, 0.8)),
    };
    // 前リップ
    let lip = SdfNode::Box3d {
        half_extents: Vec3::new(SHELF_W * 0.5, SHELF_LIP_H * 0.5, 1.6 * 0.5),
    };
    let lip_placed = SdfNode::Translate {
        child: Arc::new(lip),
        offset: Vec3::new(0.0, SHELF_LIP_H * 0.5, SHELF_D.mul_add(0.5, -0.8)),
    };
    // 2 ペグ (両端、SHELF_PEG_SPACING 離間)
    let peg = peg_facing_back(PEG_BLADE_W);
    let peg_l = SdfNode::Translate {
        child: Arc::new(peg.clone()),
        offset: Vec3::new(-SHELF_PEG_SPACING * 0.5, SHELF_BACK_H * 0.7, -SHELF_D * 0.5),
    };
    let peg_r = SdfNode::Translate {
        child: Arc::new(peg),
        offset: Vec3::new(SHELF_PEG_SPACING * 0.5, SHELF_BACK_H * 0.7, -SHELF_D * 0.5),
    };

    let step1 = SdfNode::Union {
        a: Arc::new(bottom),
        b: Arc::new(back_placed),
    };
    let step2 = SdfNode::Union {
        a: Arc::new(step1),
        b: Arc::new(lip_placed),
    };
    let step3 = SdfNode::Union {
        a: Arc::new(step2),
        b: Arc::new(peg_l),
    };
    SdfNode::Union {
        a: Arc::new(step3),
        b: Arc::new(peg_r),
    }
}

// ── elastic_cord 定数 ──
/// `elastic_cord` 本体厚 (mm、曲げ計算 2.3 + マージン)
pub const ELASTIC_CORD_BODY_T: f32 = 3.0;
/// `elastic_cord` peg 間隔 (mm、pipeline `GRID_PITCH`)
pub const ELASTIC_CORD_PEG_PITCH: f32 = 40.0;
/// `elastic_cord` フック突出 (mm)
pub const ELASTIC_CORD_HOOK_REACH: f32 = 12.0;
/// `elastic_cord` フック R (mm、折れ防止)
pub const ELASTIC_CORD_HOOK_R: f32 = 3.0;

/// SKADIS elastic cord holder SDF (2 peg 上下、pipeline `skadis-elastic-cord` 実プリント合格 spec)
///
/// 構造: 上下 2 ペグ (`GRID_PITCH` 離間) + 縦背骨 + 2 hook (上下対称、R カーブ)
/// バンド溝は省略 (近似実装)
#[must_use]
pub fn skadis_elastic_cord_sdf() -> SdfNode {
    let ht = ELASTIC_CORD_BODY_T * 0.5;
    let hook_width = 8.0; // hook 幅 (Z 方向)

    // 上ペグ (Y = 0)
    let peg_top = skadis_peg_and_shoulder(hook_width);
    // 下ペグ (Y = -GRID_PITCH)
    let peg_bot = SdfNode::Translate {
        child: Arc::new(skadis_peg_and_shoulder(hook_width)),
        offset: Vec3::new(0.0, -ELASTIC_CORD_PEG_PITCH, 0.0),
    };

    // 背骨 (Y = -pitch/2 中心、高 = pitch + 8mm、X = ht 位置、厚 = ht)
    let spine = SdfNode::Box3d {
        half_extents: Vec3::new(ht * 0.5, f32::midpoint(ELASTIC_CORD_PEG_PITCH, 8.0), 4.0),
    };
    let spine_placed = SdfNode::Translate {
        child: Arc::new(spine),
        offset: Vec3::new(ht * 0.5, -ELASTIC_CORD_PEG_PITCH * 0.5, 0.0),
    };

    // 上フック (前方突出、R カーブを Capsule 連結で近似)
    let mut top_pts: Vec<glam::Vec2> = vec![glam::Vec2::new(ht, -8.0)];
    let n = 12;
    for i in 1..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        top_pts.push(glam::Vec2::new(
            (ELASTIC_CORD_HOOK_REACH - ELASTIC_CORD_HOOK_R).mul_add(a.sin(), ht),
            ELASTIC_CORD_HOOK_R.mul_add(-(1.0 - a.cos()), -8.0),
        ));
    }
    let top_hook = capsule_polyline_sdf(&top_pts, ht, hook_width);

    // 下フック (Y = -pitch + 8、上下対称、上向きに曲がる)
    let mut bot_pts: Vec<glam::Vec2> = vec![glam::Vec2::new(ht, -ELASTIC_CORD_PEG_PITCH + 8.0)];
    for i in 1..=n {
        #[allow(clippy::cast_precision_loss)]
        let a = std::f32::consts::FRAC_PI_2 * i as f32 / n as f32;
        bot_pts.push(glam::Vec2::new(
            (ELASTIC_CORD_HOOK_REACH - ELASTIC_CORD_HOOK_R).mul_add(a.sin(), ht),
            ELASTIC_CORD_HOOK_R.mul_add(1.0 - a.cos(), -ELASTIC_CORD_PEG_PITCH + 8.0),
        ));
    }
    let bot_hook = capsule_polyline_sdf(&bot_pts, ht, hook_width);

    let step1 = SdfNode::Union {
        a: Arc::new(peg_top),
        b: Arc::new(peg_bot),
    };
    let step2 = SdfNode::Union {
        a: Arc::new(step1),
        b: Arc::new(spine_placed),
    };
    let step3 = SdfNode::Union {
        a: Arc::new(step2),
        b: Arc::new(top_hook),
    };
    SdfNode::Union {
        a: Arc::new(step3),
        b: Arc::new(bot_hook),
    }
}

// ────────────────────────────────────────────────────────
// テスト
// ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use alice_sdf::eval;

    #[test]
    fn skadis_panel_sdf_returns_subtraction() {
        let panel = skadis_panel_sdf(300.0, 5.0, 5.0);
        assert!(matches!(panel, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn skadis_panel_material_region_is_inside() {
        // 板中央 (0, 0, 0) は peg 穴 (0, 0, 0) の中心 なので 実は 穴内部
        // 材料部分は grid 外 (X, Z が pitch と一致しない位置)
        // 例: (10, 0, 10) は peg (0,0,0) の +10 = grid pitch 40 の中間 = 材料内部
        let panel = skadis_panel_sdf(300.0, 5.0, 5.0);
        assert!(eval(&panel, Vec3::new(10.0, 0.0, 10.0)) < 0.0);
    }

    #[test]
    fn skadis_panel_peg_center_is_hole() {
        // (0, 0, 0) は base grid の中心 peg = 物質外 (穴内)
        let panel = skadis_panel_sdf(300.0, 5.0, 5.0);
        assert!(eval(&panel, Vec3::ZERO) > 0.0);
    }

    #[test]
    fn skadis_panel_stagger_peg_position() {
        // stagger grid の中心 peg = (20, 0, 20) = 物質外
        let panel = skadis_panel_sdf(300.0, 5.0, 5.0);
        assert!(eval(&panel, Vec3::new(20.0, 0.0, 20.0)) > 0.0);
    }

    #[test]
    fn skadis_panel_outside_boundary() {
        // 外形外 (X=200 = size/2 + margin 外) は空間
        let panel = skadis_panel_sdf(300.0, 5.0, 5.0);
        assert!(eval(&panel, Vec3::new(200.0, 0.0, 0.0)) > 0.0);
    }

    // ── 2026-08-08 fix: Y 厚さ bound + Stadium peg hole の regression tests ──

    #[test]
    fn skadis_panel_y_thickness_bounded_at_thickness_half() {
        // 板厚方向 Y は正確に ±thickness/2 に制限 (RoundedBox の 6 面 inflate bug 対策)
        // thickness=5、corner_radius=6 で旧実装は Y=±8.5 (17mm 厚) → production 齟齬
        // 修正後は Y=±2.5 (5mm 厚) に強制
        let panel = skadis_panel_sdf(300.0, 5.0, 6.0);
        // Y = 3.0mm (thickness/2=2.5 の外) は空間
        assert!(
            eval(&panel, Vec3::new(10.0, 3.0, 10.0)) > 0.0,
            "Y=3mm は thickness/2=2.5mm を超えるので空間になるはず"
        );
        // Y = 2.0mm (thickness/2=2.5 の内) + XZ 材料内位置は material 内部
        assert!(
            eval(&panel, Vec3::new(10.0, 2.0, 10.0)) < 0.0,
            "Y=2mm + XZ=(10,10) は material 内部になるはず"
        );
    }

    #[test]
    fn skadis_panel_xz_corner_fillet_preserved() {
        // X/Z 4 corner の fillet は保持される (corner_radius=6 が X/Z に効く)
        // panel size=300、corner_radius=6 → 実効 X/Z 範囲 = ±156 (=150+6)
        let panel = skadis_panel_sdf(300.0, 5.0, 6.0);
        // 完全角 (156, 0, 156) は fillet で外側 (fillet 内側は距離 > 0)
        assert!(
            eval(&panel, Vec3::new(156.0, 0.0, 156.0)) > 0.0,
            "コーナー (156, 0, 156) は fillet で切り取られるので空間になるはず"
        );
        // fillet の外中央 (156, 0, 0) は境界近傍 (実効的にほぼ 0)
        // Panel 中の material 位置 (110, 0, 110) は material 内部
        // (100, 0, 100) は stagger grid peg (20+40*2) 中心なので穴、避ける
        assert!(
            eval(&panel, Vec3::new(110.0, 0.0, 110.0)) < 0.0,
            "(110, 0, 110) は grid 外の material 内部になるはず"
        );
    }

    #[test]
    fn skadis_panel_connector_holes_are_present() {
        // 2026-08-08 add: connector 穴 (M2.5 Ø2.7mm) は 4 辺 + 4 角に存在
        // panel origin=中央、conn_inset_from_center = 150 - 6 = 144mm
        let panel = skadis_panel_sdf(300.0, 5.0, 6.0);
        // 4 角: (±144, 0, ±144) 全部空間 (穴)
        for &(sx, sz) in &[
            (-144.0f32, -144.0),
            (144.0, -144.0),
            (-144.0, 144.0),
            (144.0, 144.0),
        ] {
            assert!(
                eval(&panel, Vec3::new(sx, 0.0, sz)) > 0.0,
                "コーナー connector 穴 ({sx}, 0, {sz}) は空間になるはず"
            );
        }
        // 上辺 conn: X=40 (Python x=190 → cx=40)、Z=-144
        // Python `arange(40, 300, 40)` = [40, 80, 120, 160, 200, 240, 280]
        // Rust cx = x - 150 = [-110, -70, -30, 10, 50, 90, 130]
        // 代表 3 位置検証
        for &cx in &[-110.0f32, 10.0, 130.0] {
            assert!(
                eval(&panel, Vec3::new(cx, 0.0, -144.0)) > 0.0,
                "上辺 connector 穴 ({cx}, 0, -144) は空間になるはず"
            );
        }
    }

    #[test]
    fn skadis_panel_mount_holes_are_present() {
        // 2026-08-08 add: mount 穴 (Ø5mm) は上下辺の X=60/150/220 (Python) = cx=-90/0/70 (Rust)
        // Z = ±(size/2 - CONN_INSET) = ±144
        let panel = skadis_panel_sdf(300.0, 5.0, 6.0);
        for &cx in &[-90.0f32, 0.0, 70.0] {
            // 上辺 (Z=-144)
            assert!(
                eval(&panel, Vec3::new(cx, 0.0, -144.0)) > 0.0,
                "上辺 mount 穴 ({cx}, 0, -144) は空間になるはず"
            );
            // 下辺 (Z=+144)
            assert!(
                eval(&panel, Vec3::new(cx, 0.0, 144.0)) > 0.0,
                "下辺 mount 穴 ({cx}, 0, 144) は空間になるはず"
            );
        }
    }

    #[test]
    fn skadis_panel_frame_zone_material_between_holes() {
        // frame 領域 (X=144 = size/2 - CONN_INSET 近辺) で穴と穴の間は material
        // 上辺 Z=-144、X=-140 (連続 conn 穴の中間) → material 内部
        // Python conn X=[190]=cx=40 vs mount X=[60,150,220]=cx=[-90,0,70]
        // cx=-140 は最も近い穴が corner (-144, ±144) 約 4mm 離、mount cx=-90 は 50mm 離
        // 穴半径 max=2.5mm (mount) なので cx=-140 は material 内
        let panel = skadis_panel_sdf(300.0, 5.0, 6.0);
        assert!(
            eval(&panel, Vec3::new(-140.0, 0.0, -140.0)) < 0.0,
            "frame 内 穴なし位置 (-140, 0, -140) は material 内部になるはず"
        );
    }

    #[test]
    fn skadis_panel_peg_hole_stadium_shape() {
        // Stadium peg 穴 5×15mm、round 2.5mm ends
        // 基準 peg は (0, 0, 0) 中心、X 幅 ±2.5、Z 高 ±7.5
        let panel = skadis_panel_sdf(300.0, 5.0, 6.0);
        // Z = 6.0mm (Z=±7.5 の semicircle end 内) + X=0 → stadium 内部 = 穴
        assert!(
            eval(&panel, Vec3::new(0.0, 0.0, 6.0)) > 0.0,
            "(0, 0, 6.0) は stadium 半円 end 内なので穴になるはず"
        );
        // Z = 8.0mm (Z=±7.5 の semicircle end 外) → 穴外、material 内部
        assert!(
            eval(&panel, Vec3::new(0.0, 0.0, 8.0)) < 0.0,
            "(0, 0, 8.0) は stadium 外側で material 内部になるはず"
        );
        // X = 3.0mm (X=±2.5 の外) → stadium 外、material 内部
        // ただし Z 位置に注意: (3, 0, 0) は peg 穴 X 幅の外だが Z 中央 → 内側判定は shape 次第
        // 旧 rectangle なら X=3 > X=±2.5 = 外、Stadium も同じ
        assert!(
            eval(&panel, Vec3::new(3.0, 0.0, 0.0)) < 0.0,
            "(3, 0, 0) は stadium X 幅 (±2.5) の外なので material 内部になるはず"
        );
    }

    #[test]
    fn skadis_peg_and_shoulder_returns_union() {
        let ps = skadis_peg_and_shoulder(8.0);
        assert!(matches!(ps, SdfNode::Union { .. }));
    }

    #[test]
    fn capsule_polyline_short_input_returns_sphere() {
        let empty: Vec<glam::Vec2> = vec![glam::Vec2::ZERO];
        let s = capsule_polyline_sdf(&empty, 1.0, 5.0);
        assert!(matches!(s, SdfNode::Sphere { .. }));
    }

    #[test]
    fn skadis_hook_l_body_returns_union() {
        let h = skadis_hook_l_sdf();
        assert!(matches!(h, SdfNode::Union { .. }));
    }

    #[test]
    fn skadis_hook_j_and_s_return_union() {
        let j = skadis_hook_j_sdf();
        let s = skadis_hook_s_sdf();
        assert!(matches!(j, SdfNode::Union { .. }));
        assert!(matches!(s, SdfNode::Union { .. }));
    }

    #[test]
    fn hook_peg_area_is_inside_material() {
        // hook 3 種の peg 部 (X ≈ -2.5, Y=0, Z=0) は材料内部
        // peg blade 中心 X = -BOARD_T/2 = -2.5
        for hook in [
            skadis_hook_l_sdf(),
            skadis_hook_j_sdf(),
            skadis_hook_s_sdf(),
        ] {
            assert!(eval(&hook, Vec3::new(-2.5, 0.0, 0.0)) < 0.0);
        }
    }

    // ── Phase 5.2 accessory 4 種の tests ──

    #[test]
    fn container_returns_union() {
        let c = skadis_container_sdf();
        assert!(matches!(c, SdfNode::Union { .. }));
    }

    #[test]
    fn clip_returns_union() {
        let c = skadis_clip_sdf();
        assert!(matches!(c, SdfNode::Union { .. }));
    }

    #[test]
    fn shelf_returns_union() {
        let s = skadis_shelf_sdf();
        assert!(matches!(s, SdfNode::Union { .. }));
    }

    #[test]
    fn elastic_cord_returns_union() {
        let e = skadis_elastic_cord_sdf();
        assert!(matches!(e, SdfNode::Union { .. }));
    }

    #[test]
    fn all_phase_5_2_accessories_produce_finite_sdf() {
        let nodes = [
            skadis_container_sdf(),
            skadis_clip_sdf(),
            skadis_shelf_sdf(),
            skadis_elastic_cord_sdf(),
        ];
        for (i, n) in nodes.iter().enumerate() {
            let d = eval(n, Vec3::new(0.1, 0.1, 0.1));
            assert!(d.is_finite(), "accessory {i}: non-finite SDF");
        }
    }
}
