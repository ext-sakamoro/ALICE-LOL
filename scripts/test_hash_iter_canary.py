#!/usr/bin/env python3
"""tests for scripts/hash_iter_canary.py: the diagnostic lines are read with either path separator (clippy prints
src\\lib.rs on Windows)

    python3 scripts/test_hash_iter_canary.py
"""
from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("hash_iter_canary", HERE / "hash_iter_canary.py")
h = importlib.util.module_from_spec(spec)
spec.loader.exec_module(h)


class Diag(unittest.TestCase):
    def test_both_separators(self):
        self.assertEqual(h.DIAG.findall("   --> src/lib.rs:202:5\n"), ["202"])
        self.assertEqual(h.DIAG.findall("   --> src\\lib.rs:210:9\n"), ["210"])

    def test_another_file_is_not_read(self):
        self.assertEqual(h.DIAG.findall("   --> src/other.rs:3:1\n"), [])


if __name__ == "__main__":
    unittest.main()
