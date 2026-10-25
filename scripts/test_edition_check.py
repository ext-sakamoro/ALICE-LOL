#!/usr/bin/env python3
"""Tests of scripts/edition_check.py on a copy of laws/demo-tax with one thing broken at a time."""
import contextlib
import io
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import edition_check as ec  # noqa: E402

REAL = HERE.parent


class EditionCheck(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        shutil.copytree(REAL / "laws" / "demo-tax", self.root / "laws" / "demo-tax")
        (self.root / "alice-lol" / "src").mkdir(parents=True)
        shutil.copy(REAL / "alice-lol" / "src" / "rule_law.rs", self.root / "alice-lol" / "src" / "rule_law.rs")
        self.saved, ec.ROOT = ec.ROOT, self.root

    def tearDown(self):
        ec.ROOT = self.saved
        self.tmp.cleanup()

    def write(self):
        with contextlib.redirect_stdout(io.StringIO()):
            ec.main(["--write"])

    def check(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(out):
            try:
                code = ec.main(["--check"])
            except SystemExit as e:
                code = f"exit {e}"
        return code, out.getvalue()

    def edit(self, rel, old, new):
        p = self.root / "laws" / "demo-tax" / rel
        s = p.read_text(encoding="utf-8")
        self.assertIn(old, s)
        p.write_text(s.replace(old, new, 1), encoding="utf-8")

    def test_the_fixture_passes_and_compares_something(self):
        code, out = self.check()
        self.assertEqual(code, 0, out)
        self.assertIn("3 edition(s), 21 example(s) compared, 0 problem(s)", out)

    def test_a_missing_changes_line_fails(self):
        self.edit("y2026/rate.delta", "changes 05ca78016716", "# changes 05ca78016716")
        code, out = self.check()
        self.assertEqual(code, 1)
        self.assertIn("but the answers that differ from y2025", out)

    def test_a_changes_line_naming_no_difference_fails(self):
        self.edit("y2027/rate.delta", "changes 273ed844682f", "changes 273ed844682f\nchanges 05ca78016716")
        self.assertEqual(self.check()[0], 1)

    def test_an_edited_frozen_answer_fails(self):
        self.edit("y2025/examples.jsonl", '"value":"reduced"', '"value":"standard"')
        code, out = self.check()
        self.assertEqual(code, 1)
        self.assertIn("EDITIONS.toml is not what the editions give", out)
        # with the ledger rewritten over it, the replay alone still fails
        self.write()
        code, out = self.check()
        self.assertEqual(code, 1, out)
        self.assertIn("the edition answers", out)

    def test_an_edited_old_edition_fails(self):
        self.edit("y2025/rate.law", "amount <= 100000", "amount <= 100001")
        self.assertNotEqual(self.check()[0], 0)

    def test_a_gap_between_editions_fails(self):
        self.edit("y2025/rate.law", "until 1767225600", "until 1767225000")
        code, out = self.check()
        self.assertNotEqual(code, 0)

    def test_the_period_check_itself(self):
        # moving `until` in the file also moves the identifier (above); here only the period moves
        sem = ec.rule_law_ids.semantics((self.root / "alice-lol/src/rule_law.rs").read_text(encoding="utf-8"))
        chain = ec.editions(self.root / "laws" / "demo-tax", sem)
        self.assertEqual(ec.problems(self.root / "laws" / "demo-tax", chain)[0], [])
        chain[0].until -= 1  # a gap
        self.assertIn("a gap or an overlap", " ".join(ec.problems(self.root / "laws" / "demo-tax", chain)[0]))
        chain[0].until += 2  # an overlap
        self.assertIn("a gap or an overlap", " ".join(ec.problems(self.root / "laws" / "demo-tax", chain)[0]))
        chain[0].until = None  # an open edition that is superseded
        self.assertIn("a gap or an overlap", " ".join(ec.problems(self.root / "laws" / "demo-tax", chain)[0]))

    def test_a_name_that_is_not_its_inputs_fails(self):
        self.edit("y2025/examples.jsonl", '"name":"', '"name":"0')
        self.write()
        code, out = self.check()
        self.assertEqual(code, 1, out)
        self.assertIn("is not the name of its inputs", out)

    def test_empty_corpora_compare_nothing_and_fail(self):
        for e in ("y2025", "y2026", "y2027"):
            (self.root / "laws" / "demo-tax" / e / "examples.jsonl").write_text("", encoding="utf-8")
        for e in ("y2026", "y2027"):  # no example differs now
            p = self.root / "laws" / "demo-tax" / e / "rate.delta"
            p.write_text("".join(ln + "\n" for ln in p.read_text(encoding="utf-8").splitlines()
                                 if not ln.startswith("changes")), encoding="utf-8")
        self.write()
        code, out = self.check()
        self.assertEqual(code, 1, out)
        self.assertIn("no example compared", out)

    def test_a_removed_corpus_fails(self):
        (self.root / "laws" / "demo-tax" / "y2026" / "examples.jsonl").unlink()
        self.assertNotEqual(self.check()[0], 0)

    def test_a_frozen_corpus_is_never_written_again(self):
        with self.assertRaises(SystemExit):
            ec.freeze(self.root / "laws" / "demo-tax" / "y2025", ec.rule_law_ids.semantics(
                (self.root / "alice-lol/src/rule_law.rs").read_text(encoding="utf-8")))

    def test_no_rule_set_fails(self):
        shutil.rmtree(self.root / "laws" / "demo-tax")
        self.assertNotEqual(self.check()[0], 0)


if __name__ == "__main__":
    unittest.main()
