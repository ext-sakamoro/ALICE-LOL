# alice-world-auditor

Deterministic planner for `alice-physics` worlds — searches for a
frame-minimal action sequence toward a
[`Goal`](../alice-world-auditor-types), returning a 3-valued
(`Proven` / `Violated` / `Undecided`) [`Verdict`](../alice-world-auditor-types).

⚠️ **Status: Phase 2 (scaffold).** `plan` and `lower_bound_frames` are
public entry points with `todo!()` bodies — the IDA\* search itself
(Phase 4) is not implemented yet. The oracle tests in `tests/` are
`#[ignore = "src gap: ..."]` for exactly that reason: they pin the
expected closed-form answers now, ahead of the implementation.

## Features

| Feature | Default | Pulls in | License note |
|---|---|---|---|
| (none) | — | `alice-world-auditor-types`, `alice-sdf`, `glam` | — |
| `physics` | off | `alice-physics` | `alice-physics` carries the same `AGPL-3.0-or-later OR LicenseRef-Commercial` dual license as this crate — enabling it does not change your license obligations beyond what using this crate already implies |

## License

`AGPL-3.0-or-later OR LicenseRef-Commercial` — dual-licensed. Pick either.

| Option | Terms | Use it when |
|--------|-------|-------------|
| **AGPL-3.0-or-later** | [LICENSE-AGPL](LICENSE-AGPL) — free, no reporting obligation | Your project is itself AGPL-compatible open source, or you are only using it internally |
| **Commercial License** | [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md) — paid, removes the copyleft | Closed-source product, proprietary SaaS, edge / firmware distribution, or a platform NDA that forbids source disclosure |

`alice-world-auditor-types` (the vocabulary-only sibling crate) is
`MIT OR Apache-2.0` and carries no copyleft obligation on its own.
