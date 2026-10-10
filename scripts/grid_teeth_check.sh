#!/usr/bin/env bash
# Reintroduces 5 historical regressions as mutants, one at a time, in an
# isolated worktree, and asserts the relevant gate (the degenerate-parameter
# grid test or a named unit test) turns red for each. A gate that cannot
# reproduce the bug it was built for has no teeth; this is how that claim is
# checked, not just asserted in a doc comment.
#
# Run weekly in CI / `preflight.sh --full`, NOT on every push: each mutant
# rebuilds the probe binary and reruns ~35,000 grid cases (~2 min), so 5
# mutants is roughly 10 minutes total.
#
# usage: scripts/grid_teeth_check.sh
# exit 0 if every mutant turned its target red; exit 1 if any mutant stayed
# green (a gap the table below should either close or explicitly justify).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# a sibling of REPO_ROOT, at the same level as the path-dependency siblings
# (ALICE-SDF / ALICE-Zip / ...) this crate's Cargo.toml resolves relative to
WT="$(dirname "$REPO_ROOT")/ALICE-LOL-grid-teeth-check-tmp"

cleanup() {
  cd "$REPO_ROOT"
  git worktree remove --force "$WT" >/dev/null 2>&1 || true
}
trap cleanup EXIT

rm -rf "$WT"
git -C "$REPO_ROOT" worktree add --detach "$WT" HEAD >/dev/null

declare -a RESULTS=()

# $1 = mutant id, $2 = description, $3 = python3 mutation script (reads/writes
# files under $WT), $4 = test command (run with cwd = $WT)
run_mutant() {
  local id="$1" desc="$2" mutate="$3" test_cmd="$4"
  echo "== $id: $desc =="
  local log="/tmp/grid_teeth_${id}.log"
  python3 -c "$mutate"
  set +e
  (cd "$WT" && eval "$test_cmd") >"$log" 2>&1
  local rc=$?
  set -e
  git -C "$WT" checkout -- . >/dev/null 2>&1
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
p = '$WT/alice-lol/src/runtime_parser.rs'
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
  "cargo test -p alice-lol --test degenerate_grid"

# ── M2: the skadis panel size guard removed from BOTH the fallible and
# infallible entry points (the original hang: NaN never terminates the
# connector-hole loop) -- expected red via the grid's one-at-a-time
# generator timing out (a real hang this time, not a panic: unlike M1,
# nothing downstream still guards it)
run_mutant "M2" \
  "skadis_panel_sdf's size guard removed from both entry points (hang)" \
  "
p = '$WT/alice-lol/src/stdlib/hardsurface/skadis_sdf.rs'
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
  "cargo test -p alice-lol --test degenerate_grid"

# ── M3: the lexer's non-finite-literal rejection removed (an overflowing
# literal like 1e39 reads as +inf again instead of a parse error) --
# expected red via tests/finite_numbers.rs's crash-input regression test
# (CRASH_EMIT), not the grid (accepting the token does not itself crash
# anything; it is the emit round-trip that notices an un-writable value)
run_mutant "M3" \
  "the lexer's finite-literal check removed (non-finite literals parse again)" \
  "
p = '$WT/alice-lol/src/runtime_parser.rs'
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
  "cargo test -p alice-lol --test finite_numbers"

# ── M4: ShelfDividerSpec::validate() skipped (hex_hole_pitch's layer-2
# check deleted) -- expected red via the named unit test, NOT the grid:
# shelf_divider is only ever called from .lol text with the fixed
# field_tested_560x250x120() spec (hex_hole_pitch always 20.0, never
# degenerate), so this hazard is unreachable from the parser-facing grid by
# design; only a direct Rust caller of the Spec API can trigger it
run_mutant "M4" \
  "ShelfDividerSpec::validate() skipped (layer-2 pitch check deleted)" \
  "
p = '$WT/alice-lol/src/stdlib/hardsurface/pattern_sdf.rs'
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
  "cargo test -p alice-lol --lib -- shelf_divider_spec_validate_refuses_a_zero_pitch"

# ── M5: GridfinitySpec::validate() itself deleted (not bypassed at the
# parser like M1 -- the check is GONE, so nothing downstream still guards
# it either) -- expected red via the grid's allocation cap (exit 42, not a
# panic this time: construction actually proceeds and tries to build the
# ~474 MB eager Vec, hitting the probe's 256 MB counting-allocator limit
# before it finishes)
run_mutant "M5" \
  "GridfinitySpec::validate() deleted entirely (unbounded eager allocation)" \
  "
p = '$WT/alice-lol/src/stdlib/hardsurface/pattern_sdf.rs'
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
  "cargo test -p alice-lol --test degenerate_grid"

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
