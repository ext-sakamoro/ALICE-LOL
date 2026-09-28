# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed — 残り 3 法則を三値化し、場の値の距離への流用をやめる (2026-09-28)

`Thermal` / `Continuity` / `VolumeConservation` は `check_one` の dispatch で
`unresolved: None` が literal 固定され、戻り値の型 (`Option<Violation>`) が
**三値を表現できなかった**。0.4.0 の「判定不能を合格にしない」原則がこの 3 つ
だけ効いておらず、`LawReport::all_passed()` が証明なしで `true` を返していた。

合わせて、**合格側の嘘** も 3 件潰した。違反の検出が健全でも、満たしている
形状を違反と断言すれば gate としては同じく壊れる。

- **`Thermal`** — 表面近傍を `|f(center)| < step` で取っていた。これは **場の値を
  表面までの距離として流用** しており、場が真の距離を過大申告する node
  (TPMS は 1.7〜7.0 倍) では帯がその分痩せ、表面セルを取りこぼして比を
  過小評価する。実測: `gyroid(1.0, 0.3)` は真の表面比 1.5615 (角の符号反転 =
  中間値定理で独立計算) に対し、閾値 1.4834 で違反と断言していた。
  場の値を使わず区間演算の三分類だけで比を上下から挟む方式に置換。
- **`Continuity`** — **セル中心の点標本だけ** で内部 mask を作っていたため、
  格子より細い接続 (首 / 薄板) は中心が 1 つも内部に落ちず、繋がっている
  形状を分離と断言していた。実測: 半径 0.1 の首で繋いだ 2 球を res 8 / 16 /
  24 のどれでも「分離」と報告。`Reachable` と同じ区間演算の三分類に移した。
  違反は「外部確定でないセル全部を使っても届かない」= 分離の証明。
- **`VolumeConservation`** — セル中心の count は、どの中心が内側に落ちるかで
  動く。実測: **体積が厳密に保存される平行移動**に対し res=8 shift=0.25 で
  12.5% の差を申告していた。セルごとの占有割合の上下界から
  `V_after − V_before` を挟む方式に置換。未定セルを一律 ±1 にすると向きが
  決まっている場合まで両振れ扱いになって検出力を落とすので、符号の組合せ
  ごとに寄与の上下界を取る。
- `UnresolvedReason` に `SurfaceRatioUnbracketed { lo, hi }` /
  `VolumeUnbracketed { lo, hi }` を追加 (`#[non_exhaustive]` なので非破壊)。

**挙動の変化**: `volume_conservation_identity` 相当 (同一 SDF) が **合格から
未定に変わる**。0.4.0 は点標本で差 0 を得ていたが、それは「たまたま同じ中心が
拾われた」だけで体積一致の証明ではない (同じ数え方が平行移動には 12% を
申告する)。区間で数えると内外が確定しないセルが内部確定セルより多く、
相対差が許容 5% を下回ることを**この解像度では証明できない**。格子 count で
5% を示すには殻 / 体積比を 5% 未満にする必要があり、比は O(1/n) でしか
縮まないので、実用解像度では未定が既定になる。検証器の後退ではなく、
元々できていなかったことが見えた形。

### Added — 3 法則の解析解 oracle と表面帯の白箱 gate (2026-09-28)

`tests/analytic_law.rs` (17 → 20 本)。この 3 法則は oracle が 1 本も無く、
`law_tests.rs` 側にあるのは**実装の出力を pin した変化検出器**だった。

- `thermal_surface_ratio_must_not_shrink_with_the_field_scale` — 真の表面比を
  角の符号反転で独立に数え、その 95% を閾値にする。場 = 真の距離の球を
  対照に置き、格子 count 自体の問題でないことを切り分ける
- `continuity_must_not_call_a_thin_neck_disconnected`
- `volume_conservation_must_not_flag_a_translation`

3 本とも実装より先に commit し、旧実装に対して red を実測してから置換した。

`law::evidence_gate_tests::surface_band_does_not_depend_on_the_field_scale` —
上の Thermal oracle は**三値化の側だけを pin していて、表面帯の判定基準の
差までは捕まえられなかった** (`|f| < step` に戻しても比の上界が閾値を上回る
ので verdict が動かない、実測)。基準そのものを白箱で固定する gate を別途
置き、壊して red を確認した (上界 0.3087 < 必要 0.3667)。scene は「内部確定
セルが多い」かつ「場の帯が取りこぼす」の両方が要り、薄い `gyroid(1.0, 0.3)`
では内部確定セルが 0 になって測れないので `gyroid(0.6, 1.2)` を使う。

### Changed — 証明のない法則に `Priority::Hard` を名乗らせない (2026-09-28、breaking)

検証器が「違反である」と言う時の裏付けには強さの差がある。0.4.0 まで、
**格子解像度に依存した推定が証明と同じ重み (`Priority::Hard`) で報告** されて
いた。`Stress` / `Thermal` / `Continuity` / `VolumeConservation` の 4 つが
これに当たり、しかも `LawSet` の convenience がその 4 つで `Hard` を
hardcode していたため、**推定を Hard 以外で積む導線が存在しなかった**。

物理 backend (`alice-physics`) を繋いでもこの区別は消えない。「モデルである」
ことと「モデルが良い」ことは別の話なので、根拠の種類を型で持たせる。

- **`Evidence` を追加** (`Proved` / `Witnessed` / `Modelled { model }`、
  `#[non_exhaustive]`、検証器が構築し利用側は読むだけ)。`Proved` は区間演算の
  包含による証明 (標本の取り方に依存しない)、`Witnessed` は中間値定理 / 点評価
  で実際に見つけた反例、`Modelled` は推定。質的な差は **前 2 つと `Modelled`
  の間だけ** で、そこが gate になる (`Evidence::is_proof`)。
- **`Constraint::evidence_class()` / `Constraint::name()` を追加**。
  `Reachable` = `Proved` / `NonOverlap`・`Containment`・`MinThickness`・
  `Contact`・`GradientBound` = `Witnessed` / `Stress`・`Thermal`・`Continuity`・
  `VolumeConservation` = `Modelled`。
- **`Law::hard` が `Result<Self, NotProvable>` を返す** (breaking)。モデル推定
  の制約には `NotProvable { constraint, model }` を返す。**黙って `Soft` に
  降格させない** — 降格を検証器が決めると「証明なしの Hard 違反」が別の形で
  復活するので、呼び出し側に選ばせる。`LawSet::hard` も同じ。
- **`Violation::evidence` を追加** (breaking)。`evidence_class` が「その制約が
  返せる最強の根拠」なのに対し、こちらは **その 1 件が実際に何に依ったか**。
  同じ `Contact` でも干渉は `Witnessed`、離れすぎ (`gap_exceeds` の区間証明)
  は `Proved` になる。
- **`LawSet` convenience の引数と優先度が変わった** (breaking)。
  `stress` / `thermal` / `continuity` / `volume_conservation` は `weight: f32`
  を取り `Soft` で積む。`contact` / `gradient_bound` / `reachable` は `Hard`
  のまま (gate を通す必要がないので内部の infallible 経路を使う)。
- **`LawReport::proven_violations()` を追加** — `Priority` と直交する軸で、
  モデル推定でなく証明 / 反例に裏付けられた違反だけを返す。
- **`format_report` が `basis=proved|witnessed|modelled` を出す**。`Modelled`
  は `model:` 行で **何を仮定したか** まで出す (読み手が重みを判断できない
  報告にしない)。
- `Stress` の model 文字列に、要求値 `force × min_thickness_factor` が無次元の
  heuristic で材料 / 断面係数 / 降伏応力を持たないことを明記。`Thermal` には
  表面帯の判定が `|f| < step` = **場の値の距離への流用** であることを明記
  (TPMS では場が真の距離の 1.7〜7.0 倍)。

移行: `Law::hard(n, c)` → 証明 / 反例つきなら `?` か `.expect(..)`、
`Stress` 等は `Law::soft(n, weight, c)`。`LawSet::stress(n, node, ..)` →
`LawSet::stress(n, weight, node, ..)`。

### Added — 根拠 gate の機械検査 (2026-09-28)

`Constraint::evidence_class` は手で書いた表なので、**実装が実際に返す
`Violation::evidence` と食い違っても誰も気付かない**。gate が形だけ残って
意味を失うのを防ぐため、`law::evidence_gate_tests` 5 本を追加:

- `all_variants_are_classified` — variant を足したら落ちる (分類漏れの検出)
- `hard_is_gated_by_evidence_class` — 10 variant すべてで gate の可否が
  `evidence_class` と一致し、`NotProvable` が対処法を含む
- `modelled_constraints_cannot_produce_hard_violations` — 実際に違反が出る
  形で `has_hard_violations()` / `proven_violations()` が空
- `reported_evidence_never_exceeds_the_declared_class` — 実装が返す根拠が
  申告した class より**強く**なっていない (表が実装から遅れていないか)
- `lawset_convenience_keeps_modelled_laws_soft` — convenience が Hard を
  作り直していない
- `tests/analytic_law.rs::stress_cannot_claim_hard_priority` (16 → 17 本)

gate は書いた直後に壊して red を実測済: `Law::hard` の gate を外すと
`hard_is_gated_by_evidence_class` が、convenience を `Hard` hardcode に戻すと
`lawset_convenience_keeps_modelled_laws_soft` と
`modelled_constraints_cannot_produce_hard_violations` が落ち、
`Law::hard_unchecked` の `debug_assert!` が 3 層目として発火する。

### Changed — alice-sdf 4.0.0 追従 (2026-09-28)

- **`alice-sdf` 要件 `3.0.0` → `4.0.0`** — 4.0.0 は計量そのものを値にする 2 node と場の主張を測る 2 API を追加し、`SdfNode` / `OpCode` を `#[non_exhaustive]` にした 外部 crate からの wildcard なし `match` は `E0004` になるので `emit::write_node_inner` に `MetricBall` / `MetricBlend` の arm と wildcard arm を追加 (未知 variant は黙って捨てず `EmitError::Unsupported` として報告する)
- **`law::interval_sign` の unit test を `Interval { lo, hi }` の struct literal 構築に変更** — alice-sdf 4.0 から `Interval::new` が外側丸め (`next_down` / `next_up`) を掛けるので、`new(0.0, 1.0)` の lo は 0 のわずか下になる すると「lo が厳密に 0 なら外側と断定する」という**検査対象そのものの性質**が測れなくなる (答えが `None` に化ける) ので、境界を動かさない構築に寄せた
- `Cargo.lock` / `fuzz/Cargo.lock` — `alice-sdf` 4.0.0 / `alice-det-math` 0.3.1 に更新

### Added — 場の勾配と 2 点間到達性を法則として書けるようにする (2026-09-27)

どちらも 3 値 (合格 / 違反 / 未定) がすべて**証明**になっている。「標本で
見つからなかった」を合格に繰り上げないことが、gate として意味を持つ条件。

- **`Constraint::GradientBound { node, max_gradient, probe }`** — 領域上で場の
  勾配が上界を超えないことを検証。距離場の勾配が 1 を超えると場は真の距離
  より大きい値を申告し (`|f(p)| ≤ L · dist(p)`)、sphere tracing が `f(p)` だけ
  進むと面を踏み越える。**合格は `eval_lipschitz` が上界以下という証明**
  (標本を 1 点も見ない)、**違反は上界を超える標本対という証拠**、どちらも出
  なければ `UnresolvedReason::GradientUnwitnessed { claimed, worst_sampled }`。
  標本対は `eval_lipschitz` の保証域 (外部) に合わせ、両端が内部の対は見ない。
- **`Constraint::Reachable { node, from, to }`** — 2 点間の到達性。
  [`Continuity`] が「内部が 1 つながりか」を見るのに対し、こちらは「この 2 点
  が繋がっているか」= **詰みの検出**。セルを区間演算で 3 分類し、**内部と確定
  したセルだけの flood fill で届けば合格** (経路が証拠)、**外部と確定していな
  いセル全部を使っても届かなければ違反** (どんな経路も外部確定セルを通る)、
  その間は `UnresolvedReason::ReachabilityUndecided { undecided_cells }`。
  区間演算に外側丸めが無く包含が真の値域より狭くなる drift (実測 相対
  2.086e-4、`stairs_intersection`) があるため、3 分類は `INTERVAL_SLACK`
  (1.0e-3 × セル対角) の余裕を取って**境界を未定側に倒す** — 合格も違反も
  緩めず、判定できない側に寄せるだけなので健全性は保たれる。
- `LawSet::gradient_bound` / `LawSet::reachable` の convenience 2 本。
- **`tests/test_field_law_oracle.rs`** (10 本) — 2 法則それぞれで 3 値が 3 分岐
  とも出ることを個別に + まとめて 1 本ずつ。未定は「検証器の解像度不足」で
  あって「繋がっていない」ではないので、解像度を上げると未定が合格に変わる
  ことも押さえた。
- **`tests/law_corpus_oracle.rs`** に probe 2 本追加 — (a) 静的上界を上界に
  渡すと corpus のどの construct でも違反が出ない (出たら `eval_lipschitz` の
  主張が破れている、SDF 側 property test と独立な経路での再検査、対象 50+
  construct) (b) 到達不能と「証明」した scene は、判定器と独立な素の点評価に
  よる flood fill で経路が見つかってはいけない (false red 検査)。


### Added — 判定器の corpus oracle と検出力 gate (2026-09-27)

- **`tests/law_corpus_oracle.rs`** — grammar corpus 全 construct + 深い合成
  fixture (計 244) を **総当たり反証器**と突き合わせる。`analytic_law.rs` が
  手計算できる 16 scene を見るのに対し、こちらは LOL の入口 (`parse_lol`) から
  到達できる形すべてに対して 2 方向を見る:
  - **false green** — 素の点評価 (区間演算を使わない独立実装) が「両方の内部に
    ある点」を実際に見つけたのに、判定器が `all_passed()` (= 証明付き合格) を
    返したら fail。未決定は合格ではないので許容する。
  - **false red** — hard violation を報告したなら、**その報告点で実際に両方が
    負**でなければ fail (落ちた時だけ近傍を総当たりして切り分ける)。
  - あわせて**未決定率を実測して print** する (resolution 4 / 8 / 16)。実測:
    `MinThickness(0.2)` で決着率 87.7 % → 92.2 % → 99.6 %。
  - 初回実行で `flange_mount` の **偽の証明付き合格**を検出した。真因は
    `alice-sdf` の `PolarRepeat` 区間 (`count ≤ 2` で扇形が潰れる) で、
    ALICE-SDF 3.1.1 側で修正済み。
- **`.github/workflows/quality-deep.yml`** (canonical: ALICE-Physics) —
  `cargo-mutants` で判定器 `law.rs` の**検出力**を測り、**生存変異 0 を gate**
  にする。law.rs / 判定器 test を触った PR と週次 + 手動で実行 (1 mutant ≈
  25 s build + 50 s test なので push 毎には回さない)。multi-repo layout では
  `--in-place` が必須 (tree copy で sibling path dep が切れる) なので `-j 1`。
  等価変異は **理由付きで `mutants.toml` に除外**を書く運用にした。
- **`law::core_probe_tests` に 3 本追加** (2026-09-27 の mutants 実測で生き残って
  いた変異を塞ぐ):
  - `gap_exceeds_does_not_clear_a_box_whose_interval_touches_zero` — 区間の下界が
    **厳密に 0.0** (表面に接する) の箱を「離れている」と刈ると、証明していない
    gap を証明済と言う。`> 0.0` → `>= 0.0` の変異 2 件 (a 側 / b 側) を殺す。
  - `contact_upper_bound_radius_includes_the_cell_diagonal` — 探索半径から cell
    対角が落ちると上界を見失う。`(aabb_max − aabb_min)` の `−` を `+` / `/` に
    する変異 2 件を殺す。
  - `contact_upper_bound_only_samples_points_outside_both` — 上界の標本点は両方の
    外側に限る。`fa <= 0.0 || fb <= 0.0` を `&&` にする変異を殺す。

### Changed — breaking: 法則検証器が場の値を距離として使わなくなった (0.4.0)
- **距離依存 5 variant (`MinThickness` / `Stress` / `NonOverlap` / `Containment` / `Contact`) を Lipschitz 非依存の三値判定に置換** — 旧実装は `sdf_eval` の返り値を距離として使っていたので、TPMS (場が距離を √3〜7 倍に過大申告) では 0.058 の薄壁を 0.1 と読んで合格させ、union の内部 (場が過小) では 0.87 の肉厚を 0.5 と読んで不合格にし、格子より薄い重なり / はみ出しは標本点をすり抜けていた (セルフレビュー 2026-09-16 § 4「検証器が嘘をつく」) 新実装は `alice_sdf::interval::eval_interval` (区間演算) で「箱に表面なし」を **証明**、点評価の符号変化 (中間値定理) で「表面まで ≤ |p − q|」の **証拠** (二分探索で締めた上界) を取り、八分木深さ `BALL_PROBE_DEPTH = 4` で決められなかった標本点は **unresolved** として報告する 違反の検出は健全 (偽陽性なし)、合格は標本点ごとの証明
- **`LawReport` に `unresolved: Vec<Unresolved>` を追加、`all_passed()` は「違反なし かつ 判定不能なし」に変更** (silent 合格の廃止、struct literal で `LawReport` を組んでいる下流は field 追加で breaking) 新 API: `LawReport::has_unresolved()` / `Unresolved { law_name, priority, point, region, reason }` / `UnresolvedReason` (`SurfaceProximity { radius }` / `SignUndecided` / `GapUnbracketed { upper }`、`#[non_exhaustive]`) / `format_report` に `[UNDECIDED]` 行
- `Contact` の residual: 近すぎは `上界 − min_distance`、遠すぎは `max_distance − 上界` (上界を取れなければ `−∞`)、gap > m の証明は各 cell を m/2 広げた箱の区間で行う (中点が検査 AABB 内にある前提)

### Added
- **`law.rs` に判定器の核の unit test 5 本 (`core_probe_tests`)** — `cargo mutants`
  (433 mutant / 104 missed) の生存変異の精査結果 missed のうち 61 件は sound 化
  対象外の 3 law (`check_continuity` 30 / `check_thermal` 16 /
  `check_volume_conservation` 15、既知) で、残り 24 件が sound 化した 5 law の核に
  あった そのうち **実際に test で殺せた 2 系統** を塞いだ:
  `interval_sign` の `hi < 0.0` → `<= 0.0` (表面ちょうどの箱を内側と断定する偽陽性)
  と `gap_exceeds` の `depth >= BALL_PROBE_DEPTH` → `<` (深さ 0 で即 `return false`)
  後者は **「細分に入る」配置が必須**で、`GridSampler` の cell 境界が
  `aabb_min + k·(extent/resolution)` である以上 resolution を奇数にして中央 cell を
  原点にまたがせる必要がある (配置を 2 回外してから通した)
  残る missed は等価 (`box_children` の `i & 1 != 0` は 8 child 全走で集合不変 等) か
  到達困難 (`probe_ball` の `best` 比較は刈り込みが先に効く / 境界を浮動小数で作れ
  ない) で、理由を `law.rs` の module doc に列挙した `BallProbe` に `Debug` を
  derive (private enum、test の失敗時に返った variant を出すため)
- **`tests/analytic_law.rs` — 法則検証器の解析解 oracle 9 本** (oracle 先行で red 4/7 を確認してから実装): gyroid 板の真の半厚 (平坦点で g ≈ √3·s、ε = 0.05778) / 2 球 union 内部の真の距離 (√0.75) / 球殻 R − r の境界 / 2 球 NonOverlap の侵入深さ上界 / 内球 Containment のはみ出し量 / Contact gap = 0.5 の 3 範囲 / 板 Stress / `InfiniteCone` (区間 EVERYTHING) が unresolved になること / `resolution` 1..8 で verdict 不変
- **`tests/transpiler_naga_validate.rs` — grammar corpus 全構文 + fixture の WGSL / GLSL を naga で parse + validate** (Level 1.5、GPU 不要) 初回実行で alice-sdf 3.0.0 の `Terrain` が WGSL / HLSL に GLSL 構文を直書きし GLSL でも `vnoise` helper 未定義であることを検出 (SDF 側 Backlog)
- **`tests/gpu_parity.rs` — grammar corpus 全構文 + 深い合成 fixture 7 本を実 GPU で実行して CPU `eval` と突合** (Level 2、Milestone A.4.1) + ci.yml `gpu-parity` job (lavapipe、`ALICE_SDF_REQUIRE_GPU=1`) 初回実行 (Metal) で fixture 7 本は drift ≤ 5e-6、corpus 237 中 235 一致、`Elongate` の CPU (`p − clamp(p, −a, a)`) と shader (`abs(p) − a` + 内部補正) の法則不一致と `Terrain` の WGSL 不正を検出 (いずれも alice-sdf 側、Backlog)
- `tests/common/corpus.rs` — grammar corpus / fixture / 標本点生成を `emit_roundtrip` / `transpiler_naga_validate` / `gpu_parity` で共有
- **fuzz target `fuzz_lol_emit_parity`** (parse → emit → parse の eval parity + emit 冪等性、strict-eval 8b) fuzz.yml matrix に追加、local 60 s / 1.27M run で crash 0
- ci.yml: `msrv` job (`cargo +1.90 check --workspace --all-targets --all-features`)、`gpu-parity` job

### Changed
- **`rust-version = "1.90"` を全 workspace crate に宣言** — clippy `incompatible_msrv` が `from_f32_snap` 等の const fn で `f32::round` (const 化 1.90) と `Vec::is_empty` (1.87) を指摘、1.85 (alice-sdf の MSRV) を宣言すると偽 MSRV になる 下流 (Manga / Foundry / Print / Kinematics 1.92.0、Bamboo / LLM 1.98.1) の toolchain pin は全て上回る `cargo +1.90.0 check --workspace --all-targets --all-features` 通過
- **ci.yml clippy を `-D warnings -D clippy::pedantic -D clippy::nursery` + `--workspace --all-targets --all-features` に昇格** (旧: lib のみ `--features llm-bridge` で `-W`) 昇格に伴い example 2 本の raw string hash / doc backtick を修正
- ci.yml の manifest-only stub (`alice-physics` 空 lib + 参照されていない Codec / Streaming / Cache / Foundry) を real `ALICE-Physics` checkout に置換 (`--all-features` で `physics` feature が実 compile されるため)
- `alice-sdf` 要件 `2.0.0` → `3.0.0` (`ShaderLang` seal のみ breaking、LOL は trait を実装していない)

### Added (previous)
- `docs/LLM_LOL_ROADMAP.md` — 「モデルに LOL を話させる」Track A/B/C の status 表 + benchmark baseline + fail 型 4 + A3 の評価 leakage 注意 (repo 側の単一 status source)
- **`alice-lol-datagen` (新 sibling crate、Track C1)** — 合成 (caption, LOL) pair generator template family 8 種 (`primitive_placed` / `stacked` / `attachment` / `plate_holes` / `transformed` / `intent_program` / `random_tree` / `product_shortcut`) が同じ parameter から LOL text (学習 target、shortcut はそのまま) / `lol_canonical` (`parse → emit` 正規形) / 英日 caption / oracle 点を同時に生成、`Sample::verify` (parse 成功 / emit 冪等 / oracle 点の eval 一致 / `grammar-check` feature で LLM grammar 受理) を通ったものだけ出力 `datagen --n 20000 --seed 42 --families all --out data.jsonl` で **6,500 sample/s、rejected 0** (evidence `~/claude-config/evidence_lol_datagen_2026_09_14/`) xorshift64* で決定論、serde 非依存 `llm_bench` baseline の fail 型 4 (translate 欠落 / half 引数 / 合成省略 / Intent 構造) を family 設計で直接狙う 自己検証は実装中に template の bug 2 件 (arch の穴が高すぎる / `repeat_finite` のコピー数は `2·floor(c/2)+1` で常に奇数・原点あり) を検出した
- **`runtime_parser::parse_expr` / `emit::write_node` を `stacker::maybe_grow` で stack 伸長** — stdlib product (`sd_card_holder` 等) は `subtract` を 2,400 段 nest した正当な tree を生成し、`parse_expr` の大 frame × 再帰で 8 MB stack が尽きていた (debug build は red zone 4 MB / 伸長 32 MB) 依存 `stacker = "0.1"` (MIT OR Apache-2.0) 残る深さ依存は `Arc<SdfNode>` の再帰 Drop (alice-sdf 側、2 MB の test thread で overflow、iterative Drop は follow-up)
- **`emit` の可変長 op 平坦化** — `union` / `intersection` / `smooth_union` 等は parser `fold_left` の左結合 2 分木を `union(a, b, c)` に戻す (同 op・同数値引数の左 spine のみ、右側の nested や `k` の違う smooth op は保持) `subtract` は binary のまま (LLM_REFERENCE の「cutter は nest、union でまとめない」規約と整合)
- **`emit::to_lol(&SdfNode) -> Result<String, EmitError>` + `Program::to_lol()` (Track C0)** — `runtime_parser::parse_lol` の逆変換 128 variant を exhaustive match (wildcard なし) で逆写像、LOL 構文のない 7 variant (`IFS` / `SdfSkinning` / `LatticeDeform` / `ProjectiveTransform` / `HeightmapDisplacement` / `SineDisplacement` / `Polygon2D`) は `EmitError::Unsupported` 正規形: 可変長 op は parser の左畳み込み通り 2 分木 (`union(union(a, b), c)`)、数値は `fmt_f32` (整数 `1.0`、`|v| < 1e-6` → `0.0` で stdlib 展開の cos/sin ノイズを吸収)、`rotate` は Euler XYZ 度を 1e-3 度に丸め `(-180, 180]` + `±180 → 180` に正規化 (度↔rad↔Quat 往復の 1 ulp drift と二重表現を固定点化) 出力は LLM grammar (comment なし、whitespace 1 文字) をそのまま通る `tests/emit_roundtrip.rs`: `lol.gbnf` の name bucket から **全 237 SDF construct** に既定引数 snippet を自動生成し `parse → emit → parse` を 64 点 eval parity + emit 冪等性で検証 (parser ⊆ grammar ⊆ emit を CI で固定) + fixture (sword / mug / product shortcut 展開形)
- **`capsule_ab(ax, ay, az, bx, by, bz, r)`** — 任意 2 点 capsule (`SdfNode::Capsule` の一般形) stdlib の SKADIS hook 等が生成する非 Y 対称 capsule を text に書き戻せるように追加 (`capsule(r, h)` は Y 対称の短縮形のまま) grammar `name_7f` + parser + LLM_REFERENCE
- `llm_bench` の出力表示を `Debug` から `Program::to_lol` (正規形 LOL) に変更
- **`lol.gbnf` に product / mechanical / fastener 構文 103 個を追加** (`pen_cup` / `coaster` / `gridfinity_bin` / `gridfinity_bin_ex` (新 `prim_7f`) / `wall_hook` / 12 archetype 3f 群 64 個 / `vesa_mount` / `l_bracket` / `screw_hole` / `heat_set_hole` / `jst_ph_slot` 等) 事案: 2026-08-20 Sprint C で text-to-print の grammar copy と `runtime_parser` には足されたが canonical `lol.gbnf` には upstream されず、mechanical 38 個はどの grammar にも無かった (system prompt は案内しているのに grammar ON だと emit 不能) 新 golden test `grammar_covers_every_runtime_parser_construct` (parser の `"name" =>` arm ⊆ grammar 名、include_str! で source を走査) で再発を CI で検知、`accepts_product_and_mechanical_shortcuts`
- **`alice_lol::LOL_GBNF`** (feature 外の `pub const`、`include_str!("lol.gbnf")`) — 下流 (alice-bamboo → text-to-print 等) が grammar を copy して drift させる運用を廃止するための単一 source `bridge::lol_grammar` も同じ bytes を parse

### Fixed
- **alice-sdf 1.10.2 の `impl Drop for SdfNode` に test code を追従** (`7292552` / `2753894`) — `match node { SdfNode::Union { a, b } => .. }` / `let SdfNode::Polygon2D { .. } = expr else` の値 destructure 10 箇所が E0509 (Drop 型から field を move 不可) になっていたのを `match &node` + `&**a` に変更 (production code 変更なし)
- **fuzz.yml が `alice-stubs` の空 alice-sdf stub で lib を build しており 3 target とも導入以来 compile 不能だった** (`c6610e2`) — job-level `continue-on-error` で silent green になっていた ci.yml と同じ実 sibling checkout (ALICE-SDF / LLM / Kinematics) + inline stub に揃え、build step を blocking / run step のみ informational に分離
- **CI `Security & Hygiene` red (`536f339`)**: `alice-stubs` action の alice-llm stub が `features = default, grammar` しか宣言しておらず、`llm-bridge` が要求する `simd` / `parallel` で cargo-deny / semver-checks の resolve が失敗 stub の feature 宣言を実 crate に追従 (stub は実 crate の feature list を鏡写しにする、が原則)
- **`llm-bridge` feature の alice-llm 依存に `simd` + `parallel` を追加** 従来 `features = ["grammar"]` のみで forward が single-thread / SIMD なしになっており、bridge 経由 (text-to-print sidecar 等) は alice-llm 直接 example より数倍遅かった MiniCPM5-2B で 1 prompt (system prompt ~600 token) 58 → 24.5 s 残りは alice-llm の逐次 prefill (batch prefill 未実装、ALICE-LLM 側 issue)

### Changed
- **clippy pedantic + nursery を workspace `--all-targets` で 0 warning に** (`e51e3c7` 機械適用 1160 → 532、`cefd9ed` 手修正 532 → 0) — `runtime_parser` の f32→u32 cast 56 箇所を `lol_u32` 単一 audit 点に集約 (per-site allow 8 個撤去)、`law` の grid index を符号なし演算 (`checked_sub` / `then_some` / `abs_diff`) に、`laser_pattern` の cast を `cell_count` / `cell_center` helper に集約 + 固定間隔 hatch を整数 index 刻みに、`pattern_sdf` の u32→f32 100 箇所を `n_f32` に集約 数値は `mul_add` (fused、単一丸め) と hatch の `d0 + i·spacing` (累積加算廃止) のみ変化、CI matrix 3 set の test 全 green 残す allow は理由付きのみ (`similar_names` = 幾何寸法の慣習命名 / `while_float` = 可変 step / demo example の `too_many_lines` 等)
- **`lol.gbnf` の whitespace を token 間 1 文字 (`ws ::= [ \t\r\n]?`) に制限 + `//` line comment を除外** (LLM 経路 grammar を runtime lexer より厳格化、grammar ⊂ parser) 無限 `ws` も rambling channel で、comment 除去後に MiniCPM5-2B が `\t` × 41 を連発した (evidence `03_*.log`) ため 1 文字に制限、以後は `cylinder(50,100)` で正しく final → EOS (evidence `05_*.log`、mask max 14 ms/step) comment 状態は `noteol*` でほぼ全 char を受理するため alice-llm B-10 token-trie mask が vocab 全走査に退化 (MiniCPM5-2B 実測 0.5-1 s/step) し、model も comment に思考を書き続けて本体を出さない (`// mug body` `// torus handle` … 48 token で本体ゼロ) 除外後は同 prompt で mask avg 5 ms/step、forward 律速 (evidence `~/claude-config/evidence_b10_trie_mask/03_*.log`) `parse_lol` / `parse_program` は comment を従来通り受理 (人間の `.lol` file は無傷) golden test `accepts_line_comments` → `rejects_line_comments`、`accepts_whitespace_and_newlines` → `accepts_single_whitespace_rejects_runs`、`sword.lol` golden は whitespace collapse して照合、`skills/lol-sdf/references/lol.gbnf` 同期、`LLM_REFERENCE.md`: Syntax Rules に rule 7 (comment / indent 禁止) 追加、LOL code block 7 箇所の `//` comment を prose に移動、LOL block 16 箇所の indent 除去 (model に grammar が弾く形を教えない、数式 / Rust block は対象外)

### Added
- **`examples/llm_bench.rs` (A2-3、feature `llm-bridge`)** — LOL 生成品質 benchmark 20 prompt (T1 単体 primitive 5 / T2 合成 5 / T3 変換・修飾 5 / T4 Phase 3 Intent 5) を ChatML system prompt 付きで GGUF に投げ、**oracle** (T1-T3: ALICE-SDF `eval` の点内外判定、T4: `Program.intent` の合成種別 + verb 列 + entity id) で判定 grammar-only (`generate_program_from_prompt`) と think→grammar (`generate_program_thinking`) を同一 prompt で比較、tier 別 pass 率 + JSONL (`--out`、serde 非依存) A3 SFT の効果測定 baseline 用
  - **Baseline (MiniCPM5-2B Q4_K_M、Apple M3 CPU、prefix budget 800、evidence `~/claude-config/evidence_lol_bench_2026_09_14/`)**: grammar-only **7/20** (T1 3/5 / T2 0/5 / T3 2/5 / T4 2/5、46 s/prompt) vs think→grammar **9/20** (T1 5/5 / T2 1/5 / T3 1/5 / T4 2/5、138 s/prompt) parse は 40/40 (grammar が構文を保証) fail 24 件は全て model の実誤りで型は 4 つ: translate 欠落 (指定座標を無視して原点) / half 引数の誤解 (cylinder half_height に全高、box3d に全寸) / 合成の省略 (mug handle・table 脚・arch subtract・plate 穴を落とす) / Intent 混入・欠落 (幾何 prompt に `program(…, seq(rest(1000)))`、Intent prompt で `entities()` 省略、`par` を `seq`) think は単体 primitive を完全に直すが合成は直らない = 語彙・引数規約の学習不足で A3 SFT の対象
- **`bridge::generate_program_thinking`** (feature `llm-bridge`、alice-llm B-11 `generate_grammar_prefixed` の one-call wrapper) — think-first model が `<think>…</think>` を grammar の外で書いてから LOL を emit、`ThinkingOutput { program, prefix_text, prefix_marker_hit, text, prefix_ms, decode_ms, total_ms }` を返す MiniCPM5-2B 実測: grammar のみは `cylinder(50,100)` (handle 省略)、think ありは `union(cylinder(25,50), translate(25,0,50, rotate(0,90,0, torus(12,4))))` (mug 完全正解) `GrammarGenResult` / `GrammarPrefix` を re-export
- `tests/lol_gbnf_test.rs` — `LLM_REFERENCE.md` の coaster example を verbatim で grammar + parser 両方に通す golden test (reference が示す形 = grammar が受理する形、の drift 検知)
- **Phase 3 Intent text 構文 (A0)** — `runtime_parser::parse_program(&str) -> Program` を追加 `program(<sdf>)` / `program(<sdf>, entities(...))` / `program(<sdf>, entities(...), <intent>)` の 3 形 + 裸 SDF 式 (後方互換 = `Program::sdf_only`) を受理 Intent verb 18 種 (`grasp` / `release` / `catch` / `walk` / `gaze` / `point` / `throw` / `push` / `pull` / `turn` / `align` / `follow` / `avoid` / `rest` / `latent` / `seq` / `par` / `music`) を `IntentNode` field 順の引数で parse、entity id 範囲・整数 slot・latent dim ≥ 4・music 8 byte を parse 時検証 `IntentNode::Rotate` の text verb は SDF transform `rotate` と衝突するため `turn`
- `IntentNode::to_lol()` / `HandSide::as_lol()` — Intent tree → LOL text (parse_program の逆変換、`semantic_hint` は落とす)
- `lol.gbnf` — `root ::= ws (program | expr) ws` に拡張、`program` / `entities` / `intent` rule 群を追加 (`skills/lol-sdf/references/lol.gbnf` も同期)
- `bridge::generate_program_from_prompt` (feature `llm-bridge`) — grammar-constrained decoding → `parse_program` の one-call wrapper
- tests: `tests/program_parser_tests.rs` (11、全 verb parse + round-trip + error) / `tests/lol_gbnf_test.rs` に program / intent golden 3 件追加

### Changed
- `alice-physics` optional dependency requirement `0.14.0-preview.4` → `1.0` (alice-physics 1.0.0 stable、2026-09-14) `physics` feature は 1.0 API で clippy clean CI stub も 1.0.0 に追従
- `rust-toolchain.toml` pin `1.92.0` → `1.98.1` (6 release 遅れで `cargo-semver-checks@latest` の MSRV に追い抜かれていた)、`cargo-semver-checks` を `0.50.0` に明示 pin

## [0.3.0] - 2026-09-13

### Added

- **`IntentNode::Music { packet: [u8; 8] }`** — L1 Musical Intent variant carrying an opaque 8-byte payload. Interpretation is defined by `alice_synth::intent::MusicIntent` (byte 0: genre / 1: mood / 2: length_bars / 3: tempo_bpm_offset / 4: key / 5: mode / 6-7: variation_seed LE). Consumers deserialize via `MusicIntent::from_bytes(packet)` and render via `synthesize()` or a `PlanHead` implementation.
- `music_intent(packet: [u8; 8]) -> IntentNode` — const constructor matching the style of other verb constructors (grasp / walk / etc.).
- 5 unit tests exercising construction, roundtrip, composition in `Sequence` and `Parallel`, and equality.

### Changed

- `alice-sdf` dep bumped `1.7.4` → `1.9.0` (crates.io publish landed with NPR / SIMD batch / GPU bytecode Phase 12-D/13/14).
- `physics` feature: removed the `alice-sdf/physics` feature reference. alice-sdf 1.9.0 dropped its physics / codec / asp / font / cache bridge features for the crates.io publish; the corresponding cfg-gated bridge modules stay retained on the alice-sdf side for restoration in a later release. `dep:alice-physics` is kept so the feature remains structurally intact for path-dep users on the sibling repo workflow.

### Notes

- Cross-crate contract with `alice-synth`: the 8-byte packet is the only interface (no crate dep in either direction). See `alice-synth` commit `c41de9b`'s companion `IntentNode::Music` landing.

## [0.2.0] - 2026-07-23

### Removed (temporary, restoration scheduled alongside alice-sdf 1.8.0)

- **`physics` feature** — previously enabled `alice-sdf/physics` and
  re-exported `sdf_to_physics_field` / `CompiledSdfField` /
  `attach_physics` / `simulate_sdf` / `SimulatedSdf`. Removed because
  `alice-sdf` 1.7.4 dropped its `physics` bridge for the crates.io
  publish (transitive dep chain not yet on crates.io). The
  corresponding `pub use alice_sdf::physics_bridge::*;` lines in
  `lib.rs` are `#[cfg(feature = "physics")]`-gated so they simply
  don't activate on crates.io 0.2.0. `path` users on the sibling repo
  workflow are unaffected. Restore alongside alice-sdf 1.8.0.

### Fixed

- Keyword `signed-distance-function` (24 chars) → `distance-field`
  (14 chars) to satisfy the crates.io 20-char limit.

### Added

- **LLM Guided Generation bridge (Phase X.8, B-5 → B-9-A)** behind the
  new `llm-bridge` feature. Off by default so pure-geometry users
  don't pay for the `alice-llm` dependency.
  - `lol.gbnf` at the workspace root: hand-written GBNF grammar
    covering the 124 constructs `runtime_parser::parse_lol` accepts.
    Bucketed by argument shape (`prim_1f` / `prim_2f` / ... /
    `op_variadic` / `op_k_children` / `mod_1f_child` / ...) rather
    than one rule per construct, so ~250 lines cover the full DSL.
  - `alice_lol::bridge` module (feature-gated):
    - `lol_grammar() -> &'static Grammar` — `OnceLock`-cached parse
      of the bundled `lol.gbnf` (compiled in via `include_str!`, no
      filesystem I/O at runtime).
    - `LOL_FSM_MAX_DEPTH: usize = 4096` — recommended
      `Fsm::with_max_depth` for the LOL grammar; the default 256 is
      too tight for deeply nested `translate + smooth_union` chains.
    - `BridgeError { GrammarGen(GrammarGenError), Parse(ParseError) }`
      with `Display`, `std::error::Error`, and both `From` impls.
    - `generate_sdf_from_prompt(&mut Llama3Model<'_>, &GgufTokenizer,
      prompt: &str, max_new_tokens: usize) -> Result<SdfNode,
      BridgeError>` — one-call wrapper: runs
      `Llama3Model::generate_grammar` with `temperature = 1.0` and
      `top_k = 1` (deterministic greedy), trims the generated text,
      and hands it to `runtime_parser::parse_lol`.
    - Re-exports from `alice-llm`: `Grammar / Fsm / FsmError /
      CharSet / parse_gbnf / GrammarTokenizer /
      mask_logits_by_grammar / advance_fsm_on_emit / Llama3Model /
      GgufTokenizer / GenerateResult / GrammarGenError`. Downstream
      writes `use alice_lol::bridge::*` and avoids a direct
      `alice-llm` dep.
  - `examples/prompt_to_sword.rs` — end-to-end demo (GGUF + prompt →
    `SdfNode` + optional STL via `print_export::node_to_stl`).
    `cargo run --example prompt_to_sword --features llm-bridge --
    --model <path> --prompt "..." --stl out.stl`.
- **CI**: matrix now covers both feature configurations — macos with
  default features (backward-compat baseline) and ubuntu with
  `--features llm-bridge` (grammar features gated by CI). `cargo
  build --examples` runs on both rows; `required-features` on the
  demo auto-skips it on the macos default row. `ALICE-LLM` is checked
  out as a sibling directory so the optional path dep resolves.
- **13 golden parse tests** (`tests/lol_gbnf_test.rs`) validate the
  grammar file itself, the shipped `examples/sword.lol` snippet, and
  reject known-bad syntax (typos, unbalanced parens,
  variadic-with-single-child, empty input, `{expr}` syntax reserved
  for the proc_macro path).

### Notes

- Real-model smoke run against Qwen 3.5-4B Q4_K_M (Mac Metal, CPU
  hybrid, ~1 tok/s) validated the API end-to-end: prompt `"generate
  lol: sphere(1.5)"` produced `SdfNode::Sphere { radius: 1.5 }` in
  ~477 s (evidence at `~/claude-config/evidence_b9a_prompt_to_sword/`).
  A shorter prompt without a primed example triggered
  `BridgeError::Parse` (grammar mask allowed the model to reach
  `with_material` before `max_new_tokens` capped it mid-parse) —
  exactly the two-stage safety net (`FSM mask` + `runtime_parser`)
  the design intended.
- Fine-tuned LOL emission is future work. The grammar mask
  guarantees the output is syntactically valid LOL; semantic quality
  (does the SDF match the prompt?) tracks the underlying model.
- Jetson Vulkan smoke run (Phase X.8 B-9-B) and a version bump to
  0.2.0 (additive `llm-bridge` feature is SemVer-minor) are
  follow-up work.

## [0.1.0]

Initial ALICE-LOL release. `proc_macro` DSL compiling to `SdfNode` +
GLSL / WGSL / HLSL transpilation. See `SPEC.md` for the full v0.5
DSL surface: 124 constructs across primitives, operations,
transforms, modifiers, 3D-print structural intent, time, and law.
Ships with:

- `alice-lol-macro` — proc_macro parser + `SdfNode` codegen.
- `alice-lol` — re-exports, `runtime_parser::parse_lol`,
  `print_export` (STL / 3MF / OBJ / FBX), `law` (NonOverlap /
  Containment / MinThickness with `LawSet` + `detect_contradictions`
  + residual reports), `laser_pattern` generator (hatch / halftone /
  guilloche / turing etc.), `pruned_compile` (interval-based space
  culling), `roblox_export` (behind the `roblox` feature).
