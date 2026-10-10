#!/usr/bin/env python3
"""The law-file readers split lines only on LF.

`str.splitlines()` (Python) and `str::lines()` (Rust) also split on, or strip, other
characters (CR alone, VT, FF, the C1 / Unicode separators in Python; a CR before LF in
Rust), so a reader that uses them sees different lines from one that splits on LF, and
the two read the same law file differently.

Scanned: conformance/ref_impl.py, scripts/law_schema.py, alice-lol/src/law_input.rs.
Fails when one of them uses the call, and when a file is missing (nothing compared).

usage: line_split_guard.py [repo root]
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

FILES = {
    "conformance/ref_impl.py": re.compile(r"\.splitlines\("),
    "scripts/law_schema.py": re.compile(r"\.splitlines\("),
    "alice-lol/src/law_input.rs": re.compile(r"\.lines\(\)"),
    "alice-lol/examples/audit_conformance.rs": re.compile(r"\.lines\(\)"),
}


def main(argv: list[str]) -> int:
    root = Path(argv[0]) if argv else Path(__file__).resolve().parent.parent
    bad, scanned = [], 0
    for rel, pat in FILES.items():
        p = root / rel
        if not p.exists():
            print(f"error: {rel} not found (compared nothing)", file=sys.stderr)
            return 1
        scanned += 1
        for n, line in enumerate(p.read_text(encoding="utf-8").split("\n"), 1):
            if pat.search(line):
                bad.append(f"{rel}:{n}: splits lines on more than LF: {line.strip()[:80]}")
    for b in bad:
        print(b)
    if bad:
        return 1
    print(f"ok: {scanned} reader files split lines only on LF")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
