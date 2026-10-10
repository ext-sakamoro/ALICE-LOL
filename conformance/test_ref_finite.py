#!/usr/bin/env python3
"""conformance/ref_impl.py: a non-finite value of a quantitative law is a rejection.

TASK.md: every intermediate value of a quantitative law is finite and the output is a
finite number; otherwise the request is rejected (`non-finite value`). The reference
implementation computes each law by hand, so Python's ways of not being finite are mapped
in one place (`main`): an exception (`OverflowError` from `math.exp` / `**`, `ValueError`
for a domain error, `ZeroDivisionError`) and an infinity or NaN that ends in an output.
Each is checked here with a stand-in law, and an audit (which never rejects) keeps the
error.
"""
from __future__ import annotations

import io
import json
import math
import os
import sys
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ref_impl  # noqa: E402


def run(law_name: str, laws: dict) -> dict:
    request = json.dumps({"law": law_name, "inputs": {"x": 1.0}}).encode()
    out = io.StringIO()
    with mock.patch.dict(ref_impl.LAWS, laws, clear=False), \
            mock.patch.object(sys, "stdin", mock.Mock(buffer=io.BytesIO(request))), \
            redirect_stdout(out):
        ref_impl.main()
    return json.loads(out.getvalue())


class NonFinite(unittest.TestCase):
    def test_each_way_of_not_being_finite_is_a_rejection(self):
        cases = {
            "exp overflow": lambda i: {"y": math.exp(1000 * i["x"])},
            "power overflow": lambda i: {"y": 10.0 ** (400 * i["x"])},
            "ln of a negative number": lambda i: {"y": math.log(-i["x"])},
            "sqrt of a negative number": lambda i: {"y": math.sqrt(-i["x"])},
            "division by zero": lambda i: {"y": 1 / (i["x"] - 1)},
            "an infinite output": lambda i: {"y": 1e308 * 10 * i["x"]},
            "a NaN output": lambda i: {"y": (1e308 * 10) - (1e308 * 10)},
            "a NaN inside a list output": lambda i: {"y": [1.0, math.nan]},
        }
        for name, law in cases.items():
            with self.subTest(name=name):
                self.assertEqual(run("stand_in", {"stand_in": law}), {"rejected": "non-finite value"})

    def test_a_finite_output_is_written(self):
        self.assertEqual(run("stand_in", {"stand_in": lambda i: {"y": [i["x"], 2.0]}}),
                         {"outputs": {"y": [1.0, 2.0]}})

    def test_the_probe_law_rejects_its_overflowing_inputs(self):
        for x, rejected in [(0.5, False), (1.0, True), (math.pi / 2, True), (2.6, False)]:
            with self.subTest(x=x):
                out = ref_impl.finite_probe({"x": x}) if not rejected else None
                if rejected:
                    with self.assertRaises(OverflowError):
                        ref_impl.finite_probe({"x": x})
                else:
                    self.assertTrue(math.isfinite(out["y"]))

    def test_the_verdict_at_the_crossings_is_the_correctly_rounded_one(self):
        # the fixture is shared with alice-lol/tests/finite_evaluation_rule.rs (Rust must agree)
        doc = json.loads((Path(__file__).with_name("finite_probe_crossings.json")).read_text(encoding="utf-8"))
        self.assertEqual(len(doc["points"]), 166)
        for row in doc["points"]:
            with self.subTest(x=row["x"]):
                try:
                    ref_impl.finite_probe({"x": row["x"]})
                    rejected = False
                except OverflowError:
                    rejected = True
                self.assertEqual(rejected, row["rejected"])

    def test_four_bar_computes_the_distance_as_written(self):
        # the law writes d = sqrt(dx^2 + dy^2); math.hypot(dx, dy) rounds differently, and at
        # this input the angle differs in its last bits (1.7962666500161355 with hypot)
        out = ref_impl.four_bar({"lc": 1.0, "lco": 2.0, "lr": 1.5, "lg": 2.3, "theta2": 1.552})
        self.assertEqual(out["theta4"].hex(), (1.7962666500161348).hex())

    def test_an_audit_keeps_the_error(self):
        # an audit never rejects: a domain error inside it is an error, not a rejection
        def broken(_inputs):
            raise ValueError("math domain error")
        with mock.patch.object(ref_impl, "read_audit_law", return_value={"stub": True}), \
                mock.patch.object(ref_impl, "generic_audit", return_value=broken), \
                self.assertRaises(ValueError):
            run("an_audit", {})


if __name__ == "__main__":
    unittest.main()
