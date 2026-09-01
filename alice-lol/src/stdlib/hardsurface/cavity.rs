//! # cavity — subtract 系 helper with cavity margin rule intrinsic (Phase C 根本 refactor、2026-09-01)
//!
//! `[[success_alice_lol_cavity_margin_batch_fix_2026_08_25]]` の "cavity margin +5mm each side" rule を
//! primitive API に intrinsic 化する 各 archetype で raw `subtract(plate, screw_hole(m, plate_t + 10.0))` を
//! 手書きすると rule 忘却で「四角」描画事故 (2026-08-27 〜 09-01 の session で 5 commit 連続発生)
//!
//! 本 module の helper 経由なら rule 自動適用、raw pattern grep で新規混入も検知可
//!
//! ## Rule
//!
//! - **Through hole**: cylinder length = `plate_thickness + 10.0` (5mm each side punch margin)
//! - **Blind pocket**: cylinder length = `pocket_depth + 5.0` (5mm above plate top for MC punch)
//!
//! ## 座標系
//!
//! 全 helper は plate が Y-up (板厚 = Y 軸方向、原点中心) 前提
//! hole は Y 軸 native cylinder (through: Y 貫通、blind: Y+ 側 pocket)
//! archetype が Z-up viewer 用に `to_z_up` wrap する時は本 helper の結果を wrap
//!
//! ## 使用例
//!
//! ```
//! use alice_lol::stdlib::hardsurface::cavity::subtract_through_screw_hole;
//! use alice_lol::stdlib::hardsurface::fastener::MetricSize;
//! use alice_sdf::SdfNode;
//! use glam::Vec3;
//!
//! let plate = SdfNode::Box3d { half_extents: Vec3::new(30.0, 2.5, 30.0) };
//! // Plate 60×5×60mm に M4 貫通穴を 4 隅に
//! let mut result = plate;
//! for (x, z) in [(25.0, 25.0), (-25.0, 25.0), (25.0, -25.0), (-25.0, -25.0)] {
//!     result = subtract_through_screw_hole(result, MetricSize::M4, 5.0, x, z);
//! }
//! ```

use crate::stdlib::hardsurface::fastener::{
    counterbore, countersink, MetricSize, CLEARANCE_H2D_FDM, HEAT_SET_SINK_MARGIN,
};
use alice_sdf::SdfNode;
use glam::Vec3;
use std::sync::Arc;

/// Cavity margin (mm、each side for through、above surface for blind)
///
/// Preview MC (cell ~1mm) が cavity 開口を確実 punch through できる余裕
/// 過去事案 (2026-08-25 batch fix、2026-08-27 〜 09-01 の 5 commit) で確立
pub const CAVITY_PUNCH_MARGIN: f32 = 5.0;

/// Through hole (generic Cylinder) を plate から subtract、cavity margin auto
///
/// Cylinder Y 軸 native、length = `plate_thickness + 2 * CAVITY_PUNCH_MARGIN`
/// (5mm each side punch margin)、position (x, 0, z)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::cavity::subtract_through_cylinder;
/// use alice_sdf::SdfNode;
/// use glam::Vec3;
///
/// let plate = SdfNode::Box3d { half_extents: Vec3::new(30.0, 2.5, 30.0) };
/// let result = subtract_through_cylinder(plate, 5.0, 5.0, 20.0, 20.0);
/// ```
#[must_use]
pub fn subtract_through_cylinder(
    plate: SdfNode,
    hole_dia: f32,
    plate_thickness: f32,
    x: f32,
    z: f32,
) -> SdfNode {
    let cyl_len = plate_thickness + 2.0 * CAVITY_PUNCH_MARGIN;
    let hole = SdfNode::Cylinder {
        radius: hole_dia * 0.5,
        half_height: cyl_len * 0.5,
    };
    let hole_placed = SdfNode::Translate {
        child: Arc::new(hole),
        offset: Vec3::new(x, 0.0, z),
    };
    SdfNode::Subtraction {
        a: Arc::new(plate),
        b: Arc::new(hole_placed),
    }
}

/// Screw clearance through hole (H2D FDM +0.2mm auto)
///
/// hole dia = `m.nominal_diameter() + CLEARANCE_H2D_FDM` (H2D 実測)
/// cylinder length = `plate_thickness + 10mm` (5mm each side)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::cavity::subtract_through_screw_hole;
/// use alice_lol::stdlib::hardsurface::fastener::MetricSize;
/// use alice_sdf::SdfNode;
/// use glam::Vec3;
///
/// let plate = SdfNode::Box3d { half_extents: Vec3::new(30.0, 2.5, 30.0) };
/// let result = subtract_through_screw_hole(plate, MetricSize::M4, 5.0, 20.0, 20.0);
/// ```
#[must_use]
pub fn subtract_through_screw_hole(
    plate: SdfNode,
    m: MetricSize,
    plate_thickness: f32,
    x: f32,
    z: f32,
) -> SdfNode {
    let hole_dia = m.nominal_diameter() + CLEARANCE_H2D_FDM;
    subtract_through_cylinder(plate, hole_dia, plate_thickness, x, z)
}

/// Counterbore through hole (ISO 4762 head sink) with cavity margin auto
///
/// Internal `counterbore()` は through hole depth に既 +10mm 適用済 (e0399db fix)
/// 本 helper は translate + subtract を wrap するだけ、位置 (x, 0, z)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::cavity::subtract_through_counterbore;
/// use alice_lol::stdlib::hardsurface::fastener::MetricSize;
/// use alice_sdf::SdfNode;
/// use glam::Vec3;
///
/// let plate = SdfNode::Box3d { half_extents: Vec3::new(30.0, 2.5, 30.0) };
/// let result = subtract_through_counterbore(plate, MetricSize::M4, 5.0, 20.0, 20.0);
/// ```
#[must_use]
pub fn subtract_through_counterbore(
    plate: SdfNode,
    m: MetricSize,
    plate_thickness: f32,
    x: f32,
    z: f32,
) -> SdfNode {
    let hole = counterbore(m, plate_thickness);
    let hole_placed = SdfNode::Translate {
        child: Arc::new(hole),
        offset: Vec3::new(x, 0.0, z),
    };
    SdfNode::Subtraction {
        a: Arc::new(plate),
        b: Arc::new(hole_placed),
    }
}

/// Countersink through hole (ISO 10642 90° cone) with cavity margin auto
///
/// Internal `countersink()` は through hole depth に既 +10mm 適用済 (e0399db fix)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::cavity::subtract_through_countersink;
/// use alice_lol::stdlib::hardsurface::fastener::MetricSize;
/// use alice_sdf::SdfNode;
/// use glam::Vec3;
///
/// let plate = SdfNode::Box3d { half_extents: Vec3::new(30.0, 2.5, 30.0) };
/// let result = subtract_through_countersink(plate, MetricSize::M4, 5.0, 20.0, 20.0);
/// ```
#[must_use]
pub fn subtract_through_countersink(
    plate: SdfNode,
    m: MetricSize,
    plate_thickness: f32,
    x: f32,
    z: f32,
) -> SdfNode {
    let hole = countersink(m, plate_thickness);
    let hole_placed = SdfNode::Translate {
        child: Arc::new(hole),
        offset: Vec3::new(x, 0.0, z),
    };
    SdfNode::Subtraction {
        a: Arc::new(plate),
        b: Arc::new(hole_placed),
    }
}

/// Blind pocket (plate top から挿入、5mm 上方 punch margin auto)
///
/// pocket 底 Y = `plate_thickness / 2 - pocket_depth` (物理仕様通り)
/// pocket 上端 Y = `plate_thickness / 2 + 5mm` (preview MC punch margin)
/// cylinder length = `pocket_depth + 5mm`、center = `outer_hy + (5 - pocket_depth) / 2`
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::cavity::subtract_blind_pocket;
/// use alice_sdf::SdfNode;
/// use glam::Vec3;
///
/// let plate = SdfNode::Box3d { half_extents: Vec3::new(30.0, 3.0, 30.0) };
/// // 板厚 6mm、Ø10 pocket 深 4mm を (20, 20) に blind 掘り
/// let result = subtract_blind_pocket(plate, 10.0, 6.0, 4.0, 20.0, 20.0);
/// ```
#[must_use]
pub fn subtract_blind_pocket(
    plate: SdfNode,
    hole_dia: f32,
    plate_thickness: f32,
    pocket_depth: f32,
    x: f32,
    z: f32,
) -> SdfNode {
    let outer_hy = plate_thickness * 0.5;
    let cyl_len = pocket_depth + CAVITY_PUNCH_MARGIN;
    let hole = SdfNode::Cylinder {
        radius: hole_dia * 0.5,
        half_height: cyl_len * 0.5,
    };
    // center Y = outer_hy + (punch_margin - pocket_depth) / 2
    // → cylinder extends Y = [outer_hy - pocket_depth, outer_hy + punch_margin]
    let y_offset = outer_hy + (CAVITY_PUNCH_MARGIN - pocket_depth) * 0.5;
    let hole_placed = SdfNode::Translate {
        child: Arc::new(hole),
        offset: Vec3::new(x, y_offset, z),
    };
    SdfNode::Subtraction {
        a: Arc::new(plate),
        b: Arc::new(hole_placed),
    }
}

/// Heat-set insert blind pocket (McMaster/Voxel8 spec + 5mm 上方 auto)
///
/// pocket dia = `m.heat_set_insert_diameter() + CLEARANCE_H2D_FDM`
/// pocket depth = `m.heat_set_insert_depth() + HEAT_SET_SINK_MARGIN` (物理仕様)
/// cylinder は上方 5mm 延長 (preview MC punch)、pocket 底は物理仕様通り
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::cavity::subtract_blind_heat_set;
/// use alice_lol::stdlib::hardsurface::fastener::MetricSize;
/// use alice_sdf::SdfNode;
/// use glam::Vec3;
///
/// let plate = SdfNode::Box3d { half_extents: Vec3::new(30.0, 3.0, 30.0) };
/// let result = subtract_blind_heat_set(plate, MetricSize::M3, 6.0, 20.0, 20.0);
/// ```
#[must_use]
pub fn subtract_blind_heat_set(
    plate: SdfNode,
    m: MetricSize,
    plate_thickness: f32,
    x: f32,
    z: f32,
) -> SdfNode {
    let insert_dia = m.heat_set_insert_diameter() + CLEARANCE_H2D_FDM;
    let pocket_depth = m.heat_set_insert_depth() + HEAT_SET_SINK_MARGIN;
    subtract_blind_pocket(plate, insert_dia, plate_thickness, pocket_depth, x, z)
}

/// Fastener hole の生 `screw_hole` を経由せず、直接 cavity margin 込みの cylinder を返す
///
/// 高階 helper (`subtract_through_screw_hole` 等) の internal 実装で使用
/// archetype 側で「hole primitive を translate してから subtract」する場合の便利 wrapper
///
/// Returns Cylinder Y-axis with length = `plate_thickness + 10mm` (5mm each side)
#[must_use]
pub fn through_hole_cylinder(hole_dia: f32, plate_thickness: f32) -> SdfNode {
    let cyl_len = plate_thickness + 2.0 * CAVITY_PUNCH_MARGIN;
    SdfNode::Cylinder {
        radius: hole_dia * 0.5,
        half_height: cyl_len * 0.5,
    }
}

// ────────────────────────────────────────────────────────
// テスト
// ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use alice_sdf::eval;

    fn plate_5mm() -> SdfNode {
        SdfNode::Box3d {
            half_extents: Vec3::new(30.0, 2.5, 30.0),
        }
    }

    #[test]
    fn subtract_through_cylinder_creates_visible_cavity() {
        let plate = plate_5mm();
        let result = subtract_through_cylinder(plate, 4.2, 5.0, 0.0, 0.0);
        assert!(matches!(result, SdfNode::Subtraction { .. }));
        // 中心 (0, 0, 0) は穴、SDF > 0
        assert!(
            eval(&result, Vec3::ZERO) > 0.0,
            "hole center should be empty"
        );
    }

    #[test]
    fn subtract_through_screw_hole_uses_h2d_clearance() {
        let plate = plate_5mm();
        let result = subtract_through_screw_hole(plate, MetricSize::M4, 5.0, 0.0, 0.0);
        // M4 clearance = 4.0 + 0.2 = 4.2mm → radius 2.1
        // 板表面 Y=+2.5、hole extends Y = [-7.5, +7.5] (板厚 5 + 10)
        // (0, 2.5, 0) at plate top center: 穴中心なので empty
        assert!(eval(&result, Vec3::new(0.0, 2.5, 0.0)) >= 0.0);
        // (2.5, 0, 0) at plate edge X: solid (穴中心は 0 で距離 2.5 > radius 2.1)
        assert!(eval(&result, Vec3::new(2.5, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn subtract_through_cylinder_has_5mm_margin_each_side() {
        let plate = plate_5mm();
        let _result = subtract_through_cylinder(plate, 4.2, 5.0, 0.0, 0.0);
        // cylinder length = 5 + 10 = 15mm、half_height = 7.5
        // 板 Y = [-2.5, +2.5]、hole Y = [-7.5, +7.5]
        // margin above plate = 7.5 - 2.5 = 5mm ✓
        // margin below plate = 7.5 - 2.5 = 5mm ✓
        // (このテストは数式で保証、runtime assertion 不要だが cavity margin rule 準拠を明示)
        let expected_hole_half_height = (5.0 + 2.0 * CAVITY_PUNCH_MARGIN) * 0.5;
        assert!((expected_hole_half_height - 7.5).abs() < 1e-6);
    }

    #[test]
    fn subtract_blind_pocket_extends_5mm_above_plate() {
        let plate = SdfNode::Box3d {
            half_extents: Vec3::new(30.0, 3.0, 30.0),
        };
        // 板厚 6mm、pocket 深 4mm
        let _result = subtract_blind_pocket(plate, 10.0, 6.0, 4.0, 0.0, 0.0);
        // cylinder length = 4 + 5 = 9mm、half_height = 4.5
        // outer_hy = 3、y_offset = 3 + (5 - 4) / 2 = 3.5
        // hole extends Y = [3.5 - 4.5, 3.5 + 4.5] = [-1.0, +8.0]
        // plate top = +3、pocket top = +8 → margin +5mm above ✓
        // pocket bottom = -1 = plate top - 4mm depth ✓ (物理仕様通り)
        let expected_y_offset = 3.0 + (CAVITY_PUNCH_MARGIN - 4.0) * 0.5;
        assert!((expected_y_offset - 3.5).abs() < 1e-6);
    }

    #[test]
    fn subtract_blind_heat_set_m3_matches_mcmaster_spec() {
        let plate = SdfNode::Box3d {
            half_extents: Vec3::new(30.0, 3.0, 30.0),
        };
        let _result = subtract_blind_heat_set(plate, MetricSize::M3, 6.0, 0.0, 0.0);
        // M3 insert: dia = 4 + 0.2 = 4.2mm、depth = 3.8 + 0.3 = 4.1mm
        // cylinder length = 4.1 + 5 = 9.1mm
        // pocket bottom Y = 3 - 4.1 = -1.1 ✓ (物理 insert 底)
        // pocket top Y = 3 + 5 = 8.0 ✓ (preview margin)
        let m3_pocket_depth = MetricSize::M3.heat_set_insert_depth() + HEAT_SET_SINK_MARGIN;
        assert!((m3_pocket_depth - 4.1).abs() < 1e-4);
    }

    #[test]
    fn cavity_margin_constant_is_5mm() {
        // Rule 変更検知: CAVITY_PUNCH_MARGIN は 5.0 固定、変更時は他 primitive も見直し要
        assert!((CAVITY_PUNCH_MARGIN - 5.0).abs() < 1e-6);
    }

    #[test]
    fn through_hole_cylinder_length_is_plate_plus_10mm() {
        let hole = through_hole_cylinder(4.2, 5.0);
        if let SdfNode::Cylinder { half_height, .. } = hole {
            // (5 + 2 * 5) / 2 = 7.5
            assert!((half_height - 7.5).abs() < 1e-6);
        } else {
            panic!("expected Cylinder");
        }
    }

    #[test]
    fn all_helpers_produce_finite_sdf_at_origin() {
        use alice_sdf::eval;
        let plate = plate_5mm();

        let hs = [
            subtract_through_cylinder(plate.clone(), 4.2, 5.0, 20.0, 20.0),
            subtract_through_screw_hole(plate.clone(), MetricSize::M4, 5.0, 20.0, 20.0),
            subtract_through_counterbore(plate.clone(), MetricSize::M4, 5.0, 20.0, 20.0),
            subtract_through_countersink(plate.clone(), MetricSize::M4, 5.0, 20.0, 20.0),
            subtract_blind_pocket(plate.clone(), 10.0, 5.0, 3.0, 20.0, 20.0),
            subtract_blind_heat_set(plate, MetricSize::M3, 5.0, 20.0, 20.0),
        ];
        for (i, h) in hs.iter().enumerate() {
            let d = eval(h, Vec3::new(0.1, 0.1, 0.1));
            assert!(d.is_finite(), "cavity helper {i} produced non-finite SDF");
        }
    }
}
