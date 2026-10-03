# alice-world-auditor

Deterministic planner for `alice-physics` worlds — searches for a
frame-minimal action sequence toward a
[`Goal`](../alice-world-auditor-types), returning a 3-valued
(`Proven` / `Violated` / `Undecided`) [`Verdict`](../alice-world-auditor-types).

**Status: Phase 4 (search implemented, 2026-10-03).** `lower_bound_frames`
and `plan` search an exact-integer 1-D bang-bang lattice via IDA\*
(`k = 1` frame-granularity actions) — see
`project_alice_world_model_phase4_design_confirmed`
(`~/claude-config/memory/`) for the design. All three Phase 3 oracle
scenes in `tests/` are green with `#[ignore]` removed. MVP scope: single
rigid body, goal axis fixed to `x`.

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
