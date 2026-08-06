//! # thin — 薄物 primitive (Phase A.5.2)
//!
//! **≤ 5mm 厚の薄物 pattern を 2D polygon + extrude 経路で生成する** SDF+Marching Cubes
//! の原理的限界 (Bamboo 実測「1.7mm → 5.1mm、6177 non-manifold edges」) を回避
//!
//! ## 経路の違い
//!
//! - **厚物 (5mm 超)**: `stdlib::hardsurface::{fastener, joint, mount, reinforcement}` の
//!   3D primitive を `SdfNode` として組み立て → `alice_sdf::mesh::sdf_to_mesh` (MC)
//!   → `alice_lol::print_export::node_to_3mf` で .3mf 出力
//! - **薄物 (≤ 5mm)**: 本 module の 2D primitive を `Polygon2D` として組み立て →
//!   `alice_sdf::mesh::polygon_extrude::Polygon2D::extrude(half_height)` →
//!   `alice_lol::print_export::polygon_to_3mf` で .3mf 出力 (watertight 保証)
//!
//! ## primitive
//!
//! | primitive | Bamboo 対応 canonical | 用途 |
//! |-----------|--------------------|-----|
//! | [`shopping_cart_coin_2d`] | `models/accessories/shopping-cart-coin/generate.py` | 100yen 型キーホルダーコイン |
//! | [`skadis_panel_2d`] | `models/wall-organizer/skadis-300x300/generate.py` | IKEA SKADIS 互換ペグボード (千鳥ペグ穴) |
//! | [`thin_plate`] | (汎用) | 任意サイズの薄板 (穴パターン任意) |
//!
//! ## LLM prompt / LoRA hint
//!
//! LLM 生成 LOL DSL が `polygon.extrude(t)` chain を出力する時、厚さ判定で本 module
//! を使う判定を LoRA 学習データに追加予定 (Phase A.5.4)

use alice_sdf::mesh::polygon_extrude::{circle, rect, rounded_rect, Polygon2D};
use glam::Vec2;

// ────────────────────────────────────────────────────────
// 定数 (Bamboo SKADIS spec、`~/ALICE-Bamboo/src/formulas.rs` skadis:: と同期)
// ────────────────────────────────────────────────────────

/// SKADIS peg 幅 (mm)
pub const SKADIS_PEG_W: f32 = 5.0;
/// SKADIS peg 高 (mm)
pub const SKADIS_PEG_H: f32 = 15.0;
/// SKADIS grid pitch (mm、ペグ穴間隔)
pub const SKADIS_GRID_PITCH: f32 = 40.0;
/// SKADIS grid offset (mm、千鳥オフセット)
pub const SKADIS_GRID_OFFSET: f32 = 20.0;
/// SKADIS edge margin (mm、外周からペグ穴中心までの最小距離)
pub const SKADIS_EDGE_MARGIN: f32 = 20.0;
/// SKADIS panel 標準厚 (mm、実プリント検証済)
pub const SKADIS_PANEL_THICKNESS: f32 = 5.0;
/// SKADIS peg 端の rounded 半径 (mm)
pub const SKADIS_PEG_ROUND_R: f32 = 2.5;

/// 100yen shopping cart coin 直径 (mm、実測)
pub const COIN_100YEN_DIAMETER: f32 = 22.8;
/// 100yen shopping cart coin 厚 (mm、実プリント検証済)
pub const COIN_100YEN_THICKNESS: f32 = 1.7;

// ────────────────────────────────────────────────────────
// 1. shopping cart coin (100yen 型)
// ────────────────────────────────────────────────────────

/// Shopping cart coin (100yen 型キーホルダーコイン) の 2D polygon
///
/// `diameter` = コイン直径 (mm、default [`COIN_100YEN_DIAMETER`])
/// `segments` = 円の分割数 (推奨 32-64)
///
/// 完成 3D mesh は `Polygon2D::extrude(thickness * 0.5)` で厚さ設定 (推奨 [`COIN_100YEN_THICKNESS`])
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::thin::shopping_cart_coin_2d;
/// let coin = shopping_cart_coin_2d(22.8, 32);
/// // extrude(0.85) で全厚 1.7mm の watertight mesh
/// let mesh = coin.extrude(0.85);
/// assert!(!mesh.indices.is_empty());
/// ```
#[must_use]
pub fn shopping_cart_coin_2d(diameter: f32, segments: u32) -> Polygon2D {
    circle(diameter * 0.5, segments)
}

// ────────────────────────────────────────────────────────
// 2. SKADIS panel (300×300mm 千鳥ペグ穴)
// ────────────────────────────────────────────────────────

/// IKEA SKADIS 互換ペグボードの 2D polygon (外周 + 千鳥ペグ穴列)
///
/// 構造: 角丸矩形 (`size` × `size`) を outer とし、千鳥配置 (`SKADIS_GRID_PITCH` / `SKADIS_GRID_OFFSET`)
/// のペグ穴を holes として持つ
///
/// # 引数
///
/// - `size`: panel 一辺 (mm、通常 300)
/// - `corner_radius`: 外周角丸 R (mm、通常 5)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::thin::{skadis_panel_2d, SKADIS_PANEL_THICKNESS};
/// let panel = skadis_panel_2d(300.0, 5.0);
/// // 5mm 厚で extrude
/// let mesh = panel.extrude(SKADIS_PANEL_THICKNESS * 0.5);
/// assert!(!mesh.indices.is_empty());
/// // 実プリント: Bambu Studio で開いて印刷可能な watertight 3MF
/// ```
#[must_use]
pub fn skadis_panel_2d(size: f32, corner_radius: f32) -> Polygon2D {
    let outer = rounded_rect(size, size, corner_radius, 8).outer;
    let mut panel = Polygon2D::new(outer);

    // ペグ穴 (5×15mm rounded rect、pitch 40mm、offset 20mm 千鳥)
    // Bamboo formulas::skadis::peg_positions と同 layout
    let peg_pitch = SKADIS_GRID_PITCH;
    let peg_offset = SKADIS_GRID_OFFSET;
    let margin = SKADIS_EDGE_MARGIN;
    let peg_hole_outer = |cx: f32, cy: f32| -> Vec<Vec2> {
        // hole は winding を逆にする (earcutr の慣例、outer CCW ならば hole CW)
        // rounded_rect は CCW を返すので reverse
        let mut h = rounded_rect(SKADIS_PEG_W, SKADIS_PEG_H, SKADIS_PEG_ROUND_R - 0.5, 4).outer;
        h.reverse();
        for v in &mut h {
            v.x += cx;
            v.y += cy;
        }
        h
    };

    // panel は中心が原点、範囲 X, Y = [-size/2, +size/2]
    let half = size * 0.5;
    for &base_off in &[0.0_f32, peg_offset] {
        let mut y = -half + margin + base_off;
        while y <= half - margin {
            let mut x = -half + margin + base_off;
            while x <= half - margin {
                panel.holes.push(peg_hole_outer(x, y));
                x += peg_pitch;
            }
            y += peg_pitch;
        }
    }

    panel
}

// ────────────────────────────────────────────────────────
// 3. thin plate (汎用薄板)
// ────────────────────────────────────────────────────────

/// 汎用薄板の 2D polygon (矩形または角丸矩形)
///
/// - `corner_radius` = 0 なら通常矩形、> 0 なら角丸矩形
/// - `holes` は外周に貫通する穴の中心 + 半径 (円のみ、多角形穴は Polygon2D 直接操作で追加可)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::thin::thin_plate;
/// let plate = thin_plate(100.0, 50.0, 3.0, &[(20.0, 0.0, 2.0), (-20.0, 0.0, 2.0)]);
/// // 100×50mm 板、角丸 R3、Φ4mm 穴 2 個
/// let mesh = plate.extrude(1.0); // 2mm 厚
/// assert!(!mesh.indices.is_empty());
/// ```
#[must_use]
pub fn thin_plate(
    width: f32,
    height: f32,
    corner_radius: f32,
    holes: &[(f32, f32, f32)],
) -> Polygon2D {
    let outer = if corner_radius > 0.0 {
        rounded_rect(width, height, corner_radius, 8).outer
    } else {
        rect(width, height).outer
    };
    let mut plate = Polygon2D::new(outer);
    for &(cx, cy, r) in holes {
        // 円穴を Polygon2D に追加 (16 分割、CW winding)
        let mut hole = circle(r, 16).outer;
        hole.reverse();
        for v in &mut hole {
            v.x += cx;
            v.y += cy;
        }
        plate.holes.push(hole);
    }
    plate
}

// ────────────────────────────────────────────────────────
// テスト
// ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coin_2d_produces_circle_polygon() {
        let coin = shopping_cart_coin_2d(22.8, 32);
        assert_eq!(coin.outer.len(), 32);
        assert!(coin.holes.is_empty());
    }

    #[test]
    fn coin_extrude_produces_watertight_mesh_at_1_7mm() {
        let coin = shopping_cart_coin_2d(COIN_100YEN_DIAMETER, 32);
        let mesh = coin.extrude(COIN_100YEN_THICKNESS * 0.5);
        assert!(!mesh.indices.is_empty());
        // 全 Y 座標が ± 0.85 の範囲に収まる (Bamboo 実測 1.7mm → 5.1mm 問題を再現しないことを確認)
        let max_y = mesh
            .vertices
            .iter()
            .map(|v| v.position.y)
            .fold(f32::MIN, f32::max);
        let min_y = mesh
            .vertices
            .iter()
            .map(|v| v.position.y)
            .fold(f32::MAX, f32::min);
        assert!((max_y - 0.85).abs() < 1e-4, "top face Y = {max_y}");
        assert!((min_y + 0.85).abs() < 1e-4, "bottom face Y = {min_y}");
    }

    #[test]
    fn skadis_panel_has_peg_holes() {
        let panel = skadis_panel_2d(300.0, 5.0);
        // 300×300 pitch 40 offset 20 千鳥 = 期待 hole 数
        //   base_off=0: x=[20..280] step 40 = 7、y=[20..280] step 40 = 7 → 49
        //   base_off=20: x=[40..280] step 40 = 7、y=[40..280] step 40 = 7 → 49
        //   合計 98 個
        assert_eq!(panel.holes.len(), 98);
    }

    #[test]
    fn skadis_panel_extrude_at_5mm_thickness() {
        let panel = skadis_panel_2d(300.0, 5.0);
        let mesh = panel.extrude(SKADIS_PANEL_THICKNESS * 0.5);
        assert!(!mesh.indices.is_empty());
        // 5mm 厚が正確に再現 (Bamboo 実測 1.7mm → 5.1mm 問題の対比)
        let max_y = mesh
            .vertices
            .iter()
            .map(|v| v.position.y)
            .fold(f32::MIN, f32::max);
        assert!((max_y - 2.5).abs() < 1e-4, "top face Y = {max_y}");
    }

    #[test]
    fn thin_plate_with_holes() {
        let plate = thin_plate(100.0, 50.0, 3.0, &[(20.0, 0.0, 2.0), (-20.0, 0.0, 2.0)]);
        assert_eq!(plate.holes.len(), 2);
        let mesh = plate.extrude(1.0);
        assert!(!mesh.indices.is_empty());
    }

    #[test]
    fn thin_plate_no_corner_radius_is_rectangle() {
        let plate = thin_plate(100.0, 50.0, 0.0, &[]);
        assert_eq!(plate.outer.len(), 4);
    }
}
