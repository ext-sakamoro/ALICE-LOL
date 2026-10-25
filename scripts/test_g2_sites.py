#!/usr/bin/env python3
"""Tests of scripts/g2_sites.py on small trees written here"""
from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import g2_sites as G  # noqa: E402

SRC = ("pub fn floor(x: f32) -> f32 {\n    crate::det_math::max(x, 0.0)\n}\n\n"
       "fn ranked(a: f32, b: f32) -> f32 {\n    // det_math::min(a, b) in a comment is not a call\n"
       "    crate::det_math::min(a, b) + crate::det_vec::max_element(v)\n}\n")
TEST = "#[test]\nfn floor_of_negative_zero() {}\n"
TABLE = ('[[site]]\nfile = "alice-lol/src/lib.rs"\nfn = "floor"\ncalls = 1\n'
         'tests = ["alice-lol/tests/ties.rs::floor_of_negative_zero"]\n\n'
         '[[site]]\nfile = "alice-lol/src/lib.rs"\nfn = "ranked"\ncalls = 2\n'
         'reason = "only compared, so the zero sign is not observable"\n')


class G2Sites(unittest.TestCase):
    def tree(self, src=SRC, table=TABLE, test=TEST):
        d = Path(tempfile.mkdtemp())
        (d / "alice-lol/src").mkdir(parents=True)
        (d / "alice-lol/tests").mkdir(parents=True)
        (d / "scripts").mkdir()
        (d / "alice-lol/src/lib.rs").write_text(src, encoding="utf-8")
        (d / "alice-lol/src/det_math.rs").write_text("pub fn max() { det_math::max(1, 2) }\n", encoding="utf-8")
        (d / "alice-lol/tests/ties.rs").write_text(test, encoding="utf-8")
        if table is not None:
            (d / "scripts/g2_sites.toml").write_text(table, encoding="utf-8")
        return d

    def code(self, **kw):
        return G.check(self.tree(**kw))[:2]

    def test_a_reviewed_table_passes(self):
        code, problems = self.code()
        self.assertEqual((code, problems), (0, []))

    def test_the_sites_are_counted_per_fn_without_comments_and_helpers(self):
        self.assertEqual(G.sites(self.tree()), {("alice-lol/src/lib.rs", "floor"): 1, ("alice-lol/src/lib.rs", "ranked"): 2})

    def test_a_site_without_a_row_fails(self):
        src = SRC + "fn more(x: f32) -> f32 {\n    crate::det_math::min64(x, 1.0)\n}\n"
        code, problems = self.code(src=src)
        self.assertEqual(code, 1)
        self.assertTrue(any("lib.rs::more: 1 Law max/min call(s) without a row" in p for p in problems), problems)

    def test_a_count_that_differs_fails(self):
        code, problems = self.code(src=SRC.replace("crate::det_math::max(x, 0.0)", "crate::det_math::max(crate::det_math::max(x, 0.0), 1.0)"))
        self.assertEqual(code, 1)
        self.assertTrue(any("2 calls, the row says 1" in p for p in problems), problems)

    def test_a_stale_row_fails(self):
        code, problems = self.code(table=TABLE + '\n[[site]]\nfile = "alice-lol/src/lib.rs"\nfn = "gone"\ncalls = 1\nreason = "a site that was removed from the library"\n')
        self.assertEqual(code, 1)
        self.assertTrue(any("no such site" in p for p in problems), problems)

    def test_a_missing_test_fails(self):
        code, problems = self.code(test="fn floor_of_negative_zero() {}\n")
        self.assertEqual(code, 1)
        self.assertTrue(any("is not a #[test] fn" in p for p in problems), problems)

    def test_a_row_with_neither_tests_nor_reason_fails(self):
        code, _ = self.code(table=TABLE.replace('reason = "only compared, so the zero sign is not observable"\n', ""))
        self.assertEqual(code, 1)

    def test_a_second_row_for_a_site_fails(self):
        code, problems = self.code(table=TABLE + TABLE.split("\n\n")[0] + "\n")
        self.assertEqual(code, 1)
        self.assertTrue(any("a second row" in p for p in problems), problems)

    def test_no_site_compared_fails(self):
        self.assertEqual(self.code(src="pub fn none() {}\n", table="")[0], 2)

    def test_a_table_that_does_not_read_fails(self):
        self.assertEqual(self.code(table="[[site]\n")[0], 2)

    def test_this_repository_passes(self):
        self.assertEqual(G.check(G.ROOT)[0], 0)


if __name__ == "__main__":
    unittest.main()
