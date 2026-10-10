#!/usr/bin/env python3
"""Differential fuzz of the law-file line reader: two implementations must agree.

A valid audit law file is perturbed one way at a time (whitespace kinds, counts and
positions, unicode spaces and zero-width characters, a byte-order mark, case, full-width
letters, line endings, an embedded NUL, very long lines), and every perturbed file is read
by each implementation with the same request. The two outcomes (a verdict, or a refusal:
exit status 2) must be equal: either both accept the file with the same meaning, or both
refuse it. Perturbations are deterministic (a fixed list plus pairs drawn with a fixed
seed), so a disagreement reproduces.

usage: law_line_fuzz.py [--pairs N] [--seed S] -- <implementation A...> -- <implementation B...>
Exit 1 on any disagreement, 2 when no case was compared.
"""
from __future__ import annotations

import argparse
import json
import os
import random
import subprocess
import sys
import tempfile
from pathlib import Path

LAW = "fuzz_probe"
BASE = [
    "law fuzz_probe",
    "kind audit",
    "x-input b list of record(f: list of text, id: optional text)",
    "x-metric n = count(b)",
    "x-metric s = distinct(set(b[].f))",
    "x-at-least s 2",
    "begin audit",
    "audit fuzz-probe",
    "evidence n",
    "evidence s",
    "end audit",
]
DECLS = [2, 3, 4, 5]  # indices of the declaration lines
REQUEST = {"law": LAW, "inputs": {"b": [{"f": ["x"], "id": "a"}, {"f": ["y"], "id": "a"}]}}

SPACES = {
    "tab": "\t", "two spaces": "  ", "nbsp": " ", "zwsp": "​", "ideographic": "　",
    "em space": " ", "bom": "﻿", "vertical tab": "\v", "form feed": "\f", "none": "",
    "space tab": " \t", "nel": "\u0085", "line separator": " ",
}


def perturbations() -> list[tuple[str, list[str], str]]:
    """(label, lines, line ending) for every single perturbation"""
    out = []
    for i in DECLS:
        kw, rest = BASE[i].split(" ", 1)
        for name, sp in SPACES.items():
            out.append((f"line {i}: separator {name}", _with(i, kw + sp + rest), "\n"))
            out.append((f"line {i}: leading {name}", _with(i, sp + BASE[i]), "\n"))
            out.append((f"line {i}: trailing {name}", _with(i, BASE[i] + sp), "\n"))
            out.append((f"line {i}: inside {name}", _with(i, BASE[i].replace(" = ", " =" + sp + " ", 1)
                                                         .replace(" list of ", " list" + sp + "of ", 1)), "\n"))
        out.append((f"line {i}: upper keyword", _with(i, kw.upper() + " " + rest), "\n"))
        out.append((f"line {i}: title keyword", _with(i, kw.title() + " " + rest), "\n"))
        out.append((f"line {i}: full-width x", _with(i, "ｘ" + BASE[i][1:]), "\n"))
        out.append((f"line {i}: full-width hyphen", _with(i, "x－" + BASE[i][2:]), "\n"))
        out.append((f"line {i}: nul inside", _with(i, BASE[i][:-1] + "\0" + BASE[i][-1:]), "\n"))
        out.append((f"line {i}: long trailing comment", _with(i, BASE[i] + "  # " + "c" * 100_000), "\n"))
        out.append((f"line {i}: long name", _with(i, BASE[i].replace(" n ", " " + "n" * 10_000 + " ", 1)), "\n"))
        out.append((f"line {i}: comment only", _with(i, "# " + BASE[i]), "\n"))
        out.append((f"line {i}: carriage return inside", _with(i, BASE[i].replace(" ", "\r", 1)), "\n"))
    for name, ending in [("crlf", "\r\n"), ("cr", "\r"), ("lf", "\n"), ("mixed", None)]:
        out.append((f"file: line ending {name}", list(BASE), ending))
    out.append(("file: bom first", ["﻿" + BASE[0]] + BASE[1:], "\n"))
    out.append(("file: bom before x-input", BASE[:2] + ["﻿" + BASE[2]] + BASE[3:], "\n"))
    out.append(("file: blank lines with spaces", BASE[:2] + ["   ", "\t"] + BASE[2:], "\n"))
    return out


def _with(i: int, line: str) -> list[str]:
    lines = list(BASE)
    lines[i] = line
    return lines


def render(lines: list[str], ending: str | None) -> bytes:
    if ending is None:
        endings = ["\n", "\r\n", "\n", "\r\n"]
        text = "".join(l + endings[k % len(endings)] for k, l in enumerate(lines))
    else:
        text = ending.join(lines) + ending
    return text.encode("utf-8")


def combine(singles, pairs: int, seed: int):
    """pairs of perturbations on two different declaration lines, drawn with a fixed seed"""
    rng = random.Random(seed)
    line_level = [s for s in singles if s[0].startswith("line ")]
    out = []
    while len(out) < pairs:
        a, b = rng.sample(line_level, 2)
        ia, ib = int(a[0].split()[1].rstrip(":")), int(b[0].split()[1].rstrip(":"))
        if ia == ib:
            continue
        lines = list(BASE)
        lines[ia], lines[ib] = a[1][ia], b[1][ib]
        out.append((f"{a[0]} + {b[0]}", lines, "\n"))
    return out


def outcome(cmd, law_bytes: bytes, timeout: float):
    with tempfile.TemporaryDirectory() as d:
        Path(d, f"{LAW}.law").write_bytes(law_bytes)
        env = dict(os.environ, LOL_LAW_DIR=d)
        r = subprocess.run(cmd, input=json.dumps(REQUEST).encode(), capture_output=True, timeout=timeout, env=env)
    err = r.stderr.decode("utf-8", "replace")
    if "Traceback (most recent call last)" in err or "panicked at" in err:
        return ("crash", r.returncode)
    if r.returncode == 2 and not r.stdout.strip():
        return ("refused",)
    if r.returncode != 0:
        return ("exit", r.returncode)
    try:
        o = json.loads(r.stdout)["outputs"]
        return ("accepted", o.get("verdict"), o.get("subject"))
    except (ValueError, KeyError, TypeError):
        return ("stdout", r.stdout[:60])


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--pairs", type=int, default=150)
    ap.add_argument("--seed", type=int, default=20261010)
    ap.add_argument("--timeout", type=float, default=60)
    ap.add_argument("--show", type=int, default=20)
    ap.add_argument("rest", nargs=argparse.REMAINDER)
    a = ap.parse_args(argv)
    rest = a.rest[1:] if a.rest[:1] == ["--"] else a.rest
    if "--" not in rest:
        ap.error("two implementations separated by --")
    k = rest.index("--")
    impl_a, impl_b = rest[:k], rest[k + 1:]
    if not impl_a or not impl_b:
        ap.error("two implementations separated by --")
    singles = perturbations()
    cases = [("unchanged", list(BASE), "\n")] + singles + combine(singles, a.pairs, a.seed)
    bad, classes = [], {}
    for label, lines, ending in cases:
        data = render(lines, ending)
        oa, ob = outcome(impl_a, data, a.timeout), outcome(impl_b, data, a.timeout)
        classes[oa[0]] = classes.get(oa[0], 0) + 1
        if oa != ob or oa[0] in ("crash", "exit", "stdout"):
            bad.append(f"{label}: {oa} vs {ob}")
    for b in bad[:a.show]:
        print("DIFF " + b)
    if len(bad) > a.show:
        print(f"... {len(bad) - a.show} more")
    print(f"compared {len(cases)} law files ({len(singles)} single, {len(cases) - 1 - len(singles)} pairs), "
          f"disagreements {len(bad)}; outcomes {dict(sorted(classes.items()))}")
    if not cases:
        return 2
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
