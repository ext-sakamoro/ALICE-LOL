# LLM → LOL roadmap — making models speak LOL

Status board for the "LOL as an interlingua" work: the entrance that lets a language model (or a latent world model) emit LOL, and the measurement that tells us whether it actually learned the language. Last updated 2026-09-14.

Why this exists: LOL is a *law language* (Phase 2/3 of the ALICE Data → Law → Intent principle). For it to be a base that other models build on, models must be able to produce it. Two entrances are planned — text emission from an LLM (Track A), and decoding from a latent state (Track B) — with a shared synthetic-data pipeline (Track C) because LOL is executable and therefore its own labeller.

## Status

| Track | Step | Status | Where |
|---|---|---|---|
| A0 | Phase 3 Intent text syntax: `program(<sdf>, entities(...), <intent>)`, 18 verbs, `parse_program`, `IntentNode::to_lol` | ✅ 2026-09-14 | `alice-lol/src/runtime_parser.rs`, `lol.gbnf` |
| A1 | Fast constrained-decoding loop | ✅ 2026-09-14 | alice-llm B-10 token-trie mask (`grammar::TokenTrie`): 8 s → ~1 ms per step on a 130k vocab; the bottleneck was the per-token FSM probe, not the GPU path |
| — | Grammar tightened for the LLM path: no `//` comments, at most one whitespace char between tokens (still a strict subset of what the runtime parser accepts) | ✅ | `lol.gbnf` |
| — | Grammar is the single source: `alice_lol::LOL_GBNF` (feature-free) → re-exported by alice-bamboo → consumed by text-to-print; parser ⊆ grammar drift guard in `tests/lol_gbnf_test.rs` | ✅ | 103 product / mechanical / fastener constructs recovered into the canonical grammar |
| A2 | Think-first generation: free `<think>…</think>` prefix, then grammar (alice-llm B-11 `generate_grammar_prefixed`, `bridge::generate_program_thinking`) | ✅ | MiniCPM5-2B produces the full mug (`union(cylinder(25,50), translate(25,0,50, rotate(0,90,0, torus(12,4))))`) only with the think prefix |
| A2 | Quality benchmark `examples/llm_bench.rs`: 20 prompts (T1 primitives / T2 compositions / T3 transforms / T4 Intent), judged by SDF-eval oracle points and Intent structure | ✅ | **baseline MiniCPM5-2B Q4_K_M: grammar-only 7/20, think→grammar 9/20** (parse 40/40) |
| C0 | `emit::to_lol`: `SdfNode` → LOL text, 128 variants exhaustive, canonical form, eval-parity round trip over all 237 grammar constructs | ✅ | `alice-lol/src/emit.rs`, `tests/emit_roundtrip.rs` |
| C1 | `alice-lol-datagen`: 8 template families emit LOL + EN/JA captions + oracle points from the same parameters, self-checked; ≈6,500 samples/s | ✅ | `alice-lol-datagen/` |
| **A3** | **SFT of a small model on datagen pairs, re-run `llm_bench` without think** | ⏳ next (the part that actually changes the model) | pass line: ≥ 15/20 without think, T2/T3 ≥ 3/5 |
| A4 | RL with verifiable reward (parse + oracle match) | planned | |
| B0–B4 | Latent side: physical RSSM, latent → Intent head, latent → LOL decoder, projector into the LLM, LOL → latent encoder | planned | the current RSSM in Project-ALICE is trained on coding-agent transitions, not physics |

## Failure taxonomy from the baseline (what the training data targets)

1. Dropped `translate` — objects placed at the origin regardless of the requested position.
2. Half-extent confusion — full height passed as `cylinder` half-height, full sizes passed to `box3d`.
3. Dropped composition — mug without handle, table without legs, arch without the subtract, plate without holes.
4. Intent structure — `program(...)` wrapped around plain geometry, `entities()` omitted, `par` written as `seq`.

The think prefix fixes (1) and (2) on single primitives but not (3); the model's problem is vocabulary and argument conventions, which is what SFT is for.

## Evaluation caveat for A3

The datagen families were designed from the benchmark's failure types, so the 20 `llm_bench` prompts share phrasing and objects with the training data. `llm_bench` therefore measures in-distribution learning. A separate held-out set (paraphrased captions, objects absent from both — lamp, chair, two-handled cup, stairs) is required to claim generalisation; build it together with the SFT run.

## Evidence

- `~/claude-config/evidence_b10_trie_mask/` — naive vs trie mask timings, think vs grammar-only mug runs
- `~/claude-config/evidence_lol_bench_2026_09_14/` — full benchmark log + JSONL
- `~/claude-config/evidence_lol_datagen_2026_09_14/` — 400-sample JSONL + run log

(Evidence lives outside the repo; the numbers above are copied from those logs.)
