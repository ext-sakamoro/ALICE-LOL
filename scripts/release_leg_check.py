#!/usr/bin/env python3
"""The release-profile leg of the Law tests is in place (ALICE-Law G5).

.github/workflows/release-profile.yml runs the Law oracle selection (scripts/law_tests.sh) under the
release profile on the aarch64 runner. This fails when that job could be lost without a red run:

  * the `release` job does not run on macos-latest (the aarch64 runner),
  * a step does not run `scripts/law_tests.sh default --release` / `scripts/law_tests.sh physics --release`
    (a missing selection, or one without --release),
  * the paths filter (.github/paths/release-profile.txt) lacks a pattern `<dir>/**` for a workspace member
    the Law tests build: every package scripts/law_tests.sh names and, through path dependencies, every
    workspace member those depend on (read from cargo metadata, so a new dependency is required at once),
    or lacks Cargo.toml, Cargo.lock, the sibling pin list, this workflow, its pattern file or the scripts
    it runs.

  * scripts/law_tests.sh does not check, when given --release, that cargo built the release profile for every
    target (the line that reads "Finished `release` profile" back from cargo's output); without it a lost
    argument would turn the release leg into a second debug run that still passes.

It reads no step at all (no `release` job, or no law_tests.sh step in it) as compared nothing: exit 2.

    python3 scripts/release_leg_check.py
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = ".github/workflows/release-profile.yml"
PATTERNS = ".github/paths/release-profile.txt"
SELECTIONS = ("default", "physics")
FIXED = ("Cargo.toml", "Cargo.lock", ".github/alice-siblings", WORKFLOW, PATTERNS, "scripts/law_tests.sh",
         "scripts/cargo_test_nonzero.sh", "scripts/release_leg_check.py")


def release_job(workflow: str) -> str | None:
    """the text of the `release:` job (two-space indented key under `jobs:`), None when there is none"""
    m = re.search(r"^  release:\n((?:    .*\n|\s*\n)*)", workflow, re.M)
    return m.group(1) if m else None


def law_steps(job: str) -> list[str]:
    return [line.strip() for line in job.splitlines() if re.search(r"\bscripts/law_tests\.sh\b", line)
            and line.strip().startswith("run:")]


def required_dirs(root: Path, metadata: dict) -> list[str]:
    """the workspace members the Law tests build, as directories relative to the root"""
    text = (root / "scripts" / "law_tests.sh").read_text(encoding="utf-8")
    named = set(re.findall(r'^\s*"([A-Za-z0-9_-]+)\|', text, re.M))
    members = {p["id"]: p for p in metadata["packages"] if p["id"] in set(metadata["workspace_members"])}
    by_name = {p["name"]: p for p in members.values()}
    todo, seen = [n for n in named if n in by_name], set()
    while todo:
        name = todo.pop()
        if name in seen:
            continue
        seen.add(name)
        for dep in by_name[name].get("dependencies", []):
            if dep.get("path") and dep["name"] in by_name and dep.get("kind") != "dev":
                todo.append(dep["name"])
    missing_named = sorted(named - set(by_name))
    if missing_named:
        raise ValueError(f"scripts/law_tests.sh names packages that are not workspace members: {missing_named}")
    # (relative to the workspace root cargo reports, with / separators: the same on every platform)
    base = Path(metadata["workspace_root"]).resolve()
    return sorted(Path(by_name[n]["manifest_path"]).resolve().parent.relative_to(base).as_posix() for n in seen)


def problems(root: Path, metadata: dict) -> tuple[list[str], int]:
    """(problems, steps read)"""
    out = []
    job = release_job((root / WORKFLOW).read_text(encoding="utf-8"))
    if job is None:
        return [f"{WORKFLOW}: no `release` job"], 0
    steps = law_steps(job)
    if not re.search(r"^    runs-on:\s*macos-latest\s*$", job, re.M):
        out.append(f"{WORKFLOW}: the release job does not run on macos-latest (the aarch64 runner G5 requires)")
    for sel in SELECTIONS:
        if not any(re.search(rf"scripts/law_tests\.sh\s+{sel}\b.*--release\b", s) for s in steps):
            out.append(f"{WORKFLOW}: no step runs `scripts/law_tests.sh {sel} --release`")
    law = (root / "scripts" / "law_tests.sh").read_text(encoding="utf-8")
    if "Finished `release` profile" not in law or not re.search(r'cargo_test_nonzero\.sh .*\$\{extra\[@\]', law):
        out.append("scripts/law_tests.sh: does not forward its extra arguments to cargo and read the release profile "
                   "back from cargo's output")
    patterns = {line.strip() for line in (root / PATTERNS).read_text(encoding="utf-8").splitlines()
                if line.strip() and not line.startswith("#")}
    for d in required_dirs(root, metadata):
        if f"{d}/**" not in patterns:
            out.append(f"{PATTERNS}: no `{d}/**` (a workspace member the Law tests build)")
    for f in FIXED:
        if f not in patterns:
            out.append(f"{PATTERNS}: no `{f}`")
    return out, len(steps)


def main() -> int:
    p = subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1", "--manifest-path",
                        str(ROOT / "Cargo.toml")], capture_output=True, text=True)
    if p.returncode != 0:
        print(f"release-leg-check: cargo metadata failed:\n{p.stderr}", file=sys.stderr)
        return 2
    try:
        found, steps = problems(ROOT, json.loads(p.stdout))
    except (OSError, ValueError) as e:
        print(f"release-leg-check: {e}", file=sys.stderr)
        return 2
    for e in found:
        print(f"error: {e}", file=sys.stderr)
    if steps == 0:
        print("release-leg-check: no law_tests.sh step read in the release job (compared nothing)", file=sys.stderr)
        return 2
    print(f"release-leg-check: {steps} release step(s), {len(found)} problem(s)")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
