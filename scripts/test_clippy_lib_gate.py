#!/usr/bin/env python3
"""Tests for scripts/clippy_lib_gate.py: the generated library configuration and
the reading of the canary's clippy output (the canary run itself needs cargo and
runs in CI and preflight through `python3 scripts/clippy_lib_gate.py`)."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import clippy_lib_gate as g  # noqa: E402

ROOT = """\
# libm
disallowed-methods = [
    { path = "f32::sin", reason = "libm" },
    { path = "f64::cos", reason = "libm" },
]

disallowed-types = [
    { path = "std::collections::HashMap", reason = "seed" },
]
"""

CANARY = """\
//! doc
pub fn a(a: f32, b: f32) -> f32 {
    a.max(b)
}
pub fn b(a: f32, b: f32) -> f32 {
    f32::min(a, b)
}
pub fn c(a: f64, b: f64) -> f64 {
    a.min(b)
}
pub fn d(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}
pub fn e(x: f32) -> (f32, f32) {
    x.sin_cos()
}
pub fn h(y: f32, x: f32) -> f32 {
    y.atan2(x)
}
pub fn i(z: f32) -> f32 {
    z.tan()
}
pub fn j(p: Vec3, q: Vec3) -> Vec3 {
    p.min(q)
}
pub fn k(p: Vec3, q: Vec3) -> Vec3 {
    p.max(q)
}
pub fn l(p: Vec3) -> f32 {
    p.min_element()
}
pub fn m(p: Vec3) -> f32 {
    p.max_element()
}
pub fn n(p: Vec3, q: Vec3, r: Vec3) -> Vec3 {
    p.clamp(q, r)
}
pub fn o(p: Vec3, q: Vec3) -> Vec3 {
    Vec3::min(p, q)
}
pub fn q(u: Vec2, w: Vec2) -> Vec2 {
    u.max(w)
}
pub fn r(h: Vec4) -> f32 {
    h.min_element()
}
pub fn s(k: Vec3A) -> Vec3A {
    k.clamp(k, k)
}
"""


def diag(line: int, path: str) -> str:
    return f"src/lib.rs:{line}:5: error: use of a disallowed method `{path}`"


class Render(unittest.TestCase):
    def test_the_extra_entries_come_first_and_the_root_list_follows_unchanged(self):
        out = g.render(ROOT)
        self.assertTrue(out.startswith(g.HEADER))
        body = out[len(g.HEADER) + 1:]
        self.assertEqual(body.replace("".join(
            f'    {{ path = "{p}", reason = "{g.reason_of(p)}" }},\n' for p in g.EXTRA), ""), ROOT)
        i = body.index("disallowed-methods = [\n")
        self.assertIn('path = "f32::max"', body[i:body.index('path = "f32::sin"')])
        self.assertEqual(g.count_entries(out), 2 + len(g.EXTRA))

    def test_the_extra_paths_are_f32_f64_max_min_then_every_glam_vector_form(self):
        self.assertEqual(g.EXTRA[:4], ["f32::max", "f32::min", "f64::max", "f64::min"])
        glam = g.EXTRA[4:]
        self.assertEqual(len(glam), 20)
        for t in ["Vec2", "Vec3", "Vec3A", "Vec4"]:
            for m in ["min", "max", "min_element", "max_element", "clamp"]:
                self.assertIn(f"glam::{t}::{m}", glam)

    def test_glam_entries_carry_the_glam_reason_and_the_scalar_ones_the_scalar_reason(self):
        self.assertEqual(g.reason_of("glam::Vec3::min"), g.GLAM_REASON)
        self.assertEqual(g.reason_of("f32::max"), g.REASON)

    def test_a_root_list_that_already_forbids_max_is_refused(self):
        with self.assertRaises(ValueError):
            g.render(ROOT.replace('"f64::cos"', '"f32::max"'))

    def test_a_root_without_the_list_is_refused(self):
        with self.assertRaises(ValueError):
            g.render(ROOT.replace("disallowed-methods", "disallowed-macros"))


class Check(unittest.TestCase):
    def test_the_generated_file_passes(self):
        self.assertEqual(g.check(ROOT, g.render(ROOT)), [])

    def test_a_missing_file_fails(self):
        self.assertTrue(g.check(ROOT, None))

    def test_a_dropped_extra_entry_fails(self):
        lib = g.render(ROOT).replace(f'    {{ path = "f64::min", reason = "{g.REASON}" }},\n', "")
        self.assertTrue(g.check(ROOT, lib))

    def test_a_libm_entry_added_to_the_root_only_fails(self):
        lib = g.render(ROOT)
        root = ROOT.replace('    { path = "f64::cos", reason = "libm" },\n',
                            '    { path = "f64::cos", reason = "libm" },\n    { path = "f32::exp", reason = "libm" },\n')
        self.assertTrue(g.check(root, lib))

    def test_an_empty_root_list_fails(self):
        root = 'disallowed-methods = [\n]\n'
        self.assertTrue(g.check(root, g.render(root)))


class Canary(unittest.TestCase):
    def setUp(self):
        self.want = g.canary_lines(CANARY)

    def test_the_sixteen_forms_are_found_on_their_lines(self):
        self.assertEqual(sorted(self.want), list(range(3, 49, 3)))

    def test_a_diagnostic_on_every_line_and_a_non_zero_exit_passes(self):
        out = "\n".join(diag(n, "x") for n in self.want)
        errors, n = g.judge_canary(101, out, self.want)
        self.assertEqual((errors, n), ([], 16))

    def test_a_missing_line_fails(self):
        out = "\n".join(diag(n, "x") for n in self.want if n != 9)
        errors, _ = g.judge_canary(101, out, self.want)
        self.assertEqual(len(errors), 1)
        self.assertIn("line 9", errors[0])

    def test_exit_zero_fails_even_with_diagnostics(self):
        out = "\n".join(diag(n, "x") for n in self.want)
        errors, _ = g.judge_canary(0, out, self.want)
        self.assertTrue(errors)

    def test_no_diagnostic_at_all_fails(self):
        errors, n = g.judge_canary(101, "error: could not compile", self.want)
        self.assertEqual(n, 0)
        self.assertTrue(any("compared nothing" in e for e in errors))

    def test_a_diagnostic_from_another_file_does_not_count(self):
        out = "\n".join(f"src/other.rs:{n}:5: error: use of a disallowed method `x`" for n in self.want)
        errors, n = g.judge_canary(101, out, self.want)
        self.assertEqual(n, 0)
        self.assertTrue(errors)

    def test_a_form_written_twice_in_the_canary_is_refused(self):
        with self.assertRaises(ValueError):
            g.canary_lines(CANARY + "pub fn f(a: f32, b: f32) -> f32 { a.max(b) }\n")

    def test_the_real_canary_has_each_form_once(self):
        src = (Path(g.CANARY) / "src" / "lib.rs").read_text(encoding="utf-8")
        self.assertEqual(len(g.canary_lines(src)), len(g.CANARY_FORMS))


class RealFiles(unittest.TestCase):
    def test_the_committed_library_configuration_is_up_to_date(self):
        root = Path(g.ROOT_CONFIG).read_text(encoding="utf-8")
        lib = Path(g.LIB_CONFIG).read_text(encoding="utf-8")
        self.assertEqual(g.check(root, lib), [])


if __name__ == "__main__":
    unittest.main()
