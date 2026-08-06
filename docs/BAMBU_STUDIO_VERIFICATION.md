# Bambu Studio 検証手順書 (Phase 5.6、user 実施用)

**目的**: Phase 5.1-5.7 で完成した LOL DSL → Bambu 対応 .3mf pipeline の全 13+ 品目
`.3mf` file を Bambu Studio で開き、表示エラーなし + slice 成功 + (option) 実プリント
成功を検証する

**前提**: Phase 5.3 example (`complete_pipeline_bambu_3mf.rs`) を実行し `./output/{thin,thick}/`
に 13 品目 `.3mf` が生成済 + Phase 5.5 script (`scripts/verify_bamboo_generators.sh`) を
実行し `/tmp/bamboo_verify/` に 4 CLI generator `.3mf` が生成済

---

## 0. Pre-Verify 実測結果 + Bambu Studio 検証優先順位 (2026-08-06 Phase B.2 代替)

**実行**: `cd ~/ALICE-Bamboo && cargo test --test bambu_pre_verify --release -- --nocapture`

Phase 5.6 手順書 17 items のうち **自動検証可能 9 items** を Rust integration test で
先行実施 (`~/ALICE-Bamboo/tests/bambu_pre_verify.rs`) user は残 **8 items** (Bambu Studio
目視 / slice / 実プリント) に絞れる

### PASS 8 pattern (推奨、user 目視 8 items 実施)

| pattern | route | mesh (vert/face) | bbox mm | file KB | PrintabilityScore |
|--|--|--|--|--|--|
| shopping_cart_coin_100yen | DC | 49K / 96K | 22×2×22 | 779 | 88 Excellent |
| skadis_panel_300x300 | DC | 51K / 102K | 22×11×22 | 1755 | 94 Excellent |
| skadis_hook_l | DC | 62K / 123K | 18×8×8 | 1211 | 79 Good (Field test 通過) |
| skadis_hook_s | DC | 54K / 108K | 18×10×5 | 1317 | 88 Excellent |
| skadis_shelf | DC | 33K / 65K | 22×2×22 | 1297 | 97 Excellent |
| shelf_divider_560x250x120 | MC | 54K / 106K | 22×22×5 | 1486 | 69 Acceptable (30lbs 実荷重 baseline) |
| wall_hook | MC | 34K / 68K | 22×22×12 | 1008 | 70 Good |
| drawer_organizer | MC | 33K / 66K | 0×22×22 | 2468 | 81 Good |

### FAIL 5 pattern (Bambu Studio 検証保留、SDF 生成 bug 個別 fix 後に再検証)

| pattern | 症状 | 真因仮説 |
|--|--|--|
| skadis_container | mesh 0 (resolution 128 でも生成失敗) | SDF `smooth_union` の tight_aabb 計算破綻 or CSG 交差問題 |
| gridfinity_bin | mesh 0 (Phase 5.8 dividers fix 済のはずが SDF eval 破綻) | GridfinitySpec::default_2x2 の SDF 経路で AABB detect 失敗 |
| skadis_elastic_cord | NME 212 (DC watertight 破綻) | 薄物間 domain 干渉、DC config 調整 or SDF 設計見直し要 |
| skadis_hook_j | NME 23 (DC 微細 NME) | Y 軸 native cylinder の DC boundary で watertight 微破綻 |
| skadis_clip | NME 4 (DC 微細 NME、許容 threshold 内かも) | 同上、resolution 上げれば解消の可能性 |

FAIL pattern の user 対応: **Bambu Studio 検証を skip 推奨、fix 完了通知を待つ**
(現状 Phase 5.6 手順書の T5-T7 / K2 は informational のみ、実プリント推奨せず)

### 自動 9 items の内訳 (test で assert 済)

1. mesh face count > 0
2. mesh vertex count > 0
3. bbox size <= Bambu H2D 315×310×315mm build volume
4. non_manifold_edges (DC 経路 0 保証、MC 経路 5% 許容)
5. boundary_edges 計測 (watertight metric)
6. degenerate_triangles 計測 (品質 metric)
7. 3MF file 生成成功 (Bambu template 埋込込み)
8. 3MF file size > 10KB (metadata + mesh 埋込確認)
9. min_wall_thickness spec >= 1.2mm (PrintabilityScore の min_thickness = 100 対応)

### user 目視 8 items (Bambu Studio 実施)

10. Bambu Studio でファイル open 可
11. slice 成功
12. thumbnail 表示
13. printer profile 認識 (Bambu H2D)
14. filament 割り当て (PLA / PETG spec 準拠)
15. 印刷開始時 bed 密着確認
16. 印刷完了品質 (層剥離 / warp / 表面)
17. functional 確認 (ペグ固定 / 荷重 / 用途)

---

## 1. 検証対象ファイル一覧 (17 品目)

### 1.1 薄物 DC 経路 (9 品目、`~/ALICE-Bamboo/output/thin/`)

| No | file | LOL DSL | 実測 (Phase 5.3) | 期待 slice 時間 (PLA, 0.12mm layer) |
|--|--|--|--|--|
| T1 | `shopping_cart_coin_100yen.3mf` | `shopping_cart_coin(22.8, 1.7)` | 49K vert / 99K tri / 779KB | ~10 min |
| T2 | `skadis_panel_300x300.3mf` | `skadis_panel(300, 5, 5)` | 119K vert / 192K tri / 1673KB | ~4-6 hours (大型) |
| T3 | `skadis_hook_l.3mf` | `skadis_hook_l()` | 60K vert / 120K tri / 1211KB | ~1.5 hours |
| T4 | `skadis_hook_j.3mf` | `skadis_hook_j()` | 62K vert / 124K tri / 1481KB | ~1.5 hours |
| T5 | `skadis_hook_s.3mf` | `skadis_hook_s()` | 57K vert / 113K tri / 1317KB | ~1 hour |
| T6 | `skadis_container.3mf` | `skadis_container()` | 150K vert / 300K tri / 2340KB | ~4-6 hours |
| T7 | `skadis_clip.3mf` | `skadis_clip()` | 94K vert / 188K tri / 1504KB | ~1 hour |
| T8 | `skadis_shelf.3mf` | `skadis_shelf()` | 87K vert / 171K tri / 1297KB | ~4-6 hours (幅 260mm) |
| T9 | `skadis_elastic_cord.3mf` | `skadis_elastic_cord()` | 66K vert / 132K tri / 1207KB | ~1 hour |

### 1.2 厚物 MC 経路 (4 品目、`~/ALICE-Bamboo/output/thick/`)

| No | file | Rust API 呼び出し | 実測 | 期待 slice 時間 |
|--|--|--|--|--|
| K1 | `wall_hook_pla_1kgf.3mf` | `pattern_sdf::wall_hook(&WallHookSpec::pla_1kgf())` | 63K vert / 125K tri / 1008KB | ~1.5 hours |
| K2 | `gridfinity_bin_2x2.3mf` | `pattern_sdf::gridfinity_bin(&GridfinitySpec::default_2x2())` | 155K vert / 309K tri / 2357KB | ~2 hours |
| K3 | `drawer_organizer_chopsticks.3mf` | `pattern_sdf::drawer_organizer(&DrawerSpec::default_chopsticks_set())` | 159K vert / 318K tri / 2468KB | ~4-6 hours (250mm 幅) |
| K4 | `shelf_divider_560x250x120.3mf` | `pattern_sdf::shelf_divider(&ShelfDividerSpec::field_tested_560x250x120())` | 98K vert / 197K tri / 1486KB | ~12+ hours (実プリント合格 spec) |

### 1.3 Bamboo CLI 経路 (4 品目、`/tmp/bamboo_verify/`、Phase 5.5)

**注意**: 本 4 品目は `alice-bamboo` CLI で生成、**素の 3MF** (Bambu template 埋込なし)
Bambu Studio で開くと「印刷設定なし」で表示される、user が material / printer を毎回設定要
本格運用は Phase 5.3 example (Bambu template embed 版) を推奨

| No | file | CLI subcommand | 実測サイズ | 状態 |
|--|--|--|--|--|
| C1 | `hook.3mf` | `alice-bamboo hook --load 3 --mount screw` | 18MB | OK |
| C2 | `gridfinity_2x2_div2x2.3mf` | `alice-bamboo gridfinity --units 2x2 --dividers 2x2` | **1.3KB** | ⚠ **degenerate mesh 疑い** (別 sprint 修正、Bamboo generators/gridfinity.rs wrapper 実装確認要) |
| C3 | `drawer_chopsticks_set.3mf` | `alice-bamboo drawer --slots "chopsticks:2,fork:4,knife:4"` | 146MB | 大きすぎる (resolution 高すぎ、--resolution 128 で縮小可) |
| C4 | `shelf_divider/shelf_divider.3mf` | `alice-bamboo shelf-divider --width 560 --depth 250 --height 120` | 64MB | OK |

---

## 2. Bambu Studio 検証 checklist (各品目に対して実施)

### 2.1 基本 checklist

各 `.3mf` について以下を確認:

- [ ] **File > Import** で開いてエラーダイアログ出ない
- [ ] **3D view** に mesh が正しく表示 (完全に欠けたり穴が空いたりしない)
- [ ] 右下 status bar に **warning icon なし** (⚠ 非多様体 / thin wall 警告なし)
- [ ] **Prepare tab > Slice plate** で slice 成功 (エラーなし、cancel 押されず完了)
- [ ] **Preview tab** で G-code preview 表示 (全 layer 移動、時間 / filament 見積表示)
- [ ] slice 時間 / filament 消費が想定範囲内 (§1 表の期待値と近い)

### 2.2 追加 checklist (Phase 5.3 Bambu template embed 版のみ、§1.1-1.2)

- [ ] File > Import 後、**プリンタ設定 = Bambu Lab H2D** が自動選択される
- [ ] **フィラメント設定 = Generic PETG** (template default) が自動選択される
- [ ] **プリントプロファイル = 0.12mm 標準** (or 近似) が自動選択される
- [ ] Metadata > Application = `BambuStudio-02.05.00.66` 表示
- [ ] サムネイル (plate preview) が正しく表示 (template PNG 由来)

### 2.3 実プリント時の追加 checklist (option)

- [ ] Send to Printer / SD card export 成功
- [ ] 実プリント: bed adhesion OK (剥がれなし、warp なし)
- [ ] 実プリント: mesh 品質 (visual defect なし、layer shift なし)
- [ ] 実プリント: 設計寸法通り (SKADIS peg 5×15mm、coin Φ22.8×1.7mm 等)
- [ ] user 判定: 実物として使用可能な品質

---

## 3. 品目別 実プリント注意点

### 3.1 薄物 (T1-T9)

**全 T1-T9 共通**:
- **DC 経路生成 = Phase 3''.2 実測で non-manifold 0 保証**、mesh 品質は SDF+MC より高い
- Bambu Studio で open 時に **warning なし** が期待、あれば Phase 5.7 の bug 可能性 (report 要)

**T1 shopping_cart_coin (1.7mm 極薄)**:
- 印刷向き: **face flat down** (Z 高さ = 1.7mm、layer 数最小)
- Layer height: 0.12mm 推奨 (14 layer)
- Infill: 100% (小型、フィラメント消費小)
- Support: 不要

**T2 skadis_panel_300x300 (300×300×5mm、大型薄板)**:
- 印刷向き: **flat on bed** (Z 高さ = 5mm)
- Bed 面積 300×300mm = H2D 単一 build volume ギリギリ (H2D 315×310mm、~ok)
- **大面積フラット板の反り対策必須** (~/CLAUDE.md §「大面積フラット板の反り・剥がれ対策」)
  - Bambu Studio でスライサー **ブリム 5-10mm 追加** 推奨
  - 千鳥ペグ穴 98 個が既に肉抜きとして機能、追加肉抜き不要
- Layer: 0.16mm、Infill: 15% grid、Support: 不要
- 実プリント時間 4-6 hours 想定

**T3-T5 skadis_hook_l/j/s (SKADIS ペグ差込フック)**:
- 印刷向き: **peg down (peg blade を bed 面に)**、hook body が上向き
- Support: hook 曲線部の overhang 要検討 (自動 support 推奨)
- Bambu Python `generate.py` の texture / R フィレット省略済 = DC で自然滑らか化予定

**T6 skadis_container**:
- 印刷向き: 底面 down、開口上向き
- Support: 内壁不要 (垂直)、外部不要
- **肉抜き穴省略済** (Phase 5.2 近似実装)、実用時 filament 消費が増える可能性

**T7 skadis_clip**:
- 印刷向き: peg down、二股 slot 上向き
- Support: slot 内不要 (垂直)
- SLOT_W = 1.2mm、Bambu Studio の nozzle 0.4mm で問題なし

**T8 skadis_shelf (260×80mm 幅広)**:
- 印刷向き: 底面 down
- 2 peg (両端 SHELF_PEG_SPACING = 240mm) は SKADIS 6-grid ピッチ準拠
- 底面 rib 省略 (Phase 5.2 近似実装)、実プリント時 rib 追加検討可

**T9 skadis_elastic_cord**:
- 印刷向き: peg down
- 2 hook (上下対称) の overhang 要検討

### 3.2 厚物 (K1-K4)

**K1 wall_hook**:
- 印刷向き: backplate flat down、hook 上向き
- Support: hook throat 部 (内側 U 字) は自動 support 推奨
- 荷重 1kgf = PLA 標準寸法、実用強度 OK

**K2 gridfinity_bin (2×2×4U + 2×2 dividers)**:
- 印刷向き: 底面 down
- Support: 内側 divider 不要 (垂直)
- 6 compartments 内、layer 0.2mm で高速印刷可

**K3 drawer_organizer (chopsticks + fork + knife、250×200×40mm)**:
- 印刷向き: 底面 down
- Support: slot 内壁不要 (垂直)
- 250mm 幅、H2D 単一プレート OK (315mm)

**K4 shelf_divider_560×250×120mm (**実プリント合格 baseline**)**:
- 印刷向き: **逆さ印刷** (~/ALICE-Bamboo/CLAUDE.md §「印刷向き」)、天板を bed 面に
- Bambu Studio で **auto-orient で逆さ配置**、または manual で Z 軸 180° 回転
- **560mm 幅は H2D 単一プレート超過** (H2D 315mm) = 分割印刷要
  - 分割方式: ラップジョイント (~/ALICE-Bamboo/CLAUDE.md §「オーバーサイズ分割印刷ルール」)
  - Bambu Studio Cut tool で 2 分割 (280mm × 2)、ラップジョイント 5mm
- Material: **PETG 推奨** (棚荷重、PLA 非推奨)、Nylon も可
- Infill: 15% gyroid、Support: brim 推奨
- 実プリント合格 30lbs (13.6kg) 荷重テスト済 (~/ALICE-Bamboo/CLAUDE.md § MakerWorld 参考モデル)

### 3.3 CLI 素の 3MF (C1-C4)

**共通**: Bambu template 埋込なし、Bambu Studio で開くと **印刷設定空**
- User が **Printer > H2D、Filament > PLA/PETG、Process > 0.2mm 標準** を手動設定要
- 推奨: § 1.1-1.2 の Bambu template 埋込版 (Phase 5.3 example 経由) を使用

**C2 gridfinity 1.3KB = degenerate mesh 疑い**:
- Bambu Studio で開くと mesh 表示なし or 極小
- **Bamboo generators/gridfinity.rs wrapper 実装の bug 疑い**
- Report 要 → 別 sprint (Phase 5.8 or B.1.d 補修) で対処

---

## 4. Trouble shooting

### 4.1 Bambu Studio で開いてエラー

| エラー | 原因候補 | 対処 |
|--|--|--|
| "3MF file corrupted" | zip package 破損、writer bug | Phase 5.7 alice-bamboo bambu_3mf.rs の bug report 要 |
| "No printable object" | mesh 空、degenerate | file size < 10KB なら degenerate 疑い (C2 gridfinity と同型) |
| "Non-manifold edges detected" | mesh 非多様体 | DC 経路のはず = Phase 3''.2 実測で 0 だったので新 bug、report 要 |
| "Object out of build volume" | build volume 超過 (H2D 315mm) | K4 shelf_divider 560mm など、Cut tool で分割印刷 |

### 4.2 slice で失敗

| エラー | 原因 | 対処 |
|--|--|--|
| "Thin wall detected" | 壁厚 < nozzle × 2 (0.8mm 未満) | 該当なし想定、あれば SDF spec 定数見直し |
| "Overhang without support" | 45° 超 overhang | Support 自動生成 ON |
| "Bridge too long" | 天井 span > 10mm | Support or 印刷向き変更 |

### 4.3 実プリントで問題

- 詳細は **~/CLAUDE.md § 「3Dプリント出力バリデーションフロー」** 参照
- 剥がれ / 反り: brim / 温度調整 / bed adhesion
- 層間剥離: material 温度 up
- 寸法ズレ: printer キャリブレーション

---

## 5. 検証結果 report format

各品目について以下を記録 (user 判断で SNS / GitHub Issue 等に共有可):

```
Item: T1 shopping_cart_coin_100yen
File: /Users/ys/ALICE-Bamboo/output/thin/shopping_cart_coin_100yen.3mf
Bambu Studio version: 02.05.00.66
Import: OK / Fail (theory: ...)
Slice: OK / Fail
G-code preview: OK / Fail
実プリント: OK / Fail / Not tested
実物品質: 5/5 stars / 実用可 / 実用不可
Note: (任意コメント)
```

---

## 6. 完成判定

Phase 5 全体 (5.0-5.7 全 sub-phase) は以下で完成判定:

- [ ] 13 品目 (T1-T9 + K1-K4) 全て Bambu Studio で開いて表示成功
- [ ] 13 品目 全て slice 成功
- [ ] user 判定で「実プリント可能な品質」= 10 品目以上
- [ ] CLI 4 品目 (C1-C4) 大部分成功、C2 gridfinity bug は別 sprint 対応

上記全満たせば Phase 5 完成 = **LOL DSL → Bambu Studio → 実プリント pipeline を Rust workspace で完全実現**

満たさない品目があれば bug report → Phase 5.8 (bug fix sprint) で対応

---

## 7. 関連 doc

- `docs/PIPELINE_COMPLETE.md` — Phase 5 全体仕様書
- `~/ALICE-Bamboo/CLAUDE.md` — Bamboo 実プリント設計原則
- `~/ALICE-Bamboo/CLAUDE.md § MakerWorld アップロード` — 3MF 内部構造仕様
- `~/CLAUDE.md § 3Dプリント出力バリデーションフロー` — user side 検証原則
- `~/.claude/projects/-Users-ys/memory/reference_bambu_3mf_analyzed_assets.md` — Bambu template 資産索引
- `~/.claude/projects/-Users-ys/memory/feedback_alice_polygon_extrude_data_route.md` — Phase 4 polygon_extrude 削除経緯
