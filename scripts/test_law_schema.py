#!/usr/bin/env python3
"""Oracle for scripts/law_schema.py: the parse of each type form, the match of
values against it (written by hand from the grammar, not from the checker), and the
failure when no x-input line is found."""
import math
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import law_schema as ls  # noqa: E402

BUILDS = "list of record(features: list of text, id: optional text)"


class Parse(unittest.TestCase):
    def test_forms(self):
        self.assertEqual(ls.parse("text"), ("text",))
        self.assertEqual(ls.parse("list of text"), ("list", ("text",)))
        self.assertEqual(ls.parse("list of list of integer"), ("list", ("list", ("integer",))))
        self.assertEqual(ls.parse(BUILDS), ("list", ("record", (
            ("features", False, ("list", ("text",))), ("id", True, ("text",))))))

    def test_rejects(self):
        for bad in ["", "texts", "list text", "record(a text)", "record(a: text", "record(a: text, a: text)",
                    "record()", "text text", "list of", "record(: text)"]:
            with self.assertRaises(ls.SchemaError, msg=bad):
                ls.parse(bad)


class Match(unittest.TestCase):
    def setUp(self):
        self.b = ls.parse(BUILDS)

    def test_well_formed(self):
        for v in [[], [{"features": []}], [{"features": ["a", "b"], "id": "x"}, {"features": ["a"]}]]:
            self.assertTrue(ls.matches(v, self.b), v)

    def test_violations_are_the_whole_input(self):
        for v in ["a", None, 3, {"features": []}, [7], [None], [{"id": "x"}], [{"features": "a"}],
                  [{"features": ["a", 3]}], [{"features": ["a"], "id": 5}], [{"features": ["a"], "id": None}],
                  [{"features": ["a"]}, {"features": ["b"], "id": True}]]:
            self.assertFalse(ls.matches(v, self.b), v)

    def test_numbers(self):
        n, i = ls.parse("number"), ls.parse("integer")
        self.assertTrue(ls.matches(1.5, n) and ls.matches(3, i) and ls.matches(3.0, i))
        for v in [True, "1", math.inf, -math.inf, math.nan]:
            self.assertFalse(ls.matches(v, n), v)
        self.assertFalse(ls.matches(2.5, i))


class Metrics(unittest.TestCase):
    def test_parse(self):
        self.assertEqual(ls.parse_metric("count(b)"), ("count", "b"))
        self.assertEqual(ls.parse_metric(" distinct( b[].id ) "), ("distinct", "b", "id"))
        self.assertEqual(ls.parse_metric("distinct(set(b[].f))"), ("distinct_set", "b", "f"))
        for bad in ["count()", "count(b[].f)", "distinct(b)", "distinct(set(b))", "sum(b)", "distinct(set(b[].f)"]:
            with self.assertRaises(ls.SchemaError, msg=bad):
                ls.parse_metric(bad)

    def test_rules(self):
        c, d, s = ("count", "b"), ("distinct", "b", "id"), ("distinct_set", "b", "f")
        two = [{"f": ["y", "x"], "id": "1"}, {"f": ["x", "y", "x"], "id": "1.0"}]
        self.assertEqual(ls.eval_metric(c, two), 2)
        self.assertEqual(ls.eval_metric(d, two), 2)  # "1" and "1.0" are different text
        self.assertEqual(ls.eval_metric(s, two), 1)  # order and duplicates ignored
        self.assertIsNone(ls.eval_metric(d, [{"f": []}, {"f": [], "id": "a"}]))  # optional absent
        self.assertEqual(ls.eval_metric(s, [{"f": []}, {"f": []}]), 1)  # an empty set is a set
        for e in (c, d, s):
            self.assertEqual(ls.eval_metric(e, []), 0)
            self.assertIsNone(ls.eval_metric(e, None))  # input not measured


class Main(unittest.TestCase):
    def test_zero_lines_fail(self):
        with tempfile.TemporaryDirectory() as d:
            Path(d, "a.law").write_text("law a\nkind audit\n", encoding="utf-8")
            self.assertEqual(ls.main([d]), 1)

    def test_spike_laws_parse(self):
        laws = Path(__file__).resolve().parent.parent / "laws" / "spike"
        self.assertEqual(ls.main([str(laws)]), 0)


if __name__ == "__main__":
    unittest.main()
