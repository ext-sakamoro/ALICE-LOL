#!/usr/bin/env python3
"""tests for scripts/entropy_deps_guard.py (ALICE-Law G10): a published member's normal or build dependency on
a source of entropy or wall-clock time fails, a dev-dependency or an unpublished member does not, and reading no
published member fails

    python3 scripts/test_entropy_deps_guard.py
"""
from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("entropy_deps_guard", HERE / "entropy_deps_guard.py")
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)


def meta(*pkgs: dict) -> dict:
    return {"workspace_members": [p["id"] for p in pkgs], "packages": list(pkgs)}


def pkg(name: str, deps: list[dict], publish=None) -> dict:
    return {"id": f"{name} 0.1.0", "name": name, "publish": publish, "dependencies": deps}


def dep(name: str, kind=None, target=None, rename=None) -> dict:
    return {"name": name, "kind": kind, "target": target, "rename": rename}


class Guard(unittest.TestCase):
    def test_a_clean_published_member_passes(self):
        read, problems = g.denied_deps(meta(pkg("alice-lol", [dep("glam"), dep("alice-sdf")])))
        self.assertEqual((read, problems), (["alice-lol"], []))

    def test_each_denied_crate_fails_as_a_normal_or_build_dependency(self):
        for name in ("rand", "rand_core", "rand_chacha", "getrandom", "uuid", "fastrand", "chrono", "time"):
            for kind in (None, "build"):
                _, problems = g.denied_deps(meta(pkg("alice-lol", [dep(name, kind=kind)])))
                self.assertEqual(len(problems), 1, (name, kind))
                self.assertIn(f"({name})", problems[0])

    def test_a_target_specific_or_renamed_dependency_still_fails(self):
        _, problems = g.denied_deps(meta(pkg("alice-lol", [dep("rand", target="cfg(unix)", rename="rng")])))
        self.assertEqual(len(problems), 1)
        self.assertIn("rng", problems[0])

    def test_dev_dependencies_and_unpublished_members_are_not_checked(self):
        read, problems = g.denied_deps(meta(pkg("alice-lol", [dep("rand", kind="dev")]),
                                            pkg("alice-lol-datagen", [dep("rand")], publish=[])))
        self.assertEqual((read, problems), (["alice-lol"], []))

    def test_names_that_only_contain_a_denied_word_pass(self):
        _, problems = g.denied_deps(meta(pkg("alice-lol", [dep("timely"), dep("operand"), dep("random-access")])))
        self.assertEqual(problems, [])

    def test_transitive_reach_follows_normal_and_build_edges_only(self):
        def node(i, *deps):
            return {"id": i, "deps": [{"pkg": d, "dep_kinds": [{"kind": k}]} for d, k in deps]}
        m = {"workspace_members": ["lol"], "packages": [
            {"id": "lol", "name": "alice-lol", "publish": None},
            {"id": "a", "name": "wgpu", "version": "1"}, {"id": "r", "name": "rand", "version": "0.8.5"},
            {"id": "u", "name": "uuid", "version": "1.2"}, {"id": "t", "name": "time", "version": "0.3"}],
            "resolve": {"nodes": [node("lol", ("a", None), ("t", "dev")), node("a", ("r", "build")),
                                  node("r", ("u", None)), node("u"), node("t")]}}
        self.assertEqual(g.transitive(m), ["rand 0.8.5", "uuid 1.2"])

    def test_no_published_member_is_compared_nothing(self):
        p = subprocess.run([sys.executable, str(HERE / "entropy_deps_guard.py"), "--manifest-path",
                            str(HERE / "clippy-lib-canary" / "Cargo.toml")], capture_output=True, text=True)
        self.assertEqual(p.returncode, 2, p.stderr)
        self.assertIn("compared nothing", p.stderr)

    def test_this_workspace_passes(self):
        p = subprocess.run([sys.executable, str(HERE / "entropy_deps_guard.py"), "--manifest-path",
                            str(HERE.parent / "Cargo.toml")], capture_output=True, text=True)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("alice-lol", p.stdout)


if __name__ == "__main__":
    unittest.main()
