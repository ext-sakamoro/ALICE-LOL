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
            unscoped = [p for p in g.PREFIXES if p not in ("test-probe-alternative:", "raw-helper:", "cli-report:", "unstamped-writer:")]
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
                '#[allow(clippy::disallowed_methods, reason = "cli-report: x")]\n'
                "fn timed() {}\n"
            ))
            write(root, g.UNSTAMPED_WRITER_FILE_REL, (
                '#[allow(clippy::disallowed_methods, reason = "unstamped-writer: x")]\n'
                "fn obj() {}\n"
            ))
            errors, counts = g.check(root)
            self.assertEqual(errors, [])
            for p in g.PREFIXES:
                self.assertEqual(counts[p], 1, p)

    def test_cli_report_only_in_a_binary(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "cli-report: the run time")]\n'
                "fn f() -> std::time::Instant { std::time::Instant::now() }\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("cli-report: は src/bin/" in e for e in errors), errors)

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

    def test_unstamped_writer_outside_the_designated_file_fails(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, SRC, (
                '#[allow(clippy::disallowed_methods, reason = "unstamped-writer: x")]\n'
                "fn w() {}\n"
            ))
            errors, _ = g.check(root)
            self.assertTrue(any("unstamped-writer" in e and "名乗れない" in e for e in errors), errors)

    def test_raw_helper_on_the_designated_file_passes(self):
        with tempfile.TemporaryDirectory() as root:
            write(root, g.RAW_HELPER_FILE_REL, (
                '#[allow(clippy::disallowed_methods, reason = "raw-helper: x")]\n'
                "fn raw(a: f32, b: f32) -> f32 { a.max(b) }\n"
            ))
            errors, counts = g.check(root)
            self.assertEqual(errors, [])
            self.assertEqual(counts["raw-helper:"], 1)


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



LOCK = (
    '[[writer]]\nkey = "src/io/stl.rs::export_stl"\npath = "alice_sdf::io::export_stl"\n'
    'canonical = "alice_sdf::io::stl::export_stl"\ncall = "c"\n\n'
    '[[writer]]\nkey = "src/io/obj.rs::export_obj"\npath = "alice_sdf::io::export_obj"\n'
    'canonical = "alice_sdf::io::obj::export_obj"\ncall = "c"\n'
)
WRAPPER = "pub fn w(m: &Mesh, p: &Path) {\n    let _ = alice_sdf::io::export_stl(m, p);\n}\n"


class LintLevel(unittest.TestCase):
    """only a reasoned #[allow] lowers clippy::disallowed_methods, and only unstamped-writer: covers an
    alice-sdf writer (ALICE-Law G8)"""

    def errors(self, rel, text, lock=True):
        with tempfile.TemporaryDirectory() as root:
            # one valid site, so a fixture is never "no allow found"
            write(root, "alice-lol/src/lib.rs", (
                '#[allow(clippy::disallowed_methods, reason = "nonzero-const: 2.0")]\n'
                "fn base(a: f32) -> f32 { a.max(2.0) }\n"))
            if lock:
                write(root, g.LOCK_REL, LOCK)
            write(root, rel, text)
            return g.check(root, require_lock=True)[0]

    def test_a_writer_under_another_prefix_is_refused(self):
        errs = self.errors("alice-lol/src/w.rs",
                           '#[allow(clippy::disallowed_methods, reason = "nonzero-const: x")]\n' + WRAPPER)
        self.assertTrue(any("export_stl" in e and "unstamped-writer:" in e for e in errs), errs)

    def test_a_writer_named_in_an_inner_allow_scope_is_refused(self):
        errs = self.errors("alice-lol/src/w.rs",
                           '#![allow(clippy::disallowed_methods, reason = "nonzero-const: x")]\n' + WRAPPER)
        self.assertTrue(any("export_stl" in e for e in errs), errs)

    def test_unstamped_writer_over_a_writer_in_its_file_passes(self):
        errs = self.errors(g.UNSTAMPED_WRITER_FILE_REL, (
            "fn f() {\n"
            '    #[allow(clippy::disallowed_methods, reason = "unstamped-writer: no stamped OBJ")]\n'
            "    export_obj(&mesh, &path, &cfg, None)?;\n}\n"))
        self.assertEqual(errs, [])

    def test_unstamped_writer_over_no_writer_is_refused(self):
        errs = self.errors(g.UNSTAMPED_WRITER_FILE_REL, (
            "fn f() {\n"
            '    #[allow(clippy::disallowed_methods, reason = "unstamped-writer: x")]\n'
            "    let y = a.max(b);\n}\n"))
        self.assertTrue(any("書き出しが無い" in e for e in errs), errs)

    def test_expect_is_refused(self):
        errs = self.errors("alice-lol/src/w.rs", "#[expect(clippy::disallowed_methods)]\n" + WRAPPER)
        self.assertTrue(any("expect / warn" in e for e in errs), errs)

    def test_expect_with_a_reason_is_refused_too(self):
        errs = self.errors("alice-lol/src/w.rs",
                           '#[expect(clippy::disallowed_methods, reason = "nonzero-const: x")]\n' + WRAPPER)
        self.assertTrue(any("expect / warn" in e for e in errs), errs)

    def test_warn_is_refused(self):
        errs = self.errors("alice-lol/src/w.rs", "#![warn(clippy::disallowed_methods)]\n" + WRAPPER)
        self.assertTrue(any("expect / warn" in e for e in errs), errs)

    def test_cfg_attr_is_refused(self):
        errs = self.errors("alice-lol/src/w.rs",
                           '#[cfg_attr(all(), allow(clippy::disallowed_methods, reason = "nonzero-const: x"))]\n' + WRAPPER)
        self.assertTrue(any("cfg_attr" in e and "条件付き" in e for e in errs), errs)

    def test_a_group_that_holds_the_lint_is_refused(self):
        for attr in ("#[allow(clippy::style)]", "#![allow(clippy::all)]", "#[expect(clippy::style, reason = \"x\")]"):
            with self.subTest(attr=attr):
                errs = self.errors("alice-lol/src/w.rs", attr + "\n" + WRAPPER)
                self.assertTrue(any("clippy::all / clippy::style" in e for e in errs), errs)

    def test_an_unrelated_lint_level_passes(self):
        errs = self.errors("alice-lol/src/w.rs", "#[allow(clippy::too_many_lines)]\nfn f() {}\n")
        self.assertEqual(errs, [])

    def test_a_reason_containing_a_bracket_does_not_end_the_attribute(self):
        errs = self.errors("alice-lol/src/w.rs",
                           '#[expect(clippy::too_many_lines, reason = "a ] b")]\n#[expect(clippy::disallowed_methods)]\n' + WRAPPER)
        self.assertTrue(any("expect / warn" in e for e in errs), errs)

    def test_an_escaped_quote_in_a_string_does_not_end_the_attribute(self):
        # `"a\"]"` in the condition: a scan that ended the string at the escaped quote would end the attribute
        # at that `]` and never see the expect behind it
        errs = self.errors("alice-lol/src/w.rs",
                           '#[cfg_attr(feature = "a\\"]", expect(clippy::disallowed_methods))]\n' + WRAPPER)
        self.assertTrue(any("cfg_attr" in e and "条件付き" in e for e in errs), errs)

    def test_the_lock_is_required_when_asked(self):
        errs = self.errors("alice-lol/src/w.rs", "fn f() {}\n", lock=False)
        self.assertTrue(any(g.LOCK_REL in e for e in errs), errs)


if __name__ == "__main__":
    unittest.main()
