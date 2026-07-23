# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
