# alice-lol-macro

[![crates.io](https://img.shields.io/crates/v/alice-lol-macro.svg)](https://crates.io/crates/alice-lol-macro)
[![docs.rs](https://img.shields.io/docsrs/alice-lol-macro)](https://docs.rs/alice-lol-macro)
[![License: MIT OR Apache-2.0](https://img.shields.io/crates/l/alice-lol-macro.svg)](#license)

**Proc-macro backend for [`alice-lol`](https://crates.io/crates/alice-lol) — parses the Law-Oriented Language DSL and emits `SdfNode` construction code.**

## This crate is normally not used directly

Depend on [`alice-lol`](https://crates.io/crates/alice-lol) instead; it re-exports the `lol!` macro alongside the SDF runtime.

```bash
cargo add alice-lol
```

```rust
use alice_lol::lol;

let scene = lol! {
    field MyScene {
        sphere(1.0)
    }
};
```

## What this crate provides

- The `lol!` procedural macro (parses the LOL DSL grammar via `syn`)
- Compile-time DSL → `SdfNode` code generation
- Variable capture (`{rust_expr}` and bare identifiers)

## License

Licensed under either of **MIT** or **Apache-2.0** at your option.

## Links

- Runtime crate: [`alice-lol`](https://crates.io/crates/alice-lol)
- Repository: <https://github.com/ext-sakamoro/ALICE-LOL>
- Documentation: <https://docs.rs/alice-lol-macro>
