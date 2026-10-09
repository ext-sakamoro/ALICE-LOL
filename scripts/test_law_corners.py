#!/usr/bin/env python3
"""Tests for scripts/law_corners.py and scripts/law_corpus_cover.py.

The cover checker is run on a small law whose corners are known by hand, on
a corpus that covers them all (exit 0) and on copies with one vector removed
(each must exit 1). The corner generator is run on the repository's law files.
"""

from __future__ import annotations

import io
import json
import math
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import law_corners as lc  # noqa: E402
import law_corpus_cover as cov  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent

LAW = """# demo
law demo
kind research
input a 1 range 0 10
input n 1 range 1 5
input t s
input b 1
x-integer n
x-list t
let q 1 = a*n
x-range q 0 20
let r 1 = t/2
x-range r 0 3
x-expr m 1 = min(b, 4) - 1
x-range m > 0
"""

# one vector per corner of LAW that no vertex covers (accept unless "reject");
# the vertices (added by corpus()) cover a@lo / a@hi / n@* / q@lo / q@hi
FULL = [
    ({"a": -1, "n": 3, "b": 2, "t": [1]}, "reject"),    # a<lo, q<lo
    ({"a": 11, "n": 3, "b": 2, "t": [1]}, "reject"),    # a>hi, q>hi
    ({"a": 2, "n": 3, "b": 2, "t": [6]}, "value"),      # r@hi
    ({"a": 2, "n": 3, "b": 2, "t": [0]}, "value"),      # r@lo
    ({"a": 2, "n": 0, "b": 2, "t": [1]}, "reject"),     # n<lo
    ({"a": 2, "n": 6, "b": 2, "t": [1]}, "reject"),     # n>hi
    ({"a": 2, "n": 2.5, "b": 2, "t": [1]}, "reject"),   # n:non-integer
    ({"a": 2, "n": 2, "b": 2, "t": [-1]}, "reject"),    # r<lo
    ({"a": 2, "n": 2, "b": 2, "t": [7]}, "reject"),     # r>hi
    ({"a": 1, "n": 2, "b": 1.01, "t": [1]}, "value"),   # m just above 0 (scale 2)
    ({"a": 1, "n": 2, "b": 1, "t": [1]}, "reject"),     # m <= 0
]
VERTICES = [(a, n) for a in (0, 10) for n in (1, 2, 4, 5)]


def corpus(vectors):
    out = [{"law": "demo", "inputs": i, "kind": k} for i, k in vectors]
    for a, n in VERTICES:
        out.append({"law": "demo", "inputs": {"a": a, "n": n, "b": 2, "t": [1]},
                    "kind": "value" if a * n <= 20 else "reject"})
    return out


def run_cover(laws_dir, corpus_list):
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(corpus_list, f)
    try:
        buf = io.StringIO()
        with redirect_stdout(buf):
            code = cov.main(["--laws", str(laws_dir), "--corpus", f.name])
        return code, buf.getvalue()
    finally:
        os.unlink(f.name)


class Expressions(unittest.TestCase):
    def ev(self, s, **env):
        return lc.evaluate(lc.parse_expr(s), env)

    def test_precedence(self):
        self.assertEqual(self.ev("-2^2"), -4)
        self.assertEqual(self.ev("2^3^2"), 512)
        self.assertEqual(self.ev("1 - 2 - 3"), -4)
        self.assertEqual(self.ev("8/4/2"), 1)
        self.assertAlmostEqual(self.ev("atan2(1, 1)"), math.pi / 4)
        self.assertEqual(self.ev("max(a, 2, b) - min(a, b)", a=1, b=5), 4)
        self.assertAlmostEqual(self.ev("exp(ln(3))"), 3)

    def test_errors(self):
        for bad in ("1 +", "(1", "1 2", "a $ b"):
            with self.assertRaises(lc.ExprError, msg=bad):
                lc.evaluate(lc.parse_expr(bad), {"a": 1, "b": 1})
        with self.assertRaises(lc.ExprError):
            self.ev("zz + 1")


class Corners(unittest.TestCase):
    def test_demo_corner_ids(self):
        ids = [c["id"] for c in lc.corners(lc.parse_law(LAW))]
        expected = ["a@lo", "a@hi", "a<lo", "a>hi", "n@lo", "n@hi", "n@lo+1", "n@hi-1",
                    "n<lo", "n>hi", "n:non-integer", "q@lo", "q@hi", "q<lo", "q>hi",
                    "r@lo", "r@hi", "r<lo", "r>hi", "m@>0", "m<=0"]
        self.assertEqual(ids[:len(expected)], expected)
        self.assertEqual(len(ids), len(expected) + len(VERTICES))

    def test_vertex_is_rejected_when_a_scalar_range_excludes_it(self):
        cs = {c["id"]: c for c in lc.corners(lc.parse_law(LAW))}
        self.assertTrue(cs["vertex[a=10,n=2]"]["accept"])     # q = 20
        self.assertFalse(cs["vertex[a=10,n=4]"]["accept"])    # q = 40
        self.assertTrue(cs["vertex[a=0,n=1]"]["accept"])      # m needs b: not decidable, kept

    def test_integer_line_adds_parity_neighbours(self):
        without = [c["id"] for c in lc.corners(lc.parse_law(LAW.replace("x-integer n\n", "")))]
        self.assertNotIn("n@lo+1", without)
        self.assertNotIn("n:non-integer", without)

    def test_matches(self):
        law = lc.parse_law(LAW)
        cs = {c["id"]: c for c in lc.corners(law)}
        self.assertTrue(lc.matches(cs["r@hi"], law, {"a": 2, "n": 1, "b": 2, "t": [1, 6]}, False))
        self.assertFalse(lc.matches(cs["r@hi"], law, {"a": 2, "n": 1, "b": 2, "t": [1, 6]}, True))
        self.assertFalse(lc.matches(cs["r@hi"], law, {"a": 2, "n": 1, "b": 2, "t": [5.9]}, False))
        self.assertTrue(lc.matches(cs["n:non-integer"], law, {"a": 2, "n": 2.5}, True))
        self.assertFalse(lc.matches(cs["n:non-integer"], law, {"a": 2, "n": 2.0}, True))
        self.assertFalse(lc.matches(cs["m@>0"], law, {"a": 1, "n": 2, "b": 1.5, "t": [1]}, False))

    def test_repository_laws(self):
        files = sorted((ROOT / "laws" / "spike").glob("*.law"))
        self.assertGreater(len(files), 0)
        total = 0
        for f in files:
            law = lc.load(f)
            self.assertEqual(f.stem, law["name"])
            total += len(lc.corners(law))
        self.assertGreater(total, 0)
        kdk = lc.load(ROOT / "laws" / "spike" / "kepler_energy_bounded_kdk.law")
        e = next(c for c in lc.corners(kdk) if c["id"] == "e@hi")
        self.assertEqual(e["value"], 0.77)
        self.assertIn("vertex[e=0.77,n=101,periods=40]", [c["id"] for c in lc.corners(kdk)])


class Cover(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        Path(self.dir.name, "demo.law").write_text(LAW)

    def tearDown(self):
        self.dir.cleanup()

    def test_full_corpus_passes(self):
        code, out = run_cover(self.dir.name, corpus(FULL))
        self.assertEqual(code, 0, out)

    def test_each_removed_vector_fails(self):
        # must-red control: a corpus with one corner removed fails
        full = corpus(FULL)
        for i in range(len(full)):
            code, out = run_cover(self.dir.name, full[:i] + full[i + 1:])
            self.assertEqual(code, 1, f"vector {i} removed: {out}")
            self.assertIn("missing:", out)
        code, out = run_cover(self.dir.name, full[:-1])
        self.assertIn("missing: vertex[a=10,n=5]", out)

    def test_wrong_expectation_does_not_cover(self):
        flipped = [(i, "reject" if k != "reject" else "value") if j == 0 else (i, k)
                   for j, (i, k) in enumerate(FULL)]
        code, _ = run_cover(self.dir.name, corpus(flipped))
        self.assertEqual(code, 1)

    def test_law_without_vectors_fails(self):
        code, out = run_cover(self.dir.name, [])
        self.assertEqual(code, 1)
        self.assertIn("no vector for this law", out)

    def test_compared_nothing(self):
        with tempfile.TemporaryDirectory() as empty:
            code, _ = run_cover(empty, corpus(FULL))
            self.assertEqual(code, 2)
        Path(self.dir.name, "demo.law").write_text("law demo\nkind audit\n")
        code, _ = run_cover(self.dir.name, [{"law": "demo", "inputs": {}, "kind": "verdict"}])
        self.assertEqual(code, 2)


if __name__ == "__main__":
    unittest.main()
