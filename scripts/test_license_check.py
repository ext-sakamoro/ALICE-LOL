#!/usr/bin/env python3
"""scripts/license_check.py の試験.

架空の 2 crate の workspace を一時 directory に作り、1 箇所だけ崩して検査器の出口を確かめる
"""
from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import license_check as lc  # noqa: E402


class LicenseCheck(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "Cargo.toml").write_text('[workspace]\nmembers = ["a", "b"]\n', encoding="utf-8")
        for name, extra in [("a", ""), ("b", "publish = false\n")]:
            (self.root / name).mkdir()
            (self.root / name / "Cargo.toml").write_text(
                f'[package]\nname = "sample-{name}"\nlicense = "Apache-2.0"\n{extra}', encoding="utf-8")
        for f in lc.TEXTS:
            (self.root / f).write_text(f"{f} text\n", encoding="utf-8")
            (self.root / "a" / f).write_text(f"{f} text\n", encoding="utf-8")
        self.saved = lc.ROOT, lc.tracked
        lc.ROOT = self.root
        lc.tracked = lambda: sorted(p.relative_to(self.root).as_posix() for p in self.root.rglob("*") if p.is_file())

    def tearDown(self):
        lc.ROOT, lc.tracked = self.saved
        self.tmp.cleanup()

    def test_a_correct_workspace_passes(self):
        self.assertEqual(lc.main([]), 0)

    def test_a_crate_still_dual_licensed_fails(self):
        p = self.root / "b" / "Cargo.toml"
        p.write_text(p.read_text(encoding="utf-8").replace('"Apache-2.0"', '"MIT OR Apache-2.0"'), encoding="utf-8")
        self.assertEqual(lc.main([]), 1)

    def test_an_mit_text_fails(self):
        (self.root / "LICENSE-MIT").write_text("MIT License\n", encoding="utf-8")
        self.assertEqual(lc.main([]), 1)

    def test_an_mit_text_anywhere_in_the_tree_fails(self):
        (self.root / "b" / "docs").mkdir()
        grant = "Permission is hereby granted, free" + " of charge, to any person"
        (self.root / "b" / "docs" / "LICENSE").write_text(f"{grant}\n", encoding="utf-8")
        self.assertEqual(lc.main([]), 1)

    def test_an_mit_title_line_fails(self):
        (self.root / "b" / "COPYING").write_text("MIT" + " License\n\nCopyright\n", encoding="utf-8")
        self.assertEqual(lc.main([]), 1)

    def test_an_mit_expression_in_prose_is_not_a_licence_text(self):
        (self.root / "b" / "README.md").write_text("0.3.x was MIT OR Apache-2.0\n", encoding="utf-8")
        self.assertEqual(lc.main([]), 0)

    def test_no_tracked_file_fails(self):
        lc.tracked = lambda: []
        self.assertEqual(lc.main([]), 2)

    def test_a_copy_that_differs_from_the_root_fails(self):
        (self.root / "a" / "NOTICE").write_text("other\n", encoding="utf-8")
        self.assertEqual(lc.main([]), 1)

    def test_a_missing_copy_in_a_published_crate_fails(self):
        (self.root / "a" / "TRADEMARK_NOTICE").unlink()
        self.assertEqual(lc.main([]), 1)

    def test_an_unpublished_crate_needs_no_copies(self):
        self.assertFalse((self.root / "b" / "NOTICE").exists())
        self.assertEqual(lc.main([]), 0)

    def test_no_published_apache_crate_checks_nothing(self):
        p = self.root / "a" / "Cargo.toml"
        p.write_text(p.read_text(encoding="utf-8") + "publish = false\n", encoding="utf-8")
        self.assertEqual(lc.main([]), 2)


if __name__ == "__main__":
    unittest.main()
