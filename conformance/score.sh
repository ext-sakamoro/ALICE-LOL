#!/usr/bin/env bash
# Generates the conformance corpus from the law files, checks that it covers
# every corner of every law, scores one implementation, and deletes the corpus.
# Run it only after the implementation is finished (see conformance/PROTOCOL.md).
#
# usage: conformance/score.sh --kind static|reader <command...>
#        e.g. conformance/score.sh --kind static python3 impl/main.py
# --kind is the kind the implementation states (TASK.md); the probes are scored with it
# (static: the probes with a law file of their own are skipped and counted)
# env:   PYTHON     interpreter that has mpmath (default python3; see conformance/requirements.txt)
#        LAWS       directory of *.law files (default laws/spike of this repository)
#        LAW_TOOLS  directory with law_corners.py / law_corpus_cover.py (default scripts/ of this repository)
#        JOBS       parallel jobs for the corpus generator (default 8)
#        MIN_VECTORS  fail when the corpus has fewer vectors (default 1: an empty corpus fails)
set -euo pipefail
usage() { echo "usage: $0 --kind static|reader <command...>" >&2; exit 2; }
[[ "${1:-}" == "--kind" && ( "${2:-}" == "static" || "${2:-}" == "reader" ) ]] || usage
kind=$2
shift 2
[[ $# -ge 1 ]] || usage
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
PYTHON="${PYTHON:-python3}"
LAWS="${LAWS:-$root/laws/spike}"
TOOLS="${LAW_TOOLS:-$root/scripts}"
"$PYTHON" -c 'import mpmath' 2>/dev/null || {
  echo "$PYTHON has no mpmath: $PYTHON -m pip install -r $here/requirements.txt" >&2; exit 2; }
tmp=$(mktemp -d "${TMPDIR:-/tmp}/law_score.XXXXXX")
trap 'rm -rf "${tmp:?}"' EXIT
"$PYTHON" "$here/gen_corpus.py" --laws "$LAWS" --tools "$TOOLS" --out "$tmp/corpus.json" --jobs "${JOBS:-8}" --min-vectors "${MIN_VECTORS:-1}"
python3 "$TOOLS/law_corpus_cover.py" --laws "$LAWS" --corpus "$tmp/corpus.json"
status=0
python3 "$here/run_conformance.py" --corpus "$tmp/corpus.json" -- "$@" || status=$?
python3 "$here/check_probes.py" --kind "$kind" -- "$@" || status=$?
exit "$status"
