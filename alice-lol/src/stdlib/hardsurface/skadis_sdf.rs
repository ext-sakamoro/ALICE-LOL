//! # skadis_sdf — SKADIS panel の純 SDF 表現 (Phase 3''、ALICE way 回帰)
//!
//! Phase A.5.2 で `thin::skadis_panel_2d` を追加した (Polygon2D + earcutr 経路)、しかし
//! これは ALICE 三相原理 Phase 1 Data 相当 = ALICE 違反であることが Phase B.1.c 議論で
//! 判明 ([[feedback_alice_polygon_extrude_data_route]])
//!
//! 本 module は SKADIS panel を **純 SDF (SdfNode)** で表現する Phase 2 Law 経路
//! mesh 化は `alice_lol::print_export::node_to_3mf_dual_contouring` (SDF+DC) 経由
//! Marching Cubes の非多様体多発問題は Dual Contouring の Hermite data で解決
//!
//! ## SKADIS panel SDF spec (Bamboo `formulas::skadis` 準拠)
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
//! - Bamboo 実測「SDF+MC で 6177 non-manifold edges」を DC が回避できるか実証

use alice_sdf::SdfNode;
use glam::Vec3;
use std::sync::Arc;

// ────────────────────────────────────────────────────────
// SKADIS 定数 (Bamboo `formulas::skadis` と同期)
// ────────────────────────────────────────────────────────

/// SKADIS peg 幅 (mm、Bamboo `PEG_W`)
pub const SKADIS_PEG_W: f32 = 5.0;

/// SKADIS peg 高 (mm、Bamboo `PEG_H`)
pub const SKADIS_PEG_H: f32 = 15.0;

/// SKADIS grid pitch (mm、Bamboo `GRID_PITCH`)
pub const SKADIS_GRID_PITCH: f32 = 40.0;

/// SKADIS grid offset (mm、千鳥、Bamboo `GRID_OFFSET`)
pub const SKADIS_GRID_OFFSET: f32 = 20.0;

/// SKADIS edge margin (mm、Bamboo `EDGE_MARGIN`)
pub const SKADIS_EDGE_MARGIN: f32 = 20.0;

/// SKADIS panel 標準厚 (mm、実プリント検証済)
pub const SKADIS_PANEL_THICKNESS: f32 = 5.0;

/// 貫通穴 depth margin (mm、subtract 用に peg 穴を板より少し長く取る)
pub const HOLE_THROUGH_MARGIN: f32 = 0.5;

// ────────────────────────────────────────────────────────
// SDF spec function
// ────────────────────────────────────────────────────────

/// SKADIS panel の SdfNode を生成する (千鳥 peg 穴付き)
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
/// 本 SDF を MC (`node_to_3mf`) で mesh 化すると Bamboo 実測相当の非多様体エッジが発生
/// DC (`node_to_3mf_dual_contouring`) で mesh 化すると Hermite data により watertight 保証
/// example `skadis_panel_dc_vs_mc.rs` で実測比較
#[must_use]
pub fn skadis_panel_sdf(size: f32, thickness: f32, corner_radius: f32) -> SdfNode {
    // 外形 (原点中心の RoundedBox、Y 軸方向 = 板厚)
    let panel = SdfNode::RoundedBox {
        half_extents: Vec3::new(size * 0.5, thickness * 0.5, size * 0.5),
        round_radius: corner_radius,
    };

    // Peg 穴 (単一)、Y 軸貫通、板厚方向に margin 付き
    let peg_hole = SdfNode::Box3d {
        half_extents: Vec3::new(
            SKADIS_PEG_W * 0.5,
            thickness * 0.5 + HOLE_THROUGH_MARGIN,
            SKADIS_PEG_H * 0.5,
        ),
    };

    // grid count: 原点中心で ±count 個 (実出力 2*count+1) を pitch で並べる
    // 使用可能範囲 = size - 2 * EDGE_MARGIN、その範囲を pitch で割った half を count に
    let usable = size - 2.0 * SKADIS_EDGE_MARGIN;
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    let count = ((usable * 0.5) / SKADIS_GRID_PITCH).floor() as u32;

    // Base grid (原点中心)
    let base_grid = SdfNode::RepeatFinite {
        child: Arc::new(peg_hole.clone()),
        count: [count, 0, count],
        spacing: Vec3::new(SKADIS_GRID_PITCH, 1.0, SKADIS_GRID_PITCH),
    };

    // Stagger grid (原点から (offset, 0, offset) シフト)
    let stagger_grid_raw = SdfNode::RepeatFinite {
        child: Arc::new(peg_hole),
        count: [count, 0, count],
        spacing: Vec3::new(SKADIS_GRID_PITCH, 1.0, SKADIS_GRID_PITCH),
    };
    let stagger_grid = SdfNode::Translate {
        child: Arc::new(stagger_grid_raw),
        offset: Vec3::new(SKADIS_GRID_OFFSET, 0.0, SKADIS_GRID_OFFSET),
    };

    // 2 grid Union で全 peg 穴
    let all_holes = SdfNode::Union {
        a: Arc::new(base_grid),
        b: Arc::new(stagger_grid),
    };

    // panel - holes
    SdfNode::Subtraction {
        a: Arc::new(panel),
        b: Arc::new(all_holes),
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
}
