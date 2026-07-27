#!/usr/bin/env bash
# ALICE-LOL `alice-lol-sdf` skill — export 2D LOL DSL to Bambu Suite .lac (H2D laser).
#
# Usage:
#   scripts/laser.sh <input.lol> --output part.lac
#   scripts/laser.sh --help
#
# Note: .lac generation lives in a separate crate (`~/Project-ALICE/alice-metal-card`),
# not directly in `alice-lol`. This script documents the pipeline; the actual
# LOL 2D DSL → SVG → .lac path requires the alice-metal-card `lac_gen.rs`
# module or equivalent.
#
# Pipeline:
#   LOL 2D DSL (circle_2d / rect_2d / bezier)
#     → alice_lol runtime parser
#     → alice_sdf 2D SDF evaluation
#     → contour extraction → SVG paths (mm coordinates)
#     → alice_metal_card::lac_gen (usvg parse + PathObject conversion)
#     → .lac (Bambu Suite ZIP+JSON)
#     → open in Bambu Suite → H2D laser
#
# Required for full pipeline:
#   - Rust toolchain
#   - ALICE-LOL workspace at ../..
#   - Companion alice-metal-card crate (Project-ALICE/alice-metal-card)

set -euo pipefail
IFS=$'\n\t'

usage() {
    sed -n '2,24p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

if [[ $# -eq 0 ]] || [[ "${1:-}" == "--help" ]] || [[ "${1:-}" == "-h" ]]; then
    usage 0
fi

INPUT="${1:?input LOL path required}"
shift

OUTPUT=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --output) OUTPUT="${2:?output path required}"; shift 2 ;;
        *) echo "unknown arg: $1" >&2; usage 1 ;;
    esac
done

if [[ ! -f "$INPUT" ]]; then
    echo "error: input file not found: $INPUT" >&2
    exit 2
fi

if [[ -z "$OUTPUT" ]]; then
    STEM="${INPUT%.*}"
    OUTPUT="${STEM}.lac"
fi

echo "[alice-lol-sdf] .lac generation requires the alice-metal-card companion crate." >&2
echo "[alice-lol-sdf] Reference implementation: ~/Project-ALICE/alice-metal-card/src/lac_gen.rs" >&2
echo "[alice-lol-sdf] Bambu Suite .lac format spec: ~/.claude/projects/-Users-ys/memory/bambu-suite-lac-format.md" >&2
echo "[alice-lol-sdf] For now, manually run the alice-metal-card pipeline against $INPUT." >&2
echo "[alice-lol-sdf] Target output: $OUTPUT" >&2
exit 4
