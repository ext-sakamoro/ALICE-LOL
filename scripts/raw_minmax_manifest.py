#!/usr/bin/env python3
"""Every caller of `alice-lol/src/raw_minmax.rs`'s `raw_max_f32`/`raw_min_f32`/
`raw_max_f64`, named and counted.

`raw_minmax.rs` is the one file allowed to call the platform `f32`/`f64`
`max`/`min` directly (see its module doc and
`scripts/disallowed_methods_allow_reasons.py`'s `raw-helper:` prefix). A
caller of its helpers is a separate, provably-safe site of its own (a
constant nonzero operand, both operands already `.abs()`-derived, or the
result cast to an integer immediately after) — this script is the default-
deny ratchet on *those* sites: `scripts/raw_minmax_manifest.toml` lists every
file that may call them and how many times, and this script fails on:

  * a file not in the manifest calling a raw_* helper (a new, unreviewed site)
  * a file's call count differing from the manifest (a site added or removed
    without updating the manifest in the same commit)
  * a manifest entry naming a file that no longer calls any raw_* helper (a
    stale entry — the exemption it tracked is gone, remove the entry)
  * an aliased import of a raw_* helper (`use ... raw_max_f32 as x;`) whose
    calls under the alias are not folded into the same file's count (the
    point of counting is defeated if a call can hide under a different name)
  * `raw_minmax.rs` itself: its own `a.max(b)`/`a.min(b)` bodies are the
    *definition*, not a caller, and must not be counted as one
  * zero files scanned, or the manifest listing zero entries (compared
    nothing)

Usage: `python3 scripts/raw_minmax_manifest.py`
`--root DIR` to check a different tree (used by the test)
`--write` to regenerate the manifest from the real counts (review the diff)
"""
from __future__ import annotations

import argparse
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_ROOT = os.path.dirname(HERE)
MANIFEST_REL = os.path.join("scripts", "raw_minmax_manifest.toml")
DEFINITION_REL = os.path.join("alice-lol", "src", "raw_minmax.rs")

MEMBERS_RE = re.compile(r'members\s*=\s*\[(?P<body>[^\]]*)\]')
MEMBER_NAME_RE = re.compile(r'"([^"]+)"')

RAW_FNS = ("raw_max_f32", "raw_min_f32", "raw_max_f64", "raw_min_f64")
# a bare reference (e.g. `.fold(0.0, crate::raw_minmax::raw_max_f64)`, passing the
# function itself rather than calling it inline) is still a real call site, so this
# does not require a trailing `(`
CALL_RE = re.compile(r"\b(?:" + "|".join(RAW_FNS) + r")\b")
# `use ... raw_max_f32 as x;`-style aliasing: the alias name is then also a call target
ALIAS_AS_RE = re.compile(r"\b(?:" + "|".join(RAW_FNS) + r")\s+as\s+([A-Za-z_][A-Za-z0-9_]*)")

TOML_HEADER_RE = re.compile(r'^\["?(?P<file>[^"\]]+)"?\]\s*$')
TOML_COUNT_RE = re.compile(r'^count\s*=\s*(?P<n>\d+)\s*$')


def workspace_members(root: str) -> list[str]:
    path = os.path.join(root, "Cargo.toml")
    try:
        with open(path, encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return []
    m = MEMBERS_RE.search(text)
    if not m:
        return []
    return MEMBER_NAME_RE.findall(m.group("body"))


def iter_rs_files(base: str):
    for dirpath, _dirnames, filenames in os.walk(base):
        for name in filenames:
            if name.endswith(".rs"):
                yield os.path.join(dirpath, name)


def read_manifest(path: str) -> dict[str, int]:
    manifest: dict[str, int] = {}
    current: str | None = None
    try:
        with open(path, encoding="utf-8") as f:
            lines = f.readlines()
    except OSError as e:
        raise RawMinmaxError(f"manifest を読めない ({path}): {e}") from e
    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        m = TOML_HEADER_RE.match(stripped)
        if m:
            current = m.group("file")
            continue
        m = TOML_COUNT_RE.match(stripped)
        if m and current is not None:
            manifest[current] = int(m.group("n"))
            current = None
    return manifest


class RawMinmaxError(Exception):
    """a manifest or source tree this check cannot read"""


def real_counts(root: str) -> dict[str, int]:
    members = workspace_members(root)
    if not members:
        raise RawMinmaxError(f"{root}/Cargo.toml に [workspace] members が無い")
    src_dirs = [os.path.join(root, member, "src") for member in members]
    missing = [d for d in src_dirs if not os.path.isdir(d)]
    if missing:
        raise RawMinmaxError(f"member の src/ が無い: {', '.join(missing)}")

    counts: dict[str, int] = {}
    definition_abs = os.path.join(root, DEFINITION_REL)
    for base in src_dirs:
        for path in iter_rs_files(base):
            if os.path.abspath(path) == os.path.abspath(definition_abs):
                continue  # the definition's own a.max(b)/a.min(b) is not a caller
            with open(path, encoding="utf-8") as f:
                text = f.read()
            n = len(CALL_RE.findall(text))
            for alias_name in ALIAS_AS_RE.findall(text):
                # an aliased import: calls under the alias name also count,
                # folded into the same file (the point is one count per file,
                # not one count per spelling)
                n += len(re.findall(rf"\b{re.escape(alias_name)}\s*\(", text))
            if n > 0:
                rel = os.path.relpath(path, root).replace(os.sep, "/")
                counts[rel] = n
    return counts


def check(root: str) -> tuple[list[str], int]:
    manifest = read_manifest(os.path.join(root, MANIFEST_REL))
    if not manifest:
        raise RawMinmaxError("manifest が 0 件 (比較できない)")
    counts = real_counts(root)
    # a scanner that silently matches nothing would otherwise look identical to
    # "every manifest entry's exemption was legitimately removed" (every entry
    # reported as merely stale, not as a hard failure) -- that distinction
    # matters only when the manifest is non-empty, which the check above
    # already guarantees, so every manifest entry would show up as stale
    # below regardless of which of the two this actually is: the stale-entry
    # errors this function already returns are the signal, not a separate
    # raise

    errors: list[str] = []
    for rel, n in sorted(counts.items()):
        if rel not in manifest:
            errors.append(f"{rel}: raw_minmax の呼び出し {n} 件、manifest に entry が無い (新規サイトを足す)")
        elif manifest[rel] != n:
            errors.append(f"{rel}: 呼び出し {n} 件、manifest は {manifest[rel]} 件 (drift)")
    for rel in sorted(set(manifest) - set(counts)):
        errors.append(f"{rel}: manifest に entry があるが呼び出しが 0 件 (使われなくなった entry、消す)")
    return errors, sum(counts.values())


def write_manifest(root: str) -> None:
    counts = real_counts(root)
    path = os.path.join(root, MANIFEST_REL)
    lines = [
        "# Every file that calls `crate::raw_minmax::raw_*` (alice-lol/src/raw_minmax.rs,",
        "# the one place the disallowed_methods lint allows f32/f64 max/min directly),",
        "# and how many times. See scripts/raw_minmax_manifest.py.",
        "#",
        "# A count here is not a budget to spend: a new call site means either a new",
        "# provably-safe site (add it here, with the same nonzero-const:/both-abs:/",
        "# int-cast: reasoning as alice-lol/src/raw_minmax.rs's module doc requires of",
        "# every caller) or a site that should route through crate::det_math::max/min",
        "# instead (the Law form, not this file).",
        "",
    ]
    for rel in sorted(counts):
        lines.append(f'["{rel}"]')
        lines.append(f"count = {counts[rel]}")
        lines.append("")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines).rstrip() + "\n")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--root", default=DEFAULT_ROOT)
    ap.add_argument("--write", action="store_true")
    args = ap.parse_args()

    if args.write:
        write_manifest(args.root)
        print(f"{MANIFEST_REL}: 再生成した")
        return 0

    try:
        errors, total = check(args.root)
    except RawMinmaxError as e:
        print(f"error: {e}")
        return 2

    if errors:
        for e in errors:
            print(f"error: {e}")
        print(f"compared: files {len(real_counts(args.root))}, calls {total}, mismatches {len(errors)}")
        return 1
    print(f"compared: files {len(real_counts(args.root))}, calls {total}, mismatches 0")
    print("raw-minmax-manifest: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
