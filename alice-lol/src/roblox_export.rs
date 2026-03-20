//! Roblox 向けメッシュエクスポートモジュール
//!
//! LOL → `SdfNode` → `Mesh` → OBJ/FBX (Roblox `MeshPart` 用) のパイプライン。
//! アクセサリー (帽子・武器・装飾品) の静的メッシュ出力に特化。
//!
//! # 使い方
//!
//! ```ignore
//! use alice_lol::roblox_export::{RobloxConfig, lol_to_obj_roblox};
//!
//! let stats = lol_to_obj_roblox(
//!     "smooth_union(0.3, sphere(1.0), translate(0.0, 1.5, 0.0, scale(0.6, sphere(1.0))))",
//!     "hat.obj",
//!     &RobloxConfig::accessory(),
//! ).unwrap();
//! println!("{stats}");
//! ```

use crate::print_export::{ExportError, MarchingCubesConfig, Mesh, MeshRepair};
use crate::SdfNode;
use alice_sdf::io::{export_fbx, export_obj, FbxConfig, FbxUpAxis, ObjConfig};
use alice_sdf::mesh::sdf_to_mesh;
use glam::Vec3;
use std::path::Path;

// ── Roblox 制約定数 ──

/// UGC アクセサリー三角形上限
const ROBLOX_ACCESSORY_MAX_TRIS: usize = 4_000;

/// 汎用 `MeshPart` 三角形上限
const ROBLOX_MESHPART_MAX_TRIS: usize = 10_000;

/// デフォルトアクセサリーサイズ上限 (studs)
const ROBLOX_DEFAULT_MAX_SIZE: Vec3 = Vec3::new(10.0, 10.0, 10.0);

/// デジェネレート面判定 epsilon
const DEGENERATE_EPSILON: f32 = 1e-8;

// ── 設定 ──

/// Roblox エクスポート設定
#[derive(Debug, Clone)]
pub struct RobloxConfig {
    /// メッシュ解像度 (Marching Cubes グリッド数)
    pub resolution: usize,

    /// SDF バウンディングボックス最小点
    pub bounds_min: Vec3,

    /// SDF バウンディングボックス最大点
    pub bounds_max: Vec3,

    /// SDF 単位 → stud 変換スケール (1.0 SDF unit = `scale_studs` studs)
    pub scale_studs: f32,

    /// 三角形数上限
    pub max_triangles: usize,

    /// バウンディングボックス上限 (studs)
    pub max_size_studs: Vec3,
}

impl Default for RobloxConfig {
    fn default() -> Self {
        Self {
            resolution: 128,
            bounds_min: Vec3::splat(-2.0),
            bounds_max: Vec3::splat(2.0),
            scale_studs: 2.0,
            max_triangles: ROBLOX_ACCESSORY_MAX_TRIS,
            max_size_studs: ROBLOX_DEFAULT_MAX_SIZE,
        }
    }
}

impl RobloxConfig {
    /// UGC アクセサリー向けプリセット (4,000 三角形上限)
    #[must_use]
    pub const fn accessory() -> Self {
        Self {
            resolution: 128,
            bounds_min: Vec3::splat(-2.0),
            bounds_max: Vec3::splat(2.0),
            scale_studs: 2.0,
            max_triangles: ROBLOX_ACCESSORY_MAX_TRIS,
            max_size_studs: ROBLOX_DEFAULT_MAX_SIZE,
        }
    }

    /// 汎用 `MeshPart` 向けプリセット (10,000 三角形上限)
    #[must_use]
    pub const fn meshpart() -> Self {
        Self {
            resolution: 192,
            bounds_min: Vec3::splat(-2.0),
            bounds_max: Vec3::splat(2.0),
            scale_studs: 2.0,
            max_triangles: ROBLOX_MESHPART_MAX_TRIS,
            max_size_studs: ROBLOX_DEFAULT_MAX_SIZE,
        }
    }

    /// 高速プレビュー向けプリセット
    #[must_use]
    pub const fn preview() -> Self {
        Self {
            resolution: 64,
            bounds_min: Vec3::splat(-2.0),
            bounds_max: Vec3::splat(2.0),
            scale_studs: 2.0,
            max_triangles: ROBLOX_ACCESSORY_MAX_TRIS,
            max_size_studs: ROBLOX_DEFAULT_MAX_SIZE,
        }
    }

    /// カスタムバウンディングボックス設定
    #[must_use]
    pub const fn with_bounds(mut self, min: Vec3, max: Vec3) -> Self {
        self.bounds_min = min;
        self.bounds_max = max;
        self
    }

    /// スケール設定
    #[must_use]
    pub const fn with_scale_studs(mut self, scale: f32) -> Self {
        self.scale_studs = scale;
        self
    }

    /// 三角形上限設定
    #[must_use]
    pub const fn with_max_triangles(mut self, max: usize) -> Self {
        self.max_triangles = max;
        self
    }

    /// サイズ上限設定 (studs)
    #[must_use]
    pub const fn with_max_size_studs(mut self, max: Vec3) -> Self {
        self.max_size_studs = max;
        self
    }
}

// ── バリデーション ──

/// Roblox 向けメッシュバリデーション結果
#[derive(Debug, Clone)]
pub struct RobloxValidation {
    /// 三角形数
    pub triangle_count: usize,
    /// 頂点数
    pub vertex_count: usize,
    /// stud 単位のバウンディングボックスサイズ
    pub bounds_studs: Vec3,
    /// 三角形上限以内か
    pub is_within_triangle_limit: bool,
    /// サイズ上限以内か
    pub is_within_size_limit: bool,
    /// デジェネレート面があるか
    pub has_degenerate_faces: bool,
}

impl RobloxValidation {
    /// 全チェック合格か
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.is_within_triangle_limit && self.is_within_size_limit && !self.has_degenerate_faces
    }
}

impl std::fmt::Display for RobloxValidation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.is_valid() { "PASS" } else { "FAIL" };
        write!(
            f,
            "[{}] {} tris, {} verts, bounds: {:.1} x {:.1} x {:.1} studs",
            status,
            self.triangle_count,
            self.vertex_count,
            self.bounds_studs.x,
            self.bounds_studs.y,
            self.bounds_studs.z,
        )?;
        if !self.is_within_triangle_limit {
            write!(f, " [OVER TRI LIMIT]")?;
        }
        if !self.is_within_size_limit {
            write!(f, " [OVER SIZE LIMIT]")?;
        }
        if self.has_degenerate_faces {
            write!(f, " [DEGENERATE FACES]")?;
        }
        Ok(())
    }
}

/// メッシュを Roblox 制約に対して検証
#[must_use]
pub fn validate_for_roblox(mesh: &Mesh, config: &RobloxConfig) -> RobloxValidation {
    let tri_count = mesh.indices.len() / 3;
    let vert_count = mesh.vertices.len();

    // バウンディングボックス計算 (stud 単位)
    let bounds = compute_bounds_studs(mesh);

    // デジェネレート面チェック
    let has_degenerate = check_degenerate_faces(mesh);

    RobloxValidation {
        triangle_count: tri_count,
        vertex_count: vert_count,
        bounds_studs: bounds,
        is_within_triangle_limit: tri_count <= config.max_triangles,
        is_within_size_limit: bounds.x <= config.max_size_studs.x
            && bounds.y <= config.max_size_studs.y
            && bounds.z <= config.max_size_studs.z,
        has_degenerate_faces: has_degenerate,
    }
}

/// バウンディングボックスサイズ (頂点座標から、スケーリング適用後)
fn compute_bounds_studs(mesh: &Mesh) -> Vec3 {
    if mesh.vertices.is_empty() {
        return Vec3::ZERO;
    }
    let mut min = Vec3::splat(f32::MAX);
    let mut max = Vec3::splat(f32::MIN);
    for v in &mesh.vertices {
        min = min.min(v.position);
        max = max.max(v.position);
    }
    max - min
}

/// デジェネレート面 (面積ゼロ) の有無チェック
fn check_degenerate_faces(mesh: &Mesh) -> bool {
    let indices = &mesh.indices;
    let verts = &mesh.vertices;
    let tri_count = indices.len() / 3;
    for i in 0..tri_count {
        let a = verts[indices[i * 3] as usize].position;
        let b = verts[indices[i * 3 + 1] as usize].position;
        let c = verts[indices[i * 3 + 2] as usize].position;
        let cross = (b - a).cross(c - a);
        if cross.length_squared() < DEGENERATE_EPSILON {
            return true;
        }
    }
    false
}

// ── メッシュ生成 ──

/// `SdfNode` → Roblox 用メッシュ生成 (スケーリング + 修復済み)
#[must_use]
pub fn node_to_mesh_roblox(node: &SdfNode, config: &RobloxConfig) -> Mesh {
    // 目標三角形数から解像度を推定
    // Marching Cubes は resolution³ の ~2% 程度が三角形数になる経験則
    let resolution = estimate_resolution(config.resolution, config.max_triangles);

    let mc_config = MarchingCubesConfig {
        resolution,
        compute_normals: true,
        ..MarchingCubesConfig::default()
    };
    let mesh = sdf_to_mesh(node, config.bounds_min, config.bounds_max, &mc_config);

    // 修復
    let mut mesh = MeshRepair::repair_all(&mesh, 5e-3);

    // SDF 座標 → stud スケーリング
    if (config.scale_studs - 1.0).abs() > f32::EPSILON {
        for v in &mut mesh.vertices {
            v.position *= config.scale_studs;
        }
    }

    mesh
}

/// 三角形上限から適切な解像度を推定
///
/// Marching Cubes の三角形数は resolution に対して O(resolution²) に比例。
/// ユーザー指定 resolution を上限として、三角形数が超過しそうなら下げる。
fn estimate_resolution(user_resolution: usize, max_triangles: usize) -> usize {
    // 経験則: resolution=128 → ~20,000 tri (球体の場合)
    // resolution=64 → ~5,000 tri
    // resolution=48 → ~2,500 tri
    // 最終的に解像度はユーザー指定を尊重し、上限でクランプ
    let estimated_tris_at_128: u16 = 20_000;
    #[allow(clippy::cast_precision_loss)]
    let ratio = max_triangles as f32 / f32::from(estimated_tris_at_128);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let auto_res = (128.0_f32 * ratio.sqrt()).round() as usize;
    // ユーザー指定と自動推定の小さい方を採用
    user_resolution.min(auto_res).max(16)
}

// ── エクスポート関数 ──

/// Roblox エクスポート統計
#[derive(Debug, Clone)]
pub struct RobloxExportStats {
    /// 頂点数
    pub vertex_count: usize,
    /// 三角形数
    pub triangle_count: usize,
    /// stud 単位のバウンディングボックスサイズ
    pub bounds_studs: Vec3,
    /// 出力ファイルパス
    pub path: String,
    /// バリデーション結果
    pub validation: RobloxValidation,
}

impl std::fmt::Display for RobloxExportStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {} vertices, {} triangles (bounds: {:.1} x {:.1} x {:.1} studs) {}",
            self.path,
            self.vertex_count,
            self.triangle_count,
            self.bounds_studs.x,
            self.bounds_studs.y,
            self.bounds_studs.z,
            self.validation,
        )
    }
}

/// `SdfNode` → OBJ ファイル出力 (Roblox 制約適用)
///
/// # Errors
///
/// メッシュが空の場合 `EmptyMesh`、ファイル書き込み失敗時 `Io`、
/// Roblox 制約違反時に警告をログ出力 (エラーにはしない)。
pub fn node_to_obj_roblox(
    node: &SdfNode,
    path: impl AsRef<Path>,
    config: &RobloxConfig,
) -> Result<RobloxExportStats, ExportError> {
    let mesh = node_to_mesh_roblox(node, config);
    if mesh.indices.is_empty() {
        return Err(ExportError::EmptyMesh);
    }

    let validation = validate_for_roblox(&mesh, config);

    let obj_config = ObjConfig {
        export_normals: true,
        export_uvs: true,
        export_materials: false,
        flip_uv_v: false,
    };
    export_obj(&mesh, &path, &obj_config, None)?;

    Ok(build_stats(&mesh, &path, validation))
}

/// `SdfNode` → FBX ファイル出力 (Roblox 制約適用)
///
/// # Errors
///
/// メッシュが空の場合 `EmptyMesh`、ファイル書き込み失敗時 `Io`。
pub fn node_to_fbx_roblox(
    node: &SdfNode,
    path: impl AsRef<Path>,
    config: &RobloxConfig,
) -> Result<RobloxExportStats, ExportError> {
    let mesh = node_to_mesh_roblox(node, config);
    if mesh.indices.is_empty() {
        return Err(ExportError::EmptyMesh);
    }

    let validation = validate_for_roblox(&mesh, config);

    let fbx_config = FbxConfig {
        export_normals: true,
        export_uvs: true,
        export_materials: false,
        up_axis: FbxUpAxis::Y,
        ..FbxConfig::default()
    };
    export_fbx(&mesh, &path, &fbx_config, None)?;

    Ok(build_stats(&mesh, &path, validation))
}

/// LOL テキスト → OBJ ファイル出力 (Roblox 制約適用, LLM 連携用)
///
/// # Errors
///
/// LOLパースエラー、メッシュ空、ファイル書き込み失敗時にエラーを返す。
pub fn lol_to_obj_roblox(
    lol_text: &str,
    path: impl AsRef<Path>,
    config: &RobloxConfig,
) -> Result<RobloxExportStats, ExportError> {
    let node = crate::runtime_parser::parse_lol(lol_text)?;
    node_to_obj_roblox(&node, path, config)
}

/// LOL テキスト → FBX ファイル出力 (Roblox 制約適用, LLM 連携用)
///
/// # Errors
///
/// LOLパースエラー、メッシュ空、ファイル書き込み失敗時にエラーを返す。
pub fn lol_to_fbx_roblox(
    lol_text: &str,
    path: impl AsRef<Path>,
    config: &RobloxConfig,
) -> Result<RobloxExportStats, ExportError> {
    let node = crate::runtime_parser::parse_lol(lol_text)?;
    node_to_fbx_roblox(&node, path, config)
}

fn build_stats(
    mesh: &Mesh,
    path: &impl AsRef<Path>,
    validation: RobloxValidation,
) -> RobloxExportStats {
    RobloxExportStats {
        vertex_count: mesh.vertices.len(),
        triangle_count: mesh.indices.len() / 3,
        bounds_studs: validation.bounds_studs,
        path: path.as_ref().display().to_string(),
        validation,
    }
}
