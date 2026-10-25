#!/usr/bin/env python3
"""Every `src gap` ignore names a row of the wiring-debt table, every row is used, and no such
ignore is stale.

A test that pins something not yet wired is ignored with `#[ignore = "src gap: <key>: <reason>"]`.
`docs/wiring-debt.md` has one row per key: `| <key> | <what is not wired> | <when it closes> |`.

Checks:
- the table reads: each row has three cells, the key is kebab-case and unique;
- every `src gap:` ignore in a tracked `.rs` file names a key of the table (and is written in that
  form), and every key of the table is named by at least one ignore;
- with `--run`: each such test, run alone (`cargo test -p <crate> --test <target> --all-features --
  --ignored --exact <name>`), runs exactly one test and fails. A src-gap test that passes means the
  gap closed and the ignore (and the row) must go; a run of 0 tests means the test was not found.

usage: src_gap_check.py [--run] [--root DIR]
Exit 1 on a finding, 2 when the table is missing or has rows but no ignore was compared.
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = Path("docs/wiring-debt.md")
KEY = re.compile(r"[a-z][a-z0-9]*(-[a-z0-9]+)*\Z")
ROW = re.compile(r"^\|\s*`?([^|`]*?)`?\s*\|(.*)\|\s*$")
IGNORE = re.compile(r'#\[ignore\s*=\s*"src gap:\s*([^:"]*)(:?)')
FN = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")


def read_table(root: Path):
    """[(line, key, cells)] of the rows after the header and its rule; problems"""
    path = root / TABLE
    if not path.is_file():
        return None, [f"{TABLE}: missing"]
    rows, problems, seen = [], [], set()
    lines = path.read_text(encoding="utf-8").splitlines()
    head = next((i for i, ln in enumerate(lines) if ln.startswith("| key |")), None)
    if head is None:
        return None, [f"{TABLE}: no `| key | ... |` header"]
    for n, ln in enumerate(lines[head + 2:], start=head + 3):
        if not ln.startswith("|"):
            break
        cells = [c.strip() for c in ln.strip().strip("|").split("|")]
        key = cells[0].strip("`")
        if len(cells) != 3 or not all(cells):
            problems.append(f"{TABLE}:{n}: a row has three cells, none empty")
            continue
        if not KEY.match(key):
            problems.append(f"{TABLE}:{n}: `{key}` is not a kebab-case key")
        if key in seen:
            problems.append(f"{TABLE}:{n}: a second row for `{key}`")
        seen.add(key)
        rows.append((n, key, cells))
    return rows, problems


def tracked_rs(root: Path) -> list[Path]:
    out = subprocess.run(["git", "-C", str(root), "ls-files", "*.rs"], capture_output=True, text=True, check=True)
    return [root / p for p in out.stdout.split()]


def find_ignores(files: list[Path], root: Path):
    """[(path, line, key, test name)] and problems (a `src gap` ignore not in the `<key>:` form)"""
    found, problems = [], []
    for f in files:
        lines = f.read_text(encoding="utf-8").splitlines()
        for i, ln in enumerate(lines):
            if "src gap" not in ln or "#[ignore" not in ln:
                continue
            m = IGNORE.search(ln)
            rel = f.relative_to(root).as_posix()
            if not m or not m.group(2) or not KEY.match(m.group(1).strip()):
                problems.append(f"{rel}:{i + 1}: write `#[ignore = \"src gap: <key>: <reason>\"]`")
                continue
            name = next((FN.search(x).group(1) for x in lines[i + 1:i + 8] if FN.search(x)), None)
            if name is None:
                problems.append(f"{rel}:{i + 1}: no `fn` follows the ignore")
                continue
            found.append((rel, i + 1, m.group(1).strip(), name))
    return found, problems


def target_of(rel: str):
    """(crate dir, test target) of a file `<crate>/tests/<target>.rs`, else None"""
    parts = rel.split("/")
    if len(parts) >= 2 and parts[-2] == "tests" and parts[-1].endswith(".rs"):
        return "/".join(parts[:-2]) or ".", parts[-1][:-3]
    return None


def run_one(root: Path, rel: str, name: str, runner=subprocess.run):
    """None when the test ran alone and failed; otherwise the problem"""
    t = target_of(rel)
    if t is None:
        return f"{rel}: a src-gap test lives in <crate>/tests/<target>.rs (to be run alone)"
    crate, target = t
    cmd = ["cargo", "test", "--quiet", "--all-features", "--test", target, "--", "--ignored", "--exact", name]
    r = runner(cmd, cwd=str(root / crate), capture_output=True, text=True)
    text = (r.stdout or "") + (r.stderr or "")
    ran = re.findall(r"test result: \w+\. (\d+) passed; (\d+) failed", text)
    total = sum(int(p) + int(f) for p, f in ran)
    if total != 1:
        return f"{rel}::{name}: ran {total} tests (expected the one src-gap test)"
    if r.returncode == 0:
        return f"{rel}::{name}: passes: the gap closed, remove the ignore and its row"
    return None


def check(root: Path, run: bool, runner=subprocess.run):
    rows, problems = read_table(root)
    if rows is None:
        return 2, problems
    ignores, more = find_ignores(tracked_rs(root), root)
    problems += more
    keys = {k for _, k, _ in rows}
    for rel, n, key, _ in ignores:
        if key not in keys:
            problems.append(f"{rel}:{n}: src gap `{key}` has no row in {TABLE}")
    used = {k for _, _, k, _ in ignores}
    for n, key, _ in rows:
        if key not in used:
            problems.append(f"{TABLE}:{n}: `{key}` is named by no src-gap ignore")
    if rows and not ignores:
        return 2, problems + ["the table has rows but no src-gap ignore was compared"]
    if run:
        for rel, _, _, name in ignores:
            p = run_one(root, rel, name, runner)
            if p:
                problems.append(p)
    print(f"src-gap: {len(rows)} row(s), {len(ignores)} ignore(s){', each run' if run else ''}, "
          f"{len(problems)} problem(s)")
    return (1 if problems else 0), problems


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", action="store_true", help="run each src-gap test alone; it must fail")
    ap.add_argument("--root", type=Path, default=ROOT)
    a = ap.parse_args(argv)
    code, problems = check(a.root.resolve(), a.run)
    for p in problems:
        print(f"  {p}", file=sys.stderr)
    return code


if __name__ == "__main__":
    sys.exit(main())
