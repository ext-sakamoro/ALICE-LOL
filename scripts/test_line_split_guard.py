#!/usr/bin/env python3
"""Oracle for scripts/line_split_guard.py: each forbidden call in each file fails, the
repository passes, a missing file fails."""
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import line_split_guard as g  # noqa: E402

CLEAN = {f: "x = 1\n" for f in g.FILES}


def tree(d: Path, files: dict) -> None:
    for rel, text in files.items():
        p = d / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")


class Guard(unittest.TestCase):
    def test_each_forbidden_call_fails(self):
        for rel, bad in [("conformance/ref_impl.py", "t.splitlines()\n"),
                         ("scripts/law_schema.py", "for l in text.splitlines():\n"),
                         ("alice-lol/src/law_input.rs", "for l in text.lines() {}\n")]:
            with tempfile.TemporaryDirectory() as d:
                tree(Path(d), {**CLEAN, rel: bad})
                self.assertEqual(g.main([d]), 1, rel)

    def test_clean_files_pass(self):
        with tempfile.TemporaryDirectory() as d:
            tree(Path(d), CLEAN)
            self.assertEqual(g.main([d]), 0)

    def test_a_missing_file_fails(self):
        with tempfile.TemporaryDirectory() as d:
            tree(Path(d), {k: v for k, v in CLEAN.items() if "law_input" not in k})
            self.assertEqual(g.main([d]), 1)

    def test_the_repository_passes(self):
        self.assertEqual(g.main([]), 0)


if __name__ == "__main__":
    unittest.main()
