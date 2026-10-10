#!/usr/bin/env python3
"""scripts/license_check.py の試験.

架空の 2 crate の workspace を一時 directory に作り、1 箇所だけ崩して検査器の出口を確かめる
旧 license の名前と許諾文は `lc.OLD` と断片から組み立て、この file 自身が検査に当たらないようにする
"""
from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import license_check as lc  # noqa: E402

OLD = lc.OLD
low = OLD.lower()
GRANT = "Permission is hereby " "granted, free of " "charge"


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
        self.saved = lc.ROOT, lc.tracked, lc.ALLOWED
        lc.ROOT = self.root
        lc.tracked = lambda: sorted(p.relative_to(self.root).as_posix() for p in self.root.rglob("*") if p.is_file())
        lc.ALLOWED = []

    def tearDown(self):
        lc.ROOT, lc.tracked, lc.ALLOWED = self.saved
        self.tmp.cleanup()

    def write(self, rel: str, text: str) -> None:
        (self.root / rel).write_text(f"intro\n{text}\n", encoding="utf-8")

    def test_a_correct_workspace_passes(self):
        self.assertEqual(lc.main([]), 0)

    def test_a_crate_still_dual_licensed_fails(self):
        p = self.root / "b" / "Cargo.toml"
        p.write_text(p.read_text(encoding="utf-8").replace('"Apache-2.0"', f'"{OLD} OR Apache-2.0"'), encoding="utf-8")
        self.assertEqual(lc.main([]), 1)

    def test_an_old_licence_file_fails(self):
        (self.root / f"LICENSE-{OLD}").write_text("text\n", encoding="utf-8")
        self.assertEqual(lc.main([]), 1)

    def test_every_form_of_the_word_fails(self):
        for line in [
            # forms an earlier version of the scan let through, each appended to a tracked file
            f"// SPDX-License-Identifier: {low}",
            f"{OLD}-licensed",
            f"license: {OLD}",
            f'license-file = "LICENSE-{OLD}"',
            f"license = '{OLD}'",
            f'license = "{low}"',
            f'license = {{ text = "{OLD}" }}',
            f'"license": "{OLD}",',
            # forms the first version of the scan did catch
            f"// SPDX-License-Identifier: {OLD}",
            f"Released under the {OLD} License",
            f"dual licensed {OLD} OR Apache-2.0",
            f"Apache-2.0 OR {OLD}",
            f"{OLD}/Apache-2.0 terms",
            f"{OLD}_LICENSE",
            f"the {OLD.capitalize()} terms",
        ]:
            with self.subTest(line=line):
                self.write("b/README.md", line)
                self.assertEqual(lc.main([]), 1)

    def test_the_grant_sentence_fails_without_the_name(self):
        self.write("b/docs.txt", f"{GRANT}, to any person obtaining a copy")
        self.assertEqual(lc.main([]), 1)

    def test_a_longer_word_is_another_word(self):
        for line in [f"sub{low}", f"{low}igate", f"e{low}s", f"{low}2", f"2{low}"]:
            with self.subTest(line=line):
                self.write("b/README.md", line)
                self.assertEqual(lc.main([]), 0)

    def test_an_entry_allows_only_its_exact_line(self):
        line = f"0.3.x was {OLD} OR Apache-2.0"
        self.write("b/README.md", line)
        lc.ALLOWED = [("b/README.md", lc.line_hash(line), "an earlier version")]
        self.assertEqual(lc.main([]), 0)
        for other in [line + " (now)", " " + line, line.lower()]:
            with self.subTest(other=other):
                (self.root / "b" / "README.md").write_text(f"{line}\n{other}\n", encoding="utf-8")
                self.assertEqual(lc.main([]), 1)

    def test_an_entry_does_not_allow_the_line_in_another_file(self):
        line = f"0.3.x was {OLD} OR Apache-2.0"
        self.write("b/README.md", line)
        self.write("b/OTHER.md", line)
        lc.ALLOWED = [("b/README.md", lc.line_hash(line), "an earlier version")]
        self.assertEqual(lc.main([]), 1)

    def test_lines_appended_next_to_an_allowed_line_fail(self):
        # an entry names one line: what is appended to the same file needs its own entry
        line = f"see the {OLD} terms of 0.3.x"
        lc.ALLOWED = [("b/README.md", lc.line_hash(line), "an earlier version")]
        for appended in [f'license = "{OLD}"', f"stacker is {OLD} OR Apache-2.0", f"{low} = '{OLD}'"]:
            with self.subTest(appended=appended):
                (self.root / "b" / "README.md").write_text(f"{line}\n{appended}\n", encoding="utf-8")
                self.assertEqual(lc.main([]), 1)

    def test_an_entry_that_matches_no_line_fails(self):
        lc.ALLOWED = [("b/README.md", lc.line_hash("gone"), "nothing there")]
        self.assertEqual(lc.main([]), 1)

    def test_no_line_read_fails(self):
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
