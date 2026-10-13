#!/usr/bin/env python3
"""The sibling checkouts a tagged build needs, each at the tag of the version in Cargo.lock.

Members of this workspace depend on other repositories through paths (`../../ALICE-SDF`).
cargo reads every member's manifest before it does anything, `cargo publish --dry-run`
included, so a build of the workspace needs each of those directories next to this
repository. A release builds a tag, so the siblings it uses must be the versions the tag's
`Cargo.lock` was resolved against, not whatever each sibling's main branch is now: this
script lists, for every sibling directory, the package the path points at, its version in
`Cargo.lock`, and the tag of that version in the sibling repository.

The tag of a version is `v<version>` or `<package>-v<version>`; the first that exists is used
(`--resolve` asks the remote with `git ls-remote --tags`). A sibling whose package is not in
`Cargo.lock`, appears there more than once with a path source, or has neither tag is an
error. A sibling directory that two members point at must name the same package.

usage: release_siblings.py [--root DIR] [--resolve] [--owner OWNER]
  prints one `<dir> <package> <version> [<tag>]` line per sibling
Exit 1 on an error above, 2 when no sibling path dependency was read.
"""
from __future__ import annotations

import os
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEP_TABLES = ("dependencies", "dev-dependencies", "build-dependencies")


class SiblingError(Exception):
    """a sibling this script cannot pin"""


def members(root: Path) -> list[Path]:
    ws = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]
    return [root / m for m in ws["members"]]


def dep_tables(manifest: dict):
    for name in DEP_TABLES:
        yield manifest.get(name, {})
    for target in manifest.get("target", {}).values():
        for name in DEP_TABLES:
            yield target.get(name, {})


def manifests(root: Path):
    """(directory the paths are relative to, dependency tables) for the workspace
    dependencies of the root manifest and for every member"""
    ws = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8")).get("workspace", {})
    yield root, [ws.get("dependencies", {})]
    for member in members(root):
        manifest = tomllib.loads((member / "Cargo.toml").read_text(encoding="utf-8"))
        yield member, list(dep_tables(manifest))


def sibling_deps(root: Path) -> dict[str, str]:
    """sibling directory name -> package name, over every path dependency of the root's
    `[workspace.dependencies]` and of every member (a sibling's own path dependencies are
    not read: it is not checked out yet, and `cargo metadata --locked` fails loudly on one)"""
    found: dict[str, str] = {}
    for member, tables in manifests(root):
        for table in tables:
            for key, spec in table.items():
                if not isinstance(spec, dict) or "path" not in spec:
                    continue
                # lexical: a sibling checkout may be a symlink, and its target is not the point
                target = Path(os.path.normpath(member / spec["path"]))
                if target == root or root in target.parents:
                    continue  # a member of this workspace
                if target.parent != root.parent:
                    raise SiblingError(f"{member.name or 'workspace'}: path {spec['path']} is not a sibling of the repository")
                package = spec.get("package", key)
                if found.get(target.name, package) != package:
                    raise SiblingError(f"{target.name}: named as {found[target.name]} and {package}")
                found[target.name] = package
    return found


def locked_version(lock: dict, package: str) -> str:
    versions = [p["version"] for p in lock.get("package", []) if p["name"] == package and "source" not in p]
    if len(versions) != 1:
        raise SiblingError(f"{package}: {len(versions)} path entries in Cargo.lock (want 1)")
    return versions[0]


def tag_candidates(package: str, version: str) -> list[str]:
    return [f"v{version}", f"{package}-v{version}"]


def remote_tags(owner: str, repo: str) -> set[str]:
    out = subprocess.run(
        ["git", "ls-remote", "--tags", f"https://github.com/{owner}/{repo}.git"],
        check=True, capture_output=True, text=True,
    ).stdout
    return {line.split("refs/tags/", 1)[1].removesuffix("^{}") for line in out.splitlines() if "refs/tags/" in line}


def pick_tag(package: str, version: str, tags: set[str]) -> str:
    for tag in tag_candidates(package, version):
        if tag in tags:
            return tag
    raise SiblingError(f"{package} {version}: none of {tag_candidates(package, version)} is a tag")


def plan(root: Path, tags_of=None) -> list[tuple[str, str, str, str | None]]:
    lock = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))
    rows = []
    for directory, package in sorted(sibling_deps(root).items()):
        version = locked_version(lock, package)
        tag = pick_tag(package, version, tags_of(directory)) if tags_of else None
        rows.append((directory, package, version, tag))
    return rows


def main(argv: list[str]) -> int:
    root, resolve, owner = ROOT, False, "ext-sakamoro"
    args = list(argv)
    while args:
        a = args.pop(0)
        if a == "--root":
            root = Path(args.pop(0)).resolve()
        elif a == "--resolve":
            resolve = True
        elif a == "--owner":
            owner = args.pop(0)
        else:
            print(f"error: unknown argument {a}", file=sys.stderr)
            return 1
    try:
        rows = plan(root, (lambda d: remote_tags(owner, d)) if resolve else None)
    except SiblingError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    for row in rows:
        print(" ".join(x for x in row if x is not None))
    print(f"release siblings: {len(rows)}", file=sys.stderr)
    if not rows:
        print("error: no sibling path dependency was read", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
