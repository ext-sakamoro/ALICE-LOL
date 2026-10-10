# Job time limits (`timeout-minutes`)

Every job of every workflow has a job-level `timeout-minutes`
(`scripts/workflow_timeouts.py` fails CI otherwise). This page records where each value
comes from.

## Why

A job can stop making progress in a step that is not ours: on 2026-10-10 a test job
finished its work in 16 minutes and then stayed in the post step of `actions/checkout`
for more than 27 minutes. Without a job-level limit such a job runs until GitHub's
6-hour limit and holds a runner for that time. A step-level limit does not cover post
steps.

Not yet observed: whether the job-level limit ends this particular kind of stall. The
stalled job did not stop on a normal cancel request and needed the force-cancel API
(`POST /repos/{owner}/{repo}/actions/runs/{run_id}/force-cancel`). If a job ever passes
its limit without ending, cancel it with that API.

## How the values were chosen

Rule: about 3 times the longest successful duration seen, rounded up, so that a build
without a warm cache still fits. Durations are from `gh run view --json jobs`
(`completedAt - startedAt`) over the most recent successful runs on 2026-10-10.

| workflow | job | runs | longest (min) | limit (min) |
|---|---|---|---|---|
| ci.yml | Test (5 entries) | 15 | 15.9 | 45 |
| ci.yml | Clippy | 15 | 0.8 | 30 |
| ci.yml | MSRV | 15 | 1.0 | 30 |
| ci.yml | GPU parity (lavapipe) | 15 | 1.4 | 30 |
| ci.yml | Format | 15 | 0.5 | 15 |
| ci.yml | Docs gates (3 OS) | 15 | 0.9 | 15 |
| ci.yml | Wiring guard (3 OS) | 15 | 0.6 | 15 |
| ci.yml | actionlint | 15 | 0.2 | 10 |
| fuzz.yml | Fuzz (4 targets) | 7 | 3.4 | 20 |
| security-audit.yml | cargo audit | 7 | 0.6 | 15 |
| security-audit.yml | cargo-deny | 7 | 0.5 | 15 |
| security-audit.yml | cargo-machete | 7 | 0.5 | 15 |
| security-audit.yml | stub / mock guard | 7 | 0.7 | 10 |
| security-audit.yml | coverage | 7 | 0.9 | 45 |
| security-audit.yml | semver-checks | 0 | (no successful run in the window) | 30 |
| release.yml | detect-crate / dry-run-publish / create-release | 0 | (runs only on a release) | 10 / 30 / 15 |
| quality-deep.yml | mutation shards | 3 | 18.1 | 120 (unchanged) |

The short durations of Clippy, MSRV and coverage come from a warm cache; their limits
are set for a cold build (a full compile of the workspace), not from the table.

Follow-up (not done): a check that each limit is at least twice the observed p95, read
from the run history, so that a limit set too tight is found before it turns a green
run red.
