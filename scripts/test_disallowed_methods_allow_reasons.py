#!/usr/bin/env python3
"""Tests for scripts/disallowed_methods_allow_reasons.py."""

from __future__ import annotations

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import disallowed_methods_allow_reasons as g  # noqa: E402


def write(root: str, rel: str, text: str) -> None:
    path = os.path.join(root, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


SRC = "alice-lol/src/lib.rs"


class Check(unittest.TestCase):
    def test_a_known_prefix_passes(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "nonzero-const: x")]\n'
                "fn f(a: f32) -> f32 { a.max(1.0) }\n"
            ))
            errors, counts = g.check(root)
            self.assertEqual(errors, [])
            self.assertEqual(counts["nonzero-const:"], 1)

    def test_each_prefix_is_counted_separately(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, "\n".join(
                f'#[allow(clippy::disallowed_methods, reason = "{p} x")]\nfn f{i}() {{}}\n'
                for i, p in enumerate(g.PREFIXES)
            ))
            errors, counts = g.check(root)
            self.assertEqual(errors, [])
            for p in g.PREFIXES:
                self.assertEqual(counts[p], 1, p)

    def test_an_unknown_prefix_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "trust me: x")]\n'
                "fn f(a: f32) -> f32 { a.max(1.0) }\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("既知の prefix" in e for e in errors))

    def test_a_reason_missing_entirely_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                "#[allow(clippy::disallowed_methods)]\n"
                "fn f(a: f32) -> f32 { a.max(1.0) }\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("reason が無い" in e for e in errors))

    def test_an_unrelated_allow_is_not_counted(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(dead_code, reason = "kept for symmetry")]\n'
                "fn f() {}\n"
                '#[allow(clippy::disallowed_methods, reason = "nonzero-const: x")]\n'
                "fn g(a: f32) -> f32 { a.max(1.0) }\n"
            ))
            errors, counts = g.check(root)
            self.assertEqual(errors, [])
            self.assertEqual(sum(counts.values()), 1)

    def test_zero_sites_at_all_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, "fn f() {}\n")
            errors, _ = g.check(root)
            self.assertTrue(any("空振り" in e for e in errors))

    def test_no_src_dir_fails(self):
        with tempfile.TemporaryDirectory() as root:
            errors, _ = g.check(root)
            self.assertTrue(any("読めなかった" in e for e in errors))

    def test_an_inner_attribute_block_form_is_found(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                "fn f(a: f32) -> f32 {\n"
                "    {\n"
                '        #![allow(clippy::disallowed_methods, reason = "both-abs: x")]\n'
                "        a.max(1.0)\n"
                "    }\n"
                "}\n"
            ))
            errors, counts = g.check(root)
            self.assertEqual(errors, [])
            self.assertEqual(counts["both-abs:"], 1)

    def test_a_multi_line_wrapped_attribute_is_found(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                "#[allow(\n"
                "    clippy::disallowed_methods,\n"
                '    reason = "int-cast: x"\n'
                ")]\n"
                "fn f(a: f32) -> usize { a.max(0.0) as usize }\n"
            ))
            errors, counts = g.check(root)
            self.assertEqual(errors, [])
            self.assertEqual(counts["int-cast:"], 1)


class LibmHiddenByAllow(unittest.TestCase):
    def test_a_libm_call_in_the_same_statement_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "nonzero-const: x")]\n'
                "fn f(a: f32, b: f32) -> f32 { a.max(1.0) + b.sin() }\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("libm 呼び出し" in e for e in errors))

    def test_a_path_form_libm_call_in_the_same_statement_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "both-abs: x")]\n'
                "fn f(a: f32, b: f32) -> f32 { a.max(f32::cos(b)) }\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("libm 呼び出し" in e for e in errors))

    def test_no_libm_call_passes(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "nonzero-const: x")]\n'
                "fn f(a: f32) -> f32 { a.max(1.0) }\n"
            ))
            errors, _ = g.check(root)
            self.assertEqual(errors, [])

    def test_a_libm_call_in_a_later_unrelated_statement_does_not_fail(self):
        # the allow is on the `let` statement itself, not the enclosing fn,
        # so a later statement in the same body is out of its scope
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                "fn f(a: f32, b: f32) -> f32 {\n"
                '    #[allow(clippy::disallowed_methods, reason = "nonzero-const: x")]\n'
                "    let r = a.max(1.0);\n"
                "    r + b.sin()\n"
                "}\n"
            ))
            errors, _ = g.check(root)
            self.assertEqual(errors, [])

    def test_test_probe_alternative_is_exempt_from_this_check(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "test-probe-alternative: x")]\n'
                "fn f(a: f32) -> f32 { a.sin() }\n"
            ))
            errors, _ = g.check(root)
            self.assertEqual(errors, [])


class RealRepo(unittest.TestCase):
    def test_the_real_repo_passes(self):
        errors, counts = g.check(g.DEFAULT_ROOT)
        self.assertEqual(errors, [])
        self.assertGreater(sum(counts.values()), 0)


if __name__ == "__main__":
    unittest.main()
