#!/usr/bin/env python3
"""Every workspace crate carries the license it states, and its package ships the texts.

Every crate of this repo is `LicenseRef-Proprietary` (with `publish = false`). A crate
package contains only the files under the crate's own directory, so each crate keeps its
own copies of `LICENSE-Proprietary` and `TRADEMARK_NOTICE`, byte for byte the root ones.

The permissive licence the crates were dual-licensed under up to 0.3.x is named `OLD` below
(spelled from fragments, so that this file and its test never hold the word themselves).

Checks:
- the `license` field of every workspace member is `LicenseRef-Proprietary`;
- no file of an earlier licence (`LICENSE-<OLD>`, `LICENSE-APACHE`, `LICENSE-AGPL`,
  `LICENSE-COMMERCIAL.md`, `NOTICE`) at the root or in a crate;
- each crate has the two files, equal to the root copies;
- with `--package`: `cargo package --list` of each crate lists them;
- no line of any tracked file holds the word `OLD` in any case (as a word: a letter or a
  digit next to it makes another word, `-` and `_` do not, so `LICENSE-<OLD>`,
  `<OLD>-licensed` and `license = "<old>"` all count), unless `ALLOWED` names that exact
  line: the file, the SHA-256 of the full line and the reason. An entry that matches no line
  fails, and so does a scan that read no line. Forms are not enumerated: any mention of the
  word needs an entry, so a new way of stating the licence cannot pass unseen. A line with
  the grant sentence that opens the licence's text counts the same way. Each line is read
  after NFKC normalisation (full-width letters count); spellings broken by other characters
  (dots, spaces between the letters) are not searched for and pass. An entry also states
  how many times its line occurs in the file, so a copy of an allowed line elsewhere in the
  same file fails.

usage: license_check.py [--package] [--hits]
`--hits` prints every line holding the word with its SHA-256 (to write an entry).
Exit 1 on a violation, 2 when no workspace crate, no license file or no line was read.
"""
from __future__ import annotations

import hashlib
import unicodedata
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LICENSE = "LicenseRef-Proprietary"
TEXTS = ["LICENSE-Proprietary", "TRADEMARK_NOTICE"]
OLD = "M" "I" "T"
# the files of the licences the crates were published under before
OLD_FILES = [f"LICENSE-{OLD}", "LICENSE-APACHE", "LICENSE-AGPL", "LICENSE-COMMERCIAL.md", "NOTICE"]
# the word in any case; a letter or a digit next to it makes another word (`submit`, `limit`)
WORD = re.compile(rf"(?<![A-Za-z0-9]){OLD}(?![A-Za-z0-9])", re.IGNORECASE)
# the grant sentence that opens the old licence's text (its body need not hold the name)
GRANT = re.compile("Permission is hereby " "granted, free of " "charge", re.IGNORECASE)
# (path, SHA-256 of the full line without its line end, occurrences in the file, reason)
ALLOWED: list[tuple[str, str, int, str]] = [
    ("alice-lol-macro/CHANGELOG.md", "4859808644fe2d0ae08cbcf2ad4d7e196607799b0b79506ef3fddc8b29c5b92a", 1, "the terms of macro 0.2.0 and earlier"),
    ("CHANGELOG.md", "3dd8a9efad371865471e35336598a1da475c143ef4889731f2b7aab29c59607b", 1, "0.4.0 notes the terms of the versions published before it"),
    ("CHANGELOG.md", "2685e44c3384aac00441cb5000fb6c97bea47c226030622c1a85c3d6e534851e", 1, "0.4.0 records removing the old licence file"),
    ("CHANGELOG.md", "b979a59120edda4899cd66ee123dc4dc2a181463a065811a7ab527e93651812c", 1, "the licence of the dependency stacker"),
    ("alice-lol/Cargo.toml", "4c153f064b25aa89cd8a9f7bafdf563b65347f8bfec9ec95025fbe1cf2e690a2", 1, "the licence of the dependency stacker"),
    ("deny.toml", "1f357a16a4708e089d0ef8a455a36aa8c2a2dd73ffc127d5b826dd013d0d2c2a", 1, "states that the allow list is for dependencies"),
    ("deny.toml", "f776c7c98bcbd52f8a6cede872b0ca38f9fd04516b954a5a1379cd00d0f8b2ad", 1, "a licence allowed for dependencies"),
    ("deny.toml", "fab278bf6bdd6a44a300ed01bf8e7d3e7ffc539139b92aac83e42c54aa044438", 1, "the licence expression of the dependency ring"),
]


def line_hash(line: str) -> str:
    return hashlib.sha256(line.encode("utf-8")).hexdigest()


def tracked() -> list[str]:
    out = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True).stdout
    return [p for p in out.decode("utf-8").split("\0") if p]


def members() -> list[tuple[str, Path, str]]:
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    out = []
    for d in re.findall(r'"([^"]+)"', re.search(r"members\s*=\s*\[([^\]]*)\]", text).group(1)):
        manifest = (ROOT / d / "Cargo.toml").read_text(encoding="utf-8")
        name = re.search(r'^name\s*=\s*"([^"]+)"', manifest, re.M).group(1)
        lic = re.search(r'^license\s*=\s*"([^"]+)"', manifest, re.M)
        out.append((name, ROOT / d, lic.group(1) if lic else ""))
    return out


def hits(paths: list[str]) -> tuple[int, list[tuple[str, int, str]]]:
    """(lines read, (path, line number, line) for each line holding the word)"""
    lines, out = 0, []
    for rel in paths:
        p = ROOT / rel
        if not p.is_file():
            continue
        for i, line in enumerate(p.read_bytes().decode("utf-8", "replace").splitlines(), 1):
            lines += 1
            norm = unicodedata.normalize("NFKC", line)
            if WORD.search(norm) or GRANT.search(norm):
                out.append((rel, i, line))
    return lines, out


def scan(paths: list[str]) -> tuple[int, list[str]]:
    """(lines read, violations): every line holding the word must be an ALLOWED line"""
    lines, found = hits(paths)
    allowed = {(p, h): k for k, (p, h, _, _) in enumerate(ALLOWED)}
    bad, seen = [], {}
    for rel, i, line in found:
        k = allowed.get((rel, line_hash(line)))
        if k is None:
            bad.append(f"{rel}:{i}: names the old licence and no ALLOWED entry names this line "
                       f"(sha256 {line_hash(line)}): {line.strip()[:80]}")
        else:
            seen[k] = seen.get(k, 0) + 1
    for k, (path, h, count, reason) in enumerate(ALLOWED):
        if k not in seen:
            bad.append(f"ALLOWED entry ({path}, {h[:12]}…: {reason}) matches no line: remove it")
        elif seen[k] != count:
            bad.append(f"ALLOWED entry ({path}, {h[:12]}…: {reason}) names {count} line(s), "
                       f"the file has {seen[k]}")
    return lines, bad


def main(argv: list[str]) -> int:
    if argv == ["--hits"]:
        for rel, i, line in hits(tracked())[1]:
            print(f"{rel}:{i}: {line_hash(line)} {line.strip()[:100]}")
        return 0
    package = argv == ["--package"]
    if argv and not package:
        print(__doc__.strip().splitlines()[-3], file=sys.stderr)
        return 2
    crates = members()
    if not crates:
        print("error: read 0 workspace crates", file=sys.stderr)
        return 2
    bad, files = [], 0
    for f in TEXTS:
        if not (ROOT / f).is_file():
            bad.append(f"{f} is missing at the root")
    for d in [ROOT, *(d for _, d, _ in crates)]:
        for f in OLD_FILES:
            if (d / f).exists():
                bad.append(f"{(d / f).relative_to(ROOT)} exists (the crates are {LICENSE})")
    lines, found = scan(tracked())
    if lines == 0:
        print("error: read 0 lines of tracked files", file=sys.stderr)
        return 2
    bad += found
    for name, d, lic in crates:
        if lic != LICENSE:
            bad.append(f"{name}: license = {lic!r}, expected {LICENSE!r}")
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
    print(f"licenses: read {len(crates)} crates and {lines} lines, checked {files} license files"
          f"{' and the package lists' if package else ''}, {len(bad)} violation(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
