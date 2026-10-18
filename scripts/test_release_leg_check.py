#!/usr/bin/env python3
"""tests for scripts/release_leg_check.py (ALICE-Law G5): the repository's files pass, and each way of losing
the release leg (the macOS runner, --release, a selection, a source directory of the paths filter, the job)
is a problem; a workflow with no release step is compared nothing

    python3 scripts/test_release_leg_check.py
"""
from __future__ import annotations

import importlib.util
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
spec = importlib.util.spec_from_file_location("release_leg_check", HERE / "release_leg_check.py")
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)
META = json.loads(subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1", "--manifest-path",
                                  str(ROOT / "Cargo.toml")], capture_output=True, text=True, check=True).stdout)


class Leg(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="release-leg-"))
        self.addCleanup(shutil.rmtree, self.tmp)
        for rel in (r.WORKFLOW, r.PATTERNS, "scripts/law_tests.sh"):
            (self.tmp / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(ROOT / rel, self.tmp / rel)
        # the metadata of this checkout (its member directories are read relative to its own workspace_root)
        self.meta = META

    def edit(self, rel, old, new):
        p = self.tmp / rel
        s = p.read_text()
        self.assertEqual(s.count(old), 1, old)
        p.write_text(s.replace(old, new))

    def check(self):
        return r.problems(self.tmp, self.meta)

    def test_the_repository_passes(self):
        found, steps = self.check()
        self.assertEqual((found, steps), ([], 2))

    def test_the_macos_runner_is_required(self):
        self.edit(r.WORKFLOW, "    runs-on: macos-latest\n    timeout-minutes: 45", "    runs-on: ubuntu-latest\n    timeout-minutes: 45")
        self.assertTrue(any("macos-latest" in p for p in self.check()[0]))

    def test_release_is_required_on_each_selection(self):
        self.edit(r.WORKFLOW, "scripts/law_tests.sh physics --release", "scripts/law_tests.sh physics")
        found, _ = self.check()
        self.assertEqual(found, [f"{r.WORKFLOW}: no step runs `scripts/law_tests.sh physics --release`"])

    def test_both_selections_are_required(self):
        self.edit(r.WORKFLOW, "        run: scripts/law_tests.sh default --release\n", "        run: echo skipped\n")
        self.assertTrue(any("default --release" in p for p in self.check()[0]))

    def test_a_member_the_law_tests_build_must_be_in_the_paths_filter(self):
        self.edit(r.PATTERNS, "alice-lol-humanoid/**\n", "")
        self.assertIn(f"{r.PATTERNS}: no `alice-lol-humanoid/**` (a workspace member the Law tests build)", self.check()[0])

    def test_the_fixed_patterns_are_required(self):
        self.edit(r.PATTERNS, "Cargo.lock\n", "")
        self.assertIn(f"{r.PATTERNS}: no `Cargo.lock`", self.check()[0])

    def test_the_required_members_come_from_law_tests_and_their_path_dependencies(self):
        dirs = r.required_dirs(self.tmp, self.meta)
        self.assertIn("alice-lol", dirs)
        self.assertIn("alice-lol-robot", dirs)
        self.assertIn("alice-lol-humanoid", dirs, "a path dependency of alice-lol-robot")
        self.assertNotIn("alice-lol-ui", dirs)

    def test_no_release_job_is_compared_nothing(self):
        self.edit(r.WORKFLOW, "  release:\n", "  other:\n")
        found, steps = self.check()
        self.assertEqual(steps, 0)

    def test_a_law_tests_package_outside_the_workspace_is_an_error(self):
        self.edit("scripts/law_tests.sh", '  "alice-lol|--test law_tests|"\n', '  "alice-lol|--test law_tests|"\n  "no-such-crate|--lib|"\n')
        with self.assertRaises(ValueError):
            self.check()


if __name__ == "__main__":
    unittest.main()
