#!/usr/bin/env bash
# Run each law oracle test target on its own and fail when any of them ran zero
# tests.
#
# `cargo test` stays green when a feature compiles a test module out, a filter
# matches nothing, or a target is renamed; summing the counts of several
# targets would hide one empty target behind the others. Each target here goes
# through scripts/cargo_test_nonzero.sh separately. A test file that uses the
# law modules but is not listed below also fails the run, so a new oracle file
# cannot be skipped silently. Used by ci.yml and scripts/preflight.sh.
#
# usage: scripts/law_tests.sh [default|physics|all|list]   (default: all)
#   list: only check that every law test file is listed (CI counts the runs from
#         the log of its full `cargo test` with scripts/law_tests_from_log.py)
set -euo pipefail
cd "$(dirname "$0")/.."

# package|cargo test target arguments|features
DEFAULT_TARGETS=(
  "alice-lol|--lib law::|"
  "alice-lol|--test law_tests|"
  "alice-lol|--test audit_law_parity|"
  "alice-lol|--test audit_law_semantics|"
    "alice-lol|--test law_id_oracle|"
  "alice-lol|--test law_corpus_oracle|"
  "alice-lol|--test analytic_law|"
  "alice-lol|--test test_field_law_oracle|"
  "alice-lol|--test analytic_research_law|"
  "alice-lol|--test research_law_bit_exact|"
  "alice-lol|--test interior_lipschitz_bound_probe|"
  "alice-lol|--test print_tests|"
  "alice-lol|--test spike_law_files_parse|"
  "alice-lol-robot|--test analytic_robot_law|"
)
PHYSICS_TARGETS=(
  "alice-lol|--test analytic_thermal|physics"
)

mode="${1:-all}"
case "$mode" in
  default) targets=("${DEFAULT_TARGETS[@]}") ;;
  physics) targets=("${PHYSICS_TARGETS[@]}") ;;
  all|list) targets=("${DEFAULT_TARGETS[@]}" "${PHYSICS_TARGETS[@]}") ;;
  *) echo "usage: $0 [default|physics|all|list]" >&2; exit 2 ;;
esac

# Every integration test file that uses `law::` or `research_law` must be listed
# (`audit_law::` matches `law::` too, so audit laws are covered by the same scan)
listed=" "
for t in "${DEFAULT_TARGETS[@]}" "${PHYSICS_TARGETS[@]}"; do
  args="${t#*|}"; args="${args%%|*}"
  [[ "$args" == --test* ]] && listed+="${args#--test } "
done
unlisted=()
while IFS= read -r f; do
  name="$(basename "$f" .rs)"
  [[ "$listed" == *" $name "* ]] || unlisted+=("$f")
done < <(grep -lE 'law::|research_law' -- */tests/*.rs)
if [ "${#unlisted[@]}" -gt 0 ]; then
  echo "error: law test files not listed in scripts/law_tests.sh: ${unlisted[*]}" >&2
  exit 1
fi

if [ "$mode" = list ]; then
  echo "law tests: every law test file is listed"
  exit 0
fi

for t in "${targets[@]}"; do
  pkg="${t%%|*}"; rest="${t#*|}"; args="${rest%%|*}"; features="${rest#*|}"
  echo "== law tests: -p $pkg $args${features:+ --features $features}"
  # shellcheck disable=SC2086  # $args is a flag and its value
  scripts/cargo_test_nonzero.sh -p "$pkg" $args ${features:+--features "$features"}
done
echo "law tests: ${#targets[@]} targets, each ran at least one test"
