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
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import law_corners as lc  # noqa: E402
import law_corpus_cover as cov  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent

LAW = """# demo
law demo
kind research
input a 1 range -1 10
input n 1 range 1 5
input t s
input b 1
x-integer n
x-list t
let q 1 = a*n
x-range q -4 25
let r 1 = t/2
x-range r 0 3
x-expr m 1 = min(b, 4) - 1
x-range m > 0
"""

# one vector per corner of LAW that no vertex covers (accept unless "reject");
# the vertices (added by corpus()) cover a@* / n@* / q<lo / q>hi
FULL = [
    ({"a": -2, "n": 3, "b": 2, "t": [1]}, "reject"),    # a<lo
    ({"a": 11, "n": 1, "b": 2, "t": [1]}, "reject"),    # a>hi
    ({"a": -1, "n": 4, "b": 2, "t": [1]}, "value"),     # q@lo (= -4, also the vertex left out)
    ({"a": 5, "n": 5, "b": 2, "t": [1]}, "value"),      # q@hi (= 25)
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
# (-1, 4) is left out: q = -4 is on the edge of its x-range
VERTICES = [(a, n) for a in (-1, 10) for n in (1, 2, 4, 5) if (a, n) != (-1, 4)]


def corpus(vectors):
    out = [{"law": "demo", "inputs": i, "kind": k} for i, k in vectors]
    for a, n in VERTICES:
        out.append({"law": "demo", "inputs": {"a": a, "n": n, "b": 2, "t": [1]},
                    "kind": "value" if -4 <= a * n <= 25 else "reject"})
    return out


def run_cover(laws_dir, corpus_list):
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        json.dump(corpus_list, f)
    try:
        buf = io.StringIO()
        with redirect_stdout(buf), redirect_stderr(io.StringIO()):
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

    def test_atan2_ignores_the_sign_of_a_zero_argument(self):
        # TASK.md: -0 reads as 0, so the negative x axis is +pi and the origin is +0
        self.assertEqual(self.ev("atan2(y, -1)", y=-0.0), math.pi)
        self.assertEqual(math.copysign(1, self.ev("atan2(y, 1)", y=-0.0)), 1)
        self.assertEqual(math.copysign(1, self.ev("atan2(y, x)", y=-0.0, x=-0.0)), 1)
        self.assertEqual(self.ev("atan2(1, x)", x=-0.0), math.pi / 2)

    def test_errors(self):
        for bad in ("1 +", "(1", "1 2", "a $ b"):
            with self.assertRaises(lc.ExprError, msg=bad):
                lc.evaluate(lc.parse_expr(bad), {"a": 1, "b": 1})
        with self.assertRaises(lc.ExprError):
            self.ev("zz + 1")


class StrictDouble(unittest.TestCase):
    """TASK.md: every intermediate value of a quantitative law's expression is finite"""

    def sd(self, s, **env):
        return lc.strict_double(lc.parse_expr(s), {k: float(v) for k, v in env.items()})

    def test_each_kind_of_non_finite_intermediate_is_refused(self):
        for expr, env in [
            ("exp(x)", {"x": 1000}),          # math.exp raises OverflowError
            ("x^y", {"x": 10, "y": 400}),      # ** raises OverflowError
            ("x*x", {"x": 1e200}),             # * gives inf without raising
            ("x+x", {"x": 1.7e308}),           # + gives inf without raising
            ("ln(x)", {"x": -1}),              # domain error (ValueError)
            ("sqrt(x)", {"x": -1}),            # domain error (ValueError)
            ("1/x", {"x": 0}),                 # ZeroDivisionError
            ("x^y", {"x": 0, "y": -1}),        # 0 to a negative power
            ("1/exp(x)", {"x": 1000}),         # finite under IEEE overflow (0): still refused
            ("exp(x) - exp(x)", {"x": 1000}),  # inf - inf
        ]:
            with self.subTest(expr=expr), self.assertRaises(lc.NonFinite):
                self.sd(expr, **env)

    def test_a_name_without_a_value_is_not_a_non_finite_value(self):
        # ExprError is a ValueError: it must stay an ExprError (finite_in_double skips the
        # expression, as env_for does), not become NonFinite
        with self.assertRaises(lc.ExprError):
            self.sd("vx^2 + 1")

    def test_finite_intermediates_evaluate_as_double(self):
        self.assertEqual(self.sd("1/exp(x)", x=700), 1 / math.exp(700))
        self.assertEqual(self.sd("x*x", x=1e150), 1e150 * 1e150)
        self.assertEqual(self.sd("exp(x) - exp(x)", x=700), 0.0)

    def test_the_probe_law_rejects_only_where_an_intermediate_overflows(self):
        law = lc.load(ROOT / "laws" / "spike" / "finite_evaluation_probe.law")
        for x, ok in [(0, True), (0.5, True), (0.7, True), (1.0, False), (math.pi / 2, False),
                      (2.0, False), (2.6, True), (math.pi, True)]:
            with self.subTest(x=x):
                self.assertEqual(lc.finite_in_double(law, {"x": x}), ok)

    def test_an_output_that_overflows_is_refused(self):
        # the overflow is in the output expression only (no let)
        law = lc.parse_law("law o\nkind research\ninput x 1 range 0 1000\noutput y 1 = exp(x)\n"
                           "tolerance y 1 = 0\n")
        self.assertTrue(lc.finite_in_double(law, {"x": 700}))
        self.assertFalse(lc.finite_in_double(law, {"x": 1000}))

    def test_a_verdict_beside_a_crossing_is_in_the_band(self):
        law = lc.load(ROOT / "laws" / "spike" / "finite_evaluation_probe.law")
        x1 = float("0.78918969925706884277")  # 1000 sin(x) at the overflow of exp
        self.assertTrue(lc.near_a_finiteness_crossing(law, {"x": x1}))
        self.assertTrue(lc.near_a_finiteness_crossing(law, {"x": math.nextafter(x1, 9)}))
        for x in (0.5, 0.7, 1.0, math.pi / 2, 2.6):
            self.assertFalse(lc.near_a_finiteness_crossing(law, {"x": x}), x)
        # a crossing through basic operations only (and cos 0 = 1, sin 0 = 0, exact in every
        # libm) is the same in every implementation: not in the band
        four_bar = lc.load(ROOT / "laws" / "spike" / "four_bar_rocker_angle.law")
        self.assertFalse(lc.near_a_finiteness_crossing(
            four_bar, {"lc": 1.7939999999998988, "lco": 94.027, "lr": 5.407, "lg": 90.414, "theta2": 0.0}))

    def test_no_quantitative_probe_is_in_the_band(self):
        probes = json.loads((ROOT / "conformance" / "probes.json").read_text(encoding="utf-8"))
        checked = 0
        for p in probes:
            f = ROOT / "laws" / "spike" / f"{p['law']}.law"
            if "law_text" in p or not f.exists():
                continue
            law = lc.load(f)
            inputs = p.get("inputs") or {}
            if law["kind"] != "research" or not all(
                    isinstance(inputs.get(v["name"]), (int, float)) and not isinstance(inputs.get(v["name"]), bool)
                    for v in law["inputs"] if v["name"] not in law["lists"]):
                continue
            with self.subTest(probe=p["note"]):
                self.assertFalse(lc.near_a_finiteness_crossing(law, inputs))
            checked += 1
        self.assertGreater(checked, 0)

    def test_the_other_laws_are_finite_at_their_corners(self):
        # the rule adds no rejection to a law whose range keeps every value finite
        for f in sorted((ROOT / "laws" / "spike").glob("*.law")):
            law = lc.load(f)
            if law["kind"] != "research" or f.stem == "finite_evaluation_probe":
                continue
            for c in lc.corners(law):
                if c["type"] == "bound" and "witness" in c:
                    inputs = {**c["witness"], c["input"]: c["value"]}
                    with self.subTest(law=f.stem, corner=c["id"]):
                        self.assertTrue(lc.finite_in_double(law, inputs))


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
        self.assertFalse(cs["vertex[a=-1,n=5]"]["accept"])    # q = -5
        self.assertTrue(cs["vertex[a=-1,n=1]"]["accept"])     # m needs b: not decidable, kept
        self.assertNotIn("vertex[a=-1,n=4]", cs)              # q = -4 on the edge

    def test_integer_line_adds_parity_neighbours(self):
        without = [c["id"] for c in lc.corners(lc.parse_law(LAW.replace("x-integer n\n", "")))]
        self.assertNotIn("n@lo+1", without)
        self.assertNotIn("n:non-integer", without)

    def test_witness_and_unreachable_bound(self):
        law = lc.parse_law("law w\nkind research\ninput a 1 range 0 10\ninput b 1 range 1 10\n"
                           "let s 1 = a + b\nx-range s 0 5\n")
        cs = {c["id"]: c for c in lc.corners(law)}
        self.assertTrue(cs["a@hi"].get("unreachable"))     # a = 10 needs s >= 11
        self.assertFalse(cs["a@hi"]["accept"])
        w = cs["a@lo"]["witness"]
        self.assertEqual(w["a"], 0)
        self.assertTrue(lc.scalar_ranges_hold(law, w))
        self.assertTrue(cs["b@hi"].get("unreachable"))     # b = 10 needs s >= 10
        self.assertTrue(cs["b@lo"]["accept"])

    def test_vertex_on_an_edge_is_left_out(self):
        law = lc.parse_law("law v\nkind research\ninput a 1 range 1 2\ninput b 1 range 1 2\n"
                           "let s 1 = a + b\nx-range s 0 3\n")
        ids = [c["id"] for c in lc.corners(law)]
        self.assertNotIn("vertex[a=1,b=2]", ids)      # s = 3, on the edge
        self.assertIn("vertex[a=1,b=1]", ids)
        self.assertIn("vertex[a=2,b=2]", ids)

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
