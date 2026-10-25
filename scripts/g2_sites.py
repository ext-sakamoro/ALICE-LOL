#!/usr/bin/env python3
"""Every library call of the Law max / min has a reviewed row (ALICE-Law G2).

The library takes its max / min from `det_math` (`max`, `min`, `max64`, `min64`) and `det_vec`
(the lane forms); the platform forms are banned by `.github/clippy-lib/clippy.toml`. This check
makes the review of those call sites mechanical: `scripts/g2_sites.toml` has one row per site,
a site being a function of a library file (`file` + `fn`, with the number of calls in it), and each
row says how the ties of its calls are settled:

- `tests`: tests that drive the site through the public API with an equal pair of zeros of
  different sign (and a NaN operand where one can reach it) and read the result back bit for bit,
  each `<path>::<fn>` naming a `#[test]` function of that file;
- `reason`: why the zero sign or the NaN cannot be observed at the site's (other) calls (reviewed).

A row has tests, a reason, or both (a site with a tested call and an equivalent one).

Fails on a call site without a row, a row whose site is gone or whose count differs, a named test
that does not exist, a row with neither tests nor a reason, and when no site was compared.

usage: g2_sites.py [--list] [--root DIR]
`--list` prints the sites found (file, fn, calls) to write rows.
Exit 1 on a finding, 2 when nothing was compared or the table does not read.
"""
from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = Path("scripts/g2_sites.toml")
LIB = Path("alice-lol/src")
# the helpers themselves are not sites
HELPERS = {"det_math.rs", "det_vec.rs"}
CALL = re.compile(r"\b(?:det_math::(?:max|min|max64|min64)|det_vec::(?:max|min|max_element|min_element|clamp))\b")
FN = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")
TEST_FN = re.compile(r"#\[test\]\s*(?:#\[[^\]]*\]\s*)*fn\s+([A-Za-z_][A-Za-z0-9_]*)")


def code_of(line: str) -> str:
    """The line without a `//` comment (string literals with `//` do not occur at these sites)"""
    i = line.find("//")
    return line if i < 0 else line[:i]


def sites(root: Path) -> dict[tuple[str, str], int]:
    """{(file, enclosing fn): calls} over the library sources"""
    out: dict[tuple[str, str], int] = {}
    for f in sorted((root / LIB).rglob("*.rs")):
        if f.name in HELPERS:
            continue
        rel = f.relative_to(root).as_posix()
        current = None
        for line in f.read_text(encoding="utf-8").splitlines():
            m = FN.match(line)
            if m:
                current = m.group(1)
            n = len(CALL.findall(code_of(line)))
            if n:
                key = (rel, current or "<file>")
                out[key] = out.get(key, 0) + n
    return out


def check(root: Path):
    found = sites(root)
    path = root / TABLE
    try:
        rows = tomllib.loads(path.read_text(encoding="utf-8")).get("site", [])
    except (OSError, tomllib.TOMLDecodeError) as e:
        return 2, [f"{TABLE}: {e}"], found
    problems = []
    seen = set()
    for i, r in enumerate(rows, start=1):
        key = (r.get("file", ""), r.get("fn", ""))
        where = f"{TABLE} row {i} ({key[0]}::{key[1]})"
        if key in seen:
            problems.append(f"{where}: a second row for the site")
            continue
        seen.add(key)
        if key not in found:
            problems.append(f"{where}: no such site (gone or renamed): remove or update the row")
            continue
        if r.get("calls") != found[key]:
            problems.append(f"{where}: {found[key]} calls, the row says {r.get('calls')}")
        tests, reason = r.get("tests", []), r.get("reason", "").strip()
        if not tests and len(reason) < 20:
            problems.append(f"{where}: a row has tests, a reason (why no tie is observable), or both")
        for test in tests:
            tpath, _, tname = test.partition("::")
            tfile = root / tpath
            names = set(TEST_FN.findall(tfile.read_text(encoding="utf-8"))) if tfile.is_file() else set()
            if not tname or tname not in names:
                problems.append(f"{where}: `{test}` is not a #[test] fn")
    for key, n in sorted(found.items()):
        if key not in seen:
            problems.append(f"{key[0]}::{key[1]}: {n} Law max/min call(s) without a row in {TABLE}")
    if not found:
        return 2, problems + ["no Law max/min call site was found (nothing compared)"], found
    return (1 if problems else 0), problems, found


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--root", type=Path, default=ROOT)
    a = ap.parse_args(argv)
    root = a.root.resolve()
    if a.list:
        for (f, fn), n in sorted(sites(root).items()):
            print(f"{f}\t{fn}\t{n}")
        return 0
    code, problems, found = check(root)
    for p in problems:
        print(f"  {p}", file=sys.stderr)
    print(f"g2-sites: {len(found)} site(s), {sum(found.values())} call(s), {len(problems)} problem(s)")
    return code


if __name__ == "__main__":
    sys.exit(main())
