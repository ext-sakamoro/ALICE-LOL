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

    def test_the_repository_passes(self):
        self.assertEqual(hf.main([]), 0)


if __name__ == "__main__":
    unittest.main()
