#!/usr/bin/env bash
# ALICE-LOL `alice-lol-sdf` skill — export LOL DSL to 3D print artifact.
#
# Usage:
#   scripts/print.sh <input.lol> --format stl
#   scripts/print.sh <input.lol> --format 3mf --output part.3mf
#   scripts/print.sh <input.lol> --format 3mf --resolution 128 --scale-mm 20
#   scripts/print.sh --help
#
# Formats: stl, 3mf
# Default resolution: 96 (safe for slicers, ~250k triangles for medium parts)
# Default scale: 1 unit = 1 mm
#
# Under the hood: runs `cargo run --example print_export` (parent crate),
# which uses `alice_lol::print_export::{lol_to_stl, node_to_3mf}` with
# automatic `MeshRepair::repair_all(epsilon=1e-3)`.
#
# Requires:
#   - Rust toolchain
#   - ALICE-LOL workspace at ../..

set -euo pipefail
IFS=$'\n\t'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILL_DIR="$(dirname "$SCRIPT_DIR")"
CRATE_DIR="$(cd "$SKILL_DIR/../.." && pwd)"

usage() {
    sed -n '2,19p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

if [[ $# -eq 0 ]] || [[ "${1:-}" == "--help" ]] || [[ "${1:-}" == "-h" ]]; then
    usage 0
fi

INPUT="${1:?input LOL path required}"
shift

FORMAT=""
OUTPUT=""
RESOLUTION="96"
SCALE_MM="1.0"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --format) FORMAT="${2:?format required (stl|3mf)}"; shift 2 ;;
        --output) OUTPUT="${2:?output path required}"; shift 2 ;;
        --resolution) RESOLUTION="${2:?resolution required}"; shift 2 ;;
        --scale-mm) SCALE_MM="${2:?scale required}"; shift 2 ;;
        *) echo "unknown arg: $1" >&2; usage 1 ;;
    esac
done

if [[ ! -f "$INPUT" ]]; then
    echo "error: input file not found: $INPUT" >&2
    exit 2
fi

case "$FORMAT" in
    stl|3mf) ;;
    "") echo "error: --format required (stl|3mf)" >&2; exit 3 ;;
    *) echo "error: unsupported format: $FORMAT (use stl or 3mf)" >&2; exit 3 ;;
esac

if [[ -z "$OUTPUT" ]]; then
    STEM="${INPUT%.*}"
    OUTPUT="${STEM}.${FORMAT}"
fi

if (( RESOLUTION > 192 )); then
    echo "warning: resolution $RESOLUTION exceeds 192 (parent crate rule for slicer safety)" >&2
fi

cd "$CRATE_DIR"

echo "[alice-lol-sdf] Exporting $INPUT → $OUTPUT (format=$FORMAT, resolution=$RESOLUTION, scale_mm=$SCALE_MM)..." >&2

# Placeholder: parent crate needs a dedicated `print` CLI accepting arbitrary LOL files.
# Currently `examples/print_export.rs` uses hardcoded scenes.
# A follow-up TODO is to add `alice-lol/src/bin/print.rs` that accepts --input/--format/--output.
echo "[alice-lol-sdf] Note: parent crate's print CLI accepting arbitrary LOL files is pending." >&2
echo "[alice-lol-sdf] For now, run the parent example directly:" >&2
echo "    cd $CRATE_DIR && cargo run --release --example print_export" >&2
echo "[alice-lol-sdf] Or embed your LOL text in a Rust program using alice_lol::print_export::lol_to_stl()." >&2
exit 4
