# ALICE-LOL

**Law-Oriented Language — SDF DSL を Rust proc_macro として実装**

> "命令を書くな。法則を宣言せよ。"

LOL（Law-Oriented Language）は、ALICE-SDF エコシステム向けの法則指向 DSL。
`lol!` マクロで SDF シーンを宣言的に記述し、コンパイル時に `SdfNode` → GLSL / WGSL / HLSL へトランスパイルする。

```rust
use alice_lol::lol;

let scene = lol! {
    field MyScene {
        smooth_union(0.2,
            sphere(1.0),
            translate(2.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5))
        )
    }
};

let glsl = alice_lol::to_glsl(&scene);
```

## 特徴

- **123 DSL 構文** — 71 プリミティブ、23 CSG オペレーション、4 トランスフォーム、20 モディファイア、3 3Dプリント構造意図、2 時間制御、3 法則制約
- **3 シェーダ出力** — GLSL（デフォルト）、WGSL、HLSL（Hardcoded / Dynamic 両モード）
- **空間枝刈りコンパイラ** — 区間演算で評価不要領域を除外、IFS フラクタルで最大 10 倍高速化
- **法則制約チェッカー** — `NonOverlap`、`Containment`、`MinThickness`、ハード/ソフト優先度、空間座標レポート
- **変数キャプチャ** — `{rust_expr}` または裸の変数名で Rust の値を DSL 内に注入
- **自動微分** — 勾配、平均曲率、ガウス曲率、主曲率、ヘシアン
- **CompiledSdf** — SIMD 8-wide バッチ評価、BVH 空間索引、Rayon 並列
- **物理連携** — `physics` feature で ALICE-Physics と接続

## アーキテクチャ

```
┌──────────────────────────────────────────┐
│  lol! { sphere(1.0) ∪ box3d(0.5,0.5,0.5) }  │  ← Rust ソース内 proc_macro
└─────────────────┬────────────────────────┘
                  │ cargo build（コンパイル時）
                  ▼
┌──────────────────────────┐
│  alice-lol-macro          │  ← syn + quote パーサー
│  LOL DSL → SdfNode 生成   │
└─────────────────┬────────┘
                  ▼
┌──────────────────────────┐
│  alice-sdf                │
│  ├─ eval()        CPU 評価 │
│  ├─ interval.rs   枝刈り   │
│  ├─ glsl.rs       GLSL    │
│  ├─ wgsl.rs       WGSL    │
│  └─ hlsl.rs       HLSL    │
└──────────────────────────┘
```

## クレート構成

| クレート | 種別 | 役割 |
|---------|------|------|
| `alice-lol-macro` | proc-macro | LOL DSL パーサー + `SdfNode` コード生成 |
| `alice-lol` | rlib | Re-export + トランスパイル関数 + 法則チェッカー + 空間枝刈り |
| `alice-lol-datagen` | rlib + bin (sibling) | 合成 (caption, LOL) pair generator (Track C1、8 template family + self-check、6,500 sample/s) |

## クイックスタート

```bash
# ビルド
cargo build

# テスト（228 テスト）
cargo test

# 基本デモ
cargo run --example basic

# 全構文ショーケース
cargo run --example showcase
```

## DSL 構文一覧（v1.0）

### プリミティブ（71）

```
sphere(r)  box3d(x,y,z)  rounded_box(x,y,z,r)  cylinder(h,r)  torus(R,r)
cone(h,r1,r2)  capsule(h,r)  ellipsoid(rx,ry,rz)  plane(nx,ny,nz,d)  octahedron(s)
rounded_cone(r1,r2,h)  pyramid(h)  hex_prism(r,h)  link(l,r1,r2)
capped_cone(h,r1,r2)  capped_torus(R,r,angle)  rounded_cylinder(r,rr,h)
tube(r,t,h)  barrel(r,h,b)  heart(s)  egg(ra,rb)  helix(R,r,pitch,h)
tetrahedron(s)  box_frame(x,y,z,e)  diamond(r,h)  star_polygon(r,n,m,h)  cross_shape(l,t,r,h)
triangle(ax,ay,az,bx,by,bz,cx,cy,cz)  bezier(ax,ay,az,bx,by,bz,cx,cy,cz,r)
triangular_prism(w,d)  cut_sphere(r,h)  cut_hollow_sphere(r,h,t)  death_star(ra,rb,d)
solid_angle(a,r)  rhombus(la,lb,h,r)  horseshoe(a,r,l,w,t)  vesica(r,d)
infinite_cylinder(r)  infinite_cone(a)  gyroid(s,t)  chamfered_cube(x,y,z,c)
schwarz_p(s,t)  superellipsoid(x,y,z,e1,e2)  rounded_x(w,r,h)  pie(a,r,h)
trapezoid(r1,r2,th,d)  parallelogram(w,h,s,d)  tunnel(w,h,d)  uneven_capsule(r1,r2,h,d)
arc_shape(a,r,t,h)  moon(d,ra,rb,h)  blobby_cross(s,h)  parabola_segment(w,h,d)
regular_polygon(r,n,h)  stairs_prim(sw,sh,n,d)
dodecahedron(r)  icosahedron(r)  truncated_octahedron(r)  truncated_icosahedron(r)
diamond_surface(s,t)  neovius(s,t)  lidinoid(s,t)  iwp(s,t)  frd(s,t)
fischer_koch_s(s,t)  pmy(s,t)
circle_2d(r,h)  rect_2d(x,y,h)  segment_2d(ax,ay,bx,by,t,h)
rounded_rect_2d(x,y,r,h)  annular_2d(r,t,h)
```

### CSG オペレーション（23）

```
union  smooth_union(k)  intersection  smooth_intersection(k)  subtract  smooth_subtract(k)
chamfer_union(r)  chamfer_intersection(r)  chamfer_subtraction(r)
stairs_union(r,n)  stairs_intersection(r,n)  stairs_subtraction(r,n)
columns_union(r,n)  columns_intersection(r,n)  columns_subtraction(r,n)
exp_smooth_union(k)  exp_smooth_intersection(k)  exp_smooth_subtraction(k)
xor  pipe(r)  engrave(r)  groove(ra,rb)  tongue(ra,rb)
```

### トランスフォーム（4）

```
translate(x,y,z, child)  rotate(rx,ry,rz, child)  scale(s, child)  scale_non_uniform(x,y,z, child)
```

### モディファイア（20）

```
round(r)  onion(t)  twist(k)  bend(k)  mirror(axis)  repeat(sx,sy,sz)
elongate(hx,hy,hz)  revolution(o)  extrude(h)  taper(k)  displacement(amp,freq)
polar_repeat(n)  shear(kxy,kxz,kyz)  noise(amp,freq,oct)  repeat_finite(sx,sy,sz,nx,ny,nz)
octant_mirror  icosahedral_symmetry  with_material(id)  surface_roughness(amp,freq)
sweep_bezier(p0x,p0y,p1x,p1y,p2x,p2y, child)
```

### 3Dプリント構造意図（3）

```
lattice_infill(shell_t, scale, lattice_t, child)   — シェル + ジャイロイドインフィル（汎用）
diamond_infill(shell_t, scale, lattice_t, child)    — シェル + ダイヤモンドインフィル（高剛性）
schwarz_infill(shell_t, scale, lattice_t, child)    — シェル + Schwarz-Pインフィル（等方性）
```

### 時間制御（2）

```
animate(speed, amplitude, child)  morph(t, a, b)
```

### 法則制約（8）— geometric proxy 制約チェッカー

```
# 幾何 (v0.3 baseline、3 variant)
NonOverlap(a, b)  Containment(outer, inner)  MinThickness(node, min_t)

# 物理 proxy (Milestone A.2、5 variant)
Stress(node, load_points, min_thickness_factor)     # 荷重点近傍の応力集中
Thermal(node, heat_sources, search_radius, min_surface_ratio)  # 熱源近傍の放熱面積比
Contact(a, b, min_distance, max_distance)           # 接触可能距離範囲
Continuity(node, seed_point)                        # 単一連結領域 (BFS flood fill)
VolumeConservation(before, after, relative_tolerance)  # morph 前後の体積保存
```

距離依存 5 variant (`MinThickness` / `Stress` / `NonOverlap` / `Containment` / `Contact`) は 0.4.0 から **場の値を距離として使わない** (TPMS は √3〜7 倍に過大、union 内部は過小、`eval_lipschitz` は外部限定) `eval_interval` (区間演算) で「箱に表面なし」を証明し、点評価の符号変化 (中間値定理) で「表面まで ≤ |p − q|」の証拠を取り、決められなかった標本点は `LawReport::unresolved` に載せる `all_passed()` は「違反なし」ではなく「全標本点で証明済」 解析解 oracle は `tests/analytic_law.rs` (gyroid 板 / union 内部 / 球殻 / 2 球 / Contact gap / 板 Stress、resolution 独立性)

### 変数キャプチャ

```rust
let r = 1.5_f32;
let node = lol! { sphere({r}) };           // {expr} 形式
let node = lol! { sphere(r) };             // 裸の変数名
let node = lol! { sphere({r * 2.0}) };     // 算術式
```

## Intent IR (Phase 3 / 3.1、2026-09-13)

ALICE 三相原理 (Data → Law → Intent) の **Phase 3 Intent 相**。`IntentNode` 17 variant + `Program { sdf, sdf_registry, intent }` 独立型で、Physical Intent と Musical Intent を型分離しつつ同一 program tree に埋め込める。

**L1 Physical Intent (14 verb + 2 合成)** — grasp / release / walk / gaze / point / throw / catch / push / pull / rotate / align / follow / avoid / rest + Sequence / Parallel

**L1 Musical Intent (1 variant、2026-09-13)** — `IntentNode::Music { packet: [u8; 8] }` は opaque 8-byte payload。内訳は [`alice-synth` の `intent::MusicIntent`](https://github.com/ext-sakamoro/ALICE-Synth) が正式定義 (genre / mood / length_bars / tempo_bpm_offset / key / mode / variation_seed)。

```rust
use alice_lol::intent::{grasp, music_intent, parallel, HandSide};

// 楽器を掴みながら C major folk を演奏する Intent
let program = parallel(vec![
    music_intent([0, 0, 4, 80, 0, 0, 0xC0, 0xDE]),  // C major Folk / Happy
    grasp(0, HandSide::Right, 5.0),
]);
```

- **依存なし** — `alice-lol` は `alice-synth` に依存しない。8-byte packet が唯一の cross-crate 契約
- consumer (interpreter / LLM plan head / remote 演奏サーバ) は `MusicIntent::from_bytes(packet)` で復元 → `synthesize()` で PCM 生成
- GPU backend 型分離 — `Program::as_sdf()` は intent field を露出せず、shader が誤って Intent を解釈する事故を型で防止
- **`emit::to_lol` (C0、2026-09-14)** — `SdfNode` → LOL text の逆変換 (128 variant exhaustive、正規形 = 左畳み込み 2 分木 / 角度 1e-3 度 / ノイズ吸収、eval parity round-trip を全 237 construct で CI 固定) + `Program::to_lol()`。合成 data generator (Track C) の基盤。
- **LOL text 構文 (A0、2026-09-14)** — `runtime_parser::parse_program("program(<sdf>, entities(...), <intent>)")` で LLM 出力から `Program` を直接構築、`IntentNode::to_lol()` で逆変換 (round-trip)。`lol.gbnf` も同構文を受理するため grammar-constrained decoding で Phase 3 Intent を emit 可能。`rotate` verb は SDF transform と衝突するため text では `turn`

## サンプル

| サンプル | 説明 |
|---------|------|
| `basic` | 基本構文 — sphere、box、union、smooth_union |
| `showcase` | 全 120 構文のショーケース |
| `pruning_demo` | 空間枝刈りコンパイラの効果比較 |
| `law_demo` | 法則制約 — NonOverlap、Containment、MinThickness |
| `autodiff_demo` | 自動微分 — 勾配、曲率解析 |
| `compiled_demo` | CompiledSdf — SIMD バッチ評価 |
| `llm_bench` (`--features llm-bridge`) | LLM 生成品質 benchmark — 20 prompt (T1-T4) を oracle (SDF 点内外 / Intent verb 構造) で判定、grammar-only vs think→grammar の pass 率 (2026-09-14) |

## Cargo Features

| Feature | デフォルト | 説明 |
|---------|----------|------|
| `glsl` | Yes | GLSL トランスパイル出力 |
| `wgsl` | No | WGSL（WebGPU）出力 |
| `hlsl` | No | HLSL（DirectX）出力 |
| `physics` | No | ALICE-Physics 連携（SDF → 力場） |
| `llm-bridge` | No | GBNF constrained decoding + think-prefix 2 段生成（alice-llm `grammar` + `simd` + `parallel`、AGPL-3.0 伝播注意） |

## API

```rust
use alice_lol::{lol, to_glsl, to_wgsl, to_hlsl, eval};
use alice_lol::law::{LawSet, Law, Priority};

// DSL → SdfNode
let node = lol! { smooth_union(0.3, sphere(1.0), box3d(0.8, 0.8, 0.8)) };

// トランスパイル
let glsl = to_glsl(&node);                   // GLSL（ハードコード定数）
let wgsl = alice_lol::to_wgsl(&node);        // WGSL
let hlsl = alice_lol::to_hlsl(&node);        // HLSL

// CPU 評価
let dist = eval(&node, glam::Vec3::ZERO);

// 法則制約チェック
let laws = LawSet::new()
    .add(Law::non_overlap(&a, &b), Priority::Hard)
    .add(Law::min_thickness(&node, 0.1), Priority::Soft(0.5));
let report = laws.check();
```

## 品質

| 指標 | 値 |
|------|-----|
| clippy (pedantic+nursery) | 0 warnings |
| テスト数 | 228 |
| fmt | clean |

## ライセンス

MIT OR Apache-2.0

## 関連プロジェクト

- [ALICE-SDF](https://github.com/ext-sakamoro/ALICE-SDF) — SDF 評価、コンパイルバックエンド、SIMD、BVH
- [ALICE-View](https://github.com/ext-sakamoro/ALICE-View) — wgpu GPU レンダラー
- [ALICE-Physics](https://github.com/ext-sakamoro/ALICE-Physics) — 決定論的 128 ビット物理エンジン
- [ALICE-Eco-System](https://github.com/ext-sakamoro/ALICE-Eco-System) — 1,250 クレート間ブリッジ
