#!/usr/bin/env python3
"""A workspace crate that another one requires is the same source as the version it resolves to.

Inside this workspace `alice-lol` uses `alice-lol-macro` through a path, but the published
`alice-lol` uses the version its requirement resolves to on crates.io. When the macro's
source changes and its version does not, the two differ: every test here passes against
the new macro, and a user of the published crate gets the old one (the 0.2.0 macro emitted
a `Taper` literal that no longer matched `alice-sdf`, and nothing here compiled it).

For every dependency between workspace members declared with both `path` and `version`:
- the newest crates.io version matching the requirement is downloaded, and its `src/`
  files are compared with the local ones: any difference fails (bump the crate and the
  requirement);
- no published version matching the requirement is "pending": the crate is to be published
  before its dependents (not a failure).

usage: published_source_check.py [--offline-index <dir>]
Exit 1 on a difference, 2 when no dependency between workspace members was read.
`--offline-index` reads `<dir>/<crate>.json` (the crates.io API response) and
`<dir>/<crate>-<version>.crate` instead of the network (for the checker's own test).
"""
from __future__ import annotations

import argparse
import io
import json
import re
import sys
import tarfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
UA = {"User-Agent": "alice-lol published_source_check (https://github.com/ext-sakamoro/ALICE-LOL)"}


def members(root: Path) -> dict[str, Path]:
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    names = re.findall(r'"([^"]+)"', re.search(r"members\s*=\s*\[([^\]]*)\]", text).group(1))
    out = {}
    for d in names:
        manifest = (root / d / "Cargo.toml").read_text(encoding="utf-8")
        out[re.search(r'^name\s*=\s*"([^"]+)"', manifest, re.M).group(1)] = root / d
    return out


def internal_deps(root: Path, crates: dict[str, Path]) -> list[tuple[str, str, str]]:
    """(dependent, dependency, requirement) for path + version dependencies between members"""
    out = []
    for name, d in crates.items():
        for m in re.finditer(r'^([a-z0-9_-]+)\s*=\s*\{([^}]*)\}', (d / "Cargo.toml").read_text(encoding="utf-8"), re.M):
            dep, body = m.group(1), m.group(2)
            req = re.search(r'version\s*=\s*"([^"]+)"', body)
            if dep in crates and "path" in body and req:
                out.append((name, dep, req.group(1)))
    return out


def parse(v: str) -> tuple[int, int, int]:
    m = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", v)
    if not m:
        raise ValueError(v)
    return tuple(int(x) for x in m.groups())


def caret_matches(req: str, v: str) -> bool:
    """cargo's default (caret) requirement, for `x`, `x.y` and `x.y.z` without pre-release"""
    if not re.fullmatch(r"\^?\d+(\.\d+){0,2}", req):
        raise SystemExit(f"error: requirement {req!r} is not a plain caret requirement")
    parts = [int(x) for x in req.lstrip("^").split(".")]
    lo = tuple(parts + [0] * (3 - len(parts)))
    try:
        ver = parse(v)
    except ValueError:
        return False  # pre-release / build metadata: not matched by a plain requirement
    if ver < lo:
        return False
    if lo[0] > 0 or len(parts) == 1:
        return ver[0] == lo[0]
    if lo[1] > 0 or len(parts) == 2:
        return ver[:2] == lo[:2]
    return ver == lo


class Index:
    def __init__(self, offline: Path | None):
        self.offline = offline

    def versions(self, name: str) -> list[str]:
        if self.offline:
            path = self.offline / f"{name}.json"
            data = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {"versions": []}
        else:
            req = urllib.request.Request(f"https://crates.io/api/v1/crates/{name}", headers=UA)
            try:
                with urllib.request.urlopen(req, timeout=60) as r:
                    data = json.load(r)
            except urllib.error.HTTPError as e:
                if e.code == 404:
                    return []
                raise
        return [v["num"] for v in data["versions"] if not v.get("yanked")]

    def source(self, name: str, version: str) -> dict[str, bytes]:
        if self.offline:
            blob = (self.offline / f"{name}-{version}.crate").read_bytes()
        else:
            url = f"https://static.crates.io/crates/{name}/{name}-{version}.crate"
            with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=120) as r:
                blob = r.read()
        out = {}
        with tarfile.open(fileobj=io.BytesIO(blob), mode="r:gz") as t:
            for m in t.getmembers():
                rel = m.name.split("/", 1)[1] if "/" in m.name else m.name
                if m.isfile() and rel.startswith("src/"):
                    out[rel] = t.extractfile(m).read()
        return out


def local_source(d: Path) -> dict[str, bytes]:
    return {p.relative_to(d).as_posix(): p.read_bytes() for p in sorted((d / "src").rglob("*")) if p.is_file()}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, default=ROOT)
    ap.add_argument("--offline-index", type=Path)
    a = ap.parse_args(argv)
    crates = members(a.root)
    deps = internal_deps(a.root, crates)
    if not deps:
        print("error: read 0 dependencies between workspace members", file=sys.stderr)
        return 2
    index, bad, compared, pending = Index(a.offline_index), [], 0, []
    for dependent, dep, req in deps:
        matching = sorted((v for v in index.versions(dep) if caret_matches(req, v)), key=parse)
        if not matching:
            pending.append(f"{dep} {req} (required by {dependent}) is not on crates.io: publish it first")
            continue
        published = matching[-1]
        theirs, ours = index.source(dep, published), local_source(crates[dep])
        if not theirs:
            print(f"error: {dep} {published} from crates.io has no src/ files", file=sys.stderr)
            return 2
        compared += 1
        differ = sorted(f for f in set(theirs) | set(ours) if theirs.get(f) != ours.get(f))
        if differ:
            bad.append(f"{dependent} requires {dep} {req}, which resolves to the published {published}, "
                       f"but the local {dep} differs in {', '.join(differ[:5])}"
                       f"{' …' if len(differ) > 5 else ''}: bump {dep} and the requirement")
    for p in pending:
        print(f"pending: {p}")
    for b in bad:
        print(f"error: {b}", file=sys.stderr)
    print(f"published source: read {len(deps)} dependencies between workspace members, "
          f"compared {compared} with crates.io, pending {len(pending)}, differing {len(bad)}")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
