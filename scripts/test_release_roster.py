#!/usr/bin/env python3
"""scripts/release_roster.py の試験: workspace の写しの manifest を 1 つずつ崩して出口を確かめる"""
from __future__ import annotations

import io
import os
import re
import shutil
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import release_roster as rr  # noqa: E402

NOT_PUBLISHED = ["alice-lol-humanoid", "alice-lol-ui", "alice-lol-robot", "alice-lol-datagen",
                 "alice-world-auditor", "alice-world-auditor-types"]


class Roster(unittest.TestCase):
    def setUp(self):
        # a copy of the workspace manifests only
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        shutil.copy(rr.ROOT / "Cargo.toml", self.root / "Cargo.toml")
        for m in ["alice-lol-macro", "alice-lol", *NOT_PUBLISHED]:
            (self.root / m).mkdir()
            shutil.copy(rr.ROOT / m / "Cargo.toml", self.root / m / "Cargo.toml")

    def tearDown(self):
        self.tmp.cleanup()

    def run_check(self) -> tuple[int, str]:
        err = io.StringIO()
        with redirect_stdout(io.StringIO()), redirect_stderr(err):
            code = rr.main(self.root)
        return code, err.getvalue()

    def set_publish(self, member: str, line: str | None) -> None:
        p = self.root / member / "Cargo.toml"
        s = re.sub(r"^publish = .*\n", "", p.read_text(encoding="utf-8"), flags=re.M)
        if line is not None:
            s = s.replace('version = "', f"{line}\nversion = \"", 1)
        p.write_text(s, encoding="utf-8")

    def test_the_repository_is_the_roster(self):
        self.assertEqual(self.run_check()[0], 0)

    def test_a_member_that_loses_publish_false_fails_and_is_named(self):
        for member in NOT_PUBLISHED:
            with self.subTest(member=member):
                self.setUp()
                self.set_publish(member, None)
                code, err = self.run_check()
                self.assertEqual(code, 1)
                self.assertIn(member, err)

    def test_a_roster_crate_that_gains_publish_false_fails_and_is_named(self):
        for member in sorted(rr.ROSTER):
            with self.subTest(member=member):
                self.setUp()
                self.set_publish(member, "publish = false")
                code, err = self.run_check()
                self.assertEqual(code, 1)
                self.assertIn(member, err)

    def test_an_empty_registry_list_is_not_publishable(self):
        self.set_publish("alice-lol-ui", "publish = []")
        self.assertEqual(self.run_check()[0], 0)
        self.set_publish("alice-lol-ui", 'publish = ["crates-io"]')
        self.assertEqual(self.run_check()[0], 1)

    def test_no_member_read_fails(self):
        (self.root / "Cargo.toml").write_text("[workspace]\nmembers = []\n", encoding="utf-8")
        self.assertEqual(self.run_check()[0], 2)


if __name__ == "__main__":
    unittest.main()
