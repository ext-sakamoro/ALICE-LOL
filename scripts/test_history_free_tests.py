#!/usr/bin/env python3
"""Oracle for scripts/history_free_tests.py: each form of reading the history is caught
in a must-red file, the forms that do not read it pass, and an empty tree fails.

The history-reading text below is assembled from parts so that this file itself does
not match the patterns it tests.
"""
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import history_free_tests as hf  # noqa: E402


def _tmpdir(**kwargs) -> str:
    """A temporary directory removed when this module's tests end (fixtures are read after the call returns)"""
    td = tempfile.TemporaryDirectory(**kwargs)
    unittest.addModuleCleanup(td.cleanup)
    return td.name

G = "git"
REV = "8d92276" + "~1" + ":laws/a.law"

RED = {
    "py_list": f'subprocess.run(["{G}", "show", "{REV}"])\n',
    "py_list_c": f'subprocess.run(["{G}", "-C", root, "log", "-1"])\n',
    "shell": f'os.system("{G} log --oneline")\n',
    "rev_path": f'p = "{REV}"\n',
    "rust": f'Command::new("{G}").args(["rev-list", "HEAD"]).output()\n',
}
GREEN = {
    "ls_files": f'subprocess.run(["{G}", "-C", root, "ls-files", "-z"])\n',
    "prose": "the log of the run and the show of hands\n",
    "file": 'Path("scripts/testdata/a.law").read_text()\n',
}


def tree(d: Path, name: str, body: str, rs: bool = False) -> Path:
    (d / "scripts").mkdir(exist_ok=True)
    if rs:
        (d / "tests").mkdir(exist_ok=True)
        p = d / "tests" / f"{name}.rs"
    else:
        p = d / "scripts" / f"test_{name}.py"
    p.write_text(body, encoding="utf-8")
    return p


class Forms(unittest.TestCase):
    def test_each_history_read_is_caught(self):
        for name, body in RED.items():
            with tempfile.TemporaryDirectory() as d:
                tree(Path(d), name, body, rs=name == "rust")
                self.assertEqual(hf.main([d]), 1, name)

    def test_reads_of_the_tree_pass(self):
        for name, body in GREEN.items():
            with tempfile.TemporaryDirectory() as d:
                tree(Path(d), name, body)
                self.assertEqual(hf.main([d]), 0, name)

    def test_no_file_scanned_fails(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(hf.main([d]), 1)

    def test_an_exempt_line_passes_and_a_stale_exemption_fails(self):
        d = Path(_tmpdir())
        (d / "scripts").mkdir()
        rel = next(iter(hf.EXEMPT))[0]
        lines = "".join(f"{text}\n    capture_output=True)\n" for r, text in hf.EXEMPT if r == rel)
        (d / rel).write_text("import subprocess\nd = 'x'\n" + lines, encoding="utf-8")
        self.assertEqual(hf.main([str(d)]), 0)
        # the same call on another line is still a finding
        (d / rel).write_text("import subprocess\nsubprocess.run([\"git\", \"-C\", str(d), \"show\", \"HEAD:x\"])\n", encoding="utf-8")
        self.assertEqual(hf.main([str(d)]), 1)

    def test_the_repository_passes(self):
        self.assertEqual(hf.main([]), 0)


if __name__ == "__main__":
    unittest.main()
