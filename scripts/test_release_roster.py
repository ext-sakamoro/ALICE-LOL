#!/usr/bin/env python3
"""scripts/release_roster.py の試験: workspace の写しの manifest と library の集合の写しを 1 つずつ崩して出口を確かめる"""
from __future__ import annotations

import io
import os
import re
import shutil
import sys
import tempfile
import tomllib
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import release_roster as rr  # noqa: E402

MEMBERS = tomllib.loads((rr.ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["members"]
# the files that hold a copy of the library set (the workflows are copied as a whole directory)
COPIES = [".github/workflows/ci.yml", "scripts/preflight.sh", rr.SEMVER_PATHS, *rr.NOTES]


class Roster(unittest.TestCase):
    def setUp(self):
        if getattr(self, "tmp", None) is not None:
            self.tmp.cleanup()  # (a subtest starts again from a fresh copy)
        # a copy of the workspace manifests only
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        shutil.copy(rr.ROOT / "Cargo.toml", self.root / "Cargo.toml")
        for m in tomllib.loads((rr.ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["members"]:
            (self.root / m).mkdir()
            shutil.copy(rr.ROOT / m / "Cargo.toml", self.root / m / "Cargo.toml")
            readme = tomllib.loads((rr.ROOT / m / "Cargo.toml").read_text(encoding="utf-8"))["package"].get("readme")
            if isinstance(readme, str) and (rr.ROOT / m / readme).is_file() and not (self.root / m / readme).exists():
                (self.root / m / readme).resolve().parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(rr.ROOT / m / readme, self.root / m / readme)
        shutil.copytree(rr.ROOT / ".github" / "workflows", self.root / ".github" / "workflows")
        for rel in COPIES:
            (self.root / rel).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(rr.ROOT / rel, self.root / rel)

    def tearDown(self):
        self.tmp.cleanup()

    def run_check(self) -> tuple[int, str]:
        err = io.StringIO()
        with redirect_stdout(io.StringIO()), redirect_stderr(err):
            code = rr.main(self.root)
        return code, err.getvalue()

    def set_publish(self, member: str, line: str | None) -> None:
        p = self.root / member / "Cargo.toml"
        s = re.sub(r"^publish = .*\n", "", p.read_text(encoding="utf-8"), flags=re.M)
        if line is not None:
            s = s.replace('version = "', f"{line}\nversion = \"", 1)
        p.write_text(s, encoding="utf-8")

    def edit(self, rel: str, old: str, new: str, count: int = 1) -> None:
        p = self.root / rel
        s = p.read_text(encoding="utf-8")
        self.assertIn(old, s, f"{rel} no longer holds {old!r}: the test needs a new anchor")
        p.write_text(s.replace(old, new, count), encoding="utf-8")

    def test_the_repository_passes(self):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = rr.main(self.root)
        self.assertEqual(code, 0, err.getvalue())
        self.assertIn(f"read {len(MEMBERS)} members, 0 publishable", out.getvalue())

    def test_nothing_is_released_and_the_library_is_three_crates(self):
        self.assertEqual(rr.ROSTER, frozenset())
        self.assertEqual(rr.LIBRARY, {"alice-lol", "alice-lol-macro", "alice-lol-layout"})

    def assert_drift(self, rel: str, old: str, new: str, *named: str) -> None:
        self.setUp()
        self.edit(rel, old, new)
        code, err = self.run_check()
        self.assertEqual(code, 1, err)
        self.assertIn(rel, err)
        for n in named:
            self.assertIn(n, err)

    def test_one_copy_that_drifts_fails_and_is_named(self):
        cases = [
            (".github/workflows/ci.yml", "        run: scripts/registry_consumer.sh\n",
             "        run: |\n          python3 scripts/package_integrity.py --member alice-lol-layout\n"
             "          scripts/registry_consumer.sh\n", "--member alice-lol-layout"),
            ("scripts/preflight.sh", "REGISTRY_CONSUMER_ALLOW_DIRTY=1 scripts/registry_consumer.sh\n",
             "python3 scripts/package_integrity.py --member alice-lol\n"
             "REGISTRY_CONSUMER_ALLOW_DIRTY=1 scripts/registry_consumer.sh\n", "--member alice-lol"),
            (".github/workflows/ci.yml", "        run: scripts/registry_consumer.sh\n",
             "        run: cargo publish --dry-run -p alice-lol\n", "cargo publish"),
            (".github/workflows/ci.yml", "--workspace --exclude alice-lol --lib",
             "--workspace --exclude alice-lol --exclude alice-lol-layout --lib", "lints alice-lol-layout"),
            ("scripts/preflight.sh", "--workspace --exclude alice-lol --lib",
             "--workspace --exclude alice-lol --exclude alice-lol-macro --lib", "lints alice-lol-macro"),
            ("alice-law-conformance.toml", "library crates (alice-lol, alice-lol-layout, alice-lol-macro)",
             "library crates (alice-lol, alice-lol-macro)", "lacks alice-lol-layout"),
            ("README.md", "library crate (alice-lol, alice-lol-layout, alice-lol-macro)",
             "library crate (alice-lol, alice-lol-layout, alice-lol-macro, alice-lol-ui)", "alice-lol-ui"),
            ("README_JP.md", "library crate (alice-lol, alice-lol-layout, alice-lol-macro)",
             "published crate (alice-lol, alice-lol-layout, alice-lol-macro)", "nothing is published"),
            (".github/paths/semver-checks.txt", "alice-lol-layout/src/**\n", "", "lacks alice-lol-layout/src/**"),
            (".github/paths/semver-checks.txt", "alice-lol-macro/Cargo.toml\n", "",
             "lacks alice-lol-macro/Cargo.toml"),
            ("README_JP.md", "| `alice-lol-layout` |", "| `alice-lol-layouts` |", "crate table"),
            ("README.md", "| `alice-lol-layout` |", "| `alice-lol-layouts` |", "crate table"),
        ]
        for rel, old, new, named in cases:
            with self.subTest(rel=rel, named=named):
                self.assert_drift(rel, old, new, named)

    def test_a_release_workflow_that_comes_back_fails_and_is_named(self):
        (self.root / rr.RELEASE).write_text("name: Release\non: push\njobs: {}\n", encoding="utf-8")
        code, err = self.run_check()
        self.assertEqual(code, 1, err)
        self.assertIn(rr.RELEASE, err)

    def test_a_commented_publish_line_is_not_a_run(self):
        p = self.root / ".github" / "workflows" / "ci.yml"
        p.write_text(p.read_text(encoding="utf-8") + "# cargo publish is not run here\n", encoding="utf-8")
        self.assertEqual(self.run_check()[0], 0)

    def test_no_workflow_read_is_an_error(self):
        for f in (self.root / ".github" / "workflows").glob("*.yml"):
            f.unlink()
        code, err = self.run_check()
        self.assertEqual(code, 2, err)
        self.assertIn("no workflow read", err)

    def test_a_library_crate_that_is_not_a_member_fails(self):
        ws = self.root / "Cargo.toml"
        ws.write_text(ws.read_text(encoding="utf-8").replace('"alice-lol-layout", ', "", 1), encoding="utf-8")
        code, err = self.run_check()
        self.assertEqual(code, 1, err)
        self.assertIn("alice-lol-layout is in the library set but is not a workspace member", err)

    def test_a_library_manifest_whose_metadata_drifts_fails_and_is_named(self):
        for old, new, named in [('license = "LicenseRef-Proprietary"', 'license = "Apache-2.0"', "license"),
                                ('rust-version = "1.90"', 'rust-version = "1.91"', "rust-version"),
                                ('documentation = "https://docs.rs/alice-lol-layout"',
                                 'documentation = "https://docs.rs/alice-lol"', "documentation"),
                                ('readme = "README.md"', 'readme = "MISSING.md"', "readme")]:
            with self.subTest(named=named):
                self.assert_drift("alice-lol-layout/Cargo.toml", old, new, named)

    def test_a_copy_that_is_missing_or_holds_nothing_is_an_error(self):
        for rel in COPIES:
            with self.subTest(rel=rel, case="missing"):
                self.setUp()
                (self.root / rel).unlink()
                code, err = self.run_check()
                self.assertEqual(code, 2, err)
                self.assertIn(rel, err)
            with self.subTest(rel=rel, case="empty"):
                self.setUp()
                (self.root / rel).write_text("nothing here\n", encoding="utf-8")
                code, err = self.run_check()
                self.assertEqual(code, 2, err)
                self.assertIn(rel, err)

    def test_a_member_that_loses_publish_false_fails_and_is_named(self):
        for member in MEMBERS:
            with self.subTest(member=member):
                self.setUp()
                self.set_publish(member, None)
                code, err = self.run_check()
                self.assertEqual(code, 1)
                self.assertIn(member, err)

    def test_an_empty_registry_list_is_not_publishable(self):
        self.set_publish("alice-lol-ui", "publish = []")
        self.assertEqual(self.run_check()[0], 0)
        self.set_publish("alice-lol-ui", 'publish = ["crates-io"]')
        self.assertEqual(self.run_check()[0], 1)

    def test_publish_from_the_workspace_is_resolved(self):
        ws = self.root / "Cargo.toml"
        # the workspace's own [workspace.package] table is replaced by each case below
        base = re.sub(r"^\[workspace\.package\]\n(?:[^\[\n][^\n]*\n|\n)*", "", ws.read_text(encoding="utf-8"), flags=re.M)
        self.assertNotIn("[workspace.package]", base)
        # [workspace.package] publish = true: a member that inherits it can be published
        ws.write_text(base + "\n[workspace.package]\npublish = true\n", encoding="utf-8")
        self.set_publish("alice-lol-ui", "publish.workspace = true")
        code, err = self.run_check()
        self.assertEqual(code, 1)
        self.assertIn("alice-lol-ui", err)
        # publish = false there: the member that inherits it cannot
        ws.write_text(base + "\n[workspace.package]\npublish = false\n", encoding="utf-8")
        self.assertEqual(self.run_check()[0], 0)
        # nothing in [workspace.package]: cargo's default, publishable
        ws.write_text(base, encoding="utf-8")
        self.assertEqual(self.run_check()[0], 1)

    def test_a_publish_value_that_is_not_understood_is_an_error(self):
        for line in ['publish = "yes"', "publish = 1", "publish.other = true"]:
            with self.subTest(line=line):
                self.setUp()
                self.set_publish("alice-lol-ui", line)
                self.assertEqual(self.run_check()[0], 2)

    def test_a_member_path_that_is_not_there_is_an_error(self):
        ws = self.root / "Cargo.toml"
        ws.write_text(ws.read_text(encoding="utf-8").replace('members = [', 'members = ["crates/*", ', 1), encoding="utf-8")
        code, err = self.run_check()
        self.assertEqual(code, 2)
        self.assertIn("crates/*", err)

    def test_no_member_read_fails(self):
        (self.root / "Cargo.toml").write_text("[workspace]\nmembers = []\n", encoding="utf-8")
        self.assertEqual(self.run_check()[0], 2)


if __name__ == "__main__":
    unittest.main()
