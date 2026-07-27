#!/usr/bin/env bash
# ALICE-LOL `alice-lol-sdf` skill — parse and validate LOL DSL text.
#
# Usage:
#   scripts/parse.sh <input.lol>
#   scripts/parse.sh --help
#
# STATUS: STUB / pending upstream CLI
#
# The alice-lol crate does not currently ship a dedicated `parse` binary
# that accepts arbitrary LOL text files. `alice_lol::runtime_parser::parse_lol`
# is exposed only as a library function; the CLI wrapper for this skill is
# waiting on an upstream addition (e.g. `alice-lol/src/bin/lol.rs`).
#
# Current workaround: embed the LOL text in a minimal Rust program:
#
#   fn main() {
#       let src = std::fs::read_to_string("<input.lol>").unwrap();
#       let node = alice_lol::runtime_parser::parse_lol(&src)
#           .expect("LOL parse failed");
#       println!("parsed OK: {:?}", node);
#   }
#
# For LLM-oriented workflows, prefer using the GBNF grammar in
# references/lol.gbnf at inference time — that guarantees LOL syntax
# validity by construction and eliminates the need for a separate parse step.
#
# This script exits non-zero to make the pending state explicit.

set -euo pipefail
IFS=$'\n\t'

usage() {
    sed -n '2,29p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

if [[ $# -eq 0 ]] || [[ "${1:-}" == "--help" ]] || [[ "${1:-}" == "-h" ]]; then
    usage 0
fi

INPUT="${1:?input LOL path required}"

if [[ ! -f "$INPUT" ]]; then
    echo "error: input file not found: $INPUT" >&2
    exit 2
fi

echo "[alice-lol-sdf] parse.sh is a STUB pending upstream CLI (see script comments)." >&2
echo "[alice-lol-sdf] For now, embed the LOL text in a Rust program using" >&2
echo "    alice_lol::runtime_parser::parse_lol(&src)" >&2
echo "[alice-lol-sdf] Input file: $INPUT" >&2
exit 4
