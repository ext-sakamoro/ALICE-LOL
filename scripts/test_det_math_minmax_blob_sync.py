#!/usr/bin/env python3
"""Tests for scripts/det_math_minmax_blob_sync.py."""

from __future__ import annotations

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import det_math_minmax_blob_sync as g  # noqa: E402


def write_target(root: str, target_rel: str, names: list[str]) -> None:
    """Write a target file carrying exactly the pinned bodies of `names`."""
    path = os.path.join(root, target_rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n\n".join(g.PINNED[n] for n in names))


def write_all_matching(root: str) -> None:
    """Every real target, each carrying exactly its own expected functions."""
    for target_rel, expected in g.TARGETS.items():
        write_target(root, target_rel, expected)


ONE_TARGET = next(iter(g.TARGETS))
ONE_EXPECTED = g.TARGETS[ONE_TARGET]


class Check(unittest.TestCase):
    def test_all_matching_targets_pass(self):
        with tempfile.TemporaryDirectory() as root:
            write_all_matching(root)
            errors, compared = g.check(root)
            self.assertEqual(errors, [])
            self.assertEqual(compared, sum(len(v) for v in g.TARGETS.values()))

    def test_a_missing_target_file_fails(self):
        with tempfile.TemporaryDirectory() as root:
            self.assertTrue(g.check(root)[0])

    def test_a_changed_body_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write_all_matching(root)
            path = os.path.join(root, ONE_TARGET)
            with open(path, encoding="utf-8") as f:
                text = f.read()
            with open(path, "w", encoding="utf-8") as f:
                f.write(text.replace("b > a", "b >= a"))
            errors, _ = g.check(root)
            self.assertTrue(any(ONE_TARGET in e and "一致しない" in e for e in errors))

    def test_a_target_may_carry_fewer_than_all_four_functions(self):
        # alice-lol-ui/alice-lol-datagen only need max/min; this must pass,
        # not be treated as a missing max64/min64
        with tempfile.TemporaryDirectory() as root:
            write_all_matching(root)
            errors, compared = g.check(root)
            self.assertEqual(errors, [])
            ui_rel = os.path.join("alice-lol-ui", "src", "det_math.rs")
            self.assertEqual(g.TARGETS[ui_rel], ["max", "min"])
            self.assertGreater(compared, 0)

    def test_a_missing_expected_function_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write_all_matching(root)
            # alice-lol's own target expects max64; drop it
            lol_rel = os.path.join("alice-lol", "src", "det_math.rs")
            write_target(root, lol_rel, ["max", "min", "min64"])
            errors, _ = g.check(root)
            self.assertTrue(any(lol_rel in e and "max64" in e and "見つからない" in e for e in errors))

    def test_no_expected_function_found_at_all_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write_all_matching(root)
            path = os.path.join(root, ONE_TARGET)
            with open(path, "w", encoding="utf-8") as f:
                f.write("// nothing here\n")
            errors, _ = g.check(root)
            self.assertTrue(any(ONE_TARGET in e and "空振り" in e for e in errors))


class RealFiles(unittest.TestCase):
    def test_every_real_target_matches_the_pin(self):
        errors, compared = g.check(g.DEFAULT_ROOT)
        self.assertEqual(errors, [])
        self.assertEqual(compared, sum(len(v) for v in g.TARGETS.values()))


if __name__ == "__main__":
    unittest.main()
