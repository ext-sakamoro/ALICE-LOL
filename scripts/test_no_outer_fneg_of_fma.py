#!/usr/bin/env python3
"""no_outer_fneg_of_fma 自身の試験.

検査器が「何も見ていないのに通る」形を塞ぐ: src file が無い / `.mul_add(` 呼び出しが
0 件 / allowlist に書けば何でも通る、のそれぞれを red で固定する
架空の内容だけで組み立てる (実物の tree は `no_outer_fneg_of_fma.py` 側が検査する)

Usage: `python3 scripts/test_no_outer_fneg_of_fma.py`
"""

from __future__ import annotations

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import no_outer_fneg_of_fma as g  # noqa: E402

SRC = "alice-lol/src/lib.rs"

CLEAN = """\
fn f(a: f32, b: f32, c: f32) -> f32 {
    a.mul_add(b, -c)
}
"""

OUTER_NEG = """\
fn f(a: f32, b: f32, c: f32) -> f32 {
    -(a.mul_add(b, c))
}
"""


def allow_toml(path: str, line: str = "-(a.mul_add(b, c))", count: int = 1) -> str:
    return f'[[allow]]\npath = "{path}"\nline = "{line}"\ncount = {count}\nreason = "test"\n'


def run(files: dict[str, str], allowlist: str | None = None) -> tuple[list[str], dict[str, int]]:
    with tempfile.TemporaryDirectory() as root:
        for rel, text in files.items():
            path = os.path.join(root, rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as f:
                f.write(text)
        if allowlist is not None:
            path = os.path.join(root, g.ALLOWLIST)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as f:
                f.write(allowlist)
        return g.check(root)


class Teeth(unittest.TestCase):
    def test_a_clean_tree_passes(self):
        e, c = run({SRC: CLEAN})
        self.assertEqual(e, [])
        self.assertEqual(c["files"], 1)
        self.assertGreater(c["calls"], 0)
        self.assertEqual(c["violations"], 0)

    def test_outer_negation_of_a_whole_mul_add_call_fails(self):
        e, c = run({SRC: OUTER_NEG})
        self.assertEqual(c["violations"], 1)
        self.assertTrue(any(f"{SRC}:2" in x for x in e), e)

    def test_negating_an_argument_before_the_call_is_not_flagged(self):
        law = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    (-a).mul_add(b, -c)\n}\n"
        e, c = run({SRC: law})
        self.assertEqual(e, [])
        self.assertEqual(c["violations"], 0)

    def test_negating_a_nested_argument_is_not_flagged(self):
        law = "fn f(a: f32, b: f32, c: f32, d: f32) -> f32 {\n    a.mul_add(b, -(c * d))\n}\n"
        e, c = run({SRC: law})
        self.assertEqual(e, [])
        self.assertEqual(c["violations"], 0)

    def test_binary_subtraction_before_a_paren_is_not_mistaken_for_unary(self):
        law = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    let x = a.mul_add(b, c);\n    x - (b)\n}\n"
        e, c = run({SRC: law})
        self.assertEqual(e, [])
        self.assertEqual(c["violations"], 0)

    def test_a_src_site_fails_even_when_the_allowlist_carries_it(self):
        # lib src/ には allowance が無い: Law form への書き換えのみが正解
        e, c = run({SRC: OUTER_NEG}, allowlist=allow_toml(SRC))
        self.assertEqual(c["violations"], 1)
        self.assertEqual(c["allowed"], 0)
        self.assertTrue(any("allowlist 不可" in x for x in e), e)

    def test_a_tests_dir_site_passes_when_the_allowlist_carries_it(self):
        oracle = "alice-lol/tests/oracle.rs"
        e, c = run({oracle: OUTER_NEG}, allowlist=allow_toml(oracle))
        self.assertEqual(e, [])
        self.assertEqual(c["violations"], 1)
        self.assertEqual(c["allowed"], 1)

    def test_an_examples_dir_site_passes_when_the_allowlist_carries_it(self):
        oracle = "alice-lol/examples/compare_forms.rs"
        e, c = run({oracle: OUTER_NEG}, allowlist=allow_toml(oracle))
        self.assertEqual(e, [])
        self.assertEqual(c["violations"], 1)
        self.assertEqual(c["allowed"], 1)

    def test_an_allowlist_entry_without_the_site_fails(self):
        e, _ = run({SRC: CLEAN}, allowlist=allow_toml(SRC))
        self.assertTrue(any("allowlist にあるが実物が無い" in x for x in e), e)

    def test_no_readable_src_file_fails(self):
        e, c = run({})
        self.assertEqual(c["files"], 0)
        self.assertTrue(any("1 つも読めなかった" in x for x in e), e)

    def test_zero_mul_add_calls_fails_even_with_files_present(self):
        e, c = run({SRC: "fn f() {}\n"})
        self.assertEqual(c["calls"], 0)
        self.assertTrue(any("0 件" in x for x in e), e)

    def test_a_call_inside_a_comment_or_string_is_not_flagged(self):
        law = (
            'fn f(a: f32, b: f32, c: f32) -> f32 {\n'
            '    // 以前は -(a.mul_add(b, c)) を使っていた\n'
            '    let _s = "-(a.mul_add(b, c))";\n'
            "    a.mul_add(b, -c)\n"
            "}\n"
        )
        e, c = run({SRC: law})
        self.assertEqual(e, [])
        self.assertEqual(c["violations"], 0)

    def test_a_string_literal_line_continuation_does_not_shift_later_line_numbers(self):
        # a `\` at the very end of a line inside a string literal is Rust's
        # line-continuation escape (the string keeps going on the next
        # physical line, dropping that line's leading whitespace) -- found
        # against a real file: the stripper's generic "\ + next char is a
        # 2-char escape" rule swallowed the newline along with it, losing
        # exactly one line from every line number reported for the rest of
        # the file. Two violations after the continuation must still each
        # report their own, distinct, correct line.
        law = (
            'fn f(a: f32, b: f32, c: f32) -> f32 {\n'
            '    let _s = "line one \\\n'
            '              line two";\n'
            "    -(a.mul_add(b, c))\n"
            "}\n"
            "fn g(a: f32, b: f32, c: f32) -> f32 {\n"
            "    -(a.mul_add(b, c))\n"
            "}\n"
        )
        e, c = run({SRC: law})
        self.assertEqual(c["violations"], 2)
        self.assertTrue(any(f"{SRC}:4:" in err for err in e), e)
        self.assertTrue(any(f"{SRC}:7:" in err for err in e), e)

    def test_extra_content_inside_the_outer_parens_is_not_flagged(self):
        # `-(a.mul_add(b, c) + d)` は mul_add の結果だけを negate していない
        law = "fn f(a: f32, b: f32, c: f32, d: f32) -> f32 {\n    -(a.mul_add(b, c) + d)\n}\n"
        e, c = run({SRC: law})
        self.assertEqual(e, [])
        self.assertEqual(c["violations"], 0)


    def test_src_dirs_can_be_overridden_for_other_repos(self):
        # Physics 等 他 repo は "alice-lol/src" でなく "src" を渡す想定
        with tempfile.TemporaryDirectory() as root:
            path = os.path.join(root, "src", "lib.rs")
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as f:
                f.write(CLEAN)
            e_default, c_default = g.check(root)  # 既定 "alice-lol/src" では見つからない
            self.assertEqual(c_default["files"], 0)
            e_override, c_override = g.check(root, src_dirs=("src",))
            self.assertEqual(e_override, [])
            self.assertEqual(c_override["files"], 1)


class RealRepo(unittest.TestCase):
    def test_this_repository_passes(self):
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        e, c = g.check(root)
        self.assertEqual(e, [])
        self.assertGreater(c["files"], 0)
        self.assertGreater(c["calls"], 0)


class WorkspaceScope(unittest.TestCase):
    def test_every_member_is_scanned(self):
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "Cargo.toml"), "w", encoding="utf-8") as f:
                f.write('[workspace]\nmembers = ["a", "b-crate"]\n')
            dirs = g.workspace_src_dirs(d)
            self.assertEqual(len(dirs), 8)
            self.assertIn("b-crate/src", dirs)
            os.makedirs(os.path.join(d, "b-crate", "src"))
            with open(os.path.join(d, "b-crate", "src", "lib.rs"), "w", encoding="utf-8") as f:
                f.write(OUTER_NEG)
            errors, counts = g.check(d, dirs)
            self.assertEqual(counts["violations"], 1, errors)

    def test_no_workspace_gives_no_dirs(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(g.workspace_src_dirs(d), ())

    def test_the_real_workspace_covers_more_than_alice_lol(self):
        dirs = g.workspace_src_dirs(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        self.assertIn("alice-lol-layout/src", dirs)


if __name__ == "__main__":
    unittest.main(verbosity=2)
