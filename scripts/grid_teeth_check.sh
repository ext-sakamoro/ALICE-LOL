#!/usr/bin/env bash
# Reintroduces 5 historical regressions as mutants, one at a time, directly
# in this working tree, and asserts the relevant gate (the degenerate-
# parameter grid test or a named unit test) turns red for each. A gate that
# cannot reproduce the bug it was built for has no teeth; this is how that
# claim is checked, not just asserted in a doc comment. Every file this
# script mutates is `git checkout --`-restored after each mutant, including
# on an interrupted run (trap), so the working tree ends clean either way.
#
# A separate checkout at another path is deliberately NOT used: this crate's
# sibling path dependencies (`alice-kinematics`'s `../ALICE-LOL/alice-lol`,
# among others) hardcode the literal directory name "ALICE-LOL", so a second
# checkout under any other name collides with it in the lockfile ("package
# collision... different, but only one can be written to lockfile
# unambiguously") before a single mutant's test ever runs -- every mutant
# "passing" that way would be a false positive from the collision, not from
# the mutation. Mutating this tree directly, immediately restoring it, is
# the only form that actually exercises what it claims to.
#
# Run weekly in CI / on demand, NOT on every push: each mutant rebuilds the
# probe binary and reruns ~35,000 grid cases (~90s-150s), so 5 mutants is
# roughly 10 minutes total, and this tree must be clean (no uncommitted
# changes of your own) before running it, since each mutant's restore is a
# `git checkout --` of the files it touched.
#
# usage: scripts/grid_teeth_check.sh
# exit 0 if every mutant turned its target red; exit 1 if any mutant stayed
# green (a gap the table in this script's own comments should either close
# or explicitly justify).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

if [ -n "$(git status --porcelain)" ]; then
  echo "grid_teeth_check: working tree is not clean -- commit or stash first" >&2
  echo "(each mutant restores its files with \`git checkout --\`, which would discard uncommitted work)" >&2
  exit 2
fi

MUTATED_FILES=()

cleanup() {
  if [ "${#MUTATED_FILES[@]}" -gt 0 ]; then
    git checkout -- "${MUTATED_FILES[@]}" 2>/dev/null || true
  fi
}
trap cleanup EXIT

declare -a RESULTS=()

# $1 = mutant id, $2 = description, $3 = python3 mutation script, $4 = test
# command, $5.. = files the mutation touches (restored after, even on
# failure/interrupt)
run_mutant() {
  local id="$1" desc="$2" mutate="$3" test_cmd="$4"
  shift 4
  MUTATED_FILES=("$@")
  echo "== $id: $desc =="
  local log="/tmp/grid_teeth_${id}.log"
  python3 -c "$mutate"
  set +e
  eval "$test_cmd" >"$log" 2>&1
  local rc=$?
  set -e
  git checkout -- "${MUTATED_FILES[@]}"
  MUTATED_FILES=()
  if [ "$rc" -ne 0 ]; then
    echo "  RED as expected (rc=$rc, log: $log)"
    RESULTS+=("$id|RED (expected)|$desc")
  else
    echo "  MISS: stayed GREEN (rc=0, log: $log) -- this gate does not catch this mutant"
    RESULTS+=("$id|GREEN (MISS)|$desc")
  fi
}

# ── M1: gridfinity_bin_ex's parser call site reverted to the infallible
# gridfinity_bin (the product-of-two-counts regression this audit started
# from) -- expected red via the grid's all-same/pairwise generators (a panic,
# not a hang: GridfinitySpec::validate() itself is untouched, so the
# infallible builder's own .expect() still fires)
run_mutant "M1" \
  "gridfinity_bin_ex parser site reverted to the infallible builder (product overflow)" \
  "
p = 'alice-lol/src/runtime_parser.rs'
t = open(p, encoding='utf-8').read()
old = '''crate::stdlib::hardsurface::pattern_sdf::try_gridfinity_bin(&spec)
                    .map_err(|e| self.spec_error(e))
            }
            \"wall_hook\"'''
new = '''Ok(crate::stdlib::hardsurface::pattern_sdf::gridfinity_bin(&spec))
            }
            \"wall_hook\"'''
assert old in t, 'M1 anchor not found'
open(p, 'w', encoding='utf-8').write(t.replace(old, new, 1))
" \
  "cargo test -p alice-lol --test degenerate_grid -- --ignored" \
  "alice-lol/src/runtime_parser.rs"

# ── M2: the skadis panel size guard removed from BOTH the fallible and
# infallible entry points (the original bug: an unguarded huge/non-finite
# size) -- expected red via the grid. Measured mechanism (not a hang: a
# literal "NaN" in .lol text is rejected earlier, at the lexer's token
# dispatch, regardless of this guard -- only a numeric-looking value reaches
# this code at all): a huge-but-finite size (e.g. 65536) makes the
# connector-hole loop enumerate far more holes than intended, and
# `balanced_union_fold`-ing that many into one SdfNode tree overflows the
# stack (observed: "thread 'main' has overflowed its stack", SIGABRT,
# reproduced directly by running the mutated probe binary by hand)
run_mutant "M2" \
  "skadis_panel_sdf's size guard removed from both entry points (unbounded hole count -> stack overflow)" \
  "
p = 'alice-lol/src/stdlib/hardsurface/skadis_sdf.rs'
t = open(p, encoding='utf-8').read()
old_infallible = '''    checked_bounded(size, 0.0, MAX_SKADIS_PANEL_MM, \"panel_size\").expect(
        \"skadis_panel_sdf の size が検査を通らない (0 より大きく MAX_SKADIS_PANEL_MM mm 以下の有限値でなければならない): untrusted な入力は try_skadis_panel_sdf を使うこと\",
    );
'''
assert old_infallible in t, 'M2 infallible-site anchor not found'
t = t.replace(old_infallible, '', 1)
old_fallible = '''    checked_bounded(size, 0.0, MAX_SKADIS_PANEL_MM, \"panel_size\")?;
    Ok(skadis_panel_sdf(size, thickness, corner_radius))'''
new_fallible = '''    Ok(skadis_panel_sdf(size, thickness, corner_radius))'''
assert old_fallible in t, 'M2 fallible-site anchor not found'
t = t.replace(old_fallible, new_fallible, 1)
open(p, 'w', encoding='utf-8').write(t)
" \
  "cargo test -p alice-lol --test degenerate_grid -- --ignored" \
  "alice-lol/src/stdlib/hardsurface/skadis_sdf.rs"

# ── M3: the lexer's non-finite-literal rejection removed (an overflowing
# literal like 1e39 reads as +inf again instead of a parse error) --
# expected red via tests/finite_numbers.rs's crash-input regression test
# (CRASH_EMIT), not the grid (accepting the token does not itself crash
# anything; it is the emit round-trip that notices an un-writable value)
run_mutant "M3" \
  "the lexer's finite-literal check removed (non-finite literals parse again)" \
  "
p = 'alice-lol/src/runtime_parser.rs'
t = open(p, encoding='utf-8').read()
old = '''        if !v.is_finite() {
            return Err(ParseError {
                message: format!(\"number out of range: '{s}'\"),
                position: start,
            });
        }
        Ok(Some(Token::Number(v)))'''
new = '''        Ok(Some(Token::Number(v)))'''
assert old in t, 'M3 anchor not found'
open(p, 'w', encoding='utf-8').write(t.replace(old, new, 1))
" \
  "cargo test -p alice-lol --test finite_numbers" \
  "alice-lol/src/runtime_parser.rs"

# ── M4: ShelfDividerSpec::validate() skipped (hex_hole_pitch's layer-2
# check deleted) -- expected red via the named unit test, NOT the grid:
# shelf_divider is only ever called from .lol text with the fixed
# field_tested_560x250x120() spec (hex_hole_pitch always 20.0, never
# degenerate), so this hazard is unreachable from the parser-facing grid by
# design; only a direct Rust caller of the Spec API can trigger it
run_mutant "M4" \
  "ShelfDividerSpec::validate() skipped (layer-2 pitch check deleted)" \
  "
p = 'alice-lol/src/stdlib/hardsurface/pattern_sdf.rs'
t = open(p, encoding='utf-8').read()
old = '''    pub fn validate(&self) -> Result<(), SpecError> {
        checked_positive_finite(self.hex_hole_pitch, MIN_PITCH_MM, \"pitch\")?;
        Ok(())
    }'''
new = '''    pub fn validate(&self) -> Result<(), SpecError> {
        Ok(())
    }'''
assert old in t, 'M4 anchor not found'
open(p, 'w', encoding='utf-8').write(t.replace(old, new, 1))
" \
  "cargo test -p alice-lol --lib -- shelf_divider_spec_validate_refuses_a_zero_pitch" \
  "alice-lol/src/stdlib/hardsurface/pattern_sdf.rs"

# ── M5: GridfinitySpec::validate() itself deleted (not bypassed at the
# parser like M1 -- the check is GONE, so nothing downstream still guards
# it either) -- expected red via the grid. Measured mechanism: construction
# actually proceeds this time (unlike M1, where validate() itself still
# exists and panics), and either the probe's 256 MB counting-allocator cap
# fires (exit 42) or the runtime allocator/OS aborts the process outright
# for a single oversized request before the cap's own per-allocation check
# ever runs (observed for 1024x1024: a crash, not exit 42 -- the system
# allocator failing a request this large is itself the finding, same class
# as M2's stack overflow: unbounded construction hits SOME hard limit, which
# one depends on the platform and the exact shape of the construction)
run_mutant "M5" \
  "GridfinitySpec::validate() deleted entirely (unbounded eager allocation)" \
  "
p = 'alice-lol/src/stdlib/hardsurface/pattern_sdf.rs'
t = open(p, encoding='utf-8').read()
old = '''    pub fn validate(&self) -> Result<(), SpecError> {
        if let Some((cols, rows)) = self.dividers {
            checked_product(cols, rows, MAX_NODE_EXPANSION, \"grid_expansion\")?;
        }
        Ok(())
    }'''
new = '''    pub fn validate(&self) -> Result<(), SpecError> {
        Ok(())
    }'''
assert old in t, 'M5 anchor not found'
open(p, 'w', encoding='utf-8').write(t.replace(old, new, 1))
" \
  "cargo test -p alice-lol --test degenerate_grid -- --ignored" \
  "alice-lol/src/stdlib/hardsurface/pattern_sdf.rs"

echo
echo "== grid_teeth_check summary =="
printf '%-4s %-16s %s\n' "id" "result" "description"
fail=0
for r in "${RESULTS[@]}"; do
  IFS='|' read -r id result desc <<<"$r"
  printf '%-4s %-16s %s\n' "$id" "$result" "$desc"
  [[ "$result" == GREEN* ]] && fail=1
done

if [ "$fail" -ne 0 ]; then
  echo
  echo "grid_teeth_check: one or more mutants did not turn their target red (see table above)"
  exit 1
fi
echo
echo "grid_teeth_check: ok (all 5 mutants turned their target red)"
