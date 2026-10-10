# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### 試験が repo の履歴を読まないことの検査 (`scripts/history_free_tests.py`)

- `scripts/test_*.py`、`conformance/*.py`、`tests/` 下の `*.rs` を走査し、`git show` / `git log` / `git rev-list` などの履歴を読む呼び出しと `<rev>~1:<path>` の形の参照を見つけたら fail、走査 0 件でも fail CI の clone は浅く (commit 1 つ)、履歴を読む試験は手元で通り CI で落ちる 試験 `scripts/test_history_free_tests.py` CI の docs job と `scripts/preflight.sh` で走らせる
- `scripts/test_law_ambiguity_lint.py` の分割前の Kepler の law は `git show` で読んでいたので、その内容を `scripts/testdata/kepler_energy_bounded_pre_split.law` に置いて file から読むようにした

#### law file の曖昧さ検査 (`scripts/law_ambiguity_lint.py`)

- `x-invariant` を持つ law は、判定が特定の積分法に限定されることを `claim` に明記しなければならない
- 同じ law が `input ... range` も持つ場合、有効範囲をどう測定したかを comment に明記しなければならない (script path を含む)
- 全 law の文面 (`claim` / `verdict` / comment) に実装言語固有の語 (`f64` / `Vec` / `::` パス / `.method()` 表記 / 数値 literal 表記 等) を書いてはならない、数値の型は言語中立に書く (`source` 行は引用として対象外)
- 比較した law file が 0 件なら fail CI と preflight は検査器自身の試験と `laws/spike` 全体の検査を走らせる

#### law file の適合性 corpus (`laws/spike/`, `conformance/`)

- `laws/spike/*.law`: 言語に依らない法則の記述 9 件 (`alice-lol/tests/spike_law_files_parse.rs` が既存の parser で読む)
- Kepler の law は積分法ごとに 2 件 (`kdk` / `dkd`) に分け、有効範囲を掃引で決めた (kdk e <= 0.77、dkd e <= 0.765) 根拠と再現手順は `conformance/kepler_range.md`
- `scripts/law_corners.py`: law file の range / x-integer / x-range / x-piece の行から有効範囲の corner を列挙する `scripts/law_corpus_cover.py`: corpus が corner を全て含まないと exit 1 (CI と preflight は両者の試験と corner 列挙を走らせる)
- `conformance/`: law file から corpus を生成する script、採点 runner、参照実装と欠陥のある変種、生成 → 採点 → 削除を 1 回で行う `score.sh`、must-red の対照 `controls.sh` corpus の生成には mpmath が要る (`conformance/requirements.txt`)
- `conformance/TASK.md`: 実装する側に渡す契約と law file の読み方 (言語に依らない) 監査の証拠は有限で 0 より大きい数、成立範囲は集合として比べる、数でない値は測られていないものとする、`x-list` に配列でない値は拒否、未知の law と欠けた入力は exit status 2 で標準出力に何も書かない、を明記 `PROTOCOL.md` はこれを参照する
- corpus と参照実装を `TASK.md` の規則に揃えた: 証拠は有限で 0 より大きい数 (負の数・text・`true`・`null` の件数は証拠にならない)、成立範囲は集合 (重複を含む同じ集合は `supports`)、配列でない成立範囲と `null` は `out_of_range`、`x-list` に数や object を渡すと拒否 それぞれの vector を corpus に足し、旧い証拠の読み方 (`REF_BUG=9`) と重複を残す範囲の比較 (`REF_BUG=10`) を `controls.sh` の must-red に加えた 参照実装は未知の law と欠けた入力で exit status 2 を返す
- 有限でない数と形の崩れた build の読み方を `TASK.md` と law file に明記した JSON に NaN と無限大の literal は無いが、double に収まらない literal (`1e400`) は無限大に読まれる 有限でない数は数でないものとし、量の法則の入力なら拒否、監査の量なら測られていない (`expect` は `undecided`) `identifier_feature_independent` は `builds` の要素数を形によらず数え (`builds` が配列でなければ測られていない)、要素が object でないか `features` が text の配列でない時は `feature_sets` を測られていないとし (`no_evidence`、subject `feature_sets`)、`id` が text でない build は識別子を持たないとする それぞれの vector を corpus に足し (326 件)、runner は無限大を `1e400` として送る `REF_BUG=11` (有限でない数を数と読む) と `REF_BUG=12` (形の崩れた build を空の feature set と読む) を must-red に加えた
- 監査の law への request に `inputs` key が無い時は `"inputs": {}` と同じ (何も測られていない) と `TASK.md` に明記した 参照実装はこれまで exit status 2 を返していた vector を足し (327 件)、旧い読み方 (`REF_BUG=13`) を must-red に加えた 量の法則では入力が欠けるので従来どおり exit status 2
- `"inputs": null` も `inputs` key が無いのと同じ (`{}`) とし、`inputs` が object でない (配列・text・数・真偽値) request と JSON object でない request は request の誤り (exit status 2、標準出力は空) と `TASK.md` に明記した 参照実装はこれまで traceback を出して exit status 1 で終わっていた 全 law に request の契約の vector を足し (361 件、request の誤りを期待する vector は runner が exit status 2 と空の標準出力を確かめる)、旧い読み方 (`REF_BUG=14`) を must-red に加えた
- `x-input` の行に型を書くようにした (`number` / `integer` / `text` / `list of <型>` / `record(<名前>: <型>, …)`、欠けてよい field は `optional`) 型に合わない値は**入力全体**が合わないとし (list の要素 1 つ・record の field 1 つで足りる)、監査では測られていない、量の法則では拒否とする (`TASK.md` に汎用の規則として明記) `gate_compares_nonzero` は `list of text`、`identifier_feature_independent` は `list of record(features: list of text, id: optional text)` 型の構文と照合は `scripts/law_schema.py` (試験 `scripts/test_law_schema.py`、`x-input` の行が 0 件なら fail)
- **判定の変更:** `identifier_feature_independent` で形の崩れた build (features の欠落・text でない features・object でない build・text でない feature・数や `null` の id) の判定は、これまでの `no_evidence` (subject `feature_sets`、数の id は `undecided`) から `no_evidence` (subject `builds`) に変わる corpus の 5 件が変わり、`null` の id の 1 件を足した (362 件) 理由: 型を持つ前は field ごとに読む規則を law ごとの文で書いていたが、型が記述する単位は入力全体なので、どの law にも同じ 1 つの規則が掛かる 旧い field ごとの読み方を `REF_BUG=15` として must-red に加えた
- `conformance/probes.json` と `conformance/check_probes.py`: 実装の間で答えが割れた入力 28 件と、`TASK.md` から手で書いた答え 実装を 1 つ渡すと全件を比べ、1 件でも違えば exit status 1、比べた件数が 0 なら 2 CI の docs job と `scripts/preflight.sh` が参照実装に対して走らせる (旧い読み方の `REF_BUG` 9 / 10 / 11 / 13 / 14 / 15 はいずれかの probe で違いが出る)

#### Law の識別子 (`law_id`) — 同じ主張に同じ名前

- `law_id::LawIdHasher` / `AuditLaw::law_id`: 監査 Law に内容由来の 32 byte 識別子を付ける (`alice-zip` の `LAW_ID_DOMAIN` を共有し種類の tag を足す形)
- 識別子は domain / 種類 / 算術の世代 / 主張から決まる **Behavior change:** 同じ text でも評価する算術が違えば別の識別子になる
- 値ごとに型と長さを書くので `["ab","c"]` と `["a","bc"]`、`u32 5` と `u64 5`、空の項と項の不在がそれぞれ別の識別子になる
- `law_id::LOL_SEMANTICS_ID` は `LOL_SEMANTICS_PINS` の fold で、pin は振る舞いから計算する (`audit_verdict_order_fingerprint`) 判定順序を変えると pin が合わなくなる
- 幾何の制約と研究の法則の識別子は未実装 (前者は正規形が失敗しうるため戻り値の形が変わり、後者は式に canonical な text 形が無い)
- `law_id.rs` を変異試験の対象に追加 (44 mutant / 30 caught / 見逃し 0、等価な 1 件は理由つきで除外)

#### 判定経路に platform 依存の超越関数が入るのを止める gate (`scripts/det_math_guard.py`)

Law は「同じ実測なら同じ判定」でなければならないが、`sin` / `cos` / `exp` 等は IEEE 754 が
値を規定しておらず platform の libm ごとに最後の 1 bit が違いうる 判定を出す file
(`law.rs` / `audit_law.rs` / `research_law.rs`) に限って直呼びを検査する
正しい丸めが要求される演算 (`sqrt` / `mul_add` / `powi`) は platform に依らないので落とさない
既存分は `scripts/det-math-baseline.txt` に理由付きで置くラチェットで、解消した行が残っていても
検査は red になる 対象 file が読めない / 呼び出しを 1 件も見ていない場合も red

#### 監査 Law — 検査を「何が成立すべきか」として書く (`audit_law` + `parse_law`)

`law` (幾何の制約) と `research_law` (データに対する式) に続く 3 つ目の Law 一致を検査する対象が
**測定そのもの**で、検査器が何件比べたか / 既知の違反がどこまでかを主張する 判定は 6 値
(`Supports` / `NoEvidence` / `Breaks` / `OutOfRange` / `ParameterUpdate` / `Undecided`)

```text
audit lock-single-version
evidence packages
expect unbaselined_duplicates == 0
range alice-det-math 0.3.2 0.4.0
```

**Law は測り方を書かない** 数値を渡すのは検査を走らせる側 (`Measurements`) なので、同じ Law を
Rust からでも別の言語からでも食わせられる 真偽の 2 値では「測れていない」と「違反した」が同じ失敗に
なり「判定不能」を表現できないため、`research_law::ResearchVerdict` と同じ考え方で証拠に対する判定を分けた
証拠は期待値より先に判定するので、空の実測で `0 == 0` が偶然通ることはない

`LAW_SYNTAX` を構文表に追加し、`parse_law` の dispatch と README の `syntax-law` 表との 3 者一致を
既存の仕組みで強制する (語彙を足して parser か README を足さなければ red) 文法 (`lol.gbnf`) にも
`law` を root の選択肢として追加したので、制約デコードで Law を書ける Law の metric 名と成立範囲の
key は測る側が決めるので列挙できず、ここだけ開いた識別子 (`ident`) を使う

`tests/audit_law_parity.rs` が `scripts/lock_single_version.py` の試験入力 13 件で判定の一致を確かめる
(残る 3 件は lock の読み方なので測る側の責務) `scripts/law_tests.sh` に登録済
`examples/audit_law_demo.rs` が公開入口 (Law の構文解析 → 実測 → 6 判定) を通して実行する

#### 文法 file の 2 つの写しが同一であることの検査 (`tests/gbnf_copy_is_in_sync.rs`)

`alice-lol/lol.gbnf` (正典) と `skills/lol-sdf/references/lol.gbnf` (配布に同梱する写し) を byte で比べる
片方だけ編集しても気付けない状態だった — `lol_gbnf_test` は正典しか読まず、しかも `llm-bridge`
feature 配下なので feature 無しの run では 0 test で通る 本 test は feature を要求しない

#### 同じ `alice-*` crate が 2 版以上 lock に入っていないことを検査する gate (`scripts/lock_single_version.py`)

semver 非互換な要求 (`^0.3` と `^0.4` 等) が混ざると cargo はどちらも正当な解決として 1 つの build に両方を link する
法則を評価する crate と法則を保存する crate が別の版の算術 crate を引くと、同じ法則が 2 通りに評価されうるが、
片方だけを相手にした bit 一致試験は green のまま通るので差が出口に現れない
`Cargo.lock` の `alice-` で始まる package が 2 版以上あれば fail し、既知の分は `scripts/lock-duplicates-baseline.txt` に
`<crate 名> <版> <版>` で載せる 解消した行が残っていても fail する (ラチェット) 読めた package が 0 件なら fail する
third-party の重複 (`syn` / `thiserror` 等) は上流の都合で日常的に起きるので対象にしない
`scripts/test_lock_single_version.py` (16 本) が各検査の歯を確かめる ci.yml の `wiring-guard` job (3 OS) と preflight で実行
現在の baseline は `alice-det-math` の 1 行で、`alice-lol` 自身と `alice-zip` / `alice-physics` が `^0.4` を、`alice-sdf` が `^0.3.1` を要求するため

#### README とコードの一致を検査する gate (`scripts/readme_sync.py`)

README.md / README_JP.md の構文表 (グループごと) が `alice-lol/src/syntax_table.rs` の名前の一覧と過不足・重複なく一致すること、
`lol!` マクロが受け付ける名前が `stdlib` グループと `RUNTIME_ONLY` を除いた構文表と一致すること、
法則表が `law::Constraint` の variant と一致し各行の根拠が `Constraint::evidence_class` の返す値と一致すること、
`research_law` の判定表が `ResearchVerdict` の variant と一致すること、feature 表が `[features]` と、MSRV 行が `rust-version` と一致すること、
最初の ```` ```rust ```` block が crate doc の doctest と同一であること、相対リンクの実在、英日の節数を検査する どの検査も比較 0 件なら fail
`scripts/test_readme_sync.py` (27 本) が各検査の歯を確かめる ci.yml の `docs` job (3 OS) と preflight で実行

#### parser の名前の一覧と、それを parser と突き合わせる test (`alice-lol/src/syntax_table.rs`)

runtime parser は名前の `match` で dispatch するので、名前の一覧を test 専用 (`#[cfg(test)]`) の表として置いた
test が 2 方向で一致を確かめる: `parse_expr_inner` / `parse_intent` の dispatch の腕を source から読んだ集合と表が一致すること、
表の全名前について不完全な呼び出しが「unknown」以外の error になること (未知の名前の対照つき) 公開 API は変えていない

#### 法則の oracle を target ごとに走らせる step (`scripts/law_tests.sh`)

`law_tests` / `law_corpus_oracle` / `analytic_law` / `test_field_law_oracle` / `analytic_research_law` / `research_law_bit_exact` /
`interior_lipschitz_bound_probe` / `print_tests` / `analytic_robot_law` / `alice-lol` の `law` unit test と、
feature `physics` の `analytic_thermal` を 1 target ずつ `scripts/cargo_test_nonzero.sh` に通し、0 件実行なら fail にする
`law::` / `research_law` を使う test file が一覧に無い場合も fail
preflight では target ごとに実行する CI では同じ target を走らせ直さず、test job の `cargo test` の log を
`scripts/law_tests_from_log.py` で読み、各 target が log にあり 1 件以上実行されたかを確かめる (既定 entry と `physics` entry)

#### 公開文書と tracked file の語彙検査 (`scripts/docs_lint.py`)

README.md / README_JP.md / CHANGELOG.md と全 tracked file について、開発の進め方を表す語・内部の記録名・機器名・
private address を検査し、CHANGELOG の版見出しの順序と `[Unreleased]` の分類 (各 1 回、絵文字なし) を検査する
固有名は SHA-256 で照合し、検査器に平文で持たない `skills/` 以下は言語モデル向けツールの package 形式なので、
その形式名の語はその中でだけ許す (path の `skills/` はどこでも語として数えない) ci.yml の `docs` job (3 OS) と preflight で実行

#### `alice-lol`: `research_law` (単位付きの多変数の研究 Law)

`research_law::ResearchLaw` を追加 式を文字列で書き (`+ - * / ^`、単項マイナス、括弧、`exp` / `ln` / `sqrt` / `sin` / `cos`)、
入力と parameter に単位 (`Pa` / `kPa` / `J/(mol*K)` / `m^3` / `L` / `m/s^2` 等、SI への倍率と 7 基本次元) を持たせ、構築時に次元を検査する
(`+` / `-` は同次元、`exp` 等の引数は無次元、指数は定数で結果の次元が整数、式の次元が出力と一致)
`evaluate` は宣言した単位で入力を受け SI で評価して出力の単位で返し、成立範囲の外・非有限の入力は `OutOfRange`、0 除算等の非有限な途中値は `NonFinite` を返す (外挿しない、NaN を `Ok` で返さない)
`compare` は `Bridge` (入力名の対応) で単位を換算して 2 つの式を同条件で評価し、差と相対差を返す (次元の違う対応は error)
`ingest` は新しい観測を `alice_zip::law::SignalLaw::ingest` と同じ規則順で 支持 / parameter 更新 / residual 増 / 破綻 / 範囲外 / 証拠なし に判定する
parameter 更新は全 parameter の最小二乗 (中心差分 Jacobian、部分 pivot 付き消去、step 半減、反復上限と収束閾値は公開定数) 残差統計は保持した観測から測る
`ValidRange` / `ResidualStats` / `Provenance` / `IngestPolicy` は `alice-zip` (0.5、no_std の `law` module) と共有し、`alice-zip` を依存に追加 (default feature なし)
example `research_law_demo` と解析解 oracle `tests/analytic_research_law.rs` (理想気体 / 自由落下 / kPa・L と SI の比較 / 判定 / 退化入力) を追加
SDF の幾何制約を扱う既存の `law` module は変更していない

#### `alice-world-auditor`: Phase 5 3 値判定 (`audit`、2026-10-04)

`audit(body, config, goal, params) -> Audit` を追加 整数格子の探索で得た行動列を、渡された `PhysicsConfig` から作った新しい world で再生し、
位置の AABB 包含 (3 軸) と線速度の `Fix128` 厳密 0 が成立し、かつ engine の overflow flag が立っていない時だけ `Proven` (証跡 = 行動列) を返す
それ以外は理由付きの `Undecided` (`BudgetExhausted` / `ReplayMismatch` / `Overflow` / `InvalidInput` / `OutOfSearchRange` / `NoActuator`)
`Violated` は空の target と、入力なし (`a_max_axis == 0`)・静止・重力 0・target 外の 2 件のみ (1-D で `a_max_axis > 0` なら必ず届くため、
法則から到達不能を一般に示すことは非目標) `Audit::verdict()` で `Verdict` に落とせる 探索の深さ上限 `MAX_AUDIT_FRAMES` (4096) を公開

`Proven` になる十分条件は `a_max_axis * dt` と `dt` が dyadic (`Fix128` で厳密)、damping 1、goal 軸方向の重力 0 `a = 5, dt = 1/60` の
537 frame の計画は engine 上で `x = 100.1000082` (target 上端 100.1 の外) / 速度 約 `-1.7e-15` で終わるので `Undecided` になる
README (英日) と crate doc に「k 粒度の macro-action 空間で、整数格子上で厳密に最適」の主張と粒度を明記

#### `alice-world-auditor`: Phase 4 探索実装 (IDA\* over 整数格子、2026-10-03)

`lower_bound_frames` / `plan` の `todo!()` 本体を実装 1-D bang-bang 制御を厳密整数格子 (`S` = 速度 index, `D` = 位置 index) に変換し、frame 粒度
(`k = 1`) の IDA\* で探索する `AtRest` の判定は lattice 内部の整数 `S == 0` で行い、実 `alice-physics` world の XPBD 再導出速度を `== Fix128::ZERO`
で比較しない (engine は位置差分 × 逆数で毎 substep 速度を再導出するため、`Fix128` でも厳密 0 にならないことを実測で確認済み)

Phase 3 oracle 3 本 (`tests/phase3_search_oracles.rs`) の `#[ignore]` を全て解除、green 確認済み 実装前に発覚した oracle 側の矛盾 3 件
(`lower_bound_frames` の signature に `Goal` が無く (a)(b) が両立不能 / (b) の自己検算 assert が誤り (49→110) / (c) の期待値が既定 `PhysicsConfig`
の damping で到達不能) も同時に解消

#### 配線ガード (`scripts/wiring_guard.py`) を導入 (2026-10-02)

実装したが production から呼ばれていない `pub` / `pub(crate)` item と、理由の無い
`#[allow(dead_code)]` の新規追加を CI で止める検査器を ALICE-Physics から移植した
Cargo workspace の全 member (`alice-lol` / `-ui` / `-humanoid` / `-robot` / `-datagen` / `-macro`) が対象

- `scripts/test_wiring_guard.py`: 検査器自身の oracle 79 本
- `scripts/wiring-baseline.txt`: 既存の違反 (unwired 171 件 / dead_code 3 件) を記録するラチェット 既存分は解消しておらず、新規の違反だけが fail する
- CI: `wiring-guard` job (ubuntu / macOS / Windows) と `scripts/preflight.sh` の step を追加

#### 3 層に閉形式 oracle 90 本 (2026-10-01)

- `alice-lol/tests/analytic_print_export.rs` 21 本 — 発散定理による体積と球 /
  直方体 / トーラスの閉形式、解像度収束、水密性 (境界 edge / 非多様体 edge /
  Euler 標数)、STL binary layout (`84 + 50n`)、3MF の mm 宣言、`scale_mm` の
  3 乗則、DC 経路の同一 invariant。変異捕捉 0/10 → 9/10
- `alice-lol/tests/analytic_hardsurface.rs` 40 本 — ISO 4762 の `k = d` (7 サイズ)、
  ISO 10642 の `dk = 2·d` (M2.5 のみ規格範囲外として逸脱を明示 pin)、面一の
  代数恒等式、90° テーパーの円錐からの復元、すきま嵌めの径差、押出スタジアムの
  厳密距離、片持ち梁のスケーリング指数。変異捕捉 2/12 → 12/12
- `alice-lol-robot/tests/analytic_robot_law.rs` 29 本 — 閉 AABB membership の
  全件突合 (7³ 点 × 4 verb)、閾値ちょうど pass / 1 ulp 上 fail、角度の偶関数性、
  単調性、`LatentIntent` のピタゴラス数と斉次性、合成の連結則

#### 法則検証器に第 2 の証明経路 (Lipschitz 包囲) を入れた (2026-09-30)

距離依存の法則は区間演算だけで「箱に表面なし」を証明していたが、区間は健全でも
依存性問題で膨らむ。真の核 5 件 (`heart` / `cut_sphere` / `link` / `capped_torus` /
`death_star`) の未決定領域を総当たりで測ると、区間 `[-5.5, +6.53]` に対し真の範囲は
`[+1.58, +1.62]` で、セル幅の約 300 倍に広がっていた。解像度を上げても幅は縮まらない。

そこで距離場が `L`-Lipschitz であることを使い、箱の中心 `c` と半対角 `ρ` から
`[f(c) − L·ρ, f(c) + L·ρ]` を第 2 の包囲として作り、区間との共通部分で締めるように
した。上記 5 件では `|f(c)| = 1.60` / `L = 1.0` / `L·ρ = 0.027` なので 59 倍の余裕で
符号が確定する。

判定が区間に依存する 3 箇所 (`probe_pair` / `gap_exceeds` / `probe_ball`) すべてに
配線した。`probe_pair` だけに入れた時点では `Contact` の判定が 1 件も動かなかった
(下界は `gap_exceeds` の別ループが出しているため)。

`eval_lipschitz` の契約は外部 (`f ≥ 0`) 限定なので、箱の中心が実体の内部にある時は
包囲を主張しない。この gate を省いた実装は `tests/analytic_law.rs` の
`undecidable_node_is_reported_not_passed` が捕らえた。

2 つの包囲の共通部分が空になった箱は、どちらも信用できないので未決定として扱う
(証明も反証もしない)。

効果 (corpus 244 scene、決着率は resolution 4 / 8):

| constraint | 何を問うか | 変更後 | 変更前 |
|---|---|---|---|
| `NonOverlap` | 箱が外部にあるか | 95.5% / 95.9% | - / 91.4% |
| `Contact` | gap が範囲内か | 88.1% / 88.9% | 85.7% / 86.1% |
| `MinThickness` | 内部の肉厚 | 87.7% / 92.2% | 87.7% / 92.2% |
| `Reachable` | 連結性 | 17.4% / 25.4% | 17.4% / 25.4% |

probe 3 半径すべてで未決定に残る構造 (真の核) は `NonOverlap` が 9 件から 0 件、
`Contact` が 11 件から 1 件になった。`MinThickness` と `Reachable` は内部と連結性を
問うため、外部限定の契約では減らない。

#### 区間包囲の健全性を corpus 全件で検査する oracle (2026-09-30)

`tests/law_corpus_oracle.rs::interval_enclosure_contains_the_brute_force_truth` を
追加した。corpus 全 construct について、箱ごとの区間包囲が総当たりの点評価を含むか
を突き合わせる。`NonOverlap` の決着は区間の下界を見るので、下界が真の最小値より
高いと誤った合格になる。

`alice-sdf` 側で `rounded_cone` の 1 件が外れる (外れ量 0.35)。外接球半径に
`(2h).hypot(max(r1,r2))` を渡しているが、丸め錐のキャップは球なので真の外接半径は
`h + max(r1,r2)` で、`max(r1,r2) > 1.5·half_height` のとき外接球が実体より小さくなる。
判定器は包囲の矛盾を検出した箱を未決定にするため、この差が合格に化けることはない。

#### `laser_pattern` と `alice-lol-humanoid` の閉形式 oracle (2026-09-30)

どちらも数式が閉じているのに、検証が「出力が空でない」「要素が 1 個ある」と
いった構造の確認に留まっていた。`laser_pattern` に変異を 10 種入れたところ
**9 種が既存 30 test を全 green で通過**する (捕まるのは halftone の符号反転
1 件だけ) 状態だったので、教科書の式と直接突き合わせる oracle を置いた。

- `alice-lol/tests/analytic_laser_pattern.rs` (27 本) — ロドネア曲線の花弁数
  (k 奇数 → k / 偶数 → 2k)、Vogel 螺旋の黄金角と面積一様性、ハイポトロコイドの
  極半径・周期・尖点数、hatch の法線方向間隔が角度に依らず `spacing` である
  こと、`δ=π/2` の lissajous が厳密な円であること、halftone の階調特性、
  誤差拡散の階調保存 (格子を細かくすると誤差が収束する) と Bayer 閾値行列の配置
- `alice-lol-humanoid/tests/analytic_humanoid.rs` (13 本) — 骨格比率の定義式、
  相似性、左右鏡像、BVH の単軸四元数・チャネル合成順・度単位・位置チャネルの
  名前対応

変異の捕捉率は laser_pattern が 1/10 → 14/14、humanoid が 11/11。

`MorphologyParams::height` の doc は「全身高」だが、頭頂は `height/2`、足首は
`−height·leg_ratio` なので実際の全高は `height·(0.5+leg_ratio)` になる。
宣言頭身と見た目の頭身が一致するのは `leg_ratio = 0.5` の時だけで、`chibi`
preset は宣言 3.0 に対し実測 2.7。現状を固定する test を置いてある。

#### `LawReport::hard_verdict()` と `HardVerdict` (2026-09-30)

`Priority::Hard` 制約の判定を 3 値 (`Proven` / `Violated` / `Undecided`) で返す
accessor を追加した。`HardVerdict::is_proven()` は `Proven` だけを `true` にする。

追加した理由は、**「Hard 制約が証明付きで満たされた」を表す述語が無かった**こと。
既存の 2 つはどちらも gate に使えない:

| 述語 | Hard が判定不能の時 | 問題 |
|---|---|---|
| `!has_hard_violations()` | `true` | 判定できなかった標本が合格側へ倒れる |
| `all_passed()` | `false` | `Priority::Soft` の判定不能でも `false` になり、Hard 制約だけの主張には過剰 |

中間が無いため呼び出し側は前者を選び、解像度不足で決まらなかった標本が
合格として通っていた。`has_hard_violations()` は doc 契約どおり
「証明か反例のある違反」だけを見る関数なので意味は変えていない。

違反と判定不能が同時にある時は `Violated` を返す (反例のある側が証拠として
強いので、そちらを捨てない)。

#### 呼び出し側の棚卸し (2026-09-30、同日追加)

既存の `!has_hard_violations()` 9 箇所を読んで分類した。**締めるべきは 1 箇所だけ**で、
残り 8 箇所は「Modelled な法則が `Priority::Hard` を名乗っていない」ことや
「違反の部分集合を選ぶ」ための正当な用法だった (一括で置換すると、違反が出ることを
期待している assertion を壊す)。

- `tests/law_corpus_oracle.rs` の `gradient_bound_never_contradicts_the_static_claim`
  を `hard_verdict().is_proven()` に変更。**締めても green** で、この test の合格が
  「判定不能を合格に倒したもの」ではなく **証明された合格**であることが確定した
  (`GradientBound` の空振りは別軸 = 渡している静的上界が緩いこと)
- 同 file の `verdict()` helper を `hard_verdict()` の `match` に単一源化
  (本 file の law は全て `Law::hard` なので結果は不変)

#### `push` trigger 初回発火で出た生存変異 5 件の判定 (2026-09-30、run 36667642110)

trigger を足した初回発火で **生存変異 5 件**が出た。**いずれも同 commit で `--re` に足した
`hard_verdict` / `is_proven` のものではなく、既存関数**だった。2026-09-27 の前回 green 以降に
`law.rs` を触った commit は 9 本あり、うち `86b6fdb`「NonOverlap / Containment の残差を場の値から
実距離にする」が該当 2 関数そのもの。**trigger 不在 × main 直 push のため 3 日間不可視**だった。

`.cargo/mutants.toml` の規約 (等価 / 未達成 の区別 + 理由 + 実測日 + run id) に沿って判定した:

| 変異 | 判定 | 理由 |
|---|---|---|
| `delete - in check_non_overlap` / `check_containment` | **未達成** | `residual.min(-f32::EPSILON)` の clamp。差が出るのは `\|residual\| < f32::EPSILON` の時だけで、**単位スケールの f32 分解能は約 1.19e-7 = `f32::EPSILON` 自身**なので `0 < depth < EPSILON` は 2 値の差として表現できない。既存の `delete - in probe_pair` と同じ idiom・同じ理由 |
| `replace > with >= in gap_exceeds` (2 件) | **未達成** | `lo == 0.0` を厳密に作れない。`sdf_interval` は**外側丸め**なので `lo` は真値以下へ丸められ、厳密 0.0 が構造的に残らない (同型: `a02ab82` で alice-sdf 4.0 の外側丸めにより `interval_sign` の test が測れなくなった)。**方針は現状の `>` を維持** = `lo == 0.0` の箱を刈らず細分する保守側 |
| `replace match guard sign == outside with true in probe_ball` | **等価** | 一様に反対符号の箱は**表面を含まない**ので、この arm の bisect は「centre は外・箱の最近点は内 ⇒ 線分が必ず表面を横切る」という**近道**でしかない。guard を潰すと近道が消えるだけで、真の最近交点は混合符号の箱 (`None` arm) が返すため `best` の最小値は不変。差は探索順と枝刈り効率だけ |

除外の自己検査も実施: 追加した 4 regex が実 mutant 名に当たること (`--list` で 5 件が消える) と、
**`hard_verdict` / `is_proven` の 4 mutant が測定対象に残っていること** (除外しすぎていない) を確認した。

#### `quality-deep.yml` に `push` trigger を追加 (2026-09-30)

それまでは `pull_request` / `schedule` / `workflow_dispatch` だけで、**main 直 push を
既定運用とするこの repo では `pull_request` が事実上発火せず、実効 gate が週次 cron
だけ**になっていた。実測として、`hard_verdict` を足して `--re` にも登録した commit
(`7faf75b`) が mutants の測定を 1 度も通らずに main に載った。

`paths` は `pull_request` 側と同一内容 (判定器と判定器 test に限定) にしてある。
片方だけに足すと「PR では測るのに直 push では測らない」が残る。

`tests/law_tests.rs` に 6 本追加 (3 分岐 + Soft 判定不能の独立性 + 違反優先 +
既存 `has_hard_violations` の契約を記録する 1 本)。3 分岐それぞれに変異を入れて
red になることを実測済み。`quality-deep.yml` の `--re` に `hard_verdict` と
`is_proven` を追加した。

#### `Thermal` を実温度場にする `ThermalField` (`physics` feature、2026-09-29)

`Thermal` は「熱源近傍の 表面セル数 / 内部セル数 比」という**幾何 proxy**
だった。0.4.0 で場の値の距離への流用はやめたが、**放熱面積比そのものが
熱の物理ではない**ことは変わっていなかった。

`alice-physics` の 3D 熱伝導 solver
(`transient_thermal::transient_step_3d` + `ThermalMaterial`) で温度場を解き、
**「最高温度 ≤ 上限」**で判定する `Constraint::ThermalField` を追加した。

**注意:** `alice-physics` は **AGPL-3.0-or-later** なので、`physics` feature を
有効にすると下流にも伝播する。`deny.toml` に exception は足していない
(feature 表と README に伝播を明記する側で対応)。

#### 境界条件を 1 つに決めず、両端で挟む

表面からどれだけ熱が逃げるか (対流熱伝達率 `h`) は形状と設置環境で決まり、
設計時には分からない。`h` を決め打ちすると、その値が外れた分だけ判定が嘘に
なる。最高温度は `h` に対して**単調に減少**するので両端で解いて挟む:

- `h = 0` (断熱) → 最高温度の**上界**
- `h = ∞` (表面が周囲温度に固定) → 最高温度の**下界**

下界 > 上限なら違反 (どれだけ冷やしても超える)、上界 ≤ 上限なら合格
(一切冷えなくても収まる)、挟んだら未定
(`UnresolvedReason::TemperatureUnbracketed { lo_c, hi_c }`)。

#### 実装上の判断

- `transient_step_3d` は**熱源項を持たず、箱の 6 面が Neumann 固定**なので、
  熱源の注入 (`P·dt / (ρ·cp·dx³)`) と形状表面の境界条件は step の合間に
  LOL 側で与える (演算子分離)。断熱側は外側セルを隣接材料セルの値で埋める
  ghost cell 方式で勾配 0 = 流束 0 を作る。
- **単位を型で要求する**: `metres_per_unit` を必須パラメータにした。
  `alice-physics` の材料定数は SI なので、mm で設計した形状にそのまま
  当てると拡散率の効き方が 10⁶ ずれる。
- 内外が確定しないセルは材料として扱う (形状側の誤差は挟んでいない)。
  この限界は `Evidence::Modelled` の model 文字列に明記。
- solver の前提 (立方セル / 各軸 3 セル以上 / CFL で step 数が上限内) を
  満たさない時は**黙って通さず** `ThermalGridUnusable { why }` で未定にする。
- `Evidence` は `Modelled`。実温度場でも陽解法の数値解 + 格子離散化 +
  境界条件の両端しか挟んでいないので証明ではない。したがって
  `Priority::Hard` は名乗れない (`LawSet::thermal_field` は soft)。

#### 決定性

`transient_thermal` は `+ - * /` のみで `mul_add` も超越関数も使っていない
(実測)。IEEE-754 の正確丸め演算だけなので、Fix128 を経由しなくても
**cross-platform で bit-exact**。`thermal_solve_is_bit_reproducible` で固定。

#### `ThermalField` の物理 oracle と法則 oracle (2026-09-29)

期待値は**閉形式**から出しており、実装の出力を pin していない。

`law::evidence_gate_tests` (`physics` feature、5 本):

- `adiabatic_peak_is_bounded_by_the_energy_balance` — 断熱系は熱を失わない
  ので、最高温度は `平均上昇 = P·t/(ρ·cp·V)` 以上、`1 セル集中 = P·t/(ρ·cp·V_cell)`
  以下に必ず入る。さらに熱源セルの**定常超過温度は収束**し、その値は
  連続体の見積もり `P/(6·k·dx)` の 0.6〜1.4 倍に入る (熱伝導率と単位の扱いを固定)
- `no_source_keeps_the_field_at_ambient` — 定数場の Laplacian は 0 (厳密)
- `isothermal_never_exceeds_adiabatic` — 上下界の順序
- `isothermal_saturates_but_adiabatic_keeps_rising` — **等温境界は定常状態を
  持ち、断熱境界は持たない**。大小比較だけでは外側セルを素通しにしても
  通ってしまうため、境界条件の定義的性質をこちらで固定する
- `thermal_solve_is_bit_reproducible`

`tests/analytic_thermal.rs` (4 本) は法則として 3 値が 3 分岐とも出るか、
発熱を増やした時に判定が緩くならないか、Hard を名乗れないか、solver の
前提を満たさない格子で黙って合格しないかを見る。

実装を 3 通り壊して red を実測した: 熱源注入で熱容量を無視すると
エネルギー収支が、断熱の ghost cell を周囲温度にすると上下界の順序が、
**等温境界を無効化すると飽和の test が**落ちる。3 つ目は最初
`isothermal_never_exceeds_adiabatic` では捕まらず (外側セルを素通しにしても
大小は保たれる)、飽和の test を足して塞いだ。

#### 3 法則の解析解 oracle と表面帯の白箱 gate (2026-09-28)

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

#### 根拠 gate の機械検査 (2026-09-28)

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

#### 場の勾配と 2 点間到達性を法則として書けるようにする (2026-09-27)

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

#### 判定器の corpus oracle と検出力 gate (2026-09-27)

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
- **`tests/transpiler_naga_validate.rs` — grammar corpus 全構文 + fixture の WGSL / GLSL を naga で parse + validate** (Level 1.5、GPU 不要) 初回実行で alice-sdf 3.0.0 の `Terrain` が WGSL / HLSL に GLSL 構文を直書きし GLSL でも `vnoise` helper 未定義であることを検出 (SDF 側の課題)
- **`tests/gpu_parity.rs` — grammar corpus 全構文 + 深い合成 fixture 7 本を実 GPU で実行して CPU `eval` と突合** (Level 2、Milestone A.4.1) + ci.yml `gpu-parity` job (lavapipe、`ALICE_SDF_REQUIRE_GPU=1`) 初回実行 (Metal) で fixture 7 本は drift ≤ 5e-6、corpus 237 中 235 一致、`Elongate` の CPU (`p − clamp(p, −a, a)`) と shader (`abs(p) − a` + 内部補正) の法則不一致と `Terrain` の WGSL 不正を検出 (いずれも alice-sdf 側)
- `tests/common/corpus.rs` — grammar corpus / fixture / 標本点生成を `emit_roundtrip` / `transpiler_naga_validate` / `gpu_parity` で共有
- **fuzz target `fuzz_lol_emit_parity`** (parse → emit → parse の eval parity + emit 冪等性、strict-eval 8b) fuzz.yml matrix に追加、local 60 s / 1.27M run で crash 0
- ci.yml: `msrv` job (`cargo +1.90 check --workspace --all-targets --all-features`)、`gpu-parity` job

- `docs/LLM_LOL_ROADMAP.md` — 「モデルに LOL を話させる」Track A/B/C の status 表 + benchmark baseline + fail 型 4 + A3 の評価 leakage 注意 (repo 側の単一 status source)
- **`alice-lol-datagen` (新 sibling crate、Track C1)** — 合成 (caption, LOL) pair generator template family 8 種 (`primitive_placed` / `stacked` / `attachment` / `plate_holes` / `transformed` / `intent_program` / `random_tree` / `product_shortcut`) が同じ parameter から LOL text (学習 target、shortcut はそのまま) / `lol_canonical` (`parse → emit` 正規形) / 英日 caption / oracle 点を同時に生成、`Sample::verify` (parse 成功 / emit 冪等 / oracle 点の eval 一致 / `grammar-check` feature で LLM grammar 受理) を通ったものだけ出力 `datagen --n 20000 --seed 42 --families all --out data.jsonl` で **6,500 sample/s、rejected 0**  xorshift64* で決定論、serde 非依存 `llm_bench` baseline の fail 型 4 (translate 欠落 / half 引数 / 合成省略 / Intent 構造) を family 設計で直接狙う 自己検証は実装中に template の bug 2 件 (arch の穴が高すぎる / `repeat_finite` のコピー数は `2·floor(c/2)+1` で常に奇数・原点あり) を検出した
- **`runtime_parser::parse_expr` / `emit::write_node` を `stacker::maybe_grow` で stack 伸長** — stdlib product (`sd_card_holder` 等) は `subtract` を 2,400 段 nest した正当な tree を生成し、`parse_expr` の大 frame × 再帰で 8 MB stack が尽きていた (debug build は red zone 4 MB / 伸長 32 MB) 依存 `stacker = "0.1"` (MIT OR Apache-2.0) 残る深さ依存は `Arc<SdfNode>` の再帰 Drop (alice-sdf 側、2 MB の test thread で overflow、iterative Drop は follow-up)
- **`emit` の可変長 op 平坦化** — `union` / `intersection` / `smooth_union` 等は parser `fold_left` の左結合 2 分木を `union(a, b, c)` に戻す (同 op・同数値引数の左 spine のみ、右側の nested や `k` の違う smooth op は保持) `subtract` は binary のまま (LLM_REFERENCE の「cutter は nest、union でまとめない」規約と整合)
- **`emit::to_lol(&SdfNode) -> Result<String, EmitError>` + `Program::to_lol()` (Track C0)** — `runtime_parser::parse_lol` の逆変換 128 variant を exhaustive match (wildcard なし) で逆写像、LOL 構文のない 7 variant (`IFS` / `SdfSkinning` / `LatticeDeform` / `ProjectiveTransform` / `HeightmapDisplacement` / `SineDisplacement` / `Polygon2D`) は `EmitError::Unsupported` 正規形: 可変長 op は parser の左畳み込み通り 2 分木 (`union(union(a, b), c)`)、数値は `fmt_f32` (整数 `1.0`、`|v| < 1e-6` → `0.0` で stdlib 展開の cos/sin ノイズを吸収)、`rotate` は Euler XYZ 度を 1e-3 度に丸め `(-180, 180]` + `±180 → 180` に正規化 (度↔rad↔Quat 往復の 1 ulp drift と二重表現を固定点化) 出力は LLM grammar (comment なし、whitespace 1 文字) をそのまま通る `tests/emit_roundtrip.rs`: `lol.gbnf` の name bucket から **全 237 SDF construct** に既定引数 snippet を自動生成し `parse → emit → parse` を 64 点 eval parity + emit 冪等性で検証 (parser ⊆ grammar ⊆ emit を CI で固定) + fixture (sword / mug / product shortcut 展開形)
- **`capsule_ab(ax, ay, az, bx, by, bz, r)`** — 任意 2 点 capsule (`SdfNode::Capsule` の一般形) stdlib の SKADIS hook 等が生成する非 Y 対称 capsule を text に書き戻せるように追加 (`capsule(r, h)` は Y 対称の短縮形のまま) grammar `name_7f` + parser + LLM_REFERENCE
- `llm_bench` の出力表示を `Debug` から `Program::to_lol` (正規形 LOL) に変更
- **`lol.gbnf` に product / mechanical / fastener 構文 103 個を追加** (`pen_cup` / `coaster` / `gridfinity_bin` / `gridfinity_bin_ex` (新 `prim_7f`) / `wall_hook` / 12 archetype 3f 群 64 個 / `vesa_mount` / `l_bracket` / `screw_hole` / `heat_set_hole` / `jst_ph_slot` 等) 事案: 2026-08-20 Sprint C で text-to-print の grammar copy と `runtime_parser` には足されたが canonical `lol.gbnf` には upstream されず、mechanical 38 個はどの grammar にも無かった (system prompt は案内しているのに grammar ON だと emit 不能) 新 golden test `grammar_covers_every_runtime_parser_construct` (parser の `"name" =>` arm ⊆ grammar 名、include_str! で source を走査) で再発を CI で検知、`accepts_product_and_mechanical_shortcuts`
- **`alice_lol::LOL_GBNF`** (feature 外の `pub const`、`include_str!("lol.gbnf")`) — 下流 (非公開の pipeline crate → text-to-print 等) が grammar を copy して drift させる運用を廃止するための単一 source `bridge::lol_grammar` も同じ bytes を parse

- **`examples/llm_bench.rs` (A2-3、feature `llm-bridge`)** — LOL 生成品質 benchmark 20 prompt (T1 単体 primitive 5 / T2 合成 5 / T3 変換・修飾 5 / T4 Phase 3 Intent 5) を ChatML system prompt 付きで GGUF に投げ、**oracle** (T1-T3: ALICE-SDF `eval` の点内外判定、T4: `Program.intent` の合成種別 + verb 列 + entity id) で判定 grammar-only (`generate_program_from_prompt`) と think→grammar (`generate_program_thinking`) を同一 prompt で比較、tier 別 pass 率 + JSONL (`--out`、serde 非依存) A3 SFT の効果測定 baseline 用
  - **Baseline (MiniCPM5-2B Q4_K_M、arm64 CPU、prefix budget 800)**: grammar-only **7/20** (T1 3/5 / T2 0/5 / T3 2/5 / T4 2/5、46 s/prompt) vs think→grammar **9/20** (T1 5/5 / T2 1/5 / T3 1/5 / T4 2/5、138 s/prompt) parse は 40/40 (grammar が構文を保証) fail 24 件は全て model の実誤りで型は 4 つ: translate 欠落 (指定座標を無視して原点) / half 引数の誤解 (cylinder half_height に全高、box3d に全寸) / 合成の省略 (mug handle・table 脚・arch subtract・plate 穴を落とす) / Intent 混入・欠落 (幾何 prompt に `program(…, seq(rest(1000)))`、Intent prompt で `entities()` 省略、`par` を `seq`) think は単体 primitive を完全に直すが合成は直らない = 語彙・引数規約の学習不足で A3 SFT の対象
- **`bridge::generate_program_thinking`** (feature `llm-bridge`、alice-llm B-11 `generate_grammar_prefixed` の one-call wrapper) — think-first model が `<think>…</think>` を grammar の外で書いてから LOL を emit、`ThinkingOutput { program, prefix_text, prefix_marker_hit, text, prefix_ms, decode_ms, total_ms }` を返す MiniCPM5-2B 実測: grammar のみは `cylinder(50,100)` (handle 省略)、think ありは `union(cylinder(25,50), translate(25,0,50, rotate(0,90,0, torus(12,4))))` (mug 完全正解) `GrammarGenResult` / `GrammarPrefix` を re-export
- `tests/lol_gbnf_test.rs` — `LLM_REFERENCE.md` の coaster example を verbatim で grammar + parser 両方に通す golden test (reference が示す形 = grammar が受理する形、の drift 検知)
- **Phase 3 Intent text 構文 (A0)** — `runtime_parser::parse_program(&str) -> Program` を追加 `program(<sdf>)` / `program(<sdf>, entities(...))` / `program(<sdf>, entities(...), <intent>)` の 3 形 + 裸 SDF 式 (後方互換 = `Program::sdf_only`) を受理 Intent verb 18 種 (`grasp` / `release` / `catch` / `walk` / `gaze` / `point` / `throw` / `push` / `pull` / `turn` / `align` / `follow` / `avoid` / `rest` / `latent` / `seq` / `par` / `music`) を `IntentNode` field 順の引数で parse、entity id 範囲・整数 slot・latent dim ≥ 4・music 8 byte を parse 時検証 `IntentNode::Rotate` の text verb は SDF transform `rotate` と衝突するため `turn`
- `IntentNode::to_lol()` / `HandSide::as_lol()` — Intent tree → LOL text (parse_program の逆変換、`semantic_hint` は落とす)
- `lol.gbnf` — `root ::= ws (program | expr) ws` に拡張、`program` / `entities` / `intent` rule 群を追加 (`skills/lol-sdf/references/lol.gbnf` も同期)
- `bridge::generate_program_from_prompt` (feature `llm-bridge`) — grammar-constrained decoding → `parse_program` の one-call wrapper
- tests: `tests/program_parser_tests.rs` (11、全 verb parse + round-trip + error) / `tests/lol_gbnf_test.rs` に program / intent golden 3 件追加

### Changed

#### 監査 Law の証拠・成立範囲・期待値の読み方を固定する (2026-10-09)

- **挙動変更:** `audit_law` の `evidence` は有限で 0 より大きい数だけを証拠と読む これまでは `0 でない` を証拠としていたので、負の数・NaN・±∞ が証拠として通っていた 今後はどれも `Verdict::NoEvidence` になる
- **挙動変更:** 成立範囲 (`range`) は集合として比べる `Measurements::with_range` と `parse_law` の `range` 行はどちらも昇順に並べて重複を除く (同じ並び順の key を持つ `1.0` と `1-0` は文字列で順を決める) これまでは重複を残していたので、同じ集合に重複があると `Verdict::ParameterUpdate` になっていた 今後は `Verdict::Supports`
- **挙動変更:** `expect` の実測が有限でない (NaN・±∞) 時は測られていないのと同じに読み `Verdict::Undecided` を返す これまでは NaN が `(NaN - v).abs() > tol` が false になるので `Verdict::Supports` に、±∞ は `Verdict::Breaks` になっていた
- 移行: 影響を受けるのは、件数に負の数を渡して「測った」ことを表していた呼び出し側、成立範囲の重複の数に意味を持たせていた呼び出し側、期待値に NaN・±∞ を渡していた呼び出し側 前者は件数を 0 より大きい数で渡す (測れていないなら 0 か値を渡さない) 2 つ目は重複の数を別の数の量 (`with_number`) で渡し、`expect` で判定する 3 つ目は計算できなかった量を渡さない (測れていない扱いで `Undecided`) Law の text に重複を書いていた場合は、重複を除いた Law と同じ Law になる (`AuditLaw` も識別子も一致)
- `law_id::audit_verdict_order_fingerprint` に 3 case (証拠が -1 / 成立範囲に重複 / 期待値の実測が NaN) を足した 既存の 4 case はどの読み方でも同じ判定を出すので、足さないと読み方を変えても pin が動かない
- 再記録: 判定順序の pin `49769f9b…` → `14e30eaa…`、`law_id::LOL_SEMANTICS_ID` `673481121a…` → `bc2befdc…` (上の 3 case が判定の並びを変えたため) 監査 Law の識別子はすべて変わる 旧値は `tests/law_id_oracle.rs` に名前つきの定数で残し、新旧が違うことを試験する
- 公開済の識別子は変わらない `alice-lol` 0.4.0 は未公開で、crates.io の最新は 0.3.0 (`law_id` を持たない)
- 試験: `alice-lol/tests/audit_law_semantics.rs` (`scripts/law_tests.sh` と変異試験の対象に追加)

#### `alice-zip` の要求を 0.8 に上げる (2026-10-09)

- `alice-zip = "0.7"` → `"0.8"` 上流の 0.8.0 は残差の容器と既定 encoder を変える破壊的変更だが、この crate が使うのは `law::{IngestPolicy, Provenance, ResidualStats, ValidRange, SignalLaw, LAW_ID_DOMAIN}` だけなので経路外
- `law_id::LOL_SEMANTICS_ID` は不変 (保存した Law の識別子はずれない)

#### `export_formats` example — 出力形式の入口を production 経路に繋ぐ (2026-10-09)

- `lol_to_3mf` / `lol_to_fbx` / `node_to_fbx` / `PrintConfig::ultra` は `src` / `tests` / `examples` から呼び出しが 0 件で、壊れても気付けない状態だった
- 同じ mesh を STL / 3MF / FBX に書き、書かれた file 側から面数を数え直して突き合わせる (STL は `84 + 50n` byte / 3MF は非圧縮の `<triangle ` を byte 検索)

#### 配線ガードが `Type::method` の修飾子を所有型と突き合わせる (2026-10-09)

- method は `Type::name` で修飾された参照だけがその型の member を配線済にする 別の型の同名 method は配線済にならない
- `.name(` の呼び出しは型を特定できないので従来どおり配線済に倒す (偽陽性より偽陰性の方針は変えない)
- 厳格化で真の未配線 4 件が露出した (呼び出しが `#[cfg(test)]` 内か doc comment だけだったもの) baseline に記録済

#### `readme_sync` が example の一覧を検査する (2026-10-09)

- `alice-lol/examples/*.rs` の集合と両 README の Example 表の行を双方向で突合する (表に無い example / 実在しない行 / ディレクトリ自体の不在をそれぞれ名指しで落とす)
- 入れた時点で 13 example が両 README の表から落ちていたので、同じ commit で表に追加した

#### **Breaking:** pattern registry の 2 つの名前を内容に合わせて改名

公開していない姉妹 crate の名前を含んでいた 2 項目を、何を表すかで名付け直した

- `CertificationSource::BambooSimulation` → `CertificationSource::SimulationOnly`
- `PatternSpec.bamboo_canonical` → `PatternSpec.canonical_kind`

`canonical_kind` は値の意味も変わっている: 以前は非公開 crate 内の file path を持っていたが、
canonical 実装の種別 (`"python generator"` / `"rust generator"`) を持つ
`certifications.yaml` の `certified_by` の値も新しい名前に合わせた

#### `alice-det-math` 0.4 へ追従

`alice-lol` の `alice-det-math` の要求を `0.3.2` から `0.4` に上げた
法則を保存・復元する経路 (`alice-zip` 0.7 / `alice-physics` 2.0) が既に `^0.4` を要求しているので、
`research_law` が法則を評価する時の超越関数と、保存した法則を復元する時の超越関数が同じ世代になる
`research_law` が使うのは `sin64` / `cos64` / `exp64` / `ln64` / `sqrt64` / `powf64` で、0.4 でも同じ名前と signature
`tests/research_law_bit_exact.rs` (22 本) は評価結果が `alice-det-math` の関数と bit 一致することを固定しており、
上げた後も同じ 22 本が通る
`scripts/lock-duplicates-baseline.txt` の `alice-det-math` の行は残る 残る `^0.3` 要求は `alice-sdf` (`^0.3.1`) だけで、
0.3 系の最新が 0.3.2 なので解決結果の版の組は `0.3.2 0.4.0` のまま変わらない

#### `alice-physics` 2.0 へ追従

**Breaking:** `alice-lol` と `alice-world-auditor` の `alice-physics` の要求を `1.0` から `2.0` に上げた (どちらも `physics` feature の optional dep)
`physics` feature を使い `alice-physics` 1.x を pin している利用者は 2.0 へ上げる必要がある
`.github/workflows/fuzz.yml` が optional path dep に作る stub の版も `2.0.0` に合わせた (stub の版が要求を外すと resolve 段で全 job が落ちる)
2.0.0 の破壊的変更 (`#[non_exhaustive]` の一括付与と enum の variant 追加) に当たる呼び出しは無く、`cargo build --features physics --all-targets` と `cargo test --features physics` はどちらも通る
要求を上げるまでは `^1.0` が sibling の 2.0.0 に一致せず、cargo を使う job が resolve 段で落ちていた

#### `alice-zip` 0.7 へ追従

`alice-lol` の `alice-zip` の要求を `0.6` から `0.7` に上げた `0.7` は配列の再構成法則を `f64` で積んで返り値で 1 度だけ `f32` に丸める形に替えており、`generators` の返す bit が下位で変わる
`alice-lol` は `alice_zip::law` の型 (`SignalLaw` / `IngestPolicy` / `Provenance` / `ResidualStats` / `ValidRange`) だけを使い `generators` を呼んでいないので、`research_law` の挙動は変わらない
要求を上げるまでは `0.6` と `0.7` のどちらでも解決できない状態だった (`^0.6` に `0.7.0` が一致しない)

#### `alice-sdf` 5.0 へ追従 (破壊的)

`alice-lol` / `alice-lol-ui` の `alice-sdf` の要求を `4.0.0` から `5.0` に上げた
`alice-lol` が再 export している `Vec3x8` から、`alice-sdf` 5.0 で消えた method 12 件 (`zero` / `length` / `length_squared` / `normalize` / `dot` / `abs` / `max_zero` / `max` / `min` / `clamp` / `max_component` / `min_component`) が無くなる
移行: field (`x` / `y` / `z` は `wide::f32x8`) に `wide` の演算を掛ける (例 `v.length()` は `(v.x * v.x + v.y * v.y + v.z * v.z).sqrt()`)
本 crate 内に 5.0 で変わった API の利用は無く、code の変更は無い

#### `alice-zip` 0.6 へ追従

`alice-lol` の `alice-zip` の要求を `0.5.1` から `0.6` に上げた (本 crate が使うのは `research_law` が再 export する `law` の型だけで、code の変更は無い)

#### `alice-world-auditor`: Phase 5 の oracle を `alice-physics` の XPBD 速度の修正に追従

`alice-physics` の XPBD は、拘束も接触も動かさなかった body の線速度を予測速度のまま保つようになった (位置差からの再導出をやめた)
これに合わせて `tests/phase5_verdict_oracles.rs` の 3 件を更新

- 正負の impulse が同数の行動列では速度が厳密に `0` になる dyadic でない 537 frame の scene は位置だけが target を外れる
  (`ReplayMismatch`、`at_rest == true`) として前提を書き直した
- dyadic でない行動列 (`a = 5, dt = 1/60`、target [0.9, 1.1]、51 frame) は再生結果が target 内で速度 `0` になり `Proven` を返す
  `Proven` の根拠は再生結果で、dyadic な `a*dt` / `dt` は十分条件の 1 つにすぎない この scene を `Proven` の oracle に変更した
- 「target 内だが動いている」場合の `Undecided` は、goal 軸以外の初速度 (`v.y = 1/64`) を持つ dyadic な scene で閉形式から検査する
- overflow する substep では範囲内の速度が保たれるため、旧 scene では最終状態が goal を満たさなくなった
  位置だけの goal で、1 substep 進んだ位置が target 内に残る scene に置き換え、`Undecided(Overflow)` を検査する
- `audit` の doc と crate README の `Proven` の十分条件を「重力 0、goal 軸以外の初速度 0」に改め、十分条件であって必要条件ではないことを明記した

#### `research_law` を `alice-det-math` で評価

- 超越関数 (`exp` / `ln` / `sqrt` / `sin` / `cos` / 非整数の冪) を `alice-det-math` 0.3.2 の f64 関数で評価し、整数の冪と単位の倍率は下位 bit からの繰り返し 2 乗で計算する ⇒ 同じ法則と条件はどの platform でも同じ bit を返す (`tests/research_law_bit_exact.rs`)
- `alice-zip` の下限を `law` module を含む 0.5.1 に上げた

#### README を書き直し、`README.ja.md` を `README_JP.md` に改名

英日で同じ節構成 (向き不向き / インストール / 使用例 / 位置づけ / 構文 / 幾何法則 / 研究 Law / crate / feature / example / MSRV / ビルドとテスト / 関連 crate / ライセンス) にした
使用例は crate doc の doctest と同一 (doctest は `ignore` を外してコンパイル・実行する形に変更)
構文表はグループごとに parser の名前の一覧と一致させ、`terrain` と `capsule_ab` の記載漏れを解消した
検査されていない件数・版数・日付つきの見出し・非公開 repository への言及を削除した `research_law` の節に、
超越関数を `alice-det-math` の `f64` 関数で評価して全プラットフォームで bit 一致にすることを記載
`alice-lol` の crates.io 向け README を root の README.md に変更し、内容が古くなっていた `alice-lol/README.md` を削除

#### コメントと文書の表記を整理

コメント・workflow・文書から内部の記録名・機器名・home directory の path を除き、技術的な内容は残した
個人のローカル設定ファイルの ignore を `.gitignore` から外した (各 clone の `.git/info/exclude` で扱う)
`scripts/wiring_guard.py` は隠しディレクトリを一律に走査対象から外す
CHANGELOG の `[Unreleased]` を Keep a Changelog の分類ごとに 1 つへまとめ、各項目の見出しを `####` にした


#### CI: `ALICE-Zip` を sibling として checkout

`alice-lol` を build する job (ci / fuzz / quality-deep / security-audit) で `ext-sakamoro/ALICE-Zip` を `ALICE-Zip` に checkout する

#### `alice-world-auditor`: example `plan_rest_to_rest` を `audit` 経由に変更

dyadic な scene (`a = 4, dt = 1/64`) で `Proven` 640 frame、既定 config (damping 0.99 / substeps 8) で `Undecided` になる両方を出力し assert する
`plan` の挙動は変えていない (探索部を内部関数に切り出して `audit` と共有) `plan` の `Ok(Optimal)` は整数格子上の主張で、engine 上の成立は検査しない
旨を doc に明記

#### 残差を場の値から実距離にする (2026-09-28)

`NonOverlap` / `Containment` の**判定**は符号だけを見るので 0.4.0 で健全に
なっていたが、**報告される残差は場の値のまま**だった。場の値は距離ではない
ので「侵入深さ」「はみ出し量」を名乗れず、`top_violations` の順位も
信用できなかった。

- `NonOverlap` の残差を `max(fa, fb)` から、証拠の点から**浅い方の表面までの
  実距離** (`probe_ball` の交点を二分探索で締めた上界) に変更。実測: 半径 1 の
  2 球 (中心 ±0.5) の union の原点で、場は −0.5 だが真の侵入深さは 0.866 —
  **42% 浅く報告していた**。
- `Containment` の残差を `−outer` から、証拠の点から outer の表面までの
  実距離に変更。実測: 半径 2 の 2 球 (中心 0 と 3) の intersection の外側で、
  場は 1.6458 だが真のはみ出し量は 2.0 — **18% 小さく報告していた**。
- `surface_distance` を追加 (private)。`probe_ball` を半径を倍にしながら
  呼ぶ。上限は検査 AABB の対角と**場の値の 16 倍**の大きい方 — 検査範囲を
  1 セルに絞った使い方だと AABB の対角が表面まで届かないため。

**残る制約**: 証拠の点を選ぶ順位付けには引き続き場の値を使う (安いため)。
報告される残差は実距離だが、同一法則内で「どの証拠を報告するか」の選択は
場の値順のままなので、場が大きく歪む形状では最悪点と報告点がずれうる。
また形状が八分木の葉 (`BALL_PROBE_DEPTH = 4`) より薄いと `probe_ball` が
点標本で跨げず距離を決められないため、その場合のみ場の値に戻す。

#### 残り 3 法則を三値化し、場の値の距離への流用をやめる (2026-09-28)

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

#### 証明のない法則に `Priority::Hard` を名乗らせない (2026-09-28、breaking)

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

#### alice-sdf 4.0.0 追従 (2026-09-28)

- **`alice-sdf` 要件 `3.0.0` → `4.0.0`** — 4.0.0 は計量そのものを値にする 2 node と場の主張を測る 2 API を追加し、`SdfNode` / `OpCode` を `#[non_exhaustive]` にした 外部 crate からの wildcard なし `match` は `E0004` になるので `emit::write_node_inner` に `MetricBall` / `MetricBlend` の arm と wildcard arm を追加 (未知 variant は黙って捨てず `EmitError::Unsupported` として報告する)
- **`law::interval_sign` の unit test を `Interval { lo, hi }` の struct literal 構築に変更** — alice-sdf 4.0 から `Interval::new` が外側丸め (`next_down` / `next_up`) を掛けるので、`new(0.0, 1.0)` の lo は 0 のわずか下になる すると「lo が厳密に 0 なら外側と断定する」という**検査対象そのものの性質**が測れなくなる (答えが `None` に化ける) ので、境界を動かさない構築に寄せた
- `Cargo.lock` / `fuzz/Cargo.lock` — `alice-sdf` 4.0.0 / `alice-det-math` 0.3.1 に更新

#### breaking: 法則検証器が場の値を距離として使わなくなった (0.4.0)
- **距離依存 5 variant (`MinThickness` / `Stress` / `NonOverlap` / `Containment` / `Contact`) を Lipschitz 非依存の三値判定に置換** — 旧実装は `sdf_eval` の返り値を距離として使っていたので、TPMS (場が距離を √3〜7 倍に過大申告) では 0.058 の薄壁を 0.1 と読んで合格させ、union の内部 (場が過小) では 0.87 の肉厚を 0.5 と読んで不合格にし、格子より薄い重なり / はみ出しは標本点をすり抜けていた (セルフレビュー 2026-09-16 § 4「検証器が嘘をつく」) 新実装は `alice_sdf::interval::eval_interval` (区間演算) で「箱に表面なし」を **証明**、点評価の符号変化 (中間値定理) で「表面まで ≤ |p − q|」の **証拠** (二分探索で締めた上界) を取り、八分木深さ `BALL_PROBE_DEPTH = 4` で決められなかった標本点は **unresolved** として報告する 違反の検出は健全 (偽陽性なし)、合格は標本点ごとの証明
- **`LawReport` に `unresolved: Vec<Unresolved>` を追加、`all_passed()` は「違反なし かつ 判定不能なし」に変更** (silent 合格の廃止、struct literal で `LawReport` を組んでいる下流は field 追加で breaking) 新 API: `LawReport::has_unresolved()` / `Unresolved { law_name, priority, point, region, reason }` / `UnresolvedReason` (`SurfaceProximity { radius }` / `SignUndecided` / `GapUnbracketed { upper }`、`#[non_exhaustive]`) / `format_report` に `[UNDECIDED]` 行
- `Contact` の residual: 近すぎは `上界 − min_distance`、遠すぎは `max_distance − 上界` (上界を取れなければ `−∞`)、gap > m の証明は各 cell を m/2 広げた箱の区間で行う (中点が検査 AABB 内にある前提)

- **`rust-version = "1.90"` を全 workspace crate に宣言** — clippy `incompatible_msrv` が `from_f32_snap` 等の const fn で `f32::round` (const 化 1.90) と `Vec::is_empty` (1.87) を指摘、1.85 (alice-sdf の MSRV) を宣言すると偽 MSRV になる 下流 (Manga / Foundry / Print / Kinematics 1.92.0、pipeline / LLM 1.98.1) の toolchain pin は全て上回る `cargo +1.90.0 check --workspace --all-targets --all-features` 通過
- **ci.yml clippy を `-D warnings -D clippy::pedantic -D clippy::nursery` + `--workspace --all-targets --all-features` に昇格** (旧: lib のみ `--features llm-bridge` で `-W`) 昇格に伴い example 2 本の raw string hash / doc backtick を修正
- ci.yml の manifest-only stub (`alice-physics` 空 lib + 参照されていない Codec / Streaming / Cache / Foundry) を real `ALICE-Physics` checkout に置換 (`--all-features` で `physics` feature が実 compile されるため)
- `alice-sdf` 要件 `2.0.0` → `3.0.0` (`ShaderLang` seal のみ breaking、LOL は trait を実装していない)

- **clippy pedantic + nursery を workspace `--all-targets` で 0 warning に** (`e51e3c7` 機械適用 1160 → 532、`cefd9ed` 手修正 532 → 0) — `runtime_parser` の f32→u32 cast 56 箇所を `lol_u32` 単一 audit 点に集約 (per-site allow 8 個撤去)、`law` の grid index を符号なし演算 (`checked_sub` / `then_some` / `abs_diff`) に、`laser_pattern` の cast を `cell_count` / `cell_center` helper に集約 + 固定間隔 hatch を整数 index 刻みに、`pattern_sdf` の u32→f32 100 箇所を `n_f32` に集約 数値は `mul_add` (fused、単一丸め) と hatch の `d0 + i·spacing` (累積加算廃止) のみ変化、CI matrix 3 set の test 全 green 残す allow は理由付きのみ (`similar_names` = 幾何寸法の慣習命名 / `while_float` = 可変 step / demo example の `too_many_lines` 等)
- **`lol.gbnf` の whitespace を token 間 1 文字 (`ws ::= [ \t\r\n]?`) に制限 + `//` line comment を除外** (LLM 経路 grammar を runtime lexer より厳格化、grammar ⊂ parser) 無限 `ws` も rambling channel で、comment 除去後に MiniCPM5-2B が `\t` × 41 を連発した (evidence `03_*.log`) ため 1 文字に制限、以後は `cylinder(50,100)` で正しく final → EOS (evidence `05_*.log`、mask max 14 ms/step) comment 状態は `noteol*` でほぼ全 char を受理するため alice-llm B-10 token-trie mask が vocab 全走査に退化 (MiniCPM5-2B 実測 0.5-1 s/step) し、model も comment に思考を書き続けて本体を出さない (`// mug body` `// torus handle` … 48 token で本体ゼロ) 除外後は同 prompt で mask avg 5 ms/step、forward 律速  `parse_lol` / `parse_program` は comment を従来通り受理 (人間の `.lol` file は無傷) golden test `accepts_line_comments` → `rejects_line_comments`、`accepts_whitespace_and_newlines` → `accepts_single_whitespace_rejects_runs`、`sword.lol` golden は whitespace collapse して照合、`skills/lol-sdf/references/lol.gbnf` 同期、`LLM_REFERENCE.md`: Syntax Rules に rule 7 (comment / indent 禁止) 追加、LOL code block 7 箇所の `//` comment を prose に移動、LOL block 16 箇所の indent 除去 (model に grammar が弾く形を教えない、数式 / Rust block は対象外)

- `alice-physics` optional dependency requirement `0.14.0-preview.4` → `1.0` (alice-physics 1.0.0 stable、2026-09-14) `physics` feature は 1.0 API で clippy clean CI stub も 1.0.0 に追従
- `rust-toolchain.toml` pin `1.92.0` → `1.98.1` (6 release 遅れで `cargo-semver-checks@latest` の MSRV に追い抜かれていた)、`cargo-semver-checks` を `0.50.0` に明示 pin

### Fixed

#### 修復が mesh の位相を変えても採られていた (2026-10-09)

- **Behavior change:** `print_export::node_to_mesh` の破壊的修復 (退化除去 / 頂点統合 / 重複面除去) は、修復の前後で **`χ` が等しく、孤立頂点が増えない**時だけ採る 従来の条件 (境界 edge + 非多様体 edge が増えない) は残す
- `χ` は index から参照される頂点だけで数える (孤立頂点は位相に寄与しない) 孤立頂点は「どの index からも指されない頂点」
- 従来は gyroid (bounds ±2.0) の res 64 / scale 2.0・2.5・3.0 で修復が採られ、境界 edge を 3 / 18 / 30 本減らす代わりに三角形を 3,179 / 3,102 / 5,182 本落とし、`χ` を −20→−39 / −34→−58 / −106→−127 に変えていた (scale 3.0 は孤立頂点 6 個も残した) この 3 件は却下され、出力は同位置頂点の統合だけを掛けた mesh になる
- gyroid の res 32 / 64 / 128 × scale 0.5〜4.0 の 21 件のうち残り 18 件と、閉じた形状 (球 / トーラス) の出力は変わらない
- 小さな手組みの mesh で `χ` と孤立頂点の数え方、gate の 3 条件がそれぞれ単独で効くことを単体 test で固定した

#### 面積 0 の三角形が export に残っていた (2026-10-09)

- **Behavior change:** `print_export::node_to_mesh` は同じ位置に落ちた頂点を統合してから、畳まれて面積 0 になった三角形を落とす 出力の三角形数と頂点数がその分だけ減る (slicer は面積 0 の facet を拒否する)
- marching cubes は格子の辺ごとに 1 頂点を作るので、格子点が表面ちょうどに載ると複数の辺の頂点が同じ位置に重なる 統合は `f32` の bit 完全一致だけで行う (距離で溶接すると別の位置を巻き込んで非多様体 edge が出る)
- 水密性は不変 (境界 edge 0 / 非多様体 edge 0 / `χ` は変わらない) 畳む対象が無い形状では出力が 1 byte も変わらない
- 畳む規模が格子と一緒に増えることを `octahedron(1.0)` の res 32〜256 で固定した (res に依らず定数しか畳まない実装を捕まえる)

#### `hardsurface::{mount, skadis_sdf}` の出来上がりの形の欠陥 11 件 (2026-10-02)

`cavity` / `mount` / `skadis_sdf` に閉形式 / 文書の寸法と突き合わせる oracle 35 本を足し、`eval` の符号変化の二分法で出来上がりの形を測ったところ、
`cavity` は全項目が正しく (穴の半径・位置・±5mm の余裕・blind の底・皿の 90 度円錐)、`mount` / `skadis_sdf` に欠陥が見つかった 既存の単体 test には
修正前の誤った値をそのまま固定したもの (`skadis_peg_dimensions_match_pipeline_spec` の内側寸法、`bracket_l` の `k == R`) があったので、出来上がりの形を測る形に直した

**`mount`**
- `rack_shelf`: ALICE-SDF の `RepeatFinite` は「`count` 個のセルを `±count/2` にクランプ」するので `notch_count` をそのまま渡すと
  完全な穴 3 個 + 位置のずれた半端な穴 2 個 (n=3) になっていた `2 * notch_count` を渡して `2n + 1` 個にした
- `skadis_peg_compat(5.0)`: 実寸 9.8 x 20 x 10 を 4.8 x 15 x 5 に (`RoundedBox` の外寸は `half_extents + round_radius`、角丸は最小の半寸法で頭打ち)
- `bracket_l` の fillet: `SmoothUnion` の `k` に R を渡していて、角から対角線上の面までの距離が円弧の 85% だった `k = (4 - 2√2)·R` に直した
- `profile_2020` / `profile_3030`: 断面が 4 つの連結成分 (バラバラの角ブロック) に分かれ (2020)、T スロットの首が設計の半分だった
  スロットの寸法を供給元の寸法表に合わせ (開口 6.0、首 1.8、全深さ 6.1、空洞幅 11)、空洞を 45 度の斜壁で底へ狭まる台形にして対角にリブを残し、
  開口を表面に一致させた (`PROFILE_SLOT_*` を追加)  旧定数 `T_SLOT_2020_*` (首 5 + 空洞 6 = 11mm) は 20 mm 角に収まらなかった
  `joint::t_slot_2020` は変更していない

**`skadis_sdf`**
- `skadis_panel_sdf`: X / Z の外寸が `size + 2R` (±155) だったのを `size` (±150) に (mount の peg と同じ `RoundedBox` の誤解)
- peg 穴の千鳥格子: 端の余白 (`SKADIS_EDGE_MARGIN` 20mm) の内側に出る穴 (x または z が +140 の 13 個) と、穴の格子が左右非対称な点を、
  使用可能範囲 `|中心| <= size/2 - 余白` に入る添字だけを並べて直した
- `skadis_container_sdf`: 底厚 1.1mm (仕様 1.6mm) を直し、背面ペグを 1 個から 2 個 (x = ±20) にして、背面の裏側へ板厚 5mm 突き出すよう向きを直した
  (peg は X 方向に伸びるのに背面は Z 方向で、回転が無く 2.5mm しか出なかった)
- `skadis_shelf_sdf`: 背面ペグの向きを同様に直した (2.4mm → 板厚 5mm 突き出す)
- `capsule_polyline_sdf` (hook L / J / S と elastic cord): 3D の `Capsule` 管 (断面が直径 2R の円、`hook_width` は管より狭くしか効かず幅 8 の hook が直径 7 の管)
  から、Z 軸の円柱を edge の向きに `Elongate` した平らな帯 (面内 2R x Z 方向 `hook_width`、端は面内で丸い) に直した
  (pipeline の `LineString.buffer(R)` + 押出に相当)
- 変異 47 種 (cavity / mount / skadis の既存実装 31 種と、修正側 16 種) が red になることを実測した

#### `roblox_export` の 3 プリセットが水密でなく三角形上限を守れなかった (2026-10-02)

`roblox_export` は test が 1 本も無く、CI の test matrix にも feature `roblox` を有効にする entry が無かった
閉形式の oracle (`tests/analytic_roblox_export.rs`、17 本) を足して `sphere(1.0)` を測ったところ 3 件の欠陥が見つかった

- **水密でない**: `MeshRepair::repair_all(&mesh, 5e-3)` を固定の絶対値で呼んでいた (cell 幅の 7〜11% で、`print_export` が
  2026-09-30 に直したのと同型) 境界エッジ 39 / 90 / 39  `print_export::node_to_mesh` (許容量は cell 幅比、水密な mesh には
  破壊的操作を掛けない) に任せる形にした
- **三角形上限を守れない**: `estimate_resolution` の経験則 (res 128 で約 2 万三角形) が実測 (約 3.5 万) と合わず、
  accessory 7183 (上限 4000) / meshpart 17962 (上限 10000)  経験則をやめ、粗い試し (32) で三角形数を見積もって
  上限に収まる最大の解像度へ進み、超えていれば解像度を単調に下げる (下限 16) 実測は accessory 3632 / meshpart 9512
- **preview が accessory と同一**: preview の解像度 64 が自動推定 57 に潰されて accessory と同じ mesh だった
  preview の解像度上限を 32 にした (2312 三角形)  `RobloxConfig::resolution` は上限として扱う
- doc の「制約違反時に警告をログ出力」は実装に無かったので、実態 (`RobloxExportStats::validation` に載る) に直した
- CI: `--features roblox` の test matrix entry と preflight の step を追加した (これまでこのモジュールの test は CI で 0 件実行)
- 既知の限界 (未対応): 既定 bounds (±2) より大きい形状は黙って切られ、開いた mesh になる (doc の使用例の帽子も該当)

#### 3D プリント出力の水密性と安全法則の緩み 4 件を直した (2026-10-01)

**`print_export` の修復が水密な mesh を壊していた**

`MeshRepair::repair_all(&mesh, 5e-3)` を無条件に掛けていたため、生の
`sdf_to_mesh` が res 32〜256 すべてで水密 (境界 edge 0 / `χ = 2`) だったのに、
既定 config (res 128) で境界 edge 741 + 非多様体 285 + `χ = −86` になっていた。
測った全 case で「修復が変えたなら必ず悪化」で、改善した case は 0 件。

- 水密な mesh には破壊的操作 (退化除去 / 頂点マージ / 重複面除去) を掛けない
- 掛けた結果が悪化したら採らない (非回帰)
- 頂点マージ許容量を cell 幅相対にした (旧 `5e-3` は res 128 で cell の 16%、
  破れ始める閾値は約 8%)
- 水密性の判定から退化三角形を外した。零面積 sliver を欠陥に数えると、水密な
  mesh でも破壊的修復に入って境界 edge 216 枚を作る
- 向きは符号付き体積で大域的に決める。facet ごとに勾配で判定すると隣接 facet と
  winding が食い違い、幾何的に閉じた mesh に境界 edge が 288 枚現れる

**`SafetyLaw` が doc で宣言した検査を一部持っていなかった**

- `Throw::force` を Overforce の対象に加えた。module doc と `max_force` の doc は
  当初から対象と書いていたが、実装は `Throw` 腕で force を捨てていた
- 速度 / 力 / 角度を大きさで判定するようにした。符号なし比較では `-5000` が
  素通りし、`Rotate` だけが `abs` を取っていて同 file 内で契約が不統一だった
- `NaN` 座標を `OutOfWorkspace` にした。6 つの比較すべてが false になるため
  「範囲内」と判定されていた。違反 0 件が安全ではなく比較が成立していない状態

**`MetricSize::from_f32_snap` が非有限と大きな有限値で誤った値を返していた**

- 範囲外は距離計算に任せず明示的に clamp する。距離で argmin を取ると入力が
  大きいほど候補間の差 (最大 6mm) が仮数に吸収され、`1e9` 以上で最遠の M2 が
  返っていた (doc は「上限 clamp」と記載)。f64 に上げても `f32::MAX` では
  相対 1.8e-38 で潰れるので、精度を上げる方向では直らない
- 非有限を弾く `MetricSize::try_from_f32` を追加し、`runtime_parser` の 11 箇所を
  そちらに通した。LLM が生成した値が非有限でも M4 のねじ穴が出ていた経路を閉じた
- `COUNTERSUNK_TAPER_ANGLE_DEG` を `countersink` が実際に読むようにした。
  pub 宣言されているだけで一度も参照されておらず、定数を変えても形状が
  変わらなかった (90° では `tan(45°) = 1` なので値は従来と厳密に一致)

#### 包囲が矛盾した箱の扱いを 3 つの呼び出し元すべてで契約どおりにした (2026-10-01)

`probe_ball` / `probe_pair` / `gap_exceeds` の doc はいずれも「包囲が矛盾した箱では
主張しない」と書いていたが、実装は **区間だけで決まった箱では矛盾を見ていなかった**。
矛盾は 2 つの包囲のどちらかが健全でないことなので、区間側の主張も信用できない。
実測で `alice_sdf` の `eval_interval` が真の最小値を包囲から外す case (`rounded_cone`)
があり、それは偽の `proven` を生む向きの誤りなので、刈らずに未決定へ倒す。

- `gap_exceeds`: `iv.lo > 0.0` の早期 return を外し、`refine` を単一の判断点にした
- `probe_ball`: `interval_sign` が先に決まった箱でも `refine` を通す。締めた区間を
  優先しつつ区間側へ戻せる形にして、`Interval::new` の外側丸めで `iv.lo` が厳密に 0 の
  箱の符号が `None` に倒れる退行を避けた
- `probe_pair`: 矛盾判定を `decided_ok` の手前へ移した。`witness` の評価位置は変えて
  いないので、報告される残差は従来と同じ

決着力の退行は無い (`tests/law_corpus_oracle.rs` 9 本が green、真の核 5 件 × probe
3 半径はすべて `Proven` のまま)。この不整合は mutants の生存変異
`replace > with == in gap_exceeds` が指していた。

#### Lipschitz 包囲の算術を mutants の測定集合内で固定した (2026-10-01)

第 2 の証明経路を入れた commit は、その経路を叩く test を `tests/law_corpus_oracle.rs`
にしか持っていなかった。mutants は `--lib --test analytic_law --test law_tests` で
回すので、この file は測定集合の外にある。結果として CI の test job と local の
preflight は両方 green なのに、`Quality Deep` の mutants gate だけが生存変異 19 件で
落ちた (4 shard のうち 1/4 と 3/4)。

`law.rs` の `core_probe_tests` (= `--lib` に載る) に閉形式 test 5 本を足して、
`lipschitz_enclosure` の算術 6 箇所と `refine_with_lipschitz` の境界判定を固定した。

- 辺長 `(2, 2, 1)` の箱は半対角が `0.5·√(4+4+1) = 1.5` で f32 厳密になるので、
  `L = 2` / `f(c) = 10` の包囲 `[7, 13]` を厳密な期待値として使える
- 箱の 3 区間はすべて非対称に取る。対称区間だと `hi - lo` を `hi / lo` に変えても
  `-1` になり、二乗で符号が消えて辺長の二乗が変わらないため変異が見えない
- `if lo > hi` は 1 点交差と空交差の 2 本で挟む。`>=` は決着できる cell を未決定に
  落とし、`==` は空交差を矛盾として扱わず `lo > hi` の壊れた区間を下流に流すので、
  向きが逆の 2 つの誤りになる
- 境界を測る test の区間は `Interval::new` の外側丸めを避けて struct literal で組む

#### `laser_pattern::gcd_f64` が最後の商を落としていた (2026-09-30)

Euclid 互除法の更新順が誤っており、剰余が eps 未満になって `break` する時点で
除数の繰り上げを通らないため、1 手前の値を返していた。測った 8 ケースすべてで
誤りで、`gcd(10,5)` が 10、`gcd(10,7)` が 3、`gcd(100,35)` が 30 になる。

影響は `guilloche` (ハイポトロコイド) の描画範囲。周期 `2π·r/gcd(R,r)` が短く
出るため曲線が全体の 16.7〜50% しか描かれず、始点に戻らないまま途中で切れて
いた。既存 test は返る点数しか見ていなかったので green のままだった。

#### `ThermalField` の bracket が両方向とも間違った側に寄っていた (2026-09-29)

温度の bracket は対流熱伝達率 `h` を両端 (`h = 0` / `h = ∞`) で挟んでいたが、
**形状について同じことを考えていなかった**。区間演算で内外が決まらないセルが
あるので真の材料 `M` は `Inside ⊆ M ⊆ Inside ∪ Undecided` の範囲にあり、
両方の run に `Inside ∪ Undecided` (真より材料が多い側) を使っていた。

材料を増やした時に最高温度が動く向きは **境界条件によって逆**:

| 境界条件 | 材料を増やすと | 理由 |
|---|---|---|
| 断熱 (`h = 0`) | **下がる** | 質量が増えて同じ熱量を吸う |
| 等温 (`h = ∞`) | **上がる** | 周囲温度に固定される面が熱源から遠のく |

したがって旧実装では **上界は真より低く、下界は真より高い** — bracket が
両側から食われ、未定に落ちるべきものが **偽の合格 / 偽の違反**になっていた。
誤差は未確定セルの体積比に比例するので、薄い形状ほど大きい。

不等式を順に適用すると両端とも最小材料で極値を取る:

```
T(M, h_true) ≤ T_断熱(M) ≤ T_断熱(Inside)      (h / 材料 とも単調減少)
T(M, h_true) ≥ T_等温(M) ≥ T_等温(Inside)      (h は単調減少、材料は単調増加)
```

- **両方の run を `Inside` だけで解く**。`Inside ∪ Undecided` の run は不要
  (4 通り解いても min/max はこの 2 つに来る)。実装は単純になり、かつ
  `max` / `min` が `(M, h)` 全域で `M = Inside` に来るので **緩くもならない**。
- `Inside` に絞ると顕在化する経路を塞いだ:
  - 内部と確定したセルが無い → `ThermalGridUnusable`
  - **熱源が材料に乗らない** → 同上。0.4.0 は「形状の外の熱源は効かない」と
    黙って捨てており、熱が入らず上界も周囲温度になる **偽の合格**の経路だった
    (`Inside ∪ Undecided` で解いていたので踏みにくかっただけで、修正で新しく
    作った穴ではなく元からあった穴が見えるようになったもの)
  - **熱源のある材料が格子の都合で分断されている** → 同上。本当に分かれている
    部品と区別するため、`Inside` の連結成分と `Inside ∪ Undecided` の連結成分を
    比べる (後者で繋がっているのに前者で切れていれば格子の都合、閾値不要)
- `Evidence` の model 文字列を「境界条件 h と形状を両端で挟む: 上界 = 断熱 ×
  内部確定セルのみ、下界 = 等温 × 同」に更新。

#### 追加した oracle (`law::evidence_gate_tests`)

設計が材料に対する単調性 2 本に全面的に依存するので、**実装より先に**その
2 本を書いて実測した (落ちたら設計の前提が崩れるため)。

- `material_moves_the_peak_in_opposite_directions_per_boundary` — 断熱で
  材料を増やすと下がり、等温で上がること
- `bracket_contains_every_admissible_material_set` — `Undecided` セルを
  0 / 25 / 50 / 75 / 100 % 採用した材料集合をすべて解き、**法則が報告した**
  `[lo_c, hi_c]` の内側に入ること。形状 (半径) を変えるのではなく
  `Undecided` の部分集合を直接作るのが要点で、範囲を外れた形状を混ぜると
  「入らなくて当然」になり oracle が嘘の red を出す。また bracket は
  solver を直接呼ばず法則の報告値から読む (直接呼ぶと法則がどの形状を
  選んでいるかを見ないので、形状選択を戻しても落ちない test になる)
- `a_source_outside_the_material_is_not_silently_passed` (`tests/analytic_thermal.rs`)

破壊試験: `check_thermal_field` の形状を `Inside ∪ Undecided` に戻すと
`bracket_contains_every_admissible_material_set` **だけ**が落ちる (他 6 本は
green のまま = 落ちるべきものだけが落ちる)。落ち方は「bracket の中点なのに
合格になった」で、偽の合格そのものが再現する。

#### `lol.gbnf` が publish された crate に入らず、公開版が build 不能だった (2026-09-28)

`pub const LOL_GBNF` は `include_str!("../../lol.gbnf")` で **package の外**
(workspace root) を指していた。`include_str!` は compile 時に解決されるが、
`cargo package` は package directory の外の file を `.crate` に含めないので、
**crates.io から取得した crate は該当 file を持たず compile に失敗する**。

`LOL_GBNF` は feature gate の無い `pub const` なので、影響は llm-bridge 利用者
に限らず**全利用者**に及ぶ。実証: CI の `cargo-semver-checks` が baseline の
**publish 済 0.3.0** の rustdoc build に失敗していた

```
error: couldn't read `.../alice-lol-0.3.0/src/bridge/../../../lol.gbnf`:
       No such file or directory
```

- canonical を `alice-lol/lol.gbnf` に移動 (workspace root から package 内へ)。
  `include_str!` は `"../lol.gbnf"` になり、参照先が package に収まる
- 参照は 2 箇所のみ (`alice-lol/src/lib.rs` / `alice-lol/tests/lol_gbnf_test.rs`)。
  下流 (非公開の pipeline crate → text-to-print) は `alice_lol::LOL_GBNF` の re-export 経由
  なので影響なし。grammar の単一 source 運用 (copy 廃止、2026-09-14) は維持
- symlink は張っていない。Windows の CI job で git symlink の checkout が
  問題になり得るため、単純な移動で解決している

**注意:** この修正後も CI の `cargo-semver-checks` (informational) は失敗し続ける。
baseline に使われる **publish 済 0.3.0 が壊れたまま**だからで、0.4.0 が
publish されるまで解消しない。追いかけないこと。

- **alice-sdf 1.10.2 の `impl Drop for SdfNode` に test code を追従** (`7292552` / `2753894`) — `match node { SdfNode::Union { a, b } => .. }` / `let SdfNode::Polygon2D { .. } = expr else` の値 destructure 10 箇所が E0509 (Drop 型から field を move 不可) になっていたのを `match &node` + `&**a` に変更 (production code 変更なし)
- **fuzz.yml が `alice-stubs` の空 alice-sdf stub で lib を build しており 3 target とも導入以来 compile 不能だった** (`c6610e2`) — job-level `continue-on-error` で silent green になっていた ci.yml と同じ実 sibling checkout (ALICE-SDF / LLM / Kinematics) + inline stub に揃え、build step を blocking / run step のみ informational に分離
- **CI `Security & Hygiene` red (`536f339`)**: `alice-stubs` action の alice-llm stub が `features = default, grammar` しか宣言しておらず、`llm-bridge` が要求する `simd` / `parallel` で cargo-deny / semver-checks の resolve が失敗 stub の feature 宣言を実 crate に追従 (stub は実 crate の feature list を鏡写しにする、が原則)
- **`llm-bridge` feature の alice-llm 依存に `simd` + `parallel` を追加** 従来 `features = ["grammar"]` のみで forward が single-thread / SIMD なしになっており、bridge 経由 (text-to-print sidecar 等) は alice-llm 直接 example より数倍遅かった MiniCPM5-2B で 1 prompt (system prompt ~600 token) 58 → 24.5 s 残りは alice-llm の逐次 prefill (batch prefill 未実装、ALICE-LLM 側 issue)

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

- Real-model smoke run against Qwen 3.5-4B Q4_K_M (Metal, CPU
  hybrid, ~1 tok/s) validated the API end-to-end: prompt `"generate
  lol: sphere(1.5)"` produced `SdfNode::Sphere { radius: 1.5 }` in
  ~477 s.
  A shorter prompt without a primed example triggered
  `BridgeError::Parse` (grammar mask allowed the model to reach
  `with_material` before `max_new_tokens` capped it mid-parse) —
  exactly the two-stage safety net (`FSM mask` + `runtime_parser`)
  the design intended.
- Fine-tuned LOL emission is future work. The grammar mask
  guarantees the output is syntactically valid LOL; semantic quality
  (does the SDF match the prompt?) tracks the underlying model.
- Vulkan smoke run (Phase X.8 B-9-B) and a version bump to
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
