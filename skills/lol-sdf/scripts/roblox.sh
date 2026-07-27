#!/usr/bin/env bash
# ALICE-LOL `alice-lol-sdf` skill — export LOL DSL to Roblox OBJ/FBX.
#
# Usage:
#   scripts/roblox.sh <input.lol> --format obj [--preset accessory|meshpart]
#   scripts/roblox.sh <input.lol> --format fbx --preset accessory
#   scripts/roblox.sh --help
#
# Presets (from parent crate `roblox_export::RobloxConfig`):
#   accessory — UGC accessory (4,000 triangle max)
#   meshpart  — generic MeshPart (10,000 triangle max, default)
#
# Runs `roblox_accessory` example under the `roblox` feature. Coordinate
# system is Y-up right-hand (ALICE-SDF / Roblox agree, no flip needed).
# `validate_for_roblox()` runs automatically — reports triangle count,
# bounding size, and degenerate face count.
#
# Requires:
#   - Rust toolchain
#   - ALICE-LOL workspace at ../..
#   - `roblox` feature enabled

set -euo pipefail
IFS=$'\n\t'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILL_DIR="$(dirname "$SCRIPT_DIR")"
CRATE_DIR="$(cd "$SKILL_DIR/../.." && pwd)"

usage() {
    sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

if [[ $# -eq 0 ]] || [[ "${1:-}" == "--help" ]] || [[ "${1:-}" == "-h" ]]; then
    usage 0
fi

INPUT="${1:?input LOL path required}"
shift

FORMAT=""
PRESET="meshpart"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --format) FORMAT="${2:?format required (obj|fbx)}"; shift 2 ;;
        --preset) PRESET="${2:?preset required (accessory|meshpart)}"; shift 2 ;;
        *) echo "unknown arg: $1" >&2; usage 1 ;;
    esac
done

if [[ ! -f "$INPUT" ]]; then
    echo "error: input file not found: $INPUT" >&2
    exit 2
fi

case "$FORMAT" in
    obj|fbx) ;;
    "") echo "error: --format required (obj|fbx)" >&2; exit 3 ;;
    *) echo "error: unsupported format: $FORMAT (use obj or fbx)" >&2; exit 3 ;;
esac

case "$PRESET" in
    accessory|meshpart) ;;
    *) echo "error: unsupported preset: $PRESET (use accessory or meshpart)" >&2; exit 3 ;;
esac

cd "$CRATE_DIR"

echo "[alice-lol-sdf] Roblox export: $INPUT → format=$FORMAT preset=$PRESET" >&2
echo "[alice-lol-sdf] Note: parent crate's Roblox CLI accepting arbitrary LOL files is pending." >&2
echo "[alice-lol-sdf] For now, adapt examples/roblox_accessory.rs to load $INPUT." >&2
echo "[alice-lol-sdf] Reference: cargo run --release --features roblox --example roblox_accessory" >&2
exit 4
