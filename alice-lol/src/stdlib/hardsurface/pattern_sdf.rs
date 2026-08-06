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
use glam::Vec3;
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
    #[allow(clippy::cast_precision_loss)]
    let int_depth = spec.height_u as f32 * gridfinity_spec::HEIGHT_UNIT - 3.0;
    let cavity_hz = int_depth * 0.5;
    let inner_hx = bin_hx - spec.wall_thickness;
    let inner_hy = bin_hy - spec.wall_thickness;
    let cavity_offset_z = spec.floor_thickness * 0.5;

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
            #[allow(clippy::cast_precision_loss)]
            let grid_cx = cx_start + (dx - 1) as f32 * cell_w * 0.5;
            #[allow(clippy::cast_precision_loss)]
            let grid_cy = cy_start + (dy - 1) as f32 * cell_d * 0.5;
            let rep_x = if dx > 1 { (dx - 1) / 2 } else { 0 };
            let rep_y = if dy > 1 { (dy - 1) / 2 } else { 0 };

            let cavity = rounded_box(cell_hx, cell_hy, cavity_hz, gridfinity_spec::INNER_FILLET);
            let cavity_repeated = SdfNode::RepeatFinite {
                child: Arc::new(cavity),
                count: [rep_x, rep_y, 0],
                spacing: Vec3::new(cell_w, cell_d, 1.0),
            };
            let cavities_placed = translate(
                cavity_repeated,
                Vec3::new(grid_cx, grid_cy, cavity_offset_z),
            );
            return subtract(outer, cavities_placed);
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
}
