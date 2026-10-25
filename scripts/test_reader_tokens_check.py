#!/usr/bin/env python3
"""Tests of scripts/reader_tokens_check.py on small trees written here"""
from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import reader_tokens_check as C  # noqa: E402


def _tmpdir(**kwargs) -> str:
    """A temporary directory removed when this module's tests end (fixtures are read after the call returns)"""
    td = tempfile.TemporaryDirectory(**kwargs)
    unittest.addModuleCleanup(td.cleanup)
    return td.name

RS = "fn a(l: &str) -> Vec<&str> {\n    words(l).into_iter().map(|(w, _)| w).collect()\n}\n"
PY = ('def _words(s):\n    """splits like str.split() but on spaces and tabs (\\\\s is not used)"""\n'
      '    return [w for w in re.split(r"[ \\t]+", s) if w]  # not .split()\n')


class ReaderTokens(unittest.TestCase):
    def tree(self, rs=RS, py=PY, extra_rs=None):
        d = Path(_tmpdir())
        (d / "alice-lol/src/rule_law").mkdir(parents=True)
        (d / "scripts").mkdir()
        if rs is not None:
            (d / "alice-lol/src/rule_law.rs").write_text(rs, encoding="utf-8")
        if extra_rs is not None:
            (d / "alice-lol/src/rule_law/statute.rs").write_text(extra_rs, encoding="utf-8")
        if py is not None:
            (d / "scripts/law_rules.py").write_text(py, encoding="utf-8")
        return d

    def test_the_spec_tokenizer_passes_and_comments_and_docstrings_do_not_count(self):
        self.assertEqual(C.check(self.tree())[:2], (0, []))

    def test_each_rust_form_is_found(self):
        for form in ("l.split_whitespace()", "l.split_ascii_whitespace()", "l.trim()", "l.trim_start()",
                     "l.trim_end()", "c.is_whitespace()", "c.is_ascii_whitespace()"):
            code, problems, _ = C.check(self.tree(extra_rs=f"fn b(l: &str) {{ let _ = {form}; }}\n"))
            self.assertEqual(code, 1, form)
        self.assertEqual(C.check(self.tree(extra_rs="fn b(l: &str) { let _ = l.trim_matches([' ', '\\t']); }\n"))[0], 0)

    def test_each_python_form_is_found(self):
        for form in ("ln.split()", "ln.split(None, 1)", "ln.strip()", "ln.lstrip()", "ln.rstrip()", "ln.isspace()",
                     're.fullmatch(r"a\\s+b", ln)', 're.fullmatch(r"(\\S+)", ln)'):
            code, _, _ = C.check(self.tree(py=PY + f"x = {form}\n"))
            self.assertEqual(code, 1, form)
        for ok in ('ln.split("#", 1)', 'ln.strip(" \\t")', "_words(ln)"):
            self.assertEqual(C.check(self.tree(py=PY + f"x = {ok}\n"))[0], 0, ok)

    def test_nothing_read_fails(self):
        self.assertEqual(C.check(self.tree(rs=None, py=None))[0], 2)

    def test_this_repository_passes(self):
        self.assertEqual(C.check(C.ROOT)[0], 0)


if __name__ == "__main__":
    unittest.main()
