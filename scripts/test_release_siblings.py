#!/usr/bin/env python3
"""Tests for release_siblings.py. Usage: python3 scripts/test_release_siblings.py"""
from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import release_siblings as rs  # noqa: E402

LOCK = """version = 4

[[package]]
name = "alice-sdf"
version = "6.0.0"

[[package]]
name = "alice-sdf"
version = "5.1.0"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "alice-kinematics"
version = "0.2.0"

[[package]]
name = "a"
version = "0.1.0"
"""


def tree(d: Path, a_deps: str, b_deps: str = "", lock: str = LOCK) -> Path:
    root = d / "REPO"
    for name, deps in (("a", a_deps), ("b", b_deps)):
        (root / name).mkdir(parents=True)
        (root / name / "Cargo.toml").write_text(f'[package]\nname = "{name}"\nversion = "0.1.0"\n\n{deps}', encoding="utf-8")
    (root / "Cargo.toml").write_text('[workspace]\nmembers = ["a", "b"]\n', encoding="utf-8")
    (root / "Cargo.lock").write_text(lock, encoding="utf-8")
    return root


SDF = '[dependencies]\nalice-sdf = { path = "../../ALICE-SDF", version = "6" }\na = { path = "../a" }\n'
KIN = '[target.\'cfg(unix)\'.dev-dependencies]\nkin = { package = "alice-kinematics", path = "../../ALICE-Kinematics", version = "0.2" }\n'


class Siblings(unittest.TestCase):
    def test_reads_every_table_and_skips_members(self):
        with tempfile.TemporaryDirectory() as d:
            root = tree(Path(d), SDF, KIN)
            self.assertEqual(rs.sibling_deps(root), {"ALICE-SDF": "alice-sdf", "ALICE-Kinematics": "alice-kinematics"})

    def test_version_is_the_path_entry_of_the_lock(self):
        with tempfile.TemporaryDirectory() as d:
            root = tree(Path(d), SDF, KIN)
            rows = rs.plan(root, lambda _d: {"v6.0.0", "alice-kinematics-v0.2.0", "v0.2.0-rc"})
            self.assertEqual(rows, [("ALICE-Kinematics", "alice-kinematics", "0.2.0", "alice-kinematics-v0.2.0"),
                                    ("ALICE-SDF", "alice-sdf", "6.0.0", "v6.0.0")])

    def test_v_prefix_wins_over_package_prefix(self):
        self.assertEqual(rs.pick_tag("alice-sdf", "6.0.0", {"v6.0.0", "alice-sdf-v6.0.0"}), "v6.0.0")

    def test_missing_tag_fails(self):
        with tempfile.TemporaryDirectory() as d:
            root = tree(Path(d), SDF)
            with self.assertRaises(rs.SiblingError):
                rs.plan(root, lambda _d: {"v6.0.1", "v5.1.0"})

    def test_package_missing_from_lock_fails(self):
        with tempfile.TemporaryDirectory() as d:
            root = tree(Path(d), '[dependencies]\nalice-zip = { path = "../../ALICE-Zip" }\n')
            with self.assertRaises(rs.SiblingError):
                rs.plan(root)

    def test_registry_entry_alone_does_not_count(self):
        lock = LOCK.replace('name = "alice-sdf"\nversion = "6.0.0"\n', 'name = "x"\nversion = "6.0.0"\n')
        with tempfile.TemporaryDirectory() as d:
            root = tree(Path(d), SDF, lock=lock)
            with self.assertRaises(rs.SiblingError):
                rs.plan(root)

    def test_two_names_for_one_directory_fail(self):
        other = '[dependencies]\nsdf = { package = "alice-sdf-x", path = "../../ALICE-SDF" }\n'
        with tempfile.TemporaryDirectory() as d:
            root = tree(Path(d), SDF, other)
            with self.assertRaises(rs.SiblingError):
                rs.sibling_deps(root)

    def test_path_outside_the_parent_fails(self):
        with tempfile.TemporaryDirectory() as d:
            root = tree(Path(d), '[dependencies]\nx = { path = "../../../elsewhere/X" }\n')
            with self.assertRaises(rs.SiblingError):
                rs.sibling_deps(root)

    def test_no_sibling_exits_2(self):
        with tempfile.TemporaryDirectory() as d:
            root = tree(Path(d), '[dependencies]\na = { path = "../a" }\n')
            self.assertEqual(rs.main(["--root", str(root)]), 2)

    def test_the_real_tree_lists_its_siblings(self):
        rows = rs.plan(rs.ROOT)
        self.assertGreater(len(rows), 0)
        self.assertIn("alice-sdf", {r[1] for r in rows})


if __name__ == "__main__":
    unittest.main()
