# alice-world-auditor

English | [日本語](README.ja.md)

Deterministic planner for `alice-physics` worlds — searches for a
frame-minimal action sequence toward a
[`Goal`](../alice-world-auditor-types), returning a 3-valued
(`Proven` / `Violated` / `Undecided`) [`Verdict`](../alice-world-auditor-types).

**Status: Phase 5 (3-valued verdict, 2026-10-04).** `audit` searches an
exact-integer 1-D bang-bang lattice with IDA\* (`k = 1`, one action per
frame), then replays the plan in a fresh `alice-physics` world built from
the caller's `PhysicsConfig` and returns `Proven` / `Violated` /
`Undecided`. MVP scope: single rigid body, goal axis fixed to `x`.

## What this crate claims

> **k 粒度の macro-action 空間で、整数格子上で厳密に最適**
> (exactly optimal on the integer lattice, over the k-granular
> macro-action space; here `k = 1`)

- The granularity is part of the claim: the frame count is minimal on the
  integer lattice model, not over every engine trajectory. "Optimal" is
  never claimed on its own.
- The tolerance ε of the engine's continuum solvers stays inside the
  solver; it does not enter the search layer.
- When the node budget runs out, the result is `Undecided`
  (`BudgetExhausted`, carrying `BestSoFar`) and optimality is not claimed.

## Verdicts

| Verdict | Returned when | Evidence / reason |
|---|---|---|
| `Proven` | The plan, replayed under the given `PhysicsConfig`, leaves the body inside the target (all 3 axes) with linear velocity exactly `0` in `Fix128`, and the engine's sticky overflow flag is not set | The action sequence (`Optimal`) |
| `Undecided` | Budget exhausted / replay misses the goal / overflow during replay / invalid input / beyond `MAX_AUDIT_FRAMES` / no actuator but the body may drift | `UndecidedCause` |
| `Violated` | Only (1) an empty target, or (2) `a_max_axis == 0` with the body at rest, zero gravity, outside the target | `Violation` |

**Sufficient conditions for `Proven`** on a reachable target:
`a_max_axis * dt` and `dt` are dyadic (exact in `Fix128`, e.g. `a = 4`,
`dt = 1/64`), damping is `1`, and gravity along the goal axis is zero.
With `dt = 1/60` or the default damping `0.99`, the lattice plan does not
replay exactly and the answer is `Undecided` (`ReplayMismatch`), not a
`Proven` the engine does not back. `plan` (the lattice search that drives
the caller's world) is unchanged; its `Ok(Optimal)` is a lattice claim and
is not checked against the engine.

**Non-goal:** proving that a goal is unreachable from the law in general
(obstacles, region constraints). In 1-D with `a_max_axis > 0` every target
is reachable in finite time, so `Violated` never comes from search.

## Features

| Feature | Default | Pulls in | License note |
|---|---|---|---|
| (none) | — | `alice-world-auditor-types`, `glam` | — |
| `physics` | off | `alice-physics` | `alice-physics` carries the same `AGPL-3.0-or-later OR LicenseRef-Commercial` dual license as this crate — enabling it does not change your license obligations beyond what using this crate already implies |

## License

`AGPL-3.0-or-later OR LicenseRef-Commercial` — dual-licensed. Pick either.

| Option | Terms | Use it when |
|--------|-------|-------------|
| **AGPL-3.0-or-later** | [LICENSE-AGPL](LICENSE-AGPL) — free, no reporting obligation | Your project is itself AGPL-compatible open source, or you are only using it internally |
| **Commercial License** | [LICENSE-COMMERCIAL.md](LICENSE-COMMERCIAL.md) — paid, removes the copyleft | Closed-source product, proprietary SaaS, edge / firmware distribution, or a platform NDA that forbids source disclosure |

`alice-world-auditor-types` (the vocabulary-only sibling crate) is
`MIT OR Apache-2.0` and carries no copyleft obligation on its own.
