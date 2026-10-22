#!/usr/bin/env bash
# Scores an implementation of the rule laws (kind machine / predicate) on the rule-law vectors
# of a corpus generated from the law files, then deletes the corpus. The expected answers come
# from scripts/law_rules.py; the implementation (CI: alice-lol examples/rule_conformance.rs) is
# a separate reader, so a defect on either side shows as a failing vector.
#
# usage: conformance/score_rules.sh <command...>
# env:   PYTHON, LAWS, LAW_TOOLS, JOBS as in score.sh
# fails when a rule law misses a vector it must have (law_corpus_cover.py), when a vector
# fails, or when the corpus has no rule-law vector (run_conformance.py exits 2)
set -euo pipefail
[[ $# -ge 1 ]] || { echo "usage: $0 <command...>" >&2; exit 2; }
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
PYTHON="${PYTHON:-python3}"
LAWS="${LAWS:-$root/laws/spike}"
TOOLS="${LAW_TOOLS:-$root/scripts}"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/law_rules.XXXXXX")
trap 'rm -rf "${tmp:?}"' EXIT
"$PYTHON" "$here/gen_corpus.py" --laws "$LAWS" --tools "$TOOLS" --out "$tmp/corpus.json" --jobs "${JOBS:-8}"
python3 "$TOOLS/law_corpus_cover.py" --laws "$LAWS" --corpus "$tmp/corpus.json"
python3 "$here/run_conformance.py" --corpus "$tmp/corpus.json" --law-dir "$LAWS" --only-rule-laws -- "$@"
