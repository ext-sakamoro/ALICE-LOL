#!/usr/bin/env bash
# scripts/preflight.sh — local reproduction of the CI gates before `git push`.
# Every command is the one CI runs. A step this file does not cover is a step
# that can only fail remotely — when a workflow step is added, add it here in
# the same commit. Run `--quick` before every push.
#
# usage: scripts/preflight.sh [--quick]   (--quick skips the test / bench suites)
set -euo pipefail
cd "$(dirname "$0")/.."

quick=0
[[ "${1:-}" == "--quick" ]] && quick=1

step() { printf '\n\033[1;34m== %s\033[0m\n' "$*"; }
# `cargo clippy` reuses fresh `cargo check` artifacts and then lints nothing;
# touching the crate roots invalidates only this repo's fingerprints.
relint() { git ls-files | grep -E '(^|/)src/(lib|main)\.rs$' | xargs -r touch; }
need() { command -v "$1" >/dev/null 2>&1 || { echo "missing tool: $1 ($2)" >&2; exit 1; }; }
has_toolchain() { rustup toolchain list | grep -q "^$1"; }

# Steps CI runs that this file cannot reproduce locally (they can only fail remotely):
#   - ci.yml:gpu-parity:Install Mesa software Vulkan (lavapipe) (needs network / runner-only)
#   - security-audit.yml:audit:Install cargo-audit (needs network / runner-only)
#   - security-audit.yml:deny:Install cargo-deny (needs network / runner-only)
#   - security-audit.yml:coverage (job is continue-on-error: informational in CI)
#   - security-audit.yml:semver-checks (job is continue-on-error: informational in CI)
#   - fuzz.yml:fuzz:Create SDF dependency stubs [target=fuzz_lol_parse] (no cargo / grep)
#   - fuzz.yml:fuzz:Install cargo-fuzz [target=fuzz_lol_parse] (needs network / runner-only)
#   - fuzz.yml:fuzz:Set fuzz duration [target=fuzz_lol_parse] (no cargo / grep)
#   - fuzz.yml:fuzz:Run fuzz target (time-boxed) [target=fuzz_lol_parse] (continue-on-error)
#   - fuzz.yml:fuzz:Report crash (informational) [target=fuzz_lol_parse] (no cargo / grep)
#   - fuzz.yml:fuzz:Create SDF dependency stubs [target=fuzz_lol_to_wgsl] (no cargo / grep)
#   - fuzz.yml:fuzz:Install cargo-fuzz [target=fuzz_lol_to_wgsl] (needs network / runner-only)
#   - fuzz.yml:fuzz:Set fuzz duration [target=fuzz_lol_to_wgsl] (no cargo / grep)
#   - fuzz.yml:fuzz:Run fuzz target (time-boxed) [target=fuzz_lol_to_wgsl] (continue-on-error)
#   - fuzz.yml:fuzz:Report crash (informational) [target=fuzz_lol_to_wgsl] (no cargo / grep)
#   - fuzz.yml:fuzz:Create SDF dependency stubs [target=fuzz_lol_eval] (no cargo / grep)
#   - fuzz.yml:fuzz:Install cargo-fuzz [target=fuzz_lol_eval] (needs network / runner-only)
#   - fuzz.yml:fuzz:Set fuzz duration [target=fuzz_lol_eval] (no cargo / grep)
#   - fuzz.yml:fuzz:Run fuzz target (time-boxed) [target=fuzz_lol_eval] (continue-on-error)
#   - fuzz.yml:fuzz:Report crash (informational) [target=fuzz_lol_eval] (no cargo / grep)
#   - fuzz.yml:fuzz:Create SDF dependency stubs [target=fuzz_lol_emit_parity] (no cargo / grep)
#   - fuzz.yml:fuzz:Install cargo-fuzz [target=fuzz_lol_emit_parity] (needs network / runner-only)
#   - fuzz.yml:fuzz:Set fuzz duration [target=fuzz_lol_emit_parity] (no cargo / grep)
#   - fuzz.yml:fuzz:Run fuzz target (time-boxed) [target=fuzz_lol_emit_parity] (continue-on-error)
#   - fuzz.yml:fuzz:Report crash (informational) [target=fuzz_lol_emit_parity] (no cargo / grep)

need actionlint "brew install actionlint"
need cargo-audit "cargo install cargo-audit --locked"
need cargo-deny "cargo install cargo-deny --locked"
need cargo-machete "cargo install cargo-machete --locked"
has_toolchain 1.90 || { echo "missing toolchain 1.90 (rustup toolchain install 1.90)" >&2; exit 1; }

step "ci.yml / test: Build"
( export CARGO_TERM_COLOR="always"; cargo build )

step "ci.yml / test: Build examples"
( export CARGO_TERM_COLOR="always"; cargo build --examples )

step "ci.yml / test: Build [features=--features llm-bridge]"
( export CARGO_TERM_COLOR="always"; cargo build --features llm-bridge )

step "ci.yml / test: Build examples [features=--features llm-bridge]"
( export CARGO_TERM_COLOR="always"; cargo build --examples --features llm-bridge )

step "ci.yml / test: Build [features=--features glsl,wgsl,hlsl]"
( export CARGO_TERM_COLOR="always"; cargo build --features glsl,wgsl,hlsl )

step "ci.yml / test: Build examples [features=--features glsl,wgsl,hlsl]"
( export CARGO_TERM_COLOR="always"; cargo build --examples --features glsl,wgsl,hlsl )

step "ci.yml / test: Build [features=--features physics]"
( export CARGO_TERM_COLOR="always"; cargo build --features physics )

step "ci.yml / test: Build examples [features=--features physics]"
( export CARGO_TERM_COLOR="always"; cargo build --examples --features physics )

step "ci.yml / test: Build [features=--features roblox]"
( export CARGO_TERM_COLOR="always"; cargo build --features roblox )

step "ci.yml / test: Build examples [features=--features roblox]"
( export CARGO_TERM_COLOR="always"; cargo build --examples --features roblox )

step "ci.yml / clippy: Clippy"
relint
( export CARGO_TERM_COLOR="always"; cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::pedantic -D clippy::nursery )

step "ci.yml / fmt: Check formatting"
( export CARGO_TERM_COLOR="always"; cargo fmt -- --check )

step "ci.yml / wiring-guard: oracle + 新規の未配線 / 理由の無い dead_code が無い"
python3 scripts/test_wiring_guard.py
python3 scripts/wiring_guard.py

step "ci.yml / wiring-guard: oracle + alice-* が 2 版以上 lock に入っていない"
python3 scripts/test_lock_single_version.py
python3 scripts/lock_single_version.py
python3 scripts/macro_kinds.py --check
python3 scripts/test_license_check.py
python3 scripts/test_fuzz_runs.py
python3 scripts/license_check.py
python3 scripts/test_published_source_check.py

step "ci.yml / wiring-guard: oracle + 判定経路に platform 依存の超越関数が無い"
python3 scripts/test_det_math_guard.py
python3 scripts/det_math_guard.py

step "ci.yml / docs: readme_sync + docs_lint (oracle + check)"
python3 scripts/test_readme_sync.py
python3 scripts/readme_sync.py --check
python3 scripts/test_docs_lint.py
python3 scripts/docs_lint.py --check

step "ci.yml / docs: law_corners (oracle + corner enumeration)"
python3 scripts/test_law_corners.py
python3 scripts/law_corners.py laws/spike

step "ci.yml / docs: law_ambiguity_lint (method scope, range provenance, language neutrality)"
python3 scripts/test_line_split_guard.py
python3 scripts/line_split_guard.py
python3 scripts/test_workflow_concurrency.py
python3 scripts/workflow_concurrency.py
python3 scripts/test_workflow_timeouts.py
python3 scripts/workflow_timeouts.py
python3 scripts/test_history_free_tests.py
python3 scripts/history_free_tests.py
python3 scripts/test_law_ambiguity_lint.py
python3 scripts/law_ambiguity_lint.py laws/spike
step "ci.yml / docs: law_schema (oracle + every x-input type) and probes"
python3 scripts/test_law_schema.py
python3 scripts/law_schema.py laws/spike
# the reference implementation needs mpmath (correctly rounded elementary functions):
# a venv under target/ with conformance/requirements.txt, made once
conf_py=target/conformance-venv/bin/python
if ! "$conf_py" -c "import mpmath" 2>/dev/null; then
  python3 -m venv target/conformance-venv
  "$conf_py" -m pip install -q -r conformance/requirements.txt
fi
"$conf_py" conformance/test_ref_finite.py
"$conf_py" conformance/check_probes.py -- "$conf_py" conformance/ref_impl.py
cargo build -q -p alice-lol --example audit_conformance
python3 conformance/check_probes.py --laws gate_compares_nonzero,identifier_feature_independent,no_such_law,decl_probe,floor_probe -- target/debug/examples/audit_conformance
python3 conformance/law_line_fuzz.py --pairs 150 -- python3 conformance/ref_impl.py -- target/debug/examples/audit_conformance

step "ci.yml / msrv: Check (workspace, all features)"
( export CARGO_TERM_COLOR="always"; cargo +1.90 check --workspace --all-targets --all-features )

step "ci.yml / actionlint: actionlint"
actionlint .github/workflows/*.yml

step "security-audit.yml / deny: Run cargo deny check all"
( export CARGO_TERM_COLOR="always" CARGO_NET_RETRY="5" CARGO_HTTP_MULTIPLEXING="false"; cargo deny --all-features check all )

step "security-audit.yml / unused-deps: Run cargo machete"
( export CARGO_TERM_COLOR="always" CARGO_NET_RETRY="5" CARGO_HTTP_MULTIPLEXING="false"; cargo machete )

step "security-audit.yml / stub-guard: Detect panic!(STUB) in src/** (blocking)"
(
  export CARGO_TERM_COLOR="always" CARGO_NET_RETRY="5" CARGO_HTTP_MULTIPLEXING="false"
  set -eo pipefail
  hits=$(grep -rnE 'panic!\([^)]*STUB' \
    . --include="*.rs" \
    --exclude-dir=bin \
    --exclude-dir=target \
    --exclude-dir=fuzz \
    || true)
  if [ -n "$hits" ]; then
    echo "❌ STUB panic detected in production path:"
    echo "$hits"
    exit 1
  fi
  echo "✓ No panic!(STUB) in workspace src/"
)

step "security-audit.yml / stub-guard: Detect dbg!() residual in src/**"
(
  export CARGO_TERM_COLOR="always" CARGO_NET_RETRY="5" CARGO_HTTP_MULTIPLEXING="false"
  set -eo pipefail
  hits=$(grep -rn 'dbg!(' . --include="*.rs" \
    --exclude-dir=target --exclude-dir=fuzz || true)
  if [ -n "$hits" ]; then
    echo "❌ dbg!() macro left in src/:"
    echo "$hits"
    exit 1
  fi
  echo "✓ No dbg!() in workspace"
)

step "security-audit.yml / stub-guard: Detect TODO / FIXME / XXX / HACK (informational)"
(
  export CARGO_TERM_COLOR="always" CARGO_NET_RETRY="5" CARGO_HTTP_MULTIPLEXING="false"
  set -eo pipefail
  hits=$(grep -rnE 'TODO|FIXME|XXX|HACK' . --include="*.rs" \
    --exclude-dir=target --exclude-dir=fuzz || true)
  if [ -n "$hits" ]; then
    echo "::warning::TODO/FIXME/XXX/HACK found (informational, not blocking):"
    echo "$hits" | head -50
  else
    echo "✓ No TODO/FIXME/XXX/HACK in workspace"
  fi
)

step "fuzz.yml / fuzz: Build fuzz target [target=fuzz_lol_parse]"
if has_toolchain nightly && cargo +nightly fuzz --version >/dev/null 2>&1; then
  (
    export CARGO_TERM_COLOR="always"
    cd fuzz
    cargo +nightly fuzz build "fuzz_lol_parse"
  )
else
  echo "skip: nightly / cargo-fuzz not installed" >&2
fi

step "fuzz.yml / fuzz: Build fuzz target [target=fuzz_lol_to_wgsl]"
if has_toolchain nightly && cargo +nightly fuzz --version >/dev/null 2>&1; then
  (
    export CARGO_TERM_COLOR="always"
    cd fuzz
    cargo +nightly fuzz build "fuzz_lol_to_wgsl"
  )
else
  echo "skip: nightly / cargo-fuzz not installed" >&2
fi

step "fuzz.yml / fuzz: Build fuzz target [target=fuzz_lol_eval]"
if has_toolchain nightly && cargo +nightly fuzz --version >/dev/null 2>&1; then
  (
    export CARGO_TERM_COLOR="always"
    cd fuzz
    cargo +nightly fuzz build "fuzz_lol_eval"
  )
else
  echo "skip: nightly / cargo-fuzz not installed" >&2
fi

step "fuzz.yml / fuzz: Build fuzz target [target=fuzz_lol_emit_parity]"
if has_toolchain nightly && cargo +nightly fuzz --version >/dev/null 2>&1; then
  (
    export CARGO_TERM_COLOR="always"
    cd fuzz
    cargo +nightly fuzz build "fuzz_lol_emit_parity"
  )
else
  echo "skip: nightly / cargo-fuzz not installed" >&2
fi

# every law test file is listed in scripts/law_tests.sh (cheap: no test runs)
step "ci.yml / test: law test files are listed"
scripts/law_tests.sh list

if [[ $quick -eq 1 ]]; then
  echo; echo "preflight --quick OK (test / bench suites skipped)"; exit 0
fi

step "ci.yml / test: Test"
( export CARGO_TERM_COLOR="always"; cargo test )

step "ci.yml / test: Law oracles (each target ran at least one test)"
( export CARGO_TERM_COLOR="always"; scripts/law_tests.sh default )

step "ci.yml / test: Test [features=--features llm-bridge]"
( export CARGO_TERM_COLOR="always"; cargo test --features llm-bridge )

step "ci.yml / test: Test [features=--features glsl,wgsl,hlsl]"
( export CARGO_TERM_COLOR="always"; cargo test --features glsl,wgsl,hlsl )

step "ci.yml / test: Test [features=--features physics]"
( export CARGO_TERM_COLOR="always"; cargo test --features physics )

step "ci.yml / test: Law oracles (physics, each target ran at least one test)"
( export CARGO_TERM_COLOR="always"; scripts/law_tests.sh physics )

step "ci.yml / test: Test [features=--features roblox]"
( export CARGO_TERM_COLOR="always"; cargo test --features roblox )

step "ci.yml / test: Test (roblox oracle, named)"
( export CARGO_TERM_COLOR="always"; cargo test --features roblox --test analytic_roblox_export )

step "ci.yml / gpu-parity: GPU ↔ CPU parity (grammar corpus + fixtures)"
( export CARGO_TERM_COLOR="always" ALICE_SDF_REQUIRE_GPU="1" WGPU_BACKEND="vulkan"; cargo test -p alice-lol --features wgsl --test gpu_parity -- --nocapture )

step "security-audit.yml / audit: Run cargo audit"
( export CARGO_TERM_COLOR="always" CARGO_NET_RETRY="5" CARGO_HTTP_MULTIPLEXING="false"; cargo audit --db "${CARGO_TARGET_DIR:-target}/advisory-db" --deny yanked --ignore RUSTSEC-2025-0141 --ignore RUSTSEC-2024-0436 )

step "ci.yml / registry-consumer: published source + every macro keyword against crates.io"
python3 scripts/published_source_check.py
python3 scripts/license_check.py --package
REGISTRY_CONSUMER_ALLOW_DIRTY=1 scripts/registry_consumer.sh

echo; echo "preflight OK"
