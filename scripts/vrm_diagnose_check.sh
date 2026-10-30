#!/usr/bin/env bash
# Run the vrm_diagnose example on the synthetic VRM files of alice-lol-humanoid's tests and assert
# its exit status: 0 for the clean rig, 1 for the rig with a finding, 2 for a file that does not
# exist. Fails on any other status and when fewer cases ran than listed (0 compared is a failure).
# The files are pinned to the rigs built in tests/vrm_diagnostics.rs by a test there.
#
# usage: scripts/vrm_diagnose_check.sh        (from the workspace root; used by ci.yml and preflight)
set -uo pipefail
data=alice-lol-humanoid/tests/data
cargo build -q -p alice-lol-humanoid --features vrm --example vrm_diagnose || exit 1
cases=("$data/vrm_clean.glb 0" "$data/vrm_hips_on_root.glb 1" "$data/does_not_exist.glb 2")
ran=0
fail=0
for c in "${cases[@]}"; do
  file=${c% *}
  want=${c##* }
  cargo run -q -p alice-lol-humanoid --features vrm --example vrm_diagnose -- "$file"
  got=$?
  ran=$((ran + 1))
  if [ "$got" != "$want" ]; then
    echo "error: vrm_diagnose $file exited $got, expected $want" >&2
    fail=1
  fi
done
echo "vrm_diagnose cases run: $ran of ${#cases[@]}"
if [ "$ran" -ne "${#cases[@]}" ] || [ "$ran" -eq 0 ]; then
  echo "error: not every case ran" >&2
  exit 1
fi
exit "$fail"
