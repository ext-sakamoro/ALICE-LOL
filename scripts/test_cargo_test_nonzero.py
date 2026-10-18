#!/usr/bin/env python3
"""Tests for scripts/cargo_test_nonzero.sh: a `cargo` on PATH that replays a cargo-shaped log stands in for
cargo, and the exit code and message are checked for a run in which tests passed, a run in which none ran
(a filter that matches nothing), a run whose targets each ran none, a failing cargo, and Windows output
(CRLF). The arguments reach cargo unchanged."""
import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

from test_run_tests import BASH

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "cargo_test_nonzero.sh"


def result(n):
    return f"test result: ok. {n} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s"


class CargoTestNonzero(unittest.TestCase):
    def setUp(self):
        # not a skip: a run without bash would otherwise report success
        self.assertIsNotNone(BASH, "bash not found: cargo_test_nonzero.sh cannot be exercised")

    def run_with(self, lines, *, status=0, newline="\n", args=("-p", "x", "--lib", "some_filter")):
        with tempfile.TemporaryDirectory() as d:
            log = Path(d) / "cargo.log"
            log.write_bytes(newline.join(lines + [""]).encode("utf-8"))
            argv = Path(d) / "argv"
            fake = Path(d) / "cargo"
            fake.write_text(f'#!/usr/bin/env bash\nprintf "%s\\n" "$@" > "{argv.as_posix()}"\n'
                            f'cat "{log.as_posix()}"\nexit {status}\n')
            fake.chmod(fake.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
            # the fake comes first on PATH (Windows: bash converts the ;-separated PATH it inherits)
            env = dict(os.environ, PATH=str(Path(d)) + os.pathsep + os.environ.get("PATH", ""))
            p = subprocess.run([BASH, SCRIPT.as_posix(), *args], capture_output=True, text=True, encoding="utf-8",
                               errors="replace", env=env)
            passed = argv.read_text().split("\n")[:-1] if argv.exists() else None
        return p.returncode, p.stdout + p.stderr, passed

    def test_tests_that_passed_pass(self):
        rc, out, argv = self.run_with(["     Running unittests src/lib.rs", result(3)])
        self.assertEqual(rc, 0, out)
        self.assertIn("tests passed in total: 3", out)
        self.assertEqual(argv, ["test", "-p", "x", "--lib", "some_filter"])

    def test_a_filter_that_matches_nothing_fails(self):
        rc, out, _ = self.run_with(["     Running unittests src/lib.rs", result(0)])
        self.assertEqual(rc, 1, out)
        self.assertIn("executed zero tests", out)

    def test_every_target_running_none_fails(self):
        rc, out, _ = self.run_with([result(0), result(0), result(0)])
        self.assertEqual(rc, 1, out)

    def test_counts_are_summed_over_targets(self):
        rc, out, _ = self.run_with([result(0), result(2), result(0)])
        self.assertEqual(rc, 0, out)
        self.assertIn("tests passed in total: 2", out)

    def test_an_empty_log_fails(self):
        rc, out, _ = self.run_with([])
        self.assertEqual(rc, 1, out)

    def test_a_failing_cargo_fails(self):
        rc, out, _ = self.run_with([result(5)], status=101)
        self.assertNotEqual(rc, 0, out)

    def test_windows_output_is_counted(self):
        rc, out, _ = self.run_with(["     Running unittests src\\lib.rs", result(4)], newline="\r\n")
        self.assertEqual(rc, 0, out)
        self.assertIn("tests passed in total: 4", out)


if __name__ == "__main__":
    unittest.main()
