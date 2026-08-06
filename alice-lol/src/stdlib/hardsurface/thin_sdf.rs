//! # thin_sdf — 薄物の純 SDF 表現 (Phase 3''.2、ALICE way 回帰)
//!
//! Phase A.5.2 の `thin` module は Polygon2D + earcutr extrude (Phase 1 Data 相当 = ALICE 違反)
//! 本 module は同じ薄物を **純 SDF (SdfNode)** で表現し、Dual Contouring で mesh 化する
//! Phase 2 Law 経路 (ALICE 三相原理準拠)
//!
//! mesh 化は `alice_lol::print_export::node_to_3mf_dual_contouring` 経由
//!
//! ## primitive
//!
//! | primitive | Bamboo 対応 canonical | 用途 |
//! |-----------|--------------------|-----|
//! | [`shopping_cart_coin_sdf`] | `models/accessories/shopping-cart-coin/generate.py` | 100 円硬貨型キーホルダーコイン (Cylinder 単純) |
//!
//! ## Bamboo 実測 (対比検証項目)
//!
//! Bamboo 実測「SDF+MC で 1.7mm 設計 → 5.1mm 出力、6177 non-manifold edges」を、
//! 同 SDF を Dual Contouring で mesh 化した時に回避できるかを example
//! `coin_dc_vs_mc.rs` で実測する 成功すれば Phase A.5 polygon_extrude を deprecate → 削除、
//! ALICE 三相原理 Phase 2 Law 経路への完全回帰を達成

use alice_sdf::SdfNode;

// ────────────────────────────────────────────────────────
// 定数 (Phase A.5.2 thin と同期、Bamboo `models/accessories/shopping-cart-coin/` 準拠)
// ────────────────────────────────────────────────────────

/// 100 円硬貨型 shopping cart coin 直径 (mm、実測)
pub const COIN_100YEN_DIAMETER: f32 = 22.8;

/// 100 円硬貨型 shopping cart coin 厚 (mm、Bamboo 実プリント検証済、極薄物)
pub const COIN_100YEN_THICKNESS: f32 = 1.7;

// ────────────────────────────────────────────────────────
// SDF spec function
// ────────────────────────────────────────────────────────

/// Shopping cart coin (100 円硬貨型) の SdfNode を生成する
///
/// 単純 Cylinder (radius = `diameter/2`, half_height = `thickness/2`)、原点中心、Y 軸
///
/// # 引数
///
/// - `diameter`: 直径 (mm、通常 `COIN_100YEN_DIAMETER = 22.8`)
/// - `thickness`: 全厚 (mm、通常 `COIN_100YEN_THICKNESS = 1.7`)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::thin_sdf::{shopping_cart_coin_sdf, COIN_100YEN_DIAMETER, COIN_100YEN_THICKNESS};
/// let coin = shopping_cart_coin_sdf(COIN_100YEN_DIAMETER, COIN_100YEN_THICKNESS);
/// // Dual Contouring 経路推奨:
/// // alice_lol::print_export::node_to_3mf_dual_contouring(&coin, "coin.3mf", &config)
/// ```
///
/// # 検証
///
/// 本 SDF を MC (`node_to_3mf`) で mesh 化すると Bamboo 実測相当 (「1.7mm → 5.1mm、
/// 6177 non-manifold edges」) が再現される想定
/// DC (`node_to_3mf_dual_contouring`) で mesh 化すると Hermite data で watertight 保証
/// example `coin_dc_vs_mc.rs` で実測比較
#[must_use]
pub fn shopping_cart_coin_sdf(diameter: f32, thickness: f32) -> SdfNode {
    SdfNode::Cylinder {
        radius: diameter * 0.5,
        half_height: thickness * 0.5,
    }
}

// ────────────────────────────────────────────────────────
// テスト
// ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use alice_sdf::eval;
    use glam::Vec3;

    #[test]
    fn coin_sdf_is_cylinder() {
        let coin = shopping_cart_coin_sdf(COIN_100YEN_DIAMETER, COIN_100YEN_THICKNESS);
        match coin {
            SdfNode::Cylinder {
                radius,
                half_height,
            } => {
                assert!((radius - 11.4).abs() < 1e-4);
                assert!((half_height - 0.85).abs() < 1e-4);
            }
            _ => panic!("expected Cylinder"),
        }
    }

    #[test]
    fn coin_center_is_inside() {
        let coin = shopping_cart_coin_sdf(COIN_100YEN_DIAMETER, COIN_100YEN_THICKNESS);
        assert!(eval(&coin, Vec3::ZERO) < 0.0);
    }

    #[test]
    fn coin_outside_boundary() {
        let coin = shopping_cart_coin_sdf(COIN_100YEN_DIAMETER, COIN_100YEN_THICKNESS);
        // 直径 22.8mm → 半径 11.4mm、X=15 は 外
        assert!(eval(&coin, Vec3::new(15.0, 0.0, 0.0)) > 0.0);
        // 厚 1.7mm → half=0.85、Y=1.5 は 外
        assert!(eval(&coin, Vec3::new(0.0, 1.5, 0.0)) > 0.0);
    }
}
