#!/usr/bin/env python3
"""Tests must not read the repository history.

CI checks out a shallow clone (one commit), so a test that runs `git show <rev>:<path>`,
`git log`, `git rev-list` or names a revision path (`8d92276~1:laws/...`) passes on a
full clone and fails in CI. Keep the old content the test needs as a file in the tree
(for example under scripts/testdata/).

Scanned: scripts/test_*.py, conformance/*.py, and every *.rs under a tests/ directory.
Fails when a scanned file reads the history, and when no file was scanned.

usage: history_free_tests.py [repo root]
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

# git subcommands that read commits other than the checked-out one
HISTORY = r"(?:show|log|rev-list|cat-file|blame|reflog|whatchanged)"
PATTERNS = [
    # "git", "show" in an argument list (Python) or .arg("show") after Command::new("git") (Rust)
    re.compile(r"""["']git["']\s*,\s*(?:["']-C["']\s*,\s*[^,]+,\s*)?["']""" + HISTORY + r"""["']"""),
    re.compile(r"""Command::new\(\s*"git"\s*\)[^;]*?\.args?\(\s*\[?\s*"""
               r""""(?:-C"\s*,\s*[^,]+,\s*")?""" + HISTORY + r'''"''', re.S),
    # a shell string
    re.compile(r"\bgit\s+(?:-C\s+\S+\s+)?" + HISTORY + r"\b"),
    # a revision path: <rev>~N:<path> or <rev>^:<path>
    re.compile(r"\b[0-9a-f]{7,40}(?:~\d*|\^\d*)+:[\w./-]+"),
]


def scanned(root: Path) -> list[Path]:
    files = sorted(root.glob("scripts/test_*.py")) + sorted(root.glob("conformance/*.py"))
    files += sorted(p for p in root.rglob("*.rs")
                    if "tests" in p.relative_to(root).parts and "target" not in p.relative_to(root).parts)
    return files


def findings(path: Path) -> list[str]:
    text = path.read_text(encoding="utf-8")
    out = []
    for pat in PATTERNS:
        for m in pat.finditer(text):
            line = text.count("\n", 0, m.start()) + 1
            out.append(f"{path}:{line}: reads the repository history: {m.group(0)[:60]!r}")
    return out


def main(argv: list[str]) -> int:
    root = Path(argv[0]) if argv else Path(__file__).resolve().parent.parent
    files = scanned(root)
    if not files:
        print("error: no test file scanned (compared nothing)", file=sys.stderr)
        return 1
    errs = [e for f in files for e in findings(f)]
    for e in errs:
        print(e)
    if errs:
        return 1
    print(f"ok: {len(files)} test files, none reads the repository history")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
