#!/usr/bin/env python3
"""Tests for scripts/det_math_minmax_blob_sync.py."""

from __future__ import annotations

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import det_math_minmax_blob_sync as g  # noqa: E402


def write(root: str, text: str) -> None:
    path = os.path.join(root, g.TARGET_REL)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


MATCHING = "\n\n".join(g.PINNED[n] for n in g.FUNCS)


class Check(unittest.TestCase):
    def test_a_matching_file_passes(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, MATCHING)
            self.assertEqual(g.check(root), [])

    def test_a_missing_file_fails(self):
        with tempfile.TemporaryDirectory() as root:
            self.assertTrue(g.check(root))

    def test_a_changed_body_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, MATCHING.replace("b > a", "b >= a"))
            errors = g.check(root)
            self.assertTrue(any("max" in e for e in errors))

    def test_a_missing_function_fails(self):
        with tempfile.TemporaryDirectory() as root:
            without_min64 = "\n\n".join(g.PINNED[n] for n in g.FUNCS if n != "min64")
            write(root, without_min64)
            errors = g.check(root)
            self.assertTrue(any("min64" in e and "見つからない" in e for e in errors))

    def test_no_function_found_at_all_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, "// nothing here\n")
            errors = g.check(root)
            self.assertTrue(any("空振り" in e for e in errors))


class RealFile(unittest.TestCase):
    def test_the_real_file_matches_the_pin(self):
        self.assertEqual(g.check(g.DEFAULT_ROOT), [])


if __name__ == "__main__":
    unittest.main()
