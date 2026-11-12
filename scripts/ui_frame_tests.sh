#!/usr/bin/env bash
# alice-lol-ui's frame law with feature `serde`: the lib tests of `frame` (the config test needs the
# feature) and tests/frame_cli.rs (the alice-ui-frame-check command on reports measured from a real
# page). Fails when a run reports fewer tests than expected, so a test that stops being compiled or
# selected cannot pass as 0 run.
set -euo pipefail
cd "$(dirname "$0")/.."
log=$(mktemp)
trap 'rm -f "$log"' EXIT
# names: frame lib tests (16 with serde) and the command tests (5)
cargo test --locked -p alice-lol-ui --features serde --lib frame:: 2>&1 | tee "$log"
lib=$(grep -E '^test result: ok\. [0-9]+ passed' "$log" | sed -E 's/^test result: ok\. ([0-9]+) passed.*/\1/' | awk '{s+=$1} END{print s+0}')
cargo test --locked -p alice-lol-ui --features serde --test frame_cli 2>&1 | tee "$log"
cli=$(grep -E '^test result: ok\. [0-9]+ passed' "$log" | sed -E 's/^test result: ok\. ([0-9]+) passed.*/\1/' | awk '{s+=$1} END{print s+0}')
echo "frame law tests run: lib $lib (want >= 16), command $cli (want >= 5)"
if [ "$lib" -lt 16 ] || [ "$cli" -lt 5 ]; then
  echo "error: fewer frame law tests ran than expected" >&2
  exit 1
fi
