# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- **`lol.gbnf` の whitespace を token 間 1 文字 (`ws ::= [ \t\r\n]?`) に制限 + `//` line comment を除外** (LLM 経路 grammar を runtime lexer より厳格化、grammar ⊂ parser) 無限 `ws` も rambling channel で、comment 除去後に MiniCPM5-2B が `\t` × 41 を連発した (evidence `03_*.log`) ため 1 文字に制限、以後は `cylinder(50,100)` で正しく final → EOS (evidence `05_*.log`、mask max 14 ms/step) comment 状態は `noteol*` でほぼ全 char を受理するため alice-llm B-10 token-trie mask が vocab 全走査に退化 (MiniCPM5-2B 実測 0.5-1 s/step) し、model も comment に思考を書き続けて本体を出さない (`// mug body` `// torus handle` … 48 token で本体ゼロ) 除外後は同 prompt で mask avg 5 ms/step、forward 律速 (evidence `~/claude-config/evidence_b10_trie_mask/03_*.log`) `parse_lol` / `parse_program` は comment を従来通り受理 (人間の `.lol` file は無傷) golden test `accepts_line_comments` → `rejects_line_comments`、`accepts_whitespace_and_newlines` → `accepts_single_whitespace_rejects_runs`、`sword.lol` golden は whitespace collapse して照合、`skills/lol-sdf/references/lol.gbnf` 同期、`LLM_REFERENCE.md`: Syntax Rules に rule 7 (comment / indent 禁止) 追加、LOL code block 7 箇所の `//` comment を prose に移動、LOL block 16 箇所の indent 除去 (model に grammar が弾く形を教えない、数式 / Rust block は対象外)

### Added
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
