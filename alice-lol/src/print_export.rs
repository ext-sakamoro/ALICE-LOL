//! 3Dプリント向けエクスポートモジュール
//!
//! LOL → `SdfNode` → Mesh → STL/3MF のワンストップパイプライン。
//! LLM が生成した LOL テキストから直接プリント可能なファイルを出力する。
//!
//! # 薄物ジオメトリの制約
//!
//! **厚さ ≤ 5mm の薄物（コイン、プレート、ワッシャー等）には本モジュールを使わないこと。**
//! マーチングキューブは薄い形状のボクセル化で非多様体エッジが大量発生し、
//! 厚さも正確に再現できない（例: 1.7mm → 5.1mm に膨張）。
//! 薄物は 2Dポリゴン(Shapely) + extrude(trimesh) → 3MF で生成すること。
//! `round()` モディファイアは薄物でさらに問題を悪化させる。
//!
//! # 使い方
//!
//! ```ignore
//! use alice_lol::print_export::{PrintConfig, lol_to_stl, lol_to_3mf};
//!
//! // LOLテキストから直接STL出力
//! lol_to_stl("lattice_infill(0.05, 5.0, 0.02, sphere(1.0))", "output.stl", &PrintConfig::default()).unwrap();
//!
//! // SdfNode から出力
//! use alice_lol::lol;
//! let node = lol! { lattice_infill(0.05, 5.0, 0.02, sphere(1.0)) };
//! node_to_stl(&node, "output.stl", &PrintConfig::default()).unwrap();
//! ```

use crate::SdfNode;
use glam::Vec3;
use std::path::Path;

// ── re-export ──
pub use alice_sdf::io::{export_3mf, export_fbx, export_stl, export_stl_ascii, FbxConfig};
pub use alice_sdf::mesh::polygon_extrude::{
    circle as polygon_circle, rect as polygon_rect, rounded_rect as polygon_rounded_rect, Polygon2D,
};
pub use alice_sdf::mesh::{
    dual_contouring, sdf_to_mesh, DualContouringConfig, MarchingCubesConfig, Mesh, MeshRepair,
    Vertex,
};

/// 3Dプリント用エクスポート設定
#[derive(Debug, Clone)]
pub struct PrintConfig {
    /// メッシュ解像度（各軸のグリッド数）。高いほど精密だがファイルサイズ増大。
    /// - 64: プレビュー（高速）
    /// - 128: 標準品質
    /// - 256: 高品質（推奨）
    /// - 512: 超高品質（大型モデル向け）
    pub resolution: usize,

    /// バウンディングボックス最小点（ワールド座標）
    pub bounds_min: Vec3,

    /// バウンディングボックス最大点（ワールド座標）
    pub bounds_max: Vec3,

    /// ワールド座標 → mm 変換スケール。
    /// LOL のデフォルト座標系は \[-5, 5\] なので、
    /// `scale_mm` = 10.0 なら 1.0 ワールド単位 = 10mm。
    pub scale_mm: f32,
}

impl Default for PrintConfig {
    fn default() -> Self {
        Self {
            resolution: 128,
            bounds_min: Vec3::splat(-2.0),
            bounds_max: Vec3::splat(2.0),
            scale_mm: 10.0,
        }
    }
}

impl PrintConfig {
    /// プレビュー品質（高速、粗い）
    #[must_use]
    pub const fn preview() -> Self {
        Self {
            resolution: 64,
            bounds_min: Vec3::splat(-2.0),
            bounds_max: Vec3::splat(2.0),
            scale_mm: 10.0,
        }
    }

    /// 高品質（推奨）
    #[must_use]
    pub const fn high_quality() -> Self {
        Self {
            resolution: 256,
            bounds_min: Vec3::splat(-2.0),
            bounds_max: Vec3::splat(2.0),
            scale_mm: 10.0,
        }
    }

    /// 超高品質（大型モデル向け）
    #[must_use]
    pub const fn ultra() -> Self {
        Self {
            resolution: 512,
            bounds_min: Vec3::splat(-2.0),
            bounds_max: Vec3::splat(2.0),
            scale_mm: 10.0,
        }
    }

    /// カスタムバウンディングボックス設定
    #[must_use]
    pub const fn with_bounds(mut self, min: Vec3, max: Vec3) -> Self {
        self.bounds_min = min;
        self.bounds_max = max;
        self
    }

    /// スケール設定（1.0ワールド単位 = `scale_mm` ミリメートル）
    #[must_use]
    pub const fn with_scale_mm(mut self, scale_mm: f32) -> Self {
        self.scale_mm = scale_mm;
        self
    }
}

/// エクスポートエラー
#[derive(Debug)]
pub enum ExportError {
    /// LOL パースエラー
    Parse(crate::runtime_parser::ParseError),
    /// ファイル I/O エラー
    Io(std::io::Error),
    /// メッシュが空（ジオメトリなし）
    EmptyMesh,
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "LOL parse error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::EmptyMesh => write!(f, "generated mesh has no triangles"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<crate::runtime_parser::ParseError> for ExportError {
    fn from(e: crate::runtime_parser::ParseError) -> Self {
        Self::Parse(e)
    }
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<alice_sdf::io::IoError> for ExportError {
    fn from(e: alice_sdf::io::IoError) -> Self {
        Self::Io(std::io::Error::other(e.to_string()))
    }
}

/// `SdfNode` → メッシュ生成（スケーリング適用済み）
#[must_use]
pub fn node_to_mesh(node: &SdfNode, config: &PrintConfig) -> Mesh {
    let mc_config = MarchingCubesConfig {
        resolution: config.resolution,
        compute_normals: true,
        ..MarchingCubesConfig::default()
    };
    let mesh = sdf_to_mesh(node, config.bounds_min, config.bounds_max, &mc_config);

    // メッシュ修復（非多様体エッジ除去、退化三角形除去、頂点マージ）
    // epsilon を大きめに設定し、マーチングキューブの境界頂点を積極的にマージ
    let mut mesh = MeshRepair::repair_all(&mesh, 5e-3);

    // ワールド座標 → mm スケーリング
    if (config.scale_mm - 1.0).abs() > f32::EPSILON {
        for v in &mut mesh.vertices {
            v.position *= config.scale_mm;
        }
    }

    mesh
}

/// `SdfNode` → STL ファイル出力
///
/// # Errors
///
/// メッシュが空の場合 `EmptyMesh`、ファイル書き込み失敗時 `Io` を返す。
pub fn node_to_stl(
    node: &SdfNode,
    path: impl AsRef<Path>,
    config: &PrintConfig,
) -> Result<ExportStats, ExportError> {
    let mesh = node_to_mesh(node, config);
    if mesh.indices.is_empty() {
        return Err(ExportError::EmptyMesh);
    }
    let stats = ExportStats::from_mesh(&mesh, &path);
    export_stl(&mesh, path)?;
    Ok(stats)
}

/// `SdfNode` → 3MF ファイル出力
///
/// # Errors
///
/// メッシュが空の場合 `EmptyMesh`、ファイル書き込み失敗時 `Io` を返す。
pub fn node_to_3mf(
    node: &SdfNode,
    path: impl AsRef<Path>,
    config: &PrintConfig,
) -> Result<ExportStats, ExportError> {
    let mesh = node_to_mesh(node, config);
    if mesh.indices.is_empty() {
        return Err(ExportError::EmptyMesh);
    }
    let stats = ExportStats::from_mesh(&mesh, &path);
    export_3mf(&mesh, path)?;
    Ok(stats)
}

/// LOL テキスト → STL ファイル出力（LLM出力をそのままファイルに）
///
/// # Errors
///
/// LOLパースエラー、メッシュ空、ファイル書き込み失敗時にエラーを返す。
pub fn lol_to_stl(
    lol_text: &str,
    path: impl AsRef<Path>,
    config: &PrintConfig,
) -> Result<ExportStats, ExportError> {
    let node = crate::runtime_parser::parse_lol(lol_text)?;
    node_to_stl(&node, path, config)
}

/// LOL テキスト → 3MF ファイル出力
///
/// # Errors
///
/// LOLパースエラー、メッシュ空、ファイル書き込み失敗時にエラーを返す。
pub fn lol_to_3mf(
    lol_text: &str,
    path: impl AsRef<Path>,
    config: &PrintConfig,
) -> Result<ExportStats, ExportError> {
    let node = crate::runtime_parser::parse_lol(lol_text)?;
    node_to_3mf(&node, path, config)
}

/// `SdfNode` → FBX ファイル出力
///
/// # Errors
///
/// メッシュが空の場合 `EmptyMesh`、ファイル書き込み失敗時 `Io` を返す。
pub fn node_to_fbx(
    node: &SdfNode,
    path: impl AsRef<Path>,
    config: &PrintConfig,
) -> Result<ExportStats, ExportError> {
    let mesh = node_to_mesh(node, config);
    if mesh.indices.is_empty() {
        return Err(ExportError::EmptyMesh);
    }
    let stats = ExportStats::from_mesh(&mesh, &path);
    export_fbx(&mesh, path, &FbxConfig::binary(), None)?;
    Ok(stats)
}

/// LOL テキスト → FBX ファイル出力
///
/// # Errors
///
/// LOLパースエラー、メッシュ空、ファイル書き込み失敗時にエラーを返す。
pub fn lol_to_fbx(
    lol_text: &str,
    path: impl AsRef<Path>,
    config: &PrintConfig,
) -> Result<ExportStats, ExportError> {
    let node = crate::runtime_parser::parse_lol(lol_text)?;
    node_to_fbx(&node, path, config)
}

// ────────────────────────────────────────────────────────
// Dual Contouring 経路 (Phase 3''、SDF 経路のまま watertight 保証、ALICE way)
// ────────────────────────────────────────────────────────

/// `SdfNode` → mesh (dual contouring 経由、SDF 経路のまま watertight 保証)
///
/// Marching Cubes は薄物 (≤ 5mm) で非多様体多発の原理的限界 (Bamboo 実測 6177 non-manifold edges)
/// **Dual Contouring は Hermite data (edge crossing position + normal) で sharp feature を保存**、
/// 薄物 + 大量穴でも topology 保証、SDF 経路のまま Phase 2 Law 準拠
///
/// 用途: SKADIS panel / thin plate / mechanical part 等、MC が破綻する SDF に対して
/// `sdf_to_mesh` の代わりに本 fn を使う
#[must_use]
pub fn node_to_mesh_dual_contouring(node: &SdfNode, config: &PrintConfig) -> Mesh {
    let dc_config = DualContouringConfig {
        resolution: config.resolution,
        compute_normals: true,
        ..DualContouringConfig::default()
    };
    let mut mesh = dual_contouring(node, config.bounds_min, config.bounds_max, &dc_config);
    if (config.scale_mm - 1.0).abs() > f32::EPSILON {
        for v in &mut mesh.vertices {
            v.position *= config.scale_mm;
        }
    }
    mesh
}

/// `SdfNode` → STL (dual contouring 経路、SDF 経路のまま watertight 保証)
///
/// # Errors
///
/// メッシュ空 `EmptyMesh`、ファイル書込 `Io`
pub fn node_to_stl_dual_contouring(
    node: &SdfNode,
    path: impl AsRef<Path>,
    config: &PrintConfig,
) -> Result<ExportStats, ExportError> {
    let mesh = node_to_mesh_dual_contouring(node, config);
    if mesh.indices.is_empty() {
        return Err(ExportError::EmptyMesh);
    }
    let stats = ExportStats::from_mesh(&mesh, &path);
    export_stl(&mesh, path)?;
    Ok(stats)
}

/// `SdfNode` → 3MF (dual contouring 経路、SDF 経路のまま watertight 保証)
///
/// SKADIS panel / thin mechanical part 等、Marching Cubes が非多様体を出す SDF に対して
/// [`node_to_3mf`] の代わりに本 fn を使う ALICE 三相原理 Phase 2 Law 準拠
///
/// Phase A.5.2 の `polygon_to_3mf` (earcutr 経路) は Phase 1 Data 相当なので、本 fn が
/// 完成次第 deprecate 予定 (`~/.claude/projects/-Users-ys/memory/feedback_alice_polygon_extrude_data_route.md` 参照)
///
/// # Errors
///
/// メッシュ空 `EmptyMesh`、ファイル書込 `Io`
pub fn node_to_3mf_dual_contouring(
    node: &SdfNode,
    path: impl AsRef<Path>,
    config: &PrintConfig,
) -> Result<ExportStats, ExportError> {
    let mesh = node_to_mesh_dual_contouring(node, config);
    if mesh.indices.is_empty() {
        return Err(ExportError::EmptyMesh);
    }
    let stats = ExportStats::from_mesh(&mesh, &path);
    export_3mf(&mesh, path)?;
    Ok(stats)
}

// ────────────────────────────────────────────────────────
// 2D polygon + extrude 経路 (Phase A.5.2、薄物 ≤ 5mm 向け)
//
// **注意**: 本経路は ALICE 三相原理 Phase 1 Data 相当 = **ALICE 違反**
// 詳細: memory/feedback_alice_polygon_extrude_data_route.md
// 真の ALICE way は上記 `node_to_3mf_dual_contouring` (Phase 2 Law 経路)
// 本経路は Phase 3'' 完成後 deprecate 予定 現状は暫定救済策として残置
// ────────────────────────────────────────────────────────

/// [`Polygon2D`] を extrude して watertight mesh を返す (スケーリング適用)
///
/// SDF+Marching Cubes を経由しないため薄物 (≤ 5mm) でも非多様体エッジが発生しない
///
/// - `half_height`: extrude 半高 (mm、全厚 = 2 × `half_height`)
/// - `scale_mm`: 座標系スケール (通常 1.0、`Polygon2D` が既に mm 単位のため)
#[must_use]
pub fn polygon_to_mesh(polygon: &Polygon2D, half_height: f32, scale_mm: f32) -> Mesh {
    let mut mesh = polygon.extrude(half_height);
    if (scale_mm - 1.0).abs() > f32::EPSILON {
        for v in &mut mesh.vertices {
            v.position *= scale_mm;
        }
    }
    mesh
}

/// [`Polygon2D`] → STL ファイル出力 (薄物向け、非多様体問題なし)
///
/// # Errors
///
/// `Polygon2D` が degenerate (頂点不足など) で triangulation 失敗時 `EmptyMesh`、
/// ファイル書き込み失敗時 `Io` を返す
pub fn polygon_to_stl(
    polygon: &Polygon2D,
    half_height: f32,
    path: impl AsRef<Path>,
) -> Result<ExportStats, ExportError> {
    let mesh = polygon_to_mesh(polygon, half_height, 1.0);
    if mesh.indices.is_empty() {
        return Err(ExportError::EmptyMesh);
    }
    let stats = ExportStats::from_mesh(&mesh, &path);
    export_stl(&mesh, path)?;
    Ok(stats)
}

/// [`Polygon2D`] → 3MF ファイル出力 (薄物向け)
///
/// SKADIS panel / shopping cart coin / thin plate 等の実プリント合格 pattern を
/// Bambu Studio に直接読ませられる .3mf を生成する canonical 経路
///
/// # Errors
///
/// メッシュ空 `EmptyMesh`、ファイル書き込み失敗 `Io`
pub fn polygon_to_3mf(
    polygon: &Polygon2D,
    half_height: f32,
    path: impl AsRef<Path>,
) -> Result<ExportStats, ExportError> {
    let mesh = polygon_to_mesh(polygon, half_height, 1.0);
    if mesh.indices.is_empty() {
        return Err(ExportError::EmptyMesh);
    }
    let stats = ExportStats::from_mesh(&mesh, &path);
    export_3mf(&mesh, path)?;
    Ok(stats)
}

/// エクスポート統計
#[derive(Debug, Clone)]
pub struct ExportStats {
    /// 頂点数
    pub vertex_count: usize,
    /// 三角形数
    pub triangle_count: usize,
    /// 出力ファイルパス
    pub path: String,
}

impl ExportStats {
    fn from_mesh(mesh: &Mesh, path: &impl AsRef<Path>) -> Self {
        Self {
            vertex_count: mesh.vertices.len(),
            triangle_count: mesh.indices.len() / 3,
            path: path.as_ref().display().to_string(),
        }
    }
}

impl std::fmt::Display for ExportStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {} vertices, {} triangles",
            self.path, self.vertex_count, self.triangle_count
        )
    }
}
