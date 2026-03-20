# ALICE-LOL → Roblox パイプライン仕様書

## 概要

ALICE-LOL DSL で定義した SDF 形状を、Roblox Studio にインポート可能な
MeshPart 用メッシュ (OBJ/FBX) として出力するパイプライン。

フェーズ B: アクセサリー / 静的 MeshPart 特化。

```
LOL DSL テキスト or lol!{} マクロ
  ↓ parse_lol() / compile-time
SdfNode
  ↓ sdf_to_mesh() (Marching Cubes)
高解像度 Mesh
  ↓ decimate (三角形数を上限以下に)
  ↓ validate_for_roblox()
検証済み Mesh
  ↓ export_obj() / export_fbx()
OBJ / FBX ファイル
  ↓ Roblox Studio → MeshPart → Import
Roblox ゲーム内アクセサリー
```

---

## Roblox MeshPart / アクセサリー技術要件

### メッシュフォーマット

| フォーマット | 対応状況 | 備考 |
|------------|---------|------|
| **FBX** | 推奨 | バイナリ/ASCII FBX 7.4。スタティックメッシュ対応 |
| **OBJ** | 対応 | MeshPart インポートに使用可能 |
| glTF/GLB | 非対応 | Roblox Studio は直接読めない |

### ポリゴン制限

| カテゴリ | 三角形上限 | 備考 |
|---------|-----------|------|
| MeshPart (汎用) | 10,000 | Studio インポート上限 |
| アクセサリー (帽子・武器等) | 4,000 | UGC マーケットプレイス制限 |
| レイヤードクロージング | 4,000 | ケージ付き |

### サイズ制限

- **単位**: 1 stud ≈ 0.28m
- **アクセサリー推奨サイズ**: 各辺 10 studs 以下
- **MeshPart 最大**: 各辺 2048 studs (実用上は数十 studs)
- **コリジョン**: MeshPart には自動コリジョンが生成される (ConvexDecomposition)

### メッシュ品質要件

| 要件 | 詳細 |
|------|------|
| 三角形化 | 必須。n-gon 禁止 |
| デジェネレート面 | 面積ゼロの三角形禁止 |
| 法線一貫性 | 裏返し法線禁止 |
| 重複頂点 | 同一座標の重複は許容（UV 境界等） |
| 非多様体エッジ | 極力避ける |
| 座標系 | Y-up 右手系 |

### テクスチャ

| 項目 | 要件 |
|------|------|
| フォーマット | PNG (推奨), TGA, BMP |
| 解像度 | 1024x1024 以下推奨 |
| PBR | SurfaceAppearance: ColorMap, MetalnessMap, NormalMap, RoughnessMap |
| UV | 標準 UV 展開。Triplanar は Roblox 非対応 |

---

## `roblox_export` モジュール API 仕様

### `RobloxConfig`

```rust
pub struct RobloxConfig {
    /// メッシュ解像度 (Marching Cubes グリッド数)
    pub resolution: usize,            // default: 128

    /// SDF バウンディングボックス
    pub bounds_min: Vec3,             // default: (-2, -2, -2)
    pub bounds_max: Vec3,             // default: (2, 2, 2)

    /// SDF 単位 → stud 変換スケール
    /// 1.0 SDF unit = scale_studs studs
    pub scale_studs: f32,             // default: 2.0

    /// 三角形数上限
    pub max_triangles: usize,         // default: 4_000

    /// バウンディングボックス上限 (studs)
    pub max_size_studs: Vec3,         // default: (10, 10, 10)
}
```

**プリセット**:

| プリセット | resolution | max_triangles | 用途 |
|-----------|-----------|---------------|------|
| `accessory()` | 128 | 4,000 | UGC アクセサリー |
| `meshpart()` | 192 | 10,000 | 汎用 MeshPart |
| `preview()` | 64 | 4,000 | 高速プレビュー |

### バリデーション

```rust
pub struct RobloxValidation {
    pub triangle_count: usize,
    pub vertex_count: usize,
    pub bounds_studs: Vec3,           // stud 単位の実サイズ
    pub is_within_triangle_limit: bool,
    pub is_within_size_limit: bool,
    pub has_degenerate_faces: bool,
}

pub fn validate_for_roblox(mesh: &Mesh, config: &RobloxConfig) -> RobloxValidation;
```

### エクスポート関数

```rust
// SdfNode → OBJ (Roblox 制約適用済み)
pub fn node_to_obj_roblox(
    node: &SdfNode,
    path: impl AsRef<Path>,
    config: &RobloxConfig,
) -> Result<RobloxExportStats, ExportError>;

// SdfNode → FBX (Roblox 制約適用済み)
pub fn node_to_fbx_roblox(
    node: &SdfNode,
    path: impl AsRef<Path>,
    config: &RobloxConfig,
) -> Result<RobloxExportStats, ExportError>;

// LOL テキスト → OBJ (LLM 連携用)
pub fn lol_to_obj_roblox(
    lol_text: &str,
    path: impl AsRef<Path>,
    config: &RobloxConfig,
) -> Result<RobloxExportStats, ExportError>;

// LOL テキスト → FBX (LLM 連携用)
pub fn lol_to_fbx_roblox(
    lol_text: &str,
    path: impl AsRef<Path>,
    config: &RobloxConfig,
) -> Result<RobloxExportStats, ExportError>;
```

### エクスポート統計

```rust
pub struct RobloxExportStats {
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub bounds_studs: Vec3,
    pub path: String,
    pub validation: RobloxValidation,
}
```

---

## メッシュデシメーション戦略

ALICE-SDF の Marching Cubes は解像度に応じて三角形を生成する。
Roblox の上限 (4,000 / 10,000) に収めるため:

1. **解像度調整**: resolution を下げることで三角形数を概算制御
   - 64³ → ~数千三角形
   - 128³ → ~数万三角形
   - 目標三角形数に応じて resolution を自動計算

2. **MeshRepair**: Marching Cubes のアーティファクト除去
   - デジェネレート面除去
   - 重複頂点マージ (epsilon = 5e-3)

3. **将来**: QEM (Quadric Error Metrics) デシメーション追加
   - 形状精度を保ちながら三角形数を指定値に削減

---

## 座標系変換

| システム | Up | 手系 | スケール |
|---------|-----|------|---------|
| ALICE-SDF | Y-up | 右手 | 任意 (通常 [-5, 5]) |
| Roblox | Y-up | 右手 | studs (1 stud ≈ 0.28m) |

ALICE-SDF → Roblox は**座標系変換不要** (同一)。スケーリングのみ実施。

---

## 使用例

### Rust (compile-time)

```rust
use alice_lol::lol;
use alice_lol::roblox_export::{RobloxConfig, node_to_obj_roblox};

let node = lol! {
    smooth_union(0.3,
        sphere(1.0),
        translate(0.0, 1.5, 0.0,
            scale(0.6, sphere(1.0))
        )
    )
};

let config = RobloxConfig::accessory();
let stats = node_to_obj_roblox(&node, "snowman_hat.obj", &config)?;
println!("{stats}");
// snowman_hat.obj: 2,340 vertices, 3,812 triangles (bounds: 2.6 x 4.2 x 2.6 studs)
```

### Rust (runtime / LLM 連携)

```rust
use alice_lol::roblox_export::{RobloxConfig, lol_to_fbx_roblox};

let lol_text = r#"
smooth_union(0.2,
    sphere(1.0),
    translate(0.0, -1.2, 0.0, cone(0.8, 1.5))
)
"#;

let stats = lol_to_fbx_roblox(lol_text, "wizard_hat.fbx", &RobloxConfig::accessory())?;
```

### Roblox Studio インポート手順

1. Roblox Studio を開く
2. `Home` → `Import 3D` (or `Insert` → `MeshPart`)
3. 生成した OBJ/FBX ファイルを選択
4. `Scale` を確認 (1 unit = 1 stud になっているか)
5. `CollisionFidelity` を `PreciseConvexDecomposition` に設定
6. テクスチャが必要な場合は `SurfaceAppearance` を追加

---

## 将来拡張 (フェーズ A/C)

### フェーズ C: Blender ブリッジ

```
ALICE-SDF → OBJ/glTF
  ↓ Blender headless (bpy)
  ↓ Roblox R15 テンプレートにフィット
  ↓ スキニング + ケージ生成
FBX (リグ付き)
  ↓ Roblox Studio
フルアバター
```

### フェーズ A: フルカスタムアバター

- R15 スケルトン自動生成
- SDF → 15 パーツ空間分割
- Inner/Outer ケージ (オフセットメッシュ)
- FACS ブレンドシェイプ (50+)
- スキニング付き FBX 出力

### Roblox Studio MCP 自動化

```rust
// MCP 経由で Roblox Studio に直接インポート
// mcp__roblox-studio__insert_model を使用
// OBJ/FBX → AssetService:CreateMeshPartAsync()
```
