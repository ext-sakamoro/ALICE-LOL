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
use std::collections::{HashMap, HashSet};
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

/// index から参照されているかを頂点ごとに返す
fn referenced_vertices(m: &Mesh) -> Vec<bool> {
    let mut used = vec![false; m.vertices.len()];
    for &i in &m.indices {
        used[i as usize] = true;
    }
    used
}

/// index から参照されない頂点 (孤立頂点) の数
///
/// 定義は「どの index からも指されない頂点」の 1 つだけ 破壊的修復は三角形を落として
/// 頂点を残すことがあり (実測 2026-10-09: gyroid scale 3.0 res 64 で 6 個)、その頂点は
/// 位相にも形状にも寄与しないまま出力に残る
fn isolated_vertices(m: &Mesh) -> usize {
    referenced_vertices(m).iter().filter(|&&u| !u).count()
}

/// Euler 標数 `V − E + F`
///
/// ⚠️ `V` は **index から参照される頂点だけ**を数える 孤立頂点は位相に寄与しないので、
/// `vertices` 全部で数えると孤立頂点の数だけ χ がずれる
/// (実測 2026-10-09: gyroid scale 3.0 res 64 の修復後で −127 に対し全頂点では −121)
fn euler_characteristic(m: &Mesh) -> i64 {
    let v = referenced_vertices(m).iter().filter(|&&u| u).count();
    let mut edges: HashSet<(u32, u32)> = HashSet::with_capacity(m.indices.len());
    for t in m.indices.as_chunks::<3>().0 {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            edges.insert((a.min(b), a.max(b)));
        }
    }
    let f = m.indices.len() / 3;
    let as_i64 = |n: usize| i64::try_from(n).expect("要素数が i64 に収まらない");
    as_i64(v) - as_i64(edges.len()) + as_i64(f)
}

/// 破壊的修復の結果を採ってよいか (非回帰 gate)
///
/// 3 条件をすべて満たす時だけ採る
/// 1. 水密性の欠陥数 ([`watertight_defects`]) が増えない
/// 2. **χ ([`euler_characteristic`]) が等しい** — 修復は距離で頂点を統合するので位相を
///    変えうる 実測 (2026-10-09、gyroid bounds ±2.0 res 64): 条件 1 だけだと
///    scale 2.0 で境界 edge −3 と引き換えに三角形 −3,179 (3.0%)、χ −20 → −39 の結果を
///    採っていた (scale 2.5 / 3.0 も同形) 「悪化」の定義を要しないよう等号で判定する
/// 3. **孤立頂点 ([`isolated_vertices`]) が増えない**
fn repair_is_non_regressive(before: &Mesh, after: &Mesh) -> bool {
    watertight_defects(&validate_mesh(after)) <= watertight_defects(&validate_mesh(before))
        && euler_characteristic(after) == euler_characteristic(before)
        && isolated_vertices(after) <= isolated_vertices(before)
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

/// 同じ位置に落ちた別頂点を統合し、index が重複した三角形を落とす
///
/// marching cubes は格子の辺ごとに 1 頂点を作るので、**格子点が表面ちょうどに
/// 載っている時はその点から出る複数の辺の頂点が同じ位置に重なる** 位置は同じまま
/// 別の index として残るため、その頂点を 2 回使う三角形 = 面積 0 の三角形ができる
/// (上流の `marching_cubes` は個数を閉形式 `2 Σ (k−1)` で固定しており、k は表面上の
/// 格子点の内側隣接数 = 意図された帰結) slicer は面積 0 の facet を拒否するので、
/// 出力の直前にここで畳む
///
/// # ⚠️ 距離や量子化で溶接しない
///
/// 統合は **`f32` の bit 完全一致**だけで行う 距離で溶接すると別の位置の頂点まで
/// 巻き込んで非多様体 edge が出る (上流が旧方式で報告した形) ⚠️ 上流の
/// `optimize::deduplicate_vertices` は量子化した位置 + 法線 + UV を hash するので
/// ここでは使えない
///
/// # ⚠️ 破壊的ではないが非回帰 guard は掛ける
///
/// 頂点を 1 つも動かさず、落とすのは面積 0 の三角形だけなので原理的に水密性を
/// 壊さない ([`repair_preserving_watertightness`] の破壊的修復とは別の操作)
/// それでも欠陥数が増えたら採らない — 判定を実測に委ねる
fn weld_coincident_vertices(raw: &Mesh) -> Mesh {
    // `Mesh::indices` が `u32` なので、これを超える頂点は index で指せない
    // ⚠️ 黙って `u32::MAX` に切り詰めずに、畳まず生 mesh を返す
    if u32::try_from(raw.vertices.len()).is_err() {
        return raw.clone();
    }
    let mut id: HashMap<[u32; 3], usize> = HashMap::with_capacity(raw.vertices.len());
    let mut out = Mesh::new();
    let mut remap: Vec<usize> = Vec::with_capacity(raw.vertices.len());
    for v in &raw.vertices {
        let next = id.len();
        let slot = *id
            .entry(v.position.to_array().map(f32::to_bits))
            .or_insert(next);
        if slot == next {
            out.vertices.push(*v);
        }
        remap.push(slot);
    }
    // 統合後の頂点数は元以下なので、上の早期 return が `u32` に収まることを保証する
    #[allow(
        clippy::cast_possible_truncation,
        reason = "頂点数が u32 に収まることは関数先頭で確認済み、統合でしか減らない"
    )]
    let remap: Vec<u32> = remap.into_iter().map(|i| i as u32).collect();
    for t in raw.indices.as_chunks::<3>().0 {
        let (a, b, c) = (
            remap[t[0] as usize],
            remap[t[1] as usize],
            remap[t[2] as usize],
        );
        // index が重複した三角形 = 畳まれて面積 0 になったもの
        if a != b && b != c && a != c {
            out.indices.extend_from_slice(&[a, b, c]);
        }
    }
    if watertight_defects(&validate_mesh(&out)) <= watertight_defects(&validate_mesh(raw)) {
        out
    } else {
        raw.clone()
    }
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
///    (上流 `alice_sdf::mesh::manifold` 側の課題)
///
/// そこで (a) **水密な mesh には破壊的操作を掛けない** (b) 掛けた結果が悪化したか
/// 位相を変えたら **採らない** ([`repair_is_non_regressive`]) の 2 段で、`node_to_mesh` を**非回帰**にする
fn repair_preserving_watertightness(raw: &Mesh, eps: f32) -> Mesh {
    // ⚠️ 先に同位置の頂点を畳む — ここで畳まないと、水密な mesh (境界 edge 0 /
    //    非多様体 edge 0) に面積 0 の三角形が残ったまま下の早期 return を通る
    let raw = &weld_coincident_vertices(raw);
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
    let candidate = if repair_is_non_regressive(raw, &m) {
        // 元から不整合があった mesh なので winding を揃える
        MeshRepair::fix_normals(&m)
    } else {
        // ⚠️ 修復が悪化させたか位相を変えた — 生 mesh を採る
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
/// Marching Cubes は薄物 (≤ 5mm) で非多様体多発の原理的限界 (pipeline 実測 6177 non-manifold edges)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn mesh(points: &[[f32; 3]], indices: &[u32]) -> Mesh {
        let mut m = Mesh::new();
        for p in points {
            m.vertices.push(Vertex::new(Vec3::from_array(*p), Vec3::Z));
        }
        m.indices.extend_from_slice(indices);
        m
    }

    const TETRA_POINTS: [[f32; 3]; 4] = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    const TETRA_INDICES: [u32; 12] = [0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3];

    fn tetra_with_extra_vertex() -> Mesh {
        let mut points = TETRA_POINTS.to_vec();
        points.push([5.0, 5.0, 5.0]);
        mesh(&points, &TETRA_INDICES)
    }

    /// 三角形 1 枚 (V 3 / E 3 / F 1) の χ は 1、どの index も指さない頂点を 1 つ
    /// 足しても χ は 1 のまま (全頂点で数えると 2 になる) で、孤立頂点は 1
    #[test]
    fn euler_characteristic_counts_only_referenced_vertices() {
        let tri = mesh(
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[0, 1, 2],
        );
        assert_eq!(euler_characteristic(&tri), 1);
        assert_eq!(isolated_vertices(&tri), 0);

        let tri_plus = mesh(
            &[
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [3.0, 3.0, 3.0],
            ],
            &[0, 1, 2],
        );
        assert_eq!(
            euler_characteristic(&tri_plus),
            1,
            "未参照の頂点が χ に入った"
        );
        assert_eq!(isolated_vertices(&tri_plus), 1);

        // 閉じた四面体 (球面と同相) は χ = 2
        let tetra = mesh(&TETRA_POINTS, &TETRA_INDICES);
        assert_eq!(euler_characteristic(&tetra), 2);
        assert_eq!(isolated_vertices(&tetra), 0);
        let tetra_plus = tetra_with_extra_vertex();
        assert_eq!(
            euler_characteristic(&tetra_plus),
            2,
            "未参照の頂点が χ に入った"
        );
        assert_eq!(isolated_vertices(&tetra_plus), 1);

        // 未参照の頂点が複数あれば全部数える (index の後ろでなく間にあっても)
        let gaps = mesh(
            &[
                [9.0, 9.0, 9.0],
                [0.0, 0.0, 0.0],
                [8.0, 8.0, 8.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            &[1, 3, 4],
        );
        assert_eq!(isolated_vertices(&gaps), 2);
        assert_eq!(euler_characteristic(&gaps), 1);
    }

    /// gate の 3 項をそれぞれ単独で破る mesh の組で、どの項も効いていることを固定する
    #[test]
    fn the_repair_gate_rejects_each_kind_of_regression_on_its_own() {
        let tetra = mesh(&TETRA_POINTS, &TETRA_INDICES);
        assert!(repair_is_non_regressive(&tetra, &tetra), "同じ mesh は通る");

        // 孤立頂点だけが増える (欠陥数 0 / χ 2 は不変)
        assert!(
            !repair_is_non_regressive(&tetra, &tetra_with_extra_vertex()),
            "孤立頂点が増えたのに採られた"
        );
        // 孤立頂点が減るのは通る
        assert!(repair_is_non_regressive(&tetra_with_extra_vertex(), &tetra));

        // χ だけが変わる: 離れた四面体 2 つ (χ 4) → 1 つ (χ 2)、欠陥数 0 / 孤立頂点 0
        let mut two_points = TETRA_POINTS.to_vec();
        two_points.extend(TETRA_POINTS.iter().map(|p| [p[0] + 3.0, p[1], p[2]]));
        let mut two_indices = TETRA_INDICES.to_vec();
        two_indices.extend(TETRA_INDICES.iter().map(|i| i + 4));
        let two = mesh(&two_points, &two_indices);
        assert_eq!(euler_characteristic(&two), 4);
        assert!(
            !repair_is_non_regressive(&two, &tetra),
            "χ が変わったのに採られた"
        );

        // 欠陥数だけが増える: 三角形 1 枚 (境界 3、χ 1) → 2 枚の四角形 (境界 4、χ 1)
        let tri = mesh(
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[0, 1, 2],
        );
        let quad = mesh(
            &[
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            &[0, 1, 2, 1, 3, 2],
        );
        assert_eq!(euler_characteristic(&quad), 1);
        assert!(
            !repair_is_non_regressive(&tri, &quad),
            "境界 edge が増えたのに採られた"
        );
        assert!(
            repair_is_non_regressive(&quad, &tri),
            "境界 edge が減るのは通る"
        );
    }
}
