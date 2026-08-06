# ALICE-LOL Complete Pipeline Specification (Phase 5)

**Status**: Phase 5.0 (仕様書) 完成、Phase 5.1-5.6 実装待ち
**作成日**: 2026-08-06
**関連 memory**: [[feedback_alice_polygon_extrude_data_route]] / [[feedback_alice_identity_law_not_data]]

## 1. 目的

**text-to-print user が自然言語で入力 → LLM が LOL DSL 生成 → alice-lol/sdf pipeline → Bambu Studio 読込可能な `.3mf` → 実プリント成功**、までの完全パイプラインを Rust workspace で完結させる

ALICE 三相原理 Phase 2 Law 経路 (SDF+DC) に統一 (Phase 4 で earcutr Data 経路撤廃済)

## 2. Pipeline Flow (完成後)

```
User Input (自然言語)
  例: "SKADIS 300mm panel を作って" / "100 円型 coin"
  ↓
text-to-print GUI (crates/app、egui + wgpu SDF viewer)
  ↓
Embedded LLM (Qwen 3.5-4B Q4_K_M / Bonsai 27B Q1_0)
  ↓  system prompt = LLM_PRINT_PROMPT.md (Step 1.5 厚さ判定 hint 含む)
  ↓  LoRA fine-tune (523+ samples + Phase 5 追加 pattern)
LOL DSL text
  例: "skadis_panel(300, 5, 5)" / "shopping_cart_coin(22.8, 1.7)"
  ↓
alice_lol::runtime_parser::parse_lol  ← Phase 5.1 で primitive 登録拡張
  ↓
SdfNode tree (alice_lol::stdlib::hardsurface::skadis_sdf / thin_sdf 等が対応)
  ↓
alice_lol::print_export::node_to_3mf_dual_contouring  ← Phase 3'' 実装済
  ↓
alice_sdf::mesh::dual_contouring (Hermite data、非多様体 0 保証)
  ↓
alice_sdf::io::export_3mf
  ↓
.3mf file
  ↓
User: Bambu Studio で開く (user 検証、Phase 5.6 手順書)
  ↓
Slice → G-code → Print
```

## 3. Phase 5 実装項目

### Phase 5.0: 本仕様書 (COMPLETED)

- `docs/PIPELINE_COMPLETE.md` 新規

### Phase 5.1: LOL `runtime_parser` に primitive 登録

**目的**: LLM が LOL DSL text で `skadis_panel(300, 5, 5)` のように 1 word で書ける
**現状**: Phase 3'' / 3''.2 / 3''.3.1 で Rust API (`skadis_panel_sdf` 等) は追加済、ただし `runtime_parser` の primitive dispatch table に未登録 = LLM が text で呼び出せない

**実装対象 (5 primitive)**:
- `skadis_panel(size, thickness, corner_radius) -> SdfNode` = `skadis_sdf::skadis_panel_sdf`
- `shopping_cart_coin(diameter, thickness) -> SdfNode` = `thin_sdf::shopping_cart_coin_sdf`
- `skadis_hook_l() -> SdfNode` = `skadis_sdf::skadis_hook_l_sdf`
- `skadis_hook_j() -> SdfNode` = 同上
- `skadis_hook_s() -> SdfNode` = 同上

**追加検討 (Phase 5.2 完成後)**:
- `skadis_container()` / `skadis_clip()` / `skadis_shelf()` / `skadis_elastic_cord()`

**成功条件**:
- `parse_lol("skadis_panel(300, 5, 5)")` が `SdfNode::Subtraction { ... }` を返す
- unit test: 各 primitive の parse 成功 + `alice_sdf::eval` で内側/外側判定
- `lol.gbnf` (LLM Guided Generation grammar) に新 primitive keyword 追加
- `LLM_REFERENCE.md` の primitive リストに追記

### Phase 5.2: SKADIS 残 4 accessory の SDF 実装

**現状**: Phase 3''.3.1 で hook 3 種は実装済、残り 4 accessory 未実装
**実装対象**:

| primitive | Bamboo canonical | 主要 shape 要素 |
|--|--|--|
| `skadis_container_sdf` | `skadis-container/generate.py` | container 板組 (底 + 4 側壁) + 2 peg + gusset 補強リブ |
| `skadis_clip_sdf` | `skadis-clip/generate.py` | 二股構造 (2 板 + tip bulge、Bamboo 論理: 差込 slot 式) |
| `skadis_shelf_sdf` | `skadis-shelf/generate.py` | 板 + rib 補強 + 2 peg |
| `skadis_elastic_cord_sdf` | `skadis-elastic-cord/generate.py` | ring holder + peg + 弾性コード用凹み |

**実装方針**:
- `skadis_peg_and_shoulder(hook_width)` helper (Phase 3''.3.1 追加) を再利用
- `capsule_polyline_sdf` (曲線 sweep) を必要に応じて活用
- container / shelf は Box3d 組立中心、clip は 2 板 + tip bulge、elastic_cord は Torus + Box3d
- テーパー / fillet は本 module で省略 (DC の Hermite で自然滑らか)

**成功条件**:
- 各 primitive の unit test (peg 位置内部、accessory 主要部内部)
- eval で shape 妥当性確認

### Phase 5.3: 全 12+ 品目の 3MF 生成 example

**目的**: Phase 5.1-5.2 完成後、LOL Rust API 経由で全 12+ 品目を `.3mf` 生成、実測ログ付き
**新規 example**: `examples/complete_pipeline_output.rs`

**出力先**:
```
./output/
├── thin/                             # DC 経路 (SDF+Dual Contouring)
│   ├── shopping_cart_coin_100yen.3mf
│   ├── skadis_panel_300x300.3mf
│   ├── skadis_hook_l.3mf
│   ├── skadis_hook_j.3mf
│   ├── skadis_hook_s.3mf
│   ├── skadis_container.3mf         # Phase 5.2 実装後
│   ├── skadis_clip.3mf
│   ├── skadis_shelf.3mf
│   └── skadis_elastic_cord.3mf
└── thick/                            # SDF+MC 経路
    ├── wall_hook.3mf
    ├── gridfinity_bin_2x2.3mf
    ├── drawer_organizer_chopsticks_set.3mf
    └── shelf_divider_560x250x120.3mf
```

**実行時出力**:
- 各品目に対して: vertices / triangles / non_manifold_edges / 実行時間 / 3MF file size
- 全 12+ 品目の合計統計 (total 3MF size、total time)
- 期待失敗ケースなし (全 success で終了)

**成功条件**:
- 全 12+ 品目の `.3mf` が生成される
- 全品目で `non_manifold_edges == 0` (repair 込み)
- ファイルサイズ 100MB 未満 (実用範囲、Bambu Studio が扱える)

### Phase 5.4: text-to-print pipeline.rs の DC 経路切替

**現状**: text-to-print `crates/core/src/pipeline.rs::export_3mf_via_bamboo` は SDF+MC hardcode
**修正内容**:
- 厚さ判定 helper 追加: SdfNode の Y 軸方向 AABB を評価、`< 5mm` なら DC 経路、`>= 5mm` なら MC 経路
- `export_3mf_via_dual_contouring` 新規関数追加 (MC 版と対を成す)
- `ExportFormat::ThreeMf` match arm で厚さ判定 → 分岐
- または: user が明示的に経路指定できる `Quality::Thin` variant 追加

**成功条件**:
- text-to-print pipeline test で 12+ 品目 (LOL DSL text 経由) が全て 3MF 生成成功
- 厚さ自動判定が MC/DC を正しく振り分ける (coin → DC、bracket → MC)

### Phase 5.5: Bamboo 4 generator CLI 実行検証

**現状**: Phase B.1.d で 4 generator を LOL pattern_sdf 呼出し wrapper に置換、しかし CLI 実行後の .3mf 生成成功は未確認
**実装**:
- `scripts/verify_bamboo_generators.sh` 新規 bash script:
  ```bash
  cargo run --release --bin alice-bamboo -- drawer --width 250 --depth 200 --height 40 --slots "chopsticks:2,fork:4,knife:4" --output /tmp/verify/drawer.3mf
  cargo run --release --bin alice-bamboo -- gridfinity --units 2x2 --height 4 --output /tmp/verify/gridfinity.3mf
  cargo run --release --bin alice-bamboo -- hook --load 3 --mount screw --output /tmp/verify/hook.3mf
  cargo run --release --bin alice-bamboo -- shelf-divider --width 560 --depth 250 --height 120 --output-dir /tmp/verify/
  # skadis は Phase B.1.c で ALICE 準拠検証済、Rust SDF 版は既存維持
  cargo run --release --bin alice-bamboo -- skadis --size 300 --output /tmp/verify/skadis_panel.3mf
  ```
- 4 CLI 実行成功 + 3MF 生成確認

**成功条件**:
- 全 CLI 実行が exit 0
- 全 3MF ファイル生成 (size > 0)
- `alice_sdf::mesh::validate_mesh` で non_manifold_edges 0 (repair 込み)

### Phase 5.6: user Bambu Studio 検証手順書

**新規 docs**: `docs/BAMBU_STUDIO_VERIFICATION.md`
**内容**:
- 12+ 品目の `.3mf` file 一覧 (出力パス + 説明 + 想定 slice 時間 + 想定 filament 消費)
- 各 file の Bambu Studio 検証チェックリスト:
  - [ ] File > Import で開いてエラーなし
  - [ ] 3D view で mesh 表示、warning icon なし
  - [ ] Prepare tab で "Slice all" 成功
  - [ ] G-code preview で全 layer 表示可能
  - [ ] slice 時間 / filament 消費が想定範囲
  - [ ] 実プリント時の注意点 (bed 位置、support 要否、印刷向き)
- 問題発生時の trouble shooting (非多様体エッジ / thin wall warning / build volume 超過 等)
- `~/CLAUDE.md` § 「3Dプリント出力バリデーションフロー」への reference

## 4. 12+ 品目リスト (完全定義)

### 4.1 薄物 (DC 経路、Phase 3''+3''.2+3''.3.1-5.2)

| No | 品目 | サイズ | 想定 tri (DC 128) | 用途 |
|--|--|--|--|--|
| 1 | shopping_cart_coin_100yen | Φ22.8 × 1.7mm | 105,660 | ショッピングカート用コイン |
| 2 | skadis_panel_300x300 | 300×300×5mm | 172,194 | IKEA SKADIS 互換ペグボード |
| 3 | skadis_hook_l | reach 75mm × 8mm 厚 | 実測予定 | 2-peg 水平フック (5kgf) |
| 4 | skadis_hook_j | reach 25mm + drop 70mm | 実測予定 | J 字深フック (3kgf) |
| 5 | skadis_hook_s | reach 22mm + drop 45mm | 実測予定 | S 字汎用フック (1kgf) |
| 6 | skadis_container | (Phase 5.2 実装後) | 未実測 | 小物入れ 2-peg |
| 7 | skadis_clip | (Phase 5.2 実装後) | 未実測 | 差込 slot 式 clip |
| 8 | skadis_shelf | (Phase 5.2 実装後) | 未実測 | 2-peg 棚 |
| 9 | skadis_elastic_cord | (Phase 5.2 実装後) | 未実測 | 弾性コードホルダー |

### 4.2 厚物 (MC 経路、Phase B.1.b)

| No | 品目 | サイズ (default) | 用途 |
|--|--|--|--|
| 10 | wall_hook | 荷重逆算、backplate 中規模 | 壁掛けフック (荷重 kgf 指定) |
| 11 | gridfinity_bin | 2×2×4U (84×84×32.75mm) | Gridfinity 42mm grid bin |
| 12 | drawer_organizer | 250×200×40mm | 引出し仕切り (chopsticks/fork/knife/etc) |
| 13 | shelf_divider_560x250x120 | 560×250×120mm | U 字棚仕切り (hex cutout 底板) |

## 5. LOL DSL 完全 primitive リスト (Phase 5.1 完成後の状態)

### 既存 (2026-08-05 時点、runtime_parser.rs 実装済)
- Primitive (71): sphere, box3d, rounded_box, cylinder, torus, cone, capsule, ellipsoid, plane, octahedron, rounded_cone, pyramid, hex_prism, link, capped_cone, capped_torus, rounded_cylinder, tube, barrel, heart, egg, helix, tetrahedron, box_frame, diamond, star_polygon, cross_shape, triangle, bezier, triangular_prism, cut_sphere, cut_hollow_sphere, death_star, solid_angle, rhombus, horseshoe, vesica, infinite_cylinder, infinite_cone, gyroid, chamfered_cube, schwarz_p, superellipsoid, rounded_x, pie, trapezoid, parallelogram, tunnel, uneven_capsule, arc_shape, moon, blobby_cross, parabola_segment, regular_polygon, stairs_prim, dodecahedron, icosahedron, truncated_octahedron, truncated_icosahedron, diamond_surface, neovius, lidinoid, iwp, frd, fischer_koch_s, pmy, circle_2d, rect_2d, segment_2d, rounded_rect_2d, annular_2d
- Operations (23): union, smooth_union, intersection, smooth_intersection, subtract, smooth_subtract, chamfer_union, chamfer_intersection, chamfer_subtraction, stairs_union, stairs_intersection, stairs_subtraction, xor, pipe, engrave, groove, tongue, columns_union, columns_intersection, columns_subtraction, exp_smooth_union, exp_smooth_intersection, exp_smooth_subtraction
- Transforms (4): translate, rotate, scale, scale_non_uniform
- Modifiers (20): round, onion, twist, bend, mirror, repeat, elongate, revolution, extrude, taper, displacement, polar_repeat, shear, noise, repeat_finite, octant_mirror, icosahedral_symmetry, with_material, surface_roughness, sweep_bezier

### Phase 5.1 追加 (LOL DSL 高階 primitive、hardsurface 完成 pattern)
- `shopping_cart_coin(diameter, thickness)` — 100 円型コイン
- `skadis_panel(size, thickness, corner_radius)` — SKADIS ペグボード
- `skadis_hook_l()` / `skadis_hook_j()` / `skadis_hook_s()` — SKADIS hook 3 種
- (Phase 5.2 完成後) `skadis_container()` / `skadis_clip()` / `skadis_shelf()` / `skadis_elastic_cord()`

### Phase 5.1 GBNF grammar 更新 (`lol.gbnf`)
- 新 primitive 名を root rule に追加
- LLM Guided Generation で新 primitive を正しく生成できる

## 6. 成功条件 (Phase 5 完成判定)

### Rust workspace 側
1. Phase 5.1-5.2 の全 SDF primitive が eval() で正しい shape (unit test)
2. `runtime_parser::parse_lol("primitive_name(args)")` が正しい SdfNode を返す (test)
3. Phase 5.3 example `complete_pipeline_output.rs` が全 12+ 品目 `.3mf` を生成、全て `non_manifold_edges == 0`
4. Phase 5.4 text-to-print pipeline test で 12+ 品目 3MF 生成成功
5. Phase 5.5 Bamboo CLI script で 4 generator 実行成功
6. `cargo test` / `cargo clippy` / `cargo fmt --check` all green

### user 側 (Bambu Studio、Phase 5.6 手順書)
1. 12+ 品目全て Bambu Studio で表示成功、warning icon なし
2. 12+ 品目全て slice 成功、G-code preview 表示
3. (option) 選択品目の実プリント成功

## 7. 実装順序と成果物

| Phase | 実装対象 | 成果物 | 想定 commit |
|--|--|--|--|
| 5.0 | 仕様書 (本 file) | `docs/PIPELINE_COMPLETE.md` | 1 (LOL) |
| 5.1 | runtime_parser primitive 登録 + GBNF + LLM_REFERENCE 更新 | `runtime_parser.rs` + `lol.gbnf` + `LLM_REFERENCE.md` | 1 (LOL) |
| 5.2 | SKADIS 残 4 accessory SDF | `stdlib/hardsurface/skadis_sdf.rs` 拡張 | 1 (LOL) |
| 5.3 | 12+ 品目 3MF 生成 example | `examples/complete_pipeline_output.rs` + 実測 | 1 (LOL) |
| 5.4 | text-to-print pipeline DC 統合 | `crates/core/src/pipeline.rs` | 1 (text-to-print) |
| 5.5 | Bamboo CLI 検証 script | `scripts/verify_bamboo_generators.sh` | 1 (Bamboo) |
| 5.6 | user Bambu Studio 検証手順書 | `docs/BAMBU_STUDIO_VERIFICATION.md` | 1 (LOL) |

**合計**: 7 commit (LOL 5、Bamboo 1、text-to-print 1)、~2000+ 行実装 + ~800 行 docs

## 8. 依存 / 前提

- alice-sdf `1.7.4` (`dual_contouring` module 既存活用)
- alice-lol `0.2.0` (Phase A / B.1 / 3'' 実装済 stdlib + print_export)
- alice-bamboo (Phase B.1.d wrapper 化済 4 generator)
- text-to-print (`crates/core/src/pipeline.rs` は Phase A.5 で README 更新のみ、pipeline 内部変更なし)

## 9. 除外項目 (Phase 5 scope 外)

- alice-lol-humanoid の既存 compile error 修正 (別 crate)
- SKADIS 追加設計 (新 accessory)、Bamboo Python `generate.py` の deprecate 完全実施
- text-to-print CI (ALICE_ECO_TOKEN 未設定、Bambu private repo access): user 側 secret 登録要
- 実プリント検証 (user 側で 3D プリンタ実行、Phase 5.6 手順書に従う)

## 10. Bambu Studio 検証で発見された不具合への対応方針

Phase 5 完了後、user が Bambu Studio で `.3mf` を検証して以下いずれかが発生した場合:

| 不具合 | 想定原因 | 修正 phase |
|--|--|--|
| 非多様体エッジ warning | DC 実装 bug or SDF spec 誤り | Phase 5.7 (bug fix) |
| slice 失敗 | mesh 破損、closed surface でない | Phase 5.7 |
| G-code 異常 | mesh normal 方向誤り | Phase 5.7 |
| build volume 超過 | SDF spec の size 過大 | user が size パラメータ調整 |
| thin wall warning | SDF spec の壁厚不足 | user が spec 調整 or Phase 5.7 で default 見直し |

Phase 5.7 (bug fix sprint) は user 検証結果次第で発火、事前定義なし
