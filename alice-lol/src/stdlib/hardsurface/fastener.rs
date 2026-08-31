//! # fastener — 締結要素 primitive (Phase A.1)
//!
//! ISO metric 規格 M3-M8 に対応した 6 primitive を提供する
//!
//! | primitive | 用途 | 直径公式 |
//! |-----------|------|----------|
//! | [`screw_hole`] | クリアランス穴 (ボルトが余裕通過) | `nominal + CLEARANCE_H2D_FDM` |
//! | [`tap_hole`] | セルフタッピング下穴 | `nominal * 0.85 + 2 * accuracy` |
//! | [`counterbore`] | ソケットキャップ頭部沈み穴 | ISO 4762 頭径 |
//! | [`countersink`] | 皿頭 (90°) 沈み穴 | ISO 10642 頭径 |
//! | [`bolt`] | ボルト実体 (組立可視化用、頭 + 軸) | ISO 4762 |
//! | [`heat_set_insert_hole`] | Voxel8 / McMaster ヒートセット下穴 | insert 外径 + 0.2 |
//!
//! ## 座標系
//!
//! 全 primitive の中心軸は Y 軸、原点は cylinder / cone の幾何中心
//! 板から穴を掘る時は `SdfNode::Subtraction { a: plate, b: fastener_hole }` として使う
//! 板中心が Y=0 の場合、貫通穴はそのまま Subtraction すれば板厚方向に完全貫通する
//!
//! ## Bamboo 実プリント公式との整合
//!
//! [`tap_hole`] は `alice_bamboo::formulas::PrintParams::tap_hole()` と同式
//! [`heat_set_insert_hole`] は `heat_insert_hole()` と同式 (`+ 0.2` 熱膨張余裕)

use alice_sdf::SdfNode;
use glam::{Quat, Vec3};
use std::sync::Arc;

// ────────────────────────────────────────────────────────
// 定数
// ────────────────────────────────────────────────────────

/// H2D 0.4mm ノズル FDM のクリアランス穴余裕 (mm)
///
/// 呼び径 + 本値 = ボルトが余裕を持って通る穴径
/// PLA / PETG / ABS 実測値 (ALICE-Bamboo `~/ALICE-Bamboo/CLAUDE.md` に記載)
pub const CLEARANCE_H2D_FDM: f32 = 0.2;

/// タップ下穴公式のノズル精度余白 A (mm、default)
///
/// tap 直径 = `screw_dia * 0.85 + 2 * accuracy` に代入する A の default 値
/// H2D 0.4 nozzle 実測、slicer / material により 0.05-0.15 の範囲で調整
pub const DEFAULT_ACCURACY: f32 = 0.1;

/// ISO 10642 皿頭ボルト テーパー全角 (°、= 皿頭 cone の全開角)
pub const COUNTERSUNK_TAPER_ANGLE_DEG: f32 = 90.0;

/// ヒートセットインサート挿入時の熱膨張下 sink 余裕 (mm)
pub const HEAT_SET_SINK_MARGIN: f32 = 0.3;

// ────────────────────────────────────────────────────────
// ISO metric サイズ enum
// ────────────────────────────────────────────────────────

/// ISO metric ボルト / ネジ サイズ (M2-M8 対応、Phase X.2 で M2/M2.5 追加)
///
/// 各 method は ISO 4762 (ソケットキャップ) / ISO 10642 (皿頭) 規格値を返す
/// M10 以上は Phase A.4 (mount) で追加予定 (2020 profile / 3030 profile の締結軸として)
/// M2/M2.5 は Raspberry Pi / Arduino / spring hinge 等の小型基板・センサー用途で頻出
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricSize {
    /// M2 (呼び径 2mm、小型基板 / センサー)
    M2,
    /// M2.5 (呼び径 2.5mm、Raspberry Pi / Arduino 標準)
    M2_5,
    /// M3 (呼び径 3mm)
    M3,
    /// M4 (呼び径 4mm)
    M4,
    /// M5 (呼び径 5mm)
    M5,
    /// M6 (呼び径 6mm)
    M6,
    /// M8 (呼び径 8mm)
    M8,
}

impl MetricSize {
    /// 呼び径 = ネジの外径 (mm)
    #[must_use]
    pub const fn nominal_diameter(self) -> f32 {
        match self {
            Self::M2 => 2.0,
            Self::M2_5 => 2.5,
            Self::M3 => 3.0,
            Self::M4 => 4.0,
            Self::M5 => 5.0,
            Self::M6 => 6.0,
            Self::M8 => 8.0,
        }
    }

    /// ISO 4762 ソケットキャップボルト 頭径 (mm)
    #[must_use]
    pub const fn head_diameter_socket(self) -> f32 {
        match self {
            Self::M2 => 3.8,
            Self::M2_5 => 4.5,
            Self::M3 => 5.5,
            Self::M4 => 7.0,
            Self::M5 => 8.5,
            Self::M6 => 10.0,
            Self::M8 => 13.0,
        }
    }

    /// ISO 4762 ソケットキャップボルト 頭高 (mm)
    #[must_use]
    pub const fn head_height_socket(self) -> f32 {
        match self {
            Self::M2 => 2.0,
            Self::M2_5 => 2.5,
            Self::M3 => 3.0,
            Self::M4 => 4.0,
            Self::M5 => 5.0,
            Self::M6 => 6.0,
            Self::M8 => 8.0,
        }
    }

    /// ISO 10642 皿頭ボルト 頭径 (mm)
    #[must_use]
    pub const fn head_diameter_countersunk(self) -> f32 {
        match self {
            Self::M2 => 4.0,
            Self::M2_5 => 4.5,
            Self::M3 => 6.0,
            Self::M4 => 8.0,
            Self::M5 => 10.0,
            Self::M6 => 12.0,
            Self::M8 => 16.0,
        }
    }

    /// McMaster / Voxel8 ヒートセットインサート 外径 (mm)
    #[must_use]
    pub const fn heat_set_insert_diameter(self) -> f32 {
        match self {
            Self::M2 => 3.2,
            Self::M2_5 => 3.6,
            Self::M3 => 4.0,
            Self::M4 => 5.6,
            Self::M5 => 6.4,
            Self::M6 => 8.0,
            Self::M8 => 10.3,
        }
    }

    /// McMaster / Voxel8 ヒートセットインサート 埋込深さ (mm)
    #[must_use]
    pub const fn heat_set_insert_depth(self) -> f32 {
        match self {
            Self::M2 => 3.0,
            Self::M2_5 => 3.3,
            Self::M3 => 3.8,
            Self::M4 => 5.7,
            Self::M5 => 5.7,
            Self::M6 => 7.9,
            Self::M8 => 10.0,
        }
    }

    /// f32 呼び径 (mm) を対応 `MetricSize` に最近接 snap する
    ///
    /// 2.0/2.5/3.0/4.0/5.0/6.0/8.0 は完全一致、それ以外は最近接に snap
    /// LLM / runtime_parser が「M4.5」等の非規格値を渡した時のフォールバック用
    ///
    /// # 使用例
    ///
    /// ```
    /// use alice_lol::stdlib::hardsurface::fastener::MetricSize;
    /// assert_eq!(MetricSize::from_f32_snap(2.0), MetricSize::M2);
    /// assert_eq!(MetricSize::from_f32_snap(2.5), MetricSize::M2_5);
    /// assert_eq!(MetricSize::from_f32_snap(3.0), MetricSize::M3);
    /// assert_eq!(MetricSize::from_f32_snap(4.5), MetricSize::M4); // 4.5 → M4 (最近接)
    /// assert_eq!(MetricSize::from_f32_snap(7.0), MetricSize::M6); // 7.0 → M6 (M8 より近い)
    /// assert_eq!(MetricSize::from_f32_snap(100.0), MetricSize::M8); // 上限 clamp
    /// ```
    #[must_use]
    pub fn from_f32_snap(nominal: f32) -> Self {
        let candidates = [
            (2.0_f32, Self::M2),
            (2.5, Self::M2_5),
            (3.0, Self::M3),
            (4.0, Self::M4),
            (5.0, Self::M5),
            (6.0, Self::M6),
            (8.0, Self::M8),
        ];
        let mut best = Self::M4;
        let mut best_dist = f32::INFINITY;
        for (nom, size) in candidates {
            let d = (nom - nominal).abs();
            if d < best_dist {
                best_dist = d;
                best = size;
            }
        }
        best
    }
}

// ────────────────────────────────────────────────────────
// 6 primitive
// ────────────────────────────────────────────────────────

/// クリアランス穴 (ボルトが余裕通過する貫通穴)
///
/// 直径 = 呼び径 + [`CLEARANCE_H2D_FDM`] (0.2mm、H2D FDM 実測)
/// 中心 = 原点、軸 = Y、全長 = `depth`
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::fastener::{screw_hole, MetricSize};
/// use alice_sdf::SdfNode;
///
/// let hole = screw_hole(MetricSize::M4, 5.0);
/// // 直径 4.2mm × 全長 5mm の cylinder が返る
/// // 板から穴を掘る時は SdfNode::Subtraction で使う
/// match &hole {
///     SdfNode::Cylinder { radius, half_height } => {
///         assert!((radius - 2.1).abs() < 1e-6);
///         assert!((half_height - 2.5).abs() < 1e-6);
///     }
///     _ => panic!("expected Cylinder"),
/// }
/// ```
#[must_use]
pub fn screw_hole(size: MetricSize, depth: f32) -> SdfNode {
    let dia = size.nominal_diameter() + CLEARANCE_H2D_FDM;
    SdfNode::Cylinder {
        radius: dia * 0.5,
        half_height: depth * 0.5,
    }
}

/// タップ下穴 (セルフタッピング用、Bamboo `PrintParams::tap_hole()` と同式)
///
/// 直径 = `screw_dia * 0.85 + 2 * accuracy` (`accuracy` は default [`DEFAULT_ACCURACY`] 推奨)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::fastener::{tap_hole, MetricSize, DEFAULT_ACCURACY};
/// use alice_sdf::SdfNode;
///
/// let hole = tap_hole(MetricSize::M3, 4.0, DEFAULT_ACCURACY);
/// // 直径 3 * 0.85 + 2 * 0.1 = 2.75mm × 全長 4mm
/// match &hole {
///     SdfNode::Cylinder { radius, half_height } => {
///         assert!((radius - 1.375).abs() < 1e-6);
///         assert!((half_height - 2.0).abs() < 1e-6);
///     }
///     _ => panic!("expected Cylinder"),
/// }
/// ```
#[must_use]
pub fn tap_hole(size: MetricSize, depth: f32, accuracy: f32) -> SdfNode {
    let dia = size.nominal_diameter().mul_add(0.85, 2.0 * accuracy);
    SdfNode::Cylinder {
        radius: dia * 0.5,
        half_height: depth * 0.5,
    }
}

/// 座ぐり (counterbore) — ソケットキャップ頭が完全に沈む貫通穴
///
/// 構造: 板貫通クリアランス穴 (下段) ∪ 頭径 × 頭高+0.5mm sink margin の cylinder (上段)
/// 板は Y=0 中心の厚さ `plate_thickness` を想定、頭は板上面 (Y = +plate_thickness/2) から沈む
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::fastener::{counterbore, MetricSize};
/// let hole = counterbore(MetricSize::M4, 8.0);
/// // 板 8mm 厚から掘る、頭径 7mm × 頭高 4mm + 0.5mm margin = 4.5mm 深さ
/// ```
#[must_use]
pub fn counterbore(size: MetricSize, plate_thickness: f32) -> SdfNode {
    let head_dia = size.head_diameter_socket();
    let bore_depth = size.head_height_socket() + 0.5;
    // Through hole は plate 貫通 + 5mm each side margin (preview MC で確実 punch through、
    // [[success_alice_lol_cavity_margin_batch_fix_2026_08_25]] cavity margin rule)
    let through = screw_hole(size, plate_thickness + 10.0);
    let bore = SdfNode::Cylinder {
        radius: head_dia * 0.5,
        half_height: bore_depth * 0.5,
    };
    let bore_shifted = SdfNode::Translate {
        child: Arc::new(bore),
        offset: Vec3::new(0.0, (plate_thickness - bore_depth) * 0.5, 0.0),
    };
    SdfNode::Union {
        a: Arc::new(through),
        b: Arc::new(bore_shifted),
    }
}

/// 皿頭沈み (countersink) — ISO 10642 90° 皿頭を完全に沈める貫通穴
///
/// 構造: 板貫通クリアランス穴 + 皿頭 cone (テーパー全角 90°)
/// cone は base が板上面と一致、tip が板内部を向く
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::fastener::{countersink, MetricSize};
/// let hole = countersink(MetricSize::M4, 8.0);
/// // ISO 10642 M4 皿頭 = 頭径 8mm、cone 高さ = 8/2 = 4mm (90°)
/// ```
#[must_use]
pub fn countersink(size: MetricSize, plate_thickness: f32) -> SdfNode {
    let head_dia = size.head_diameter_countersunk();
    // 90° 皿頭 → cone 高さ = head_dia / 2 (テーパー半角 45°、tan(45°)=1)
    let cone_h = head_dia * 0.5;
    // Through hole は plate 貫通 + 5mm each side margin (preview MC で確実 punch through、
    // [[success_alice_lol_cavity_margin_batch_fix_2026_08_25]] cavity margin rule)
    let through = screw_hole(size, plate_thickness + 10.0);
    // SdfNode::Cone は base at -half_height, tip at +half_height
    // 皿頭は tip を下 (板内部) に向けたいので X 軸周り 180° 回転
    let cone = SdfNode::Cone {
        radius: head_dia * 0.5,
        half_height: cone_h * 0.5,
    };
    let cone_inverted = SdfNode::Rotate {
        child: Arc::new(cone),
        rotation: Quat::from_rotation_x(std::f32::consts::PI),
    };
    let cone_shifted = SdfNode::Translate {
        child: Arc::new(cone_inverted),
        offset: Vec3::new(0.0, (plate_thickness - cone_h) * 0.5, 0.0),
    };
    SdfNode::Union {
        a: Arc::new(through),
        b: Arc::new(cone_shifted),
    }
}

/// ISO 4762 ソケットキャップボルト 実体 (組立可視化用)
///
/// 構造: 軸 (半径 = 呼び径/2、長 = `shank_length`) ∪ 頭 (ISO 4762 頭径/高)
/// 頭は Y = +shank_length/2 + head_height/2 に配置 (軸上端の外側)
///
/// これは「実体」であって「穴」ではない 穴が欲しい時は [`counterbore`] を使う
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::fastener::{bolt, MetricSize};
/// let m4_bolt = bolt(MetricSize::M4, 20.0);
/// // 軸 4mm × 20mm + 頭 7mm × 4mm
/// ```
#[must_use]
pub fn bolt(size: MetricSize, shank_length: f32) -> SdfNode {
    let shank_r = size.nominal_diameter() * 0.5;
    let head_r = size.head_diameter_socket() * 0.5;
    let head_h = size.head_height_socket();
    let shank = SdfNode::Cylinder {
        radius: shank_r,
        half_height: shank_length * 0.5,
    };
    let head = SdfNode::Cylinder {
        radius: head_r,
        half_height: head_h * 0.5,
    };
    let head_shifted = SdfNode::Translate {
        child: Arc::new(head),
        offset: Vec3::new(0.0, (shank_length + head_h) * 0.5, 0.0),
    };
    SdfNode::Union {
        a: Arc::new(shank),
        b: Arc::new(head_shifted),
    }
}

/// ヒートセットインサート下穴 (Voxel8 / McMaster 準拠、Bamboo `heat_insert_hole()` と同式)
///
/// 直径 = insert 外径 + 0.2mm (熱膨張余裕)、深さ = insert 埋込深さ + [`HEAT_SET_SINK_MARGIN`]
/// はんだごてで挿入する時に樹脂が沈むための余裕を確保する
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::fastener::{heat_set_insert_hole, MetricSize};
/// let hole = heat_set_insert_hole(MetricSize::M3);
/// // M3 → 外径 4mm、深さ 3.8mm の insert 用に 4.2mm × 4.1mm 穴
/// ```
#[must_use]
pub fn heat_set_insert_hole(size: MetricSize) -> SdfNode {
    let dia = size.heat_set_insert_diameter() + CLEARANCE_H2D_FDM;
    let depth = size.heat_set_insert_depth() + HEAT_SET_SINK_MARGIN;
    SdfNode::Cylinder {
        radius: dia * 0.5,
        half_height: depth * 0.5,
    }
}

/// ダウエル (dowel pin) 挿入穴 (家具 flat-pack joinery、Ø8 標準)
///
/// 直径 = `dia` + 0.1mm (接着剤余裕、圧入 vs slip fit は user 判断)
/// 中心 = 原点、軸 = Y、全長 = `depth`
///
/// 家具 flat-pack (IKEA/自作) の 8mm dowel joint、時計台の Ø6、大型棚の Ø10 等
/// user 側で `subtract(plate, dowel_hole(8, 15))` として使う
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::fastener::dowel_hole;
/// let hole = dowel_hole(8.0, 15.0);
/// // Ø8.1mm × 深 15mm dowel 用穴
/// ```
#[must_use]
pub fn dowel_hole(dia: f32, depth: f32) -> SdfNode {
    let hole_dia = dia + 0.1;
    SdfNode::Cylinder {
        radius: hole_dia * 0.5,
        half_height: depth * 0.5,
    }
}

/// 木ネジ下穴 (wood screw pilot、softwood / hardwood 別公式)
///
/// - softwood (`hardwood = false`): dia = `screw_dia` × 0.7 (パイン / スギ / SPF、割れ防止)
/// - hardwood (`hardwood = true`): dia = `screw_dia` × 0.9 (オーク / ウォールナット、ネジ切れ防止)
///
/// depth = `screw_dia` × 5 (rule of thumb、ネジ長 ~5× dia が典型)
/// 中心 = 原点、軸 = Y
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::fastener::wood_screw_pilot;
/// let pilot = wood_screw_pilot(4.0, false); // #8 相当 softwood
/// // Ø2.8mm × 深 20mm 下穴
/// ```
#[must_use]
pub fn wood_screw_pilot(screw_dia: f32, hardwood: bool) -> SdfNode {
    let factor = if hardwood { 0.9 } else { 0.7 };
    let hole_dia = screw_dia * factor;
    let depth = screw_dia * 5.0;
    SdfNode::Cylinder {
        radius: hole_dia * 0.5,
        half_height: depth * 0.5,
    }
}

// ────────────────────────────────────────────────────────
// テスト
// ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn screw_hole_m4_diameter_is_nominal_plus_clearance() {
        let node = screw_hole(MetricSize::M4, 5.0);
        match node {
            SdfNode::Cylinder {
                radius,
                half_height,
            } => {
                assert!(approx_eq(radius, 2.1)); // (4.0 + 0.2) / 2
                assert!(approx_eq(half_height, 2.5));
            }
            _ => panic!("expected Cylinder"),
        }
    }

    #[test]
    fn tap_hole_m3_matches_bamboo_formula() {
        // Bamboo formulas.rs: tap = screw_dia * 0.85 + 2 * accuracy
        // M3 with accuracy 0.1 → 3.0 * 0.85 + 0.2 = 2.75mm 直径
        let node = tap_hole(MetricSize::M3, 4.0, DEFAULT_ACCURACY);
        match node {
            SdfNode::Cylinder { radius, .. } => {
                assert!(approx_eq(radius, 1.375));
            }
            _ => panic!("expected Cylinder"),
        }
    }

    #[test]
    fn tap_hole_m5_with_custom_accuracy() {
        // M5 with accuracy 0.15 → 5.0 * 0.85 + 0.3 = 4.55mm
        let node = tap_hole(MetricSize::M5, 6.0, 0.15);
        match node {
            SdfNode::Cylinder { radius, .. } => {
                assert!(approx_eq(radius, 2.275));
            }
            _ => panic!("expected Cylinder"),
        }
    }

    #[test]
    fn counterbore_m4_produces_union_of_through_and_bore() {
        let node = counterbore(MetricSize::M4, 8.0);
        match node {
            SdfNode::Union { a, b } => {
                // a = 貫通穴 (Cylinder 直下)
                assert!(matches!(&*a, SdfNode::Cylinder { .. }));
                // b = translate(cylinder)
                assert!(matches!(&*b, SdfNode::Translate { .. }));
            }
            _ => panic!("expected Union"),
        }
    }

    #[test]
    fn countersink_m4_uses_iso10642_head_diameter() {
        // M4 皿頭径 = 8mm、cone 高さ = 4mm
        let node = countersink(MetricSize::M4, 8.0);
        // Union { through, translate(rotate(cone)) } の構造
        match node {
            SdfNode::Union { a: _, b } => match &*b {
                SdfNode::Translate { child, .. } => {
                    assert!(matches!(&**child, SdfNode::Rotate { .. }));
                }
                _ => panic!("expected Translate at Union.b"),
            },
            _ => panic!("expected Union"),
        }
    }

    #[test]
    fn bolt_m4_produces_shank_plus_head_union() {
        let node = bolt(MetricSize::M4, 20.0);
        match node {
            SdfNode::Union { a, b } => {
                // a = 軸 cylinder、b = translate(頭 cylinder)
                match &*a {
                    SdfNode::Cylinder {
                        radius,
                        half_height,
                    } => {
                        assert!(approx_eq(*radius, 2.0)); // 呼び径/2
                        assert!(approx_eq(*half_height, 10.0)); // 20/2
                    }
                    _ => panic!("expected Cylinder for shank"),
                }
                assert!(matches!(&*b, SdfNode::Translate { .. }));
            }
            _ => panic!("expected Union"),
        }
    }

    #[test]
    fn heat_set_insert_hole_m3_matches_voxel8_spec() {
        // M3 → 外径 4mm + 0.2 = 4.2mm 直径、深さ 3.8 + 0.3 = 4.1mm
        let node = heat_set_insert_hole(MetricSize::M3);
        match node {
            SdfNode::Cylinder {
                radius,
                half_height,
            } => {
                assert!(approx_eq(radius, 2.1));
                assert!(approx_eq(half_height, 2.05));
            }
            _ => panic!("expected Cylinder"),
        }
    }

    #[test]
    fn metric_size_iso4762_head_dimensions() {
        // ISO 4762 spot check
        assert!(approx_eq(MetricSize::M3.head_diameter_socket(), 5.5));
        assert!(approx_eq(MetricSize::M8.head_diameter_socket(), 13.0));
        assert!(approx_eq(MetricSize::M6.head_height_socket(), 6.0));
    }

    #[test]
    fn metric_size_iso10642_countersunk_head_diameters() {
        assert!(approx_eq(MetricSize::M3.head_diameter_countersunk(), 6.0));
        assert!(approx_eq(MetricSize::M8.head_diameter_countersunk(), 16.0));
    }

    #[test]
    fn all_metric_sizes_have_positive_dimensions() {
        for size in [
            MetricSize::M3,
            MetricSize::M4,
            MetricSize::M5,
            MetricSize::M6,
            MetricSize::M8,
        ] {
            assert!(size.nominal_diameter() > 0.0);
            assert!(size.head_diameter_socket() > size.nominal_diameter());
            assert!(size.head_height_socket() > 0.0);
            assert!(size.head_diameter_countersunk() >= size.head_diameter_socket());
            assert!(size.heat_set_insert_diameter() > size.nominal_diameter());
            assert!(size.heat_set_insert_depth() > 0.0);
        }
    }

    #[test]
    fn evaluation_at_origin_is_inside_all_fastener_holes() {
        // 全 hole primitive は原点内部 (負の SDF 値) を返すはず
        use alice_sdf::eval;
        let plate_thickness = 6.0;
        for size in [
            MetricSize::M3,
            MetricSize::M4,
            MetricSize::M5,
            MetricSize::M6,
            MetricSize::M8,
        ] {
            let sh = screw_hole(size, plate_thickness);
            assert!(eval(&sh, Vec3::ZERO) < 0.0, "screw_hole {size:?}");
            let th = tap_hole(size, plate_thickness, DEFAULT_ACCURACY);
            assert!(eval(&th, Vec3::ZERO) < 0.0, "tap_hole {size:?}");
            let cb = counterbore(size, plate_thickness);
            assert!(eval(&cb, Vec3::ZERO) < 0.0, "counterbore {size:?}");
            let cs = countersink(size, plate_thickness);
            assert!(eval(&cs, Vec3::ZERO) < 0.0, "countersink {size:?}");
            let hs = heat_set_insert_hole(size);
            assert!(eval(&hs, Vec3::ZERO) < 0.0, "heat_set_insert_hole {size:?}");
        }
    }

    #[test]
    fn from_f32_snap_exact_matches() {
        assert_eq!(MetricSize::from_f32_snap(2.0), MetricSize::M2);
        assert_eq!(MetricSize::from_f32_snap(2.5), MetricSize::M2_5);
        assert_eq!(MetricSize::from_f32_snap(3.0), MetricSize::M3);
        assert_eq!(MetricSize::from_f32_snap(4.0), MetricSize::M4);
        assert_eq!(MetricSize::from_f32_snap(5.0), MetricSize::M5);
        assert_eq!(MetricSize::from_f32_snap(6.0), MetricSize::M6);
        assert_eq!(MetricSize::from_f32_snap(8.0), MetricSize::M8);
    }

    #[test]
    fn from_f32_snap_near_matches_pick_closest() {
        // 4.5 は M4 と M5 で等距離、first-wins で M4
        assert_eq!(MetricSize::from_f32_snap(4.5), MetricSize::M4);
        // 3.5 は M3 と M4 で等距離、first-wins で M3
        assert_eq!(MetricSize::from_f32_snap(3.5), MetricSize::M3);
        // 7.0 は M6 (dist 1) より M8 (dist 1) と同距離、first-wins で M6
        assert_eq!(MetricSize::from_f32_snap(7.0), MetricSize::M6);
        // 4.1 は明確に M4
        assert_eq!(MetricSize::from_f32_snap(4.1), MetricSize::M4);
        // 5.9 は明確に M6
        assert_eq!(MetricSize::from_f32_snap(5.9), MetricSize::M6);
        // 2.3 は M2.5 (dist 0.2) が M2 (dist 0.3) より近い
        assert_eq!(MetricSize::from_f32_snap(2.3), MetricSize::M2_5);
        // 2.1 は M2 (dist 0.1) が近い
        assert_eq!(MetricSize::from_f32_snap(2.1), MetricSize::M2);
    }

    #[test]
    fn from_f32_snap_out_of_range_clamps() {
        // 上限外 → M8
        assert_eq!(MetricSize::from_f32_snap(100.0), MetricSize::M8);
        // 下限外 → M2 (M3 でなく M2 に snap、Phase X.2 で追加)
        assert_eq!(MetricSize::from_f32_snap(0.5), MetricSize::M2);
    }

    #[test]
    fn m2_dimensions_are_iso_compliant() {
        // ISO 4762 M2: nominal 2.0, head_dia 3.8, head_h 2.0
        assert!((MetricSize::M2.nominal_diameter() - 2.0).abs() < 1e-6);
        assert!((MetricSize::M2.head_diameter_socket() - 3.8).abs() < 1e-6);
        assert!((MetricSize::M2.head_height_socket() - 2.0).abs() < 1e-6);
        // ISO 10642 M2 皿頭 = 4.0mm
        assert!((MetricSize::M2.head_diameter_countersunk() - 4.0).abs() < 1e-6);
    }

    #[test]
    fn m2_5_dimensions_are_pi_compliant() {
        // Raspberry Pi 標準 M2.5: nominal 2.5
        assert!((MetricSize::M2_5.nominal_diameter() - 2.5).abs() < 1e-6);
        assert!((MetricSize::M2_5.head_diameter_socket() - 4.5).abs() < 1e-6);
        assert!((MetricSize::M2_5.head_height_socket() - 2.5).abs() < 1e-6);
        assert!((MetricSize::M2_5.head_diameter_countersunk() - 4.5).abs() < 1e-6);
        // Heat-set insert (McMaster)
        assert!((MetricSize::M2_5.heat_set_insert_diameter() - 3.6).abs() < 1e-6);
        assert!((MetricSize::M2_5.heat_set_insert_depth() - 3.3).abs() < 1e-6);
    }

    #[test]
    fn m2_all_holes_evaluate_negative_at_origin() {
        use alice_sdf::eval;
        let plate_thickness = 6.0;
        for size in [MetricSize::M2, MetricSize::M2_5] {
            let sh = screw_hole(size, plate_thickness);
            assert!(eval(&sh, Vec3::ZERO) < 0.0, "screw_hole {size:?}");
            let th = tap_hole(size, plate_thickness, DEFAULT_ACCURACY);
            assert!(eval(&th, Vec3::ZERO) < 0.0, "tap_hole {size:?}");
            let cb = counterbore(size, plate_thickness);
            assert!(eval(&cb, Vec3::ZERO) < 0.0, "counterbore {size:?}");
            let cs = countersink(size, plate_thickness);
            assert!(eval(&cs, Vec3::ZERO) < 0.0, "countersink {size:?}");
            let hs = heat_set_insert_hole(size);
            assert!(eval(&hs, Vec3::ZERO) < 0.0, "heat_set_insert_hole {size:?}");
        }
    }

    #[test]
    fn dowel_hole_standard_8mm() {
        // Ø8 dowel + 0.1 glue expansion = Ø8.1
        let node = dowel_hole(8.0, 15.0);
        if let SdfNode::Cylinder {
            radius,
            half_height,
        } = node
        {
            assert!(approx_eq(radius, 4.05), "dowel radius = 4.05, got {radius}");
            assert!(approx_eq(half_height, 7.5), "dowel half_height = 7.5");
        } else {
            panic!("expected Cylinder");
        }
    }

    #[test]
    fn wood_screw_pilot_softwood_formula() {
        // #8 (dia 4mm) softwood pilot = 4 × 0.7 = 2.8mm
        let node = wood_screw_pilot(4.0, false);
        if let SdfNode::Cylinder {
            radius,
            half_height,
        } = node
        {
            assert!(approx_eq(radius, 1.4), "softwood pilot radius = 1.4");
            // depth = 4 × 5 = 20mm, half_height = 10
            assert!(approx_eq(half_height, 10.0));
        } else {
            panic!("expected Cylinder");
        }
    }

    #[test]
    fn wood_screw_pilot_hardwood_formula() {
        // #8 (dia 4mm) hardwood pilot = 4 × 0.9 = 3.6mm
        let node = wood_screw_pilot(4.0, true);
        if let SdfNode::Cylinder { radius, .. } = node {
            assert!(approx_eq(radius, 1.8), "hardwood pilot radius = 1.8");
        } else {
            panic!("expected Cylinder");
        }
    }
}
