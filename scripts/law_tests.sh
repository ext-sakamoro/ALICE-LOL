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
# usage: scripts/law_tests.sh [default|physics|crypto|all|list] [cargo test args...]   (default: all)
#   extra arguments go to every `cargo test` (e.g. `--release` for the release-profile leg)
#   list: only check that every law test file is listed (CI counts the runs from
#         the log of its full `cargo test` with scripts/law_tests_from_log.py)
set -euo pipefail
cd "$(dirname "$0")/.."

# package|cargo test target arguments|features
DEFAULT_TARGETS=(
  "alice-lol|--lib law::|"
  "alice-lol|--test law_tests|"
  "alice-lol|--test audit_law_includes|"
  "alice-lol|--test audit_law_parity|"
  "alice-lol|--test audit_law_semantics|"
    "alice-lol|--test law_id_oracle|"
  "alice-lol|--test law_corpus_oracle|"
  "alice-lol|--test analytic_law|"
  "alice-lol|--test test_field_law_oracle|"
  "alice-lol|--test analytic_research_law|"
  "alice-lol|--test research_law_bit_exact|"
  "alice-lol|--test research_law_id_oracle|"
  "alice-lol|--test analytic_research_functions|"
  "alice-lol|--test interior_lipschitz_bound_probe|"
  "alice-lol|--test print_tests|"
  "alice-lol|--test spike_law_files_parse|"
  "alice-lol|--test finite_evaluation_rule|"
  "alice-lol|--test rule_law_oracle|"
  "alice-lol|--test table_law_oracle|"
  "alice-lol|--test statute_law_oracle|"
  "alice-lol|--test memory_law_oracle|"
  "alice-lol|--test procedure_law_oracle|"
  "alice-lol|--test contract_law_oracle|"
  "alice-lol|--test rule_law_limits|"
  "alice-lol|--test motion_law_oracle|"
  "alice-lol|--test law_ref_oracle|"
  "alice-lol|--test program_law|"
  "alice-lol|--test motion_frame_oracle|"
  "alice-lol|--test audit_series_oracle|"
  "alice-lol|--test proportion_law_oracle|"
  "alice-lol|--test avatar_laws|"
  "alice-lol|--test sheet_law|"
  "alice-lol|--test variant|"
  "alice-lol|--test three_view|"
  "alice-lol|--test sheet_render|"
  "alice-lol-humanoid|--test measure_body|"
)
PHYSICS_TARGETS=(
  "alice-lol|--test analytic_thermal|physics"
)
# the rule-law call (`hmac_sha256`) is built only with crypto-calls; run with `crypto` / `all`
CRYPTO_TARGETS=(
  "alice-lol|--test rule_law_oracle|crypto-calls"
)

mode="${1:-all}"
extra=("${@:2}")
case "$mode" in
  default) targets=("${DEFAULT_TARGETS[@]}") ;;
  physics) targets=("${PHYSICS_TARGETS[@]}") ;;
  crypto) targets=("${CRYPTO_TARGETS[@]}") ;;
  all|list) targets=("${DEFAULT_TARGETS[@]}" "${PHYSICS_TARGETS[@]}" "${CRYPTO_TARGETS[@]}") ;;
  *) echo "usage: $0 [default|physics|crypto|all|list]" >&2; exit 2 ;;
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

# with --release among the extra arguments, every run must say it built the release profile: the arguments
# reaching cargo are read back from what cargo did, not from what this script was asked
release=0
for a in ${extra[@]+"${extra[@]}"}; do [ "$a" = --release ] && release=1; done
tlog=$(mktemp)
trap 'rm -f "$tlog"' EXIT
released=0
for t in "${targets[@]}"; do
  pkg="${t%%|*}"; rest="${t#*|}"; args="${rest%%|*}"; features="${rest#*|}"
  echo "== law tests: -p $pkg $args${features:+ --features $features}"
  # shellcheck disable=SC2086  # $args is a flag and its value
  scripts/cargo_test_nonzero.sh -p "$pkg" $args ${features:+--features "$features"} ${extra[@]+"${extra[@]}"} 2>&1 | tee "$tlog"
  if [ "$release" -eq 1 ]; then
    # (the colour codes of CARGO_TERM_COLOR=always sit between "Finished" and the profile: strip them first)
    if ! python3 -c 'import re, sys; t = re.sub(r"\x1b\[[0-9;]*m", "", open(sys.argv[1], errors="replace").read()); sys.exit(0 if "Finished `release` profile" in t else 1)' "$tlog"; then
      echo "error: law tests: -p $pkg $args was asked for --release but cargo did not build the release profile" >&2
      exit 1
    fi
    released=$((released + 1))
  fi
done
if [ "$release" -eq 1 ] && [ "$released" -ne "${#targets[@]}" ]; then
  echo "error: law tests: $released of ${#targets[@]} targets built the release profile" >&2
  exit 1
fi
if [ "$release" -eq 1 ]; then
  echo "law tests: ${#targets[@]} targets, each ran at least one test, each in the release profile"
else
  echo "law tests: ${#targets[@]} targets, each ran at least one test"
fi
