#!/usr/bin/env python3
"""Tests for float_build_flags.py. Usage: python3 scripts/test_float_build_flags.py"""
from __future__ import annotations

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import float_build_flags as fb  # noqa: E402


def repo(files: dict[str, str]) -> Path:
    d = Path(tempfile.mkdtemp(prefix="float-build-flags-"))
    subprocess.run(["git", "init", "-q", str(d)], check=True)
    for rel, text in files.items():
        (d / rel).parent.mkdir(parents=True, exist_ok=True)
        (d / rel).write_text(text, encoding="utf-8")
    if files:
        subprocess.run(["git", "-C", str(d), "add", "-A"], check=True)
    return d


CLEAN = {"Cargo.toml": "[workspace]\n", "a/Cargo.toml": "[package]\nname = \"a\"\n",
         ".github/workflows/ci.yml": "jobs:\n  t:\n    steps:\n      - run: cargo test --release\n"}


class FloatBuildFlags(unittest.TestCase):
    def test_a_clean_tree_passes(self):
        self.assertEqual(fb.main(["--root", str(repo(CLEAN))]), 0)

    def test_each_flag_form_fails(self):
        cases = {
            ".cargo/config.toml": '[build]\nrustflags = ["-C", "target-cpu=native"]\n',
            "a/Cargo.toml": '[package]\nname = "a"\n[profile.release]\n# fine\n',
            ".github/workflows/ci.yml": "env:\n  RUSTFLAGS: -Ctarget-cpu=native\n",
            "scripts/preflight.sh": "RUSTFLAGS='-C target-cpu native' cargo build\n",
            "build.rs": 'fn main() { println!("cargo:rustc-env=X=-ffast-math"); }\n',
        }
        for rel, text in cases.items():
            if rel == "a/Cargo.toml":
                continue
            with self.subTest(file=rel):
                self.assertEqual(fb.main(["--root", str(repo({**CLEAN, rel: text}))]), 1)

    def test_a_comment_is_not_a_flag(self):
        tree = {**CLEAN, ".github/workflows/ci.yml": "# never set target-cpu=native here\njobs: {}\n"}
        self.assertEqual(fb.main(["--root", str(repo(tree))]), 0)

    def test_files_that_cannot_carry_flags_are_not_read(self):
        self.assertEqual(fb.build_files(repo({**CLEAN, "README.md": "target-cpu=native"})),
                         [".github/workflows/ci.yml", "Cargo.toml", "a/Cargo.toml"])

    def test_no_build_file_compares_nothing(self):
        self.assertEqual(fb.main(["--root", str(repo({"README.md": "x"}))]), 2)

    def test_the_real_tree_is_clean(self):
        self.assertEqual(fb.main([]), 0)


if __name__ == "__main__":
    unittest.main()
