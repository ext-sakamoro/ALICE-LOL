#!/usr/bin/env python3
"""Fails when a conformance corpus does not contain the corners of every law.

usage: law_corpus_cover.py --corpus <corpus.json> [--laws <dir>] [--law <name>]

The corpus is a JSON list of vectors `{"law": <name>, "inputs": {...},
"kind": <kind>}`; `kind == "reject"` means the vector expects a rejection,
any other kind expects outputs. The corners of each law come from
`law_corners.py` (read from the law file, not chosen here). A corner is
covered when at least one vector of that law matches it.

Exit status: 0 every corner of every law is covered; 1 a corner is missing,
or a law file has no vector at all; 2 nothing was compared (0 laws, or 0
corners over all laws). Standard library only.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import law_corners as lc  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent


def cover(laws: list[dict], corpus: list[dict]) -> tuple[dict, int, int]:
    """Returns ({law: (covered ids, missing ids, vector count)}, laws compared, corners)"""
    by_law: dict[str, list[dict]] = {}
    for v in corpus:
        by_law.setdefault(v["law"], []).append(v)
    report = {}
    n_corners = 0
    for law in laws:
        vecs = by_law.get(law["name"], [])
        cs = lc.corners(law)
        n_corners += len(cs)
        covered, missing = [], []
        for c in cs:
            hit = any(lc.matches(c, law, v["inputs"], v["kind"] == "reject") for v in vecs)
            (covered if hit else missing).append(c["id"])
        report[law["name"]] = (covered, missing, len(vecs))
    return report, len(laws), n_corners


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--corpus", required=True)
    ap.add_argument("--laws", default=str(ROOT / "laws" / "spike"))
    ap.add_argument("--law", action="append", help="check only this law (repeatable)")
    a = ap.parse_args(argv)
    files = sorted(Path(a.laws).glob("*.law"))
    laws = [lc.load(f) for f in files]
    if a.law:
        laws = [x for x in laws if x["name"] in a.law]
    corpus = json.loads(Path(a.corpus).read_text(encoding="utf-8"))
    report, n_laws, n_corners = cover(laws, corpus)
    bad = 0
    for name, (covered, missing, nvec) in report.items():
        total = len(covered) + len(missing)
        status = "ok" if not missing and nvec else "MISSING"
        print(f"{name}: {len(covered)}/{total} corners covered, {nvec} vectors  {status}")
        if nvec == 0:
            print("  no vector for this law")
            bad += 1
        for cid in missing:
            print(f"  missing: {cid}")
        bad += len(missing)
    print(f"{n_laws} laws, {n_corners} corners, {bad} gaps")
    if n_laws == 0 or n_corners == 0:
        print("error: compared nothing (0 laws or 0 corners)", file=sys.stderr)
        return 2
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
