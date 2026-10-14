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
    # a minimal [workspace] so g.check()'s member discovery has something to
    # find; writing it unconditionally on every write() call is harmless
    # (same content each time) and keeps every test self-contained
    with open(os.path.join(root, "Cargo.toml"), "w", encoding="utf-8") as f:
        f.write('[workspace]\nmembers = ["alice-lol"]\n')


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
        # test-probe-alternative: and raw-helper: are scope-restricted (see
        # the Scoping class below), so this writes each prefix to a location
        # where it is actually valid, not all into one plain function
        with tempfile.TemporaryDirectory() as root:
            unscoped = [p for p in g.PREFIXES if p not in ("test-probe-alternative:", "raw-helper:", "timing-display:")]
            write(root, SRC, "\n".join(
                f'#[allow(clippy::disallowed_methods, reason = "{p} x")]\nfn f{i}() {{}}\n'
                for i, p in enumerate(unscoped)
            ))
            write(root, "alice-lol/src/other.rs", (
                "#[cfg(test)]\n"
                "mod tests {\n"
                '    #[allow(clippy::disallowed_methods, reason = "test-probe-alternative: x")]\n'
                "    fn probe() {}\n"
                "}\n"
            ))
            write(root, g.RAW_HELPER_FILE_REL, (
                '#[allow(clippy::disallowed_methods, reason = "raw-helper: x")]\n'
                "fn raw() {}\n"
            ))
            write(root, "alice-lol/src/bin/tool.rs", (
                '#[allow(clippy::disallowed_methods, reason = "timing-display: x")]\n'
                "fn timed() {}\n"
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


class Scoping(unittest.TestCase):
    """`test-probe-alternative:` and `raw-helper:` are not accepted on the
    reason text alone; the allow's own location must actually be the scope
    each prefix claims."""

    def test_test_probe_alternative_outside_cfg_test_and_outside_tests_dir_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "test-probe-alternative: x")]\n'
                "fn f(a: f32) -> f32 { a.sin() }\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("test-probe-alternative" in e and "名乗れない" in e for e in errors))

    def test_test_probe_alternative_in_a_sibling_plain_mod_fails(self):
        # a `mod` that is NOT itself `#[cfg(test)]`-gated does not count,
        # even if a `#[cfg(test)] mod` exists elsewhere in the same file
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                "#[cfg(test)]\n"
                "mod tests {\n"
                "    fn unrelated() {}\n"
                "}\n"
                "mod production {\n"
                '    #[allow(clippy::disallowed_methods, reason = "test-probe-alternative: x")]\n'
                "    fn f(a: f32) -> f32 { a.sin() }\n"
                "}\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("test-probe-alternative" in e and "名乗れない" in e for e in errors))

    def test_raw_helper_outside_the_designated_file_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "raw-helper: x")]\n'
                "fn raw(a: f32, b: f32) -> f32 { a.max(b) }\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("raw-helper" in e and "名乗れない" in e for e in errors))

    def test_raw_helper_on_the_designated_file_passes(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, g.RAW_HELPER_FILE_REL, (
                '#[allow(clippy::disallowed_methods, reason = "raw-helper: x")]\n'
                "fn raw(a: f32, b: f32) -> f32 { a.max(b) }\n"
            ))
            errors, counts = g.check(root)
            self.assertEqual(errors, [])
            self.assertEqual(counts["raw-helper:"], 1)


class TimingDisplay(unittest.TestCase):
    """`timing-display:` (a wall-clock read only printed for the operator) is
    accepted under src/bin/ and refused in library code (ALICE-Law G10)."""

    def test_in_a_bin_passes(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, "pub fn f() {}\n")
            write(root, "alice-lol/src/bin/tool.rs", (
                '#[allow(clippy::disallowed_methods, reason = "timing-display: x")]\n'
                "fn main() { let _t = std::time::Instant::now(); }\n"
            ))
            errors, counts = g.check(root)
            self.assertEqual(errors, [])
            self.assertEqual(counts["timing-display:"], 1)

    def test_in_library_code_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "timing-display: x")]\n'
                "pub fn f() -> std::time::Instant { std::time::Instant::now() }\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("timing-display" in e and "名乗れない" in e for e in errors))


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
                "#[cfg(test)]\n"
                "mod tests {\n"
                '    #[allow(clippy::disallowed_methods, reason = "test-probe-alternative: x")]\n'
                "    fn f(a: f32) -> f32 { a.sin() }\n"
                "}\n"
            ))
            errors, _ = g.check(root)
            self.assertEqual(errors, [])

    def test_a_bare_function_reference_libm_form_fails(self):
        # `.fold(acc, f32::sin)` passes the function itself, never calling
        # it with `(`, which a `(?:f32|f64)::name\(` pattern would miss
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "nonzero-const: x")]\n'
                "fn f(a: f32, v: &[f32]) -> f32 {\n"
                "    a.max(1.0) + v.iter().copied().fold(0.0, f32::sin)\n"
                "}\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("libm 呼び出し" in e for e in errors))

    def test_a_fully_qualified_libm_call_form_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "both-abs: x")]\n'
                "fn f(a: f32, b: f32) -> f32 { a.max(<f32>::cos(b)) }\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("libm 呼び出し" in e for e in errors))

    def test_an_inner_allow_hiding_a_libm_call_later_in_the_file_fails(self):
        # an inner `#![allow(...)]`'s scope is the whole enclosing module,
        # not just "up to the next `;`/`}`" — a libm call several
        # statements later, still in the same module, must still be caught
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#![allow(clippy::disallowed_methods, reason = "nonzero-const: x")]\n'
                "fn f(a: f32) -> f32 {\n"
                "    a.max(1.0)\n"
                "}\n"
                "fn g(b: f32) -> f32 {\n"
                "    b.sin()\n"
                "}\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("libm 呼び出し" in e and "enclosing file" in e for e in errors))

    def test_an_outer_allow_does_not_see_past_its_own_statement_into_a_later_one(self):
        # confirms the broadened scan did not accidentally turn the OUTER
        # (per-statement) form into a whole-file scan too
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "nonzero-const: x")]\n'
                "fn f(a: f32) -> f32 { a.max(1.0) }\n"
                "fn g(b: f32) -> f32 { b.sin() }\n"
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
