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

step "ci.yml / clippy: Identifier moves (tests, then against the published release)"
step "eol.yml / line endings (tests, then check)"
# text files are LF in the index and the working tree (.github/workflows/eol.yml, shared by the ALICE crates)
python3 scripts/test_check_eol.py
python3 scripts/check_eol.py
python3 scripts/test_id_move_check.py
python3 scripts/id_move_check.py

step "ci.yml / clippy: Clippy"
relint
( export CARGO_TERM_COLOR="always"; cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::pedantic -D clippy::nursery )

step "ci.yml / fmt: Check formatting"
( export CARGO_TERM_COLOR="always"; cargo fmt -- --check )

step "ci.yml / wiring-guard: oracle + 新規の未配線 / 理由の無い dead_code が無い"
python3 scripts/test_wiring_guard.py
python3 scripts/wiring_guard.py

step "ci.yml / parser-no-direct-validating-builders: oracle + parser は validate() 付き infallible builder を直接呼ばない"
python3 scripts/test_parser_no_direct_validating_builders.py
python3 scripts/parser_no_direct_validating_builders.py

step "ci.yml / runtime-keywords: oracle + parser の keyword dispatch と syntax_table.rs の SDF_SYNTAX が一致"
python3 scripts/test_runtime_keywords.py
python3 scripts/runtime_keywords.py

step "ci.yml / gen-gbnf: oracle + lol.gbnf と配布用の写しが language/constructs.toml から生成したものと一致"
python3 scripts/test_gen_gbnf.py
python3 scripts/gen_gbnf.py --check

step "ci.yml / gen-units-doc: oracle + 文書の単位の段落が language/constructs.toml の unit 表から生成したものと一致"
python3 scripts/test_gen_units_doc.py
python3 scripts/gen_units_doc.py --check

step "ci.yml / wiring-guard: oracle + alice-* が 2 版以上 lock に入っていない"
python3 scripts/test_lock_single_version.py
python3 scripts/lock_single_version.py
python3 scripts/macro_kinds.py --check
python3 scripts/g7_macro_pairing.py
python3 scripts/test_release_roster.py
python3 scripts/release_roster.py
python3 scripts/test_release_siblings.py
python3 scripts/release_siblings.py
python3 scripts/test_alice_siblings_check.py
python3 scripts/alice_siblings_check.py
python3 scripts/test_entropy_deps_guard.py
python3 scripts/entropy_deps_guard.py
python3 scripts/test_release_leg_check.py
python3 scripts/release_leg_check.py
python3 scripts/test_root_artifacts.py
python3 scripts/root_artifacts.py
python3 scripts/test_license_check.py
python3 scripts/test_fuzz_runs.py
python3 scripts/license_check.py
python3 scripts/test_published_source_check.py
python3 scripts/test_package_integrity.py

step "law-readme-block.yml: README ALICE-Law block (tests, then the check the job runs, at the block's ALICE-Law pin)"
python3 scripts/test_law_readme_block_check.py
python3 scripts/law_readme_block_check.py

step "ci.yml / wiring-guard: oracle + 判定経路に platform 依存の超越関数が無い"
python3 scripts/test_det_math_guard.py
python3 scripts/det_math_guard.py

step "ci.yml / fma-guard: oracle + Law code に mul_add の結果全体を negate する形が無い (syn AST、主検査)"
cargo fmt --manifest-path tools/no-outer-fneg-of-fma/Cargo.toml -- --check
cargo test --quiet --locked --manifest-path tools/no-outer-fneg-of-fma/Cargo.toml
cargo run --quiet --locked --release --manifest-path tools/no-outer-fneg-of-fma/Cargo.toml -- --root .

step "ci.yml / fma-guard: 同上、regex 版 (副検査、安価な独立経路として併走)"
python3 scripts/test_no_outer_fneg_of_fma.py
python3 scripts/no_outer_fneg_of_fma.py

step "ci.yml / clippy-lib: lib 専用 clippy 設定の生成物一致 + canary"
python3 scripts/test_clippy_lib_gate.py
python3 scripts/clippy_lib_gate.py
python3 scripts/test_hash_iter_canary.py
python3 scripts/hash_iter_canary.py

step "ci.yml / clippy-lib: alice-lol (lib のみ、examples/tests/benches は既存どおり std のまま許容)"
CLIPPY_CONF_DIR=.github/clippy-lib cargo clippy --quiet -p alice-lol --lib --all-features -- -D clippy::disallowed_methods -D clippy::disallowed_macros -D clippy::disallowed_types -D clippy::iter_over_hash_type

step "ci.yml / clippy-lib: alice-lol 以外の workspace 全体 (lib+bins+examples、既存の除外実績が無いので production 相当を広く取る)"
CLIPPY_CONF_DIR=.github/clippy-lib cargo clippy --quiet --workspace --exclude alice-lol --lib --bins --examples --all-features -- -D clippy::disallowed_methods -D clippy::disallowed_macros -D clippy::disallowed_types -D clippy::iter_over_hash_type

step "ci.yml / det-math-minmax-blob-sync: alice-lol の det_math max/min が alice-physics の実装と一致する"
python3 scripts/test_det_math_minmax_blob_sync.py
python3 scripts/det_math_minmax_blob_sync.py

step "ci.yml / disallowed-methods-allow-reasons: 個別 allow の reason が既知 prefix + 隠れた libm 呼び出し無し"
python3 scripts/test_disallowed_methods_allow_reasons.py
python3 scripts/disallowed_methods_allow_reasons.py

step "ci.yml / raw-minmax-manifest: raw_minmax_here!() を呼ぶ module を一覧する (G2、drift 検知)"
python3 scripts/test_raw_minmax_manifest.py
python3 scripts/raw_minmax_manifest.py


step "ci.yml / libm-hidden-check: disallowed_methods の allow を全部外し、libm が src/ scope に漏れていないか clippy で検査"
python3 scripts/test_libm_hidden_check.py
python3 scripts/libm_hidden_check.py

step "ci.yml / docs: readme_sync + doc_numbers + docs_lint + tracked_binaries (oracle + check)"
python3 scripts/test_readme_sync.py
python3 scripts/readme_sync.py --check
python3 scripts/test_doc_numbers.py
python3 scripts/doc_numbers.py
python3 scripts/test_docs_lint.py
python3 scripts/docs_lint.py --check
python3 scripts/test_tracked_binaries.py
python3 scripts/tracked_binaries.py

step "ci.yml / docs: libm_dependency_guard (oracle + check)"
python3 scripts/test_libm_dependency_guard.py
python3 scripts/libm_dependency_guard.py

step "ci.yml / docs: law_corners (oracle + corner enumeration)"
python3 scripts/test_law_corners.py
python3 scripts/law_corners.py laws/spike

step "ci.yml / docs: law_ambiguity_lint (method scope, range provenance, language neutrality)"
python3 scripts/test_line_split_guard.py
python3 scripts/line_split_guard.py
python3 scripts/test_workflow_concurrency.py
python3 scripts/workflow_concurrency.py
python3 scripts/test_changed_paths.py
python3 scripts/changed_paths.py sync
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
python3 conformance/test_check_probes.py
python3 conformance/test_run_conformance.py
"$conf_py" conformance/check_probes.py --kind reader -- "$conf_py" conformance/ref_impl.py
cargo build -q -p alice-lol --example audit_conformance
python3 conformance/check_probes.py --kind reader --laws gate_compares_nonzero,identifier_feature_independent,lint_bans_include,no_such_law,decl_probe,floor_probe -- target/debug/examples/audit_conformance
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
step "ci.yml / wiring-guard: Build configuration guard (G14, tests and check)"
python3 scripts/test_build_config_guard.py
python3 scripts/build_config_guard.py
python3 scripts/test_build_flags_check.py
python3 scripts/test_run_tests.py
python3 scripts/test_cargo_test_nonzero.py
python3 scripts/test_sdf_writers.py
python3 scripts/test_g7_boundaries.py

step "ci.yml / sdf-writers: alice-sdf writers (G8) and type-erasing boundaries (G7)"
nightly=$(python3 -c 'import tomllib; print(tomllib.load(open("scripts/sdf_writers.toml", "rb"))["nightly"])')
if has_toolchain "$nightly"; then
  python3 scripts/sdf_writers.py
  python3 scripts/g7_boundaries.py
else
  echo "skip: $nightly not installed (rustup toolchain install $nightly); CI runs it" >&2
fi

step "ci.yml / wiring-guard: Shared files equal ALICE-Law's table (G11 (c))"
python3 scripts/test_shared_files.py
scripts/shared_files.sh

step "ci.yml / wiring-guard: G11 table matches the declared gates (G11)"
scripts/g11_table.sh

step "ci.yml / wiring-guard: Effective rustc flags (G14)"
python3 scripts/build_flags_check.py --crate alice_lol -- cargo check --release --lib -p alice-lol
python3 scripts/build_flags_check.py --crate alice_lol -- cargo check --release --lib -p alice-lol --all-features
python3 scripts/build_flags_check.py --crate alice_lol -- cargo check --release --lib -p alice-lol --no-default-features

step "ci.yml / geometry-digest: Float environment (G14, debug and release)"
scripts/run_tests.sh float_environment=2 -- cargo test -p alice-lol --test float_environment
scripts/run_tests.sh float_environment=2 -- cargo test -p alice-lol --release --test float_environment

step "ci.yml / test: law test files are listed"
scripts/law_tests.sh list

step "public-text.yml / public-text: Public text (ALICE-Law checker at the pin; tree, then the commits)"
python3 scripts/test_public_text.py
scripts/public_text.sh

if [[ $quick -eq 1 ]]; then
  echo
  echo "preflight --quick OK -- the following CI steps were NOT run (full preflight,"
  echo "no --quick, runs all of them):"
  echo "  - cargo test (default + llm-bridge/glsl,wgsl,hlsl/physics/roblox features)"
  echo "  - law oracles (default + physics)"
  echo "  - release-profile.yml: law oracles under --release (default + physics)"
  echo "  - geometry-digest: identifier and digest oracles (debug + release)"
  echo "  - gpu-parity (CPU/GPU grammar-corpus parity)"
  echo "  - doc-programs (export of every lol print program in the documents, release)"
  echo "  - degenerate-grid (~35,000-case parameter sweep, ubuntu CI job only --"
  echo "    a count/assertion here that is wrong locally still shows green)"
  echo "A push that changes the grammar (SDF_SYNTAX / the macro parser / the gbnf)"
  echo "needs the full run at least once: these are exactly the gates most likely"
  echo "to catch a count that drifted from the new construct (rotate_q's own"
  echo "landing missed degenerate_grid.rs's hard-coded SDF-name count this way,"
  echo "since --quick never reaches that assertion)."
  exit 0
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

step "release-profile.yml / release: Law tests (release, default + physics)"
( export CARGO_TERM_COLOR="always"; scripts/law_tests.sh default --release && scripts/law_tests.sh physics --release )

for prof in "" "--release"; do
  step "ci.yml / geometry-digest: Geometry identifier and digest oracles ${prof:-(debug)}"
  scripts/cargo_test_nonzero.sh -p alice-lol $prof --lib geometry_id
  scripts/cargo_test_nonzero.sh -p alice-lol $prof --lib quat_from_euler_xyz_is_bit_identical_across_architectures
  scripts/cargo_test_nonzero.sh -p alice-lol $prof --lib y_component_cancellation_preserves_a_negative_zero_input
  scripts/run_tests.sh geometry_id_oracle=4,stdlib_profile_determinism_oracle,laser_pattern_digest_golden,golden_compare=9 -- \
    cargo test -p alice-lol $prof --test geometry_id_oracle --test stdlib_profile_determinism_oracle \
    --test laser_pattern_digest_golden --test golden_compare
done

step "ci.yml / test: Test [features=--features roblox]"
( export CARGO_TERM_COLOR="always"; cargo test --features roblox )

step "ci.yml / test: Test (roblox oracle, named)"
( export CARGO_TERM_COLOR="always"; cargo test --features roblox --test analytic_roblox_export )

step "ci.yml / gpu-parity: GPU ↔ CPU parity (grammar corpus + fixtures)"
( export CARGO_TERM_COLOR="always" ALICE_SDF_REQUIRE_GPU="1" WGPU_BACKEND="vulkan"; cargo test -p alice-lol --features wgsl --test gpu_parity -- --nocapture )

step "ci.yml / doc-programs: Programs in the documents (parse, grammar, closed export under 1M triangles, resolved at the export resolution)"
( export CARGO_TERM_COLOR="always"; scripts/cargo_test_nonzero.sh --release -p alice-lol --features llm-bridge --test doc_programs -- --include-ignored )

step "ci.yml / doc-programs: Argument kinds (an H argument grows the shape by twice its change, an L argument does not)"
( export CARGO_TERM_COLOR="always"; scripts/cargo_test_nonzero.sh --release -p alice-lol --test args_kinds -- --include-ignored )

step "ci.yml / degenerate-grid: Degenerate-parameter grid (one-at-a-time / all-same / pairwise, ~35,000 cases, ~90s)"
( export CARGO_TERM_COLOR="always"; cargo test -p alice-lol --test degenerate_grid -- --ignored --nocapture )

step "security-audit.yml / audit: Run cargo audit"
( export CARGO_TERM_COLOR="always" CARGO_NET_RETRY="5" CARGO_HTTP_MULTIPLEXING="false"; cargo audit --db "${CARGO_TARGET_DIR:-target}/advisory-db" --deny yanked --ignore RUSTSEC-2025-0141 --ignore RUSTSEC-2024-0436 )

step "ci.yml / registry-consumer: published source + every macro keyword against crates.io"
python3 scripts/published_source_check.py
python3 scripts/license_check.py --package
python3 scripts/package_integrity.py --member alice-lol
REGISTRY_CONSUMER_ALLOW_DIRTY=1 scripts/registry_consumer.sh
python3 scripts/package_integrity.py --member alice-lol --patches > target/patches-fresh.txt
diff -u target/patches-fresh.txt target/registry-consumer/patches.txt

step "conformance.yml / corpus: score the reference implementation (corpus + probes) and the controls"
( export PYTHON="$conf_py" MIN_VECTORS="$(sed -n 's/^ *MIN_VECTORS: *//p' .github/workflows/conformance.yml)"
  conformance/score.sh --kind reader "$conf_py" conformance/ref_impl.py
  conformance/controls.sh )

step "semver-checks.yml / semver-checks: semver gate (tests)"
python3 scripts/test_semver_gate.py

step "semver-checks.yml / semver-checks: semver gate (alice-lol, latest crates.io release, every consumer feature set)"
need cargo-semver-checks "cargo install cargo-semver-checks --version 0.50.0 --locked"
python3 scripts/semver_gate.py --manifest alice-lol/Cargo.toml --config .github/semver-gate.toml --target-dir "${CARGO_TARGET_DIR:-target}/semver"

step "g3-ir.yml / ir: probe tests + no negated fma result in alice-lol's release IR (host build; the per-target floors are checked in CI)"
python3 scripts/test_fneg_fma_ir_check.py
python3 scripts/fneg_fma_ir_check.py --package alice-lol --all-features --findings-only

echo; echo "preflight OK"
