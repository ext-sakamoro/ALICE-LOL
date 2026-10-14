#!/usr/bin/env python3
"""Tests for root_artifacts.py. Usage: python3 scripts/test_root_artifacts.py"""
from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import root_artifacts as ra  # noqa: E402


def repo(files: dict[str, str]) -> Path:
    d = Path(tempfile.mkdtemp(prefix="root-artifacts-"))
    subprocess.run(["git", "init", "-q", str(d)], check=True)
    for rel, text in files.items():
        (d / rel).parent.mkdir(parents=True, exist_ok=True)
        (d / rel).write_text(text, encoding="utf-8")
    if files:
        subprocess.run(["git", "-C", str(d), "add", "-A"], check=True)
    return d


class RootArtifacts(unittest.TestCase):
    def test_a_clean_root_passes(self):
        self.assertEqual(ra.main(["--root", str(repo({"README.md": "x", "alice-lol/tests/fixtures/a.stl": "s"}))]), 0)

    def test_each_extension_at_the_root_fails(self):
        for name in ("crown.obj", "gem.FBX", "a.stl", "b.3mf"):
            with self.subTest(name=name):
                self.assertEqual(ra.main(["--root", str(repo({"README.md": "x", name: "m"}))]), 1)

    def test_a_mesh_below_the_root_is_not_its_business(self):
        self.assertEqual(ra.offenders(["README.md"]), [])
        self.assertEqual(ra.root_files(repo({"d/x.obj": "m", "README.md": "x"})), ["README.md"])

    def test_untracked_files_are_ignored(self):
        d = repo({"README.md": "x"})
        (d / "stray.obj").write_text("m", encoding="utf-8")
        self.assertEqual(ra.main(["--root", str(d)]), 0)

    def test_no_tracked_file_compares_nothing(self):
        self.assertEqual(ra.main(["--root", str(repo({}))]), 2)

    def test_the_real_tree_is_clean(self):
        self.assertEqual(ra.main([]), 0)


if __name__ == "__main__":
    unittest.main()
