#!/usr/bin/env python3
"""conformance/run_conformance.py: every spawned request must receive LOL_LAW_DIR.

run_conformance.spawn() sets the implementation's environment itself (TASK.md: the
directory of the law files is given in LOL_LAW_DIR, and the checks always set it).
A stand-in reader that checks its own environment and fails when the variable is
missing or wrong pins that this spawn() actually passes it, not only that it reads
an in-repo laws/spike/ directory that an unset variable would also happen to find.
"""
from __future__ import annotations

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import run_conformance as rc  # noqa: E402


class SpawnEnv(unittest.TestCase):
    def test_every_spawn_receives_the_law_directory(self):
        # the stand-in answers only when LOL_LAW_DIR names the directory run_conformance uses
        reader = [sys.executable, "-c",
                  "import json, os, sys\n"
                  "if os.environ.get('LOL_LAW_DIR') != sys.argv[1]: sys.exit(3)\n"
                  "print(json.dumps({'outputs': {'verdict': 'supports', 'subject': None}}))", rc.LAW_DIR]
        p = rc.spawn(reader, {"law": "a", "inputs": {}}, timeout=5)
        self.assertEqual(p.returncode, 0, p.stderr)


if __name__ == "__main__":
    unittest.main()
