#!/usr/bin/env python3
"""Tests of scripts/src_gap_check.py on small repositories written here (the `--run` mode with a
runner that stands in for cargo)"""
from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import src_gap_check as G  # noqa: E402


def _tmpdir(**kwargs) -> str:
    """A temporary directory removed when this module's tests end (fixtures are read after the call returns)"""
    td = tempfile.TemporaryDirectory(**kwargs)
    unittest.addModuleCleanup(td.cleanup)
    return td.name

TABLE = ("# Wiring debt\n\n| key | what is not wired | closes when |\n|---|---|---|\n"
         "| `glue-a` | a is not wired | a is wired |\n")
TEST = ('#[test]\n#[ignore = "src gap: glue-a: a is not wired"]\nfn a_is_wired() {\n    assert!(false);\n}\n')


class Result:
    def __init__(self, code, out):
        self.returncode, self.stdout, self.stderr = code, out, ""


def fake(code, passed, failed):
    def run(cmd, cwd, capture_output, text):
        assert cmd[:2] == ["cargo", "test"] and "--ignored" in cmd and "--exact" in cmd, cmd
        return Result(code, f"test result: {'ok' if code == 0 else 'FAILED'}. {passed} passed; {failed} failed; 0 ignored")
    return run


class SrcGap(unittest.TestCase):
    def repo(self, table=TABLE, files=None):
        d = Path(_tmpdir())
        subprocess.run(["git", "init", "-q", str(d)], check=True)
        if table is not None:
            (d / "docs").mkdir()
            (d / "docs" / "wiring-debt.md").write_text(table, encoding="utf-8")
        for rel, text in (files if files is not None else {"crate/tests/gap.rs": TEST}).items():
            p = d / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text, encoding="utf-8")
        subprocess.run(["git", "-C", str(d), "add", "-A"], check=True)
        return d

    def test_a_used_row_passes(self):
        self.assertEqual(G.check(self.repo(), False)[0], 0)

    def test_a_failing_src_gap_test_passes_the_run(self):
        self.assertEqual(G.check(self.repo(), True, fake(101, 0, 1))[0], 0)

    def test_a_src_gap_test_that_passes_is_stale(self):
        code, problems = G.check(self.repo(), True, fake(0, 1, 0))
        self.assertEqual(code, 1)
        self.assertTrue(any("the gap closed" in p for p in problems), problems)

    def test_a_run_of_no_test_fails(self):
        code, problems = G.check(self.repo(), True, fake(0, 0, 0))
        self.assertEqual(code, 1)
        self.assertTrue(any("ran 0 tests" in p for p in problems), problems)

    def test_an_ignore_without_a_row_fails(self):
        files = {"crate/tests/gap.rs": TEST + TEST.replace("glue-a", "glue-b").replace("a_is_wired", "b_is_wired")}
        code, problems = G.check(self.repo(files=files), False)
        self.assertEqual(code, 1)
        self.assertTrue(any("`glue-b` has no row" in p for p in problems), problems)

    def test_a_row_without_an_ignore_fails(self):
        code, problems = G.check(self.repo(table=TABLE + "| `glue-b` | b | b is wired |\n"), False)
        self.assertEqual(code, 1)
        self.assertTrue(any("`glue-b` is named by no" in p for p in problems), problems)

    def test_rows_but_no_ignore_compared_fails(self):
        self.assertEqual(G.check(self.repo(files={"crate/src/lib.rs": "fn x() {}\n"}), False)[0], 2)

    def test_a_missing_table_fails(self):
        self.assertEqual(G.check(self.repo(table=None), False)[0], 2)

    def test_an_empty_table_with_no_ignore_passes(self):
        empty = "| key | what is not wired | closes when |\n|---|---|---|\n"
        self.assertEqual(G.check(self.repo(table=empty, files={"crate/src/lib.rs": "fn x() {}\n"}), False)[0], 0)

    def test_the_forms_that_do_not_read(self):
        for table, why in ((TABLE.replace("`glue-a`", "`Glue_A`"), "not kebab-case"),
                           (TABLE + "| `glue-a` | again | again |\n", "a second row"),
                           (TABLE.replace("| a is wired |", "|  |"), "an empty cell")):
            self.assertEqual(G.check(self.repo(table=table), False)[0], 1, why)
        for test, why in ((TEST.replace("src gap: glue-a: a", "src gap glue-a a"), "no colons"),
                          (TEST.replace("src gap: glue-a:", "src gap: glue-a"), "no colon after the key"),
                          (TEST.replace("fn a_is_wired", "const A: u8 = 0; //"), "no fn after it")):
            code, problems = G.check(self.repo(files={"crate/tests/gap.rs": test}), False)
            self.assertNotEqual(code, 0, why)
            self.assertTrue(any("write `#[ignore" in p or "no `fn` follows" in p for p in problems), (why, problems))

    def test_a_src_gap_test_outside_a_tests_target_fails_the_run(self):
        code, problems = G.check(self.repo(files={"crate/src/lib.rs": TEST}), True, fake(101, 0, 1))
        self.assertEqual(code, 1)
        self.assertTrue(any("<crate>/tests/<target>.rs" in p for p in problems), problems)

    def test_an_ignore_over_several_lines_is_found_and_run(self):
        split = '#[test]\n#[ignore =\n    "src gap: glue-a: a is not wired"]\nfn a_is_wired() {\n    assert!(false);\n}\n'
        repo = self.repo(files={"crate/tests/gap.rs": split})
        self.assertEqual(G.check(repo, False)[0], 0)
        code, problems = G.check(repo, True, fake(0, 1, 0))
        self.assertEqual(code, 1, "a stale gap in the split form is caught")
        self.assertTrue(any("the gap closed" in p for p in problems), problems)

    def test_src_gap_text_outside_an_ignore_fails(self):
        files = {"crate/tests/gap.rs": TEST + "// src gap: glue-a: mentioned in a comment\n"}
        code, problems = G.check(self.repo(files=files), False)
        self.assertEqual(code, 1)
        self.assertTrue(any("outside an `#[ignore" in p for p in problems), problems)
        # an ignore written as a string the checker does not read is a finding, not silence
        odd = TEST.replace('#[ignore = "src gap: glue-a: a is not wired"]', '#[ignore(reason = "src gap: glue-a: x")]')
        code, problems = G.check(self.repo(files={"crate/tests/gap.rs": odd}), False)
        self.assertNotEqual(code, 0)
        self.assertTrue(any("outside an `#[ignore" in p for p in problems), problems)

    def test_this_repository_passes(self):
        self.assertEqual(G.check(G.ROOT, False)[0], 0)


if __name__ == "__main__":
    unittest.main()
