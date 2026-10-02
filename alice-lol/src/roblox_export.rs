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

use crate::print_export::{node_to_mesh, ExportError, Mesh, PrintConfig};
use crate::SdfNode;
use alice_sdf::io::{export_fbx, export_obj, FbxConfig, FbxUpAxis, ObjConfig};
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

/// 三角形上限を守るために下げてよい解像度の下限
const MIN_RESOLUTION: usize = 16;

/// 三角形数を見積もるための試し解像度 (粗いので速い)
const PROBE_RESOLUTION: usize = 32;

// ── 設定 ──

/// Roblox エクスポート設定
#[derive(Debug, Clone)]
pub struct RobloxConfig {
    /// メッシュ解像度 (Marching Cubes グリッド数) の **上限**
    ///
    /// 実際の解像度は `max_triangles` を守れる最大値まで自動で下がる
    /// (下限 [`MIN_RESOLUTION`]) 単純な形状はこの値まで細かくなる
    pub resolution: usize,

    /// SDF バウンディングボックス最小点
    pub bounds_min: Vec3,

    /// SDF バウンディングボックス最大点
    pub bounds_max: Vec3,

    /// SDF 単位 → stud 変換スケール (1.0 SDF unit = `scale_studs` studs)
    pub scale_studs: f32,

    /// 三角形数上限 (解像度を下げて守る、下限解像度でも超える形状は
    /// [`RobloxValidation::is_within_triangle_limit`] が偽になる)
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

    /// 高速プレビュー向けプリセット (解像度上限 32、accessory より粗い)
    #[must_use]
    pub const fn preview() -> Self {
        Self {
            resolution: 32,
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
///
/// 水密性を保つ修復とスケーリングは [`crate::print_export::node_to_mesh`] に任せる
/// (許容量は cell 幅比、水密な mesh には破壊的操作を掛けない) 解像度は
/// `config.resolution` を上限に、`config.max_triangles` を守れる最大値を実測で選ぶ
#[must_use]
pub fn node_to_mesh_roblox(node: &SdfNode, config: &RobloxConfig) -> Mesh {
    let mesh_at = |resolution: usize| {
        node_to_mesh(
            node,
            &PrintConfig {
                resolution,
                bounds_min: config.bounds_min,
                bounds_max: config.bounds_max,
                // `PrintConfig::scale_mm` は「ワールド座標 → 出力単位」の倍率 (ここでは stud)
                scale_mm: config.scale_studs,
            },
        )
    };
    let cap = config.resolution.max(MIN_RESOLUTION);

    // 表面の三角形数は解像度の 2 乗にほぼ比例する 粗い試し (32) で見積もった解像度へ進む
    let mut resolution = cap.min(PROBE_RESOLUTION);
    let mut mesh = mesh_at(resolution);
    let tris = mesh.indices.len() / 3;
    if tris == 0 {
        return mesh;
    }
    resolution =
        refine_resolution(resolution, tris, config.max_triangles).clamp(MIN_RESOLUTION, cap);
    if resolution != cap.min(PROBE_RESOLUTION) {
        mesh = mesh_at(resolution);
    }

    // 超えていれば解像度を単調に下げる (厳密に減るので必ず止まる)
    // 保証: 三角形数が上限以内、または下限解像度に達している
    while mesh.indices.len() / 3 > config.max_triangles && resolution > MIN_RESOLUTION {
        let next = refine_resolution(resolution, mesh.indices.len() / 3, config.max_triangles);
        resolution = next.clamp(MIN_RESOLUTION, resolution - 1);
        mesh = mesh_at(resolution);
    }
    mesh
}

/// 三角形数が解像度の 2 乗に比例するとして、`max_triangles` に収まる解像度を見積もる
///
/// 見積もりの誤差を吸収するため 3% 手前に寄せる (超えた場合は呼び出し側が下げ直す)
fn refine_resolution(resolution: usize, triangles: usize, max_triangles: usize) -> usize {
    #[allow(clippy::cast_precision_loss)]
    let ratio = (max_triangles as f32 / triangles.max(1) as f32).sqrt();
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let next = (resolution as f32 * ratio * 0.97) as usize;
    next
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
/// メッシュが空の場合 `EmptyMesh`、ファイル書き込み失敗時 `Io`
/// Roblox 制約違反はエラーにもログにもせず、戻り値の [`RobloxExportStats::validation`] に載せる
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
