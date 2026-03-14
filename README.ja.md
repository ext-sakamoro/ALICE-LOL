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

- **76 DSL 構文** — 27 プリミティブ、23 CSG オペレーション、4 トランスフォーム、19 モディファイア、2 時間制御、3 法則制約
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

## クイックスタート

```bash
# ビルド
cargo build

# テスト（133 テスト）
cargo test

# 基本デモ
cargo run --example basic

# 全構文ショーケース
cargo run --example showcase
```

## DSL 構文一覧（v0.5）

### プリミティブ（27）

```
sphere(r)  box3d(x,y,z)  rounded_box(x,y,z,r)  cylinder(h,r)  torus(R,r)
cone(h,r1,r2)  capsule(h,r)  ellipsoid(rx,ry,rz)  plane(nx,ny,nz,d)  octahedron(s)
rounded_cone(r1,r2,h)  pyramid(h,base)  hex_prism(h,r)  link(le,r1,r2)
capped_cone(h,r1,r2)  capped_torus(r_major,r_minor,angle)  rounded_cylinder(r,rr,h)
tube(r_outer,r_inner,h)  barrel(r1,r2,h)  heart(s)  egg(r1,r2)  helix(r,pitch,thickness)
tetrahedron(s)  box_frame(x,y,z,e)  diamond(s)  star_polygon(r,n,m)  cross_shape(x,y,z,r)
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

### モディファイア（19）

```
round(r)  onion(t)  twist(k)  bend(k)  mirror(axis)  repeat(sx,sy,sz)
elongate(hx,hy,hz)  revolution(o)  extrude(h)  taper(k)  displacement(amp,freq)
polar_repeat(n)  shear(kxy,kxz,kyz)  noise(amp,freq,oct)  repeat_finite(sx,sy,sz,nx,ny,nz)
octant_mirror  icosahedral_symmetry  with_material(id)  surface_roughness(amp,freq)
```

### 時間制御（2）

```
animate(speed, amplitude, child)  morph(t, a, b)
```

### 法則制約（3）

```
NonOverlap(a, b)  Containment(outer, inner)  MinThickness(node, min_t)
```

### 変数キャプチャ

```rust
let r = 1.5_f32;
let node = lol! { sphere({r}) };           // {expr} 形式
let node = lol! { sphere(r) };             // 裸の変数名
let node = lol! { sphere({r * 2.0}) };     // 算術式
```

## サンプル

| サンプル | 説明 |
|---------|------|
| `basic` | 基本構文 — sphere、box、union、smooth_union |
| `showcase` | 全 76 構文のショーケース |
| `pruning_demo` | 空間枝刈りコンパイラの効果比較 |
| `law_demo` | 法則制約 — NonOverlap、Containment、MinThickness |
| `autodiff_demo` | 自動微分 — 勾配、曲率解析 |
| `compiled_demo` | CompiledSdf — SIMD バッチ評価 |

## Cargo Features

| Feature | デフォルト | 説明 |
|---------|----------|------|
| `glsl` | Yes | GLSL トランスパイル出力 |
| `wgsl` | No | WGSL（WebGPU）出力 |
| `hlsl` | No | HLSL（DirectX）出力 |
| `physics` | No | ALICE-Physics 連携（SDF → 力場） |

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
| テスト数 | 211 |
| fmt | clean |

## ライセンス

MIT OR Apache-2.0

## 関連プロジェクト

- [ALICE-SDF](https://github.com/ext-sakamoro/ALICE-SDF) — SDF 評価、コンパイルバックエンド、SIMD、BVH
- [ALICE-View](https://github.com/ext-sakamoro/ALICE-View) — wgpu GPU レンダラー
- [ALICE-Physics](https://github.com/ext-sakamoro/ALICE-Physics) — 決定論的 128 ビット物理エンジン
- [ALICE-Eco-System](https://github.com/ext-sakamoro/ALICE-Eco-System) — 1,250 クレート間ブリッジ
