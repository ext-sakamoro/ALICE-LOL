#!/usr/bin/env python3
"""gen_gbnf.py の試験 (python3 scripts/test_gen_gbnf.py)"""
from __future__ import annotations

import importlib.util
import sys
import tomllib
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("gg", HERE / "gen_gbnf.py")
gg = importlib.util.module_from_spec(spec)
sys.modules["gg"] = gg
spec.loader.exec_module(gg)

TEMPLATE = """\
root ::= expr
intent_grasp ::= "grasp" ws "(" ws hand ws ")"
@names name_1f@
@names name_ab@
@names hand@
@names unit_length@
@names unit_render_only@
@names purpose@
"""

UNITS = [{"name": "mm", "mm": 1.0}, {"name": "in", "mm": 25.4}, {"name": "world"}]


def definition(*rows: tuple[str, str], hands=("left", "right"), purposes=("print", "render"),
               units=UNITS) -> dict:
    return {"construct": [{"name": n, "rule": r} for n, r in rows], "hand": list(hands),
            "purpose": list(purposes), "unit": list(units)}


GOOD = definition(("sphere", "name_1f"), ("heart", "name_1f"), ("subtract", "name_ab"), ("grasp", "intent_grasp"))


class Render(unittest.TestCase):
    def test_writes_each_rule_one_name_per_line_in_row_order(self):
        out = gg.render(GOOD, TEMPLATE)
        self.assertIn('name_1f ::= "sphere"\n          | "heart"\n', out)
        self.assertIn('name_ab ::= "subtract"\n', out)
        self.assertIn('hand ::= "left"\n       | "right"\n', out)
        self.assertIn('unit_length ::= "mm"\n              | "in"\n', out)
        self.assertIn('unit_render_only ::= "world"\n', out)
        self.assertIn('purpose ::= "print"\n          | "render"\n', out)
        self.assertNotIn("@names", out)

    def test_a_dropped_row_drops_its_name(self):
        rows = [r for r in GOOD["construct"] if r["name"] != "heart"]
        out = gg.render({**GOOD, "construct": rows}, TEMPLATE)
        self.assertNotIn('"heart"', out)

    def test_row_order_is_the_output_order(self):
        rows = list(reversed(GOOD["construct"]))
        out = gg.render({**GOOD, "construct": rows}, TEMPLATE)
        self.assertIn('name_1f ::= "heart"\n          | "sphere"\n', out)


class Disagreement(unittest.TestCase):
    def fails(self, d: dict, template: str = TEMPLATE, message: str = ""):
        with self.assertRaises(gg.DefinitionError) as cm:
            gg.render(d, template)
        self.assertIn(message, str(cm.exception))

    def test_no_rows(self):
        self.fails(definition(), message="no construct rows")

    def test_no_hand_values(self):
        self.fails(definition(("sphere", "name_1f"), ("subtract", "name_ab"), hands=()), message="no hand values")

    def test_no_purpose_values_or_no_render(self):
        base = (("sphere", "name_1f"), ("subtract", "name_ab"))
        self.fails(definition(*base, purposes=()), message="no purpose values")
        self.fails(definition(*base, purposes=("print",)), message="no purpose values")

    def test_no_unit_rows(self):
        self.fails(definition(("sphere", "name_1f"), ("subtract", "name_ab"), units=()), message="no unit rows")

    def test_a_unit_with_a_bad_scale_or_name(self):
        base = (("sphere", "name_1f"), ("subtract", "name_ab"))
        for bad in ({"name": "mm", "mm": 0.0}, {"name": "mm", "mm": -1.0}, {"name": "mm", "mm": 1}):
            self.fails(definition(*base, units=[bad, {"name": "world"}]), message="positive float")
        self.fails(definition(*base, units=[{"name": "sphere", "mm": 1.0}, {"name": "world"}]),
                   message="is taken")
        self.fails(definition(*base, units=[{"name": "mm", "mm": 1.0}, {"name": "mm", "mm": 2.0},
                                            {"name": "world"}]), message="is taken")

    def test_args_that_do_not_fit_the_rule(self):
        def d(args):
            return {**definition(("sphere", "name_1f"), ("subtract", "name_ab")),
                    "construct": [{"name": "sphere", "rule": "name_1f", "args": args},
                                  {"name": "subtract", "rule": "name_ab"}]}
        gg.render(d("L"), TEMPLATE)
        for bad in ("", "LL", "X"):
            self.fails(d(bad), message="do not fit rule")

    def test_a_name_twice(self):
        self.fails(definition(("sphere", "name_1f"), ("sphere", "name_ab")), message="defined twice")

    def test_a_rule_with_no_place(self):
        d = definition(("sphere", "name_1f"), ("subtract", "name_ab"), ("torus", "name_2f"))
        self.fails(d, message="no @names place")

    def test_a_place_with_no_rows(self):
        self.fails(definition(("sphere", "name_1f")), message="places with no rows")

    def test_a_place_twice(self):
        self.fails(GOOD, TEMPLATE + "@names name_1f@\n", message="appears twice")

    def test_an_intent_row_the_template_does_not_spell(self):
        d = definition(("sphere", "name_1f"), ("subtract", "name_ab"), ("throw", "intent_grasp"))
        self.fails(d, message="does not spell")

    def test_a_name_that_is_not_an_identifier(self):
        self.fails(definition(("Sphere", "name_1f"), ("subtract", "name_ab")), message="not a lower-case")


class Repository(unittest.TestCase):
    """the files in this repository"""

    def test_the_definition_has_rows_and_generates_without_error(self):
        with gg.DEFINITION.open("rb") as f:
            d = tomllib.load(f)
        self.assertGreater(len(d["construct"]), 100)
        self.assertTrue(gg.generate().startswith("# ALICE-LOL DSL grammar"))

    def test_every_output_is_the_generated_text(self):
        text = gg.generate()
        paths = gg.outputs()
        self.assertGreaterEqual(len(paths), 2, "the canonical grammar and at least its distribution copy")
        self.assertEqual(paths[0], gg.CANONICAL)
        for path in paths:
            self.assertEqual(path.read_text(encoding="utf-8"), text, path)


if __name__ == "__main__":
    unittest.main()
