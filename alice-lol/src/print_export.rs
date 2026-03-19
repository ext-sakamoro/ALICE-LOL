//! 3Dプリント向けエクスポートモジュール
//!
//! LOL → `SdfNode` → Mesh → STL/3MF のワンストップパイプライン。
//! LLM が生成した LOL テキストから直接プリント可能なファイルを出力する。
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
pub use alice_sdf::io::{export_3mf, export_stl, export_stl_ascii};
pub use alice_sdf::mesh::{sdf_to_mesh, MarchingCubesConfig, Mesh, MeshRepair, Vertex};

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
