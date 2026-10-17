#!/usr/bin/env python3
"""gen_units_doc.py の試験 (python3 scripts/test_gen_units_doc.py)"""
from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("gud", HERE / "gen_units_doc.py")
gud = importlib.util.module_from_spec(spec)
sys.modules["gud"] = gud
spec.loader.exec_module(gud)

GOOD = {"purpose": ["print", "render"],
        "unit": [{"name": "mm", "mm": 1.0}, {"name": "in", "mm": 25.4}, {"name": "world"}]}


class Render(unittest.TestCase):
    def test_one_row_per_unit_in_order_with_its_scale_and_purposes(self):
        out = gud.render(GOOD)
        self.assertIn("| `mm` | 1 mm | `print`, `render` |\n| `in` | 25.4 mm | `print`, `render` |\n"
                      "| `world` | no physical size | `render` only |", out)
        self.assertTrue(out.startswith(gud.BEGIN) and out.endswith(gud.END))
        self.assertIn("units(mm, print, cylinder(50, 5))", out)
        self.assertIn("units(world, render, <program>)", out)

    def test_a_dropped_unit_drops_its_row(self):
        out = gud.render({**GOOD, "unit": [GOOD["unit"][0], GOOD["unit"][2]]})
        self.assertNotIn("`in`", out)

    def test_no_units_or_no_purposes(self):
        for d in ({**GOOD, "unit": []}, {**GOOD, "purpose": []}):
            with self.assertRaises(gud.DefinitionError):
                gud.render(d)


class Replace(unittest.TestCase):
    def test_replaces_only_between_the_markers(self):
        text = f"head\n{gud.BEGIN}\nold\n{gud.END}\ntail\n"
        self.assertEqual(gud.replace(text, "NEW", "x"), "head\nNEW\ntail\n")

    def test_a_marker_mentioned_inside_a_line_is_not_a_marker(self):
        text = f"see `{gud.BEGIN}` in prose\n{gud.BEGIN}\nold\n{gud.END}\n"
        self.assertEqual(gud.replace(text, "NEW", "x"), f"see `{gud.BEGIN}` in prose\nNEW\n")
        with self.assertRaises(gud.DefinitionError):
            gud.replace(f"see `{gud.BEGIN}` and `{gud.END}`\n", "NEW", "x")

    def test_markers_missing_twice_or_reversed(self):
        for text in (f"{gud.BEGIN}\n", f"{gud.END}\n{gud.BEGIN}\n",
                     f"{gud.BEGIN}\n{gud.END}\n{gud.BEGIN}\n{gud.END}\n"):
            with self.assertRaises(gud.DefinitionError):
                gud.replace(text, "NEW", "x")


class Repository(unittest.TestCase):
    def test_this_repository_matches_and_compares_documents(self):
        self.assertGreaterEqual(len(gud.documents()), 2)
        self.assertEqual(gud.main(["gen_units_doc.py", "--check"]), 0)


if __name__ == "__main__":
    unittest.main()
