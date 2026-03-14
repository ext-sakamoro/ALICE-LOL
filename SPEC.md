# ALICE-LOL: Law-Oriented Language Specification

## Overview

LOL（Law-Oriented Language）は、ALICE エコシステムから生まれた法則指向プログラミング言語。
従来の「変数を書き換える」「手続きを記述する」パラダイムを破壊し、
**「法則（Law）を宣言し、システムがそこへ収束する」** という新しい計算モデルを提供する。

**ターゲット領域**: 汎用言語ではない。連続空間・物理・最適化のための特化型超言語（Domain-Specific Super-Language）。

---

## 1. 四大パラダイムシフト

### 1.1 変数（State）の終焉と「場（Field）」の第一級オブジェクト化

状態（変数）という概念が存在しない。プログラマーが定義するのは、空間や時間に広がる**連続的な場（Field）**のみ。

- ALICE-SDF がメッシュ（離散）を数式（連続）に置き換えたように、LOL ではすべてのデータが関数やテンソル場として定義される
- ある瞬間の「値」が欲しい時は、システムがその場をサンプリング（観測）する
- メモリを書き換えるのではなく、「空間の歪み（法則）」を定義して放置するのが基本構文

### 1.2 命令（Instruction）から「制約の収束（Constraint Convergence）」へ

手続き（Control Flow）が存在しない。記述するのは**幾何学的・物理的・論理的な制約（Law）**のみ。

- 「リストをソートする」アルゴリズムを書くのではなく、「E_i ≤ E_{i+1}」という法則を宣言する
- コンパイラが制約を満たすための最適解（勾配降下法等）を自動生成する
- 実行とは「計算」ではなく「法則への収束」

### 1.3 通信の消滅と「相対論的結合（Relativistic Coupling）」

「ネットワーク越しにデータを送る（Send/Receive）」という関数が存在しない。

- 「この法則はノード A と B で共有されている」と宣言するだけ
- 外部からの特異点（ユーザー入力等）が発生した時のみ、境界条件の変化が最小バイトで伝播する
- ネットワーク遅延は「バグ」ではなく「物理定数」として言語に組み込まれる（遅延ポテンシャル）

### 1.4 離散時間（Tick）から「連続時間（Continuous Time）」のネイティブ解決

プログラマーは delta_time を意識しない。d/dt（時間微分）を含む法則を書くだけ。

- コンパイラが微分方程式を解析し、ハードウェア上で最も安全で効率的な評価方法を自動導出する
- 「ここは SIMD で一気に積分」「ここは区間演算で安全にバウンディング」を自動判定

---

## 2. 仮想シンタックス

```
domain Space = R^3;
domain Time = R^+;

// 1. データの代わりに「場」を定義する
field ObjectSDF(p: Space) -> Real {
    return Sphere(p, radius=1.0) ∪ Box(p, extents=[0.5, 0.5, 0.5]);
}

// 2. 処理の代わりに「制約（Law）」を定義する
law CollisionAvoidance {
    ∀ p ∈ Space, t ∈ Time:
        Agent.Distance(p, t) > 0.0;
}

// 3. 通信の代わりに「相対論的結合」を定義する
domain NetworkSpacetime {
    metric: Minkowski;
    c: Float = 150.0;  // 情報伝播の最高速度（レイテンシ限界 ms）
}

couple FluidDynamics across (EdgeA, EdgeB) in NetworkSpacetime {
    observe(EdgeA.t) = EdgeB.State(t - latency);

    resolve_strain {
        minimize ∫ (d/dt Field)^2 dt;
    }
}
```

---

## 3. 設計課題と解決策

### 3.1 矛盾する制約系（収束しない法則）

**解決策**: ハード制約とソフト制約の XPBD 的統合。

- **ハード法則（絶対不可侵）**: コンパイル時に Z3 等のソルバーで静的証明。証明できなければコンパイルエラー（Rust ボローチェッカーの拡張）
  - 例: 質量保存の法則、メモリ境界の不可侵
- **ソフト法則（エネルギー最小化）**: 実行時に矛盾が発生し得る。コンパイラは「損失関数の勾配降下」としてコンパイルする
  - 例: オブジェクト非重複、関節角度制限
  - 矛盾時は「発散」ではなく「両法則が最も妥協できる局所的最適解に留まる（バネの均衡点）」

### 3.2 NP困難な制約とソルバーの限界

**解決策**: 解析解と近似解のシームレスなフォールバック。

- 線形代数で O(1) で解ける → そのアセンブリを出力
- Gauss-Newton で収束する → 数イテレーションのコードを出力
- NP困難 / リアルタイム制約に間に合わない → ビルド時に AI モデルを訓練し、軽量推論モデル（Neural SDF 的）を自動生成してバイナリに埋め込む

### 3.3 リッチの定理（コンパイラの停止問題）

「任意の初等関数の組み合わせが恒等的にゼロになるかを判定するアルゴリズムは存在しない（決定不能）」。
汎用的な微分方程式の解析解探索はコンパイルが終わらない欠陥アーキテクチャになる。

**解決策 1: 構成的代数系の導入（リッチの定理の回避）**

- ユーザーに「任意の数式」を書かせない
- 解析可能性が保証された「プリミティブと演算子の AST」のみを言語仕様とする
- AST の各ノードが「私はこう積分できる」「私の特異点はここだ」という自己記述的 Trait を持つ
- コンパイラは木構造をトラバースするだけで O(N) で決定論的に解析解や境界を合成

**解決策 2: 保守的バウンディング（interval.rs アプローチ）**

- 解析解が見つからないノードに遭遇 → 即座に「保守的な区間（Conservative Interval）」にフォールバック
- 生成コード = 「安全なステップ幅（距離 ÷ リプシッツ定数）で進む Sphere Tracing 的コード」
- 実行時安全性は数学的に 100% 保証、コンパイル時間の爆発を防止

**解決策 3: タイムボックス・コンパイル**

| Level | 名称 | 動作 | 時間 |
|-------|------|------|------|
| 0 | Debug | 解析解を探さない。愚直なオイラー法で数値積分 | ~1秒 |
| 1 | Release | AST トラバースによる決定論的数式簡約 + SIMD 最適化 | ~数分 |
| 2 | Deep Optimize | AI（ALICE-Train）でニューラル近似を探索、時間切れで最良モデルをバイナリに焼き込む | ~24時間バジェット |

### 3.4 CAP定理とネットワーク分断

**解決策**: 最小作用の原理によるエネルギー最小化マージ。

- **分断中**: 各ノードは独立した局所的法則に従い場を更新（Git ブランチの連続空間版）
- **再結合時**: A と B の場の「段差（矛盾）」を、「場の加速度の二乗積分を最小化する変分問題」として解く
  - `minimize ∫ (d/dt Field)² dt`
  - Raft（片方破棄）でも CRDT（機械的加算）でもない第三の分散同期パラダイム
- プログラマーは「場の硬さ（Inertia/質量）」を定義するだけ
  - 質量∞ = 絶対優先、同じ硬さ = SmoothUnion 的に中間状態へモーフ

### 3.5 光速の壁と遅延のセマンティクス

**解決策**: 特殊相対性理論の因果光円錐と遅延ポテンシャルの言語組み込み。

- ノード A がノード B の場を参照する際、現在時刻 t ではなく t - latency の場として評価（遅延ポテンシャル）
- コンパイラがレイテンシに基づく推論（Prediction）を自動生成
- 正しい情報が到達した瞬間、過去に遡り滑らかに再積分（Rollback & Smooth Correction）
- ALICE-Sync の P2P Diff / Lockstep / Rollback がそのまま基盤エンジン

---

## 4. キラーユースケース

LOL が真価を発揮し、他の言語が太刀打ちできない領域:

| 領域 | LOL での記述 | 生成されるもの |
|------|-------------|--------------|
| **ロボティクス・逆運動学** | 「手先をこの座標に持っていく」（法則） | 7自由度関節の最適トルク計算コード |
| **流体力学・気象シミュレーション** | 境界条件（法則） | ナビエ・ストークス方程式ソルバー（SIMD化済み） |
| **5軸CNC・3Dプリント** | 「この SDF 形状を削り出す」（法則） | 刃の負荷を最小化する G コード生成器 |
| **自動運転・群制御** | 「衝突せず目的地へ向かう」（ポテンシャル場） | 最適経路計算コード |
| **SDF レンダリング** | SDF の数式定義 | 空間枝刈り済み超高速 WGSL シェーダ |

---

## 5. ブートストラップ戦略

### 5.1 なぜ WGSL/GLSL ターゲットか

- **LLVM IR の泥沼回避**: メモリ管理・レジスタ割り当て・ABI 互換・リンカ統合に3年沈む
- **GPU との本質的親和性**: LOL の「場を評価する」= GPU フラグメントシェーダがやっていること
- **既存資産の最大活用**: ALICE-SDF の全パイプラインがそのまま使える

### 5.2 実装形式: proc_macro（Rust への寄生）

`.lol` ファイルを作らない。LSP・シンタックスハイライト・フォーマッタの自作を完全回避。

```rust
use alice_lol::lol;

const SCENE_WGSL: &str = lol! {
    domain Space = R^3;

    field Core(p: Space) {
        Sphere(radius: 1.0) ∪ Box(extents: [0.5, 0.5, 0.5])
    }
};
```

- `cargo build` するだけで LOL がコンパイルされる
- Rust の定数をマクロ引数に渡せる（ホスト/ゲスト言語の境界がゼロコスト）
- クレート構成: `alice-lol-macro`（proc_macro 本体）+ `alice-lol`（re-export + ユーティリティ）

### 5.3 コンパイルパイプライン

```
┌─────────────────────────────────────────┐
│  lol! { field Core(p) { Sphere ∪ Box } }│  ← Rust ソース内の proc_macro
└──────────────┬──────────────────────────┘
               │ コンパイル時（cargo build）
               ▼
┌──────────────────────────┐
│  ALICE-Parser → SdfNode  │  ← 既存資産
└──────────────┬───────────┘
               │
               ▼
┌──────────────────────────┐
│  interval.rs 空間枝刈り   │  ← 既存資産（最適化パス）
└──────────────┬───────────┘
               │
               ▼
┌──────────────────────────┐
│  glsl.rs 拡張 → WGSL 出力 │  ← 既存資産 + 薄い拡張
└──────────────┬───────────┘
               │
               ▼
┌──────────────────────────┐
│  ALICE-View (wgpu) 実行   │  ← 既存資産
└──────────────────────────┘
```

全段が既存 ALICE クレートの上に乗る。新規コードは「proc_macro のパーサー」と「空間枝刈り→WGSL 分岐生成」の接合部分のみ。

---

## 6. エラーメッセージ設計: Law Checker

### 6.1 CLI エラー出力（コンパイル時）

interval.rs の eval_interval が矛盾する法則を検知した際、空間座標の AABB を出力:

```
error[L001]: Law contradiction detected in field `Core`
  --> src/main.rs:10:5
   |
10 |     Constraint: Agent.Distance(p) > 0.0
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |     This law is violated by `Box(extents: [2.0, 2.0, 2.0])`
   |     Contradiction bound: [X: 0.5..2.0, Y: -1.0..1.0, Z: 0.0..0.0]
   |
   = help: The energy field diverges in this spatial interval.
           Consider softening the constraint to `minimize(Energy)`.
```

### 6.2 Error Scene の自動生成

コンパイルエラー時にオプションで「矛盾領域が赤く発光する WGSL」を出力し、ALICE-View でプレビュー。

- テキストではなく、GPU レンダリングで「数式のどこが壊れているか」を空間的に直接視認できる
- グラフィックスと数式が融合した LOL 固有の究極の DX（開発者体験）

---

## 7. ロードマップ

### v0.1: SDF シェーダ DSL（最短距離）

| Step | 内容 | 既存資産 | 新規作業量 |
|------|------|---------|-----------|
| 1 | 言語仕様定義 | SdfNode の Enum 定義がそのまま文法 | 構文糖衣の設計のみ |
| 2 | Rust 製フロントエンド | ALICE-Parser | LOL 構文→SdfNode 変換 |
| 3 | WGSL バックエンド | glsl.rs, hlsl.rs | WGSL 出力の追加 |
| 4 | レンダリング検証 | ALICE-View (wgpu) | 統合テスト |

### v0.1 デモ: 空間枝刈りの暴力

IFS 反復フラクタル（Hyper-SDF、Kaleidoscopic IFS 12回反復）のレンダリング:

| 指標 | 手書き WGSL | LOL コンパイル出力 |
|------|-----------|-----------------|
| コード量 | 数百行 | lol! { ... } 数十行 |
| 実行時評価 | 全オブジェクト毎ステップ | 空間セルごとに枝刈り済み |
| FPS | ~15 | ~144 |
| 見た目 | 同一 | 同一 |

「同じ見た目で10倍速い。コードは1/10。理由はコンパイラが数学的証明（区間演算）で枝刈りしたから。」

### v0.2: 時間と法則の基盤（実装済み）

| Step | 内容 | 既存資産 | 新規作業量 |
|------|------|---------|-----------|
| 1 | `animate(speed, amplitude, child)` 構文 | `SdfNode::Animated` | パーサー+コード生成 |
| 2 | `morph(t, a, b)` 構文 | `SdfNode::Morph` | パーサー+コード生成 |
| 3 | 空間枝刈り対応 | `pruned_compile.rs` | `Animated`/`Morph` 枝刈りルール |
| 4 | テスト 7件 + showcase 更新 | — | 品質検証 |

#### v0.2 DSL 構文

```
時間(2): animate, morph
```

- `animate(speed, amplitude, child)` — `SdfNode::Animated` を生成。速度と振幅を指定した時間ベース変形
- `morph(t, a, b)` — `SdfNode::Morph` を生成。t=0 で a、t=1 で b への線形補間

### v0.3（実装済み）: 制約収束（Constraint Convergence）

#### Solver 透明性の設計

| 問題 | 対処 |
|------|------|
| 「なぜ発散したか」がわからない | **制約グラフの可視化** — 各制約ノードの残差（violation magnitude）をヒートマップで出力。残差が大きい上位 N 個を「疑わしい法則」としてレポート |
| 硬さ調整が困難 | **明示的なプライオリティ構文** — `law Collision { priority: hard }` / `law Aesthetics { priority: soft(0.3) }` のように、ユーザーが硬さを宣言的に書ける。暗黙の重み推定はしない |
| 矛盾の早期検出 | **静的矛盾チェック** — コンパイル時に型レベルで「この 2 法則は同一自由度を逆方向に拘束する」を検出。コンパイルエラーとして弾く |

核心原則: **ソルバーをブラックボックスにしない**。残差と制約グラフを第一級市民として公開する。

#### コンパイル時間とイテレーションの 3 段階パイプライン

```
開発中 (μs)          プレビュー (sec)        出荷 (hours)
─────────────        ──────────────          ─────────────
lol! { ... }         eval_interval           Neural SDF
  ↓ proc_macro         ↓ 空間枝刈り            ↓ Deep Optimize
SdfNode 直接eval     pruned GLSL 生成        onnx/safetensors
```

- 3段階の出力は同一の `SdfNode` ツリーから生成される
- 開発者が触るのは常に `lol!` DSL。最適化レベルはビルドフラグで切替

#### ホスト言語との境界（API 設計原則）

- `SdfNode` が唯一のゲートウェイ。LOL は `SdfNode` を生成し、Rust は `SdfNode` を消費する
- `SdfNode` は `Serialize/Deserialize` 実装済み → serde エコシステムがそのまま利用可
- ALICE-SDF の `ffi` feature により C/Python/Unity/UE5 から操作可能
- v0.3 拡張候補: 変数キャプチャ構文 `lol!(r => { sphere($r) })`、`SdfNode` コンポジション

#### v0.3 実装内容

| モジュール | 内容 |
|-----------|------|
| `law.rs` | 制約チェッカー: `NonOverlap`, `Containment`, `MinThickness` |
| 優先度 | `Priority::Hard`（エラー）/ `Priority::Soft(weight)`（警告） |
| 検出方式 | グリッド点サンプリング + セル AABB レポート |
| レポート | `LawReport` — 違反リスト（残差の絶対値降順）、空間座標、AABB |
| テスト | 11 件（law_tests.rs） |
| デモ | `law_demo.rs` — 5 シナリオ |

### v0.4（実装済み）: DSL 大幅拡張

| カテゴリ | 追加数 | 内容 |
|---------|--------|------|
| プリミティブ | +17 | rounded_cone, pyramid, hex_prism, link, capped_cone, capped_torus, rounded_cylinder, tube, barrel, heart, egg, helix, tetrahedron, box_frame, diamond, star_polygon, cross_shape |
| オペレーション | +17 | chamfer/stairs/columns/exp_smooth 族, xor, pipe, engrave, groove, tongue |
| トランスフォーム | +1 | scale_non_uniform |
| モディファイア | +13 | elongate, revolution, extrude, taper, displacement, polar_repeat, shear, noise, repeat_finite, octant_mirror, icosahedral_symmetry, with_material, surface_roughness |

### v0.5（実装済み）: 変数キャプチャ + API 拡張

| 機能 | 内容 |
|------|------|
| 変数キャプチャ `{expr}` | 数値位置に `{rust_expr}` で任意のRust式をランタイム注入 |
| 裸の変数名 | `lol! { sphere(r) }` で変数 `r` を直接参照 |
| Autodiff re-exports | `eval_with_gradient`, `mean_curvature`, `principal_curvatures`, `gaussian_curvature`, `eval_hessian`, `Dual`, `Dual3` |
| CompiledSdf re-exports | `CompiledSdf`, `eval_compiled`, SIMD/batch/BVH 系関数群 |
| Physics bridge | `sdf_to_physics_field`, `CompiledSdfField`, `simulate_sdf` (feature-gated) |

#### 変数キャプチャ構文

```rust
let r = 1.5_f32;
let node = lol! { sphere({r}) };            // {expr} 形式
let node = lol! { sphere(r) };              // 裸の変数名
let node = lol! { sphere({r * 2.0}) };      // 算術式
let node = lol! { translate({x}, {y}, 0.0, sphere({compute_radius()})) };
```

### v1.0: 完全版 — 物理・ロボティクス・CNC への拡大

v0.1〜v0.5 で構築した DSL 基盤（76構文 + 変数キャプチャ + autodiff + CompiledSdf + 法則チェッカー）の上に、
SPEC §1〜§4 で定義した四大パラダイムシフトの実装基盤を構築する。

#### Phase 1: 制約（Law）構文の本格実装

| 項目 | 内容 |
|------|------|
| ハード/ソフト分類 | `law! { hard: NonOverlap(...) }` / `law! { soft(0.3): MinThickness(...) }` |
| 制約合成 | 複数法則の AND/OR 合成、優先度付き残差ソート |
| 静的矛盾検出 | コンパイル時に同一自由度を逆方向に拘束する法則ペアを検出 |
| 残差レポート強化 | ヒートマップ出力、違反トップN、空間座標+AABB |

#### Phase 2: 連続時間セマンティクス

| 項目 | 内容 |
|------|------|
| `d/dt` 構文 | `lol! { d_dt(field) }` で時間微分を宣言的に記述 |
| 自動積分選択 | コンパイラが Euler / RK4 / 解析解を AST 構造から自動選択 |
| 区間演算統合 | `eval_interval` による安全なステップ幅の自動計算 |

#### Phase 3: ALICE エコシステム統合

| 統合先 | 内容 |
|--------|------|
| ALICE-Physics | `physics` feature による力場生成、SDF CCD 連続衝突判定 |
| ALICE-Kinematics | 逆運動学制約を Law として記述、関節角度制限の自動導出 |
| ALICE-Train | Deep Optimize レベルでの Neural SDF 近似自動生成 |
| ALICE-View | Law 違反領域の GPU ヒートマップ可視化 |

#### Phase 4: パフォーマンス最適化

| 項目 | 内容 |
|------|------|
| 3段階コンパイル | Debug(μs) → Release(sec, SIMD) → Deep Optimize(hours, Neural) |
| 空間枝刈り強化 | 法則制約を枝刈りヒントとして活用、評価不要領域の早期除外 |
| GPU バックエンド | WGSL/GLSL/HLSL 出力に法則チェックのコンパイル時埋め込み |

#### 品質基準（v1.0 リリース条件）

| 指標 | 目標 |
|------|------|
| clippy (pedantic+nursery) | 0 warnings |
| テスト数 | 200+ |
| ベンチマーク | CompiledSdf eval ≤ 10ns/point |
| ドキュメント | 全公開 API に doc comment |
| examples | 8+ デモ（basic, showcase, law, autodiff, compiled, physics, kinematics, benchmark） |

### 将来: セルフホスティング

WGSL バックエンドを「物理チップの回路マッピング」に差し替えることで、
LOL は「SDF 用 DSL」から「世界の物理法則を直接計算機に焼き付ける超言語」へ進化する。

---

## 8. 設計原則

LOL は「新しい言語を作る」のではなく、**「ALICE のコンパイルパイプラインに人間が読める構文を被せる」** プロジェクト。

1. **ALICE 資産の最大活用**: 新規コードは最小限。既存クレートの薄いラッパー
2. **構成的代数系**: 任意の数式ではなく、解析可能性が保証されたプリミティブと演算子の AST
3. **安全側フォールバック**: 解析解が出なければ即座に保守的バウンディング。安全性は数学的に 100% 保証
4. **矛盾は吸収する**: パニックや UB ではなく、エネルギー最小化による均衡点への収束
5. **遅延は物理定数**: ネットワークレイテンシを if 文のエラーハンドリングではなく遅延ポテンシャルとして数式に吸収
