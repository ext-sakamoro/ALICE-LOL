#!/usr/bin/env python3
"""conformance/check_probes.py: the kind an implementation states decides which probes count.

A `static` implementation is written for the given law files, so the probes that hand it a law
file of their own (`law_text`) are skipped, counted and shown. A `reader` reads law files when
it runs, so it is checked on every probe, and one that ignores a law file it is handed fails.
The stand-in implementation here ignores law files: it answers every request the same way.
"""
from __future__ import annotations

import io
import json
import os
import sys
import tempfile
import unittest
import unittest.mock
from contextlib import redirect_stdout
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import check_probes as cp  # noqa: E402

# answers "supports" to every request and never reads a law file
STATIC = [sys.executable, "-c",
          'import json; print(json.dumps({"outputs": {"verdict": "supports", "subject": None}}))']
SUPPORTS = {"verdict": "supports", "subject": None}
LAW_TEXT = "law own\nkind audit\nbegin audit\naudit own\nevidence n\nend audit\n"


def probes(rows) -> str:
    d = tempfile.mkdtemp()
    path = Path(d, "probes.json")
    path.write_text(json.dumps(rows), encoding="utf-8")
    return str(path)


def run(kind: str, path: str) -> tuple[int, str]:
    out = io.StringIO()
    with redirect_stdout(out):
        code = cp.main(["--kind", kind, "--probes", path, "--", *STATIC])
    return code, out.getvalue()


class Kinds(unittest.TestCase):
    def setUp(self):
        # one probe any implementation passes, one that needs the law file to be read
        self.path = probes([
            {"note": "plain", "law": "a", "inputs": {}, "expect": SUPPORTS},
            {"note": "own law file: n is not measured", "law": "own", "law_text": LAW_TEXT,
             "inputs": {}, "expect": {"verdict": "no_evidence", "subject": "n"}},
        ])

    def test_static_skips_the_law_file_probes_and_says_so(self):
        code, out = run("static", self.path)
        self.assertEqual(code, 0)
        self.assertIn("compared 1 probes, skipped by kind 1", out)

    def test_a_reader_is_checked_on_the_law_file_probes(self):
        code, out = run("reader", self.path)
        self.assertEqual(code, 1)
        self.assertIn("compared 2 probes, skipped by kind 0", out)
        self.assertIn("DIFF own", out)

    def test_the_kind_is_required(self):
        with self.assertRaises(SystemExit), redirect_stdout(io.StringIO()), \
                open(os.devnull, "w") as null, unittest.mock.patch("sys.stderr", null):
            cp.main(["--probes", self.path, "--", *STATIC])

    def test_nothing_compared_fails_for_either_kind(self):
        only_law_text = probes([{"note": "own", "law": "own", "law_text": LAW_TEXT, "inputs": {},
                                 "expect": SUPPORTS}])
        code, out = run("static", only_law_text)
        self.assertEqual(code, 2)
        self.assertIn("skipped by kind 1", out)
        code, _ = run("reader", probes([]))
        self.assertEqual(code, 2)


if __name__ == "__main__":
    unittest.main()
