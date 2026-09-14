# alice-lol

[![crates.io](https://img.shields.io/crates/v/alice-lol.svg)](https://crates.io/crates/alice-lol)
[![docs.rs](https://img.shields.io/docsrs/alice-lol)](https://docs.rs/alice-lol)
[![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/alice-lol.svg)](#license)
[![CI](https://github.com/ext-sakamoro/ALICE-LOL/actions/workflows/ci.yml/badge.svg)](https://github.com/ext-sakamoro/ALICE-LOL/actions/workflows/ci.yml)

**Law-Oriented Language — SDF DSL as a Rust `proc_macro`.**

Declare SDF scenes at compile time; `alice-lol` transpiles them to `SdfNode` and emits GLSL / WGSL / HLSL, plus 3D-print export (STL / 3MF / OBJ / FBX), a law checker, a laser pattern generator, and an optional LLM Guided Generation bridge.

## Quick Start

```bash
cargo add alice-lol
```

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

- **123 DSL constructs** — 71 primitives, 23 CSG ops, 4 transforms, 20 modifiers, 3 print structural intents, 2 time controls, 3 law constraints
- **3 shader targets** — GLSL (default), WGSL, HLSL (hardcoded / dynamic modes)
- **Spatial pruning compiler** — interval arithmetic prunes unevaluated regions, up to ~10× speedup on IFS fractals
- **Law constraint checker** — `NonOverlap`, `Containment`, `MinThickness` with hard / soft priority and spatial violation reports
- **Variable capture** — `{rust_expr}` or bare identifiers inject Rust values into the DSL
- **Autodiff** — gradient, mean / Gaussian / principal curvature, Hessian
- **CompiledSdf** — SIMD 8-wide batch evaluation, BVH spatial index, Rayon parallelism
- **3D-print export** — STL / 3MF / OBJ / FBX via the `print_export` module
- **Roblox export** (`roblox` feature) — MeshPart / UGC accessory OBJ / FBX with triangle-count validation
- **Physics bridge** (`physics` feature, optional) — ALICE-Physics integration for material-backed stress checks
- **LLM Guided Generation** (`llm-bridge` feature, optional) — GBNF grammar-constrained decoding

## Feature flags

| Flag | Default | Description |
|------|:-:|-|
| `glsl` | on | GLSL shader transpilation |
| `wgsl` | off | WGSL transpilation (pulls in `alice-sdf/gpu`) |
| `hlsl` | off | HLSL transpilation |
| `physics` | off | ALICE-Physics bridge (⚠ AGPL propagation, see below) |
| `roblox` | off | Roblox MeshPart / UGC accessory export |
| `llm-bridge` | off | GBNF grammar-constrained decoding bridge (⚠ AGPL propagation) |

### License note on optional features

`physics` and `llm-bridge` pull in `alice-physics` / `alice-llm`, both licensed **AGPL-3.0-or-later**. Enabling either propagates AGPL terms to downstream consumers. Leave them off (the default) to stay on **MIT OR Apache-2.0** terms.

## Companion crates

- [`alice-lol-macro`](https://crates.io/crates/alice-lol-macro) — proc-macro backend (consumed transitively via `alice-lol`, not directly)
- [`alice-sdf`](https://crates.io/crates/alice-sdf) — SDF runtime `alice-lol` transpiles to
- [`alice-physics`](https://crates.io/crates/alice-physics) — physics engine (optional, `physics` feature)
- [`alice-llm`](https://crates.io/crates/alice-llm) — LLM inference (optional, `llm-bridge` feature)

## License

Licensed under either of **MIT** or **Apache-2.0** at your option. See `LICENSE-MIT` and `LICENSE-APACHE` in the [workspace root](https://github.com/ext-sakamoro/ALICE-LOL).

Enabling the `physics` or `llm-bridge` features pulls in AGPL-3.0-or-later dependencies; those terms then propagate to downstream. Keep those features off to stay on MIT / Apache-2.0.

## Links

- Repository: <https://github.com/ext-sakamoro/ALICE-LOL>
- Documentation: <https://docs.rs/alice-lol>
- Workspace overview: [repo root `README.md`](https://github.com/ext-sakamoro/ALICE-LOL/blob/main/README.md)
