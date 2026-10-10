# ALICE-LOL

[![crates.io](https://img.shields.io/crates/v/alice-lol.svg)](https://crates.io/crates/alice-lol)
[![docs.rs](https://img.shields.io/docsrs/alice-lol)](https://docs.rs/alice-lol)
[![MSRV](https://img.shields.io/crates/msrv/alice-lol)](#msrv)
[![CI](https://github.com/ext-sakamoro/ALICE-LOL/actions/workflows/ci.yml/badge.svg)](https://github.com/ext-sakamoro/ALICE-LOL/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/crates/l/alice-lol.svg)](#ライセンス)

[English](README.md) | 日本語

符号付き距離場 (SDF) を書くためのテキスト言語 `lol!` マクロ (コンパイル時) と
ランタイムパーサが同じテキストを [ALICE-SDF](https://github.com/ext-sakamoro/ALICE-SDF)
の `SdfNode` tree に変換し、CPU 評価・GLSL / WGSL / HLSL への変換・3D プリント用の
メッシュ出力・幾何制約の検査ができる 制約の判定は 3 値 (成立 / 違反 / 未決定)
別 module の `research_law` は、単位を検査した式を成立範囲・残差・出典と一緒に持つ

## 何に向き、何に向かないか

人と言語モデルのどちらでも書ける短いテキストで形状を書くためのもの crate に同梱の
文法 ([`alice-lol/lol.gbnf`](alice-lol/lol.gbnf)) でモデルのデコードを制約すると、
パースできるテキストしか出てこない パーサはそのテキストを評価・描画・印刷できる場に変換する

CAD カーネルではない 境界表現・アセンブリの拘束・STEP 出力は持たない 距離評価、
コンパイル済みバックエンド、シェーダ変換、メッシュ化は ALICE-SDF の担当で、
この crate はその上の言語にあたる 幾何法則の検査は標本格子の上で場について答え、
決められなかったものは合格にせず報告する 有限要素ソルバではない

## インストール

```bash
cargo add alice-lol
```

ソースからビルドする時は、この repository の path 依存 (`alice-sdf`、`alice-zip`、
任意の `alice-physics` / `alice-llm`) を隣のディレクトリに置く
[ビルドとテスト](#ビルドとテスト) を参照

## 使用例

```rust
use alice_lol::lol;
use alice_lol::runtime_parser::parse_lol;
use alice_lol::Vec3;

// Compile time: the macro builds the tree
let scene = lol! {
    smooth_union(0.2,
        sphere(1.0),
        translate(2.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5))
    )
};

// Run time: the same text through the parser (from a file or a language model)
let parsed = parse_lol(
    "smooth_union(0.2, sphere(1.0), translate(2.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5)))",
)
.unwrap();

// Both are the same field; the origin is inside the sphere
let p = Vec3::new(0.3, 0.1, 0.0);
assert_eq!(alice_lol::eval(&scene, p), alice_lol::eval(&parsed, p));
assert!(alice_lol::eval(&scene, Vec3::ZERO) < 0.0);

// GLSL source for a shader (default feature `glsl`)
let glsl = alice_lol::to_glsl(&scene);
assert!(!glsl.is_empty());
```

2 つの入口で 1 つだけ規約が違う `SdfNode::box3d` は **全長**、DSL の `box3d` は
**半長** を取る 同じ形を 2 通りに書いたものを
[`alice-lol/tests/readme_parity.rs`](alice-lol/tests/readme_parity.rs) で突き合わせている

## 位置づけ

| Crate | 担当 |
|-------|------|
| ALICE-LOL | 言語 (マクロ、ランタイムパーサ、文法、`SdfNode` → テキストの逆変換) と法則の検査 |
| [ALICE-SDF](https://github.com/ext-sakamoro/ALICE-SDF) | 距離関数、コンパイル済みバックエンド (scalar / SIMD / BVH)、シェーダ変換、メッシュ化 |
| [ALICE-Physics](https://github.com/ext-sakamoro/ALICE-Physics) | 固定小数点の剛体と、`Constraint::ThermalField` が使う熱伝導ソルバ (feature `physics`) |
| [ALICE-DetMath](https://github.com/ext-sakamoro/ALICE-DetMath) | bit 一致を契約にした `sin` / `cos` / `exp` / `ln` など (プラットフォームの libm の代わりに共有する) |
| [ALICE-Zip](https://github.com/ext-sakamoro/ALICE-Zip) | `research_law` と共有する `law` の語彙 (成立範囲、残差統計、出典、判定ポリシー) |

`alice-det-math` は依存グラフ全体で 1 つの版にそろえる 1 つの tree に 2 つの版があると、
同じ関数の実装が 2 つになる 確認は `cargo tree -i alice-det-math`

## 構文

以下の名前はすべて `alice_lol::runtime_parser::parse_lol` が受け付ける `lol!`
マクロは `stdlib` グループと `capsule_ab` を除いて同じ名前を受け付ける 引数は数値
(マクロでは `{rust_expr}` や裸の識別子も可) と子の式 名前ごとの引数の順序は
[LLM_REFERENCE.md](LLM_REFERENCE.md) と文法にある テキストは `field Name { ... }` で
包んでもよい ランタイムパーサは `//` コメントを受け付けるが、モデルのデコード用の文法は受け付けない

プリミティブ:

<!-- readme-sync: syntax-primitives -->
```text
sphere box3d rounded_box cylinder torus cone capsule capsule_ab ellipsoid
plane octahedron rounded_cone pyramid hex_prism link capped_cone capped_torus
rounded_cylinder tube barrel heart egg helix tetrahedron box_frame diamond
star_polygon cross_shape triangle bezier triangular_prism cut_sphere
cut_hollow_sphere death_star solid_angle rhombus horseshoe vesica
infinite_cylinder infinite_cone gyroid chamfered_cube schwarz_p superellipsoid
rounded_x pie trapezoid parallelogram tunnel uneven_capsule arc_shape moon
blobby_cross parabola_segment regular_polygon stairs_prim dodecahedron
icosahedron truncated_octahedron truncated_icosahedron diamond_surface neovius
lidinoid iwp frd fischer_koch_s pmy circle_2d rect_2d segment_2d
rounded_rect_2d annular_2d terrain
```

CSG 演算 (可変長のものは左畳み込み):

<!-- readme-sync: syntax-csg -->
```text
union smooth_union intersection smooth_intersection subtract smooth_subtract
chamfer_union chamfer_intersection chamfer_subtraction stairs_union
stairs_intersection stairs_subtraction xor pipe engrave groove tongue
columns_union columns_intersection columns_subtraction exp_smooth_union
exp_smooth_intersection exp_smooth_subtraction
```

変換:

<!-- readme-sync: syntax-transforms -->
```text
translate rotate scale scale_non_uniform
```

モディファイア:

<!-- readme-sync: syntax-modifiers -->
```text
round onion twist bend mirror repeat elongate revolution extrude taper
displacement polar_repeat shear noise repeat_finite octant_mirror
icosahedral_symmetry with_material surface_roughness sweep_bezier
```

3D プリントの内部充填 (外殻 + ラティス):

<!-- readme-sync: syntax-print -->
```text
lattice_infill diamond_infill schwarz_infill
```

時間:

<!-- readme-sync: syntax-time -->
```text
animate morph
```

<details>
<summary>製品・機械要素のショートカット (ランタイムパーサのみ)</summary>

<!-- readme-sync: syntax-stdlib -->
```text
shopping_cart_coin skadis_panel skadis_hook_l skadis_hook_j skadis_hook_s
skadis_container skadis_clip skadis_shelf skadis_elastic_cord mug
gridfinity_bin gridfinity_bin_ex wall_hook drawer_organizer shelf_divider
sticky_note_holder business_card_holder pen_cup phone_stand headphone_holder
under_desk_mount desk_shelf monitor_riser coaster tissue_box_cover storage_box
cable_clip led_channel card_tray token_well wrench_holder socket_rail
hex_bit_holder raspi_case esp32_enclosure battery_18650_holder
toothbrush_holder drill_bit_holder pliers_rack spice_rack egg_tray
utensil_caddy filament_spool_holder nozzle_holder build_plate_rack
cutlery_tray pill_organizer magnetic_strip hairdryer_holder kcup_holder
hex_key_holder wrap_holder sock_divider soap_tray razor_holder
chopstick_holder swatch_holder tp_holder sd_card_holder driver_rack
cotton_dispenser sink_caddy clamp_rack dry_box outdoor_enclosure jewelry_stand
phone_dock cutting_board_rack tape_dispenser shower_caddy caliper_holder
bag_clip_org can_rack led_hub_box makeup_organizer vesa_mount l_bracket
t_slot_bracket_2020 raspi_mount_plate heat_set_array flange_mount
dovetail_pair profile_extrusion snap_fit_pair boss_array screw_hole tap_hole
counterbore countersink heat_set_hole bolt bracket_l flange_circular
t_slot_2020 profile_2020 profile_3030 dovetail slot snap_fit_annular
pin_hinge_knuckle boss rib bearing_seat rack_shelf cable_grommet
curtain_rod_bracket dowel_hole wood_screw_pilot arduino_mount_plate
pixhawk_mount servo_mount jst_ph_slot
```

</details>

`runtime_parser::parse_program` は `program(<sdf>, entities(<sdf>, ...), <intent>)` も
読む intent は次の動詞で組む (`rotate` は SDF の変換なので、回転の動詞はテキストでは `turn`)

<!-- readme-sync: syntax-intent -->
```text
grasp release catch walk gaze point throw push pull turn align follow avoid
rest latent seq par music
```

## 監査法則 (`audit_law`)

監査法則は検査が何を見つけるべきかを述べ、どう測るかは書かない 1 行 1 項で、
`parse_law` が読み `AuditLaw::evaluate` が 6 値の判定 (`Supports` / `NoEvidence` /
`Breaks` / `OutOfRange` / `ParameterUpdate` / `Undecided`) を返す
数値を渡すのは検査を走らせる側なので、同じ法則を Rust からでも別の言語からでも
食わせられる

<!-- readme-sync: syntax-law -->
```text
audit evidence expect range
```

`evidence <metric>` はその量が測られたこと自体を要求する — 1 件も比べていない検査は
合格ではなく `NoEvidence` を返す 証拠になるのは有限で 0 より大きい数だけで、0・負の数・
NaN・無限大は証拠にならない `expect <metric> == <value> [within <tol>]` が期待値、
`range <key> <value>...` は既知の違反を記録し、実測と一致する間は許容され、行が
当てはまらなくなれば `OutOfRange`、値の組が動けば `ParameterUpdate` になる
値の組は集合として比べる (順序と重複は判定に影響しない)
証拠は期待値より先に判定するので、空の実測で `0 == 0` が偶然通ることはない

## Law の識別子 (`law_id`)

保存した Law と評価する Law が同じものだと言えるのは、同じ名前が付いている時だけ
`AuditLaw::law_id` は domain / Law の種類 / 算術の世代 / 主張そのものから 32 byte の
識別子を導く 値ごとに型と長さを書くので、`["ab", "c"]` と `["a", "bc"]`、`u32 5` と
`u64 5`、空の項と項の不在がそれぞれ別の識別子になる

算術の世代は名前の一部 同じ text でも評価する算術が違えば答えが変わりうるので、
別の Law として扱う `LOL_SEMANTICS_ID` は `LOL_SEMANTICS_PINS` の fold で、pin の値は
書き下すのでなく振る舞いから計算する (`audit_verdict_order_fingerprint`、
`law_input::input_reading_fingerprint`、
`research_law::expression_functions_fingerprint`、
`law_input::derivation_fingerprint`) ので、判定の順序や request の入力の読み方を
変えると pin が合わなくなる 監査の Law の識別子は `x-input` の行の型も含む
(`law_input::audit_law_from_file`) 幾何法則と研究 Law の識別子はまだ無い

Example: [`law_id_demo`](alice-lol/examples/law_id_demo.rs)、oracle:
[`alice-lol/tests/law_id_oracle.rs`](alice-lol/tests/law_id_oracle.rs)

## 幾何法則 (`law`)

`law::Constraint` は場の性質を宣言し、`LawSet` が Hard / Soft の優先度つきで集めて
格子の上で検査する 標本点ごとに 成立・違反・**未決定** のどれかになり、未決定の点は
合格に数えず `LawReport::unresolved` に載る `LawReport::hard_verdict` は Hard の制約に
ついて `Proven` / `Violated` / `Undecided` を返す

距離に依存する制約は場の値を距離として読まない (TPMS の面や union の内部では誤るため)
箱に表面が無いことは区間演算で証明し、表面までの距離の上界は評価した 2 点の符号の変化から取る

制約ごとに根拠の種類がある (`Constraint::evidence_class`) `Modelled` の制約はモデル
(格子解像度、閾値、代理量) に依存するので `Priority::Hard` では追加できない

<!-- readme-sync: laws -->
| Constraint | 検査する内容 | 根拠 |
|------------|--------------|------|
| `NonOverlap` | 2 つの場が重ならない | `Witnessed` |
| `Containment` | 一方の場がもう一方の内側にある | `Witnessed` |
| `MinThickness` | 肉厚が指定値以上 | `Witnessed` |
| `Stress` | 荷重点の近くの肉厚が荷重に見合う (幾何の代理量) | `Modelled` |
| `Thermal` | 熱源の近くの表面積 / 体積比 (幾何の代理量) | `Modelled` |
| `Contact` | 2 つの場の隙間が範囲内 | `Witnessed` |
| `Continuity` | 内部が 1 つの連結領域 (flood fill) | `Modelled` |
| `GradientBound` | 場の勾配が上限を超えない | `Witnessed` |
| `Reachable` | 内部の 2 点が内部を通って繋がる | `Proved` |
| `VolumeConservation` | 変化の前後で体積が許容誤差内で一致する | `Modelled` |
| `ThermalField` (feature `physics`) | 解いた温度場の最高温度が上限以下 (断熱と等温の両端で挟む) | `Modelled` |

閉形式の oracle: [`alice-lol/tests/analytic_law.rs`](alice-lol/tests/analytic_law.rs)、
[`alice-lol/tests/test_field_law_oracle.rs`](alice-lol/tests/test_field_law_oracle.rs)
文法 corpus 全件を総当たりの反証器と突き合わせる oracle:
[`alice-lol/tests/law_corpus_oracle.rs`](alice-lol/tests/law_corpus_oracle.rs)

## 研究 Law (`research_law`)

`research_law` は幾何の `law` module とは別物 `ResearchLaw` はデータについての主張
`output = f(inputs; parameters)` で、テキストの式 (`n*R*T/V`) と次のものを一緒に持つ

- **単位と次元**: 入力・出力・parameter が単位 (`Pa`、`kPa`、`J/(mol*K)`、`m^3`、`L` など)
  を持ち、構築時に式の次元を検査する (`+` / `-` は同じ次元、`exp` / `ln` / `sin` / `cos`
  の引数は無次元、結果は出力の次元)
- **成立範囲**: 入力が範囲の外なら評価しない (`OutOfRange`) 外挿はせず、途中の値が
  有限でなければ NaN ではなく error を返す
- **残差**: 保持した観測から測った値 (申告値は持たない)
- **出典** と **oracle**: 式の出どころと、許容誤差内で再現すべき参照値 (`check_oracles`)
- **比較**: `compare` は単位の違う 2 つの式を `Bridge` (入力名の対応と単位換算) で同じ条件で評価する
- **新しい証拠**: `ingest` は新しい観測を `alice_zip::law::SignalLaw::ingest` と同じ規則順で
  判定する parameter 更新は全 parameter の最小二乗 (反復上限と収束閾値を固定した Gauss–Newton)

<!-- readme-sync: research-verdicts -->
| 判定 | 意味 |
|------|------|
| `NoEvidence` | 観測が無い |
| `OutOfRange` | 評価できない条件の観測があり、何も判定していない |
| `Supports` | 新しい観測が帯の内側で式と合う |
| `ParameterUpdate` | parameter を当て直すと、保持した証拠と新しい証拠を同じ式で説明できる |
| `ResidualGrew` | ずれが帯を超えたが、破綻の閾値は超えない |
| `Breaks` | この式では証拠を説明できない |

式の超越関数 (`exp`、`ln`、`sqrt`、累乗、`sin`、`cos`) はプラットフォームの数学
ライブラリではなく `alice-det-math` の `f64` 関数で評価し、評価は fused multiply-add を
使わない決まった順の `f64` 演算なので、同じ入力はどのプラットフォームでも同じ bit になる
成立範囲・残差・出典・判定ポリシーの型は `alice_zip::law` のもの
example: [`research_law_demo`](alice-lol/examples/research_law_demo.rs)、
oracle: [`alice-lol/tests/analytic_research_law.rs`](alice-lol/tests/analytic_research_law.rs)

## この workspace の crate

| Crate | 役割 |
|-------|------|
| `alice-lol-macro` | `lol!` proc-macro |
| `alice-lol` | ランタイムパーサ、逆変換、シェーダ変換と出力の関数、法則の検査、intent IR、stdlib の形状 |
| `alice-lol-humanoid` | パラメトリックな人型テンプレート (関節 FK、VRM / BVH 読み込み) |
| `alice-lol-robot` | intent の動詞から人型 FK と 8 byte の kinematics packet へ、安全法則つき |
| `alice-lol-ui` | UI 形状 (button、card、panel など)、flex / grid / stack レイアウト、コントラストの法則 |
| `alice-lol-datagen` | 自己検査つきの合成 (caption, LOL) ペア生成 (非公開 crate) |
| `alice-world-auditor-types` | 法則の検査器とプランナが共有する goal と判定の型 |
| `alice-world-auditor` | `alice-physics` の world 上のプランナ、3 値判定 (AGPL-3.0-or-later または商用) |

## Feature

<!-- readme-sync: features -->
| Feature | 既定 | 内容 |
|---------|------|------|
| `glsl` | 有効 | GLSL への変換 (`to_glsl`、`to_glsl_dynamic`、`to_glsl_full`) |
| `wgsl` | 無効 | WGSL への変換 |
| `hlsl` | 無効 | HLSL への変換 |
| `physics` | 無効 | `Constraint::ThermalField` と `alice-physics` の材料データ (AGPL-3.0-or-later、有効にすると下流に及ぶ) |
| `roblox` | 無効 | Roblox のメッシュ上限に収めた OBJ / FBX 出力 (`roblox_export`) |
| `llm-bridge` | 無効 | `alice-llm` による文法制約つき生成 (`bridge`、AGPL-3.0-or-later、下流に及ぶ) |

## Example

| Example | 内容 |
|---------|------|
| [`basic`](alice-lol/examples/basic.rs) | マクロと GLSL 出力 |
| [`showcase`](alice-lol/examples/showcase.rs) | 構文の一巡、変数キャプチャ、自動微分、`CompiledSdf` |
| [`law_demo`](alice-lol/examples/law_demo.rs) | 幾何法則の宣言と検査 |
| [`research_law_demo`](alice-lol/examples/research_law_demo.rs) | 理想気体の式を SI と (kPa, L) で書き、再計算・比較・oracle・新しい証拠の判定 |
| [`audit_law_demo`](alice-lol/examples/audit_law_demo.rs) | 検査を監査 Law として書き、6 値の判定に到達させる |
| [`law_id_demo`](alice-lol/examples/law_id_demo.rs) | Law の識別子が何から決まり、何を変えると変わるか |
| [`audit_conformance`](alice-lol/examples/audit_conformance.rs) | `conformance/TASK.md` の契約を監査の law について `law_input` で実装した CLI (`conformance/probes.json` で確かめる) |
| [`export_formats`](alice-lol/examples/export_formats.rs) | 同じ mesh を STL / 3MF / FBX に書き、解像度 preset を並べる |
| [`pruning_demo`](alice-lol/examples/pruning_demo.rs) | 区間演算による格子セルごとの枝刈り |
| [`autodiff_demo`](alice-lol/examples/autodiff_demo.rs) | 勾配、曲率、ヘッセ行列 |
| [`compiled_demo`](alice-lol/examples/compiled_demo.rs) | コンパイル済み評価 (1 点、SIMD バッチ、法線) |
| [`print_export`](alice-lol/examples/print_export.rs) | STL / 3MF 出力 |
| [`print_verify`](alice-lol/examples/print_verify.rs) | ラティス充填をメッシュと数値で照合 |
| [`roblox_accessory`](alice-lol/examples/roblox_accessory.rs) | Roblox アクセサリ出力 (feature `roblox`) |
| [`prompt_to_sword`](alice-lol/examples/prompt_to_sword.rs) | プロンプト → 文法制約つき LOL → メッシュ (feature `llm-bridge`) |
| [`alice_coaster`](alice-lol/examples/alice_coaster.rs) | SDF の模様で作る 10cm 丸型コースター |
| [`coin_dc_vs_mc`](alice-lol/examples/coin_dc_vs_mc.rs) | 1.7mm の薄い coin で marching cubes と dual contouring を比較 |
| [`complete_pipeline_output`](alice-lol/examples/complete_pipeline_output.rs) | カタログの全品目を 1 回で 3MF 出力 |
| [`hardsurface_bolt_plate`](alice-lol/examples/hardsurface_bolt_plate.rs) | 締結の primitive |
| [`hardsurface_snap_case`](alice-lol/examples/hardsurface_snap_case.rs) | 組立の primitive |
| [`hardsurface_ribbed_bracket`](alice-lol/examples/hardsurface_ribbed_bracket.rs) | 補強の primitive |
| [`hardsurface_wall_bracket`](alice-lol/examples/hardsurface_wall_bracket.rs) | 締結・組立・補強・取付を 1 つの部品にまとめた例 |
| [`pattern_catalog`](alice-lol/examples/pattern_catalog.rs) | pattern registry のカタログ表示 |
| [`print_demo`](alice-lol/examples/print_demo.rs) | 3D プリント向けの構造意図 |
| [`royal_crown`](alice-lol/examples/royal_crown.rs) | 装飾的な王冠を OBJ で出力 |
| [`skadis_panel_dc_vs_mc`](alice-lol/examples/skadis_panel_dc_vs_mc.rs) | 穴あきパネルで marching cubes と dual contouring を比較 |
| [`verify_customizers`](alice-lol/examples/verify_customizers.rs) | customizer の原型が期待した形になることの確認 |
| [`llm_bench`](alice-lol/examples/llm_bench.rs) | 文法制約デコードと think→文法の pass 率の比較 (feature `llm-bridge`) |
| [`gridfinity_untrusted_input`](alice-lol/examples/gridfinity_untrusted_input.rs) | dividers の個数が退化している時、panic でなく `try_gridfinity_bin` が拒否で返す例 |
| [`parse_one_lol`](alice-lol/examples/parse_one_lol.rs) | メモリ上限つき allocator を持つ単発の `parse_lol` probe、`tests/degenerate_grid.rs` が生成した各 case をこれ 1 回ずつの別 process で走らせる |

実行は `cargo run -p alice-lol --example <name>`

## MSRV

最小対応 Rust バージョン: **1.90** <!-- readme-sync: msrv --> CI はこの toolchain で
workspace 全体を検査する

## ビルドとテスト

crate は隣の repository に path で依存するので、並べて clone する

```bash
git clone https://github.com/ext-sakamoro/ALICE-LOL
git clone https://github.com/ext-sakamoro/ALICE-SDF
git clone https://github.com/ext-sakamoro/ALICE-Zip
git clone https://github.com/ext-sakamoro/ALICE-Physics     # feature `physics`
git clone https://github.com/ext-sakamoro/ALICE-LLM         # feature `llm-bridge`
git clone https://github.com/ext-sakamoro/ALICE-Kinematics  # alice-lol-robot
cd ALICE-LOL
cargo test
scripts/law_tests.sh            # the law oracles, failing if any of them ran zero tests
python scripts/readme_sync.py --check
python scripts/docs_lint.py --check
scripts/preflight.sh --quick    # the CI gates that do not run the test suites
```

## 関連 crate

- [ALICE-SDF](https://github.com/ext-sakamoro/ALICE-SDF): この言語が記述する場
- [ALICE-Physics](https://github.com/ext-sakamoro/ALICE-Physics): 決定論的な物理
- [ALICE-DetMath](https://github.com/ext-sakamoro/ALICE-DetMath): bit 一致の数学関数
- [ALICE-Zip](https://github.com/ext-sakamoro/ALICE-Zip): `research_law` と共有する `law` の語彙
- [ALICE-Synth](https://github.com/ext-sakamoro/ALICE-Synth): `music` intent の 8 byte の中身を定義する
- [ALICE-View](https://github.com/ext-sakamoro/ALICE-View): wgpu レンダラ

## ライセンス

0.4.0 から Apache-2.0 ([LICENSE-APACHE](LICENSE-APACHE)、[NOTICE](NOTICE)、[TRADEMARK_NOTICE](TRADEMARK_NOTICE))
0.3.x 以前は MIT OR Apache-2.0 で公開しており、その版の条件はそのまま
ただし `alice-world-auditor` は AGPL-3.0-or-later または商用 feature `physics` と
`llm-bridge` は AGPL-3.0-or-later の依存を引き込む

再配布 (source でも binary でも) には [LICENSE-APACHE](LICENSE-APACHE) と [NOTICE](NOTICE) を含める
NOTICE は他の license の text と同じ場所に置けばよく、画面に表示する必要はない
「ALICE」と「ALICE-*」の名前は商標で、license に依らず [TRADEMARK_NOTICE](TRADEMARK_NOTICE) に従う

DSL は ALICE-SDF のプリミティブ一式を公開しているので、その crate の帰属表示を引き継ぐ
距離関数の形の多くは Inigo Quilez の公開記事に、stairs / columns / chamfer 演算は
Mercury の hg_sdf に、ノイズの勾配表は Ken Perlin に倣っている 実装は ALICE-SDF 自身の
Rust コードで、一覧は
[ALICE-SDF/THIRD-PARTY-NOTICES.md](https://github.com/ext-sakamoro/ALICE-SDF/blob/main/THIRD-PARTY-NOTICES.md) にある
