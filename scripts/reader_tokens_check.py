#!/usr/bin/env python3
"""The rule-law readers separate tokens by spaces and tabs only (conformance/TASK.md "Tokens are
separated by spaces or tabs").

Both readers have one tokenizer for that: `fields` / `words` / `trim_matches([' ', '\\t'])` in
`alice-lol/src/rule_law.rs` (and `rule_law/*.rs`), `_words` / `_trim` / `_split1` in
`scripts/law_rules.py`. The standard library's whitespace treats more characters as separators
(a no-break space, a form feed …), and a reader that calls it disagrees with the other reader and
with the contract on such input. This check fails on such a call:

- Rust: `split_whitespace`, `split_ascii_whitespace`, `trim()` / `trim_start()` / `trim_end()`
  without a pattern, `is_whitespace`, `is_ascii_whitespace`;
- Python (code only, read with `tokenize`, so comments and docstrings do not count): `split()`,
  `split(None, ...)`, `strip()` / `lstrip()` / `rstrip()` without an argument, `isspace`, and a
  `\\s` or `\\S` in a string (a regular expression).

usage: reader_tokens_check.py [--root DIR]
Exit 1 on a finding, 2 when no file or no line was read.
"""
from __future__ import annotations

import argparse
import io
import re
import sys
import tokenize
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RUST = re.compile(
    r"\bsplit_whitespace\b|\bsplit_ascii_whitespace\b|\.trim\(\)|\.trim_start\(\)|\.trim_end\(\)"
    r"|\bis_whitespace\b|\bis_ascii_whitespace\b")
PY_METHODS = {"split", "strip", "lstrip", "rstrip"}


def rust_files(root: Path) -> list[Path]:
    src = root / "alice-lol" / "src"
    return [p for p in [src / "rule_law.rs", *sorted((src / "rule_law").glob("*.rs"))] if p.is_file()]


def rust_findings(path: Path, root: Path) -> tuple[list[str], int]:
    out, lines = [], 0
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        lines += 1
        code = line.split("//", 1)[0]
        for m in RUST.finditer(code):
            out.append(f"{path.relative_to(root).as_posix()}:{n}: `{m.group(0)}` separates by Unicode whitespace")
    return out, lines


def py_findings(path: Path, root: Path) -> tuple[list[str], int]:
    text = path.read_text(encoding="utf-8")
    toks = list(tokenize.generate_tokens(io.StringIO(text).readline))
    rel = path.relative_to(root).as_posix()
    out = []
    for i, t in enumerate(toks):
        # a call `.name(` with no argument (or `None` first) for the four methods
        if (t.type == tokenize.NAME and t.string in PY_METHODS and i > 0 and toks[i - 1].string == "."
                and i + 2 < len(toks) and toks[i + 1].string == "("):
            nxt = toks[i + 2]
            if nxt.string == ")" or (t.string == "split" and nxt.string == "None"):
                out.append(f"{rel}:{t.start[0]}: `.{t.string}({'' if nxt.string == ')' else 'None, ...'})` separates by Unicode whitespace")
        if t.type == tokenize.NAME and t.string == "isspace":
            out.append(f"{rel}:{t.start[0]}: `isspace` is Unicode whitespace")
        if t.type == tokenize.STRING:
            prev = toks[i - 1] if i > 0 else None
            docstring = prev is not None and prev.type in (tokenize.INDENT, tokenize.NEWLINE, tokenize.NL, tokenize.DEDENT, tokenize.ENCODING)
            if not docstring and re.search(r"\\[sS]", t.string):
                out.append(f"{rel}:{t.start[0]}: `\\s` / `\\S` in a pattern is Unicode whitespace")
    return out, text.count("\n")


def check(root: Path):
    findings, files, lines = [], 0, 0
    for p in rust_files(root):
        f, n = rust_findings(p, root)
        findings += f
        files, lines = files + 1, lines + n
    py = root / "scripts" / "law_rules.py"
    if py.is_file():
        f, n = py_findings(py, root)
        findings += f
        files, lines = files + 1, lines + n
    if files == 0 or lines == 0:
        return 2, ["no reader file was read (nothing compared)"], files
    return (1 if findings else 0), findings, files


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, default=ROOT)
    a = ap.parse_args(argv)
    code, problems, files = check(a.root.resolve())
    for p in problems:
        print(f"  {p}", file=sys.stderr)
    print(f"reader-tokens: {files} file(s), {len(problems)} finding(s)")
    return code


if __name__ == "__main__":
    sys.exit(main())
