#!/usr/bin/env python3
"""Tests of scripts/raw_minmax_manifest.py (python3 scripts/test_raw_minmax_manifest.py)"""
from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("rmm", HERE / "raw_minmax_manifest.py")
rmm = importlib.util.module_from_spec(spec)
sys.modules["rmm"] = rmm
spec.loader.exec_module(rmm)


def write(root: Path, rel: str, text: str) -> None:
    p = root / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text, encoding="utf-8")


def base_tree(root: Path) -> None:
    write(root, "Cargo.toml", '[workspace]\nmembers = ["alice-lol"]\n')
    write(root, rmm.DEFINITION_REL, "pub(crate) fn raw_max_f32(a: f32, b: f32) -> f32 { a.max(b) }\n")


def write_manifest(root: Path, entries: dict[str, int]) -> None:
    lines = []
    for rel, n in entries.items():
        lines.append(f'["{rel}"]')
        lines.append(f"count = {n}")
        lines.append("")
    write(root, rmm.MANIFEST_REL, "\n".join(lines))


class Check(unittest.TestCase):
    def test_a_matching_count_passes(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", "fn f(a: f32, b: f32) -> f32 { crate::raw_minmax::raw_max_f32(a, b) }\n")
            write_manifest(root, {"alice-lol/src/a.rs": 1})
            errors, total = rmm.check(str(root))
            self.assertEqual(errors, [])
            self.assertEqual(total, 1)

    def test_a_bare_function_reference_is_counted(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs",
                  "fn f(v: &[f32]) -> f32 { v.iter().copied().fold(0.0, crate::raw_minmax::raw_max_f32) }\n")
            write_manifest(root, {"alice-lol/src/a.rs": 1})
            errors, total = rmm.check(str(root))
            self.assertEqual(errors, [])
            self.assertEqual(total, 1)

    def test_an_unlisted_file_fails(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", "fn f(a: f32, b: f32) -> f32 { crate::raw_minmax::raw_max_f32(a, b) }\n")
            write_manifest(root, {"alice-lol/src/other.rs": 1})
            errors, _ = rmm.check(str(root))
            self.assertEqual(len(errors), 2)  # a.rs unlisted + other.rs stale
            self.assertTrue(any("manifest に entry が無い" in e for e in errors))

    def test_a_count_drift_fails(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs",
                  "fn f(a: f32, b: f32) -> f32 { crate::raw_minmax::raw_max_f32(crate::raw_minmax::raw_max_f32(a, b), 1.0) }\n")
            write_manifest(root, {"alice-lol/src/a.rs": 1})
            errors, _ = rmm.check(str(root))
            self.assertEqual(len(errors), 1)
            self.assertIn("drift", errors[0])

    def test_a_stale_entry_fails(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", "fn f() {}\n")
            write_manifest(root, {"alice-lol/src/a.rs": 1})
            errors, _ = rmm.check(str(root))
            self.assertEqual(len(errors), 1)
            self.assertIn("使われなくなった entry", errors[0])

    def test_an_aliased_import_is_still_counted(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs",
                  "use crate::raw_minmax::raw_max_f32 as rmax;\n"
                  "fn f(a: f32, b: f32) -> f32 { rmax(a, b) }\n")
            write_manifest(root, {"alice-lol/src/a.rs": 2})  # the `as` text itself + the call
            errors, total = rmm.check(str(root))
            self.assertEqual(errors, [])
            self.assertEqual(total, 2)

    def test_the_definition_file_itself_is_not_counted(self):
        # base_tree's raw_minmax.rs fixture contains `fn raw_max_f32(...)` (the
        # declaration itself) and `a.max(b)` -- neither must inflate any count
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", "fn f(a: f32, b: f32) -> f32 { crate::raw_minmax::raw_max_f32(a, b) }\n")
            write_manifest(root, {"alice-lol/src/a.rs": 1})
            errors, total = rmm.check(str(root))
            self.assertEqual(errors, [])
            self.assertEqual(total, 1)

    def test_zero_manifest_entries_raises(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", "fn f(a: f32, b: f32) -> f32 { crate::raw_minmax::raw_max_f32(a, b) }\n")
            write(root, rmm.MANIFEST_REL, "# empty\n")
            with self.assertRaises(rmm.RawMinmaxError):
                rmm.check(str(root))

    def test_no_src_dir_raises(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            write(root, "Cargo.toml", '[workspace]\nmembers = ["alice-lol"]\n')
            write_manifest(root, {"x": 1})
            with self.assertRaises(rmm.RawMinmaxError):
                rmm.check(str(root))

    def test_unreadable_manifest_raises(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", "fn f(a: f32, b: f32) -> f32 { crate::raw_minmax::raw_max_f32(a, b) }\n")
            with self.assertRaises(rmm.RawMinmaxError):
                rmm.check(str(root))  # manifest file does not exist at all


class WriteManifest(unittest.TestCase):
    def test_write_then_check_round_trips(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", "fn f(a: f32, b: f32) -> f32 { crate::raw_minmax::raw_max_f32(a, b) }\n")
            rmm.write_manifest(str(root))
            errors, total = rmm.check(str(root))
            self.assertEqual(errors, [])
            self.assertEqual(total, 1)


class RealFiles(unittest.TestCase):
    def test_the_real_repo_passes(self):
        root = HERE.parent
        errors, total = rmm.check(str(root))
        self.assertEqual(errors, [])
        self.assertGreater(total, 0)


if __name__ == "__main__":
    unittest.main()
