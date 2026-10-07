# alice-world-auditor

[English](README.md) | 日本語

`alice-physics` の world を対象にした決定論的な planner 剛体 1 体を
[`Goal`](../alice-world-auditor-types) へ運ぶ frame 数最小の行動列を探索し、結果を
3 値 (`Proven` / `Violated` / `Undecided`) の
[`Verdict`](../alice-world-auditor-types) で返す

**Status: Phase 5 (3 値判定、2026-10-04)** `audit` は 1-D bang-bang の厳密整数格子を
IDA\* (`k = 1`、1 frame に 1 行動) で探索し、得た行動列を呼び出し側の `PhysicsConfig`
から作った新しい `alice-physics` world で再生して `Proven` / `Violated` / `Undecided`
を返す MVP の範囲は剛体 1 体、goal 軸は `x` 固定

## 名乗る主張

> **k 粒度の macro-action 空間で、整数格子上で厳密に最適**
> (本 crate は `k = 1`)

- 粒度は主張の一部 frame 数が最小なのは整数格子モデル上であって、engine の全軌道に対してではない
  「最適」を単独では名乗らない
- ε は engine の連続体 solver 内部の許容差であって、探索層には掛からない
- node 予算を使い切った時は `Undecided` (`BudgetExhausted`、`BestSoFar` を保持) を返し、最適性を名乗らない

## 判定

| Verdict | 返る条件 | 証跡 / 理由 |
|---|---|---|
| `Proven` | 行動列を指定の `PhysicsConfig` で再生した結果、body が target 内 (3 軸) にあり、線速度が `Fix128` で厳密に `0`、かつ engine の sticky overflow flag が立っていない | 行動列 (`Optimal`) |
| `Undecided` | 予算切れ / 再生結果が goal を満たさない / 再生中の overflow / 入力不正 / `MAX_AUDIT_FRAMES` 超 / 入力なしだが body が動きうる | `UndecidedCause` |
| `Violated` | (1) target が空、(2) `a_max_axis == 0` かつ body が静止・重力 0・target 外、の 2 件のみ | `Violation` |

**`Proven` になる十分条件** (到達可能な target の場合): `a_max_axis * dt` と `dt` が dyadic
(`Fix128` で厳密に表せる、例 `a = 4`、`dt = 1/64`)、damping が `1`、重力が 0、goal 軸以外の
初速度が 0 これは十分条件で必要条件ではない `Proven` の根拠は再生結果なので、`dt = 1/60` の
行動列でも再生結果が goal を厳密に満たせば `Proven` になる 再生結果が goal を外れる場合
(既定の damping `0.99`、dyadic でない位置の丸めが端を越える場合) は `Undecided`
(`ReplayMismatch`) になる engine が裏付けない `Proven` は返さない `plan`
(格子探索の後に呼び出し側の world を駆動する関数) の挙動は変えていない その `Ok(Optimal)`
は格子上の主張で、engine 上の成立は検査していない

**非目標**: 法則から到達不能を一般に証明すること (障害物 / 領域制約) 1-D で
`a_max_axis > 0` なら任意の target に有限時間で届くので、`Violated` が探索から出ることはない

## Features

| Feature | Default | 依存 | License |
|---|---|---|---|
| (なし) | — | `alice-world-auditor-types`, `glam` | — |
| `physics` | off | `alice-physics` | `alice-physics` は本 crate と同じ `AGPL-3.0-or-later OR LicenseRef-Commercial` |

## License

`AGPL-3.0-or-later OR LicenseRef-Commercial` のデュアルライセンス 詳細は [README.md](README.md) を参照
