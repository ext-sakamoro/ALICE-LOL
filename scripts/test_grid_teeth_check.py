#!/usr/bin/env python3
"""scripts/grid_teeth_check.sh: what it counts as a stale anchor, as red and as a miss.

Each case runs a copy of the script over a copy of the files its mutants touch, so a stale anchor
is produced by editing the copy and the tree is not touched. The full mode runs against a stand-in
`cargo` that is green on the committed source and prints the given output on a mutated one; the
output of a missing target is what cargo prints for `--test no_such_target`.
"""
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "grid_teeth_check.sh"
PARSER = "alice-lol/src/runtime_parser.rs"
TARGETS = sorted(set(re.findall(r"^p = '([^']+)'$", SCRIPT.read_text(encoding="utf-8"), re.M)))


def tree(tmp: Path) -> Path:
    (tmp / "scripts").mkdir()
    shutil.copy(SCRIPT, tmp / "scripts" / SCRIPT.name)
    for rel in TARGETS:
        (tmp / rel).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(ROOT / rel, tmp / rel)
    return tmp


FAKE_CARGO = """#!/usr/bin/env bash
[ "$1" = build ] && exit 0
if git diff --quiet && [ -z "${FAKE_RED_UNMUTATED:-}" ]; then
  exit 0
fi
printf '%s\\n' "$FAKE_OUTPUT"
exit "$FAKE_RC"
"""

FAILED = "test a_target ... FAILED\ntest result: FAILED. 0 passed; 1 failed"
NO_TARGET = (
    "error: no test target named `no_such_target` in `alice-lol` package\n"
    "help: available test targets:\n    finite_numbers"
)
NOT_COMPILED = "error[E0425]: cannot find value\nerror: could not compile `alice-lol` (lib)"


def full(tmp: Path, output: str, rc: int, **env: str) -> subprocess.CompletedProcess:
    """The full mode with the stand-in cargo, in a committed copy."""
    bin_dir = tmp / "fake-bin"
    bin_dir.mkdir()
    cargo = bin_dir / "cargo"
    cargo.write_text(FAKE_CARGO, encoding="utf-8")
    cargo.chmod(0o755)
    (tmp / ".gitignore").write_text("fake-bin\n", encoding="utf-8")
    for args in (
        ["init", "-q"],
        ["add", "-A"],
        ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "t"],
    ):
        subprocess.run(["git", *args], cwd=tmp, check=True, capture_output=True)
    return subprocess.run(
        ["bash", str(tmp / "scripts" / SCRIPT.name)],
        capture_output=True,
        text=True,
        check=False,
        env={
            **os.environ,
            "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}",
            "FAKE_OUTPUT": output,
            "FAKE_RC": str(rc),
            **env,
        },
    )


def anchors(tmp: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["bash", str(tmp / "scripts" / SCRIPT.name), "--anchors-only"],
        capture_output=True,
        text=True,
        check=False,
    )


class AnchorsOnly(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmpdir.cleanup)
        self.tmp = tree(Path(self.tmpdir.name))

    def test_the_current_source_has_every_anchor(self):
        self.assertEqual(len(TARGETS), 3, TARGETS)  # the regex above found the files the mutants touch
        done = anchors(self.tmp)
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
        self.assertEqual(done.stdout.count("  applies"), 5, done.stdout)
        self.assertIn("all 5 mutants apply", done.stdout)

    def test_a_moved_anchor_is_red_and_named_and_the_others_still_report(self):
        p = self.tmp / PARSER
        text = p.read_text(encoding="utf-8")
        moved = text.replace("number out of range: '{s}'", "number too large: '{s}'")
        self.assertNotEqual(moved, text)
        p.write_text(moved, encoding="utf-8")
        done = anchors(self.tmp)
        self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
        self.assertIn("M3 anchor not found", done.stderr)
        self.assertIn("STALE: M3", done.stdout)
        self.assertEqual(done.stdout.count("  applies"), 4, done.stdout)

    def test_a_missing_file_is_red(self):
        (self.tmp / PARSER).unlink()
        done = anchors(self.tmp)
        self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
        self.assertIn("STALE: M1", done.stdout)
        self.assertIn("STALE: M3", done.stdout)

    def test_a_failing_test_is_red(self):
        done = full(self.tmp, FAILED, 101)
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
        self.assertEqual(done.stdout.count("RED as expected"), 5, done.stdout)
        self.assertEqual(done.stdout.count("test a_target ... FAILED"), 5, done.stdout)
        # three different commands, each run once without a mutant
        self.assertEqual(done.stdout.count("  unmutated: "), 3, done.stdout)

    def test_a_missing_test_target_is_not_red(self):
        done = full(self.tmp, NO_TARGET, 101)
        self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
        self.assertNotIn("RED as expected", done.stdout)
        self.assertEqual(done.stdout.count("NO FAILURE LINE:"), 5, done.stdout)
        self.assertIn("no test target named", done.stdout)

    def test_a_summary_line_alone_is_not_red(self):
        done = full(self.tmp, "test result: FAILED. 0 passed; 0 failed", 101)
        self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
        self.assertNotIn("RED as expected", done.stdout)

    def test_a_stack_overflow_and_a_signal_are_red(self):
        for output in ("thread 'main' has overflowed its stack", "(signal: 6, SIGABRT: abort)"):
            with self.subTest(output), tempfile.TemporaryDirectory() as tmp:
                done = full(tree(Path(tmp)), output, 101)
                self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
                self.assertEqual(done.stdout.count("RED as expected"), 5, done.stdout)

    def test_a_mutant_that_does_not_compile_is_broken(self):
        done = full(self.tmp, NOT_COMPILED, 101)
        self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
        self.assertNotIn("RED as expected", done.stdout)
        self.assertEqual(done.stdout.count("BROKEN:"), 5, done.stdout)
        self.assertIn("could not compile", done.stdout)

    def test_a_green_mutant_is_a_miss(self):
        done = full(self.tmp, "test result: ok. 1 passed", 0)
        self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
        self.assertEqual(done.stdout.count("MISS:"), 5, done.stdout)

    def test_a_target_red_without_a_mutant_is_not_a_caught_mutant(self):
        done = full(self.tmp, FAILED, 101, FAKE_RED_UNMUTATED="1")
        self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
        self.assertNotIn("RED as expected", done.stdout)
        self.assertEqual(done.stdout.count("TARGET RED:"), 5, done.stdout)


if __name__ == "__main__":
    unittest.main()
