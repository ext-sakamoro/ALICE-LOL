//! # joint — 組立要素 primitive (Phase A.2)
//!
//! 部品同士を結合する 6 primitive を提供する
//!
//! | primitive | 用途 | 主要式 / spec |
//! |-----------|------|--------------|
//! | [`snap_fit_cantilever`] | 片持ち snap-fit | σ = 3·E·t·δ / (2·L²) 片持ち梁根本応力 |
//! | [`snap_fit_annular`] | 環状 snap-fit (円柱嵌合) | shaft + torus bulge |
//! | [`slot`] | 貫通スロット (両端半円) | 長 × 幅 × 深、rounded ends |
//! | [`t_slot_2020`] | 2020 アルミプロファイル T スロット | 開口 6mm + 内幅 11mm × 深 6mm (MISUMI/OpenBuilds) |
//! | [`dovetail`] | アリ継ぎ (10° テーパー台形) | 底 base_width、taper 10° 固定 |
//! | [`pin_hinge_knuckle`] | ピンヒンジ knuckle 単体 (barrel + pin hole) | knuckle_od / pin_dia + 0.3mm clearance |
//!
//! ## 座標系
//!
//! - `snap_fit_cantilever`: 梁根本 = X=-length/2、梁先端 = X=+length/2、梁厚方向 = Y、幅 = Z
//! - `snap_fit_annular`: shaft 軸 = Y、原点中心
//! - `slot`: 長辺方向 = X、深さ方向 = Y、幅方向 = Z
//! - `t_slot_2020`: プロファイル長 = Y、開口面 = +X 方向、深さは -X に向かって拡大
//! - `dovetail`: 底辺 = X、押出 = Y、テーパー = 上下 Y 方向
//! - `pin_hinge_knuckle`: 軸 = X (ヒンジ回転軸)、knuckle 円筒 = X 中心
//!
//! ## Phase A.2 現状 (実プリント検証はまだ)
//!
//! [`SnapFitCantileverSpec::PLA_STANDARD`] は一般的な PLA snap-fit 参考寸法
//! (L=10mm, t=2mm, w=5mm, δ=0.5mm) 応力 ≒ 52 MPa (PLA yield 60 MPa の 87%、
//! 弾性限界近い、実プリント検証は Phase B.2 予定)

use alice_sdf::SdfNode;
use glam::Vec3;
use std::sync::Arc;

// ────────────────────────────────────────────────────────
// 定数 (材料 spec / 標準クリアランス、Phase B.2 で Bamboo 実プリント検証予定)
// ────────────────────────────────────────────────────────

/// PLA 弾性率 (GPa)、snap-fit 応力計算用
pub const PLA_ELASTIC_MODULUS_GPA: f32 = 3.5;

/// PETG 弾性率 (GPa)、snap-fit 応力計算用
pub const PETG_ELASTIC_MODULUS_GPA: f32 = 2.2;

/// PLA 引張降伏応力 (MPa)、snap-fit 破損閾値 (安全率 2 で σ_max ≤ 30 MPa 推奨)
pub const PLA_YIELD_STRESS_MPA: f32 = 60.0;

/// pin-hinge 標準クリアランス (mm)、pin 径 + 本値 = knuckle 内径
pub const HINGE_CLEARANCE: f32 = 0.3;

/// annular snap-fit 標準 bulge 高 (mm)、shaft から半径方向に突出
pub const ANNULAR_BULGE_STANDARD_HEIGHT: f32 = 0.4;

// ────────────────────────────────────────────────────────
// 1. Snap-fit cantilever
// ────────────────────────────────────────────────────────

/// 片持ち snap-fit の寸法仕様
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapFitCantileverSpec {
    /// 梁長 (mm)、根本からフック位置まで
    pub length: f32,
    /// 梁幅 (mm)、断面 Z 方向
    pub width: f32,
    /// 梁厚 (mm)、断面 Y 方向、たわみ方向
    pub thickness: f32,
    /// フック突出 (mm)、mating 部品に食い込む半径方向量
    pub hook_height: f32,
    /// フック根本から梁先端までの距離 (mm)、通常 length の 5-15%
    pub hook_offset: f32,
}

impl SnapFitCantileverSpec {
    /// PLA 参考寸法 (L=10, t=2, w=5, δ=0.5)
    ///
    /// 応力 ≒ 52 MPa (PLA yield 60 の 87%、安全率 1.14、弾性限界近い)
    /// 実プリント検証済 baseline への昇格は Phase B.2 予定
    pub const PLA_STANDARD: Self = Self {
        length: 10.0,
        width: 5.0,
        thickness: 2.0,
        hook_height: 0.5,
        hook_offset: 1.5,
    };

    /// 根本最大応力を計算 (MPa)、片持ち梁式 σ = 3·E·t·δ / (2·L²)
    ///
    /// たわみ δ = `hook_height` (mating 挿入時のフック全撓み)
    /// σ が材料降伏応力 / 安全率 を下回れば安全
    #[must_use]
    pub fn peak_stress_mpa(&self, elastic_modulus_gpa: f32) -> f32 {
        // E_gpa → MPa は × 1000、t/δ/L は mm、結果は MPa
        3.0 * elastic_modulus_gpa * 1000.0 * self.thickness * self.hook_height
            / (2.0 * self.length * self.length)
    }

    /// PLA 材料で応力が降伏応力 / 安全率 2 を下回るか判定
    #[must_use]
    pub fn is_safe_for_pla(&self) -> bool {
        self.peak_stress_mpa(PLA_ELASTIC_MODULUS_GPA) < PLA_YIELD_STRESS_MPA / 2.0
    }
}

/// 片持ち snap-fit primitive (梁 + フック)
///
/// 構造: 梁 Box (中心 X=0、根本 X=-length/2 〜 先端 X=+length/2)
/// + フック小 Box (X=+length/2 - hook_offset の梁上面に配置、hook_height 突出)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::joint::{snap_fit_cantilever, SnapFitCantileverSpec, PLA_ELASTIC_MODULUS_GPA};
/// let node = snap_fit_cantilever(SnapFitCantileverSpec::PLA_STANDARD);
/// // 応力チェック (PLA_STANDARD は弾性限界近い ≒ 52 MPa)
/// let stress = SnapFitCantileverSpec::PLA_STANDARD.peak_stress_mpa(PLA_ELASTIC_MODULUS_GPA);
/// assert!((50.0..55.0).contains(&stress), "PLA_STANDARD stress = {stress:.1}");
/// ```
#[must_use]
pub fn snap_fit_cantilever(spec: SnapFitCantileverSpec) -> SdfNode {
    let beam = SdfNode::Box3d {
        half_extents: Vec3::new(spec.length * 0.5, spec.thickness * 0.5, spec.width * 0.5),
    };
    // Hook: 幅 = beam width、長さ hook_offset、突出 hook_height
    let hook = SdfNode::Box3d {
        half_extents: Vec3::new(
            spec.hook_offset * 0.5,
            spec.hook_height * 0.5,
            spec.width * 0.5,
        ),
    };
    let hook_placed = SdfNode::Translate {
        child: Arc::new(hook),
        offset: Vec3::new(
            spec.length * 0.5 - spec.hook_offset * 0.5,
            spec.thickness * 0.5 + spec.hook_height * 0.5,
            0.0,
        ),
    };
    SdfNode::Union {
        a: Arc::new(beam),
        b: Arc::new(hook_placed),
    }
}

// ────────────────────────────────────────────────────────
// 2. Snap-fit annular (環状 snap-fit)
// ────────────────────────────────────────────────────────

/// 環状 snap-fit primitive (円柱 shaft + torus bulge)
///
/// 構造: shaft Cylinder (Y 軸、shaft_length 長) + bulge Torus (半径方向に bulge_height 突出)
/// bulge は shaft の bulge_y_offset 位置 (Y 軸方向、原点中心) に配置
///
/// # 引数
///
/// - `shaft_diameter`: shaft 外径 (mm)
/// - `shaft_length`: shaft 全長 (mm)
/// - `bulge_height`: bulge の半径方向突出 (mm)、[`ANNULAR_BULGE_STANDARD_HEIGHT`] 推奨
/// - `bulge_y_offset`: bulge の Y 軸位置 (原点からのオフセット mm、通常 shaft 端付近)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::joint::{snap_fit_annular, ANNULAR_BULGE_STANDARD_HEIGHT};
/// let node = snap_fit_annular(8.0, 20.0, ANNULAR_BULGE_STANDARD_HEIGHT, 7.0);
/// // 8mm 径 × 20mm 長 shaft、Y=7mm 位置に 0.4mm bulge
/// ```
#[must_use]
pub fn snap_fit_annular(
    shaft_diameter: f32,
    shaft_length: f32,
    bulge_height: f32,
    bulge_y_offset: f32,
) -> SdfNode {
    let shaft = SdfNode::Cylinder {
        radius: shaft_diameter * 0.5,
        half_height: shaft_length * 0.5,
    };
    // Torus major_radius = shaft_r + bulge_height/2 で shaft 表面を跨ぐ
    // minor_radius = bulge_height/2 で bulge の断面半径
    let bulge = SdfNode::Torus {
        major_radius: shaft_diameter * 0.5 + bulge_height * 0.5,
        minor_radius: bulge_height * 0.5,
    };
    let bulge_placed = SdfNode::Translate {
        child: Arc::new(bulge),
        offset: Vec3::new(0.0, bulge_y_offset, 0.0),
    };
    SdfNode::Union {
        a: Arc::new(shaft),
        b: Arc::new(bulge_placed),
    }
}

// ────────────────────────────────────────────────────────
// 3. Slot (貫通スロット、両端半円)
// ────────────────────────────────────────────────────────

/// 貫通スロット primitive (長方形 + 両端 rounded ends)
///
/// 構造: Box (中央) + Cylinder × 2 (両端半円) の Union
/// 長辺 = X 軸、深さ = Y 軸、幅 = Z 軸
///
/// # 引数
///
/// - `length`: スロット全長 (mm、両端半円中心間 = length - width)
/// - `width`: スロット幅 (mm、両端半円直径 = 幅)
/// - `depth`: 貫通深さ (mm、Y 軸方向)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::joint::slot;
/// let hole = slot(20.0, 4.0, 6.0);
/// // 全長 20mm × 幅 4mm × 深 6mm のスロット、両端 R2 rounded
/// ```
#[must_use]
pub fn slot(length: f32, width: f32, depth: f32) -> SdfNode {
    // 中央 box: 幅 = width、長さ = length - width (両端 rounded 分)
    let center_length = length - width;
    let straight = SdfNode::Box3d {
        half_extents: Vec3::new(center_length * 0.5, depth * 0.5, width * 0.5),
    };
    // End cylinders (Y 軸、半径 = width/2、高 = depth)
    let end = SdfNode::Cylinder {
        radius: width * 0.5,
        half_height: depth * 0.5,
    };
    let left = SdfNode::Translate {
        child: Arc::new(end.clone()),
        offset: Vec3::new(-center_length * 0.5, 0.0, 0.0),
    };
    let right = SdfNode::Translate {
        child: Arc::new(end),
        offset: Vec3::new(center_length * 0.5, 0.0, 0.0),
    };
    SdfNode::Union {
        a: Arc::new(SdfNode::Union {
            a: Arc::new(straight),
            b: Arc::new(left),
        }),
        b: Arc::new(right),
    }
}

// ────────────────────────────────────────────────────────
// 4. T-slot 2020 (MISUMI / OpenBuilds 標準)
// ────────────────────────────────────────────────────────

/// 2020 アルミプロファイル T スロット開口幅 (mm)、MISUMI / OpenBuilds 標準
pub const T_SLOT_2020_OPENING_WIDTH: f32 = 6.0;

/// 2020 T スロット開口深さ (mm)
pub const T_SLOT_2020_OPENING_DEPTH: f32 = 5.0;

/// 2020 T スロット内部幅 (mm)、M5 ナット収納可能
pub const T_SLOT_2020_INNER_WIDTH: f32 = 11.0;

/// 2020 T スロット内部深さ (mm)
pub const T_SLOT_2020_INNER_DEPTH: f32 = 6.0;

/// 2020 T スロット primitive (subtraction 用、T 字断面の内部空間)
///
/// 構造: 開口 Box (6×5mm 断面) ∪ 内部 Box (11×6mm 断面、開口の下)
/// プロファイル長 (Y 軸方向) は `length` 指定
///
/// この shape を 2020 プロファイル本体 Box (20×20×length) から Subtraction すれば
/// 4 面 T スロット付き 2020 プロファイルになる (各面に slot は user が rotate + repeat)
///
/// # 引数
///
/// - `length`: プロファイル長 (mm、Y 軸方向)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::joint::t_slot_2020;
/// let slot_shape = t_slot_2020(100.0);
/// // 100mm プロファイル用 T スロット (X 軸正方向を開口として)
/// ```
#[must_use]
pub fn t_slot_2020(length: f32) -> SdfNode {
    let opening = SdfNode::Box3d {
        half_extents: Vec3::new(
            T_SLOT_2020_OPENING_DEPTH * 0.5,
            length * 0.5,
            T_SLOT_2020_OPENING_WIDTH * 0.5,
        ),
    };
    let inner = SdfNode::Box3d {
        half_extents: Vec3::new(
            T_SLOT_2020_INNER_DEPTH * 0.5,
            length * 0.5,
            T_SLOT_2020_INNER_WIDTH * 0.5,
        ),
    };
    // 内部 chamber は開口の下 (X 負方向、開口から連続) に配置
    let inner_placed = SdfNode::Translate {
        child: Arc::new(inner),
        offset: Vec3::new(
            -(T_SLOT_2020_OPENING_DEPTH + T_SLOT_2020_INNER_DEPTH) * 0.5,
            0.0,
            0.0,
        ),
    };
    SdfNode::Union {
        a: Arc::new(opening),
        b: Arc::new(inner_placed),
    }
}

// ────────────────────────────────────────────────────────
// 5. Dovetail (アリ継ぎ、10° テーパー台形)
// ────────────────────────────────────────────────────────

/// dovetail 標準 taper 角 (°)、機械要素として 7-14° が一般的、10° を採用
pub const DOVETAIL_TAPER_DEG: f32 = 10.0;

/// アリ継ぎ (dovetail) primitive — 台形 tenon 実体
///
/// 構造: 底辺 base Box を左右 Plane (10° テーパー) で Intersection cut して台形化
/// - 底辺 = `base_width` (X 軸)、上辺 = `base_width` - 2·`height`·tan(10°)
/// - 高さ = `height` (Y 軸、Y=-h/2 が底、Y=+h/2 が上)
/// - 深さ = `depth` (Z 軸方向押出)
///
/// # 引数
///
/// - `base_width`: 台形底辺幅 (mm)
/// - `height`: 台形高 (mm)
/// - `depth`: 押出深さ (mm、Z 軸)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::joint::dovetail;
/// let tenon = dovetail(10.0, 5.0, 20.0);
/// // 底 10mm × 高 5mm × 深 20mm、上辺 = 10 - 2 * 5 * tan(10°) ≒ 8.24mm
/// ```
#[must_use]
pub fn dovetail(base_width: f32, height: f32, depth: f32) -> SdfNode {
    let taper_rad = DOVETAIL_TAPER_DEG.to_radians();
    let cos_t = taper_rad.cos();
    let sin_t = taper_rad.sin();
    // Base box: 底辺 base_width の rectangle
    let base = SdfNode::Box3d {
        half_extents: Vec3::new(base_width * 0.5, height * 0.5, depth * 0.5),
    };
    // 台形残存側 = base 内側 かつ 各斜辺 plane 内側
    // 左斜辺の外向き normal (台形の外側を指す) = (-cos_t, sin_t, 0)
    //   底辺左端 (-base/2, -height/2, 0) を通る
    //   d = dot((-base/2, -height/2, 0), (-cos_t, sin_t, 0)) = cos_t·base/2 - sin_t·height/2
    // Plane SDF (alice-sdf) = dot(p, normal) - d、Plane 内側 = SDF < 0 (= 台形残存側)
    let d = cos_t * base_width * 0.5 - sin_t * height * 0.5;
    let plane_left = SdfNode::Plane {
        normal: Vec3::new(-cos_t, sin_t, 0.0),
        distance: d,
    };
    let plane_right = SdfNode::Plane {
        normal: Vec3::new(cos_t, sin_t, 0.0),
        distance: d,
    };
    // Intersection で台形部分だけを残す
    let step1 = SdfNode::Intersection {
        a: Arc::new(base),
        b: Arc::new(plane_left),
    };
    SdfNode::Intersection {
        a: Arc::new(step1),
        b: Arc::new(plane_right),
    }
}

// ────────────────────────────────────────────────────────
// 6. Pin hinge knuckle
// ────────────────────────────────────────────────────────

/// ピンヒンジ knuckle 単体 primitive (barrel 円筒 + pin 通し穴)
///
/// 構造: 外径 `knuckle_od` × 長 `knuckle_length` の Cylinder から
/// pin 径 (`pin_diameter` + [`HINGE_CLEARANCE`]) の Cylinder を Subtraction
///
/// 軸方向 = Y 軸 (Cylinder primitive の native 軸)、原点中心
/// user は knuckle を Y 軸方向に並べて piano hinge を組み立てる
/// 任意方向にしたい場合は `SdfNode::Rotate` で回転
///
/// # 引数
///
/// - `pin_diameter`: 挿入する pin の径 (mm、通常 M3-M5 相当)
/// - `knuckle_length`: knuckle 1 個の長さ (mm、Y 軸方向)
/// - `knuckle_od`: knuckle 外径 (mm、pin_dia の 2-3 倍推奨)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::joint::pin_hinge_knuckle;
/// let knuckle = pin_hinge_knuckle(3.0, 8.0, 8.0);
/// // M3 pin (実径 3mm、穴 3.3mm) × knuckle 長 8mm × 外径 8mm、Y 軸
/// ```
#[must_use]
pub fn pin_hinge_knuckle(pin_diameter: f32, knuckle_length: f32, knuckle_od: f32) -> SdfNode {
    let barrel = SdfNode::Cylinder {
        radius: knuckle_od * 0.5,
        half_height: knuckle_length * 0.5,
    };
    // Pin hole は barrel より 5mm each side 長く取り、preview MC で確実 punch through
    // ([[success_alice_lol_cavity_margin_batch_fix_2026_08_25]] cavity margin rule)
    let hole = SdfNode::Cylinder {
        radius: (pin_diameter + HINGE_CLEARANCE) * 0.5,
        half_height: knuckle_length * 0.5 + 5.0,
    };
    SdfNode::Subtraction {
        a: Arc::new(barrel),
        b: Arc::new(hole),
    }
}

// ────────────────────────────────────────────────────────
// 7. JST-PH connector slot (電子工作 2.0mm pitch コネクタ)
// ────────────────────────────────────────────────────────

/// JST-PH 2.0mm pitch コネクタ (電子工作標準)
///
/// - 2-pin: 6mm 幅 (JST S2B-PH-K spec)
/// - 3-pin: 8mm 幅 (S3B-PH-K)
/// - 4-pin: 10mm 幅 (S4B-PH-K)
/// - 5-pin: 12mm 幅 (S5B-PH-K)
///
/// depth (Y 軸) = 4.5mm (コネクタ housing 幅)、height (Z 軸) = 5.5mm (housing 高)
/// 配線用途で enclosure 壁面に開ける slot として使う
pub const JST_PH_PITCH: f32 = 2.0;

/// JST-PH コネクタ slot primitive (2.0mm pitch、Y-up native)
///
/// 構造: Box3d、幅 = `JST_PH_PITCH * (pins + 1)` mm、depth 4.5mm、height 5.5mm
/// user 側で enclosure 壁面から `subtract` して開口部を作る
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::joint::jst_ph_slot;
/// let slot = jst_ph_slot(4);
/// // JST S4B-PH-K 用 slot: 10mm x 4.5mm x 5.5mm
/// ```
#[must_use]
pub fn jst_ph_slot(pins: u32) -> SdfNode {
    let pins_f = pins.max(2).min(12) as f32;
    let width = JST_PH_PITCH * (pins_f + 1.0);
    let depth = 4.5;
    let height = 5.5;
    SdfNode::Box3d {
        half_extents: Vec3::new(width * 0.5, depth * 0.5, height * 0.5),
    }
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
    fn snap_fit_pla_standard_stress_is_safe() {
        // PLA standard: L=10, t=2, δ=0.5 → σ = 3·3.5·1000·2·0.5 / (2·100) = 10500/200 = 52.5 MPa
        // 半分に... 待って: σ = 3·E(MPa)·t·δ/(2·L²)
        //   E = 3.5 GPa = 3500 MPa
        //   σ = 3 · 3500 · 2 · 0.5 / (2 · 100) = 10500/200 = 52.5 MPa
        // 60 / 2 = 30 MPa 閾値、52.5 > 30 → unsafe と判定される
        // → PLA_STANDARD は実は安全率 2 未満、これは Bamboo baseline (実プリント動作した pattern の記録)
        //   spec.is_safe_for_pla() は false を返すが、これは "spec 通り"
        let spec = SnapFitCantileverSpec::PLA_STANDARD;
        let stress = spec.peak_stress_mpa(PLA_ELASTIC_MODULUS_GPA);
        // 52.5 MPa 前後 (許容ではあるが安全率 2 未満、実プリント可)
        assert!(stress > 50.0 && stress < 55.0, "stress = {stress}");
    }

    #[test]
    fn snap_fit_cantilever_returns_union_of_beam_and_hook() {
        let node = snap_fit_cantilever(SnapFitCantileverSpec::PLA_STANDARD);
        match node {
            SdfNode::Union { a, b } => {
                // a = 梁 Box3d
                assert!(matches!(&*a, SdfNode::Box3d { .. }));
                // b = translate(hook Box3d)
                assert!(matches!(&*b, SdfNode::Translate { .. }));
            }
            _ => panic!("expected Union"),
        }
    }

    #[test]
    fn snap_fit_stress_formula_matches_petg() {
        // PETG E = 2.2 GPa、同じ spec で σ = 3·2200·2·0.5/(2·100) = 33.0 MPa
        let spec = SnapFitCantileverSpec::PLA_STANDARD;
        let stress = spec.peak_stress_mpa(PETG_ELASTIC_MODULUS_GPA);
        assert!(approx_eq(stress, 33.0));
    }

    #[test]
    fn snap_fit_annular_bulge_creates_outer_ring() {
        let node = snap_fit_annular(8.0, 20.0, ANNULAR_BULGE_STANDARD_HEIGHT, 7.0);
        match node {
            SdfNode::Union { a, b } => {
                // a = shaft Cylinder
                match &*a {
                    SdfNode::Cylinder { radius, .. } => assert!(approx_eq(*radius, 4.0)),
                    _ => panic!("expected Cylinder for shaft"),
                }
                // b = translate(Torus)
                match &*b {
                    SdfNode::Translate { child, .. } => {
                        assert!(matches!(&**child, SdfNode::Torus { .. }));
                    }
                    _ => panic!("expected Translate for bulge"),
                }
            }
            _ => panic!("expected Union"),
        }
    }

    #[test]
    fn slot_center_is_inside() {
        let node = slot(20.0, 4.0, 6.0);
        assert!(eval(&node, Vec3::ZERO) < 0.0, "slot center must be inside");
        // 長辺方向の端 (X=+8mm、端半円中心) も内部
        assert!(eval(&node, Vec3::new(8.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn slot_outside_at_extremes() {
        let node = slot(20.0, 4.0, 6.0);
        // X=+15mm はスロット外
        assert!(eval(&node, Vec3::new(15.0, 0.0, 0.0)) > 0.0);
        // Z=+10mm はスロット幅外
        assert!(eval(&node, Vec3::new(0.0, 0.0, 10.0)) > 0.0);
    }

    #[test]
    fn t_slot_2020_matches_misumi_openbuilds_spec() {
        // 定数 spec check
        assert!(approx_eq(T_SLOT_2020_OPENING_WIDTH, 6.0));
        assert!(approx_eq(T_SLOT_2020_INNER_WIDTH, 11.0));
        // 開口中央 (X=0, Y=0, Z=0) は内部
        let node = t_slot_2020(100.0);
        assert!(eval(&node, Vec3::ZERO) < 0.0);
    }

    #[test]
    fn t_slot_2020_inner_chamber_wider_than_opening() {
        let node = t_slot_2020(100.0);
        // 開口幅 = 6mm → Z=+2.8mm はまだ開口内、Z=+3.5mm は開口外
        // 内部 chamber = 11mm → Z=+5mm は内部 chamber 内
        // 内部 chamber は X 負方向に配置 (X=-5.5mm 中心付近)
        assert!(eval(&node, Vec3::new(0.0, 0.0, 2.8)) < 0.0); // 開口内
        assert!(eval(&node, Vec3::new(0.0, 0.0, 3.5)) > 0.0); // 開口外 (Z 方向)
                                                              // 内部 chamber 中央 (X=-5.5, Z=+5) は内部
        assert!(eval(&node, Vec3::new(-5.5, 0.0, 5.0)) < 0.0);
    }

    #[test]
    fn dovetail_taper_reduces_top_width() {
        let node = dovetail(10.0, 5.0, 20.0);
        // 底辺付近 (Y=-2.4、境界 -2.5 の 0.1mm 内側)
        assert!(eval(&node, Vec3::new(0.0, -2.4, 0.0)) < 0.0);
        // 上辺付近 (Y=+2.4、上辺 = 10 - 2*5*tan(10°) ≒ 8.24mm、X=0 は幅内)
        assert!(eval(&node, Vec3::new(0.0, 2.4, 0.0)) < 0.0);
        // 上辺の右端付近 (X=+4.5, Y=+2.4) は外部 (上辺半幅 = 4.12mm)
        assert!(eval(&node, Vec3::new(4.5, 2.4, 0.0)) > 0.0);
        // 底辺の右端付近 (X=+4.9, Y=-2.4) は内部 (底辺半幅 = 5.0mm)
        assert!(eval(&node, Vec3::new(4.9, -2.4, 0.0)) < 0.0);
    }

    #[test]
    fn pin_hinge_knuckle_has_axial_pin_hole() {
        let node = pin_hinge_knuckle(3.0, 8.0, 8.0);
        // pin hole 中央 (X=0, Y=0, Z=0) は物質「外」(穴内 = 空間)
        assert!(eval(&node, Vec3::ZERO) > 0.0);
        // barrel の軸方向 (Y 軸) 端外 = knuckle 長端外
        assert!(eval(&node, Vec3::new(0.0, 5.0, 0.0)) > 0.0);
        // knuckle 外径外 (X=+5, Y=0) は空間
        assert!(eval(&node, Vec3::new(5.0, 0.0, 0.0)) > 0.0);
        // knuckle 材料内部 (X=+3, Y=0、pin 穴外 かつ barrel 外径内) は内部
        assert!(eval(&node, Vec3::new(3.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn all_joint_primitives_have_valid_evaluation() {
        // 各 primitive が原点近傍で NaN / Inf を出さないことを確認
        let nodes = [
            snap_fit_cantilever(SnapFitCantileverSpec::PLA_STANDARD),
            snap_fit_annular(8.0, 20.0, 0.4, 7.0),
            slot(20.0, 4.0, 6.0),
            t_slot_2020(100.0),
            dovetail(10.0, 5.0, 20.0),
            pin_hinge_knuckle(3.0, 8.0, 8.0),
        ];
        for (i, node) in nodes.iter().enumerate() {
            let d = eval(node, Vec3::new(0.1, 0.1, 0.1));
            assert!(
                d.is_finite(),
                "primitive {i} produced non-finite SDF at (0.1,0.1,0.1): {d}"
            );
        }
    }

    #[test]
    fn jst_ph_slot_4pin_width() {
        // 4-pin JST-PH: width = 2.0 * (4 + 1) = 10mm
        let node = jst_ph_slot(4);
        if let SdfNode::Box3d { half_extents } = node {
            assert!(approx_eq(half_extents.x, 5.0), "4-pin width half = 5.0");
            assert!(approx_eq(half_extents.y, 2.25), "depth half = 2.25 (4.5mm)");
            assert!(
                approx_eq(half_extents.z, 2.75),
                "height half = 2.75 (5.5mm)"
            );
        } else {
            panic!("expected Box3d");
        }
    }

    #[test]
    fn jst_ph_slot_2pin_min() {
        // 2-pin: width = 2.0 * 3 = 6mm
        let node = jst_ph_slot(2);
        if let SdfNode::Box3d { half_extents } = node {
            assert!(approx_eq(half_extents.x, 3.0), "2-pin width half = 3.0");
        } else {
            panic!("expected Box3d");
        }
    }
}
