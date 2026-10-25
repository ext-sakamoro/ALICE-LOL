#!/usr/bin/env python3
"""Tests for scripts/docs_lint.py.

Each case builds a small tree in a temporary directory, breaks exactly one
thing, and asserts that the linter reports it. The first case runs the linter
against this repository.
"""

from __future__ import annotations

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import docs_lint as dl  # noqa: E402


def _tmpdir(**kwargs) -> str:
    """A temporary directory removed when this module's tests end (fixtures are read after the call returns)"""
    td = tempfile.TemporaryDirectory(**kwargs)
    unittest.addModuleCleanup(td.cleanup)
    return td.name

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

CARGO = '[package]\nname = "demo"\nversion = "1.5.0"\n\n[features]\n'

CHANGELOG = """# Changelog

## [Unreleased]

### Added

- `world::step_n`: run several steps

### Changed

- **Behavior change:** `RigidBody::new` friction default 0.3 -> 0.5

### Fixed

- `fluid::kernel` normalisation

## [1.4.0] - 2026-09-17

### Added — old style heading kept as history

## [1.3.0] - 2026-09-16
"""

EXAMPLE = "use demo::step;\n\nlet x = step(1.0);\nassert!(x > 0.0);\n"
README = "# demo\n\nA math library. The memory footprint is small.\n\n```rust\n" + EXAMPLE + "```\n"
LIB_RS = "//! demo\n//!\n//! ```rust\n" + "".join("//! " + l + "\n" if l else "//!\n" for l in EXAMPLE.splitlines()) + "//! ```\n\npub fn step(x: f32) -> f32 { x }\n"


def tree(overrides: dict[str, str] | None = None, drop: tuple[str, ...] = ()) -> str:
    files = {
        dl.CRATE_TOML: CARGO,
        "CHANGELOG.md": CHANGELOG,
        "README.md": README,
        "README_JP.md": README,
        dl.LIB_RS: LIB_RS,
    }
    files.update(overrides or {})
    d = _tmpdir()
    for rel, text in files.items():
        if rel in drop:
            continue
        p = os.path.join(d, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w", encoding="utf-8") as f:
            f.write(text)
    return d


def errors(overrides=None, drop=()):
    return dl.check(tree(overrides, drop))[0]


class RealRepo(unittest.TestCase):
    def test_this_repository_passes(self):
        errs, counts = dl.check(ROOT)
        self.assertEqual(errs, [])
        self.assertGreaterEqual(counts["versions"], 3)
        self.assertEqual(counts["documents"], 3)


class Documents(unittest.TestCase):
    def test_clean_tree_passes(self):
        self.assertEqual(errors(), [])

    def test_missing_document(self):
        e = errors(drop=("README_JP.md",))
        self.assertTrue(any("README_JP.md: missing" in x for x in e), e)


class Example(unittest.TestCase):
    def test_identical_example_passes_and_is_counted(self):
        e, counts = dl.check(tree())
        self.assertEqual(e, [])
        self.assertEqual(counts["example lines"], len(EXAMPLE.splitlines()))

    def test_readme_example_differs_from_doctest(self):
        e = errors({"README.md": README.replace("step(1.0)", "step(2.0)")})
        self.assertTrue(any(f"differs from the {dl.LIB_RS} doctest at example line 3" in x for x in e), e)

    def test_doctest_missing(self):
        e = errors({dl.LIB_RS: "pub fn step(x: f32) -> f32 { x }\n"})
        self.assertTrue(any("no ```rust example block" in x for x in e), e)

    def test_readme_without_example(self):
        e = errors({"README.md": "# demo\n"})
        self.assertTrue(any("README.md: no ```rust example block" in x for x in e), e)
        self.assertTrue(any("`example lines` compared nothing" in x for x in e), e)


class Changelog(unittest.TestCase):
    def test_duplicate_version(self):
        e = errors({"CHANGELOG.md": CHANGELOG + "\n## [1.3.0] - 2026-09-16\n"})
        self.assertTrue(any("appear twice" in x for x in e), e)

    def test_unreleased_must_be_first(self):
        e = errors({"CHANGELOG.md": CHANGELOG.replace("## [Unreleased]", "## [1.5.0] - 2026-10-04\n\n## [Unreleased]")})
        self.assertTrue(any("not [Unreleased]" in x for x in e), e)

    def test_versions_descending(self):
        e = errors({"CHANGELOG.md": CHANGELOG.replace("## [1.3.0]", "## [1.4.1]")})
        self.assertTrue(any("is not newer" in x for x in e), e)

    def test_prerelease_sorts_below_its_release(self):
        cl = CHANGELOG + "\n## [1.0.0] - x\n\n## [1.0.0-preview.2] - x\n\n## [1.0.0-preview.1] - x\n"
        self.assertEqual(errors({"CHANGELOG.md": cl}), [])

    def test_cargo_version_without_section_or_unreleased(self):
        cl = CHANGELOG.replace("## [Unreleased]\n", "")
        cl = cl.replace("### Added\n\n- `world::step_n`", "## [1.4.5] - x\n\n### Added\n\n- `world::step_n`", 1)
        e = errors({"CHANGELOG.md": cl})
        self.assertTrue(any("no section and there is no [Unreleased]" in x for x in e), e)

    def test_cargo_version_older_than_newest_section(self):
        e = errors({dl.CRATE_TOML: CARGO.replace('1.5.0', '1.2.0')})
        self.assertTrue(any("older than the newest section" in x for x in e), e)

    def test_category_twice_in_unreleased(self):
        cl = CHANGELOG.replace("### Fixed\n", "### Added\n\n- extra\n\n### Fixed\n")
        e = errors({"CHANGELOG.md": cl})
        self.assertTrue(any("`### Added` appears 2 times" in x for x in e), e)

    def test_non_keep_a_changelog_category(self):
        cl = CHANGELOG.replace("### Fixed\n", "### Improvements\n\n- x\n\n### Fixed\n")
        e = errors({"CHANGELOG.md": cl})
        self.assertTrue(any("`### Improvements` is not a Keep a Changelog category" in x for x in e), e)

    def test_emoji_marker_in_unreleased(self):
        cl = CHANGELOG.replace("**Behavior change:**", "⚠️")
        e = errors({"CHANGELOG.md": cl})
        self.assertTrue(any("emoji status marker" in x for x in e), e)

    def test_released_sections_keep_their_old_headings(self):
        # "### Added — old style heading" in [1.4.0] is history, not checked
        self.assertEqual(errors(), [])

    def test_unreleased_without_categories_compares_nothing(self):
        cl = CHANGELOG.split("### Added")[0] + "## [1.4.0] - 2026-09-17\n"
        e = errors({"CHANGELOG.md": cl})
        self.assertTrue(any("compared nothing" in x and "categories" in x for x in e), e)

    def test_an_empty_unreleased_checks_the_newest_section(self):
        # just after a cut: empty [Unreleased], the work in the newest section
        body = CHANGELOG.split("## [Unreleased]\n", 1)[1]
        cut = CHANGELOG.split("## [Unreleased]\n", 1)[0] + "## [Unreleased]\n\n## [1.5.0] - 2026-10-10\n" + body
        self.assertEqual(errors({"CHANGELOG.md": cut}), [])
        bad = cut.replace("### Added", "### Added\n\n### Added", 1)
        e = errors({"CHANGELOG.md": bad})
        self.assertTrue(any("[1.5.0]" in x and "appears 2 times" in x for x in e), e)

    def test_an_empty_unreleased_above_a_section_without_categories_compares_nothing(self):
        cl = CHANGELOG.split("### Added")[0].split("## [Unreleased]")[0] + "## [Unreleased]\n\n## [1.5.0] - 2026-10-10\n\nprose only\n"
        e = errors({"CHANGELOG.md": cl})
        self.assertTrue(any("compared nothing" in x and "categories" in x for x in e), e)


if __name__ == "__main__":
    unittest.main()
