#!/usr/bin/env python3
"""scripts/grid_teeth_check.sh --anchors-only: passes on this tree, names the anchor that went stale.

Each case runs a copy of the script over a copy of the files its mutants touch, so a stale anchor
is produced by editing the copy and the tree is not touched.
"""
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


if __name__ == "__main__":
    unittest.main()
