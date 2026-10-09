#!/usr/bin/env bash
# Must-red controls for the corpus and the runner: generates one corpus, then runs
# conformance/ref_impl.py unchanged and with each REF_BUG variant, and fails unless
# every variant gets the expected outcome:
#   unchanged, 6, 8 (other evaluation orders of r^3)            -> every vector passes
#   1, 2, 3 (closed-form / range / audit defects)               -> at least one vector fails
#   9 (evidence read as "not 0"), 10 (ranges as multisets)     -> at least one vector fails
#   11 (1e400 read as a number), 12 (malformed build as empty)  -> at least one vector fails
#   4 (RK4), 5 (last step dropped), 7 (state before each step)  -> every in-range Kepler vector fails
#   4 again with the state comparison switched off              -> every in-range Kepler vector fails
# The corpus is deleted at the end.
#
# usage: conformance/controls.sh       env: PYTHON, LAWS, LAW_TOOLS, JOBS as in score.sh
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
PYTHON="${PYTHON:-python3}"
LAWS="${LAWS:-$root/laws/spike}"
TOOLS="${LAW_TOOLS:-$root/scripts}"
"$PYTHON" -c 'import mpmath' 2>/dev/null || {
  echo "$PYTHON has no mpmath: $PYTHON -m pip install -r $here/requirements.txt" >&2; exit 2; }
tmp=$(mktemp -d "${TMPDIR:-/tmp}/law_controls.XXXXXX")
trap 'rm -rf "${tmp:?}"' EXIT
"$PYTHON" "$here/gen_corpus.py" --laws "$LAWS" --tools "$TOOLS" --out "$tmp/corpus.json" --jobs "${JOBS:-8}"
python3 "$TOOLS/law_corpus_cover.py" --laws "$LAWS" --corpus "$tmp/corpus.json" >/dev/null
total=$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))))' "$tmp/corpus.json")
kepler_in=$(python3 -c 'import json,sys; print(sum(v["kind"] == "invariant" for v in json.load(open(sys.argv[1]))))' "$tmp/corpus.json")
[[ "$total" -gt 0 && "$kepler_in" -gt 0 ]] || { echo "error: empty corpus (compared nothing)" >&2; exit 1; }
echo "corpus: $total vectors, $kepler_in in-range Kepler vectors"

bad=0
# run <label> <expect: pass|fail|kepler-fail> [env assignments...]
run() {
  local label=$1 expect=$2; shift 2
  local log="$tmp/$label.log" rc=0
  env "$@" python3 "$here/run_conformance.py" --corpus "$tmp/corpus.json" --show 1 \
    -- python3 "$here/ref_impl.py" > "$log" 2>&1 || rc=$?
  local passed failed kfail
  passed=$(awk '$1 == "TOTAL" {print $2}' "$log")
  failed=$(awk '$1 == "TOTAL" {print $3 + $4}' "$log")
  kfail=$(awk '$1 ~ /^kepler_energy_bounded_/ {s += $3 + $4} END {print s + 0}' "$log")
  local ok=0
  case $expect in
    pass) [[ $rc -eq 0 && "$passed" == "$total" ]] && ok=1 ;;
    fail) [[ $rc -eq 1 && "${failed:-0}" -gt 0 ]] && ok=1 ;;
    kepler-fail) [[ $rc -eq 1 && "$kfail" -eq "$kepler_in" ]] && ok=1 ;;
  esac
  printf '%-34s expect %-11s exit %d, pass %s, fail %s, Kepler fail %s/%s  %s\n' \
    "$label" "$expect" "$rc" "${passed:-?}" "${failed:-?}" "$kfail" "$kepler_in" "$([[ $ok -eq 1 ]] && echo ok || echo WRONG)"
  [[ $ok -eq 1 ]] || { bad=$((bad + 1)); tail -20 "$log"; }
}
run unchanged pass
run ref_bug_6_pow_r3 pass REF_BUG=6
run ref_bug_8_sqrt_cubed pass REF_BUG=8
run ref_bug_1_drag_factor fail REF_BUG=1
run ref_bug_2_isa_range fail REF_BUG=2
run ref_bug_3_audit_rule fail REF_BUG=3
run ref_bug_9_evidence_not_zero fail REF_BUG=9
run ref_bug_10_range_multiset fail REF_BUG=10
run ref_bug_11_overflow_is_number fail REF_BUG=11
run ref_bug_12_malformed_build_empty fail REF_BUG=12
run ref_bug_4_rk4 kepler-fail REF_BUG=4
run ref_bug_5_last_step_dropped kepler-fail REF_BUG=5
run ref_bug_7_state_before_step kepler-fail REF_BUG=7
run ref_bug_4_rk4_invariants_only kepler-fail REF_BUG=4 KEPLER_INVARIANTS_ONLY=1
[[ $bad -eq 0 ]] || { echo "$bad control(s) did not behave as expected" >&2; exit 1; }
echo "all controls behave as expected"
