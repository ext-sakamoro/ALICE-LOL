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

ALLOW = (
    "#![allow(clippy::disallowed_macros, reason = \"test\")]\n\n"
)


def write(root: Path, rel: str, text: str) -> None:
    p = root / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text, encoding="utf-8")


def base_tree(root: Path) -> None:
    write(root, "Cargo.toml", '[workspace]\nmembers = ["alice-lol"]\n')
    write(
        root,
        rmm.DEFINITION_REL,
        "macro_rules! raw_minmax_here {\n"
        "    ($($which:ident),+) => {\n"
        "        $( $crate::raw_minmax::raw_minmax_here!(@one $which); )+\n"
        "    };\n"
        "    (@one max_f32) => { const fn raw_max_f32(a: f32, b: f32) -> f32 { a.max(b) } };\n"
        "}\n",
    )


def write_manifest(root: Path, entries: dict[str, list[str]]) -> None:
    lines = []
    for rel, which in entries.items():
        lines.append(f'["{rel}"]')
        generates = ", ".join(f'"{w}"' for w in which)
        lines.append(f"generates = [{generates}]")
        lines.append("")
    write(root, rmm.MANIFEST_REL, "\n".join(lines))


class Check(unittest.TestCase):
    def test_a_matching_module_passes(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", ALLOW + "crate::raw_minmax::raw_minmax_here!(max_f32);\n")
            write_manifest(root, {"alice-lol/src/a.rs": ["max_f32"]})
            errors, total = rmm.check(str(root))
            self.assertEqual(errors, [])
            self.assertEqual(total, 1)

    def test_multiple_which_args_are_all_tracked(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", ALLOW + "crate::raw_minmax::raw_minmax_here!(max_f32, min_f32);\n")
            write_manifest(root, {"alice-lol/src/a.rs": ["max_f32", "min_f32"]})
            errors, total = rmm.check(str(root))
            self.assertEqual(errors, [])
            self.assertEqual(total, 2)

    def test_an_unlisted_module_fails(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", ALLOW + "crate::raw_minmax::raw_minmax_here!(max_f32);\n")
            write_manifest(root, {"alice-lol/src/other.rs": ["max_f32"]})
            errors, _ = rmm.check(str(root))
            self.assertEqual(len(errors), 2)  # a.rs unlisted + other.rs stale
            self.assertTrue(any("manifest に entry が無い" in e for e in errors))

    def test_a_generates_drift_fails(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", ALLOW + "crate::raw_minmax::raw_minmax_here!(max_f32, min_f32);\n")
            write_manifest(root, {"alice-lol/src/a.rs": ["max_f32"]})
            errors, _ = rmm.check(str(root))
            self.assertEqual(len(errors), 1)
            self.assertIn("drift", errors[0])

    def test_a_stale_entry_fails(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", "fn f() {}\n")
            write_manifest(root, {"alice-lol/src/a.rs": ["max_f32"]})
            errors, _ = rmm.check(str(root))
            self.assertEqual(len(errors), 1)
            self.assertIn("使われなくなった entry", errors[0])

    def test_an_unknown_which_argument_fails(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", ALLOW + "crate::raw_minmax::raw_minmax_here!(min_f64);\n")
            write_manifest(root, {"alice-lol/src/a.rs": ["min_f64"]})
            errors, _ = rmm.check(str(root))
            self.assertTrue(any("未知の引数" in e for e in errors))

    def test_an_invocation_without_its_own_allow_fails(self):
        # clippy itself already fails this build (no #![allow(clippy::disallowed_macros,
        # ...)] in the same file means the macro invocation is a clippy error) -- this
        # script catches it too, as a cheaper non-cargo check
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", "crate::raw_minmax::raw_minmax_here!(max_f32);\n")
            write_manifest(root, {"alice-lol/src/a.rs": ["max_f32"]})
            errors, _ = rmm.check(str(root))
            self.assertTrue(any("disallowed_macros" in e and "allow" in e for e in errors))

    def test_the_definition_file_itself_is_not_counted(self):
        # base_tree's raw_minmax.rs fixture contains the macro's own `@one` recursive
        # self-invocation; it must not be counted as a caller
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", ALLOW + "crate::raw_minmax::raw_minmax_here!(max_f32);\n")
            write_manifest(root, {"alice-lol/src/a.rs": ["max_f32"]})
            errors, total = rmm.check(str(root))
            self.assertEqual(errors, [])
            self.assertEqual(total, 1)

    def test_zero_manifest_entries_raises(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", ALLOW + "crate::raw_minmax::raw_minmax_here!(max_f32);\n")
            write(root, rmm.MANIFEST_REL, "# empty\n")
            with self.assertRaises(rmm.RawMinmaxError):
                rmm.check(str(root))

    def test_no_src_dir_raises(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            write(root, "Cargo.toml", '[workspace]\nmembers = ["alice-lol"]\n')
            write_manifest(root, {"x": ["max_f32"]})
            with self.assertRaises(rmm.RawMinmaxError):
                rmm.check(str(root))

    def test_unreadable_manifest_raises(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", ALLOW + "crate::raw_minmax::raw_minmax_here!(max_f32);\n")
            with self.assertRaises(rmm.RawMinmaxError):
                rmm.check(str(root))  # manifest file does not exist at all


class WriteManifest(unittest.TestCase):
    def test_write_then_check_round_trips(self):
        with tempfile.TemporaryDirectory() as t:
            root = Path(t)
            base_tree(root)
            write(root, "alice-lol/src/a.rs", ALLOW + "crate::raw_minmax::raw_minmax_here!(max_f32);\n")
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
