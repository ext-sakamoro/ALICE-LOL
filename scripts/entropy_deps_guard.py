#!/usr/bin/env python3
"""The published crates depend on no source of entropy or wall-clock time (ALICE-Law G10).

A Law-bearing value must not depend on an unseeded RNG, a freshly generated UUID or an ambient
wall-clock read. The std forms (`Instant::now`, `SystemTime::now`, `.elapsed()`, `RandomState`)
are forbidden in the library code by `.github/clippy-lib/clippy.toml` (scripts/clippy_lib_gate.py
shows each entry fires). The other forms come from crates — `rand` and its family, `getrandom`,
`uuid`, `fastrand`, `chrono`, `time` — and Rust code can only call a crate its manifest depends on
directly, so this checks the manifests instead of scanning text: no workspace member that is
published (`publish` not false) may list one of them as a normal or build dependency, under any
target or feature. Dev-dependencies are the tests' and do not reach the published crate.

It reads `cargo metadata --no-deps` (no resolution, no network). It fails when a published member
lists a denied crate, and when no published member is read (compared nothing).

    python3 scripts/entropy_deps_guard.py [--manifest-path Cargo.toml]
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys

# a crate whose name matches is a source of entropy or of the wall clock
DENIED = re.compile(r"(?:rand(?:_[a-z0-9]+)?|getrandom|uuid|fastrand|chrono|time|oorandom|nanorand)")


def denied_deps(metadata: dict) -> tuple[list[str], list[str]]:
    """(the published members read, one problem per denied dependency)"""
    members = set(metadata.get("workspace_members", []))
    read, problems = [], []
    for pkg in metadata.get("packages", []):
        if pkg.get("id") not in members:
            continue
        if pkg.get("publish") == []:
            continue  # (publish = false: not a published crate)
        read.append(pkg["name"])
        for dep in pkg.get("dependencies", []):
            if dep.get("kind") == "dev":
                continue
            name = dep.get("rename") or dep["name"]
            if DENIED.fullmatch(dep["name"]):
                where = f" (target {dep['target']})" if dep.get("target") else ""
                kind = dep.get("kind") or "normal"
                problems.append(f"{pkg['name']}: {kind} dependency `{name}` ({dep['name']}){where} is a source of "
                                "entropy or wall-clock time (ALICE-Law G10): take the randomness / time as an input")
    return read, problems


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--manifest-path", default="Cargo.toml")
    a = ap.parse_args(argv)
    p = subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1", "--manifest-path", a.manifest_path],
                       capture_output=True, text=True)
    if p.returncode != 0:
        print(f"entropy-deps-guard: cargo metadata failed:\n{p.stderr}", file=sys.stderr)
        return 2
    read, problems = denied_deps(json.loads(p.stdout))
    for e in problems:
        print(f"error: {e}", file=sys.stderr)
    if not read:
        print("entropy-deps-guard: no published workspace member read (compared nothing)", file=sys.stderr)
        return 2
    print(f"entropy-deps-guard: {len(read)} published member(s) ({', '.join(sorted(read))}), "
          f"{len(problems)} denied dependenc{'y' if len(problems) == 1 else 'ies'}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
