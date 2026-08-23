//! # pattern_sdf — Bamboo Rust generator を LOL に移設した完成 pattern (Phase B.1.b)
//!
//! Bamboo `src/generators/{hook,gridfinity,drawer,shelf_divider}.rs` の LOL DSL
//! 文字列生成ロジックを LOL 側で **`SdfNode` 直接構築 API** に翻訳した完成 pattern
//! 4 種を提供する `parse_lol()` 経由を skip し性能向上、型安全性確保
//!
//! ## Pattern
//!
//! | pattern | Bamboo canonical | 用途 |
//! |---------|-----------------|-----|
//! | [`wall_hook`] | `src/generators/hook.rs` | 壁掛けフック (荷重逆算済寸法) |
//! | [`gridfinity_bin`] | `src/generators/gridfinity.rs` | Gridfinity 42mm grid bin |
//! | [`drawer_organizer`] | `src/generators/drawer.rs` | 引出し仕切り (chopsticks/fork/knife/spoon 等) |
//! | [`shelf_divider`] | `src/generators/shelf_divider.rs` | U 字棚仕切り (hex cutout 底板 + 2 側板) |
//!
//! ## 設計方針
//!
//! - **material 非依存**: pattern 関数は dimensional parameter のみ受け取る
//!   material 依存の応力逆算 (`hook.rs` の `required_area` 計算等) は user 側で事前に完了、
//!   LOL には finalized geometry を渡す
//! - **[`crate::stdlib::pattern`] registry の実装対**: `registry::WALL_HOOK` / `GRIDFINITY_BIN`
//!   / `DRAWER_ORGANIZER` / `SHELF_DIVIDER_560X250X120` の metadata と対を成す関数
//! - **Bamboo との互換**: Bamboo Python `models/*/generate.py` は本 module の対象外 (薄物専用)
//!   本 module は **`SdfMarchingCubes` 経路のみ** (`SdfNode::sdf_to_mesh` 経由)

use alice_sdf::SdfNode;
use glam::{Quat, Vec3};
use std::sync::Arc;

// ────────────────────────────────────────────────────────
// 共通 helpers (Arc wrap 簡略化)
// ────────────────────────────────────────────────────────

fn rounded_box(hx: f32, hy: f32, hz: f32, r: f32) -> SdfNode {
    SdfNode::RoundedBox {
        half_extents: Vec3::new(hx, hy, hz),
        round_radius: r,
    }
}

fn box3d(hx: f32, hy: f32, hz: f32) -> SdfNode {
    SdfNode::Box3d {
        half_extents: Vec3::new(hx, hy, hz),
    }
}

fn cylinder(radius: f32, half_height: f32) -> SdfNode {
    SdfNode::Cylinder {
        radius,
        half_height,
    }
}

/// Y-axis cylinder を Z-axis 世界向けに 90° 回転
///
/// `SdfNode::Cylinder` は Y-axis alignment (半径 XZ 平面、高さ Y 軸) だが
/// text-to-print viewer は Z-up なので、cup / cable hole 等の縦向き用途では
/// 本 helper で Z-axis alignment に変換する
fn cylinder_z(radius: f32, half_height: f32) -> SdfNode {
    SdfNode::Rotate {
        child: Arc::new(cylinder(radius, half_height)),
        rotation: Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
    }
}

/// Y-up 設計の archetype 全体を Z-up 世界向けに 90° 回転 (Y→ +Z、正立)
///
/// text-to-print viewer は Z-up 固定なので、Y-up (Y=vertical) で設計した
/// pattern を viewer に正しい向きで表示するため本 helper で wrap する
/// 変換: 内部 (0, 1, 0) = 世界 (0, 0, 1)、intended bottom (Y-) は世界 Z=0 bed 側
/// storage_box 等の「底が bed で top open」pattern で使う
fn to_z_up(y_up_node: SdfNode) -> SdfNode {
    SdfNode::Rotate {
        child: Arc::new(y_up_node),
        rotation: Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
    }
}

/// Y-up 設計を Z-up 世界向けに 90° 回転 (Y→ -Z、upside-down)
///
/// `to_z_up` の逆方向、intended top (Y+) を bed 側 (Z=0) に配置
/// tissue_box_cover の「Print upside-down: slot on bed, walls up, bottom-open ceiling」
/// (household.md § 1 spec 準拠)、desk_shelf の「shelf on bed, legs up」等
/// 「印刷時に元の top を bed に置きたい」場合に使う
fn to_z_up_flipped(y_up_node: SdfNode) -> SdfNode {
    SdfNode::Rotate {
        child: Arc::new(y_up_node),
        rotation: Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
    }
}

fn translate(child: SdfNode, offset: Vec3) -> SdfNode {
    SdfNode::Translate {
        child: Arc::new(child),
        offset,
    }
}

fn union(a: SdfNode, b: SdfNode) -> SdfNode {
    SdfNode::Union {
        a: Arc::new(a),
        b: Arc::new(b),
    }
}

fn smooth_union(a: SdfNode, b: SdfNode, k: f32) -> SdfNode {
    SdfNode::SmoothUnion {
        a: Arc::new(a),
        b: Arc::new(b),
        k,
    }
}

fn subtract(a: SdfNode, b: SdfNode) -> SdfNode {
    SdfNode::Subtraction {
        a: Arc::new(a),
        b: Arc::new(b),
    }
}

// ────────────────────────────────────────────────────────
// 1. wall_hook (Bamboo generators/hook.rs 翻訳)
// ────────────────────────────────────────────────────────

/// 壁掛けフックの寸法仕様 (荷重逆算済、material 非依存)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallHookSpec {
    /// フック幅 (mm、荷重 × 安全率 3 / (tensile × adhesion) 逆算値)
    pub hook_width: f32,
    /// フック厚 (mm、同上)
    pub hook_thickness: f32,
    /// フック protrusion (mm、壁からの突出、default 35)
    pub hook_depth: f32,
    /// フック opening (mm、コート掛け径、default 20)
    pub hook_opening: f32,
    /// フック throat (mm、内側深さ、default 30)
    pub hook_throat: f32,
    /// backplate 追加幅 (mm、hook_width + 本値 = backplate 幅、default 10)
    pub backplate_extra_w: f32,
    /// backplate 追加高 (mm、throat + depth + 本値 = backplate 高、default 10)
    pub backplate_extra_h: f32,
    /// backplate 厚 (mm、default max(thickness, 4))
    pub backplate_thickness: f32,
    /// fillet R (mm、material `structural_fillet()` 相当)
    pub fillet_radius: f32,
    /// マウント穴径 (mm、`Some(4.5)` で M4 clearance、`None` でマウント穴なし)
    pub mount_hole_dia: Option<f32>,
}

impl WallHookSpec {
    /// PLA 標準寸法 (荷重 1kgf 相当、Bamboo hook.rs default 相当)
    #[must_use]
    pub const fn pla_1kgf() -> Self {
        Self {
            hook_width: 10.0,
            hook_thickness: 6.0,
            hook_depth: 35.0,
            hook_opening: 20.0,
            hook_throat: 30.0,
            backplate_extra_w: 10.0,
            backplate_extra_h: 10.0,
            backplate_thickness: 6.0,
            fillet_radius: 3.0,
            mount_hole_dia: Some(4.5),
        }
    }
}

/// 壁掛けフック (Bamboo `hook.rs` LOL DSL 生成と等価な `SdfNode` を返す)
///
/// 構造 (Bamboo `hook.rs:49-60` `format!` を SdfNode 直接構築に翻訳):
/// - backplate `RoundedBox` (中心 X=0, Y=0, Z=0)
/// - hook arm `RoundedBox` (Y=bp_hy - arm_hy、Z=+bp_hz + arm_hz)
/// - hook tip `RoundedBox` (Y=bp_hy - opening/2、Z=+bp_hz + hook_depth)
/// - 3-way `SmoothUnion` で blend
/// - mount holes は screw 指定時のみ subtract
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{wall_hook, WallHookSpec};
/// let hook = wall_hook(&WallHookSpec::pla_1kgf());
/// // node_to_3mf(&hook, "hook.3mf", &PrintConfig::default()) で出力
/// ```
#[must_use]
pub fn wall_hook(spec: &WallHookSpec) -> SdfNode {
    let bp_hw = spec.hook_width + spec.backplate_extra_w;
    let bp_hh = spec.hook_throat + spec.hook_depth + spec.backplate_extra_h;
    let bp_hx = bp_hw * 0.5;
    let bp_hy = bp_hh * 0.5;
    let bp_hz = spec.backplate_thickness * 0.5;
    let arm_hx = spec.hook_width * 0.5;
    let arm_hy = spec.hook_thickness * 0.5;
    let arm_hz = spec.hook_depth * 0.5;
    let tip_hy = spec.hook_opening * 0.5;

    let backplate = rounded_box(bp_hx, bp_hy, bp_hz, spec.fillet_radius);
    let arm_placed = translate(
        rounded_box(arm_hx, arm_hy, arm_hz, spec.fillet_radius),
        Vec3::new(0.0, bp_hy - arm_hy, bp_hz + arm_hz),
    );
    let tip_placed = translate(
        rounded_box(arm_hx, tip_hy, arm_hy, spec.fillet_radius),
        Vec3::new(
            0.0,
            bp_hy - spec.hook_opening * 0.5,
            bp_hz + spec.hook_depth,
        ),
    );

    // 3-way SmoothUnion (Bamboo `smooth_union(2.0, backplate, arm, tip)` と等価)
    let bp_arm = smooth_union(backplate, arm_placed, 2.0);
    let hook_body = smooth_union(bp_arm, tip_placed, 2.0);

    // Mount holes (screw 指定時のみ subtract)
    if let Some(hole_dia) = spec.mount_hole_dia {
        let hole_r = hole_dia * 0.5;
        let hole_half_h = bp_hz + 1.0;
        let hole_spacing = bp_hh * 0.3;
        let hole = cylinder(hole_r, hole_half_h);
        let hole_top = translate(hole.clone(), Vec3::new(0.0, hole_spacing, 0.0));
        let hole_bottom = translate(hole, Vec3::new(0.0, -hole_spacing, 0.0));
        let holes = union(hole_top, hole_bottom);
        subtract(hook_body, holes)
    } else {
        hook_body
    }
}

// ────────────────────────────────────────────────────────
// 2. gridfinity_bin (Bamboo generators/gridfinity.rs 翻訳)
// ────────────────────────────────────────────────────────

/// Gridfinity spec 定数 (Bamboo `formulas::gridfinity` と同期)
pub mod gridfinity_spec {
    /// grid unit 幅 (mm、Gridfinity 標準)
    pub const GRID_UNIT: f32 = 42.0;
    /// bin clearance (mm、外形との隙間)
    pub const BIN_CLEARANCE: f32 = 0.25;
    /// height unit (mm、1 U)
    pub const HEIGHT_UNIT: f32 = 7.0;
    /// lip height (mm、Gridfinity 標準の 4.75)
    pub const LIP_HEIGHT: f32 = 4.75;
    /// corner fillet (mm、外角丸)
    pub const CORNER_FILLET: f32 = 4.0;
    /// bin 内側 corner fillet (mm)
    pub const INNER_FILLET: f32 = 1.0;
}

/// Gridfinity bin の寸法仕様
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridfinitySpec {
    /// units X 方向 (1U = 42mm)
    pub units_x: u32,
    /// units Y 方向
    pub units_y: u32,
    /// 高さ U 数 (1U = 7mm)
    pub height_u: u32,
    /// dividers `Some((cols, rows))` で仕切り、`None` で単一 cavity
    pub dividers: Option<(u32, u32)>,
    /// 壁厚 (mm、default 1.2 = thin wall variant)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 1.5)
    pub floor_thickness: f32,
}

impl GridfinitySpec {
    /// 2×2 単 cavity default
    #[must_use]
    pub const fn default_2x2() -> Self {
        Self {
            units_x: 2,
            units_y: 2,
            height_u: 4,
            dividers: None,
            wall_thickness: 1.2,
            floor_thickness: 1.5,
        }
    }
}

/// Gridfinity bin (Bamboo `gridfinity.rs` LOL DSL 生成と等価な `SdfNode` を返す)
///
/// 構造:
/// - 外形 `RoundedBox` (`units × GRID_UNIT - 2 × BIN_CLEARANCE` 幅)
/// - dividers あり → `RepeatFinite` cavity grid を `subtract`
/// - dividers なし → 単一 cavity `RoundedBox` を `subtract`
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{gridfinity_bin, GridfinitySpec};
/// let bin = gridfinity_bin(&GridfinitySpec::default_2x2());
/// ```
#[must_use]
pub fn gridfinity_bin(spec: &GridfinitySpec) -> SdfNode {
    #[allow(clippy::cast_precision_loss)]
    let ext_x = spec.units_x as f32 * gridfinity_spec::GRID_UNIT;
    #[allow(clippy::cast_precision_loss)]
    let ext_y = spec.units_y as f32 * gridfinity_spec::GRID_UNIT;
    let bin_hx = (ext_x - 2.0 * gridfinity_spec::BIN_CLEARANCE) * 0.5;
    let bin_hy = (ext_y - 2.0 * gridfinity_spec::BIN_CLEARANCE) * 0.5;
    #[allow(clippy::cast_precision_loss)]
    let ext_h = spec.height_u as f32 * gridfinity_spec::HEIGHT_UNIT + gridfinity_spec::LIP_HEIGHT;
    let bin_hz = ext_h * 0.5;
    // 2026-08-20 fix: cavity 天面が outer top 未満で塞がる bug を修正
    // 旧: cavity_hz = int_depth/2、cavity_offset_z = floor/2 で cavity 天面が
    //     outer top より 6.25mm 内側 = 「ただの四角」に見える
    // 新: cavity を outer top を貫通するサイズにして top open を保証
    let cavity_hz = (ext_h - spec.floor_thickness + 1.0) * 0.5;
    let inner_hx = bin_hx - spec.wall_thickness;
    let inner_hy = bin_hy - spec.wall_thickness;
    let cavity_offset_z = (spec.floor_thickness + 1.0) * 0.5;

    let outer = rounded_box(bin_hx, bin_hy, bin_hz, gridfinity_spec::CORNER_FILLET);

    if let Some((dx, dy)) = spec.dividers {
        if dx > 1 && dy > 1 {
            #[allow(clippy::cast_precision_loss)]
            let cell_w = inner_hx * 2.0 / dx as f32;
            #[allow(clippy::cast_precision_loss)]
            let cell_d = inner_hy * 2.0 / dy as f32;
            let cell_hx = (cell_w - spec.wall_thickness) * 0.5;
            let cell_hy = (cell_d - spec.wall_thickness) * 0.5;
            let cx_start = -inner_hx + cell_w * 0.5;
            let cy_start = -inner_hy + cell_d * 0.5;

            // Phase 5.8 bug fix: 従来 RepeatFinite count = (dx-1)/2 の integer 除算で
            // dx=2 (最典型 2×2 divider) の時 count=0 → cavity 1 個 のみ生成 → subtract で
            // 空 mesh 生成 (1.3KB degenerate mesh、Phase 5.5 CLI 実行で発覚)
            // 修正: dx * dy 個の cavity を Union で明示配置、RepeatFinite 依存廃止
            let cavity = rounded_box(cell_hx, cell_hy, cavity_hz, gridfinity_spec::INNER_FILLET);
            // 2026-08-07 fix: 線形左入れ子 fold → balanced fold で eval recursion 削減
            // (skadis_panel の 98-deep 事案と同 pattern の予防、depth O(n) → O(log n))
            let mut cavity_list: Vec<SdfNode> = Vec::with_capacity(dx as usize * dy as usize);
            for i in 0..dx {
                for j in 0..dy {
                    #[allow(clippy::cast_precision_loss)]
                    let cx = cx_start + i as f32 * cell_w;
                    #[allow(clippy::cast_precision_loss)]
                    let cy = cy_start + j as f32 * cell_d;
                    cavity_list.push(translate(
                        cavity.clone(),
                        Vec3::new(cx, cy, cavity_offset_z),
                    ));
                }
            }
            let cavities = super::balanced_union_fold(cavity_list);
            if let Some(all_cavities) = cavities {
                return subtract(outer, all_cavities);
            }
        }
    }
    // dividers なし (単一 cavity)
    let single_cavity = translate(
        rounded_box(inner_hx, inner_hy, cavity_hz, gridfinity_spec::INNER_FILLET),
        Vec3::new(0.0, 0.0, cavity_offset_z),
    );
    subtract(outer, single_cavity)
}

// ────────────────────────────────────────────────────────
// 3. drawer_organizer (Bamboo generators/drawer.rs 翻訳)
// ────────────────────────────────────────────────────────

/// slot type (Bamboo `drawer.rs::SlotDef` と同期)
#[derive(Debug, Clone, Copy)]
pub struct DrawerSlotSpec {
    /// slot 幅 (mm)
    pub width: f32,
    /// slot 最小深さ (mm、drawer depth と min で clamp)
    pub min_depth: f32,
    /// この slot の個数
    pub count: u32,
}

/// drawer organizer の寸法仕様
#[derive(Debug, Clone)]
pub struct DrawerSpec {
    /// drawer 幅 (mm、X 軸)
    pub width: f32,
    /// drawer 深さ (mm、Y 軸)
    pub depth: f32,
    /// drawer 高さ (mm、Z 軸)
    pub height: f32,
    /// slot 定義列 (順に X 方向配置)
    pub slots: Vec<DrawerSlotSpec>,
    /// 壁厚 (mm、default 1.5 = FDM min_wall)
    pub wall_thickness: f32,
    /// 底厚 (mm、default max(wall, 1.5))
    pub floor_thickness: f32,
    /// 仕切り厚 (mm、default max(wall, 1.2))
    pub divider_thickness: f32,
    /// fillet R (mm)
    pub fillet_radius: f32,
}

impl DrawerSpec {
    /// chopsticks + fork + knife の 3 slot default (250×200×40mm PLA drawer)
    #[must_use]
    pub fn default_chopsticks_set() -> Self {
        Self {
            width: 250.0,
            depth: 200.0,
            height: 40.0,
            slots: vec![
                DrawerSlotSpec {
                    width: 14.0,
                    min_depth: 260.0,
                    count: 2,
                },
                DrawerSlotSpec {
                    width: 32.0,
                    min_depth: 220.0,
                    count: 4,
                },
                DrawerSlotSpec {
                    width: 28.0,
                    min_depth: 250.0,
                    count: 4,
                },
            ],
            wall_thickness: 1.5,
            floor_thickness: 1.5,
            divider_thickness: 1.2,
            fillet_radius: 1.0,
        }
    }
}

/// drawer organizer (Bamboo `drawer.rs` LOL DSL 生成と等価な `SdfNode` を返す)
///
/// 構造:
/// - 外形 tray `RoundedBox` (`width × depth × height`)
/// - 内側 cavities を Union で組立て、Subtraction で outer から刳り抜く
/// - slot 幅は total_width 超過時 scale (Bamboo `drawer.rs::scale` 相当)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{drawer_organizer, DrawerSpec};
/// let tray = drawer_organizer(&DrawerSpec::default_chopsticks_set());
/// ```
#[must_use]
pub fn drawer_organizer(spec: &DrawerSpec) -> SdfNode {
    let tray_hx = spec.width * 0.5;
    let tray_hy = spec.depth * 0.5;
    let tray_hz = spec.height * 0.5;
    let inner_width = spec.width - 2.0 * spec.wall_thickness;
    let inner_hz = (spec.height - spec.floor_thickness) * 0.5;
    let outer = rounded_box(tray_hx, tray_hy, tray_hz, spec.fillet_radius);

    // slot 幅の scale (総幅 > drawer 内寸なら圧縮)
    let total_slots: u32 = spec.slots.iter().map(|s| s.count).sum();
    #[allow(clippy::cast_precision_loss)]
    let total_slot_width: f32 = spec
        .slots
        .iter()
        .map(|s| s.width * s.count as f32)
        .sum::<f32>();
    #[allow(clippy::cast_precision_loss)]
    let total_divider_width = spec.divider_thickness * total_slots.saturating_sub(1) as f32;
    let scale = if total_slot_width + total_divider_width > inner_width {
        (inner_width - total_divider_width) / total_slot_width
    } else {
        1.0
    };

    // 全 cavity を Union で組立て、最後に outer から subtract
    let mut cavities: Option<SdfNode> = None;
    let mut x_cursor = -inner_width * 0.5;
    let cavity_z_offset = spec.floor_thickness * 0.5;
    for slot in &spec.slots {
        let sw = slot.width * scale;
        let sd = slot.min_depth.min(spec.depth - 2.0 * spec.wall_thickness);
        let sd_half = sd * 0.5;
        let sw_half = sw * 0.5 - 0.5; // Bamboo drawer.rs の inset 相当
        for i in 0..slot.count {
            let slot_cx = x_cursor + sw * 0.5;
            let cav = translate(
                rounded_box(sw_half, sd_half, inner_hz, spec.fillet_radius),
                Vec3::new(slot_cx, 0.0, cavity_z_offset),
            );
            cavities = Some(match cavities {
                Some(prev) => union(prev, cav),
                None => cav,
            });
            x_cursor += sw;
            if i < slot.count - 1 || slot.count > 1 {
                x_cursor += spec.divider_thickness;
            }
        }
        if x_cursor < inner_width * 0.5 {
            x_cursor += spec.divider_thickness;
        }
    }

    match cavities {
        Some(c) => subtract(outer, c),
        None => outer,
    }
}

// ────────────────────────────────────────────────────────
// 4. shelf_divider (Bamboo generators/shelf_divider.rs 翻訳)
// ────────────────────────────────────────────────────────

/// shelf divider の寸法仕様 (U 字、Bamboo `shelf_divider.rs::generate` 相当)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShelfDividerSpec {
    /// 全幅 (mm、half_width × 2 = 2 パーツ結合時の合計幅)
    pub total_width: f32,
    /// 奥行 (mm、Y 軸)
    pub depth: f32,
    /// 高さ (mm、Z 軸、側板高)
    pub height: f32,
    /// 壁厚 (mm、`max(input, structural_wall)`)
    pub thickness: f32,
    /// hex cutout 半径 (mm、default 7.5)
    pub hex_hole_radius: f32,
    /// hex cutout ピッチ (mm、default 20)
    pub hex_hole_pitch: f32,
    /// hex cutout 外周余白 (mm、default 15)
    pub hex_border: f32,
}

impl ShelfDividerSpec {
    /// Bamboo `models/shelf/divider-560x250x120/` と同 spec (実プリント合格)
    #[must_use]
    pub const fn field_tested_560x250x120() -> Self {
        Self {
            total_width: 560.0,
            depth: 250.0,
            height: 120.0,
            thickness: 5.0,
            hex_hole_radius: 7.5,
            hex_hole_pitch: 20.0,
            hex_border: 15.0,
        }
    }
}

/// shelf divider (Bamboo `shelf_divider.rs` LOL DSL 生成と等価な `SdfNode` を返す)
///
/// 構造 (Bamboo コメント準拠、逆さ印刷想定で天板 Z=0 に配置):
/// - 天板 `Box3d` (Z=+t、hx × depth × thickness)
/// - 左右 側板 `Box3d` (X=±(hx-t)、Y=depth 中央、Z=wall+wall_hz)
/// - 上記 3 面を `SmoothUnion` (k = wall × 0.5)
/// - 天板に千鳥 hex cutout (`RepeatFinite` × 2 + Y offset)
/// - `Subtraction` で hex holes を刳り抜き
///
/// # 引数
///
/// [`ShelfDividerSpec`] で全パラメータ指定 [`ShelfDividerSpec::field_tested_560x250x120`]
/// で Bamboo 実プリント合格 spec を利用可
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{shelf_divider, ShelfDividerSpec};
/// let s = shelf_divider(&ShelfDividerSpec::field_tested_560x250x120());
/// ```
#[must_use]
pub fn shelf_divider(spec: &ShelfDividerSpec) -> SdfNode {
    let half_width = spec.total_width * 0.5;
    let hx = half_width * 0.5;
    let hy = spec.depth * 0.5;
    let t = spec.thickness * 0.5;
    let plate_z = t;
    let wall_height = spec.height - spec.thickness;
    let wall_hz = wall_height * 0.5;
    let wall_cz = spec.thickness + wall_hz;
    let side_left_x = -(hx - t);
    let side_right_x = hx - t;
    let fillet_k = spec.thickness * 0.5;

    // Positive geometry (plate + 2 side walls) を SmoothUnion で 3-way blend
    let plate = translate(box3d(hx, hy, t), Vec3::new(0.0, 0.0, plate_z));
    let wall_l = translate(box3d(t, hy, wall_hz), Vec3::new(side_left_x, 0.0, wall_cz));
    let wall_r = translate(box3d(t, hy, wall_hz), Vec3::new(side_right_x, 0.0, wall_cz));
    let structure = smooth_union(smooth_union(plate, wall_l, fillet_k), wall_r, fillet_k);

    // Hex cutout (千鳥、Bamboo `shelf_divider.rs::71-100` と等価)
    let grid_hx = hx - spec.hex_border;
    let grid_hy = hy - spec.hex_border;
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    let count_x = (grid_hx / spec.hex_hole_pitch).floor() as u32;
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    let count_y = (grid_hy / spec.hex_hole_pitch).floor() as u32;
    let stagger_x = spec.hex_hole_pitch * 0.5;
    let stagger_y = spec.hex_hole_pitch * 0.866;
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    let count_x2 = ((grid_hx - stagger_x) / spec.hex_hole_pitch).floor() as u32;
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    let count_y2 = ((grid_hy - stagger_y * 0.5) / stagger_y).floor() as u32;
    let hole_half_h = t + 1.0;
    let hole_shape = cylinder(spec.hex_hole_radius, hole_half_h);

    let grid1 = SdfNode::RepeatFinite {
        child: Arc::new(hole_shape.clone()),
        count: [count_x, count_y, 0],
        spacing: Vec3::new(spec.hex_hole_pitch, spec.hex_hole_pitch, 1.0),
    };
    let grid1_placed = translate(grid1, Vec3::new(0.0, 0.0, plate_z));
    let grid2 = SdfNode::RepeatFinite {
        child: Arc::new(hole_shape),
        count: [count_x2, count_y2, 0],
        spacing: Vec3::new(spec.hex_hole_pitch, stagger_y, 1.0),
    };
    let grid2_placed = translate(grid2, Vec3::new(stagger_x, stagger_y * 0.5, plate_z));
    let all_holes = union(grid1_placed, grid2_placed);

    subtract(structure, all_holes)
}

// ────────────────────────────────────────────────────────
// 5. sticky_note_holder (organizer-gridfinity-desk § 2.7)
// ────────────────────────────────────────────────────────

/// 付箋ホルダーの寸法仕様 (76×76mm 小型 or 76×127mm 大型対応)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StickyNoteHolderSpec {
    /// pad 幅 (mm、X、標準 76 = 3 inch)
    pub pad_width: f32,
    /// pad 深さ (mm、Y、標準 76 or 127)
    pub pad_depth: f32,
    /// 全高 (mm、pad 3-8 枚分 = 25-40mm)
    pub height: f32,
    /// 壁厚 (mm、default 1.5)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 1.5)
    pub floor_thickness: f32,
}

impl StickyNoteHolderSpec {
    /// 小型正方形 76×76 × 30mm (Post-it 3×3 inch standard)
    #[must_use]
    pub const fn small_square() -> Self {
        Self {
            pad_width: 76.0,
            pad_depth: 76.0,
            height: 30.0,
            wall_thickness: 1.5,
            floor_thickness: 1.5,
        }
    }
}

/// 付箋ホルダー (`RoundedBox` outer - `Box3d` cavity)
///
/// 構造 (organizer-gridfinity-desk § 2.7 準拠):
/// - Outer: `RoundedBox` (`(pad_width + 2×wall) × (pad_depth + 2×wall) × height`)
/// - Cavity: `Box3d` (`pad_width × pad_depth × (height - floor)`)、Z=+floor/2 offset
/// - `Subtraction` で cavity を outer から刳り抜き
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{sticky_note_holder, StickyNoteHolderSpec};
/// let holder = sticky_note_holder(&StickyNoteHolderSpec::small_square());
/// ```
#[must_use]
pub fn sticky_note_holder(spec: &StickyNoteHolderSpec) -> SdfNode {
    let outer_hx = (spec.pad_width + 2.0 * spec.wall_thickness) * 0.5;
    let outer_hy = (spec.pad_depth + 2.0 * spec.wall_thickness) * 0.5;
    let outer_hz = spec.height * 0.5;
    let inner_hx = spec.pad_width * 0.5;
    let inner_hy = spec.pad_depth * 0.5;
    let cavity_h = spec.height - spec.floor_thickness;
    let inner_hz = cavity_h * 0.5;
    let cavity_offset_z = spec.floor_thickness * 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, 2.0);
    let cavity = translate(
        box3d(inner_hx, inner_hy, inner_hz),
        Vec3::new(0.0, 0.0, cavity_offset_z),
    );
    subtract(outer, cavity)
}

// ────────────────────────────────────────────────────────
// 6. business_card_holder (organizer-gridfinity-desk § 2.6)
// ────────────────────────────────────────────────────────

/// 名刺ホルダーの寸法仕様 (JP meishi 91×55 / US 89×51 / EU 85.6×54)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BusinessCardHolderSpec {
    /// card 幅 (mm、X、JP=91 / US=89 / EU=85.6)
    pub card_width: f32,
    /// card 高さ (mm、Y、JP=55 / US=51 / EU=54)
    pub card_height: f32,
    /// slot 厚 (mm、30-50 枚分 = 20-25mm)
    pub slot_thickness: f32,
    /// 全高 (mm、card の 30-40% が露出、slot depth)
    pub slot_depth: f32,
    /// 壁厚 (mm、default 1.5)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 2.0)
    pub floor_thickness: f32,
}

impl BusinessCardHolderSpec {
    /// JP meishi 91×55mm default (30-50 枚収容想定)
    #[must_use]
    pub const fn jp_meishi() -> Self {
        Self {
            card_width: 91.0,
            card_height: 55.0,
            slot_thickness: 22.0,
            slot_depth: 32.0,
            wall_thickness: 1.5,
            floor_thickness: 2.0,
        }
    }
}

/// 名刺ホルダー (縦置き slot、card protrude で掴みやすさ確保)
///
/// 構造 (organizer-gridfinity-desk § 2.6 準拠):
/// - Outer: `RoundedBox` (`(card_w + 2×wall) × (slot_thickness + 2×wall) × slot_depth`)
/// - Cavity: `Box3d` (`card_w × slot_thickness × (slot_depth - floor)`)、Z=+floor/2 offset
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{business_card_holder, BusinessCardHolderSpec};
/// let holder = business_card_holder(&BusinessCardHolderSpec::jp_meishi());
/// ```
#[must_use]
pub fn business_card_holder(spec: &BusinessCardHolderSpec) -> SdfNode {
    let outer_hx = (spec.card_width + 2.0 * spec.wall_thickness) * 0.5;
    let outer_hy = (spec.slot_thickness + 2.0 * spec.wall_thickness) * 0.5;
    let outer_hz = spec.slot_depth * 0.5;
    let inner_hx = spec.card_width * 0.5;
    let inner_hy = spec.slot_thickness * 0.5;
    let cavity_h = spec.slot_depth - spec.floor_thickness;
    let inner_hz = cavity_h * 0.5;
    let cavity_offset_z = spec.floor_thickness * 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, 2.0);
    let cavity = translate(
        box3d(inner_hx, inner_hy, inner_hz),
        Vec3::new(0.0, 0.0, cavity_offset_z),
    );
    subtract(outer, cavity)
}

// ────────────────────────────────────────────────────────
// 7. pen_cup (organizer-gridfinity-desk § 2.2)
// ────────────────────────────────────────────────────────

/// ペン立ての寸法仕様 (single-compartment 円筒 cup)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PenCupSpec {
    /// cup 内径 (mm、default 70-85、複数本のペン収容想定)
    pub inner_diameter: f32,
    /// cup 全高 (mm、default 90-120)
    pub height: f32,
    /// 壁厚 (mm、default 1.5-2.0)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 2.0)
    pub floor_thickness: f32,
}

impl PenCupSpec {
    /// 標準サイズ Ø75×100mm (organizer-gridfinity-desk § 2.2 sweet spot)
    #[must_use]
    pub const fn standard_75x100() -> Self {
        Self {
            inner_diameter: 75.0,
            height: 100.0,
            wall_thickness: 2.0,
            floor_thickness: 2.0,
        }
    }
}

/// ペン立て (`Cylinder` outer - `Cylinder` cavity、Z-up)
///
/// 構造 (organizer-gridfinity-desk § 2.2 準拠、`cylinder_z` で Z-axis alignment):
/// - Outer: Z-axis `Cylinder` (r = `(inner_dia + 2×wall) / 2`, half_h = `height / 2`)
/// - Cavity: Z-axis `Cylinder` (r = `inner_dia / 2`, half_h = `(height - floor) / 2`)、Z=+floor/2 offset
///   (Z+ 方向が cup 開口部、Z- 方向が floor)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{pen_cup, PenCupSpec};
/// let cup = pen_cup(&PenCupSpec::standard_75x100());
/// ```
#[must_use]
pub fn pen_cup(spec: &PenCupSpec) -> SdfNode {
    let outer_r = (spec.inner_diameter + 2.0 * spec.wall_thickness) * 0.5;
    let outer_hz = spec.height * 0.5;
    let inner_r = spec.inner_diameter * 0.5;
    let cavity_h = spec.height - spec.floor_thickness;
    let inner_hz = cavity_h * 0.5;
    let cavity_offset_z = spec.floor_thickness * 0.5;

    let outer = cylinder_z(outer_r, outer_hz);
    let cavity = translate(
        cylinder_z(inner_r, inner_hz),
        Vec3::new(0.0, 0.0, cavity_offset_z),
    );
    subtract(outer, cavity)
}

// ────────────────────────────────────────────────────────
// 8. phone_stand (organizer-gridfinity-desk § 2.9)
// ────────────────────────────────────────────────────────

/// スマホ / タブレット スタンドの寸法仕様 (L 字構造 + slot)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhoneStandSpec {
    /// slot 幅 (mm、phone 10-15 / tablet 12-18、ケース対応時 +2-3)
    pub slot_width: f32,
    /// slot 奥行 (mm、Y、back plate 厚さ + 底刳り、default 6)
    pub slot_depth: f32,
    /// base 幅 (mm、X、default 80-100 phone / 150-200 tablet)
    pub base_width: f32,
    /// base 奥行 (mm、Y、default 80-100 phone / 100-150 tablet)
    pub base_depth: f32,
    /// base 厚 (mm、Z、default 5-8)
    pub base_thickness: f32,
    /// back plate 高さ (mm、default 80-120 phone / 150-190 tablet)
    pub back_height: f32,
    /// back plate 厚 (mm、Y、default 4-6)
    pub back_thickness: f32,
    /// cable 通し穴径 (mm、default 15-25)、`None` で穴なし
    pub cable_hole_dia: Option<f32>,
}

impl PhoneStandSpec {
    /// スマホ default (base 90×90×6, back 100×5, slot 14mm, cable Ø18)
    #[must_use]
    pub const fn phone_default() -> Self {
        Self {
            slot_width: 14.0,
            slot_depth: 6.0,
            base_width: 90.0,
            base_depth: 90.0,
            base_thickness: 6.0,
            back_height: 100.0,
            back_thickness: 5.0,
            cable_hole_dia: Some(18.0),
        }
    }
}

/// スマホ / タブレット スタンド (L 字 base + back plate、front 面 slot)
///
/// 構造 (organizer-gridfinity-desk § 2.9 準拠):
/// - Base: `Box3d` (`base_width × base_depth × base_thickness`)、Z=+base_thickness/2
/// - Back plate: `Box3d` (`base_width × back_thickness × back_height`)、後端 Y=-(base_depth/2 - back_thickness/2)、Z 中心=+base_thickness+back_height/2
/// - Slot: `Box3d` を back plate 前面から刳り抜き (phone を差し込む溝)
/// - Cable hole: `Cylinder` を base 中央から Z 貫通 (指定時のみ)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{phone_stand, PhoneStandSpec};
/// let stand = phone_stand(&PhoneStandSpec::phone_default());
/// ```
#[must_use]
pub fn phone_stand(spec: &PhoneStandSpec) -> SdfNode {
    let base_hx = spec.base_width * 0.5;
    let base_hy = spec.base_depth * 0.5;
    let base_hz = spec.base_thickness * 0.5;
    let back_hx = spec.base_width * 0.5;
    let back_hy = spec.back_thickness * 0.5;
    let back_hz = spec.back_height * 0.5;

    // Base at Z=+base_hz (bottom on Z=0)
    let base = translate(
        box3d(base_hx, base_hy, base_hz),
        Vec3::new(0.0, 0.0, base_hz),
    );

    // Back plate at rear edge (Y=-(base_hy - back_hy))、Z=base_thickness + back_hz
    let back_plate_y = -(base_hy - back_hy);
    let back_plate_z = spec.base_thickness + back_hz;
    let back = translate(
        box3d(back_hx, back_hy, back_hz),
        Vec3::new(0.0, back_plate_y, back_plate_z),
    );

    // Slot cut: front face of back plate、Y=back_plate_y + back_hy (前面)
    // slot 幅方向 X、深さ Y、高さ Z (top 部分に配置、phone を挿入する溝)
    let slot_hx = spec.slot_width * 0.5;
    let slot_hy = back_hy + 0.5; // 貫通用余裕
    let slot_hz = spec.back_height * 0.4; // back 高さの 40% を slot 深に
    let slot_center_z = back_plate_z + back_hz - slot_hz;
    let slot = translate(
        box3d(slot_hx, slot_hy, slot_hz),
        Vec3::new(0.0, back_plate_y, slot_center_z),
    );

    let structure = union(base, back);
    let with_slot = subtract(structure, slot);

    // Cable hole (指定時のみ、base 中央 Z 貫通、Z-axis cylinder で viewer Z-up 対応)
    if let Some(dia) = spec.cable_hole_dia {
        let hole = translate(
            cylinder_z(dia * 0.5, base_hz + 1.0),
            Vec3::new(0.0, 0.0, base_hz),
        );
        subtract(with_slot, hole)
    } else {
        with_slot
    }
}

// ────────────────────────────────────────────────────────
// 9. headphone_holder (organizer-gridfinity-desk § 2.5)
// ────────────────────────────────────────────────────────

/// ヘッドホンホルダー spec (wall-mount type、hook arm + backplate)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeadphoneHolderSpec {
    /// hook arm 長さ (mm、Z 方向 protrusion、default 80)
    pub arm_length: f32,
    /// hook arm 太さ (mm、Y 方向厚さ、default 6)
    pub arm_thickness: f32,
    /// hook arm 幅 (mm、X、headband_width + margin、default 50)
    pub arm_width: f32,
    /// mount plate 幅 (mm、X、default 100)
    pub mount_width: f32,
    /// mount plate 高さ (mm、Y、default 68)
    pub mount_height: f32,
    /// mount plate 厚 (mm、Z、default 6)
    pub mount_thickness: f32,
    /// hook tip 上方向 curl (mm、Y、default 18)、slip 防止
    pub hook_tip_up: f32,
    /// M4 mount hole 径 (mm)、`None` で穴なし
    pub mount_hole_dia: Option<f32>,
}

impl HeadphoneHolderSpec {
    /// 標準 wall-mount default (arm 80mm、mount 100×68mm、M4 穴あり)
    #[must_use]
    pub const fn wall_mount_default() -> Self {
        Self {
            arm_length: 80.0,
            arm_thickness: 6.0,
            arm_width: 50.0,
            mount_width: 100.0,
            mount_height: 68.0,
            mount_thickness: 6.0,
            hook_tip_up: 18.0,
            mount_hole_dia: Some(4.5),
        }
    }
}

/// ヘッドホンホルダー (wall_hook variant、hook 太くて headband 対応)
///
/// 構造 (organizer-gridfinity-desk § 2.5 準拠、wall_hook と同 pattern):
/// - Mount plate: `RoundedBox` (X×Y×Z = mount_w × mount_h × mount_thickness)、Z=0 中心
/// - Hook arm: `RoundedBox`、mount plate 前面 (Z=+mount_thickness/2) から Z 方向 protrusion
/// - Hook tip: `RoundedBox`、arm 先端で Y 上方向 curl (slip 防止)
/// - 3-way `SmoothUnion` で blend
/// - Mount holes: `Box3d` (方形穴、Z-thickness で 2 個縦並び)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{headphone_holder, HeadphoneHolderSpec};
/// let h = headphone_holder(&HeadphoneHolderSpec::wall_mount_default());
/// ```
#[must_use]
pub fn headphone_holder(spec: &HeadphoneHolderSpec) -> SdfNode {
    let mp_hx = spec.mount_width * 0.5;
    let mp_hy = spec.mount_height * 0.5;
    let mp_hz = spec.mount_thickness * 0.5;
    let arm_hx = spec.arm_width * 0.5;
    let arm_hy = spec.arm_thickness * 0.5;
    let arm_hz = spec.arm_length * 0.5;
    let tip_hy = spec.hook_tip_up * 0.5;

    let mount = rounded_box(mp_hx, mp_hy, mp_hz, 3.0);
    let arm = translate(
        rounded_box(arm_hx, arm_hy, arm_hz, 2.0),
        Vec3::new(0.0, 0.0, mp_hz + arm_hz),
    );
    let tip = translate(
        rounded_box(arm_hx, tip_hy, arm_hy, 2.0),
        Vec3::new(0.0, arm_hy + tip_hy, mp_hz + spec.arm_length - arm_hy),
    );

    let body = smooth_union(smooth_union(mount, arm, 3.0), tip, 3.0);

    if let Some(dia) = spec.mount_hole_dia {
        let hole_hx = dia * 0.5;
        let hole_hy = dia * 0.5;
        let hole_hz = mp_hz + 0.5;
        let hole_spacing = mp_hy * 0.6;
        let hole_top = translate(
            box3d(hole_hx, hole_hy, hole_hz),
            Vec3::new(0.0, hole_spacing, 0.0),
        );
        let hole_bot = translate(
            box3d(hole_hx, hole_hy, hole_hz),
            Vec3::new(0.0, -hole_spacing, 0.0),
        );
        // 2026-08-20 unwrap: 元 Y-up 設計 = mount plate 100×68 flat on bed / arm 上向き が
        // print-optimal (wall_hook と同 pattern) to_z_up wrap は mount thin edge on bed
        // 化して印刷不能にしていたので撤廃
        subtract(body, union(hole_top, hole_bot))
    } else {
        body
    }
}

// ────────────────────────────────────────────────────────
// 10. under_desk_mount (organizer-gridfinity-desk § 2.4)
// ────────────────────────────────────────────────────────

/// 机下 clamp mount spec (C 字クランプ、机端に取付)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnderDeskMountSpec {
    /// clamp gap = 机厚 (mm、Y、default 25、range 0-60)
    pub desk_thickness: f32,
    /// clamp jaw 幅 (mm、X、default 40)
    pub clamp_width: f32,
    /// clamp jaw 奥行 (mm、Z、default 50)
    pub clamp_depth: f32,
    /// clamp 壁厚 (mm、default 4)、構造強度
    pub clamp_wall_thickness: f32,
    /// screw hole 径 (mm、M4 = 4mm、`None` で穴なし = 両面テープ想定)
    pub screw_hole_dia: Option<f32>,
}

impl UnderDeskMountSpec {
    /// 標準机 default (25mm 机厚、40mm jaw、M4 screw)
    #[must_use]
    pub const fn standard_desk() -> Self {
        Self {
            desk_thickness: 25.0,
            clamp_width: 40.0,
            clamp_depth: 50.0,
            clamp_wall_thickness: 4.0,
            screw_hole_dia: Some(4.0),
        }
    }
}

/// 机下 clamp mount (C 字構造、机端に上下 jaw で挟み込み)
///
/// 構造 (organizer-gridfinity-desk § 2.4 準拠):
/// - Top jaw: `Box3d` (机上に載る、X×Y×Z = clamp_w × wall_t × clamp_depth)、Y=+top_y 位置
/// - Bottom jaw: `Box3d` (机下、同 size)、Y=-bottom_y 位置
/// - Back stem: `Box3d` (背面接続、X×Y×Z = clamp_w × (desk+2×wall) × wall_t)、Z=-back_z 位置
/// - Screw hole: `Cylinder` Y-axis (bottom jaw を貫通、締付ネジ用)、指定時のみ
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{under_desk_mount, UnderDeskMountSpec};
/// let m = under_desk_mount(&UnderDeskMountSpec::standard_desk());
/// ```
#[must_use]
pub fn under_desk_mount(spec: &UnderDeskMountSpec) -> SdfNode {
    let jaw_hx = spec.clamp_width * 0.5;
    let jaw_hy = spec.clamp_wall_thickness * 0.5;
    let jaw_hz = spec.clamp_depth * 0.5;
    let top_y = spec.desk_thickness * 0.5 + jaw_hy;
    let bottom_y = -(spec.desk_thickness * 0.5 + jaw_hy);
    let stem_hx = spec.clamp_width * 0.5;
    let stem_hy = (spec.desk_thickness + 2.0 * spec.clamp_wall_thickness) * 0.5;
    let stem_hz = spec.clamp_wall_thickness * 0.5;
    let back_z = -(jaw_hz - stem_hz);

    let top_jaw = translate(box3d(jaw_hx, jaw_hy, jaw_hz), Vec3::new(0.0, top_y, 0.0));
    let bottom_jaw = translate(box3d(jaw_hx, jaw_hy, jaw_hz), Vec3::new(0.0, bottom_y, 0.0));
    let back_stem = translate(
        box3d(stem_hx, stem_hy, stem_hz),
        Vec3::new(0.0, 0.0, back_z),
    );

    let body = smooth_union(smooth_union(top_jaw, bottom_jaw, 2.0), back_stem, 2.0);

    if let Some(dia) = spec.screw_hole_dia {
        // 2026-08-20 unwrap: 元 Y-up 設計 = back stem 40×33 flat on bed / jaws 上向き平行
        // (`|_|` U 字) が print-optimal to_z_up wrap は thin edge on bed 化して不能にしていた
        // Y-axis cylinder = bottom jaw を Y 方向 (元 Y-up 世界の縦) に貫通
        let hole = translate(
            cylinder(dia * 0.5, jaw_hy + 0.5),
            Vec3::new(0.0, bottom_y, 0.0),
        );
        subtract(body, hole)
    } else {
        body
    }
}

// ────────────────────────────────────────────────────────
// 11. desk_shelf (organizer-gridfinity-desk § 2.3)
// ────────────────────────────────────────────────────────

/// 卓上シェルフ spec (平板 + 左右 2 脚、shelf_divider 簡易版)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeskShelfSpec {
    /// shelf 幅 (mm、X、default 400、range 200-500)
    pub shelf_width: f32,
    /// shelf 奥行 (mm、Z、default 200、range 150-300)
    pub shelf_depth: f32,
    /// shelf 厚 (mm、Y、default 5)
    pub shelf_thickness: f32,
    /// leg 高さ (mm、Y、default 100、range 60-150)
    pub leg_height: f32,
    /// leg 厚 (mm、X、default 20)、構造強度
    pub leg_thickness: f32,
}

impl DeskShelfSpec {
    /// 標準卓上 default (400×200×100mm、単一プリント想定)
    #[must_use]
    pub const fn desktop_400x200() -> Self {
        Self {
            shelf_width: 400.0,
            shelf_depth: 200.0,
            shelf_thickness: 5.0,
            leg_height: 100.0,
            leg_thickness: 20.0,
        }
    }
}

/// 卓上シェルフ (shelf plate + 左右 2 脚、シンプル L 構造)
///
/// 構造 (organizer-gridfinity-desk § 2.3 準拠、shelf_divider 簡易版):
/// - Shelf plate: `Box3d` (X×Y×Z = shelf_w × shelf_t × shelf_d)、Y=+leg_h + shelf_t/2
/// - Left leg: `Box3d` (X×Y×Z = leg_t × leg_h × shelf_d)、X=-(shelf_w/2 - leg_t/2)、Y=leg_h/2
/// - Right leg: 同 X=+(shelf_w/2 - leg_t/2)
/// - `SmoothUnion` で 3-way blend
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{desk_shelf, DeskShelfSpec};
/// let s = desk_shelf(&DeskShelfSpec::desktop_400x200());
/// ```
#[must_use]
pub fn desk_shelf(spec: &DeskShelfSpec) -> SdfNode {
    let shelf_hx = spec.shelf_width * 0.5;
    let shelf_hy = spec.shelf_thickness * 0.5;
    let shelf_hz = spec.shelf_depth * 0.5;
    let leg_hx = spec.leg_thickness * 0.5;
    let leg_hy = spec.leg_height * 0.5;
    let leg_hz = spec.shelf_depth * 0.5;

    let shelf_y = spec.leg_height + shelf_hy;
    let leg_x = shelf_hx - leg_hx;

    let shelf = translate(
        box3d(shelf_hx, shelf_hy, shelf_hz),
        Vec3::new(0.0, shelf_y, 0.0),
    );
    let leg_l = translate(
        box3d(leg_hx, leg_hy, leg_hz),
        Vec3::new(-leg_x, leg_hy, 0.0),
    );
    let leg_r = translate(box3d(leg_hx, leg_hy, leg_hz), Vec3::new(leg_x, leg_hy, 0.0));

    let fillet_k = spec.shelf_thickness * 0.5;
    // 2026-08-20 flip: shelf plate 400×200 を bed に置いて legs を上向きピラーに
    // to_z_up (Y→+Z) だと legs 下 / shelf 上 = 380mm ブリッジ = 印刷不能
    // to_z_up_flipped (Y→-Z) で intended-top (shelf) が Z=0 bed 側に
    to_z_up_flipped(smooth_union(
        smooth_union(shelf, leg_l, fillet_k),
        leg_r,
        fillet_k,
    ))
}

// ────────────────────────────────────────────────────────
// 12. monitor_riser (organizer-gridfinity-desk § 2.1)
// ────────────────────────────────────────────────────────

/// モニターライザー spec (簡易版、単一プリント想定 = 250mm 以下)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonitorRiserSpec {
    /// 全幅 (mm、X、default 250、range 200-280 単一プリント制約)
    pub width: f32,
    /// 全奥行 (mm、Z、default 180、range 150-240)
    pub depth: f32,
    /// 全高 (mm、Y、default 90、range 60-120 ergonomic range)
    pub height: f32,
    /// プラットフォーム厚 (mm、Y、default 8)
    pub platform_thickness: f32,
    /// leg 厚 (mm、X、default 25)、構造強度
    pub leg_thickness: f32,
    /// cable 通し穴径 (mm、`None` で穴なし、default `Some(40)`)
    pub cable_hole_dia: Option<f32>,
}

impl MonitorRiserSpec {
    /// 標準 compact desk default (250×180×90mm、cable Ø40mm)
    #[must_use]
    pub const fn compact_desk() -> Self {
        Self {
            width: 250.0,
            depth: 180.0,
            height: 90.0,
            platform_thickness: 8.0,
            leg_thickness: 25.0,
            cable_hole_dia: Some(40.0),
        }
    }
}

/// モニターライザー (platform + 左右 2 脚 + optional cable hole、Z-up)
///
/// 構造 (organizer-gridfinity-desk § 2.1 準拠、簡易版 = 単一プリント):
/// - Platform: `RoundedBox` (X×Y×Z = width × depth × plat_t)、Z 上端
/// - Left leg: `Box3d` (X×Y×Z = leg_t × depth × leg_h)、Z 方向脚
/// - Right leg: 同、X 反対
/// - Cable hole: Z-axis `Cylinder` (`cylinder_z`、platform 貫通、指定時のみ)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{monitor_riser, MonitorRiserSpec};
/// let m = monitor_riser(&MonitorRiserSpec::compact_desk());
/// ```
#[must_use]
pub fn monitor_riser(spec: &MonitorRiserSpec) -> SdfNode {
    let plat_hx = spec.width * 0.5;
    let plat_hy = spec.depth * 0.5;
    let plat_hz = spec.platform_thickness * 0.5;
    let leg_hx = spec.leg_thickness * 0.5;
    let leg_hy = spec.depth * 0.45; // depth の 90% で少し内側
    let leg_h = spec.height - spec.platform_thickness;
    let leg_hz = leg_h * 0.5;

    let plat_z = leg_h + plat_hz;
    let leg_x = plat_hx - leg_hx;

    let platform = translate(
        rounded_box(plat_hx, plat_hy, plat_hz, 4.0),
        Vec3::new(0.0, 0.0, plat_z),
    );
    let leg_l = translate(
        box3d(leg_hx, leg_hy, leg_hz),
        Vec3::new(-leg_x, 0.0, leg_hz),
    );
    let leg_r = translate(box3d(leg_hx, leg_hy, leg_hz), Vec3::new(leg_x, 0.0, leg_hz));

    let structure = smooth_union(smooth_union(platform, leg_l, 2.0), leg_r, 2.0);

    if let Some(dia) = spec.cable_hole_dia {
        // Z-axis cylinder = platform 上下貫通 (Z 方向)
        let hole = translate(
            cylinder_z(dia * 0.5, plat_hz + 0.5),
            Vec3::new(0.0, 0.0, plat_z),
        );
        subtract(structure, hole)
    } else {
        structure
    }
}

// ────────────────────────────────────────────────────────
// 13. coaster (household § 7)
// ────────────────────────────────────────────────────────

/// コースター spec (round disc、top に shallow recess で rim 形成)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoasterSpec {
    /// 直径 (mm、default 95、range 80-110)
    pub diameter: f32,
    /// 全厚 (mm、default 5、range 4-8)
    pub thickness: f32,
    /// rim 幅 (mm、default 2.5)、外周の rim の水平厚
    pub lip_width: f32,
    /// rim 高さ (mm、default 1.5)、top からの rim 突出高
    pub lip_height: f32,
}

impl CoasterSpec {
    /// 標準 round Ø95×5mm (household § 7 sweet spot)
    #[must_use]
    pub const fn round_95x5() -> Self {
        Self {
            diameter: 95.0,
            thickness: 5.0,
            lip_width: 2.5,
            lip_height: 1.5,
        }
    }
}

/// 円形コースター (bowl 状、rim で液滴 catch、Z-up)
///
/// 構造 (household § 7 準拠、`cylinder_z` で Z-axis alignment):
/// - Base: Z-axis `Cylinder` (r = `diameter/2`, half_h = `thickness/2`)
/// - Recess: Z-axis `Cylinder` (r = `(diameter - 2×lip_width)/2`, depth = `lip_height`)
///   Z+ 方向 (上面) から subtract、`Subtraction { base, recess }`
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{coaster, CoasterSpec};
/// let c = coaster(&CoasterSpec::round_95x5());
/// ```
#[must_use]
pub fn coaster(spec: &CoasterSpec) -> SdfNode {
    let outer_r = spec.diameter * 0.5;
    let outer_hz = spec.thickness * 0.5;
    let inner_r = outer_r - spec.lip_width;
    let recess_hz = spec.lip_height * 0.5;
    let recess_offset_z = outer_hz - recess_hz;

    let base = cylinder_z(outer_r, outer_hz);
    let recess = translate(
        cylinder_z(inner_r, recess_hz + 0.5),
        Vec3::new(0.0, 0.0, recess_offset_z + 0.25),
    );
    subtract(base, recess)
}

// ────────────────────────────────────────────────────────
// 14. tissue_box_cover (household § 1)
// ────────────────────────────────────────────────────────

/// ティッシュボックスカバー spec (bottom open で箱にかぶせる、top に pull slot)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TissueBoxCoverSpec {
    /// 内部 長さ (mm、L、default 231 = US rectangular + 2mm clearance)
    pub internal_length: f32,
    /// 内部 幅 (mm、W、default 116 = US rectangular + 2mm clearance)
    pub internal_width: f32,
    /// 内部 高さ (mm、H、default 53)
    pub internal_height: f32,
    /// 壁厚 (mm、default 1.6)
    pub wall_thickness: f32,
    /// top pull slot 長辺 (mm、default 80、slot は long axis に配置)
    pub slot_length: f32,
    /// top pull slot 短辺 (mm、default 30)
    pub slot_width: f32,
}

impl TissueBoxCoverSpec {
    /// US rectangular standard (231×116×53mm 内部、tissue 抽出 slot 80×30)
    #[must_use]
    pub const fn rectangular_us() -> Self {
        Self {
            internal_length: 231.0,
            internal_width: 116.0,
            internal_height: 53.0,
            wall_thickness: 1.6,
            slot_length: 80.0,
            slot_width: 30.0,
        }
    }
}

/// ティッシュボックスカバー (bottom open、top pull slot 付き)
///
/// 構造 (household § 1 準拠、Y-up、bottom Y=-hy 側 open):
/// - Outer: `RoundedBox` (`(L+2w) × (H+w) × (W+2w)`)、上面 (Y+) は塞ぐ、底面 (Y-) は cavity で貫通
/// - Cavity: `Box3d` (`L × (H+w) × W`)、Y- 方向にオフセット (底面貫通)
/// - Top slot: `Box3d` (`slot_L × w × slot_W`)、Y+ 上面配置 (tissue 抽出用)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{tissue_box_cover, TissueBoxCoverSpec};
/// let t = tissue_box_cover(&TissueBoxCoverSpec::rectangular_us());
/// ```
#[must_use]
pub fn tissue_box_cover(spec: &TissueBoxCoverSpec) -> SdfNode {
    let ext_l = spec.internal_length + 2.0 * spec.wall_thickness;
    let ext_w = spec.internal_width + 2.0 * spec.wall_thickness;
    let ext_h = spec.internal_height + spec.wall_thickness;

    let outer_hx = ext_l * 0.5;
    let outer_hy = ext_h * 0.5;
    let outer_hz = ext_w * 0.5;

    // Cavity: 底面 (Y-) を開口したいので Y- 方向オフセット、Y+ (top wall) 残す
    let cavity_hx = spec.internal_length * 0.5;
    let cavity_hy = (spec.internal_height + spec.wall_thickness + 1.0) * 0.5;
    let cavity_hz = spec.internal_width * 0.5;
    let cavity_offset_y = -(spec.wall_thickness + 0.5) * 0.5;

    // Top slot (Y+ 面貫通): X 方向 slot_length、Y 方向 wall_thickness+margin、Z 方向 slot_width
    let slot_hx = spec.slot_length * 0.5;
    let slot_hy = (spec.wall_thickness + 1.0) * 0.5;
    let slot_hz = spec.slot_width * 0.5;
    let slot_offset_y = outer_hy - slot_hy + 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, 3.0);
    let cavity = translate(
        box3d(cavity_hx, cavity_hy, cavity_hz),
        Vec3::new(0.0, cavity_offset_y, 0.0),
    );
    let slot = translate(
        box3d(slot_hx, slot_hy, slot_hz),
        Vec3::new(0.0, slot_offset_y, 0.0),
    );

    // 2026-08-20 flip: household § 1 spec「Print upside-down: open bottom up
    // to avoid supports on top surface」準拠、slot 面 (元 +Y) を bed 側 (Z=0) に、
    // bottom-open (元 -Y) を天井に置いて ceiling bridging を回避
    to_z_up_flipped(subtract(subtract(outer, cavity), slot))
}

// ────────────────────────────────────────────────────────
// 15. storage_box (household § 3、基本形 lid なし)
// ────────────────────────────────────────────────────────

/// 収納 BOX spec (top open、lid + hinge は future sprint)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StorageBoxSpec {
    /// 内部 長さ (mm、L、default 150 = medium size)
    pub internal_length: f32,
    /// 内部 幅 (mm、W、default 100)
    pub internal_width: f32,
    /// 内部 高さ (mm、H、default 60)
    pub internal_height: f32,
    /// 壁厚 (mm、default 2.0)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 2.0)
    pub floor_thickness: f32,
}

impl StorageBoxSpec {
    /// medium size 150×100×60mm 内部 (household § 3 medium spec)
    #[must_use]
    pub const fn medium() -> Self {
        Self {
            internal_length: 150.0,
            internal_width: 100.0,
            internal_height: 60.0,
            wall_thickness: 2.0,
            floor_thickness: 2.0,
        }
    }
}

/// 収納 BOX 基本形 (top open、底 + 4 側壁、lid + hinge なし)
///
/// 構造 (household § 3 準拠、Y-up、top (Y+) 側 open):
/// - Outer: `RoundedBox` (`(L+2w) × (H+floor) × (W+2w)`)
/// - Cavity: `Box3d` (`L × (H+margin) × W`)、Y+ 方向にオフセット (top 開口)
///
/// lid + hinge は future sprint (Print-in-place knuckle / filament pin / living hinge)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{storage_box, StorageBoxSpec};
/// let b = storage_box(&StorageBoxSpec::medium());
/// ```
#[must_use]
pub fn storage_box(spec: &StorageBoxSpec) -> SdfNode {
    let ext_l = spec.internal_length + 2.0 * spec.wall_thickness;
    let ext_w = spec.internal_width + 2.0 * spec.wall_thickness;
    let ext_h = spec.internal_height + spec.floor_thickness;

    let outer_hx = ext_l * 0.5;
    let outer_hy = ext_h * 0.5;
    let outer_hz = ext_w * 0.5;

    let cavity_hx = spec.internal_length * 0.5;
    let cavity_hy = (spec.internal_height + 1.0) * 0.5;
    let cavity_hz = spec.internal_width * 0.5;
    let cavity_offset_y = spec.floor_thickness * 0.5 + 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, 2.0);
    let cavity = translate(
        box3d(cavity_hx, cavity_hy, cavity_hz),
        Vec3::new(0.0, cavity_offset_y, 0.0),
    );
    to_z_up(subtract(outer, cavity))
}

// ────────────────────────────────────────────────────────
// 16. cable_clip (hobby-diy § 2 Desk Organizer / Cable Management)
// ────────────────────────────────────────────────────────

/// ケーブルクリップ spec (半月 shell、snap-fit で cable を保持)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CableClipSpec {
    /// 対応ケーブル直径 (mm、USB-A 3.5 / Ethernet 6 / HDMI 7 / Power 8-10、default 7)
    pub cable_diameter: f32,
    /// クリップ長 (mm、Y 軸方向 cable 走行方向、default 28)
    pub clip_length: f32,
    /// 壁厚 (mm、default 2.0)
    pub wall_thickness: f32,
    /// snap-fit 開口幅 = `cable_diameter × 本値` (default 0.7 = 30% 狭い)
    pub opening_ratio: f32,
}

impl CableClipSpec {
    /// HDMI ケーブル用 Ø7 × L28 (hobby-diy § 2 Clip Length 表 HDMI 行)
    #[must_use]
    pub const fn hdmi() -> Self {
        Self {
            cable_diameter: 7.0,
            clip_length: 28.0,
            wall_thickness: 2.0,
            opening_ratio: 0.7,
        }
    }
}

/// ケーブルクリップ (Y-axis 沿い cable、+Z 開口 snap-fit)
///
/// 構造 (hobby-diy § 2 準拠、Z-up 直接設計、wrapper なし):
/// - Outer: `RoundedBox` (`(cable+2w) × length × (cable+2w)`)
/// - Cavity: Y-axis `Cylinder` (r = `cable/2 + 0.1` clearance、cable 走行方向)
/// - Opening: `Box3d` slot (X 狭 opening、Z+ 上端から中央下へ)、snap-fit で cable を押し込む
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{cable_clip, CableClipSpec};
/// let c = cable_clip(&CableClipSpec::hdmi());
/// ```
#[must_use]
pub fn cable_clip(spec: &CableClipSpec) -> SdfNode {
    let outer_side = spec.cable_diameter + 2.0 * spec.wall_thickness;
    let outer_hx = outer_side * 0.5;
    let outer_hy = spec.clip_length * 0.5;
    let outer_hz = outer_side * 0.5;

    let cavity_r = spec.cable_diameter * 0.5 + 0.1;
    let cavity_hy = outer_hy + 1.0;

    let slot_hx = spec.cable_diameter * spec.opening_ratio * 0.5;
    let slot_hy = outer_hy + 1.0;
    // slot: 上端 (Z+ = outer_hz+1) から cable 中心の少し下 (Z = -cable_dia*0.15) まで
    let slot_top_z = outer_hz + 1.0;
    let slot_bottom_z = -spec.cable_diameter * 0.15;
    let slot_hz = (slot_top_z - slot_bottom_z) * 0.5;
    let slot_offset_z = (slot_top_z + slot_bottom_z) * 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, spec.wall_thickness * 0.5);
    let cavity = cylinder(cavity_r, cavity_hy);
    let slot = translate(
        box3d(slot_hx, slot_hy, slot_hz),
        Vec3::new(0.0, 0.0, slot_offset_z),
    );

    subtract(subtract(outer, cavity), slot)
}

// ────────────────────────────────────────────────────────
// 17. led_channel (hobby-diy § 3 LED Light Strip Channel)
// ────────────────────────────────────────────────────────

/// LED strip channel spec (U 溝、strip を top 側から挿入)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LedChannelSpec {
    /// LED strip PCB 幅 (mm、SMD3528=8 / WS2812B=10-12、default 10)
    pub strip_width: f32,
    /// channel 全長 (mm、Y 軸方向、default 300)
    pub channel_length: f32,
    /// channel 内深さ (mm、hobby-diy § 3 inner depth 表、default 2.5)
    pub channel_depth: f32,
    /// 壁厚 (mm、default 2.0)
    pub wall_thickness: f32,
}

impl LedChannelSpec {
    /// WS2812B 10mm × 300mm (hobby-diy § 3 標準)
    #[must_use]
    pub const fn ws2812b_10mm() -> Self {
        Self {
            strip_width: 10.0,
            channel_length: 300.0,
            channel_depth: 2.5,
            wall_thickness: 2.0,
        }
    }
}

/// LED strip channel (Y-axis 沿い strip、+Z 開口 U 溝、Z-up 直接設計)
///
/// 構造 (hobby-diy § 3 準拠):
/// - Outer: `Box3d` (`(strip+2w) × length × (depth+floor)`)
/// - Cavity: `Box3d` (`(strip+1mm 遊び) × length+1 × depth+1`)、Z+ 上端開口
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{led_channel, LedChannelSpec};
/// let c = led_channel(&LedChannelSpec::ws2812b_10mm());
/// ```
#[must_use]
pub fn led_channel(spec: &LedChannelSpec) -> SdfNode {
    let inner_w = spec.strip_width + 1.0; // 0.5mm clearance / side
    let outer_w = spec.strip_width + 2.0 * spec.wall_thickness;
    let floor = 1.5;
    let outer_h = spec.channel_depth + floor;

    let outer_hx = outer_w * 0.5;
    let outer_hy = spec.channel_length * 0.5;
    let outer_hz = outer_h * 0.5;

    let cavity_hx = inner_w * 0.5;
    let cavity_hy = outer_hy + 1.0;
    let cavity_hz = (spec.channel_depth + 1.0) * 0.5;
    let cavity_offset_z = outer_hz - cavity_hz + 0.5;

    let outer = box3d(outer_hx, outer_hy, outer_hz);
    let cavity = translate(
        box3d(cavity_hx, cavity_hy, cavity_hz),
        Vec3::new(0.0, 0.0, cavity_offset_z),
    );

    subtract(outer, cavity)
}

// ────────────────────────────────────────────────────────
// 18. card_tray (hobby-diy § 6 Board Game Inserts / Card Tray)
// ────────────────────────────────────────────────────────

/// カードトレー spec (top 開口、front edge に finger 半円 cutout)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardTraySpec {
    /// カード幅 (mm、Poker=63 / Mini Euro=44 / Standard Euro=59、default 63)
    pub card_width: f32,
    /// カード高さ (mm、Poker=88 / Mini Euro=68 / Standard Euro=92、default 88)
    pub card_height: f32,
    /// tray 内深さ (mm、hobby-diy § 6 depth per 50 cards ≈ 10mm、default 30 = 100-150 cards)
    pub tray_depth: f32,
    /// カード↔壁 clearance/side (mm、default 1.0 = 全 clearance 2mm)
    pub card_clearance: f32,
    /// 壁厚 (mm、default 2.0)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 1.5)
    pub floor_thickness: f32,
    /// finger cutout 半径 (mm、hobby-diy § 6 15-20mm wide → r=9、default 9)
    pub finger_notch_radius: f32,
}

impl CardTraySpec {
    /// Poker card 63×88mm、深さ 30mm (hobby-diy § 6 Standard Poker、100-150 cards)
    #[must_use]
    pub const fn poker() -> Self {
        Self {
            card_width: 63.0,
            card_height: 88.0,
            tray_depth: 30.0,
            card_clearance: 1.0,
            wall_thickness: 2.0,
            floor_thickness: 1.5,
            finger_notch_radius: 9.0,
        }
    }
}

/// カードトレー (top 開口、front edge finger 半円 cutout、印刷正立 = 底が bed)
///
/// 構造 (hobby-diy § 6 Card Tray Dimensions 準拠、Y-up 設計 → `to_z_up` wrap):
/// - Outer: `RoundedBox` (`(cw+2×(clear+wall)) × (depth+floor) × (ch+2×(clear+wall))`)
/// - Cavity: `Box3d` (`(cw+2×clear) × (depth+1) × (ch+2×clear)`)、Y+ 上端開口
/// - Finger notch: Y-axis `Cylinder` (r=9)、front edge (Z=-`outer_hz`) 中央、Y 全高貫通
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{card_tray, CardTraySpec};
/// let t = card_tray(&CardTraySpec::poker());
/// ```
#[must_use]
pub fn card_tray(spec: &CardTraySpec) -> SdfNode {
    let ext_x = spec.card_width + 2.0 * (spec.card_clearance + spec.wall_thickness);
    let ext_z = spec.card_height + 2.0 * (spec.card_clearance + spec.wall_thickness);
    let ext_y = spec.tray_depth + spec.floor_thickness;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    let cavity_hx = (spec.card_width + 2.0 * spec.card_clearance) * 0.5;
    let cavity_hy = (spec.tray_depth + 1.0) * 0.5;
    let cavity_hz = (spec.card_height + 2.0 * spec.card_clearance) * 0.5;
    let cavity_offset_y = spec.floor_thickness * 0.5 + 0.5;

    let notch_hy = outer_hy + 1.0;
    let notch_offset_z = -outer_hz;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, spec.wall_thickness);
    let cavity = translate(
        box3d(cavity_hx, cavity_hy, cavity_hz),
        Vec3::new(0.0, cavity_offset_y, 0.0),
    );
    let notch = translate(
        cylinder(spec.finger_notch_radius, notch_hy),
        Vec3::new(0.0, 0.0, notch_offset_z),
    );

    to_z_up(subtract(subtract(outer, cavity), notch))
}

// ────────────────────────────────────────────────────────
// 19. token_well (hobby-diy § 6 Board Game Inserts / Token Well)
// ────────────────────────────────────────────────────────

/// トークン井戸 spec (row 状に count 個の円筒 well、dice/meeples/miniatures)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TokenWellSpec {
    /// well 直径 (mm、default 20)
    pub well_diameter: f32,
    /// well 深さ (mm、shallow=10-15 / dice=20-25 / mini=30-40、default 20)
    pub well_depth: f32,
    /// well 個数 (row 方向、default 4、実用 range 1-10)
    pub well_count: u32,
    /// well 間 clearance/side (mm、default 1.0 = 全 clearance 2mm)
    pub well_clearance: f32,
    /// 外周 壁厚 (mm、default 2.0)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 1.5)
    pub floor_thickness: f32,
}

impl TokenWellSpec {
    /// dice 用 4 well × Ø20 × 深 20mm (hobby-diy § 6 dice/meeples 標準)
    #[must_use]
    pub const fn dice_4() -> Self {
        Self {
            well_diameter: 20.0,
            well_depth: 20.0,
            well_count: 4,
            well_clearance: 1.0,
            wall_thickness: 2.0,
            floor_thickness: 1.5,
        }
    }
}

/// トークン井戸 (row 状 count well、top 開口、印刷正立、`to_z_up` wrap)
///
/// 構造 (hobby-diy § 6 Token Well Design 準拠、Y-up 設計):
/// - Outer: `RoundedBox` (`(count×(dia+2×clear)+2×wall) × (depth+floor) × (dia+2×(clear+wall))`)
/// - Wells: Y-axis `Cylinder` × count、X 方向等間隔、Y+ 上端開口
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{token_well, TokenWellSpec};
/// let t = token_well(&TokenWellSpec::dice_4());
/// ```
#[must_use]
pub fn token_well(spec: &TokenWellSpec) -> SdfNode {
    let count = spec.well_count.max(1);
    let count_f = count as f32;
    let pitch = spec.well_diameter + 2.0 * spec.well_clearance;
    let ext_x = count_f * pitch + 2.0 * spec.wall_thickness;
    let ext_z = spec.well_diameter + 2.0 * (spec.well_clearance + spec.wall_thickness);
    let ext_y = spec.well_depth + spec.floor_thickness;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    let well_r = spec.well_diameter * 0.5;
    let well_hy = (spec.well_depth + 1.0) * 0.5;
    let well_offset_y = spec.floor_thickness * 0.5 + 0.5;
    let x_start = -(count_f - 1.0) * pitch * 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, spec.wall_thickness);
    let mut result = outer;
    for i in 0..count {
        let x = x_start + i as f32 * pitch;
        let well = translate(cylinder(well_r, well_hy), Vec3::new(x, well_offset_y, 0.0));
        result = subtract(result, well);
    }

    to_z_up(result)
}

// ────────────────────────────────────────────────────────
// 20. wrench_holder (tools § 1 Wrench Organizer)
// ────────────────────────────────────────────────────────

/// レンチホルダー spec (row 状 slot、min〜max mm を count 等間隔で配置)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WrenchHolderSpec {
    /// 最小レンチ幅 (mm、default 8)
    pub min_size_mm: f32,
    /// 最大レンチ幅 (mm、default 19)
    pub max_size_mm: f32,
    /// slot 個数 (default 6、min-max を linear interpolate)
    pub count: u32,
    /// slot depth (mm、レンチ頭部を保持する深さ、default 22)
    pub slot_depth: f32,
    /// slot 両側 clearance (mm、default 0.6 = 全 clearance 1.2mm、tools § 1)
    pub slot_clearance: f32,
    /// slot 厚み係数 (× wrench size = 頭部厚 approx、tools § 1 ISO 10102 近似、default 0.5)
    pub thickness_ratio: f32,
    /// 外周 壁厚 (mm、default 3.0)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 4.0)
    pub floor_thickness: f32,
}

impl WrenchHolderSpec {
    /// Metric 6 slot 8-19mm (tools § 1 標準セット、8/10/12/14/16/19 相当)
    #[must_use]
    pub const fn metric_6_8to19() -> Self {
        Self {
            min_size_mm: 8.0,
            max_size_mm: 19.0,
            count: 6,
            slot_depth: 22.0,
            slot_clearance: 0.6,
            thickness_ratio: 0.5,
            wall_thickness: 3.0,
            floor_thickness: 4.0,
        }
    }
}

/// レンチホルダー (row 状 slot、top 開口、印刷 slot 上向き、`to_z_up` wrap)
///
/// 構造 (tools § 1 準拠、Y-up 設計):
/// - Outer: `RoundedBox` (`(count×pitch+2×wall) × (depth+floor) × (max_thickness+2×wall)`)
/// - Slots: `Box3d` × count、X 方向等間隔、size = min + i×(max-min)/(count-1)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{wrench_holder, WrenchHolderSpec};
/// let w = wrench_holder(&WrenchHolderSpec::metric_6_8to19());
/// ```
#[must_use]
pub fn wrench_holder(spec: &WrenchHolderSpec) -> SdfNode {
    let count = spec.count.max(1);
    let count_f = count as f32;
    let max_slot_w = spec.max_size_mm + 2.0 * spec.slot_clearance;
    let pitch = max_slot_w + 3.0; // 3mm inter-slot wall
    let max_thickness = spec.max_size_mm * spec.thickness_ratio + 2.0 * spec.slot_clearance;

    let ext_x = count_f * pitch + 2.0 * spec.wall_thickness;
    let ext_y = spec.slot_depth + spec.floor_thickness;
    let ext_z = max_thickness + 2.0 * spec.wall_thickness;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    let x_start = -(count_f - 1.0) * pitch * 0.5;
    let slot_hy = (spec.slot_depth + 1.0) * 0.5;
    let slot_offset_y = spec.floor_thickness * 0.5 + 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, spec.wall_thickness);
    let mut result = outer;
    for i in 0..count {
        let t = if count == 1 {
            0.0
        } else {
            i as f32 / (count_f - 1.0)
        };
        let size = spec.min_size_mm + t * (spec.max_size_mm - spec.min_size_mm);
        let slot_w = size + 2.0 * spec.slot_clearance;
        let slot_thick = size * spec.thickness_ratio + 2.0 * spec.slot_clearance;
        let x = x_start + i as f32 * pitch;
        let slot = translate(
            box3d(slot_w * 0.5, slot_hy, slot_thick * 0.5),
            Vec3::new(x, slot_offset_y, 0.0),
        );
        result = subtract(result, slot);
    }

    to_z_up(result)
}

// ────────────────────────────────────────────────────────
// 21. socket_rail (tools § 2 Socket Holder/Organizer)
// ────────────────────────────────────────────────────────

/// ソケットレール spec (base plate 上に post を row 配置、ソケット差し込み)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SocketRailSpec {
    /// post 直径 (mm、1/4"=6.0 / 3/8"=9.2 / 1/2"=12.4 / 3/4"=18.7、default 12.4)
    pub post_diameter: f32,
    /// post 高さ (mm、drive size に応じて 12-30、default 22)
    pub post_height: f32,
    /// post 個数 (row 方向、default 6)
    pub post_count: u32,
    /// post 間 pitch = `post_diameter + 本値` (mm、default 6.0)
    pub post_spacing: f32,
    /// base 厚 (mm、default 4.0)
    pub base_thickness: f32,
    /// base 周り 余白 (mm、default 3.0)
    pub base_margin: f32,
}

impl SocketRailSpec {
    /// 1/2" drive 6-post (tools § 2 中型セット、post_dia 12.4mm)
    #[must_use]
    pub const fn half_inch_6() -> Self {
        Self {
            post_diameter: 12.4,
            post_height: 22.0,
            post_count: 6,
            post_spacing: 6.0,
            base_thickness: 4.0,
            base_margin: 3.0,
        }
    }
}

/// ソケットレール (base plate 上に count post、印刷 post 上向き、`to_z_up` wrap)
///
/// 構造 (tools § 2 準拠、Y-up 設計):
/// - Base: `RoundedBox` (`(count×pitch+2×margin) × base_thickness × (dia+2×margin)`)
/// - Posts: Y-axis `Cylinder` × count、X 方向等間隔、base 上に union (0.1mm overlap)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{socket_rail, SocketRailSpec};
/// let s = socket_rail(&SocketRailSpec::half_inch_6());
/// ```
#[must_use]
pub fn socket_rail(spec: &SocketRailSpec) -> SdfNode {
    let count = spec.post_count.max(1);
    let count_f = count as f32;
    let pitch = spec.post_diameter + spec.post_spacing;
    let ext_x = count_f * pitch + 2.0 * spec.base_margin;
    let ext_y = spec.base_thickness;
    let ext_z = spec.post_diameter + 2.0 * spec.base_margin;

    let base_hx = ext_x * 0.5;
    let base_hy = ext_y * 0.5;
    let base_hz = ext_z * 0.5;

    let post_r = spec.post_diameter * 0.5;
    let post_hy = spec.post_height * 0.5;
    let post_offset_y = base_hy + post_hy - 0.1; // 0.1mm overlap with base for watertight union
    let x_start = -(count_f - 1.0) * pitch * 0.5;

    let base = rounded_box(base_hx, base_hy, base_hz, spec.base_margin);
    let mut result = base;
    for i in 0..count {
        let x = x_start + i as f32 * pitch;
        let post = translate(cylinder(post_r, post_hy), Vec3::new(x, post_offset_y, 0.0));
        result = union(result, post);
    }

    to_z_up(result)
}

// ────────────────────────────────────────────────────────
// 22. hex_bit_holder (tools § 3 Screwdriver/Bit Holder)
// ────────────────────────────────────────────────────────

/// ヘックスビットホルダー spec (grid 状 hex hole、1/4" bit 想定)
///
/// hex hole across-flats = 6.85mm 固定 (tools § 3 標準、`hex_r = 3.425`)
/// hex depth = 14mm 固定 (25mm bit の約半分、tools § 3)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HexBitHolderSpec {
    /// grid 行数 (Y 方向、default 5)
    pub rows: u32,
    /// grid 列数 (X 方向、default 4)
    pub cols: u32,
    /// hole 中心間 pitch (mm、default 12、min 10)
    pub spacing: f32,
    /// 外周 壁厚 (mm、default 2.0)
    pub wall_thickness: f32,
    /// 底厚 (mm、hole depth 下の material、default 2.0)
    pub floor_thickness: f32,
}

impl HexBitHolderSpec {
    /// 20 hole (4×5) 標準 (tools § 3 汎用ビットセット規模)
    #[must_use]
    pub const fn grid_4x5() -> Self {
        Self {
            rows: 5,
            cols: 4,
            spacing: 12.0,
            wall_thickness: 2.0,
            floor_thickness: 2.0,
        }
    }
}

/// hex hole across-flats (mm、1/4" bit 6.35 に FDM undersize 0.3-0.5 補償の 6.85)
const HEX_BIT_ACROSS_FLATS: f32 = 6.85;

/// hex hole depth (mm、25mm bit の約半分)
const HEX_BIT_HOLE_DEPTH: f32 = 14.0;

/// ヘックスビットホルダー (grid 状 hex hole、印刷 hole 上向き、Z-up 直接設計)
///
/// 構造 (tools § 3 準拠、Z-up 直接、`HexPrism` の Z-axis alignment を活用):
/// - Outer: `Box3d` (`(cols×spacing+2×wall) × (rows×spacing+2×wall) × (hole_depth+floor)`)
/// - Holes: `HexPrism` × (rows×cols)、Z+ 上端開口
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{hex_bit_holder, HexBitHolderSpec};
/// let h = hex_bit_holder(&HexBitHolderSpec::grid_4x5());
/// ```
#[must_use]
pub fn hex_bit_holder(spec: &HexBitHolderSpec) -> SdfNode {
    let rows = spec.rows.max(1);
    let cols = spec.cols.max(1);
    let rows_f = rows as f32;
    let cols_f = cols as f32;

    let ext_x = cols_f * spec.spacing + 2.0 * spec.wall_thickness;
    let ext_y = rows_f * spec.spacing + 2.0 * spec.wall_thickness;
    let ext_z = HEX_BIT_HOLE_DEPTH + spec.floor_thickness;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    let hex_r = HEX_BIT_ACROSS_FLATS * 0.5;
    let hex_half_h = (HEX_BIT_HOLE_DEPTH + 1.0) * 0.5;
    let hex_offset_z = outer_hz - hex_half_h + 0.5;
    let x_start = -(cols_f - 1.0) * spec.spacing * 0.5;
    let y_start = -(rows_f - 1.0) * spec.spacing * 0.5;

    let outer = box3d(outer_hx, outer_hy, outer_hz);
    let mut result = outer;
    for r in 0..rows {
        for c in 0..cols {
            let x = x_start + c as f32 * spec.spacing;
            let y = y_start + r as f32 * spec.spacing;
            let hex = translate(
                SdfNode::HexPrism {
                    hex_radius: hex_r,
                    half_height: hex_half_h,
                },
                Vec3::new(x, y, hex_offset_z),
            );
            result = subtract(result, hex);
        }
    }

    result
}

// ────────────────────────────────────────────────────────
// 23. raspi_case (electronics-enclosure § 1 Raspberry Pi Cases)
// ────────────────────────────────────────────────────────

/// Raspberry Pi ケース spec (4-side walls + 4 corner standoff pegs + port opening、top open)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaspiCaseSpec {
    /// PCB 幅 (mm、RPi 5/4=85 / Zero 2W=65、default 85)
    pub pcb_width: f32,
    /// PCB 奥行 (mm、RPi 5/4=56 / Zero 2W=30、default 56)
    pub pcb_depth: f32,
    /// PCB 上の内部高さ (mm、Active Cooler=25 / bare=15、default 25)
    pub internal_height: f32,
    /// 外周 壁厚 (mm、default 2.0)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 3.0)
    pub floor_thickness: f32,
    /// PCB↔壁 clearance/side (mm、default 1.0 = 全 2mm)
    pub pcb_clearance: f32,
    /// standoff peg 直径 (mm、default 6.0)
    pub standoff_diameter: f32,
    /// standoff peg 高さ (mm、PCB 底面 clearance、default 5.0)
    pub standoff_height: f32,
    /// M2.5 pilot 穴径 (mm、self-tap、default 2.2)
    pub standoff_pilot_diameter: f32,
    /// standoff mount hole 座標 inset (PCB corner から、mm、default 3.5)
    pub standoff_inset: f32,
    /// port opening 幅 (mm、long side 沿い、default 60、USB+HDMI+Ethernet 集約)
    pub port_opening_width: f32,
}

impl RaspiCaseSpec {
    /// RPi 5 with Active Cooler 85×56×25mm (electronics-enclosure § 1 default)
    #[must_use]
    pub const fn rpi5_active_cooler() -> Self {
        Self {
            pcb_width: 85.0,
            pcb_depth: 56.0,
            internal_height: 25.0,
            wall_thickness: 2.0,
            floor_thickness: 3.0,
            pcb_clearance: 1.0,
            standoff_diameter: 6.0,
            standoff_height: 5.0,
            standoff_pilot_diameter: 2.2,
            standoff_inset: 3.5,
            port_opening_width: 60.0,
        }
    }
}

/// Raspberry Pi ケース (top 開口、4 standoff peg、片側 port opening、`to_z_up` wrap)
///
/// 構造 (electronics-enclosure § 1 準拠、Y-up 設計):
/// - Outer: `RoundedBox` (`(pcb_w+2×(clear+wall)) × (int_h+floor) × (pcb_d+2×(clear+wall))`)
/// - Cavity: `Box3d` (`(pcb_w+2×clear) × (int_h+1) × (pcb_d+2×clear)`)、Y+ 開口
/// - Standoffs: 4× Y-axis `Cylinder` (r=`standoff_diameter/2`, h=`standoff_height`)、PCB corner に inset 位置
/// - Pilot holes: 4× Y-axis `Cylinder` (r=`pilot/2`)、standoff top から subtract
/// - Port opening: `Box3d` slot、-Z side wall (long side) 貫通
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{raspi_case, RaspiCaseSpec};
/// let c = raspi_case(&RaspiCaseSpec::rpi5_active_cooler());
/// ```
#[must_use]
pub fn raspi_case(spec: &RaspiCaseSpec) -> SdfNode {
    let ext_x = spec.pcb_width + 2.0 * (spec.pcb_clearance + spec.wall_thickness);
    let ext_z = spec.pcb_depth + 2.0 * (spec.pcb_clearance + spec.wall_thickness);
    let ext_y = spec.internal_height + spec.floor_thickness;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    let cavity_hx = (spec.pcb_width + 2.0 * spec.pcb_clearance) * 0.5;
    let cavity_hy = (spec.internal_height + 1.0) * 0.5;
    let cavity_hz = (spec.pcb_depth + 2.0 * spec.pcb_clearance) * 0.5;
    let cavity_offset_y = spec.floor_thickness * 0.5 + 0.5;

    let standoff_r = spec.standoff_diameter * 0.5;
    let standoff_hy = spec.standoff_height * 0.5;
    let standoff_offset_y = -outer_hy + spec.floor_thickness + standoff_hy;
    // Standoff 位置: PCB corner から standoff_inset だけ内側
    let sx = spec.pcb_width * 0.5 - spec.standoff_inset;
    let sz = spec.pcb_depth * 0.5 - spec.standoff_inset;

    let pilot_r = spec.standoff_pilot_diameter * 0.5;
    let pilot_hy = spec.standoff_height * 0.75;
    let pilot_offset_y = standoff_offset_y + standoff_hy - pilot_hy + 0.5;

    let port_hx = spec.port_opening_width * 0.5;
    let port_hy = spec.internal_height * 0.5;
    let port_hz = (spec.wall_thickness + 2.0) * 0.5;
    let port_offset_y = spec.floor_thickness * 0.5 + spec.pcb_clearance + port_hy * 0.5;
    let port_offset_z = -outer_hz + port_hz - 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, spec.wall_thickness);
    let cavity = translate(
        box3d(cavity_hx, cavity_hy, cavity_hz),
        Vec3::new(0.0, cavity_offset_y, 0.0),
    );
    let mut result = subtract(outer, cavity);

    // 4 corner standoffs
    for (dx, dz) in [(-sx, -sz), (sx, -sz), (-sx, sz), (sx, sz)] {
        let peg = translate(
            cylinder(standoff_r, standoff_hy),
            Vec3::new(dx, standoff_offset_y, dz),
        );
        result = union(result, peg);
        let pilot = translate(
            cylinder(pilot_r, pilot_hy),
            Vec3::new(dx, pilot_offset_y, dz),
        );
        result = subtract(result, pilot);
    }

    // Port opening (long side、-Z 面)
    let port = translate(
        box3d(port_hx, port_hy, port_hz),
        Vec3::new(0.0, port_offset_y, port_offset_z),
    );
    result = subtract(result, port);

    to_z_up(result)
}

// ────────────────────────────────────────────────────────
// 24. esp32_enclosure (electronics-enclosure § 2 ESP32/Arduino Cases)
// ────────────────────────────────────────────────────────

/// ESP32/Arduino ケース spec (standoff なし、friction cradle 想定、USB 短辺 opening)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Esp32EnclosureSpec {
    /// PCB 幅 (mm、ESP32 DevKit V1=51.6 / Arduino Uno=68.6、default 51.6)
    pub pcb_width: f32,
    /// PCB 奥行 (mm、ESP32=28.4 / Uno=53.4、default 28.4)
    pub pcb_depth: f32,
    /// PCB 上の内部高さ (mm、default 15、header 露出時 20)
    pub internal_height: f32,
    /// 外周 壁厚 (mm、default 1.6)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 2.0)
    pub floor_thickness: f32,
    /// PCB↔壁 clearance/side (mm、default 0.5)
    pub pcb_clearance: f32,
    /// USB opening 幅 (mm、Micro-USB=7.5 / USB-C=9 / USB-B=12、default 9)
    pub usb_opening_width: f32,
    /// USB opening 高 (mm、Micro=3 / C=3.5 / B=11、default 5)
    pub usb_opening_height: f32,
}

impl Esp32EnclosureSpec {
    /// ESP32 DevKit V1 51.6×28.4×15mm (electronics-enclosure § 2 default)
    #[must_use]
    pub const fn esp32_devkit_v1() -> Self {
        Self {
            pcb_width: 51.6,
            pcb_depth: 28.4,
            internal_height: 15.0,
            wall_thickness: 1.6,
            floor_thickness: 2.0,
            pcb_clearance: 0.5,
            usb_opening_width: 9.0,
            usb_opening_height: 5.0,
        }
    }
}

/// ESP32/Arduino ケース (top 開口、USB 短辺 opening、`to_z_up` wrap)
///
/// 構造 (electronics-enclosure § 2 準拠、Y-up 設計):
/// - Outer: `RoundedBox`
/// - Cavity: `Box3d` (Y+ 開口)
/// - USB opening: `Box3d` slot、+X or -X 短辺 貫通 (default +X)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{esp32_enclosure, Esp32EnclosureSpec};
/// let e = esp32_enclosure(&Esp32EnclosureSpec::esp32_devkit_v1());
/// ```
#[must_use]
pub fn esp32_enclosure(spec: &Esp32EnclosureSpec) -> SdfNode {
    let ext_x = spec.pcb_width + 2.0 * (spec.pcb_clearance + spec.wall_thickness);
    let ext_z = spec.pcb_depth + 2.0 * (spec.pcb_clearance + spec.wall_thickness);
    let ext_y = spec.internal_height + spec.floor_thickness;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    let cavity_hx = (spec.pcb_width + 2.0 * spec.pcb_clearance) * 0.5;
    let cavity_hy = (spec.internal_height + 1.0) * 0.5;
    let cavity_hz = (spec.pcb_depth + 2.0 * spec.pcb_clearance) * 0.5;
    let cavity_offset_y = spec.floor_thickness * 0.5 + 0.5;

    let usb_hx = (spec.wall_thickness + 2.0) * 0.5;
    let usb_hy = spec.usb_opening_height * 0.5;
    let usb_hz = spec.usb_opening_width * 0.5;
    let usb_offset_x = outer_hx - usb_hx + 0.5;
    let usb_offset_y = spec.floor_thickness * 0.5 + spec.pcb_clearance + usb_hy;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, spec.wall_thickness);
    let cavity = translate(
        box3d(cavity_hx, cavity_hy, cavity_hz),
        Vec3::new(0.0, cavity_offset_y, 0.0),
    );
    let usb = translate(
        box3d(usb_hx, usb_hy, usb_hz),
        Vec3::new(usb_offset_x, usb_offset_y, 0.0),
    );

    to_z_up(subtract(subtract(outer, cavity), usb))
}

// ────────────────────────────────────────────────────────
// 25. battery_18650_holder (electronics-enclosure § 3 18650 Battery Holder)
// ────────────────────────────────────────────────────────

/// 18650 リチウムイオン電池 ホルダー spec (row 状 cylindrical cavity)
///
/// Cell 直径 18.6mm × 長さ 68mm 固定 (Li-ion 標準 + FDM clearance)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Battery18650HolderSpec {
    /// cell 個数 (row 方向、default 4、range 1-10)
    pub cell_count: u32,
    /// inter-cell wall (mm、thermal safety、default 2.5)
    pub wall_thickness: f32,
    /// 端部 floor 厚 (mm、0 = 両端貫通 / >0 = 片端閉塞、default 0)
    pub floor_thickness: f32,
}

impl Battery18650HolderSpec {
    /// 4 cell × 2.5mm wall × through (電子工作用の row 4 pack)
    #[must_use]
    pub const fn row_4_through() -> Self {
        Self {
            cell_count: 4,
            wall_thickness: 2.5,
            floor_thickness: 0.0,
        }
    }
}

/// 18650 cell 直径 (mm、18.0 + 0.3 wrapper + 0.3 clearance/side)
const CELL_18650_DIAMETER: f32 = 18.6;

/// 18650 cell 長さ (mm、flat top 65 + 3 clearance)
const CELL_18650_LENGTH: f32 = 68.0;

/// 18650 バッテリーホルダー (row 状 cylindrical cavity、`to_z_up` wrap)
///
/// 構造 (electronics-enclosure § 3 準拠、Y-up 設計):
/// - Outer: `RoundedBox`
/// - Cavities: N× Y-axis `Cylinder` (r=`CELL/2`, h=`(CELL_LEN+1)/2`)、X 方向等間隔
/// - `floor_thickness = 0` なら両端貫通 (cell 挿入両端 open)
/// - `floor_thickness > 0` なら片端閉塞 (positive terminal spring 保持用)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{battery_18650_holder, Battery18650HolderSpec};
/// let b = battery_18650_holder(&Battery18650HolderSpec::row_4_through());
/// ```
#[must_use]
pub fn battery_18650_holder(spec: &Battery18650HolderSpec) -> SdfNode {
    let count = spec.cell_count.max(1);
    let count_f = count as f32;
    let pitch = CELL_18650_DIAMETER + spec.wall_thickness;
    let ext_x = count_f * pitch + spec.wall_thickness;
    let ext_y = CELL_18650_LENGTH + 2.0 * spec.floor_thickness;
    let ext_z = CELL_18650_DIAMETER + 2.0 * spec.wall_thickness;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    let cell_r = CELL_18650_DIAMETER * 0.5;
    // cavity length: floor=0 → 貫通 (h > outer_hy)、floor>0 → 内側 CELL_LEN 範囲のみ
    let cell_hy = (CELL_18650_LENGTH + 1.0) * 0.5;
    let x_start = -(count_f - 1.0) * pitch * 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, 2.0);
    let mut result = outer;
    for i in 0..count {
        let x = x_start + i as f32 * pitch;
        let cavity = translate(cylinder(cell_r, cell_hy), Vec3::new(x, 0.0, 0.0));
        result = subtract(result, cavity);
    }

    to_z_up(result)
}

// ────────────────────────────────────────────────────────
// 26. toothbrush_holder (organizer-bathroom-garage § 7.1 Toothbrush Holder)
// ────────────────────────────────────────────────────────

/// 歯ブラシホルダー spec (row 状 cylindrical hole、top 開口)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToothbrushHolderSpec {
    /// hole 個数 (default 4)
    pub count: u32,
    /// hole 直径 (mm、manual=15 / electric=40、default 15)
    pub hole_diameter: f32,
    /// hole 深さ = 全体 height (mm、default 70)
    pub hole_depth: f32,
    /// hole 間 wall 厚 (mm、default 6.0)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 4.0、drainage 想定で少し厚め)
    pub floor_thickness: f32,
}

impl ToothbrushHolderSpec {
    /// 4 hole × Ø15 × H70mm (manual toothbrush 4 本用、bathroom § 7.1 default)
    #[must_use]
    pub const fn manual_4() -> Self {
        Self {
            count: 4,
            hole_diameter: 15.0,
            hole_depth: 70.0,
            wall_thickness: 6.0,
            floor_thickness: 4.0,
        }
    }
}

/// 歯ブラシホルダー (row 状 cylindrical hole、top 開口、`to_z_up` wrap)
///
/// 構造 (bathroom § 7.1 準拠、Y-up 設計):
/// - Outer: `RoundedBox`
/// - Holes: N× Y-axis `Cylinder` (r=`dia/2`, h=`(depth+1)/2`)、X 方向等間隔
/// - top 開口 (Y+)、floor 部分は material (drainage 穴は user 側で加工推奨)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{toothbrush_holder, ToothbrushHolderSpec};
/// let t = toothbrush_holder(&ToothbrushHolderSpec::manual_4());
/// ```
#[must_use]
pub fn toothbrush_holder(spec: &ToothbrushHolderSpec) -> SdfNode {
    let count = spec.count.max(1);
    let count_f = count as f32;
    let pitch = spec.hole_diameter + spec.wall_thickness;
    let ext_x = count_f * pitch + spec.wall_thickness;
    let ext_y = spec.hole_depth + spec.floor_thickness;
    let ext_z = spec.hole_diameter + 2.0 * spec.wall_thickness;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    let hole_r = spec.hole_diameter * 0.5;
    let hole_hy = (spec.hole_depth + 1.0) * 0.5;
    let hole_offset_y = spec.floor_thickness * 0.5 + 0.5;
    let x_start = -(count_f - 1.0) * pitch * 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, 2.0);
    let mut result = outer;
    for i in 0..count {
        let x = x_start + i as f32 * pitch;
        let hole = translate(cylinder(hole_r, hole_hy), Vec3::new(x, hole_offset_y, 0.0));
        result = subtract(result, hole);
    }

    to_z_up(result)
}

// ────────────────────────────────────────────────────────
// 27. drill_bit_holder (organizer-bathroom-garage § 8.1 Drill Bit Holder)
// ────────────────────────────────────────────────────────

/// ドリルビットホルダー spec (row 状 hole、min-max mm を count 等間隔補間)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrillBitHolderSpec {
    /// 最小ビット径 (mm、default 3.0)
    pub min_size_mm: f32,
    /// 最大ビット径 (mm、default 13.0)
    pub max_size_mm: f32,
    /// hole 個数 (default 11、Metric 3-13mm 1mm step)
    pub count: u32,
    /// hole 深さ (mm、default 22)
    pub hole_depth: f32,
    /// 片側 clearance (mm、default 0.25 = 全 0.5mm)
    pub hole_clearance: f32,
    /// 外周 壁厚 (mm、default 3.0)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 3.0)
    pub floor_thickness: f32,
}

impl DrillBitHolderSpec {
    /// Metric 11 hole 3-13mm (garage § 8.1 標準セット、1mm step 相当)
    #[must_use]
    pub const fn metric_11_3to13() -> Self {
        Self {
            min_size_mm: 3.0,
            max_size_mm: 13.0,
            count: 11,
            hole_depth: 22.0,
            hole_clearance: 0.25,
            wall_thickness: 3.0,
            floor_thickness: 3.0,
        }
    }
}

/// ドリルビットホルダー (row 状 hole、size linear interpolate、`to_z_up` wrap)
///
/// 構造 (garage § 8.1 準拠、Y-up 設計、wrench_holder 類似だが hole 円形):
/// - Outer: `RoundedBox` (`(count×pitch+2×wall) × (depth+floor) × (max_dia+2×wall)`)
/// - Holes: N× Y-axis `Cylinder`、size = `min + i×(max-min)/(count-1)` + clearance
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{drill_bit_holder, DrillBitHolderSpec};
/// let d = drill_bit_holder(&DrillBitHolderSpec::metric_11_3to13());
/// ```
#[must_use]
pub fn drill_bit_holder(spec: &DrillBitHolderSpec) -> SdfNode {
    let count = spec.count.max(1);
    let count_f = count as f32;
    let max_hole_dia = spec.max_size_mm + 2.0 * spec.hole_clearance;
    let pitch = max_hole_dia + 4.0; // 4mm inter-hole wall

    let ext_x = count_f * pitch + 2.0 * spec.wall_thickness;
    let ext_y = spec.hole_depth + spec.floor_thickness;
    let ext_z = max_hole_dia + 2.0 * spec.wall_thickness;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    let x_start = -(count_f - 1.0) * pitch * 0.5;
    let hole_hy = (spec.hole_depth + 1.0) * 0.5;
    let hole_offset_y = spec.floor_thickness * 0.5 + 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, spec.wall_thickness);
    let mut result = outer;
    for i in 0..count {
        let t = if count == 1 {
            0.0
        } else {
            i as f32 / (count_f - 1.0)
        };
        let size = spec.min_size_mm + t * (spec.max_size_mm - spec.min_size_mm);
        let hole_r = (size + 2.0 * spec.hole_clearance) * 0.5;
        let x = x_start + i as f32 * pitch;
        let hole = translate(cylinder(hole_r, hole_hy), Vec3::new(x, hole_offset_y, 0.0));
        result = subtract(result, hole);
    }

    to_z_up(result)
}

// ────────────────────────────────────────────────────────
// 28. pliers_rack (organizer-bathroom-garage § 8.4 Pliers Rack)
// ────────────────────────────────────────────────────────

/// プライヤーラック spec (row 状 rectangular slot、pliers 挿入)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PliersRackSpec {
    /// slot 個数 (default 6)
    pub slot_count: u32,
    /// slot 幅 (mm、needle-nose=10 / combi=15 / tongue-groove=20-25、default 15)
    pub slot_width: f32,
    /// slot 深さ (mm、handle 挿入深さ、default 60、range 40-80)
    pub slot_depth: f32,
    /// slot 高さ = rack 厚 (mm、default 35)
    pub slot_height: f32,
    /// slot 間 wall 厚 (mm、default 5.0)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 5.0)
    pub floor_thickness: f32,
}

impl PliersRackSpec {
    /// 6 slot × W15 × D60mm (garage § 8.4 標準セット、combination pliers 6 本)
    #[must_use]
    pub const fn standard_6() -> Self {
        Self {
            slot_count: 6,
            slot_width: 15.0,
            slot_depth: 60.0,
            slot_height: 35.0,
            wall_thickness: 5.0,
            floor_thickness: 5.0,
        }
    }
}

/// プライヤーラック (row 状 rectangular slot、top 開口、`to_z_up` wrap)
///
/// 構造 (garage § 8.4 準拠、Y-up 設計):
/// - Outer: `RoundedBox` (`(count×pitch+2×wall) × (depth+floor) × slot_height`)
/// - Slots: N× `Box3d` slot、X 方向等間隔、Y+ 開口
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{pliers_rack, PliersRackSpec};
/// let p = pliers_rack(&PliersRackSpec::standard_6());
/// ```
#[must_use]
pub fn pliers_rack(spec: &PliersRackSpec) -> SdfNode {
    let count = spec.slot_count.max(1);
    let count_f = count as f32;
    let pitch = spec.slot_width + spec.wall_thickness;
    let ext_x = count_f * pitch + spec.wall_thickness;
    let ext_y = spec.slot_depth + spec.floor_thickness;
    let ext_z = spec.slot_height;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    let slot_hy = (spec.slot_depth + 1.0) * 0.5;
    let slot_offset_y = spec.floor_thickness * 0.5 + 0.5;
    let x_start = -(count_f - 1.0) * pitch * 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, 2.0);
    let mut result = outer;
    for i in 0..count {
        let x = x_start + i as f32 * pitch;
        let slot = translate(
            box3d(spec.slot_width * 0.5, slot_hy, outer_hz + 1.0),
            Vec3::new(x, slot_offset_y, 0.0),
        );
        result = subtract(result, slot);
    }

    to_z_up(result)
}

// ────────────────────────────────────────────────────────
// 29. spice_rack (organizer-cable-kitchen § 6.1 Spice Rack)
// ────────────────────────────────────────────────────────

/// スパイスラック spec (薄 shelf + jar 用 shallow recess + 前縁 lip)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpiceRackSpec {
    /// jar 個数 (default 6、range 3-12)
    pub count: u32,
    /// jar 直径 (mm、small=42 / std=48 / large=52、default 48)
    pub jar_diameter: f32,
    /// jar 高さ (mm、default 100、lip_height 計算に使う)
    pub jar_height: f32,
    /// jar recess 深さ (mm、default 5.0)
    pub recess_depth: f32,
    /// jar 前後余白 (mm、front margin + back margin、default 15.0)
    pub shelf_depth_margin: f32,
    /// base 厚 (mm、default 5.0)
    pub base_thickness: f32,
    /// 外周 壁厚 (mm、default 3.0)
    pub wall_thickness: f32,
    /// 前縁 lip 高さ係数 (× jar_height = lip_height、default 0.15)
    pub lip_height_ratio: f32,
    /// 前縁 lip 厚 (mm、default 3.0)
    pub lip_thickness: f32,
}

impl SpiceRackSpec {
    /// 6 jars × Ø48 × H100mm (standard spice jar 6 本、kitchen § 6.1 default)
    #[must_use]
    pub const fn standard_6() -> Self {
        Self {
            count: 6,
            jar_diameter: 48.0,
            jar_height: 100.0,
            recess_depth: 5.0,
            shelf_depth_margin: 15.0,
            base_thickness: 5.0,
            wall_thickness: 3.0,
            lip_height_ratio: 0.15,
            lip_thickness: 3.0,
        }
    }
}

/// スパイスラック (薄 shelf + jar recess + 前縁 lip、`to_z_up` wrap)
///
/// 構造 (kitchen § 6.1 準拠、Y-up 設計):
/// - Base: `RoundedBox` (`(count×pitch+2×wall) × base_thickness × (jar_dia+margin)`)
/// - Recesses: N× Y-axis `Cylinder` (r=`jar_dia/2 + 2mm`, depth=`recess_depth`)、上面 subtract
/// - Front lip: `Box3d` union、-Z edge (前縁)、`lip_height = jar_height × ratio`
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{spice_rack, SpiceRackSpec};
/// let s = spice_rack(&SpiceRackSpec::standard_6());
/// ```
#[must_use]
pub fn spice_rack(spec: &SpiceRackSpec) -> SdfNode {
    let count = spec.count.max(1);
    let count_f = count as f32;
    let jar_clearance = 2.0; // 片側 2mm 余裕 (spec: +3-5mm 全径、+1.5-2.5mm 片側)
    let pitch = spec.jar_diameter + jar_clearance * 2.0 + spec.wall_thickness;
    let shelf_depth = spec.jar_diameter + spec.shelf_depth_margin;

    let lip_height = spec.jar_height * spec.lip_height_ratio;
    let ext_x = count_f * pitch + spec.wall_thickness;
    let ext_y = spec.base_thickness + lip_height;
    let ext_z = shelf_depth + 2.0 * spec.wall_thickness;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    // Base plate (Y bottom half)、lip 部分は base より Y+ 方向に飛び出す構造
    let base_hy = spec.base_thickness * 0.5;
    let base_offset_y = -outer_hy + base_hy;

    let base = translate(
        rounded_box(outer_hx, base_hy, outer_hz, 2.0),
        Vec3::new(0.0, base_offset_y, 0.0),
    );

    // Front lip: -Z 側の壁 (前縁)
    let lip_hz = spec.lip_thickness * 0.5;
    let lip_hy = lip_height * 0.5;
    let lip_offset_y = -outer_hy + spec.base_thickness + lip_hy;
    let lip_offset_z = -outer_hz + lip_hz;
    let lip = translate(
        box3d(outer_hx, lip_hy, lip_hz),
        Vec3::new(0.0, lip_offset_y, lip_offset_z),
    );

    let mut result = union(base, lip);

    // jar recess: base 上面 (Y+) から下向きに subtract
    let recess_r = spec.jar_diameter * 0.5 + jar_clearance;
    let recess_hy = (spec.recess_depth + 0.5) * 0.5;
    let recess_offset_y = -outer_hy + spec.base_thickness - recess_hy + 0.25;
    let x_start = -(count_f - 1.0) * pitch * 0.5;
    for i in 0..count {
        let x = x_start + i as f32 * pitch;
        let recess = translate(
            cylinder(recess_r, recess_hy),
            Vec3::new(x, recess_offset_y, 0.0),
        );
        result = subtract(result, recess);
    }

    to_z_up(result)
}

// ────────────────────────────────────────────────────────
// 30. egg_tray (organizer-cable-kitchen § 6.5 Fridge Organizer / Egg)
// ────────────────────────────────────────────────────────

/// 卵トレー spec (2D grid 状 cup、egg diameter 40mm 固定)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EggTraySpec {
    /// 行数 (Z 方向、default 3、range 1-8)
    pub rows: u32,
    /// 列数 (X 方向、default 4、range 1-8)
    pub cols: u32,
    /// cup depth (mm、default 18、range 12-25、egg が 2/3 見える程度)
    pub cup_depth: f32,
    /// egg pitch (cup 中心間、mm、default 50 = spec 45mm + 5mm wall)
    pub pitch: f32,
    /// 外周 壁厚 (mm、default 3.0)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 3.0)
    pub floor_thickness: f32,
}

impl EggTraySpec {
    /// 4×3 grid × 深 18mm (12-egg tray standard、kitchen § 6.5 default)
    #[must_use]
    pub const fn tray_4x3() -> Self {
        Self {
            rows: 3,
            cols: 4,
            cup_depth: 18.0,
            pitch: 50.0,
            wall_thickness: 3.0,
            floor_thickness: 3.0,
        }
    }
}

/// egg cup diameter (mm、large egg spec 44-45 + 2mm clearance → 40mm recess で 2/3 保持)
const EGG_CUP_DIAMETER: f32 = 40.0;

/// 卵トレー (2D grid 状 cup、egg cup diameter 40mm 固定、`to_z_up` wrap)
///
/// 構造 (kitchen § 6.5 準拠、Y-up 設計、hex_bit_holder の 2D grid pattern の cyl 版):
/// - Outer: `RoundedBox` (`(cols×pitch+2×wall) × (cup_depth+floor) × (rows×pitch+2×wall)`)
/// - Cups: (rows×cols)× Y-axis `Cylinder` (r=`20mm`, h=`cup_depth+1`)、grid 配置
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{egg_tray, EggTraySpec};
/// let e = egg_tray(&EggTraySpec::tray_4x3());
/// ```
#[must_use]
pub fn egg_tray(spec: &EggTraySpec) -> SdfNode {
    let rows = spec.rows.max(1);
    let cols = spec.cols.max(1);
    let rows_f = rows as f32;
    let cols_f = cols as f32;

    let ext_x = cols_f * spec.pitch + 2.0 * spec.wall_thickness;
    let ext_y = spec.cup_depth + spec.floor_thickness;
    let ext_z = rows_f * spec.pitch + 2.0 * spec.wall_thickness;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    let cup_r = EGG_CUP_DIAMETER * 0.5;
    let cup_hy = (spec.cup_depth + 1.0) * 0.5;
    let cup_offset_y = spec.floor_thickness * 0.5 + 0.5;
    let x_start = -(cols_f - 1.0) * spec.pitch * 0.5;
    let z_start = -(rows_f - 1.0) * spec.pitch * 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, 3.0);
    let mut result = outer;
    for r in 0..rows {
        for c in 0..cols {
            let x = x_start + c as f32 * spec.pitch;
            let z = z_start + r as f32 * spec.pitch;
            let cup = translate(cylinder(cup_r, cup_hy), Vec3::new(x, cup_offset_y, z));
            result = subtract(result, cup);
        }
    }

    to_z_up(result)
}

// ────────────────────────────────────────────────────────
// 31. utensil_caddy (organizer-cable-kitchen § 6.8 Utensil Caddy)
// ────────────────────────────────────────────────────────

/// キッチンツールキャディ spec (row 状 large cylindrical compartment)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UtensilCaddySpec {
    /// compartment 個数 (default 4、range 1-6)
    pub count: u32,
    /// compartment 内径 (mm、small=45-50 / large=60-70、default 65)
    pub compartment_diameter: f32,
    /// compartment 高さ (mm、default 130、utensil 長 300mm の 1/3-1/2)
    pub height: f32,
    /// compartment 間 wall 厚 (mm、default 5.0)
    pub wall_thickness: f32,
    /// 底厚 (mm、default 4.0)
    pub floor_thickness: f32,
}

impl UtensilCaddySpec {
    /// 4 compartment × Ø65 × H130mm (spatula/ladle/whisk/tongs 分別、kitchen § 6.8 default)
    #[must_use]
    pub const fn standard_4() -> Self {
        Self {
            count: 4,
            compartment_diameter: 65.0,
            height: 130.0,
            wall_thickness: 5.0,
            floor_thickness: 4.0,
        }
    }
}

/// キッチンツールキャディ (row 状 large cylindrical compartment、top 開口、`to_z_up` wrap)
///
/// 構造 (kitchen § 6.8 準拠、Y-up 設計、toothbrush_holder pattern の大径 kitchen version):
/// - Outer: `RoundedBox`
/// - Compartments: N× Y-axis `Cylinder`、X 方向等間隔、Y+ 開口
/// - Drainage holes は user 側で加工推奨 (sink 側で使う場合)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::hardsurface::pattern_sdf::{utensil_caddy, UtensilCaddySpec};
/// let u = utensil_caddy(&UtensilCaddySpec::standard_4());
/// ```
#[must_use]
pub fn utensil_caddy(spec: &UtensilCaddySpec) -> SdfNode {
    let count = spec.count.max(1);
    let count_f = count as f32;
    let pitch = spec.compartment_diameter + spec.wall_thickness;
    let ext_x = count_f * pitch + spec.wall_thickness;
    let ext_y = spec.height + spec.floor_thickness;
    let ext_z = spec.compartment_diameter + 2.0 * spec.wall_thickness;

    let outer_hx = ext_x * 0.5;
    let outer_hy = ext_y * 0.5;
    let outer_hz = ext_z * 0.5;

    let comp_r = spec.compartment_diameter * 0.5;
    let comp_hy = (spec.height + 1.0) * 0.5;
    let comp_offset_y = spec.floor_thickness * 0.5 + 0.5;
    let x_start = -(count_f - 1.0) * pitch * 0.5;

    let outer = rounded_box(outer_hx, outer_hy, outer_hz, 3.0);
    let mut result = outer;
    for i in 0..count {
        let x = x_start + i as f32 * pitch;
        let compartment = translate(cylinder(comp_r, comp_hy), Vec3::new(x, comp_offset_y, 0.0));
        result = subtract(result, compartment);
    }

    to_z_up(result)
}

// ────────────────────────────────────────────────────────
// テスト
// ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use alice_sdf::eval;

    #[test]
    fn wall_hook_default_produces_smooth_union_tree() {
        let hook = wall_hook(&WallHookSpec::pla_1kgf());
        // mount_hole があるので最終 node は Subtraction
        assert!(matches!(hook, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn wall_hook_no_mount_holes() {
        let mut spec = WallHookSpec::pla_1kgf();
        spec.mount_hole_dia = None;
        let hook = wall_hook(&spec);
        // mount_hole なしなので最終 node は SmoothUnion
        assert!(matches!(hook, SdfNode::SmoothUnion { .. }));
    }

    #[test]
    fn wall_hook_backplate_center_is_inside() {
        let hook = wall_hook(&WallHookSpec::pla_1kgf());
        // backplate 中心 (0, 0, 0) 付近は材料内部 (mount hole 外の位置で判定)
        // hole spacing = bp_hh × 0.3、mount holes は Y=±hole_spacing
        // Y=0 は 2 hole の間 → 材料内、Z=0 は backplate 中心
        assert!(eval(&hook, Vec3::new(0.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn gridfinity_2x2_returns_subtraction() {
        let bin = gridfinity_bin(&GridfinitySpec::default_2x2());
        assert!(matches!(bin, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn gridfinity_dimensions_match_grid_unit_spec() {
        let bin = gridfinity_bin(&GridfinitySpec::default_2x2());
        // 外形 X = 2×42 - 2×0.25 = 83.5mm、hx = 41.75
        // hz = (4×7 + 4.75)/2 = 16.375mm、底厚 1.5mm、cavity Z 中心 = +0.75、range Z=[+0.75-cavity_hz, +0.75+cavity_hz]
        // Z=-16 付近 (bin 底) は floor 材料内
        assert!(eval(&bin, Vec3::new(0.0, 0.0, -16.0)) < 0.0);
        // 外形外 (X=50) は空間
        assert!(eval(&bin, Vec3::new(50.0, 0.0, 0.0)) > 0.0);
    }

    #[test]
    fn drawer_default_chopsticks_set_is_subtraction() {
        let tray = drawer_organizer(&DrawerSpec::default_chopsticks_set());
        assert!(matches!(tray, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn drawer_empty_slots_returns_solid_tray() {
        let spec = DrawerSpec {
            width: 100.0,
            depth: 80.0,
            height: 20.0,
            slots: Vec::new(),
            wall_thickness: 1.5,
            floor_thickness: 1.5,
            divider_thickness: 1.2,
            fillet_radius: 1.0,
        };
        let tray = drawer_organizer(&spec);
        // slots 空 → cavities None → outer RoundedBox のまま
        assert!(matches!(tray, SdfNode::RoundedBox { .. }));
    }

    #[test]
    fn shelf_divider_field_tested_spec_is_subtraction() {
        let s = shelf_divider(&ShelfDividerSpec::field_tested_560x250x120());
        // hex holes subtracted なので Subtraction
        assert!(matches!(s, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn shelf_divider_plate_center_is_inside() {
        let s = shelf_divider(&ShelfDividerSpec::field_tested_560x250x120());
        // plate 中央 (X 中央、Y 中央、Z=+2.5=plate 中心) は材料
        // ただし hex hole がある位置は空間、中央 X=0 Y=0 は千鳥 grid1 の中心なので hex 内
        // grid1 中心の外側 (X=5, Y=5) で判定
        assert!(
            eval(&s, Vec3::new(5.0, 5.0, 2.5)) < 0.0 || eval(&s, Vec3::new(10.0, 5.0, 2.5)) < 0.0
        );
    }

    #[test]
    fn all_pattern_sdf_evaluations_finite() {
        let nodes = [
            wall_hook(&WallHookSpec::pla_1kgf()),
            gridfinity_bin(&GridfinitySpec::default_2x2()),
            drawer_organizer(&DrawerSpec::default_chopsticks_set()),
            shelf_divider(&ShelfDividerSpec::field_tested_560x250x120()),
        ];
        for (i, node) in nodes.iter().enumerate() {
            let d = eval(node, Vec3::new(0.1, 0.1, 0.1));
            assert!(d.is_finite(), "pattern {i} produced non-finite SDF: {d}");
        }
    }

    // ── organizer-gridfinity-desk PART 2 archetypes (Phase B) ──

    #[test]
    fn sticky_note_holder_small_square_is_subtraction() {
        let h = sticky_note_holder(&StickyNoteHolderSpec::small_square());
        assert!(matches!(h, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn sticky_note_holder_floor_center_is_inside() {
        let h = sticky_note_holder(&StickyNoteHolderSpec::small_square());
        // floor 中央 (0, 0, +0.5 = floor 中心付近) は材料内部
        assert!(eval(&h, Vec3::new(0.0, 0.0, -14.0)) < 0.0);
    }

    #[test]
    fn business_card_holder_jp_meishi_is_subtraction() {
        let h = business_card_holder(&BusinessCardHolderSpec::jp_meishi());
        assert!(matches!(h, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn business_card_holder_wall_is_solid() {
        let h = business_card_holder(&BusinessCardHolderSpec::jp_meishi());
        // 側壁 (X=46、slot cavity 外) は材料
        // inner_hx=45.5、outer_hx=47.0 なので wall は X=45.5 to 47.0
        assert!(eval(&h, Vec3::new(46.0, 0.0, 5.0)) < 0.0);
    }

    #[test]
    fn pen_cup_standard_is_subtraction() {
        let cup = pen_cup(&PenCupSpec::standard_75x100());
        assert!(matches!(cup, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn pen_cup_wall_is_solid() {
        let cup = pen_cup(&PenCupSpec::standard_75x100());
        // Z-axis alignment (cylinder_z で 90° rotate、cup 軸 = Z、radial = XY 平面)
        // outer_r=39.5、inner_r=37.5、Z=0 中央 → 壁は radial 37.5 < sqrt(x²+y²) < 39.5
        assert!(eval(&cup, Vec3::new(38.5, 0.0, 0.0)) < 0.0);
        // cup 底 (Z=-49、Z-axis 底、cavity は Z=+1 offset なので Z=-49 は材料)
        assert!(eval(&cup, Vec3::new(0.0, 0.0, -49.0)) < 0.0);
    }

    #[test]
    fn phone_stand_default_has_cable_hole() {
        let s = phone_stand(&PhoneStandSpec::phone_default());
        // Some(cable_hole) 指定なので最終 node は Subtraction (hole subtract)
        assert!(matches!(s, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn phone_stand_no_cable_hole_is_slot_subtract() {
        let mut spec = PhoneStandSpec::phone_default();
        spec.cable_hole_dia = None;
        let s = phone_stand(&spec);
        // cable_hole なしなので slot subtract で止まる = Subtraction
        assert!(matches!(s, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn all_part2_archetypes_evaluations_finite() {
        let nodes = [
            sticky_note_holder(&StickyNoteHolderSpec::small_square()),
            business_card_holder(&BusinessCardHolderSpec::jp_meishi()),
            pen_cup(&PenCupSpec::standard_75x100()),
            phone_stand(&PhoneStandSpec::phone_default()),
        ];
        for (i, node) in nodes.iter().enumerate() {
            let d = eval(node, Vec3::new(0.1, 0.1, 0.1));
            assert!(
                d.is_finite(),
                "PART 2 archetype {i} produced non-finite SDF: {d}"
            );
        }
    }

    // ── organizer-gridfinity-desk PART 2 完成 4 archetype (Phase B2) ──

    // 2026-08-20 (v2): archetype 別 print-optimal 方針
    // - headphone_holder / under_desk_mount: unwrap (元 Z-up で print-optimal)
    // - desk_shelf / tissue_box_cover: to_z_up_flipped (intended-top を bed に = upside-down 印刷)
    // - storage_box: to_z_up 維持 (intended-bottom を bed に = 正立印刷)

    #[test]
    fn headphone_holder_default_is_subtraction() {
        let h = headphone_holder(&HeadphoneHolderSpec::wall_mount_default());
        // unwrap 済み、mount_hole あり → Subtraction (mount plate flat + arm 上向き 印刷)
        assert!(matches!(h, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn headphone_holder_no_holes_is_smooth_union() {
        let mut spec = HeadphoneHolderSpec::wall_mount_default();
        spec.mount_hole_dia = None;
        let h = headphone_holder(&spec);
        // unwrap 済み、mount_hole なし → SmoothUnion
        assert!(matches!(h, SdfNode::SmoothUnion { .. }));
    }

    #[test]
    fn under_desk_mount_default_is_subtraction() {
        let m = under_desk_mount(&UnderDeskMountSpec::standard_desk());
        // unwrap 済み、screw_hole あり → Subtraction (back stem flat + jaws 上向き `|_|`)
        assert!(matches!(m, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn under_desk_mount_no_screw_is_smooth_union() {
        let mut spec = UnderDeskMountSpec::standard_desk();
        spec.screw_hole_dia = None;
        let m = under_desk_mount(&spec);
        // unwrap 済み、screw なし → SmoothUnion (両面テープ想定)
        assert!(matches!(m, SdfNode::SmoothUnion { .. }));
    }

    #[test]
    fn desk_shelf_default_is_rotate_flipped() {
        let s = desk_shelf(&DeskShelfSpec::desktop_400x200());
        // to_z_up_flipped wrap で top-level は Rotate (shelf 下 / legs 上 印刷)
        assert!(matches!(s, SdfNode::Rotate { .. }));
    }

    #[test]
    fn desk_shelf_leg_position_is_inside() {
        let s = desk_shelf(&DeskShelfSpec::desktop_400x200());
        // to_z_up_flipped (rot_x -π/2): 内部 (X, Y, Z) → 世界 (X, Z, -Y)
        // 内部脚位置 (-190, 50, 0) → 世界 (-190, 0, -50)
        assert!(eval(&s, Vec3::new(-190.0, 0.0, -50.0)) < 0.0);
    }

    #[test]
    fn monitor_riser_default_has_cable_hole() {
        let r = monitor_riser(&MonitorRiserSpec::compact_desk());
        // cable_hole 指定なので Subtraction
        assert!(matches!(r, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn monitor_riser_no_cable_is_smooth_union() {
        let mut spec = MonitorRiserSpec::compact_desk();
        spec.cable_hole_dia = None;
        let r = monitor_riser(&spec);
        // cable_hole なしなら SmoothUnion
        assert!(matches!(r, SdfNode::SmoothUnion { .. }));
    }

    #[test]
    fn all_part2_b2_archetypes_evaluations_finite() {
        let nodes = [
            headphone_holder(&HeadphoneHolderSpec::wall_mount_default()),
            under_desk_mount(&UnderDeskMountSpec::standard_desk()),
            desk_shelf(&DeskShelfSpec::desktop_400x200()),
            monitor_riser(&MonitorRiserSpec::compact_desk()),
        ];
        for (i, node) in nodes.iter().enumerate() {
            let d = eval(node, Vec3::new(0.1, 0.1, 0.1));
            assert!(
                d.is_finite(),
                "PART 2 B2 archetype {i} produced non-finite SDF: {d}"
            );
        }
    }

    // ── household.md 3 archetype (Sprint 4) ──

    #[test]
    fn coaster_default_is_subtraction() {
        let c = coaster(&CoasterSpec::round_95x5());
        // recess subtract で Subtraction
        assert!(matches!(c, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn coaster_rim_is_solid() {
        let c = coaster(&CoasterSpec::round_95x5());
        // rim (X=46、outer_r=47.5、inner_r=45) 内は材料
        // Cylinder Y-axis なので radial 判定は (X, Z) plane、Y=0 が厚さ中央
        assert!(eval(&c, Vec3::new(46.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn coaster_bottom_is_solid() {
        let c = coaster(&CoasterSpec::round_95x5());
        // Y=-2 (底面付近、Y-axis で outer_hy=2.5 の底寄り) は材料
        assert!(eval(&c, Vec3::new(0.0, -2.0, 0.0)) < 0.0);
    }

    #[test]
    fn tissue_box_cover_default_is_rotate_flipped() {
        let t = tissue_box_cover(&TissueBoxCoverSpec::rectangular_us());
        // to_z_up_flipped wrap で top-level は Rotate (slot 下 印刷 upside-down)
        assert!(matches!(t, SdfNode::Rotate { .. }));
    }

    #[test]
    fn tissue_box_cover_top_wall_at_slot_center_is_hollow() {
        let t = tissue_box_cover(&TissueBoxCoverSpec::rectangular_us());
        // to_z_up_flipped: 内部 (X, Y, Z) → 世界 (X, Z, -Y)
        // 内部 top slot 中央 (0, 27.5, 0) → 世界 (0, 0, -27.5)
        // slot が bed 側 (-Z) にある = 印刷 upside-down 姿勢
        assert!(eval(&t, Vec3::new(0.0, 0.0, -27.5)) > 0.0);
    }

    #[test]
    fn storage_box_default_is_rotate_wrapped() {
        let b = storage_box(&StorageBoxSpec::medium());
        // to_z_up wrap で top-level は Rotate、内部は Subtraction
        assert!(matches!(b, SdfNode::Rotate { .. }));
    }

    #[test]
    fn storage_box_floor_is_solid() {
        let b = storage_box(&StorageBoxSpec::medium());
        // 内部 Y-up 設計: 底面は Y=-31 (ext_h=62、outer_hy=31)、floor 材料
        // to_z_up 変換: 世界 Z_new = 内部 Y_i → 世界 (0, 0, -30) で verify
        assert!(eval(&b, Vec3::new(0.0, 0.0, -30.0)) < 0.0);
    }

    #[test]
    fn all_household_archetypes_evaluations_finite() {
        let nodes = [
            coaster(&CoasterSpec::round_95x5()),
            tissue_box_cover(&TissueBoxCoverSpec::rectangular_us()),
            storage_box(&StorageBoxSpec::medium()),
        ];
        for (i, node) in nodes.iter().enumerate() {
            let d = eval(node, Vec3::new(0.1, 0.1, 0.1));
            assert!(
                d.is_finite(),
                "household archetype {i} produced non-finite SDF: {d}"
            );
        }
    }

    // ── hobby-diy.md 4 archetype (Sprint 5) ──

    #[test]
    fn cable_clip_hdmi_is_subtraction() {
        let c = cable_clip(&CableClipSpec::hdmi());
        assert!(matches!(c, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn cable_clip_wall_is_solid() {
        let c = cable_clip(&CableClipSpec::hdmi());
        // outer_side = 7 + 4 = 11、outer_hz = 5.5、cavity r = 3.6、slot X 幅 = 4.9
        // (0, 0, -5): 外周内部 (|Z|<5.5)、cavity 外 (radial from Y-axis = 5 > 3.6)、slot 外 (Z < slot_bottom = -1.05)
        assert!(eval(&c, Vec3::new(0.0, 0.0, -5.0)) < 0.0);
    }

    #[test]
    fn cable_clip_top_is_open() {
        let c = cable_clip(&CableClipSpec::hdmi());
        // (0, 0, +5): 上端 slot 内 (|X|<2.45、Z in [-1.05, 6.5]) → 空間
        assert!(eval(&c, Vec3::new(0.0, 0.0, 5.0)) > 0.0);
    }

    #[test]
    fn led_channel_ws2812b_is_subtraction() {
        let c = led_channel(&LedChannelSpec::ws2812b_10mm());
        assert!(matches!(c, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn led_channel_floor_is_solid() {
        let c = led_channel(&LedChannelSpec::ws2812b_10mm());
        // outer_h = 2.5 + 1.5 = 4、outer_hz = 2、cavity Z range = [-1.0, 2.5]
        // Z=-1.5 は cavity 外、outer 内 = floor 材料
        assert!(eval(&c, Vec3::new(0.0, 0.0, -1.5)) < 0.0);
    }

    #[test]
    fn led_channel_top_is_open() {
        let c = led_channel(&LedChannelSpec::ws2812b_10mm());
        // (0, 0, +1.5): cavity 内 (Z in [-1.0, 2.5]、|X|<5.5) → 空間
        assert!(eval(&c, Vec3::new(0.0, 0.0, 1.5)) > 0.0);
    }

    #[test]
    fn card_tray_poker_is_rotate_wrapped() {
        let t = card_tray(&CardTraySpec::poker());
        // to_z_up wrap で top-level は Rotate
        assert!(matches!(t, SdfNode::Rotate { .. }));
    }

    #[test]
    fn card_tray_side_wall_is_solid() {
        let t = card_tray(&CardTraySpec::poker());
        // 内部 Y-up: outer_hx = (63+2+4)/2 = 34.5、cavity_hx = (63+2)/2 = 32.5
        // to_z_up (Q_x π/2): world (Wx, Wy, Wz) → internal (Wx, Wz, -Wy)
        // world (33, 0, 0) → internal (33, 0, 0): X=33 is between cavity_hx(32.5) and outer_hx(34.5) = 側壁材料
        assert!(eval(&t, Vec3::new(33.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn token_well_dice_is_rotate_wrapped() {
        let t = token_well(&TokenWellSpec::dice_4());
        assert!(matches!(t, SdfNode::Rotate { .. }));
    }

    #[test]
    fn token_well_floor_is_solid() {
        let t = token_well(&TokenWellSpec::dice_4());
        // 内部 Y-up: ext_y = 20 + 1.5 = 21.5、outer_hy = 10.75、floor Y range [-10.75, -9.25]
        // to_z_up: world (0, 0, -10) → internal (0, -10, 0): Y=-10 in floor = 材料
        assert!(eval(&t, Vec3::new(0.0, 0.0, -10.0)) < 0.0);
    }

    #[test]
    fn token_well_center_of_well_is_open() {
        let t = token_well(&TokenWellSpec::dice_4());
        // 4 well、pitch = 22、x_start = -33、well x = [-33, -11, 11, 33]
        // to_z_up: world (Wx, Wy, Wz) → internal (Wx, Wz, -Wy)
        // 先頭 well 中心 world (-33, 0, 0) → internal (-33, 0, 0): Y=0 in well cavity range [-9.25, 11.75] = 空間
        assert!(eval(&t, Vec3::new(-33.0, 0.0, 0.0)) > 0.0);
    }

    #[test]
    fn all_hobby_diy_archetypes_evaluations_finite() {
        let nodes = [
            cable_clip(&CableClipSpec::hdmi()),
            led_channel(&LedChannelSpec::ws2812b_10mm()),
            card_tray(&CardTraySpec::poker()),
            token_well(&TokenWellSpec::dice_4()),
        ];
        for (i, node) in nodes.iter().enumerate() {
            let d = eval(node, Vec3::new(0.1, 0.1, 0.1));
            assert!(
                d.is_finite(),
                "hobby-diy archetype {i} produced non-finite SDF: {d}"
            );
        }
    }

    // ── tools.md 3 archetype (Sprint 6) ──

    #[test]
    fn wrench_holder_metric_6_is_rotate_wrapped() {
        let w = wrench_holder(&WrenchHolderSpec::metric_6_8to19());
        // to_z_up wrap で top-level は Rotate
        assert!(matches!(w, SdfNode::Rotate { .. }));
    }

    #[test]
    fn wrench_holder_wall_between_slots_is_solid() {
        let w = wrench_holder(&WrenchHolderSpec::metric_6_8to19());
        // 内部 Y-up: max_slot_w = 19+1.2 = 20.2、pitch = 23.2
        // 6 slot、x_start = -57.98、slot X = [-58, -34.8, -11.6, 11.6, 34.8, 58]
        // slot 間 wall (X = 0、Y = 0 中央、Z = 0 中央) は material
        // to_z_up: world (Wx, Wy, Wz) → internal (Wx, Wz, -Wy)
        // world (0, 0, 0) → internal (0, 0, 0)、slot 間中央 = material
        assert!(eval(&w, Vec3::new(0.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn socket_rail_half_inch_is_rotate_wrapped() {
        let s = socket_rail(&SocketRailSpec::half_inch_6());
        assert!(matches!(s, SdfNode::Rotate { .. }));
    }

    #[test]
    fn socket_rail_post_center_is_solid() {
        let s = socket_rail(&SocketRailSpec::half_inch_6());
        // 内部 Y-up: post_dia=12.4、pitch=18.4、6 post、x_start=-46
        // post 位置 X = [-46, -27.6, -9.2, 9.2, 27.6, 46]、post 中心 Y = base_hy + post_hy - 0.1 = 2 + 11 - 0.1 = 12.9
        // to_z_up: world (0, 12.9, -9.2) → internal (0, -9.2, -12.9)... 逆算
        // world (Wx, Wy, Wz) samples internal (Wx, Wz, -Wy)
        // 先頭 post 中心 internal (-46, 12.9, 0) を確認するには world (-46, 0, 12.9) を sample
        assert!(eval(&s, Vec3::new(-46.0, 0.0, 12.9)) < 0.0);
    }

    #[test]
    fn hex_bit_holder_grid_4x5_is_subtraction() {
        let h = hex_bit_holder(&HexBitHolderSpec::grid_4x5());
        // Z-up direct、hex 全 subtract で Subtraction
        assert!(matches!(h, SdfNode::Subtraction { .. }));
    }

    #[test]
    fn hex_bit_holder_floor_is_solid() {
        let h = hex_bit_holder(&HexBitHolderSpec::grid_4x5());
        // Z-up direct: outer_hz = 8、hex_offset_z = 8 - 7.5 + 0.5 = 1、hex Z range [-6.5, 8.5]
        // 底 Z=-7 は hex 外、outer 内 = 材料
        assert!(eval(&h, Vec3::new(0.0, 0.0, -7.0)) < 0.0);
    }

    #[test]
    fn hex_bit_holder_wall_between_holes_is_solid() {
        let h = hex_bit_holder(&HexBitHolderSpec::grid_4x5());
        // grid 4×5、spacing=12、x_start=-18、y_start=-24、hex 位置格子中央 (0, 0) 上端付近
        // hex 間 wall (X=6, Y=-18) は hole 外 (最寄り hole X=6,Y=-24 = 距離 6mm > hex_r=3.425)
        assert!(eval(&h, Vec3::new(6.0, -18.0, 5.0)) < 0.0);
    }

    #[test]
    fn all_tools_archetypes_evaluations_finite() {
        let nodes = [
            wrench_holder(&WrenchHolderSpec::metric_6_8to19()),
            socket_rail(&SocketRailSpec::half_inch_6()),
            hex_bit_holder(&HexBitHolderSpec::grid_4x5()),
        ];
        for (i, node) in nodes.iter().enumerate() {
            let d = eval(node, Vec3::new(0.1, 0.1, 0.1));
            assert!(
                d.is_finite(),
                "tools archetype {i} produced non-finite SDF: {d}"
            );
        }
    }

    // ── electronics-enclosure.md 3 archetype (Sprint 7) ──

    #[test]
    fn raspi_case_rpi5_is_rotate_wrapped() {
        let c = raspi_case(&RaspiCaseSpec::rpi5_active_cooler());
        // to_z_up wrap で top-level は Rotate
        assert!(matches!(c, SdfNode::Rotate { .. }));
    }

    #[test]
    fn raspi_case_floor_is_solid() {
        let c = raspi_case(&RaspiCaseSpec::rpi5_active_cooler());
        // 内部 Y-up: ext_y = 25+3 = 28, outer_hy = 14, floor Y range [-14, -11]
        // to_z_up: world (0, 0, -13) → internal (0, -13, 0), floor 材料
        assert!(eval(&c, Vec3::new(0.0, 0.0, -13.0)) < 0.0);
    }

    #[test]
    fn esp32_enclosure_devkit_is_rotate_wrapped() {
        let e = esp32_enclosure(&Esp32EnclosureSpec::esp32_devkit_v1());
        assert!(matches!(e, SdfNode::Rotate { .. }));
    }

    #[test]
    fn esp32_enclosure_wall_is_solid() {
        let e = esp32_enclosure(&Esp32EnclosureSpec::esp32_devkit_v1());
        // ext_x = 51.6 + 2*(0.5+1.6) = 55.8, outer_hx = 27.9
        // cavity_hx = (51.6 + 2*0.5)/2 = 26.3, 側壁 X in [26.3, 27.9]
        // to_z_up: world (27, 0, 0) → internal (27, 0, 0), 側壁材料
        // USB opening は +X 側なので -X 側の壁で verify
        assert!(eval(&e, Vec3::new(-27.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn battery_18650_holder_row_4_is_rotate_wrapped() {
        let b = battery_18650_holder(&Battery18650HolderSpec::row_4_through());
        assert!(matches!(b, SdfNode::Rotate { .. }));
    }

    #[test]
    fn battery_18650_holder_wall_between_cells_is_solid() {
        let b = battery_18650_holder(&Battery18650HolderSpec::row_4_through());
        // 4 cell、pitch = 18.6+2.5 = 21.1、x_start = -31.65
        // cell X = [-31.65, -10.55, 10.55, 31.65]、cell 間中央 X = 0 (cells 2-3 間)
        // Y=0 中央 (Y-axis cyl 貫通)、Z=0 中央 (cell 中心)
        // to_z_up: world (0, 0, 0) → internal (0, 0, 0), 材料 (cell 間 wall 中央)
        assert!(eval(&b, Vec3::new(0.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn battery_18650_holder_cell_cavity_is_hollow() {
        let b = battery_18650_holder(&Battery18650HolderSpec::row_4_through());
        // 先頭 cell 中心 X = -31.65、Y = 0、Z = 0
        // to_z_up: world (-31.65, 0, 0) → internal (-31.65, 0, 0), cavity 内 = 空間
        assert!(eval(&b, Vec3::new(-31.65, 0.0, 0.0)) > 0.0);
    }

    #[test]
    fn all_electronics_archetypes_evaluations_finite() {
        let nodes = [
            raspi_case(&RaspiCaseSpec::rpi5_active_cooler()),
            esp32_enclosure(&Esp32EnclosureSpec::esp32_devkit_v1()),
            battery_18650_holder(&Battery18650HolderSpec::row_4_through()),
        ];
        for (i, node) in nodes.iter().enumerate() {
            let d = eval(node, Vec3::new(0.1, 0.1, 0.1));
            assert!(
                d.is_finite(),
                "electronics archetype {i} produced non-finite SDF: {d}"
            );
        }
    }

    // ── organizer-bathroom-garage.md 3 archetype (Sprint 8) ──

    #[test]
    fn toothbrush_holder_manual_4_is_rotate_wrapped() {
        let t = toothbrush_holder(&ToothbrushHolderSpec::manual_4());
        assert!(matches!(t, SdfNode::Rotate { .. }));
    }

    #[test]
    fn toothbrush_holder_wall_is_solid() {
        let t = toothbrush_holder(&ToothbrushHolderSpec::manual_4());
        // pitch=21、x_start=-31.5、hole X = [-31.5, -10.5, 10.5, 31.5]、間 X=0 は wall
        // to_z_up: world (0, 0, 0) → internal (0, 0, 0) 材料
        assert!(eval(&t, Vec3::new(0.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn drill_bit_holder_metric_is_rotate_wrapped() {
        let d = drill_bit_holder(&DrillBitHolderSpec::metric_11_3to13());
        assert!(matches!(d, SdfNode::Rotate { .. }));
    }

    #[test]
    fn drill_bit_holder_floor_is_solid() {
        let d = drill_bit_holder(&DrillBitHolderSpec::metric_11_3to13());
        // ext_y = 22+3 = 25、outer_hy = 12.5、floor Y [-12.5, -9.5]
        // to_z_up: world (0, 0, -11) → internal (0, -11, 0) 材料
        assert!(eval(&d, Vec3::new(0.0, 0.0, -11.0)) < 0.0);
    }

    #[test]
    fn pliers_rack_standard_6_is_rotate_wrapped() {
        let p = pliers_rack(&PliersRackSpec::standard_6());
        assert!(matches!(p, SdfNode::Rotate { .. }));
    }

    #[test]
    fn pliers_rack_wall_between_slots_is_solid() {
        let p = pliers_rack(&PliersRackSpec::standard_6());
        // pitch=20、6 slot、x_start=-50、slot X = [-50, -30, -10, 10, 30, 50]
        // slot 間 X=0 は wall (slot_width=15、|X|<7.5 が slot 内)
        // to_z_up: world (0, 0, 0) → internal (0, 0, 0) 材料
        assert!(eval(&p, Vec3::new(0.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn all_bathroom_garage_archetypes_evaluations_finite() {
        let nodes = [
            toothbrush_holder(&ToothbrushHolderSpec::manual_4()),
            drill_bit_holder(&DrillBitHolderSpec::metric_11_3to13()),
            pliers_rack(&PliersRackSpec::standard_6()),
        ];
        for (i, node) in nodes.iter().enumerate() {
            let d = eval(node, Vec3::new(0.1, 0.1, 0.1));
            assert!(
                d.is_finite(),
                "bathroom-garage archetype {i} produced non-finite SDF: {d}"
            );
        }
    }

    // ── organizer-cable-kitchen.md 3 archetype (Sprint 9) ──

    #[test]
    fn spice_rack_standard_6_is_rotate_wrapped() {
        let s = spice_rack(&SpiceRackSpec::standard_6());
        assert!(matches!(s, SdfNode::Rotate { .. }));
    }

    #[test]
    fn spice_rack_base_is_solid() {
        let s = spice_rack(&SpiceRackSpec::standard_6());
        // 内部 Y-up: base 底面近く (Y=-outer_hy+1) は base material
        // lip_height = 100*0.15 = 15, base=5, ext_y=20, outer_hy=10, base 範囲 Y=[-10, -5]
        // to_z_up: world (0, 0, -8) → internal (0, -8, 0) 材料
        assert!(eval(&s, Vec3::new(0.0, 0.0, -8.0)) < 0.0);
    }

    #[test]
    fn egg_tray_4x3_is_rotate_wrapped() {
        let e = egg_tray(&EggTraySpec::tray_4x3());
        assert!(matches!(e, SdfNode::Rotate { .. }));
    }

    #[test]
    fn egg_tray_floor_is_solid() {
        let e = egg_tray(&EggTraySpec::tray_4x3());
        // ext_y = 18+3 = 21, outer_hy = 10.5, floor Y [-10.5, -7.5]
        // to_z_up: world (0, 0, -9) → internal (0, -9, 0) 材料
        assert!(eval(&e, Vec3::new(0.0, 0.0, -9.0)) < 0.0);
    }

    #[test]
    fn egg_tray_wall_between_cups_is_solid() {
        let e = egg_tray(&EggTraySpec::tray_4x3());
        // 4×3 grid、pitch=50、x_start=-75、z_start=-50、cup X=[-75,-25,25,75]
        // cup 間中央 X=0 (cup 2-3 間)、Z=-25 (row 1-2 間) は wall
        // to_z_up: world (0, 0, -25) → internal (0, -25, 0)... Y=-25 out of range
        // 別確認点: cup 間中央 world (0, y, z) → internal (0, z, -y)
        // internal (0, 0, -50) は Z=-50 = z_start+0 は row 1 cup 中央
        // wall 位置 internal (0, 0, -25) にするには world (0, 25, 0)
        // internal Y=0 は cup 内 (cup Y range [-9.25, 9.75])、しかし X=0 は cup 2-3 間 wall
        // X=0 → cup 中心 X 半径 20mm 圏外 (最寄り cup ±25、距離 25 > 20) = wall
        assert!(eval(&e, Vec3::new(0.0, 25.0, 0.0)) < 0.0);
    }

    #[test]
    fn utensil_caddy_standard_4_is_rotate_wrapped() {
        let u = utensil_caddy(&UtensilCaddySpec::standard_4());
        assert!(matches!(u, SdfNode::Rotate { .. }));
    }

    #[test]
    fn utensil_caddy_wall_is_solid() {
        let u = utensil_caddy(&UtensilCaddySpec::standard_4());
        // pitch=70、4 comp、x_start=-105、comp X = [-105, -35, 35, 105]
        // comp 間中央 X=0 (comp 2-3 間) は wall
        // to_z_up: world (0, 0, 0) → internal (0, 0, 0) 材料
        assert!(eval(&u, Vec3::new(0.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn all_cable_kitchen_archetypes_evaluations_finite() {
        let nodes = [
            spice_rack(&SpiceRackSpec::standard_6()),
            egg_tray(&EggTraySpec::tray_4x3()),
            utensil_caddy(&UtensilCaddySpec::standard_4()),
        ];
        for (i, node) in nodes.iter().enumerate() {
            let d = eval(node, Vec3::new(0.1, 0.1, 0.1));
            assert!(
                d.is_finite(),
                "cable-kitchen archetype {i} produced non-finite SDF: {d}"
            );
        }
    }
}
