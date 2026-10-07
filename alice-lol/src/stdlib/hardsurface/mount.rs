//! # mount — 建築 / 取付要素 primitive (Phase A.4)
//!
//! 部品を壁 / 板 / 押出フレームに固定する 6 primitive を提供する
//!
//! | primitive | 用途 | 実装 |
//! |-----------|------|------|
//! | [`bracket_l`] | L 字 bracket (水平 + 垂直板、内角 fillet 統合) | 2 Box3d + `SmoothUnion` (fillet) |
//! | [`flange_circular`] | 円形フランジ (PCD 上に bolt 穴 pattern) | Cylinder + `polar_repeat` bolt holes |
//! | [`rack_shelf`] | 棚受けレール (等間隔 notch 列) | Box3d - `RepeatFinite` (Cylinder notches) |
//! | [`skadis_peg_compat`] | IKEA SKADIS 互換 peg 単体 (Bamboo `PEG_W - FDM_CLEARANCE` 準拠) | `RoundedBox` 単体 |
//! | [`profile_2020`] | 20×20 アルミプロファイル外形 (4 面 T スロット + 中央 M5 穴) | Box + 4 rotate/translate T-slot + 中央 Cylinder |
//! | [`profile_3030`] | 30×30 アルミプロファイル外形 (4 面 T スロット + 中央 M6 穴) | 同上、寸法違い |
//!
//! ## SKADIS hook 全体は SDF 非推奨
//!
//! 薄板 (≤ 5mm) の複雑形状は SDF+マーチングキューブで非多様体エッジが多発する
//!
//! IKEA SKADIS hook 全体 (peg + 爪 + 引っかけ部) を実プリント可能な品質で
//! 生成するには **2D polygon + extrude 方式** が canonical
//! → `ALICE-Bamboo/src/generators/skadis.rs` (Python `generate.py` 経由) を使う
//!
//! 本 module では **peg tenon 単体のみ SDF 提供** ([`skadis_peg_compat`])
//! hook 本体組立は Bamboo 側に委譲

use alice_sdf::SdfNode;
use glam::{Quat, Vec3};
use std::sync::Arc;

// ────────────────────────────────────────────────────────
// 定数 (SKADIS / 2020 / 3030 標準寸法)
// ────────────────────────────────────────────────────────

/// IKEA SKADIS peg 幅 (mm、Bamboo `formulas::skadis::PEG_W` 準拠)
pub const SKADIS_PEG_W: f32 = 5.0;

/// IKEA SKADIS peg 高 (mm、Bamboo `PEG_H`)
pub const SKADIS_PEG_H: f32 = 15.0;

/// IKEA SKADIS peg 上下端の rounded 半径 (mm、Bamboo `PEG_R`)
pub const SKADIS_PEG_R: f32 = 2.5;

/// FDM 実測 clearance (mm、Bamboo `FDM_CLEARANCE`)
/// hook 側 peg 幅 = `SKADIS_PEG_W` - `FDM_CLEARANCE` = 4.8mm
pub const FDM_CLEARANCE: f32 = 0.2;

/// 2020 プロファイル外形 (mm)
pub const PROFILE_2020_SIZE: f32 = 20.0;

/// 3030 プロファイル外形 (mm)
pub const PROFILE_3030_SIZE: f32 = 30.0;

/// 2020 プロファイル中央穴径 (M5 通し、mm)
pub const PROFILE_2020_CENTER_BORE: f32 = 5.2;

/// 3030 プロファイル中央穴径 (M6 通し、mm)
pub const PROFILE_3030_CENTER_BORE: f32 = 6.2;

/// プロファイルの T スロット開口幅 (mm、20 シリーズの供給元の寸法表は 6.0〜6.2)
pub const PROFILE_SLOT_OPENING_WIDTH: f32 = 6.0;

/// プロファイルの T スロットの首 (lip) の厚み (mm、供給元の寸法表は 1.5〜1.8)
pub const PROFILE_SLOT_NECK_DEPTH: f32 = 1.8;

/// プロファイルの T スロットの全深さ (mm、表面から底まで、供給元の寸法表は 5.5〜6.1)
pub const PROFILE_SLOT_DEPTH: f32 = 6.1;

/// プロファイルの T スロットの空洞の最大幅 (mm、首の直下、45 度の斜壁で底へ向かって狭まる)
pub const PROFILE_SLOT_CAVITY_WIDTH: f32 = 11.0;

/// `SmoothUnion` の `k` を円弧の fillet 半径 R に対応づける係数 (`4 - 2√2` = 1.1716)
///
/// 直角の内角の 2 面を 2 次多項式の smooth min (`min - h²k/4`) でつなぐと、角から対角線上の
/// 面までの距離は `k√2/4`  半径 R の円弧なら `R(√2 - 1)`  両者を等しくする `k = (4 - 2√2)·R`
/// (形は厳密な円弧ではないが、角の埋まり方は R の円弧に一致する)
const SMOOTH_UNION_K_PER_FILLET_RADIUS: f32 = 4.0 - 2.0 * std::f32::consts::SQRT_2;

// ────────────────────────────────────────────────────────
// 1. L 字 bracket
// ────────────────────────────────────────────────────────

/// L 字 bracket (水平板 + 垂直板、内角 fillet 統合)
///
/// 構造: 水平板 (`horizontal_length` × `thickness` × `depth`) を Y=0 に配置、
/// 垂直板 (`thickness` × `vertical_height` × `depth`) を水平板の -X 端に立てて
/// `SdfNode::SmoothUnion` で内角 R = `fillet_radius` を付ける
/// (`k = (4 - 2√2)·R` で、角から対角線上の面までの距離が半径 R の円弧と一致する)
///
/// # 引数
///
/// - `horizontal_length`: 水平板長 (mm、X 軸)
/// - `vertical_height`: 垂直板高 (mm、Y 軸、水平板上面からの立上り)
/// - `thickness`: 板厚 (mm、両板共通)
/// - `depth`: 奥行 (mm、Z 軸、両板共通)
/// - `fillet_radius`: 内角 R (mm、0 なら fillet なし = 通常 Union)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::mount::bracket_l;
/// let b = bracket_l(60.0, 40.0, 4.0, 40.0, 3.0);
/// // 水平 60×4×40 + 垂直 4×40×40、内角 R3
/// ```
#[must_use]
pub fn bracket_l(
    horizontal_length: f32,
    vertical_height: f32,
    thickness: f32,
    depth: f32,
    fillet_radius: f32,
) -> SdfNode {
    let horizontal = SdfNode::Box3d {
        half_extents: Vec3::new(horizontal_length * 0.5, thickness * 0.5, depth * 0.5),
    };
    let vertical_raw = SdfNode::Box3d {
        half_extents: Vec3::new(thickness * 0.5, vertical_height * 0.5, depth * 0.5),
    };
    // 垂直板を水平板の -X 端に立てる
    // 水平板中心 X=0 / Y=0、垂直板は X = -(horizontal_length/2 - thickness/2)、Y = vertical_height/2 + thickness/2
    let vertical = SdfNode::Translate {
        child: Arc::new(vertical_raw),
        offset: Vec3::new(
            -(horizontal_length - thickness) * 0.5,
            f32::midpoint(vertical_height, thickness),
            0.0,
        ),
    };
    if fillet_radius > 0.0 {
        SdfNode::SmoothUnion {
            a: Arc::new(horizontal),
            b: Arc::new(vertical),
            k: fillet_radius * SMOOTH_UNION_K_PER_FILLET_RADIUS,
        }
    } else {
        SdfNode::Union {
            a: Arc::new(horizontal),
            b: Arc::new(vertical),
        }
    }
}

// ────────────────────────────────────────────────────────
// 2. 円形フランジ
// ────────────────────────────────────────────────────────

/// 円形フランジ (中央 through hole + PCD 上の bolt 穴 pattern)
///
/// 構造: 外径 `od` の Cylinder から中央 through hole (`center_bore_dia`) と、
/// PCD (`bolt_pcd`) 上に `bolt_count` 個の bolt 穴 (`bolt_dia`) を放射状配置して Subtraction
/// bolt 穴の配置は `SdfNode::PolarRepeat` で Y 軸周り count 個回転コピー
///
/// # 引数
///
/// - `od`: 外径 (mm)
/// - `center_bore_dia`: 中央通し穴径 (mm、0 なら中央穴なし)
/// - `thickness`: フランジ厚 (mm、Y 軸)
/// - `bolt_pcd`: bolt PCD (Pitch Circle Diameter、mm)
/// - `bolt_count`: bolt 穴個数 (u32)
/// - `bolt_dia`: bolt 通し穴径 (mm、H2D FDM で M4 なら 4.2 相当)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::mount::flange_circular;
/// let f = flange_circular(60.0, 20.0, 6.0, 45.0, 4, 4.2);
/// // Φ60 × 厚 6mm、中央 Φ20 穴、PCD45mm 上に 4 個 Φ4.2 bolt 穴
/// ```
#[must_use]
pub fn flange_circular(
    od: f32,
    center_bore_dia: f32,
    thickness: f32,
    bolt_pcd: f32,
    bolt_count: u32,
    bolt_dia: f32,
) -> SdfNode {
    let disc = SdfNode::Cylinder {
        radius: od * 0.5,
        half_height: thickness * 0.5,
    };
    // Bolt 穴の 1 個目を X = bolt_pcd/2 位置に配置し、PolarRepeat で count 個回転コピー
    // +5.0 = 5mm each side、preview MC で確実 punch through (cavity margin rule)
    let single_bolt = SdfNode::Cylinder {
        radius: bolt_dia * 0.5,
        half_height: thickness.mul_add(0.5, 5.0),
    };
    let bolt_offset = SdfNode::Translate {
        child: Arc::new(single_bolt),
        offset: Vec3::new(bolt_pcd * 0.5, 0.0, 0.0),
    };
    let bolt_ring = SdfNode::PolarRepeat {
        child: Arc::new(bolt_offset),
        count: bolt_count,
    };
    let with_bolts = SdfNode::Subtraction {
        a: Arc::new(disc),
        b: Arc::new(bolt_ring),
    };
    if center_bore_dia > 0.0 {
        // +5.0 = 5mm each side、preview MC で確実 punch through (cavity margin rule)
        let center_bore = SdfNode::Cylinder {
            radius: center_bore_dia * 0.5,
            half_height: thickness.mul_add(0.5, 5.0),
        };
        SdfNode::Subtraction {
            a: Arc::new(with_bolts),
            b: Arc::new(center_bore),
        }
    } else {
        with_bolts
    }
}

// ────────────────────────────────────────────────────────
// 3. rack_shelf
// ────────────────────────────────────────────────────────

/// 棚受けレール (板に等間隔 notch 穴列を持つ長物)
///
/// 構造: 板 (`length` × `thickness` × `width`) から notch 穴 (`notch_dia` 径、Y 軸 cylinder)
/// を `notch_pitch` 間隔で `2*notch_count+1` 個 `RepeatFinite` で subtract
/// notch は板長辺 (X 軸) に沿って中央配置 (中心 `k * notch_pitch`、`k = -notch_count..=notch_count`)
///
/// ⚠️ ALICE-SDF の `RepeatFinite` は「`count` 個のセルを `±count/2` にクランプ」する
/// (`count` が偶数なら `count + 1` 個、奇数なら端に位置のずれた半端な穴が出る) ので、
/// `2 * notch_count` を渡して `2n + 1` 個の穴にする
///
/// # 引数
///
/// - `length`: レール全長 (mm、X 軸)
/// - `thickness`: 板厚 (mm、Y 軸方向、貫通深さ)
/// - `width`: 板幅 (mm、Z 軸)
/// - `notch_pitch`: notch 間隔 (mm)
/// - `notch_dia`: notch 径 (mm)
/// - `notch_count`: `RepeatFinite` 半径 (実個数 = `2 * count + 1`)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::mount::rack_shelf;
/// let r = rack_shelf(200.0, 5.0, 30.0, 25.0, 6.0, 3);
/// // 200mm レール、5mm 厚、幅 30mm、25mm ピッチで Φ6 notch 7 個 (2*3+1)
/// ```
#[must_use]
pub fn rack_shelf(
    length: f32,
    thickness: f32,
    width: f32,
    notch_pitch: f32,
    notch_dia: f32,
    notch_count: u32,
) -> SdfNode {
    let plate = SdfNode::Box3d {
        half_extents: Vec3::new(length * 0.5, thickness * 0.5, width * 0.5),
    };
    // +5.0 = 5mm each side、preview MC で確実 punch through (cavity margin rule)
    let notch = SdfNode::Cylinder {
        radius: notch_dia * 0.5,
        half_height: thickness.mul_add(0.5, 5.0),
    };
    let notch_row = SdfNode::RepeatFinite {
        child: Arc::new(notch),
        count: [notch_count.saturating_mul(2), 0, 0],
        spacing: Vec3::new(notch_pitch, 1.0, 1.0),
    };
    SdfNode::Subtraction {
        a: Arc::new(plate),
        b: Arc::new(notch_row),
    }
}

// ────────────────────────────────────────────────────────
// 4. SKADIS peg (単体、hook 全体は Bamboo に委譲)
// ────────────────────────────────────────────────────────

/// IKEA SKADIS 互換 peg 単体 (hook 側 tenon、Bamboo `PEG_W - FDM_CLEARANCE` 準拠)
///
/// 構造: `SdfNode::RoundedBox` 単体 (幅 `SKADIS_PEG_W - FDM_CLEARANCE` × 高 `SKADIS_PEG_H` ×
/// 厚 `board_thickness`、上下端 R = `SKADIS_PEG_R`)
///
/// **SDF 版は peg tenon のみ提供** hook 本体 (爪 + 引っかけ部) は 2D polygon + extrude 方式が
/// canonical (Bamboo `src/generators/skadis.rs` 参照)
///
/// # 引数
///
/// - `board_thickness`: SKADIS 板厚 (mm、通常 5mm)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::mount::skadis_peg_compat;
/// let peg = skadis_peg_compat(5.0);
/// // IKEA SKADIS 標準 5mm 板厚用 peg tenon
/// ```
#[must_use]
pub fn skadis_peg_compat(board_thickness: f32) -> SdfNode {
    let peg_w = SKADIS_PEG_W - FDM_CLEARANCE;
    let half = Vec3::new(peg_w * 0.5, SKADIS_PEG_H * 0.5, board_thickness * 0.5);
    // `RoundedBox` の外寸は `half_extents + round_radius` (全方向に R が足される) ので、
    // 外寸 `half` を得るには内側の寸法 `half - R` を渡す  R は最小の半寸法を超えられない
    // (幅 4.8 の半寸法 2.4 は `SKADIS_PEG_R` 2.5 より小さい)
    let r = SKADIS_PEG_R.min(half.min_element()).max(0.0);
    SdfNode::RoundedBox {
        half_extents: half - Vec3::splat(r),
        round_radius: r,
    }
}

// ────────────────────────────────────────────────────────
// 5-6. アルミプロファイル 2020 / 3030
// ────────────────────────────────────────────────────────

/// プロファイルの T スロット 1 本 (局所座標: 原点 = 表面の開口の中心、+X = 外向き、Y = 長手)
///
/// 首 (幅 6、厚み 1.8) と、その奥の空洞 (首の直下で幅 11、45 度の斜壁で底 6.1 まで狭まる)
/// 空洞を矩形にすると、隣り合う面の空洞が対角で重なって中心のコアが孤立する
/// (実際のプロファイルも斜壁で対角にリブを残す)  外側へ 5mm 延ばした刃で開口を確実に貫く
fn profile_t_slot(length: f32) -> SdfNode {
    let half_len = length.mul_add(0.5, 5.0);
    let neck = PROFILE_SLOT_NECK_DEPTH;
    let depth = PROFILE_SLOT_DEPTH;
    let open_half = PROFILE_SLOT_OPENING_WIDTH * 0.5;
    let cavity_half = PROFILE_SLOT_CAVITY_WIDTH * 0.5;
    let boxed = |x_center: f32, half_x: f32, half_z: f32| SdfNode::Translate {
        child: Arc::new(SdfNode::Box3d {
            half_extents: Vec3::new(half_x, half_len, half_z),
        }),
        offset: Vec3::new(x_center, 0.0, 0.0),
    };
    let neck_box = boxed(-neck * 0.5, neck * 0.5, open_half);
    let punch = boxed(2.5, 2.5, open_half);
    // 空洞: 首の直下 (x = -neck) の半幅 cavity_half から 1:1 で狭まる台形
    let cavity_box = boxed(-(neck + depth) * 0.5, (depth - neck) * 0.5, cavity_half);
    // 許す範囲 |z| <= cavity_half + neck + x  ⇔  (±z - x)/√2 <= (cavity_half + neck)/√2
    let inv_sqrt2 = std::f32::consts::FRAC_1_SQRT_2;
    let limit = (cavity_half + neck) * inv_sqrt2;
    let plane_pos = SdfNode::Plane {
        normal: Vec3::new(-inv_sqrt2, 0.0, inv_sqrt2),
        distance: limit,
    };
    let plane_neg = SdfNode::Plane {
        normal: Vec3::new(-inv_sqrt2, 0.0, -inv_sqrt2),
        distance: limit,
    };
    let cavity = SdfNode::Intersection {
        a: Arc::new(SdfNode::Intersection {
            a: Arc::new(cavity_box),
            b: Arc::new(plane_pos),
        }),
        b: Arc::new(plane_neg),
    };
    SdfNode::Union {
        a: Arc::new(SdfNode::Union {
            a: Arc::new(neck_box),
            b: Arc::new(punch),
        }),
        b: Arc::new(cavity),
    }
}

fn extrusion_profile(size: f32, length: f32, center_bore_dia: f32) -> SdfNode {
    // 外形 Box (size × size × length、Y 軸 = 長手方向)
    let outer = SdfNode::Box3d {
        half_extents: Vec3::new(size * 0.5, length * 0.5, size * 0.5),
    };
    // 4 面 T スロット: 局所座標の +X (開口の向き) を Y 軸まわりに回して X+ / Z+ / X- / Z- に向け、
    // 表面の開口の中心 (size/2 だけ外側) に置く
    let t_slot = profile_t_slot(length);
    let face_offset = size * 0.5;
    let mut with_slots = outer;
    for i in 0..4_u8 {
        let angle = std::f32::consts::FRAC_PI_2 * f32::from(i);
        let rotated = SdfNode::Rotate {
            child: Arc::new(t_slot.clone()),
            rotation: Quat::from_rotation_y(angle),
        };
        let offset = Vec3::new(face_offset * angle.cos(), 0.0, -face_offset * angle.sin());
        let placed = SdfNode::Translate {
            child: Arc::new(rotated),
            offset,
        };
        with_slots = SdfNode::Subtraction {
            a: Arc::new(with_slots),
            b: Arc::new(placed),
        };
    }
    // 中央 through bore (Y 軸)
    if center_bore_dia > 0.0 {
        // +5.0 = 5mm each side、preview MC で確実 punch through (cavity margin rule)
        let bore = SdfNode::Cylinder {
            radius: center_bore_dia * 0.5,
            half_height: length.mul_add(0.5, 5.0),
        };
        SdfNode::Subtraction {
            a: Arc::new(with_slots),
            b: Arc::new(bore),
        }
    } else {
        with_slots
    }
}

/// 20×20mm アルミプロファイル外形 (4 面 T スロット + 中央 M5 通し穴、MISUMI / `OpenBuilds` 準拠)
///
/// 構造: `Box3d` 外形 - 4 rotate/translate T-slot (`profile_t_slot`、首 + 斜壁の空洞) - 中央 Cylinder (M5)
/// 長手方向 = Y 軸
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::mount::profile_2020;
/// let p = profile_2020(300.0);
/// // 20×20mm × 長 300mm、4 面 T スロット + 中央 Φ5.2mm
/// ```
#[must_use]
pub fn profile_2020(length: f32) -> SdfNode {
    extrusion_profile(PROFILE_2020_SIZE, length, PROFILE_2020_CENTER_BORE)
}

/// 30×30mm アルミプロファイル外形 (4 面 T スロット + 中央 M6 通し穴)
///
/// 構造: `profile_2020` と同型、寸法 30×30 + 中央 Φ6.2 に置換
/// T スロットは 2020 と同じ寸法 (開口 6、首 1.8、全深さ 6.1、空洞幅 11) 3030 用の広口版は未実装
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::mount::profile_3030;
/// let p = profile_3030(500.0);
/// // 30×30mm × 長 500mm、4 面 T スロット (2020 spec 流用) + 中央 Φ6.2mm
/// ```
#[must_use]
pub fn profile_3030(length: f32) -> SdfNode {
    extrusion_profile(PROFILE_3030_SIZE, length, PROFILE_3030_CENTER_BORE)
}

// ────────────────────────────────────────────────────────
// テスト
// ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use alice_sdf::eval;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn bracket_l_with_fillet_returns_smooth_union() {
        let b = bracket_l(60.0, 40.0, 4.0, 40.0, 3.0);
        // k = (4 - 2√2)·R (角から対角線上の面までの距離を半径 R の円弧に合わせる)
        let want = 3.0 * 2.0_f32.mul_add(-std::f32::consts::SQRT_2, 4.0);
        assert!(matches!(b, SdfNode::SmoothUnion { k, .. } if approx_eq(k, want)));
    }

    #[test]
    fn bracket_l_without_fillet_returns_plain_union() {
        let b = bracket_l(60.0, 40.0, 4.0, 40.0, 0.0);
        assert!(matches!(b, SdfNode::Union { .. }));
    }

    #[test]
    fn bracket_l_horizontal_center_inside() {
        let b = bracket_l(60.0, 40.0, 4.0, 40.0, 3.0);
        // 水平板中央 (0, 0, 0) は板内部
        assert!(eval(&b, Vec3::ZERO) < 0.0);
    }

    #[test]
    fn flange_circular_has_bolt_holes() {
        let f = flange_circular(60.0, 20.0, 6.0, 45.0, 4, 4.2);
        // 中央 (穴内) は物質外
        assert!(eval(&f, Vec3::ZERO) > 0.0);
        // PCD22.5 上の bolt 穴中心 (X=22.5, Y=0, Z=0) は物質外
        assert!(eval(&f, Vec3::new(22.5, 0.0, 0.0)) > 0.0);
        // フランジ材料内 (X=28, Y=0, Z=0、PCD22.5 の外側 + bolt 穴外) は内部
        assert!(eval(&f, Vec3::new(28.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn flange_circular_no_center_bore() {
        let f = flange_circular(60.0, 0.0, 6.0, 45.0, 4, 4.2);
        // 中央は穴なし = 材料内部
        assert!(eval(&f, Vec3::ZERO) < 0.0);
    }

    #[test]
    fn rack_shelf_has_periodic_notches() {
        let r = rack_shelf(200.0, 5.0, 30.0, 25.0, 6.0, 3);
        // 中央 (notch 中心) は穴内 = 外部
        assert!(eval(&r, Vec3::ZERO) > 0.0);
        // notch 間 (X=+12.5, notch と notch の中間) は材料内部
        assert!(eval(&r, Vec3::new(12.5, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn skadis_peg_dimensions_match_bamboo_spec() {
        // Bamboo skadis peg (PEG_W=5.0 - FDM_CLEARANCE=0.2 = 4.8mm 幅、PEG_H=15mm 高)
        // `RoundedBox` の外寸は half_extents + round_radius なので、出来上がった形 (外寸) で測る
        let p = skadis_peg_compat(5.0);
        let extent = |dir: Vec3| {
            let (mut lo, mut hi) = (0.0_f32, 30.0_f32);
            for _ in 0..60 {
                let mid = f32::midpoint(lo, hi);
                if eval(&p, dir * mid) <= 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            lo * 2.0
        };
        assert!(
            (extent(Vec3::X) - 4.8).abs() < 1e-3,
            "幅 {}",
            extent(Vec3::X)
        );
        assert!(
            (extent(Vec3::Y) - 15.0).abs() < 1e-3,
            "高さ {}",
            extent(Vec3::Y)
        );
        assert!(
            (extent(Vec3::Z) - 5.0).abs() < 1e-3,
            "厚さ {}",
            extent(Vec3::Z)
        );
    }

    #[test]
    fn profile_2020_outer_dimensions() {
        let p = profile_2020(100.0);
        // 4 面 T スロット inner が中心方向に 11mm 突入するため 4 隅の三角形部分のみ材料が残る
        // X=+9, Z=+9 (右上隅) は 4 隅材料内 (T スロットの inner/opening 外)
        assert!(eval(&p, Vec3::new(9.0, 0.0, 9.0)) < 0.0);
        // 外形外 (X=+15) は空間
        assert!(eval(&p, Vec3::new(15.0, 0.0, 0.0)) > 0.0);
    }

    #[test]
    fn profile_3030_center_bore_is_hollow() {
        let p = profile_3030(100.0);
        // 中央 bore (M6 = Φ6.2、原点) は物質外
        assert!(eval(&p, Vec3::ZERO) > 0.0);
        // 4 隅材料 (X=+13, Z=+13) は内部 (30×30 の 4 隅、T スロット外)
        assert!(eval(&p, Vec3::new(13.0, 0.0, 13.0)) < 0.0);
    }

    #[test]
    fn all_mount_primitives_produce_finite_sdf() {
        let nodes = [
            bracket_l(60.0, 40.0, 4.0, 40.0, 3.0),
            flange_circular(60.0, 20.0, 6.0, 45.0, 4, 4.2),
            rack_shelf(200.0, 5.0, 30.0, 25.0, 6.0, 3),
            skadis_peg_compat(5.0),
            profile_2020(300.0),
            profile_3030(500.0),
        ];
        for (i, node) in nodes.iter().enumerate() {
            let d = eval(node, Vec3::new(0.1, 0.1, 0.1));
            assert!(d.is_finite(), "primitive {i} produced non-finite SDF: {d}");
        }
    }
}
