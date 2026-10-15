#!/usr/bin/env python3
"""Lint the public documents for CHANGELOG structure and the README example.

The repository is public; its documents are read by people and by language
models judging whether the project is real. Two classes of problem are
checked mechanically:

  * changelog   version headings: no duplicates, `[Unreleased]` first,
                released versions in descending semver order, the Cargo.toml
                version either released or covered by `[Unreleased]`; inside
                `[Unreleased]` (or, when it is empty just after a version is
                cut, the newest section), each Keep a Changelog category at
                most once and no emoji status markers
  * example     the first ```rust block of README.md is identical to the
                ```rust block in the crate documentation of alice-lol/src/lib.rs (which
                rustdoc compiles and runs), so the README example compiles

The words a public text must not carry (how the work was done, record names,
device names, private addresses, private names held as SHA-256) are checked by
the ALICE-Law public text checker (scripts/public_text.sh, at the revision
.github/public-text-pin names), not here, so there is one list and it cannot
drift apart from a copy.

Released sections are history and are not rewritten, so the per-category and
emoji checks apply to `[Unreleased]` only.

Every check must compare at least one item; a check that compared nothing
fails, so a renamed file or heading cannot turn the gate into a no-op.

Usage: `python3 scripts/docs_lint.py --check` (exit 1 on any finding).
`--root DIR` runs against another tree (used by scripts/test_docs_lint.py).
"""

from __future__ import annotations

import os
import re
import sys

DOCS = ("README.md", "README_JP.md", "CHANGELOG.md")
# The published crate of this workspace: its manifest carries the version the
# CHANGELOG is compared with, its crate documentation carries the doctest the
# README example is compared with
CRATE_TOML = "alice-lol/Cargo.toml"
LIB_RS = "alice-lol/src/lib.rs"

CATEGORIES = ("Added", "Changed", "Deprecated", "Removed", "Fixed", "Security")
EMOJI_RE = re.compile("[⚠✅❌⭐\U0001f300-\U0001faff]")
VERSION_RE = re.compile(r"^## \[([^\]]+)\]", re.M)


def read(root: str, rel: str) -> str:
    with open(os.path.join(root, rel), encoding="utf-8") as f:
        return f.read()


def semver_key(v: str) -> tuple:
    core, _, pre = v.partition("-")
    nums = tuple(int(x) for x in re.findall(r"\d+", core)[:3])
    # a pre-release sorts below the release it precedes
    return nums + ((1,) if not pre else (0, tuple(int(x) if x.isdigit() else x for x in re.split(r"[.]", pre))))


def cargo_version(root: str) -> str | None:
    m = re.search(r'^\[package\]\s*$(.*?)^\[', read(root, CRATE_TOML), re.M | re.S)
    if not m:
        return None
    v = re.search(r'^version\s*=\s*"([^"]+)"', m.group(1), re.M)
    return v.group(1) if v else None


def unreleased_body(changelog: str) -> str | None:
    m = re.search(r"^## \[Unreleased\][^\n]*\n(.*?)(?=^## \[|\Z)", changelog, re.M | re.S)
    return m.group(1) if m else None


def newest_section_body(changelog: str) -> str | None:
    m = re.search(r"^## \[(?!Unreleased\])[^\]]+\][^\n]*\n(.*?)(?=^## \[|\Z)", changelog, re.M | re.S)
    return m.group(1) if m else None


RUST_BLOCK_RE = re.compile(r"^```rust[^\n]*\n(.*?)^```", re.M | re.S)


def crate_doc(lib_rs: str) -> str:
    """The `//!` crate documentation of src/lib.rs with the comment markers removed."""
    out = []
    for line in lib_rs.splitlines():
        stripped = line.strip()
        if stripped.startswith("//!"):
            out.append(stripped[4:] if stripped.startswith("//! ") else stripped[3:])
    return "\n".join(out) + "\n"


def check_example(root: str, errors: list[str]) -> int:
    """README's first ```rust block == the ```rust block of the crate doc (a doctest)."""
    if not os.path.exists(os.path.join(root, "README.md")):
        return 0
    m = RUST_BLOCK_RE.search(read(root, "README.md"))
    if not m:
        errors.append("README.md: no ```rust example block")
        return 0
    if not os.path.exists(os.path.join(root, LIB_RS)):
        errors.append(f"{LIB_RS}: missing (the README example is compared with its crate doctest)")
        return 0
    d = RUST_BLOCK_RE.search(crate_doc(read(root, LIB_RS)))
    if not d:
        errors.append(f"{LIB_RS}: the crate documentation has no ```rust example block")
        return 0
    if m.group(1) != d.group(1):
        a, b = m.group(1).splitlines(), d.group(1).splitlines()
        i = next((k for k in range(min(len(a), len(b))) if a[k] != b[k]), min(len(a), len(b)))
        errors.append(f"README.md example differs from the {LIB_RS} doctest at example line {i + 1}")
    return len(m.group(1).splitlines())


def check(root: str) -> tuple[list[str], dict[str, int]]:
    errors: list[str] = []
    counts = {"documents": 0, "versions": 0, "categories": 0}
    counts["example lines"] = check_example(root, errors)

    for rel in DOCS:
        if not os.path.exists(os.path.join(root, rel)):
            errors.append(f"{rel}: missing")
            continue
        counts["documents"] += 1

    if os.path.exists(os.path.join(root, "CHANGELOG.md")):
        cl = read(root, "CHANGELOG.md")
        heads = VERSION_RE.findall(cl)
        counts["versions"] = len(heads)
        dup = sorted({h for h in heads if heads.count(h) > 1})
        if dup:
            errors.append(f"CHANGELOG.md: version headings appear twice: {dup}")
        if heads and heads[0] != "Unreleased":
            errors.append(f"CHANGELOG.md: the first version heading is [{heads[0]}], not [Unreleased]")
        released = [h for h in heads if h != "Unreleased"]
        for a, b in zip(released, released[1:]):
            if semver_key(a) <= semver_key(b):
                errors.append(f"CHANGELOG.md: [{a}] is listed above [{b}] but is not newer")
        cv = cargo_version(root)
        if cv and cv not in released and "Unreleased" not in heads:
            errors.append(f"CHANGELOG.md: Cargo.toml version {cv} has no section and there is no [Unreleased]")
        if cv and released and cv not in released and semver_key(cv) < semver_key(released[0]):
            errors.append(f"CHANGELOG.md: Cargo.toml version {cv} is older than the newest section [{released[0]}]")
        body, where = unreleased_body(cl), "[Unreleased]"
        if body is not None and not body.strip() and released:
            # just after a version is cut, [Unreleased] is empty and the work sits in the
            # newest section: check that section (it must still have categories)
            body, where = newest_section_body(cl), f"[{released[0]}]"
        if body is not None:
            cats = re.findall(r"^### (\w+)", body, re.M)
            counts["categories"] = len(cats)
            for c in sorted(set(cats)):
                if c not in CATEGORIES:
                    errors.append(f"CHANGELOG.md {where}: `### {c}` is not a Keep a Changelog category")
                elif cats.count(c) > 1:
                    errors.append(f"CHANGELOG.md {where}: `### {c}` appears {cats.count(c)} times (append to one list)")
            for i, line in enumerate(body.splitlines(), 1):
                if EMOJI_RE.search(line):
                    errors.append(f"CHANGELOG.md {where} line {i}: emoji status marker (use **Behavior change:** / **Breaking:**)")
        else:
            counts["categories"] = 1  # no unreleased work: nothing to compare, not an error

    for name, c in counts.items():
        if c == 0:
            errors.append(f"check `{name}` compared nothing")
    return errors, counts


def main() -> int:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8")
        except (AttributeError, ValueError):
            pass
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if "--root" in sys.argv:
        root = sys.argv[sys.argv.index("--root") + 1]
    errors, counts = check(root)
    print("compared: " + ", ".join(f"{k} {v}" for k, v in counts.items()))
    for e in errors:
        print(f"error: {e}", file=sys.stderr)
    if "--check" in sys.argv and errors:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
