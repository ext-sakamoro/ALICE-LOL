---
name: alice-lol-sdf
description: Generate 3D geometry via ALICE-LOL DSL (Law-Oriented Language) — a Rust proc_macro + runtime DSL with 124 constructs (71 primitives, 23 CSG ops, 4 transforms, 20 modifiers, 3 print intents, 2 time, 3 laws) that compiles to Signed Distance Function trees. Ships with GBNF grammar for LLM constrained decoding (syntax-error-free output guaranteed), 3D print export (STL/3MF), Bambu H2D laser (.lac) generation, Roblox accessory export (OBJ/FBX), law checker (NonOverlap / Containment / MinThickness), and 7-mode SDF evaluation. Prefer this skill for LLM-driven text→3D authoring, 3D print automation, and any workflow where LLM output correctness matters.
---

# ALICE-LOL — Law-Oriented Language for 3D

Provenance: maintained in [ext-sakamoro/ALICE-LOL](https://github.com/ext-sakamoro/ALICE-LOL). This skill wraps the parent `alice-lol` crate. Install with `cargo add alice-lol` or clone the repo.

## Purpose

Write concise DSL for 3D geometry that compiles at build-time (via `proc_macro`) or parses at runtime (via `alice_lol::runtime_parser::parse_lol`) into a Signed Distance Function tree. The DSL is designed for both human authors and LLMs — the accompanying GBNF grammar (`references/lol.gbnf`) enables **constrained decoding** so a well-behaved LLM inference stack (llama.cpp, vLLM, TGI) cannot emit syntactically invalid LOL.

```
smooth_union(0.3, sphere(1.0), box3d(0.5, 0.5, 0.5))
```

That single line compiles to a valid `SdfNode`, evaluable on CPU (SIMD 8-wide), GPU (wgpu), or transpiled to GLSL / WGSL / HLSL for engine integration.

## When to use this skill

Use this skill when the task calls for:

- **LLM-driven text→3D** with correctness-critical output — the GBNF grammar rejects invalid syntax at inference time, eliminating a whole class of LLM failure modes
- **3D print automation** — `print_export` emits FDM / SLA / SLS-tuned STL / 3MF with automatic mesh repair (`MeshRepair::repair_all`)
- **Bambu Lab H2D laser** — `.lac` project generation from LOL 2D DSL (see companion `alice-metal-card` for SVG → `.lac` implementation)
- **Roblox UGC** — `roblox_export` emits OBJ / FBX with triangle-count validation for UGC accessory (4000 tri) or generic MeshPart (10000 tri)
- **Law-constrained generation** — declare `NonOverlap`, `Containment`, `MinThickness` constraints and get a residual report with spatial coordinates
- **Variable capture** — `{rust_expr}` inject Rust values into DSL at compile time (proc_macro path only, not runtime LLM path)

Do **not** use this skill for:

- **Low-level GLSL / WGSL / HLSL authoring** where you need custom shader logic — use `alice-implicit-cad` skill or write raw shader
- **STEP-first parametric CAD** with mating / assembly — use `earthtojake/text-to-cad` `cad` skill
- **Non-SDF mesh authoring** (Blender / OpenSCAD style CSG on BREP) — SDF is a different mathematical model

## Core value: GBNF constrained decoding

`references/lol.gbnf` is a **GBNF (Guided Backus-Naur Form)** grammar that covers all 124 runtime-parseable constructs. When loaded into a supported inference stack:

- **llama.cpp**: `--grammar-file references/lol.gbnf`
- **vLLM / TGI**: pass grammar via OpenAI-compatible `response_format: {"type": "grammar", "grammar": "..."}` API
- **Guidance / Outlines / LMQL**: convert GBNF to native format

The LLM literally cannot emit tokens that violate LOL syntax. Combined with the system prompt in `references/print-guide.md`, the LLM produces production-ready 3D print DSL directly.

**Contrast with alternative approaches**:
- Free-form Python (build123d, CadQuery, OpenSCAD): syntax valid but semantic errors common (`import` missing, wrong API, unhandled edge cases)
- Free-form JSON tree: LLM often emits invalid JSON or wrong schema
- LOL + GBNF: syntax invariant is enforced by the sampler; only semantic content is LLM responsibility

## Available constructs (124 total)

See `references/syntax.md` for full argument tables. Summary:

| Category | Count | Examples |
|--|--|--|
| Primitives | 71 | `sphere`, `box3d`, `torus`, `gyroid`, `heart`, `helix`, TPMS surfaces |
| CSG Operations | 23 | `union`, `smooth_union`, `subtract`, `chamfer_union`, `stairs_union` |
| Transforms | 4 | `translate`, `rotate`, `scale`, `scale_non_uniform` |
| Modifiers | 20 | `round`, `onion`, `twist`, `bend`, `mirror`, `repeat`, `noise`, `shell` |
| 3D Print structural intent | 3 | `lattice_infill`, `diamond_infill`, `schwarz_infill` |
| Time control | 2 | `animate`, `morph` |
| Laws | 3 | `NonOverlap`, `Containment`, `MinThickness` |

## Workflow

1. **Read the modeling brief** (dimensions, coordinate frame, target output: 3D print / laser / Roblox / shader).
2. **Load the appropriate system prompt** into the LLM:
   - General 3D: `references/syntax.md`
   - 3D print with material-aware defaults: `references/print-guide.md`
3. **Constrained decoding** (recommended): pass `references/lol.gbnf` to the inference stack. Fallback: free-form generation with post-parse validation.
4. **Parse & validate**: `scripts/parse.sh <output.lol>` runs `alice_lol::runtime_parser::parse_lol` and reports parse errors with line/column.
5. **Generate artifact**:
   - `scripts/print.sh <input.lol> --format stl` for FDM print
   - `scripts/print.sh <input.lol> --format 3mf` for Bambu Studio import
   - `scripts/laser.sh <input.lol> --output part.lac` for Bambu Suite H2D laser (2D DSL only)
   - `scripts/roblox.sh <input.lol> --format obj|fbx --preset accessory|meshpart`
6. **Verify**:
   - Print: import into Bambu Studio / PrusaSlicer / OrcaSlicer and confirm slice succeeds
   - Laser: open `.lac` in Bambu Suite and check path preview
   - Roblox: import OBJ / FBX into Studio and check triangle count against preset limit

## Scripts (current status)

⚠️ **All four scripts below are STUBS pending upstream CLI additions.** The `alice-lol` crate exposes its functionality as library APIs (`alice_lol::runtime_parser::parse_lol`, `alice_lol::print_export::{lol_to_stl, node_to_3mf}`, `alice_lol::roblox_export::*`) but does not yet ship a dedicated CLI binary that accepts arbitrary LOL text files. The scripts run `--help` cleanly and document the intended interface, but exit with status 4 and a note when actually invoked. This makes the pending state explicit rather than pretending to work.

From this skill directory:

```bash
scripts/parse.sh <input.lol>                                   # syntax validation (STUB — use library API)
scripts/print.sh <input.lol> --format {stl|3mf} [--output <path>]  # STUB — parent crate example is hardcoded
scripts/laser.sh <input.lol> --output <path.lac>               # STUB — requires alice-metal-card companion
scripts/roblox.sh <input.lol> --format {obj|fbx} [--preset accessory|meshpart]  # STUB — same pending CLI
```

**Working paths available today** (Rust API, not shell CLI):

```rust
// Parse
let node = alice_lol::runtime_parser::parse_lol(&lol_text)?;

// Print export (STL / 3MF)
alice_lol::print_export::lol_to_stl(&lol_text, "out.stl", &PrintConfig::default())?;
alice_lol::print_export::node_to_3mf(&node, "out.3mf", &PrintConfig::high_quality())?;

// Roblox (with --features roblox)
alice_lol::roblox_export::lol_to_obj_roblox(&lol_text, "out.obj", &RobloxConfig::accessory())?;
```

For LLM-oriented workflows the shell CLI gap is less critical because the primary value is the **GBNF grammar** (`references/lol.gbnf`) — that plugs into the inference stack directly (llama.cpp `--grammar-file`, vLLM grammar param) and enforces syntax at token-sampling time. The parse validation step becomes redundant when constrained decoding is in play.

**Roadmap** (would unblock the scripts):
- Add `alice-lol/src/bin/lol.rs` with subcommands `parse`, `print`, `roblox` accepting `--input <path>` — the API surface exists; this is packaging work.
- The `laser.sh` script additionally depends on the `alice-metal-card` companion crate at `~/Project-ALICE/alice-metal-card` for `.lac` generation from SVG.

Use `scripts/<name>.sh --help` for the full documented interface.

## Law checker

The `Law` API (`NonOverlap`, `Containment`, `MinThickness`) lets the author declare geometric constraints and get a spatial residual report. This is critical for 3D print (min wall thickness for FDM), assembly (parts must not intersect), and packaging (contents must fit in container).

```
LawSet::new()
    .with_law(NonOverlap { a, b, priority: Hard })
    .with_law(MinThickness { child: wall, min_mm: 1.2 })
    .check(&scene) → Report { violations: [...], residuals: [...] }
```

The runtime parser does not currently support Law declarations (Law is a proc_macro-only construct); LLM output emits geometry only, and the caller applies laws in Rust code before mesh export.

## Print output rules (safety)

From parent crate `~/ALICE-LOL/CLAUDE.md`:

1. **`subtract` must be nested** (sequential carving). Do **not** `union` the cutters — non-manifold edges will result.
2. **`intersection` + TPMS (gyroid / schwarz / etc.)** is unsafe for mesh export. Use `lattice_infill` / `diamond_infill` / `schwarz_infill` instead — they guarantee full containment.
3. **Resolution ceiling** ≤ 192 (roughly 1M triangles). Above that, the mesh becomes unwieldy for slicers.
4. **`print_export` applies `MeshRepair::repair_all(epsilon=1e-3)` automatically** — do not disable unless you know why.

## Handoff

After generating a print / laser / Roblox artifact, always report the file path. If a downstream viewer skill or slicer skill (`$cad-viewer`, `$bambu-labs`, `$gcode`) is available, hand off the path.

## Related skills

- **`alice-implicit-cad`** (companion, ALICE-SDF side) — lower-level SDF authoring with 126 constructs and direct GLSL / WGSL / HLSL emit. Prefer for engine-integration workflows and shader-native output.
- **`earthtojake/text-to-cad` `cad`** — STEP-first parametric CAD (build123d Python). Use for mechanical CAD with mating / assembly semantics; ALICE-LOL does not model BREP.
- **`earthtojake/text-to-cad` `bambu-labs`** — post-slice print job dispatch. Feed LOL-generated `.3mf` to that skill for actual printer control.
- **`karikari-review`** (Rust push-time gate) — apply when editing the parent `alice-lol` crate itself.

## References

- `references/syntax.md` — 124-construct reference (mirrors parent `LLM_REFERENCE.md`)
- `references/print-guide.md` — 3D print system prompt (mirrors parent `LLM_PRINT_PROMPT.md`)
- `references/lol.gbnf` — GBNF grammar for constrained decoding
- Parent docs: `~/ALICE-LOL/README.md`, `~/ALICE-LOL/LLM_REFERENCE.md`, `~/ALICE-LOL/LLM_PRINT_PROMPT.md`, `~/ALICE-LOL/SPEC.md`
