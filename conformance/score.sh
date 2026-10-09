#!/usr/bin/env bash
# Generates the conformance corpus from the law files, checks that it covers
# every corner of every law, scores one implementation, and deletes the corpus.
# Run it only after the implementation is finished (see conformance/PROTOCOL.md).
#
# usage: conformance/score.sh <command...>     e.g. conformance/score.sh python3 impl/main.py
# env:   PYTHON     interpreter that has mpmath (default python3; see conformance/requirements.txt)
#        LAWS       directory of *.law files (default laws/spike of this repository)
#        LAW_TOOLS  directory with law_corners.py / law_corpus_cover.py (default scripts/ of this repository)
#        JOBS       parallel jobs for the corpus generator (default 8)
set -euo pipefail
[[ $# -ge 1 ]] || { echo "usage: $0 <command...>" >&2; exit 2; }
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
PYTHON="${PYTHON:-python3}"
LAWS="${LAWS:-$root/laws/spike}"
TOOLS="${LAW_TOOLS:-$root/scripts}"
"$PYTHON" -c 'import mpmath' 2>/dev/null || {
  echo "$PYTHON has no mpmath: $PYTHON -m pip install -r $here/requirements.txt" >&2; exit 2; }
tmp=$(mktemp -d "${TMPDIR:-/tmp}/law_score.XXXXXX")
trap 'rm -rf "${tmp:?}"' EXIT
"$PYTHON" "$here/gen_corpus.py" --laws "$LAWS" --tools "$TOOLS" --out "$tmp/corpus.json" --jobs "${JOBS:-8}"
python3 "$TOOLS/law_corpus_cover.py" --laws "$LAWS" --corpus "$tmp/corpus.json"
python3 "$here/run_conformance.py" --corpus "$tmp/corpus.json" -- "$@"
