# ALICE-LOL Humanoid Template Design

`alice-lol-humanoid` crate の設計仕様 実装フェーズは `HUMANOID_TEMPLATE_ROADMAP.md` 参照

## 1. 三相原理での位置付け

ALICE エコシステム 三相原理 (Data → Law → Intent) における本 crate の役割

| Phase | 送るもの | 本 crate との関係 |
|--|--|--|
| Phase 1 Data | mesh polygon (VRM の bind pose mesh) | VRM file を入力として受け取るが、mesh 自体は runtime に届けない |
| **Phase 2 Law** | **SDF 数式 + parametric morphology** | **本 crate の主戦場、`HumanoidTemplate` = parametric law** |
| **Phase 3 Intent** | **8-byte Intent packet** | **H.6 で `apply_intent(&IntentNode)` として結線** |

canonical narrative
- VRM (Phase 1 Data、~MB) → `HumanoidTemplate` (Phase 2 Law、~KB parameter) → Intent verb (Phase 3、8 byte) の 3 段圧縮
- 受信側は Intent + template だけで SdfNode を再構築、runtime 描画

`[[feedback_alice_identity_law_not_data]]` の「データを送るな、法則を送れ」を humanoid character 領域に適用したもの

## 2. Crate boundary

```
┌──────────────────────────────────────────┐
│ downstream consumer                       │
│ (games / metaverse / manga / education)   │
└─────────────┬────────────────────────────┘
              │ use alice_lol_humanoid::HumanoidTemplate;
              ▼
┌──────────────────────────────────────────┐
│ alice-lol-humanoid (本 crate)             │
│   HumanoidTemplate                        │
│   MorphologyParams                        │
│   VrmImport (feature = "vrm")             │
│   BvhImport (pure Rust)                   │
│   IntentAdapter (H.6)                     │
└─────────────┬────────────────────────────┘
              │ use alice_lol::{SdfNode, to_wgsl, intent};
              ▼
┌──────────────────────────────────────────┐
│ alice-lol (satellite host)                │
│   SdfNode re-export / GLSL/WGSL/HLSL      │
│   intent module (Phase 3 IR)              │
│   stdlib::hardsurface (前例)              │
└─────────────┬────────────────────────────┘
              │ use alice_sdf::SdfNode;
              ▼
┌──────────────────────────────────────────┐
│ alice-sdf (中核)                          │
│   SdfNode 71 primitive + 23 CSG + ...     │
└──────────────────────────────────────────┘
```

- alice-sdf は透過依存 (alice-lol 経由)
- 姉妹 crate 拡張路線: 将来 `alice-lol-quadruped` / `alice-lol-mech` / `alice-lol-creature` を同構造で追加可能
- alice-manga との関係は Phase H.5 で決定 (A 併存 / B wrapper 化 / C 段階移行)

## 3. API 表面 draft

### 3.1 中核型

```rust
pub struct HumanoidTemplate {
    pub joints: HashMap<Joint, [f32; 3]>,
    pub bones: Vec<Bone>,
    pub morphology: MorphologyParams,
}

pub struct MorphologyParams {
    pub height: f32,           // 全身高 (m、default 1.7)
    pub head_body_ratio: f32,  // 頭身 (2.0-8.0、default 7.5)
    pub arm_ratio: f32,        // 腕長 / 身長 (default 0.4)
    pub shoulder_ratio: f32,   // 肩幅 / 身長 (default 0.22)
    pub leg_ratio: f32,        // 脚長 / 身長 (default 0.53)
}

pub enum Joint {
    Head, Neck, Chest, Waist,
    LShoulder, RShoulder, LElbow, RElbow, LWrist, RWrist,
    LHip, RHip, LKnee, RKnee, LAnkle, RAnkle,
}

pub struct Bone {
    pub from: Joint,
    pub to: Joint,
    pub thickness: f32,
}
```

### 3.2 factory + conversion

```rust
impl HumanoidTemplate {
    // H.1
    pub fn default() -> Self;

    // H.2
    pub fn builder() -> HumanoidTemplateBuilder;
    pub fn with_morphology(morphology: MorphologyParams) -> Self;

    // H.3
    #[cfg(feature = "vrm")]
    pub fn from_vrm(path: impl AsRef<Path>) -> Result<Self, VrmImportError>;

    // H.4
    pub fn apply_pose(&mut self, pose: &BvhFrame);

    // H.6 (別 Sprint)
    pub fn apply_intent(&mut self, intent: &alice_lol::intent::IntentNode);

    // 全 Phase 共通、SdfNode 生成
    pub fn to_sdf(&self, smoothness_k: f32) -> SdfNode;
}
```

### 3.3 example usage

```rust
use alice_lol_humanoid::HumanoidTemplate;

// H.1 静的
let template = HumanoidTemplate::default();
let sdf = template.to_sdf(0.15);

// H.2 parametric
let template = HumanoidTemplate::builder()
    .height(1.5)
    .head_body_ratio(3.0)  // chibi 3 頭身
    .build();

// H.3 VRM
#[cfg(feature = "vrm")]
let template = HumanoidTemplate::from_vrm("alice.vrm")?;

// H.4 BVH pose
let bvh = BvhFile::load("walk.bvh")?;
let mut template = HumanoidTemplate::from_vrm("alice.vrm")?;
template.apply_pose(&bvh.frame(30));
let sdf = template.to_sdf(0.15);

// H.6 Intent (別 Sprint)
let intent = IntentNode::Verb(Verb::RaiseArm { side: Side::Left, angle: 90.0 });
template.apply_intent(&intent);
```

## 4. 依存戦略

| dep | version | 種別 | feature gate | license |
|--|--|--|--|--|
| `alice-lol` | path 0.1 | 必須 | — | MIT OR Apache-2.0 |
| `glam` | 0.29 | 必須 | — | MIT OR Apache-2.0 |
| `gltf` | 1.x | optional | `vrm` | Apache-2.0 OR MIT |
| `serde_json` | 1.x | optional | `vrm` | Apache-2.0 OR MIT |
| BVH parser | pure Rust 内蔵 | 必須 | — | 本 crate と同 (MIT OR Apache-2.0) |

`llm-bridge` 等 AGPL propagate なし
default features: なし (`vrm` は opt-in、pure な static + BVH のみで動く最小構成を default に)

## 5. Test 戦略

### 5.1 unit test

各 Phase 5-8 個
- H.1: joint 数 / Y up / to_sdf tree shape / factory 幾何関係
- H.2: morphology 適用結果 / builder chain / boundary value (2 頭身 / 8 頭身)
- H.3: VRM 15 bone mapping / missing bone (head 欠落) / upperChest fallback
- H.4: BVH parse / Identity rotation で bind pose 再現 / 単一 rotation の期待座標 / hips propagation
- H.6: Intent verb → pose 変換の期待値

### 5.2 doctest

各 public API に minimal doctest 1 個 (Quick start snippet)

### 5.3 example

各 Phase に対応 example 1 個 (Roadmap §H.1-H.4 参照)、CPU rasterize で PNG 出力し `examples/output/` に配置

### 5.4 karikari-review Rust 準拠

- `cargo clippy -p alice-lol-humanoid --all-targets --all-features -- -W clippy::pedantic -W clippy::nursery` 0 warning
- `cargo fmt --check` clean
- `RUSTFLAGS='-Dwarnings'` build green
- 仮実装 grep 罠回避 (`unwrap!` / `unimplemented!` / `todo!` / `panic!.*stub` は 0)
- 詳細は `~/claude-config/claude-skills/karikari-review/SKILL.md` (Rust)

## 6. ALICE-Manga との関係

- 現状: `~/ALICE-Manga/src/skeleton3d.rs` (937 行) が同等 logic 実装済
- 本 crate は Phase H.0-H.4 で **code copy** で立ち上げる (依存反転しない、変更影響を最小化)
- Phase H.5 で 3 案検討 (A 併存 / B Manga → LOL wrapper 化 / C 段階移行)
- 決定は user 判断、当面は併存想定

## 7. License

- crate: MIT OR Apache-2.0
- LOL 本体と揃える (dual license の下、下流に選択権を与える)
- AGPL propagate は default features では発生させない

## 8. 拡張路線 (将来)

同構造で以下の姉妹 crate を切り出す想定

| crate | 対象 | 主 use case |
|--|--|--|
| `alice-lol-humanoid` | 人型 | game character / metaverse avatar / manga |
| `alice-lol-quadruped` | 四足獣 | 犬 / 猫 / ドラゴン / モンスター |
| `alice-lol-mech` | 機械 / 乗り物 | robot / vehicle / rig |
| `alice-lol-creature` | 妖怪 / 異形 | 触手 / 多脚 / 変形生物 |

いずれも `SdfNode` 中核型を共有、alice-lol の transpile 層と Intent 層を再利用

## 9. 参考

- `[[project_alice_lol_humanoid_template]]` 進捗 memory
- `[[project_alice_lol_ir_roadmap]]` 三相原理 Milestone A-C
- `[[project_alice_cognitive_meta_intent]]` L3 Architectural Intent 姉妹
- `[[feedback_alice_identity_law_not_data]]` データを送るな法則を送れ
- ALICE-Manga `src/skeleton3d.rs` (canonical Reference 実装)
- ALICE-Manga `docs/CHARACTER_AUTHORING_PIPELINE.md` (Phase α / β empirical 実測)
