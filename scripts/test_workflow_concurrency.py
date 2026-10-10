#!/usr/bin/env python3
"""Oracle for scripts/workflow_concurrency.py: a branch-push workflow without the key fails,
with it passes, tag-only and push-less workflows are exempt, an empty tree fails."""
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import workflow_concurrency as wc  # noqa: E402

JOBS = "jobs:\n  a:\n    runs-on: x\n    timeout-minutes: 5\n"


def run(*texts: str) -> int:
    with tempfile.TemporaryDirectory() as d:
        for i, t in enumerate(texts):
            Path(d, f"w{i}.yml").write_text(t, encoding="utf-8")
        return wc.main([d])


class Concurrency(unittest.TestCase):
    def test_a_branch_push_without_a_group_fails(self):
        self.assertEqual(run("name: x\non:\n  push:\n    branches: [main]\n" + JOBS), 1)

    def test_a_push_without_filters_needs_a_group_too(self):
        self.assertEqual(run("name: x\non:\n  push:\n  pull_request:\n" + JOBS), 1)

    def test_a_branch_push_with_a_group_passes(self):
        self.assertEqual(run("name: x\non:\n  push:\n    branches: [main]\nconcurrency:\n  group: g\n" + JOBS), 0)

    def test_a_tag_only_push_is_exempt(self):
        self.assertEqual(run("name: x\non:\n  push:\n    tags: ['v*']\n  workflow_dispatch:\n" + JOBS), 0)

    def test_a_workflow_without_push_is_exempt(self):
        self.assertEqual(run("name: x\non:\n  schedule:\n    - cron: '0 0 * * *'\n" + JOBS), 0)

    def test_one_bad_workflow_among_good_ones_fails(self):
        good = "name: x\non:\n  push:\nconcurrency:\n  group: g\n" + JOBS
        bad = "name: y\non:\n  push:\n    branches: ['ci/**']\n" + JOBS
        self.assertEqual(run(good, bad), 1)

    def test_no_workflow_fails(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(wc.main([d]), 1)

    def test_the_repository_passes(self):
        self.assertEqual(wc.main([]), 0)


if __name__ == "__main__":
    unittest.main()
