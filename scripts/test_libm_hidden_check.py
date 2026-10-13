#!/usr/bin/env python3
"""Tests for scripts/libm_hidden_check.py (the rewrite and the reading of clippy's
messages; the scratch-copy clippy run itself needs cargo and runs in CI and preflight
through `python3 scripts/libm_hidden_check.py`)."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import libm_hidden_check as h  # noqa: E402


def msg(path: str, text: str, code: str = "clippy::disallowed_methods") -> dict:
    return {
        "code": {"code": code},
        "message": text,
        "spans": [{"is_primary": True, "file_name": path, "line_start": 7}],
    }


class Neutralise(unittest.TestCase):
    def test_every_mention_of_the_lint_is_replaced_and_counted(self):
        src = (
            '#[allow(clippy::disallowed_methods, reason = "x")]\nfn a() {}\n'
            "#![allow(clippy::disallowed_methods)]\n"
        )
        out, n = h.neutralise(src)
        self.assertEqual(n, 2)
        self.assertNotIn(h.LINT, out)
        self.assertIn(h.STAND_IN, out)


class Judge(unittest.TestCase):
    def test_a_libm_method_call_diagnostic_is_a_hit(self):
        hits, seen = h.judge([msg("src/a.rs", "use of a disallowed method `f32::sin`")])
        self.assertEqual((len(hits), seen), (1, 1))

    def test_each_libm_name_is_recognised(self):
        for name in ("sin", "cos", "tan", "atan2", "exp", "ln", "log", "powf", "sin_cos"):
            hits, _ = h.judge([msg("src/a.rs", f"use of a disallowed method `f32::{name}`")])
            self.assertEqual(len(hits), 1, name)

    def test_a_max_min_diagnostic_is_not_a_hit(self):
        hits, seen = h.judge([msg("src/a.rs", "use of a disallowed method `f32::max`")])
        self.assertEqual((hits, seen), ([], 1))

    def test_other_lints_are_not_counted_at_all(self):
        hits, seen = h.judge([msg("src/a.rs", "use of a disallowed method `f32::sin`", code="clippy::useless_vec")])
        self.assertEqual((hits, seen), ([], 0))

    def test_f64_libm_names_are_also_recognised(self):
        hits, _ = h.judge([msg("src/a.rs", "use of a disallowed method `f64::atan2`")])
        self.assertEqual(len(hits), 1)


if __name__ == "__main__":
    unittest.main()
