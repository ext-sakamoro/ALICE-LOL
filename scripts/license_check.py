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
- with `--package`: `cargo package --list` of each published crate lists them;
- no line of a tracked file states an MIT licence (`MIT_STATEMENTS`: an SPDX tag, a
  "licensed under the MIT license" sentence, a `license = "...MIT..."` field, an
  `MIT OR / AND ...` expression) unless an `ALLOWED` entry names the file, the line and the
  reason (an earlier version's terms, a dependency's licence); an entry that matches no
  line fails, so the list cannot outlive what it allows.

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
# a line that states an MIT licence, by form (literals split so this file does not match itself)
MIT_STATEMENTS = {
    "SPDX tag": re.compile(r"SPDX-License-" r"Identifier:[^\n]*\bMIT\b"),
    "licence sentence": re.compile(r"(?i)licen[cs]ed under (the )?MIT\b|\bMIT" r" licen[cs]e\b"),
    "license field": re.compile(r"\blicense\s*=\s*\"[^\"\n]*\bMIT" r"\b"),
    "licence expression": re.compile(r"\bMIT\s+(OR|AND|or|and)\s+[A-Z]|\b[A-Z][\w.-]*\s+(OR|AND|or|and)\s+MIT" r"\b|\bMIT/[A-Z]|[\w.-]/MIT" r"\b"),
}
# (path, regex the line must match ("" = any line of the file), reason)
ALLOWED: list[tuple[str, str, str]] = [
    ("CHANGELOG.md", r"0\.3\.x 以前", "the terms of the versions published before 0.4.0"),
    ("CHANGELOG.md", r"stacker", "the licence of the dependency stacker"),
    ("README.md", r"under MIT OR Apache-2\.0 and keep those terms", "the terms of the versions published before 0.4.0"),
    ("README_JP.md", r"0\.3\.x 以前は MIT OR Apache-2\.0", "the terms of the versions published before 0.4.0"),
    ("alice-lol-macro/README.md", r"released under MIT OR Apache-2\.0 and keep those terms", "the terms of macro 0.2.0 and earlier"),
    ("alice-lol/Cargo.toml", r"rustc と同じ手法", "the licence of the dependency stacker"),
    ("deny.toml", r"MIT AND ISC AND OpenSSL", "the licence expression of the dependency ring"),
    ("docs/HUMANOID_TEMPLATE_DESIGN.md", r"`(glam|gltf|serde_json)`", "the licences of the dependencies"),
    ("docs/HUMANOID_TEMPLATE_ROADMAP.md", r"`(gltf|serde_json)`", "the licences of the dependencies"),
    ("scripts/license_check.py", r"", "the checker names the forms it looks for"),
    ("scripts/test_license_check.py", r"", "the checker's test writes each form into a fixture"),
]


def statements(rel: str, text: str) -> list[tuple[int, str, str]]:
    """(line number, form, line) for each line of `text` that states an MIT licence"""
    out = []
    for i, line in enumerate(text.splitlines(), 1):
        for form, rx in MIT_STATEMENTS.items():
            if rx.search(line):
                out.append((i, form, line))
                break
    return out


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
    used: set[int] = set()
    for rel in paths:
        if rel.startswith(THIRD_PARTY) if THIRD_PARTY else False:
            continue
        p = ROOT / rel
        if not p.is_file():
            continue
        text = p.read_bytes().decode("utf-8", "replace")
        if MIT_TEXT.search(text):
            bad.append(f"{rel} holds an MIT licence text")
        for i, form, line in statements(rel, text):
            hits = [k for k, (path, rx, _) in enumerate(ALLOWED) if path == rel and re.search(rx, line)]
            used.update(hits)
            if not hits:
                bad.append(f"{rel}:{i}: states an MIT licence ({form}) and no ALLOWED entry names it: {line.strip()[:80]}")
    for k, (path, rx, reason) in enumerate(ALLOWED):
        if k not in used:
            bad.append(f"ALLOWED entry ({path}, {rx!r}: {reason}) matches no line: remove it")
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
