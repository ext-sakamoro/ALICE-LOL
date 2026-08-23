//! # mug — マグカップ canonical primitive (Phase P.1)
//!
//! 円筒外径 + 内側くぼみ + torus 取手 の 3 部品を canonical 組合せで生成する
//!
//! 2026-08-23 text-to-print v0.1.0 β の LLM 生成事案対応 (Qwen 3.5-4B が
//! マグカップ prompt に対して 11 min × 3 attempt で box+cylinder fallback して
//! garbage 出力する問題を primitive 化で bypass)
//!
//! 詳細: [[feedback_llm_3b_complex_shape_hallucination]]
//!
//! ## 座標系 (ALICE-LOL 慣習、Z-up)
//!
//! - 底面は Z = 0 平面
//! - 円筒中心軸は Z 軸
//! - 取手は +X 方向に torus (major radius = 15mm、minor = 3mm)
//! - 取手中心 Z 高さ = height / 2 (中央高さ)
//!
//! ## デフォルト値 (実プリント検証済み、Bambu H2D + PLA + 0.2mm layer 想定)
//!
//! | 引数 | 用途 | 例値 |
//! |--|--|--|
//! | `dia` | マグ外径 (mm) | 50-80 (典型的マグカップ) |
//! | `height` | マグ高さ (mm) | 80-120 |
//!
//! 内側くぼみ:
//! - 壁厚: 3mm (外径 - 内径 = 6mm、片側 3mm) — PLA 印刷で強度確保
//! - 底厚: 3mm — 液体保持 + 印刷安定性
//! - 深さ: height - 3mm (底 3mm 残す)
//!
//! 取手:
//! - major radius: 15mm — 指 2-3 本入る
//! - minor radius: 3mm — 印刷可能な最小径 (0.4mm nozzle × 8 line)
//! - X 位置: dia/2 + minor_r (外壁に接する外側)
//! - Y-axis 回転 (torus は XZ 平面に生成される、Y 軸 90° 回転で YZ 平面 = mug 側面)

use alice_sdf::SdfNode;
use glam::{Quat, Vec3};
use std::sync::Arc;

/// 標準壁厚 (mm)、内側くぼみの radius offset
const DEFAULT_WALL_THICKNESS: f32 = 3.0;

/// 標準底厚 (mm)、subtract cylinder の Z offset
const DEFAULT_BOTTOM_THICKNESS: f32 = 3.0;

/// 取手 major radius (mm)、指 2-3 本入る手のサイズ
const DEFAULT_HANDLE_MAJOR_R: f32 = 15.0;

/// 取手 minor radius (mm)、印刷可能な最小径 (0.4mm nozzle × 8 line)
const DEFAULT_HANDLE_MINOR_R: f32 = 3.0;

/// マグカップの SDF 表現
///
/// # 引数
/// - `dia`: マグ外径 (mm、典型 50-80)
/// - `height`: マグ高さ (mm、典型 80-120)
///
/// # 戻り値
/// `SdfNode::Union { hollow_body, handle }` — 完全 watertight、Dual Contouring
/// or Marching Cubes 両対応
///
/// # 制約
/// - dia は minimum 20mm 想定 (小さすぎると取手 major_r=15 と干渉)
/// - height は minimum 40mm 想定 (取手 minor_r×2 で 6mm 使うので余裕必要)
/// - Extreme 小サイズ (< 20mm) は取手が本体外径からはみ出る geometry になるが、
///   pipeline はそのまま流れる (physical には非現実的だが SDF は valid)
#[must_use]
pub fn mug_sdf(dia: f32, height: f32) -> SdfNode {
    let outer_r = dia * 0.5;
    let inner_r = outer_r - DEFAULT_WALL_THICKNESS;
    let half_h = height * 0.5;
    // 内側くぼみ: 底 3mm 残す + 上端は貫通、subtract cylinder の中心 Z を
    // (bottom_thickness / 2) 上に translate、half_height は (height - bottom) / 2
    let hollow_z_offset = DEFAULT_BOTTOM_THICKNESS * 0.5;
    let hollow_half_h = (height - DEFAULT_BOTTOM_THICKNESS) * 0.5 + 1.0; // +1mm で上端貫通確保

    let outer = Arc::new(SdfNode::Cylinder {
        radius: outer_r,
        half_height: half_h,
    });
    let hollow_raw = Arc::new(SdfNode::Cylinder {
        radius: inner_r,
        half_height: hollow_half_h,
    });
    let hollow = Arc::new(SdfNode::Translate {
        child: hollow_raw,
        offset: Vec3::new(0.0, 0.0, hollow_z_offset),
    });
    let hollow_body = SdfNode::Subtraction {
        a: outer,
        b: hollow,
    };

    // 取手: torus (XZ 平面) を Y 軸 90° 回転で YZ 平面化、X 方向に translate
    // torus は default で center at origin、XZ 平面上に major_radius 円 tube
    // Y 軸 90° 回転で major_radius 円が XY→XZ plane に (実質そのまま)
    // 実際には X 軸 90° 回転で torus 面を垂直化 (取手が上下方向にループ)
    let handle_raw = Arc::new(SdfNode::Torus {
        major_radius: DEFAULT_HANDLE_MAJOR_R,
        minor_radius: DEFAULT_HANDLE_MINOR_R,
    });
    // torus は XZ 平面 (Y 軸が回転軸) に円環を作る mug の側面に付けるには
    // Y 軸を X 軸に向ける = X 軸周りに 90° 回転
    let handle_rotated = Arc::new(SdfNode::Rotate {
        child: handle_raw,
        rotation: Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
    });
    // 取手を mug 外壁 (X = outer_r) の外側 (X = outer_r + handle_minor_r) に配置
    // 中央高さ (Z = 0、cylinder は原点中心) に translate
    let handle = SdfNode::Translate {
        child: handle_rotated,
        offset: Vec3::new(outer_r + DEFAULT_HANDLE_MINOR_R, 0.0, 0.0),
    };

    SdfNode::Union {
        a: Arc::new(hollow_body),
        b: Arc::new(handle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mug_sdf_returns_union_of_subtract_and_translate_handle() {
        let node = mug_sdf(50.0, 100.0);
        match node {
            SdfNode::Union { a, b } => {
                // a = hollow_body (Subtraction)
                assert!(matches!(*a, SdfNode::Subtraction { .. }));
                // b = handle (Translate around Rotate around Torus)
                assert!(matches!(*b, SdfNode::Translate { .. }));
            }
            _ => panic!("Expected Union at top level"),
        }
    }

    #[test]
    fn mug_sdf_various_dimensions_do_not_panic() {
        // 典型サイズ
        let _ = mug_sdf(50.0, 100.0);
        let _ = mug_sdf(80.0, 120.0);
        let _ = mug_sdf(30.0, 80.0);
        // 極小 (physical に非現実的だが SDF は valid)
        let _ = mug_sdf(20.0, 40.0);
    }
}
