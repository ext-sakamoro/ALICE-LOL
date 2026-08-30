//! # reinforcement — 補強要素 primitive (Phase A.3)
//!
//! 部品の強度・剛性を高める 6 primitive を提供する
//!
//! | primitive | 用途 | 実装 |
//! |-----------|------|------|
//! | [`rib`] | 板裏の補強リブ | Box3d 単体 |
//! | [`boss`] | ネジ穴周りボス | Cylinder outer + tap hole subtraction (Bamboo `screw_boss_od = screw_dia × 2.2`) |
//! | [`fillet`] | 内角 R (2 SdfNode ブレンド) | `SdfNode::SmoothUnion` wrapper |
//! | [`chamfer`] | edge 面取り (2 SdfNode) | `SdfNode::ChamferUnion` wrapper |
//! | [`honeycomb_infill`] | 6 角形 infill 壁パターン | `RepeatFinite` + `HexPrism` を container から `Subtraction` |
//! | [`gyroid_infill`] | Gyroid TPMS infill | 既存 `SdfNode::Gyroid` + container との `Intersection` |
//!
//! ## alice-physics 連携 (`physics` feature、opt-in、AGPL-3.0 汚染注意)
//!
//! `--features physics` で有効化すると以下 API が使える
//!
//! - [`fillet_kt_shaft_shoulder`] — `alice_physics::fillet_stress::kt_shaft_shoulder_bending`
//! - [`material_elastic_modulus_gpa`] — `alice_physics::filament_db::MaterialProperties::pla/petg/abs`
//! - [`recommended_fillet_radius_mm`] — `alice_physics::fillet_stress::recommended_fillet_radius_mm`
//!
//! Bamboo `src/safety.rs` の canonical material spec と SSOT を共有できる

use alice_sdf::SdfNode;
use glam::Vec3;
use std::sync::Arc;

// ────────────────────────────────────────────────────────
// 定数 (Bamboo formulas 準拠、実プリント検証は Phase B.2 予定)
// ────────────────────────────────────────────────────────

/// ネジ穴周りボスの外径係数 (Bamboo `formulas::PrintParams::screw_boss_od`)
///
/// boss 外径 = `screw_dia` × [`SCREW_BOSS_OD_RATIO`]、周囲肉厚を確保する経験式
pub const SCREW_BOSS_OD_RATIO: f32 = 2.2;

/// タップ下穴公式のノズル精度余白 A のデフォルト値 (Phase A.1 [`crate::stdlib::hardsurface::fastener::DEFAULT_ACCURACY`] と同値、boss 内穴計算に流用)
pub const BOSS_TAP_ACCURACY: f32 = 0.1;

// ────────────────────────────────────────────────────────
// 1. rib
// ────────────────────────────────────────────────────────

/// 補強リブ (Box3d 単体、板裏の強度確保用)
///
/// 座標系: 長辺 = X、高さ = Y (板から立ち上がる方向)、厚 = Z
///
/// # 引数
///
/// - `length`: リブ長 (mm、X 軸)
/// - `height`: リブ高 (mm、Y 軸、板表面からの立上り)
/// - `thickness`: リブ厚 (mm、Z 軸、通常 板厚 × 0.6-0.8 推奨)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::reinforcement::rib;
/// let r = rib(30.0, 5.0, 1.6);
/// // 30mm 長 × 5mm 高 × 1.6mm 厚 (板厚 2mm 相当の 80%)
/// ```
#[must_use]
pub fn rib(length: f32, height: f32, thickness: f32) -> SdfNode {
    SdfNode::Box3d {
        half_extents: Vec3::new(length * 0.5, height * 0.5, thickness * 0.5),
    }
}

// ────────────────────────────────────────────────────────
// 2. boss
// ────────────────────────────────────────────────────────

/// ネジ穴周りボス (外径 [`SCREW_BOSS_OD_RATIO`] × `screw_dia`、内側 tap 穴 subtraction)
///
/// 構造: 外筒 Cylinder (外径 = `screw_dia` × 2.2) - 内側 tap 穴 (`screw_dia` × 0.85 + 2A)
/// 軸 = Y (Cylinder native)
///
/// # 引数
///
/// - `screw_dia`: ネジ呼び径 (mm)
/// - `height`: ボス全長 (mm、Y 軸)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::reinforcement::boss;
/// let b = boss(3.0, 8.0);
/// // M3 ボス: 外径 6.6mm × 内穴径 2.75mm × 長 8mm
/// ```
#[must_use]
pub fn boss(screw_dia: f32, height: f32) -> SdfNode {
    let outer_r = screw_dia * SCREW_BOSS_OD_RATIO * 0.5;
    let tap_r = screw_dia.mul_add(0.85, 2.0 * BOSS_TAP_ACCURACY) * 0.5;
    let barrel = SdfNode::Cylinder {
        radius: outer_r,
        half_height: height * 0.5,
    };
    let hole = SdfNode::Cylinder {
        radius: tap_r,
        // +5.0 = 5mm each side、preview MC (cell ~1mm) で確実 punch through
        // (cavity margin rule、[[success_alice_lol_cavity_margin_batch_fix_2026_08_25]])
        half_height: height * 0.5 + 5.0,
    };
    SdfNode::Subtraction {
        a: Arc::new(barrel),
        b: Arc::new(hole),
    }
}

// ────────────────────────────────────────────────────────
// 3. fillet (SmoothUnion wrapper)
// ────────────────────────────────────────────────────────

/// 内角 R (2 SdfNode を smooth-union で滑らかに blend)
///
/// 応力集中を緩和する canonical 手段
/// R が大きいほど内角の応力集中係数 Kt が下がる (`physics` feature で計算可)
///
/// # 引数
///
/// - `a`, `b`: blend 対象の 2 SdfNode
/// - `radius`: フィレット半径 (mm、smooth-union の blend radius k)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::reinforcement::fillet;
/// use alice_sdf::SdfNode;
/// use glam::Vec3;
/// let base = SdfNode::Box3d { half_extents: Vec3::new(10.0, 2.0, 20.0) };
/// let vertical = SdfNode::Box3d { half_extents: Vec3::new(2.0, 15.0, 20.0) };
/// let filleted = fillet(base, vertical, 1.5);
/// // L 字部品の内角に R1.5mm フィレット
/// ```
#[must_use]
pub fn fillet(a: SdfNode, b: SdfNode, radius: f32) -> SdfNode {
    SdfNode::SmoothUnion {
        a: Arc::new(a),
        b: Arc::new(b),
        k: radius,
    }
}

// ────────────────────────────────────────────────────────
// 4. chamfer (ChamferUnion wrapper)
// ────────────────────────────────────────────────────────

/// 面取り (2 SdfNode を 45° chamfer union で結合)
///
/// fillet より加工しやすい直線的 edge、CNC / レーザー切断で標準
///
/// # 引数
///
/// - `a`, `b`: 結合対象の 2 SdfNode
/// - `size`: 面取り寸法 (mm、chamfer 幅)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::reinforcement::chamfer;
/// use alice_sdf::SdfNode;
/// use glam::Vec3;
/// let base = SdfNode::Box3d { half_extents: Vec3::new(10.0, 2.0, 20.0) };
/// let vertical = SdfNode::Box3d { half_extents: Vec3::new(2.0, 15.0, 20.0) };
/// let chamfered = chamfer(base, vertical, 1.0);
/// // L 字部品の内角に C1.0 面取り
/// ```
#[must_use]
pub fn chamfer(a: SdfNode, b: SdfNode, size: f32) -> SdfNode {
    SdfNode::ChamferUnion {
        a: Arc::new(a),
        b: Arc::new(b),
        r: size,
    }
}

// ────────────────────────────────────────────────────────
// 5. honeycomb infill
// ────────────────────────────────────────────────────────

/// ハニカム infill (container を 6 角形 cell の repeat から Subtraction、壁だけ残す)
///
/// 内部を 6 角形 pattern で刳り抜いて軽量化しつつ剛性を確保する
///
/// # 引数
///
/// - `container`: 中身を刳り抜く対象 SdfNode (bbox 内で cell を repeat)
/// - `cell_size`: 6 角形 cell の対辺距離 (mm、hexagon flat-to-flat)
/// - `wall_thickness`: 残す壁厚 (mm、cell 間の材料肉厚)
/// - `count`: 各軸方向の repeat 半径 (`2 * count + 1` 個生成、X-Z 平面)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::reinforcement::honeycomb_infill;
/// use alice_sdf::SdfNode;
/// use glam::Vec3;
/// let plate = SdfNode::Box3d { half_extents: Vec3::new(50.0, 5.0, 50.0) };
/// let lightweight = honeycomb_infill(plate, 8.0, 1.2, 6);
/// ```
#[must_use]
pub fn honeycomb_infill(
    container: SdfNode,
    cell_size: f32,
    wall_thickness: f32,
    count: u32,
) -> SdfNode {
    // 6 角形 hex_radius (中心から頂点までの距離) = cell_size / √3 (flat-to-flat / √3)
    // hex_prism は Y 軸押出、板は Y = 板厚方向想定 (rib と揃える)
    let hex_r = cell_size / 3.0_f32.sqrt();
    // wall_thickness 分だけ hex を縮小して壁を残す (cell 間ギャップ = wall)
    let cell_effective_r = (hex_r - wall_thickness * 0.5).max(0.0);
    let hex = SdfNode::HexPrism {
        hex_radius: cell_effective_r,
        half_height: 1000.0, // container で切るので大きめに取る
    };
    // 6 角形千鳥は複雑なので Phase A.3 では rect grid で単純化 (Phase A.4 で hex 千鳥化検討)
    // X 方向 pitch = cell_size × √3 (行間)、Z 方向 pitch = cell_size × 1.5 (列間)
    let repeated = SdfNode::RepeatFinite {
        child: Arc::new(hex),
        count: [count, 0, count],
        spacing: Vec3::new(cell_size * 3.0_f32.sqrt(), 1.0, cell_size * 1.5),
    };
    SdfNode::Subtraction {
        a: Arc::new(container),
        b: Arc::new(repeated),
    }
}

// ────────────────────────────────────────────────────────
// 6. gyroid infill (TPMS)
// ────────────────────────────────────────────────────────

/// Gyroid TPMS infill (container ∩ Gyroid 曲面壁)
///
/// Gyroid = 極小曲面、等方性・高剛性の TPMS ラティス
/// Bambu Studio infill 「Gyroid」相当を LOL レベルで直接生成する
///
/// # 引数
///
/// - `container`: 中身を Gyroid で埋める対象 SdfNode
/// - `cell_scale`: Gyroid 空間周波数 (小さいほど cell 大)
/// - `wall_thickness`: Gyroid 曲面の壁半厚 (mm)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::reinforcement::gyroid_infill;
/// use alice_sdf::SdfNode;
/// use glam::Vec3;
/// let cube = SdfNode::Box3d { half_extents: Vec3::new(20.0, 20.0, 20.0) };
/// let g = gyroid_infill(cube, 3.0, 0.4);
/// ```
#[must_use]
pub fn gyroid_infill(container: SdfNode, cell_scale: f32, wall_thickness: f32) -> SdfNode {
    let gyroid = SdfNode::Gyroid {
        scale: cell_scale,
        thickness: wall_thickness * 0.5,
    };
    SdfNode::Intersection {
        a: Arc::new(container),
        b: Arc::new(gyroid),
    }
}

// ────────────────────────────────────────────────────────
// alice-physics 連携 (opt-in、`physics` feature 必須)
// ────────────────────────────────────────────────────────

/// 材料名から弾性率 (GPa) を取得 (`alice_physics::filament_db::MaterialProperties`)
///
/// SSOT: Bamboo `safety.rs` と同じ FilamentDb を参照 hardcode 定数を置換
/// 対応材料: `"pla"` / `"petg"` / `"abs"` (大文字小文字不問)
///
/// # 使用例
///
/// ```ignore
/// // cargo test --features physics
/// use alice_lol::stdlib::hardsurface::reinforcement::material_elastic_modulus_gpa;
/// let e = material_elastic_modulus_gpa("pla");
/// assert!(e.is_some());
/// ```
#[cfg(feature = "physics")]
#[must_use]
pub fn material_elastic_modulus_gpa(name: &str) -> Option<f32> {
    use alice_physics::filament_db::MaterialProperties;
    let props = match name.to_ascii_lowercase().as_str() {
        "pla" => MaterialProperties::pla(),
        "petg" => MaterialProperties::petg(),
        "abs" => MaterialProperties::abs(),
        _ => return None,
    };
    Some(props.youngs_modulus_gpa.to_f32())
}

/// 段付き軸曲げの応力集中係数 Kt (`alice_physics::fillet_stress::kt_shaft_shoulder_bending`)
///
/// フィレット半径が大きいほど Kt が下がる (応力集中緩和)
///
/// # 引数
///
/// - `fillet_radius_mm`: フィレット半径 R (mm)
/// - `small_dia_mm`: 小径 d (mm)
/// - `large_dia_mm`: 大径 D (mm)
///
/// # 使用例
///
/// ```ignore
/// // cargo test --features physics
/// use alice_lol::stdlib::hardsurface::reinforcement::fillet_kt_shaft_shoulder;
/// let kt = fillet_kt_shaft_shoulder(0.5, 10.0, 20.0);
/// assert!(kt > 1.0);
/// ```
#[cfg(feature = "physics")]
#[must_use]
pub fn fillet_kt_shaft_shoulder(
    fillet_radius_mm: f32,
    small_dia_mm: f32,
    large_dia_mm: f32,
) -> f32 {
    use alice_physics::fillet_stress::kt_shaft_shoulder_bending;
    use alice_physics::math::Fix128;
    let kt = kt_shaft_shoulder_bending(
        Fix128::from_f32(fillet_radius_mm),
        Fix128::from_f32(small_dia_mm),
        Fix128::from_f32(large_dia_mm),
    );
    kt.to_f32()
}

/// 目標 Kt に対する推奨フィレット半径 (mm) を逆算 (`alice_physics::fillet_stress::recommended_fillet_radius_mm`)
///
/// # 引数
///
/// - `small_dia_mm`: 小径 d (mm)
/// - `large_dia_mm`: 大径 D (mm)
/// - `kt_target`: 目標応力集中係数
///
/// # 使用例
///
/// ```ignore
/// // cargo test --features physics
/// use alice_lol::stdlib::hardsurface::reinforcement::recommended_fillet_radius_mm;
/// let r = recommended_fillet_radius_mm(10.0, 20.0, 1.5);
/// assert!(r > 0.0);
/// ```
#[cfg(feature = "physics")]
#[must_use]
pub fn recommended_fillet_radius_mm(small_dia_mm: f32, large_dia_mm: f32, kt_target: f32) -> f32 {
    use alice_physics::fillet_stress::recommended_fillet_radius_mm as inner;
    use alice_physics::math::Fix128;
    inner(
        Fix128::from_f32(small_dia_mm),
        Fix128::from_f32(large_dia_mm),
        Fix128::from_f32(kt_target),
    )
    .to_f32()
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
    fn rib_is_box3d_with_expected_dimensions() {
        let node = rib(30.0, 5.0, 1.6);
        match node {
            SdfNode::Box3d { half_extents } => {
                assert!(approx_eq(half_extents.x, 15.0));
                assert!(approx_eq(half_extents.y, 2.5));
                assert!(approx_eq(half_extents.z, 0.8));
            }
            _ => panic!("expected Box3d"),
        }
    }

    #[test]
    fn boss_outer_diameter_uses_bamboo_screw_boss_od_ratio() {
        // M3 → 外径 = 3 * 2.2 = 6.6mm、外半径 3.3mm
        let node = boss(3.0, 8.0);
        match node {
            SdfNode::Subtraction { a, b: _ } => match &*a {
                SdfNode::Cylinder { radius, .. } => assert!(approx_eq(*radius, 3.3)),
                _ => panic!("expected outer Cylinder"),
            },
            _ => panic!("expected Subtraction"),
        }
    }

    #[test]
    fn boss_inner_hole_uses_tap_formula() {
        // M3 tap: 3 * 0.85 + 0.2 = 2.75mm 直径、半径 1.375mm
        let node = boss(3.0, 8.0);
        match node {
            SdfNode::Subtraction { a: _, b } => match &*b {
                SdfNode::Cylinder { radius, .. } => assert!(approx_eq(*radius, 1.375)),
                _ => panic!("expected inner Cylinder"),
            },
            _ => panic!("expected Subtraction"),
        }
    }

    #[test]
    fn fillet_returns_smooth_union() {
        let a = SdfNode::Sphere { radius: 1.0 };
        let b = SdfNode::Sphere { radius: 1.0 };
        let f = fillet(a, b, 0.3);
        match f {
            SdfNode::SmoothUnion { k, .. } => assert!(approx_eq(k, 0.3)),
            _ => panic!("expected SmoothUnion"),
        }
    }

    #[test]
    fn chamfer_returns_chamfer_union() {
        let a = SdfNode::Sphere { radius: 1.0 };
        let b = SdfNode::Sphere { radius: 1.0 };
        let c = chamfer(a, b, 0.5);
        match c {
            SdfNode::ChamferUnion { r, .. } => assert!(approx_eq(r, 0.5)),
            _ => panic!("expected ChamferUnion"),
        }
    }

    #[test]
    fn honeycomb_infill_returns_subtraction() {
        let container = SdfNode::Box3d {
            half_extents: Vec3::new(50.0, 5.0, 50.0),
        };
        let node = honeycomb_infill(container, 8.0, 1.2, 6);
        assert!(matches!(node, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn gyroid_infill_returns_intersection_with_gyroid() {
        let container = SdfNode::Box3d {
            half_extents: Vec3::new(20.0, 20.0, 20.0),
        };
        let node = gyroid_infill(container, 3.0, 0.4);
        match node {
            SdfNode::Intersection { a: _, b } => {
                assert!(matches!(&*b, SdfNode::Gyroid { .. }));
            }
            _ => panic!("expected Intersection with Gyroid"),
        }
    }

    #[test]
    fn boss_evaluation_at_origin_is_inside_tap_hole() {
        // 原点 (0, 0, 0) は tap 穴内部 = 物質「外」(穴は空間)
        let node = boss(3.0, 8.0);
        assert!(eval(&node, Vec3::ZERO) > 0.0);
    }

    #[test]
    fn boss_evaluation_between_tap_and_outer_is_inside_material() {
        // 半径 2.0mm 位置 (tap 1.375 外、外 3.3 内) は材料内部
        let node = boss(3.0, 8.0);
        assert!(eval(&node, Vec3::new(2.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn rib_evaluation_at_center_is_inside() {
        let node = rib(30.0, 5.0, 1.6);
        assert!(eval(&node, Vec3::ZERO) < 0.0);
    }

    #[test]
    fn all_reinforcement_primitives_produce_finite_sdf() {
        let container = SdfNode::Box3d {
            half_extents: Vec3::new(20.0, 20.0, 20.0),
        };
        let nodes = [
            rib(30.0, 5.0, 1.6),
            boss(3.0, 8.0),
            fillet(
                SdfNode::Sphere { radius: 1.0 },
                SdfNode::Sphere { radius: 1.0 },
                0.3,
            ),
            chamfer(
                SdfNode::Sphere { radius: 1.0 },
                SdfNode::Sphere { radius: 1.0 },
                0.5,
            ),
            honeycomb_infill(container.clone(), 8.0, 1.2, 6),
            gyroid_infill(container, 3.0, 0.4),
        ];
        for (i, node) in nodes.iter().enumerate() {
            let d = eval(node, Vec3::new(0.1, 0.1, 0.1));
            assert!(d.is_finite(), "primitive {i} produced non-finite SDF: {d}");
        }
    }

    #[cfg(feature = "physics")]
    #[test]
    fn physics_material_elastic_modulus_pla_matches_hardcode() {
        // Phase A.2 hardcode (3.5 GPa) と physics feature の FilamentDb 値が一致するか
        let e = material_elastic_modulus_gpa("pla").expect("PLA registered");
        // Bamboo FilamentDb の PLA elastic_modulus は 3.5 GPa (実装値)
        assert!((3.0..=4.0).contains(&e), "PLA elastic modulus = {e} GPa");
    }

    #[cfg(feature = "physics")]
    #[test]
    fn physics_fillet_kt_decreases_with_larger_radius() {
        // R を大きくすると Kt が下がる (応力集中緩和)
        let kt_small = fillet_kt_shaft_shoulder(0.2, 10.0, 20.0);
        let kt_large = fillet_kt_shaft_shoulder(2.0, 10.0, 20.0);
        assert!(
            kt_large < kt_small,
            "kt_large ({kt_large}) should be < kt_small ({kt_small})"
        );
    }

    #[cfg(feature = "physics")]
    #[test]
    fn physics_recommended_fillet_radius_positive() {
        let r = recommended_fillet_radius_mm(10.0, 20.0, 1.5);
        assert!(r > 0.0, "recommended R = {r}");
    }

    #[cfg(feature = "physics")]
    #[test]
    fn physics_unknown_material_returns_none() {
        assert!(material_elastic_modulus_gpa("unknown_polymer").is_none());
    }
}
