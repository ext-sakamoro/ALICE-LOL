# ALICE-LOL Humanoid Template Roadmap

新 crate `alice-lol-humanoid` を satellite として切り出し、VRM humanoid の LOL DSL テンプレート化を段階的に実装するロードマップ

## 背景

- ALICE-Manga `src/skeleton3d.rs` (937 行) が既に VRM → `SdfNode` の canonical converter として完成 (Phase α / Phase β 実装済、Alice VRM 実 PoC empirical 実証済)
- alice-lol は `SdfNode` を中核型として持ち、GLSL/WGSL/HLSL transpile + `intent` module (Milestone B.1 skeleton) を備える
- 三相原理 Milestone B (Intent スケルトン) の hero use case として humanoid template が最短距離
- 詳細背景は `HUMANOID_TEMPLATE_DESIGN.md` §1-2 参照

## 選択済 判断軸 (2026-08-06 確定)

| 軸 | 選択 | 理由 |
|--|--|--|
| Q1 対象範囲 | humanoid のみ | MVP 最短、四足獣 / mech は姉妹 crate で後段 |
| Q2 パラメータ化深度 | 骨格比パラメータ化 | 静的だけだと template 名に値せず、身長 / 腕長 / 頭身の最小 morphology parameter が必須 |
| Q3 格納 crate | 新 crate `alice-lol-humanoid` | LOL 本体 pure 維持、姉妹 crate 拡張路線を確保 |
| Q4 入力 format | VRM + BVH | 静的 skeleton + 動的 motion の両輪、Mixamo / CMU 資産流用可 |
| Q5 三相原理位置 | Phase 2 Law → 段階的に Phase 3 Intent | H.0-H.4 で Phase 2、H.6 で Phase 3 Intent 結線 |

## Phase 一覧

MVP = H.0 - H.4 (合計 26-44 h、~1 週間 sprint) H.5 は user 判断待ち、H.6 は Milestone B との合流で別 Sprint

### H.0 Scaffolding

**scope**: workspace member 追加 + crate 骨格

- `~/ALICE-LOL/alice-lol-humanoid/` 新規 dir
- `Cargo.toml` (name / version 0.1.0 / license MIT OR Apache-2.0 / dep: alice-lol path)
- `src/lib.rs` (module tree + placeholder `HumanoidTemplate` struct)
- workspace `Cargo.toml` の `members` に追加
- smoke test 1 個 (`cargo build -p alice-lol-humanoid` green)

**成功基準**
- `cargo build -p alice-lol-humanoid` エラーなし
- `cargo test -p alice-lol-humanoid` smoke test pass
- `cargo clippy -p alice-lol-humanoid -- -W clippy::pedantic -W clippy::nursery` 0 warning

**想定工数**: 2-4 h

### H.1 Static template

**scope**: 静的 humanoid template を LOL 独自実装として持つ

- ALICE-Manga `skeleton3d.rs` の `Skeleton3D::humanoid_default()` 相当 logic を移植 (code copy、依存反転しない、H.5 で再検討)
- `HumanoidTemplate::default()` factory (canonical T-pose、16 joint / 15 bone、Y up 右手系)
- `HumanoidTemplate::to_sdf(k)` method (Capsule chain + SmoothUnion tree で `SdfNode` 生成)
- `examples/humanoid_default.rs` (SdfNode を GLSL transpile して stdout 出力、及び CPU rasterize で PNG 出力)
- unit test 5-8 個 (16 joint 数 / Y up 検証 / to_sdf tree shape / factory 幾何関係 等)

**成功基準**
- `cargo run -p alice-lol-humanoid --example humanoid_default` で SdfNode 生成 + PNG 出力成功
- unit test 全 pass、clippy 0 warning

**想定工数**: 4-8 h

### H.2 Parametric morphology

**scope**: 骨格比 parameter による morphology 制御

- `MorphologyParams` struct 定義
  - `height`: 全身高 (default 1.7m)
  - `head_body_ratio`: 頭身 (2.0 - 8.0、default 7.5)
  - `arm_ratio`: 腕長 / 身長 (default 0.4)
  - `shoulder_ratio`: 肩幅 / 身長 (default 0.22)
  - `leg_ratio`: 脚長 / 身長 (default 0.53)
  - (追加余地: 頸長 / 骨盤幅 / etc)
- builder pattern (`HumanoidTemplate::builder().height(2.0).head_body_ratio(3.0).build()`)
- morphology → joint position 算出 logic (chibi 3 頭身 / real 7-8 頭身 / hero 8 頭身)
- `examples/humanoid_morphology.rs` (3 頭身 vs 8 頭身 の PNG 描き分け)
- unit test 5-8 個

**成功基準**
- example で 3 頭身 vs 8 頭身の PNG が視認可能に描き分けられる
- morphology parameter を変えると `to_sdf()` 結果が期待通り変化する unit test pass

**想定工数**: 8-12 h

### H.3 VRM import

**scope**: 実 VRM file からの HumanoidTemplate 生成

- Cargo.toml に optional dep 追加: `gltf` 1.x + `serde_json` (`vrm` feature)
- ALICE-Manga `vrm_import.rs` の `VrmFile::extract_humanoid_bones()` 相当 logic を移植 or 抽出 (code copy first、H.5 で共通化検討)
- `HumanoidTemplate::from_vrm(path)` method (VRM 15 bone → joint position 変換、`from_vrm_bones()` 相当)
- Alice VRM (`~/CTW-sakamoto/*` は隔離対象なので non-secret な local test asset を選定要) 相当の integration test
- `examples/humanoid_from_vrm.rs` (env `VRM_PATH` で任意 VRM を読み込み PNG 出力)

**成功基準**
- 実 VRM から HumanoidTemplate 生成成功、`to_sdf()` で SdfNode 生成
- integration test (VRM_PATH env 経由) で PNG 生成
- feature `vrm` off の場合ビルド green (optional dep 分離検証)

**想定工数**: 4-8 h

### H.4 BVH import + pose

**scope**: 動的 pose 変形

- ALICE-Manga `bvh_import.rs` の pure Rust BVH parser (773 行) を移植 (dep 無しなので拾いやすい)
- `mixamo_to_vrm()` / `cmu_to_vrm()` bone map converter 移植
- `HumanoidTemplate::apply_pose(&BvhFrame)` method (FK 適用、`from_vrm_bones_with_pose()` 相当)
- `examples/humanoid_bvh_animation.rs` (Alice VRM + Mixamo walk BVH の複数 frame PNG 出力、animation GIF 化は次段)
- unit test (Identity rotation で bind pose 再現 / 単一 rotation の期待座標 / hips propagation 等 5-8 個)

**成功基準**
- Alice VRM + Mixamo BVH で複数 frame の SdfNode 生成
- unit test 全 pass、FK 数値検証 pass

**想定工数**: 8-12 h

**MVP 到達**: H.0-H.4 完了時点で「3D モデル (VRM) + motion (BVH) → LOL テンプレート化された `SdfNode`」の canonical pipeline 完成

### H.5 ALICE-Manga との duplication 整理

**scope**: Manga 側 `skeleton3d.rs` と LOL 側 `alice-lol-humanoid` の code 重複を解消するか、併存するかを決定

**3 案 (要 user 判断、Sprint 前議論)**

| 案 | 内容 | pros | cons |
|--|--|--|--|
| **A. 併存** | 双方独立で維持、code duplication 許容 | 相互影響なし、Manga の 2D + 3D 統合 API を保持 | duplication rot リスク、bug fix 両方要 |
| **B. Manga → LOL wrapper 化** | Manga skeleton3d を alice-lol-humanoid re-export の wrapper に降格 | duplication 解消、canonical source が LOL 側に一本化 | Manga の dep 追加、既存 API 微調整 |
| **C. 段階移行** | まず H.0-H.4 は併存 (案 A)、Manga 側 next major version で B に移行 | 短期リスクゼロ、長期 clean | 中間期に duplication 発生、移行 timeline 要 |

**成功基準**: user が A/B/C いずれかを選択、選択に沿った実装 or 記録完了

**想定工数**: 議論 30-60 min + 案 B 選択時に追加 2-4 h

### H.6 Intent 結線 (別 Sprint、三相原理 Milestone B との合流)

**scope**: Phase 3 Intent 相 IR (`alice_lol::intent`) と HumanoidTemplate の結線

- `HumanoidTemplate::apply_intent(&IntentNode)` method
- L1 Physical Intent verb 定義 (「立つ」= StandUp / 「歩く」= Walk / 「腕を上げる」= RaiseArm / 「掴む」= Grasp 等 14 verb)
- 各 verb → template.apply_pose() 経由での SdfNode 変形実装
- `examples/humanoid_intent_verb.rs` (8-byte Intent packet → humanoid pose 変形 → SdfNode → PNG)
- Milestone B (Intent スケルトン) の hero example として機能

**成功基準**
- 8-byte Intent packet で humanoid pose 変形実証
- 三相原理 Phase 3 Intent の 10,000× 圧縮 narrative の proof-of-concept
- `project_alice_lol_ir_roadmap` Milestone B.4 の acceptance criteria 満たす

**想定工数**: 12-20 h (別 Sprint)

## 依存関係と license

- 本 crate: MIT OR Apache-2.0 (LOL 本体と揃える)
- `alice-lol` path dep (必須)
- `gltf` 1.x (optional、feature `vrm` 経由、Apache-2.0 OR MIT)
- `serde_json` (optional、feature `vrm` 経由、Apache-2.0 OR MIT)
- BVH parser は pure Rust 内蔵 (dep 追加なし)

`llm-bridge` 等の AGPL propagate なし、default で MIT OR Apache-2.0 のまま

## 三相原理 Milestone との対応

| 三相原理 Milestone | 本 Roadmap Phase | 内容 |
|--|--|--|
| A (Law 完成) | H.0-H.2 | Static + parametric template = Phase 2 Law の parametric 化 |
| B (Intent スケルトン) | H.3-H.6 | VRM/BVH ingest → Intent verb 受け入れ、Phase 3 入口 |
| C (統合、8-byte packet 実運用) | H.6 以降 | Kinematics との合流、実 game / metaverse で稼働 |

詳細: `[[project_alice_lol_ir_roadmap]]` (~/.claude memory)

## 進捗記録

- 2026-08-06 Roadmap 制定 (Q1-Q5 判断確定、H.0 未着手)
- 進捗詳細は memory `project_alice_lol_humanoid_template.md` にトラッキング
