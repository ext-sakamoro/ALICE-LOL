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
pub use alice_sdf::mesh::{
    dual_contouring, sdf_to_mesh, validate_mesh, DualContouringConfig, MarchingCubesConfig, Mesh,
    MeshRepair, MeshValidation, Vertex,
};

/// 頂点マージ許容量を cell 幅の何倍にするか
///
/// ⚠️ **絶対値で固定してはいけない** — 許容量が cell 幅に対して大きいと隣接 cell の
/// 別頂点まで融合して**水密な mesh に穴を開ける** 実測 (2026-09-30、球 r=1.0 /
/// bounds ±2.0) で破れ始めるのは **eps/cell ≈ 8%** で、旧実装の固定 `5e-3` は
/// res 128 (cell 0.03125) で **16%**、res 256 で **32%** だった
/// (境界 edge が res 128 で 741 / res 256 で 3561、`χ` は 2 → −86 / −601)
const VERTEX_MERGE_CELL_RATIO: f32 = 0.04;

/// 解像度 (cell 数) を f32 に落とす
///
/// `PrintConfig` の doc が想定する 64〜512 は f32 が整数を厳密に表せる範囲に収まる
/// 用途は cell 幅の算出だけなので、超えた場合の丸めは許容量のわずかな誤差で済む
#[allow(clippy::cast_precision_loss)]
const fn resolution_as_f32(resolution: usize) -> f32 {
    resolution as f32
}

/// 水密性の欠陥数 (0 が健全)
///
/// ⚠️ **退化三角形 (零面積) は数えない** — 実測 (2026-10-01) で球 res 192 の生 mesh は
/// 境界 edge 0 / 非多様体 edge 0 でありながら零面積 sliver を持っており、これを
/// 欠陥に数えると破壊的修復へ入って **境界 edge 216 枚**を作ってしまう
/// (`remove_degenerate_triangles` が sliver を消して縫い直さないため)
/// 向きの不整合 (`inconsistent_normals`) も winding の問題なので数えない
const fn watertight_defects(v: &MeshValidation) -> usize {
    v.boundary_edges + v.non_manifold_edges
}

/// 発散定理による符号付き体積 外向き CCW なら正
fn signed_volume(m: &Mesh) -> f64 {
    m.indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| {
            let a = m.vertices[t[0] as usize].position.as_dvec3();
            let b = m.vertices[t[1] as usize].position.as_dvec3();
            let c = m.vertices[t[2] as usize].position.as_dvec3();
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

/// 全体の向きを符号付き体積で決める (facet ごとの判定はしない)
///
/// # ⚠️ なぜ facet ごとに勾配で判定しないか
///
/// 実測 (2026-10-01、`box3d(1.8³) − sphere(1.1)` を bounds ±2.0 / res 32):
/// **同じ生 mesh に対して外向き判定の計器 3 つが互いに違う答えを出す** —
/// 有限差分勾配 (eps 1e-3) は内向き **144 枚** / facet 法線方向の SDF 値差
/// (δ = cell/4) は **96 枚** / marching cubes が作った頂点法線は **264 枚**
/// CSG の稜線では距離場の勾配が不連続なので、どれも局所的に信用できない
///
/// 一方 **有向 edge の対消滅が 0** (= 全 edge が各向きにちょうど 1 回辿られる) は
/// winding が**大域的に整合**していることの証明で、整合した閉曲面は
/// 「全部外向き」か「全部内向き」のどちらかにしかなりえない
/// ⇒ ⚠️ **判定は符号付き体積 1 つで足りる**
///
/// facet ごとに勾配で反転させると、⚠️ **隣接 facet と winding が食い違って
/// 幾何的に閉じた mesh に「境界 edge」が現れる** (実測: 0 → 288 枚)
fn orient_outward(mut mesh: Mesh) -> Mesh {
    if signed_volume(&mesh) >= 0.0 {
        return mesh;
    }
    for tri in mesh.indices.as_chunks_mut::<3>().0 {
        tri.swap(1, 2);
    }
    mesh
}

/// 修復が実際に改善した時だけ修復後を採る
///
/// # ⚠️ なぜ `MeshRepair::repair_all` を直接呼ばないか
///
/// `repair_all` は **winding を揃える操作** (`fix_normals`) と **破壊的な操作**
/// (退化除去 / 頂点マージ / 重複面除去) を束ねている 実測 (2026-09-30) では
/// `sdf_to_mesh` の生 mesh が res 32〜256 すべてで境界 edge 0 / 非多様体 edge 0 /
/// `χ = 2` だったのに、`repair_all(5e-3)` 通過後は **res 64 で境界 edge 111 /
/// 既定 res 128 で 741 + 非多様体 285 / res 256 で 3561** になっていた
/// ⚠️ **測った全 case で「修復が何かを変えたなら必ず悪化」で、改善は 0 件**
///
/// 真因は 2 段で、⚠️ **許容量を下げるだけでは直らない**:
/// 1. `merge_duplicate_vertices` の許容量が cell 幅に対して過大
///    (→ [`VERTEX_MERGE_CELL_RATIO`] で cell 相対にした)
/// 2. 許容量 `1e-4` (何も融合しない) でも res ≥ 192 で破れる —
///    `remove_degenerate_triangles` が sliver を消して**縫い直さない**
///    (上流 `alice_sdf::mesh::manifold` 側の課題、Backlog に起票済)
///
/// そこで (a) **水密な mesh には破壊的操作を掛けない** (b) 掛けた結果が悪化したら
/// **採らない** の 2 段で、`node_to_mesh` を**非回帰**にする
fn repair_preserving_watertightness(raw: &Mesh, eps: f32) -> Mesh {
    let before = validate_mesh(raw);
    if watertight_defects(&before) == 0 {
        // ⚠️ 既に水密 — 破壊的操作も winding の付け替えもしない
        // (整合している winding を触ると境界 edge が現れる、[`orient_outward`])
        return orient_outward(raw.clone());
    }
    let m = MeshRepair::remove_degenerate_triangles(raw);
    let m = MeshRepair::merge_duplicate_vertices(&m, eps);
    let m = MeshRepair::remove_degenerate_triangles(&m);
    let m = MeshRepair::remove_duplicate_triangles(&m);
    let candidate = if watertight_defects(&validate_mesh(&m)) <= watertight_defects(&before) {
        // 元から不整合があった mesh なので winding を揃える
        MeshRepair::fix_normals(&m)
    } else {
        // ⚠️ 修復が悪化させた — 生 mesh を採る
        raw.clone()
    };
    orient_outward(candidate)
}

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
    let raw = sdf_to_mesh(node, config.bounds_min, config.bounds_max, &mc_config);

    // メッシュ修復 ⚠️ **非回帰** — 水密なら破壊的操作を掛けず、掛けた結果が悪化したら
    // 採らない (詳細は [`repair_preserving_watertightness`])
    // 許容量は cell 幅相対 (⚠️ 旧実装の固定 `5e-3` は res 128 で cell の 16%)
    let cell = (config.bounds_max - config.bounds_min).max_element()
        / resolution_as_f32(config.resolution.max(1));
    let mut mesh = repair_preserving_watertightness(&raw, VERTEX_MERGE_CELL_RATIO * cell);

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
/// Phase 3''.2 の実測 (`coin_dc_vs_mc.rs` example) で 極薄物 (1.7mm coin) でも
/// DC は全 resolution で `non_manifold_edges = 0` を実現、MC の破綻 (resolution 512 で
/// 24,808 non-manifold) を完全回避 これに基づき Phase 4 で旧 `polygon_to_*`
/// (earcutr Data 経路 = ALICE 違反) を削除、DC 経路に完全統一
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
