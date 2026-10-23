#!/usr/bin/env bash
# The frame oracle (alice-lol/tests/motion_frame_oracle.rs) on the targets the native legs of
# the geometry-digest job do not cover: wasm32-wasip1 under wasmtime, and the iOS simulator.
# Each run compares against the same recorded frame hashes, so a target that draws different
# bits goes red; each must report all of the oracle's tests passed (a run that executed fewer,
# or none, fails).
#
# usage: scripts/frame_targets.sh [wasm|ios|all]   (default: all)
# needs: the rust targets wasm32-wasip1 / aarch64-apple-ios-sim, wasmtime on PATH, Xcode's
# simctl with an available iPhone simulator (macOS only). A missing tool is an error, not a skip.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"
want=6
mode="${1:-all}"

require_all_passed() { # <label> <test output file>
  if ! grep -q "^test result: ok\. ${want} passed; 0 failed" "$2"; then
    echo "frame-targets: $1 did not pass all ${want} frame oracle tests:" >&2
    tail -20 "$2" >&2
    exit 1
  fi
  echo "frame-targets: $1: ${want} passed"
}

wasm() {
  command -v wasmtime >/dev/null || { echo "frame-targets: wasmtime is not on PATH" >&2; exit 2; }
  local out
  out="$(mktemp)"
  # the test reads laws/ by its absolute path, so the repository is mapped at the same path
  CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=${root}::${root}" \
    cargo test -p alice-lol --target wasm32-wasip1 --test motion_frame_oracle -- --test-threads=1 2>&1 | tee "$out"
  require_all_passed wasm32-wasip1 "$out"
}

ios() {
  command -v xcrun >/dev/null || { echo "frame-targets: xcrun is not available (macOS only)" >&2; exit 2; }
  local bin udid out
  bin="$(cargo test -p alice-lol --target aarch64-apple-ios-sim --test motion_frame_oracle --no-run \
    --message-format=json 2>/dev/null | python3 -c '
import json, sys
for line in sys.stdin:
    m = json.loads(line)
    if m.get("reason") == "compiler-artifact" and m.get("target", {}).get("name") == "motion_frame_oracle" and m.get("executable"):
        print(m["executable"])
')"
  [ -n "$bin" ] || { echo "frame-targets: no iOS simulator test binary was built" >&2; exit 2; }
  udid="$(xcrun simctl list devices available -j | python3 -c '
import json, sys
d = json.load(sys.stdin)["devices"]
print(next((x["udid"] for k, v in d.items() if "iOS" in k for x in v if "iPhone" in x["name"]), ""))
')"
  [ -n "$udid" ] || { echo "frame-targets: no iPhone simulator is available" >&2; exit 2; }
  xcrun simctl boot "$udid" 2>/dev/null || true
  out="$(mktemp)"
  local status=0
  xcrun simctl spawn "$udid" "$bin" --test-threads=1 >"$out" 2>&1 || status=$?
  xcrun simctl shutdown "$udid" 2>/dev/null || true
  cat "$out"
  [ "$status" -eq 0 ] || { echo "frame-targets: the iOS simulator run exited $status" >&2; exit 1; }
  require_all_passed aarch64-apple-ios-sim "$out"
}

case "$mode" in
  wasm) wasm ;;
  ios) ios ;;
  all) wasm; ios ;;
  *) echo "usage: $0 [wasm|ios|all]" >&2; exit 2 ;;
esac
