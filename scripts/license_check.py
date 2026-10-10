#!/usr/bin/env python3
"""Every workspace crate carries the license it states, and its package ships the texts.

From 0.4.0 the crates of this repo are Apache-2.0 (`alice-world-auditor` is
AGPL-3.0-or-later OR LicenseRef-Commercial). Apache-2.0 section 4 asks that a copy of the
license and the NOTICE file go with every distribution, and a crate package contains only
the files under the crate's own directory, so each published Apache-2.0 crate keeps its own
copies of `LICENSE-APACHE`, `NOTICE` and `TRADEMARK_NOTICE`, byte for byte the root ones.

Checks:
- the `license` field of every workspace member is the expected expression;
- no `LICENSE-MIT` at the root or in a crate, and no MIT licence text in any tracked file
  (the MIT terms ended with 0.3.x; the only exempt paths are third-party ones, `THIRD_PARTY`);
- each published Apache-2.0 crate has the three files, equal to the root copies;
- with `--package`: `cargo package --list` of each published crate lists them.

usage: license_check.py [--package]
Exit 1 on a violation, 2 when no workspace crate was read.
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
APACHE = "Apache-2.0"
EXPECTED = {"alice-world-auditor": "AGPL-3.0-or-later OR LicenseRef-Commercial"}
TEXTS = ["LICENSE-APACHE", "NOTICE", "TRADEMARK_NOTICE"]
# the opening of the MIT licence text (its grant sentence and its usual title line)
# (split so that this file does not hold the sentence itself)
MIT_TEXT = re.compile(r"Permission is hereby granted, free" r" of charge|^\s*MIT" r" License\s*$", re.M)
# tracked paths that hold someone else's code under its own licence (none today)
THIRD_PARTY: tuple[str, ...] = ()


def tracked() -> list[str]:
    out = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True).stdout
    return [p for p in out.decode("utf-8").split("\0") if p]


def members() -> list[tuple[str, Path, str, bool]]:
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    out = []
    for d in re.findall(r'"([^"]+)"', re.search(r"members\s*=\s*\[([^\]]*)\]", text).group(1)):
        manifest = (ROOT / d / "Cargo.toml").read_text(encoding="utf-8")
        name = re.search(r'^name\s*=\s*"([^"]+)"', manifest, re.M).group(1)
        lic = re.search(r'^license\s*=\s*"([^"]+)"', manifest, re.M)
        published = not re.search(r"^publish\s*=\s*false", manifest, re.M)
        out.append((name, ROOT / d, lic.group(1) if lic else "", published))
    return out


def main(argv: list[str]) -> int:
    package = argv == ["--package"]
    if argv and not package:
        print(__doc__.strip().splitlines()[-2], file=sys.stderr)
        return 2
    crates = members()
    if not crates:
        print("error: read 0 workspace crates", file=sys.stderr)
        return 2
    bad, files = [], 0
    for f in TEXTS:
        if not (ROOT / f).is_file():
            bad.append(f"{f} is missing at the root")
    for p in [ROOT / "LICENSE-MIT", *(d / "LICENSE-MIT" for _, d, _, _ in crates)]:
        if p.exists():
            bad.append(f"{p.relative_to(ROOT)} exists (the crates are no longer MIT)")
    paths = tracked()
    if not paths:
        print("error: read 0 tracked files", file=sys.stderr)
        return 2
    for rel in paths:
        if rel.startswith(THIRD_PARTY) if THIRD_PARTY else False:
            continue
        p = ROOT / rel
        if p.is_file() and MIT_TEXT.search(p.read_bytes().decode("utf-8", "replace")):
            bad.append(f"{rel} holds an MIT licence text")
    for name, d, lic, published in crates:
        want = EXPECTED.get(name, APACHE)
        if lic != want:
            bad.append(f"{name}: license = {lic!r}, expected {want!r}")
        if not (published and want == APACHE):
            continue
        for f in TEXTS:
            files += 1
            copy = d / f
            if not copy.is_file():
                bad.append(f"{name}: {f} is missing in {d.relative_to(ROOT)}/")
            elif (ROOT / f).is_file() and copy.read_bytes() != (ROOT / f).read_bytes():
                bad.append(f"{name}: {d.relative_to(ROOT)}/{f} differs from the root {f}")
        if package:
            listed = subprocess.run(["cargo", "package", "--list", "--allow-dirty", "-p", name],
                                    cwd=ROOT, capture_output=True, text=True, check=True).stdout.split()
            for f in TEXTS:
                if f not in listed:
                    bad.append(f"{name}: the package does not list {f}")
    if files == 0:
        print("error: checked 0 license files", file=sys.stderr)
        return 2
    for b in bad:
        print(f"error: {b}", file=sys.stderr)
    print(f"licenses: read {len(crates)} crates and {len(paths)} tracked files, checked {files} license files"
          f"{' and the package lists' if package else ''}, {len(bad)} violation(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
