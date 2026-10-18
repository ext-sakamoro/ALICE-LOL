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

It also reports, without failing, the denied crates reachable from the published members through
normal and build dependencies with the default features (`cargo metadata --locked --offline`, the
resolution of the lock): a library the crate uses may draw entropy or read the clock inside, which
the direct-dependency rule cannot see. That line is a count to read, not a verdict: a crate in the
graph is not necessarily called on a path that feeds a Law-bearing value. When the resolution cannot
be read (no lock, a cache miss offline) the line says so.

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


def transitive(metadata: dict) -> list[str]:
    """the denied crates reachable from the published members through normal and build dependencies"""
    members = set(metadata.get("workspace_members", []))
    pkgs = {p["id"]: p for p in metadata.get("packages", [])}
    nodes = {n["id"]: n for n in (metadata.get("resolve") or {}).get("nodes", [])}
    todo = [i for i in members if pkgs.get(i, {}).get("publish") != []]
    seen, found = set(todo), set()
    while todo:
        node = nodes.get(todo.pop())
        if node is None:
            continue
        for d in node.get("deps", []):
            if all(k.get("kind") == "dev" for k in d.get("dep_kinds", [])):
                continue
            if d["pkg"] in seen:
                continue
            seen.add(d["pkg"])
            name = pkgs.get(d["pkg"], {}).get("name", "")
            if DENIED.fullmatch(name):
                found.add(f"{name} {pkgs[d['pkg']].get('version', '')}")
            todo.append(d["pkg"])
    return sorted(found)


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
    full = subprocess.run(["cargo", "metadata", "--format-version", "1", "--locked", "--offline",
                           "--manifest-path", a.manifest_path], capture_output=True, text=True)
    if full.returncode != 0:
        print("entropy-deps-guard: transitive (report only): the resolution could not be read offline from the lock")
    else:
        reach = transitive(json.loads(full.stdout))
        print(f"entropy-deps-guard: transitive (report only): {len(reach)} denied crate(s) reachable through "
              f"normal / build dependencies with the default features{': ' + ', '.join(reach) if reach else ''}")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
