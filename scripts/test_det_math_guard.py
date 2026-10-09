#!/usr/bin/env python3
"""det_math_guard 自身の試験.

検査器が「何も見ていないのに通る」形を塞ぐ: 対象 file が無い / 呼び出しが 0 件 /
baseline に書けば何でも通る、のそれぞれを red で固定する
架空の内容だけで組み立てる (実物の tree は `det_math_guard.py` 側が検査する)

Usage: `python3 scripts/test_det_math_guard.py`
"""

from __future__ import annotations

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import det_math_guard as g  # noqa: E402

LAW = "alice-lol/src/law.rs"
AUDIT = "alice-lol/src/audit_law.rs"
RESEARCH = "alice-lol/src/research_law.rs"

CLEAN = """\
fn judge(x: f64) -> f64 {
    let a = x.abs();
    let b = a.sqrt();
    b.mul_add(2.0, 1.0)
}
"""


def run(files: dict[str, str], baseline: str | None = None) -> tuple[list[str], dict[str, int]]:
    with tempfile.TemporaryDirectory() as root:
        for rel, text in files.items():
            path = os.path.join(root, rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as f:
                f.write(text)
        if baseline is not None:
            path = os.path.join(root, g.BASELINE)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as f:
                f.write(baseline)
        return g.check(root)


def all_three(law: str) -> dict[str, str]:
    return {LAW: law, AUDIT: CLEAN, RESEARCH: CLEAN}


class Teeth(unittest.TestCase):
    def test_a_clean_tree_passes(self):
        e, c = run(all_three(CLEAN))
        self.assertEqual(e, [])
        self.assertEqual(c["files"], 3)
        self.assertGreater(c["calls"], 0)
        self.assertEqual(c["impl_defined"], 0)

    def test_a_new_implementation_defined_call_fails(self):
        e, _ = run(all_three("fn judge(t: f64) -> f64 {\n    t.sin()\n}\n"))
        self.assertTrue(any("judge::sin" in x for x in e), e)

    def test_the_same_call_passes_when_the_baseline_carries_it(self):
        e, c = run(all_three("fn judge(t: f64) -> f64 {\n    t.sin()\n}\n"),
                   baseline=f"# 理由を書く場所\n{LAW}::judge::sin\n")
        self.assertEqual(e, [])
        self.assertEqual(c["impl_defined"], 1)

    def test_a_baseline_entry_without_the_call_fails(self):
        e, _ = run(all_three(CLEAN), baseline=f"{LAW}::judge::sin\n")
        self.assertTrue(any("baseline にあるが実物が無い" in x for x in e), e)

    def test_no_readable_judgment_file_fails(self):
        e, c = run({})
        self.assertEqual(c["files"], 0)
        self.assertTrue(any("1 つも読めなかった" in x for x in e), e)

    def test_zero_calls_fails_even_with_files_present(self):
        e, c = run({LAW: "fn judge() {}\n", AUDIT: "fn a() {}\n", RESEARCH: "fn r() {}\n"})
        self.assertEqual(c["calls"], 0)
        self.assertTrue(any("0 件" in x for x in e), e)

    def test_exactly_rounded_operations_are_not_flagged(self):
        # IEEE 754 が値を規定する演算は platform に依らないので落とさない
        body = "".join(f"    let _ = x.{m}();\n" for m in ("sqrt", "abs", "floor", "ceil"))
        e, c = run(all_three(f"fn judge(x: f64) {{\n{body}    let _ = x.powi(2);\n}}\n"))
        self.assertEqual(e, [])
        self.assertEqual(c["impl_defined"], 0)

    def test_the_key_names_the_enclosing_function(self):
        law = "fn first(x: f64) -> f64 {\n    x.sqrt()\n}\n\nfn second(x: f64) -> f64 {\n    x.ln()\n}\n"
        e, _ = run(all_three(law))
        self.assertTrue(any("second::ln" in x for x in e), e)
        self.assertFalse(any("first::" in x for x in e), e)

    def test_a_call_inside_a_comment_is_not_flagged(self):
        law = "fn judge(x: f64) -> f64 {\n    // 以前は x.sin() を使っていた\n    x.sqrt()\n}\n"
        e, c = run(all_three(law))
        self.assertEqual(e, [])
        self.assertEqual(c["impl_defined"], 0)

    def test_every_implementation_defined_name_is_detected(self):
        # 一覧の取りこぼしを防ぐ: 全 name がそれぞれ単独で検出されること
        for m in g.IMPL_DEFINED:
            law = f"fn judge(x: f64) -> f64 {{\n    x.{m}()\n}}\n"
            e, _ = run(all_three(law))
            self.assertTrue(any(f"judge::{m}" in x for x in e), (m, e))


class RealRepo(unittest.TestCase):
    def test_this_repository_passes(self):
        root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        e, c = g.check(root)
        self.assertEqual(e, [])
        self.assertEqual(c["files"], len(g.JUDGMENT_FILES))
        self.assertGreater(c["calls"], 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
