#!/usr/bin/env python3
"""Tests for scripts/libm_hidden_check.py (the rewrite and the reading of clippy's
messages; the scratch-copy clippy run itself needs cargo and runs in CI and preflight
through `python3 scripts/libm_hidden_check.py`)."""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import libm_hidden_check as h  # noqa: E402


def msg(path: str, text: str, code: str = "clippy::disallowed_methods") -> dict:
    return {
        "code": {"code": code},
        "message": text,
        "spans": [{"is_primary": True, "file_name": path, "line_start": 7}],
    }


class Neutralise(unittest.TestCase):
    def test_every_mention_of_the_lint_is_replaced_and_counted(self):
        src = (
            '#[allow(clippy::disallowed_methods, reason = "x")]\nfn a() {}\n'
            "#![allow(clippy::disallowed_methods)]\n"
        )
        out, n = h.neutralise(src)
        self.assertEqual(n, 2)
        self.assertNotIn(h.LINT, out)
        self.assertIn(h.STAND_IN, out)


class Judge(unittest.TestCase):
    def test_a_libm_method_call_diagnostic_is_a_hit(self):
        hits, seen = h.judge([msg("src/a.rs", "use of a disallowed method `f32::sin`")])
        self.assertEqual((len(hits), seen), (1, 1))

    def test_each_libm_name_is_recognised(self):
        for name in ("sin", "cos", "tan", "atan2", "exp", "ln", "log", "powf", "sin_cos"):
            hits, _ = h.judge([msg("src/a.rs", f"use of a disallowed method `f32::{name}`")])
            self.assertEqual(len(hits), 1, name)

    def test_a_max_min_diagnostic_is_not_a_hit(self):
        hits, seen = h.judge([msg("src/a.rs", "use of a disallowed method `f32::max`")])
        self.assertEqual((hits, seen), ([], 1))

    def test_other_lints_are_not_counted_at_all(self):
        hits, seen = h.judge([msg("src/a.rs", "use of a disallowed method `f32::sin`", code="clippy::useless_vec")])
        self.assertEqual((hits, seen), ([], 0))

    def test_f64_libm_names_are_also_recognised(self):
        hits, _ = h.judge([msg("src/a.rs", "use of a disallowed method `f64::atan2`")])
        self.assertEqual(len(hits), 1)


class CopyNotInPlace(unittest.TestCase):
    """The rewrite happens in a copy; the checked-out files never change."""

    def _repo(self, d: str) -> str:
        root = os.path.join(d, "ALICE-X")
        os.makedirs(os.path.join(root, "m", "src"))
        os.makedirs(os.path.join(d, "ALICE-Sibling"))
        lib = os.path.join(root, "m", "src", "lib.rs")
        with open(lib, "w", encoding="utf-8") as f:
            f.write('#[allow(clippy::disallowed_methods, reason = "x")]\nfn a() {}\n')
        with open(os.path.join(root, "m", "keep.rs"), "w", encoding="utf-8") as f:
            f.write("#[allow(clippy::disallowed_methods)]\n")
        subprocess.run(["git", "init", "-q", root], check=True)
        subprocess.run(["git", "-C", root, "add", "-A"], check=True)
        return root

    def test_the_copy_is_rewritten_and_the_original_is_not(self):
        with tempfile.TemporaryDirectory() as d:
            root = self._repo(d)
            lib = os.path.join(root, "m", "src", "lib.rs")
            before = open(lib, encoding="utf-8").read()
            parent = os.path.join(d, "work")
            os.makedirs(parent)
            copy_root = h.make_copy(root, parent)
            n = h.neutralise_tree(copy_root)
            self.assertEqual(n, 1)  # only src/ files are rewritten
            self.assertEqual(open(lib, encoding="utf-8").read(), before)
            copied = open(os.path.join(copy_root, "m", "src", "lib.rs"), encoding="utf-8").read()
            self.assertNotIn(h.LINT, copied)
            kept = open(os.path.join(copy_root, "m", "keep.rs"), encoding="utf-8").read()
            self.assertIn(h.LINT, kept)

    def test_siblings_are_linked_next_to_the_copy(self):
        with tempfile.TemporaryDirectory() as d:
            root = self._repo(d)
            parent = os.path.join(d, "work")
            os.makedirs(parent)
            h.make_copy(root, parent)
            link = os.path.join(parent, "ALICE-Sibling")
            self.assertTrue(os.path.islink(link))
            self.assertEqual(os.path.realpath(link), os.path.realpath(os.path.join(d, "ALICE-Sibling")))

    def test_working_copy_edits_are_what_gets_copied(self):
        with tempfile.TemporaryDirectory() as d:
            root = self._repo(d)
            lib = os.path.join(root, "m", "src", "lib.rs")
            with open(lib, "a", encoding="utf-8") as f:
                f.write("// edited\n")
            parent = os.path.join(d, "work")
            os.makedirs(parent)
            copy_root = h.make_copy(root, parent)
            self.assertIn("// edited", open(os.path.join(copy_root, "m", "src", "lib.rs"), encoding="utf-8").read())


if __name__ == "__main__":
    unittest.main()
