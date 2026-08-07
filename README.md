# ALICE-LOL

**Law-Oriented Language — SDF DSL as Rust proc_macro**

> "Don't write instructions. Declare laws."

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

## Features

- **123 DSL 構文** — 71 プリミティブ、23 CSG オペレーション、4 トランスフォーム、20 モディファイア、3 3Dプリント構造意図、2 時間制御、3 法則制約
- **3 シェーダ出力** — GLSL (default), WGSL, HLSL（Hardcoded / Dynamic 両モード）
- **空間枝刈りコンパイラ** — 区間演算で評価不要領域を除外、IFS フラクタルで最大 10x 高速化
- **法則制約チェッカー** — `NonOverlap`, `Containment`, `MinThickness`、ハード/ソフト優先度、空間座標レポート
- **変数キャプチャ** — `{rust_expr}` または裸の変数名で Rust の値を DSL 内に注入
- **Autodiff** — 勾配、平均曲率、ガウス曲率、主曲率、ヘシアン
- **CompiledSdf** — SIMD 8-wide バッチ評価、BVH 空間索引、Rayon 並列
- **Physics bridge** — `physics` feature で ALICE-Physics 連携

## Architecture

```
┌──────────────────────────────────────────┐
│  lol! { sphere(1.0) ∪ box3d(0.5,0.5,0.5) }  │  ← Rust ソース内 proc_macro
└─────────────────┬────────────────────────┘
                  │ cargo build (コンパイル時)
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

## Crate Structure

| Crate | Type | Role |
|-------|------|------|
| `alice-lol-macro` | proc-macro | LOL DSL パーサー + `SdfNode` コード生成 |
| `alice-lol` | rlib | Re-export + トランスパイル関数 + 法則チェッカー + 空間枝刈り |

## Quick Start

```bash
# ビルド
cargo build

# テスト (216 tests)
cargo test

# 基本デモ
cargo run --example basic

# 全構文ショーケース
cargo run --example showcase
```

## DSL Syntax (v1.0)

### Primitives (71)

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

### CSG Operations (23)

```
union  smooth_union(k)  intersection  smooth_intersection(k)  subtract  smooth_subtract(k)
chamfer_union(r)  chamfer_intersection(r)  chamfer_subtraction(r)
stairs_union(r,n)  stairs_intersection(r,n)  stairs_subtraction(r,n)
columns_union(r,n)  columns_intersection(r,n)  columns_subtraction(r,n)
exp_smooth_union(k)  exp_smooth_intersection(k)  exp_smooth_subtraction(k)
xor  pipe(r)  engrave(r)  groove(ra,rb)  tongue(ra,rb)
```

### Transforms (4)

```
translate(x,y,z, child)  rotate(rx,ry,rz, child)  scale(s, child)  scale_non_uniform(x,y,z, child)
```

### Modifiers (20)

```
round(r)  onion(t)  twist(k)  bend(k)  mirror(axis)  repeat(sx,sy,sz)
elongate(hx,hy,hz)  revolution(o)  extrude(h)  taper(k)  displacement(amp,freq)
polar_repeat(n)  shear(kxy,kxz,kyz)  noise(amp,freq,oct)  repeat_finite(sx,sy,sz,nx,ny,nz)
octant_mirror  icosahedral_symmetry  with_material(id)  surface_roughness(amp,freq)
sweep_bezier(p0x,p0y,p1x,p1y,p2x,p2y, child)
```

### 3D Print Structural Intent (3)

```
lattice_infill(shell_t, scale, lattice_t, child)   — Shell + Gyroid infill (general purpose)
diamond_infill(shell_t, scale, lattice_t, child)    — Shell + Diamond infill (high stiffness)
schwarz_infill(shell_t, scale, lattice_t, child)    — Shell + Schwarz-P infill (isotropic)
```

### Time (2)

```
animate(speed, amplitude, child)  morph(t, a, b)
```

### Laws (8) — geometric proxy 制約チェッカー

```
# 幾何 (v0.3 baseline、3 variant)
NonOverlap(a, b)  Containment(outer, inner)  MinThickness(node, min_t)

# 物理 proxy (Milestone A.2、2026-08-06 追加、5 variant)
Stress(node, load_points, min_thickness_factor)     # 荷重点近傍の応力集中
Thermal(node, heat_sources, search_radius, min_surface_ratio)  # 熱源近傍の放熱面積比
Contact(a, b, min_distance, max_distance)           # 接触可能距離範囲
Continuity(node, seed_point)                        # 単一連結領域 (BFS flood fill)
VolumeConservation(before, after, relative_tolerance)  # morph 前後の体積保存
```

geometric proxy 評価 (grid + `sdf_eval()`)、Physics dep 追加なし。精密 physics-backed 評価は Milestone A.2.1 で `alice-physics` API 経由に置換予定。

`LawSet` builder に convenience method 全 8 variant 分あり (`.stress()` / `.thermal()` / `.contact()` / `.continuity()` / `.volume_conservation()`)。`detect_contradictions()` で NonOverlap+Containment、Contact+NonOverlap の静的矛盾検出も追加。

### Intent (Milestone B.1、2026-08-06、Phase 3 IR skeleton)

`IntentNode` 16 variant + `Program { sdf, sdf_registry, intent }` 独立型 (GPU backend 型分離設計)。L1 Physical Intent verb 14 種 (grasp / release / walk / gaze / point / throw / catch / push / pull / rotate / align / follow / avoid / rest) + 合成 2 種 (Sequence / Parallel)。

```rust
use alice_lol::intent::{grasp, walk, sequence, HandSide, ProgramBuilder};
use alice_lol::SdfNode;
use glam::Vec3;

let mut builder = ProgramBuilder::new().with_sdf(SdfNode::sphere(1.0));
let cup_id = builder.register(SdfNode::sphere(0.3));
let intent = sequence(vec![
    walk(Vec3::new(1.0, 0.0, 0.0), 0.5),
    grasp(cup_id, HandSide::Right, 5.0),
]);
let prog = builder.with_intent(intent).build();
```

- `Program::as_sdf()` は intent field を露出しない = GPU backend 型分離で誤解釈事故を防止
- ALICE-Kinematics `lol` feature 経由で 8-byte Intent packet に翻訳可 (Milestone B.3)

### Variable Capture

```rust
let r = 1.5_f32;
let node = lol! { sphere({r}) };           // {expr} 形式
let node = lol! { sphere(r) };             // 裸の変数名
let node = lol! { sphere({r * 2.0}) };     // 算術式
```

## Examples

| Example | Description |
|---------|-------------|
| `basic` | 基本構文 — sphere, box, union, smooth_union |
| `showcase` | 全120構文のショーケース |
| `pruning_demo` | 空間枝刈りコンパイラの効果比較 |
| `law_demo` | 法則制約 — NonOverlap, Containment, MinThickness |
| `autodiff_demo` | 自動微分 — 勾配、曲率解析 |
| `compiled_demo` | CompiledSdf — SIMD バッチ評価 |
| `print_demo` | 3Dプリント構造意図 — 装飾/構造/ソリッド |

## Cargo Features

| Feature | Default | Description |
|---------|---------|-------------|
| `glsl` | Yes | GLSL トランスパイル出力 |
| `wgsl` | No | WGSL (WebGPU) 出力 |
| `hlsl` | No | HLSL (DirectX) 出力 |
| `physics` | No | ALICE-Physics bridge (SdfField trait impl + sim_modifier chain、Milestone A.1.0 2026-08-06 復帰) |
| `roblox` | No | Roblox OBJ/FBX (MeshPart / accessory) |
| `llm-bridge` | No | GBNF constrained decoding (AGPL-3.0 propagation 注意) |

### Backend parity test suite (Milestone A.4、2026-08-06)

`tests/backend_parity.rs` に 22 test を追加、GLSL/WGSL/HLSL 3 backend で同一 SdfNode の transpile parity を CI で保証 (primitive 7 + CSG 5 + transform 3 + modifier 4 + TPMS 1 + composite 2)。

CI matrix に `--features glsl,wgsl,hlsl` entry (`backend-parity` label) 追加済。Level 2 (実 GPU 実行 + CPU eval 数値比較) は A.4.1 別 sprint (wgpu setup 必要)。

## API

```rust
use alice_lol::{lol, to_glsl, to_wgsl, to_hlsl, eval};
use alice_lol::law::{LawSet, Law, Priority};

// DSL → SdfNode
let node = lol! { smooth_union(0.3, sphere(1.0), box3d(0.8, 0.8, 0.8)) };

// Transpile
let glsl = to_glsl(&node);                   // GLSL (hardcoded)
let wgsl = alice_lol::to_wgsl(&node);        // WGSL
let hlsl = alice_lol::to_hlsl(&node);        // HLSL

// CPU evaluation
let dist = eval(&node, glam::Vec3::ZERO);

// Law constraint check
let laws = LawSet::new()
    .add(Law::non_overlap(&a, &b), Priority::Hard)
    .add(Law::min_thickness(&node, 0.1), Priority::Soft(0.5));
let report = laws.check();
```

## Quality

| Metric | Value |
|--------|-------|
| clippy (pedantic+nursery) | 0 warnings |
| Tests | 228 |
| fmt | clean |

## License

MIT OR Apache-2.0

## Claude Code / Codex Skill

The `skills/lol-sdf/` directory bundles ALICE-LOL as an installable agent skill for Claude Code / Codex. It ships the GBNF grammar (`references/lol.gbnf`) for LLM constrained decoding, the print-oriented system prompt (`references/print-guide.md`), and thin CLI wrappers for STL/3MF export, Bambu H2D laser (`.lac`), and Roblox OBJ/FBX. See [`skills/lol-sdf/SKILL.md`](skills/lol-sdf/SKILL.md). Companion `alice-implicit-cad` skill (in the [ALICE-SDF](https://github.com/ext-sakamoro/ALICE-SDF) repo) provides the lower-level SDF composition front-end.

## Related

- [ALICE-SDF](https://github.com/ext-sakamoro/ALICE-SDF) — SDF evaluation, compiled backends, SIMD, BVH
- [ALICE-View](https://github.com/ext-sakamoro/ALICE-View) — wgpu GPU renderer
- [ALICE-Physics](https://github.com/ext-sakamoro/ALICE-Physics) — Deterministic 128-bit physics engine

## Consumers

以下 crate が ALICE-LOL DSL を consumer として利用中 or 計画中:

| Consumer | 状態 | 用途 |
|---|---|---|
| [ALICE-Bamboo](https://github.com/ext-sakamoro/ALICE-Bamboo) | Active | 3D プリント統合 pipeline (LOL DSL → SDF → Physics 検証 → 3MF → Bambu Studio) |
| [ALICE-Manga](https://github.com/ext-sakamoro/ALICE-Manga) | Integration PoC (2026-07-29〜) | 漫画キャラ silhouette / scene の宣言的定義 T1 statement 完了 (dev-dep + PoC example)、`docs/INTEGRATION_STRATEGY.md` T1-T5 roadmap で本格統合予定 |
| ALICE-Metal-Card | Active | H2D レーザー用 SVG → `.lac` 生成 |
- [ALICE-Eco-System](https://github.com/ext-sakamoro/ALICE-Eco-System) — 1,250 cross-crate bridges
