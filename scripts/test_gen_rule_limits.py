#!/usr/bin/env python3
"""Tests of scripts/gen_rule_limits.py on temporary copies of the table, the manifest and the seeds"""
from __future__ import annotations

import shutil
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gen_rule_limits as G  # noqa: E402


class GenRuleLimits(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.saved = (G.TABLE, G.MANIFEST, G.SEEDS)
        G.TABLE = self.tmp / "rule-limits.toml"
        G.MANIFEST = self.tmp / "rule_limits.json"
        G.SEEDS = self.tmp / "seeds"
        shutil.copy(self.saved[0], G.TABLE)
        self.assertEqual(G.main(["--write"]), 0)

    def tearDown(self):
        G.TABLE, G.MANIFEST, G.SEEDS = self.saved
        shutil.rmtree(self.tmp)

    def test_the_written_files_check(self):
        self.assertEqual(G.main(["--check"]), 0)

    def test_a_changed_limit_fails_the_check(self):
        G.TABLE.write_text(G.TABLE.read_text().replace("expression_depth = 64", "expression_depth = 63"))
        self.assertEqual(G.main(["--check"]), 1)

    def test_an_edited_manifest_or_a_stray_seed_fails_the_check(self):
        G.MANIFEST.write_text(G.MANIFEST.read_text().replace('"reads"', '"unreadable"', 1))
        self.assertEqual(G.main(["--check"]), 1)
        G.main(["--write"])
        (G.SEEDS / "stray").write_bytes(b"x")
        self.assertEqual(G.main(["--check"]), 1)

    def test_every_limit_has_entries_below_at_and_past_it(self):
        lim = {"law_bytes": 100, "law_lines": 10, "expression_tokens": 8, "expression_depth": 4,
               "numeral_chars": 5, "json_depth": 6}
        names = [e["name"] for e in G.entries(lim)]
        for n in ("bytes_99", "bytes_100", "bytes_101", "lines_9", "lines_10", "lines_11", "tokens_7", "tokens_8",
                  "tokens_9", "parens_3", "parens_4", "parens_5", "example_numeral_4", "example_numeral_5",
                  "example_numeral_6", "example_depth_5", "example_depth_6", "example_depth_7"):
            self.assertIn(n, names)

    def test_a_table_that_does_not_read_exits_2(self):
        G.TABLE.write_text("law_bytes = \n")
        self.assertEqual(G.main(["--check"]), 2)


if __name__ == "__main__":
    unittest.main()
