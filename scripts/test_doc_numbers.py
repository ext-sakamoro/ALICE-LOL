#!/usr/bin/env python3
"""Tests of scripts/doc_numbers.py (synthetic trees in a temp dir, and this repository)"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import doc_numbers as dn  # noqa: E402

SRC = '''
const TABLE: &[(&str, &[&str])] = &[
    ("shapes", &["sphere", "box3d", "torus"]),
    ("ops", &["union", "subtract"]),
];

pub enum Rule {
    /// one
    NonOverlap { a: u8, b: u8 },
    #[cfg(feature = "x")]
    Within(u8),
    Thin,
}

#[test]
fn a() {}
#[test]
fn b() {}
'''

CONFIG = """
[scan]
include = ["**/*.md"]
exclude = ["CHANGELOG.md"]

[[countable]]
pattern = '''(?i)(?<![\\w.])(?P<n>\\d+)\\s+(?:shapes|ops|rules|tests)\\b'''

[[countable]]
pattern = '''(?i)\\b(?:shapes|ops)\\s*\\(\\s*(?:<!--[^>]*-->\\s*)?(?P<n>\\d+)\\s*[),]'''

[measured]
bound_before = '''≤|<'''
bound_after = '''未満'''
units = '''ms\\b|samples/s\\b'''

[sources.shapes]
kind = "strings_in_block"
file = "src/lib.rs"
block = '''\\("shapes",\\s*&\\[(.*?)\\]'''

[sources.ops]
kind = "strings_in_block"
file = "src/lib.rs"
block = '''\\("ops",\\s*&\\[(.*?)\\]'''

[sources.all]
kind = "sum"
of = ["shapes", "ops"]

[sources.not_ops]
kind = "sum"
of = ["all"]
minus = ["ops"]

[sources.rules]
kind = "rust_enum_variants"
file = "src/lib.rs"
name = "Rule"

[sources.tests]
kind = "rust_test_fns"
file = "src/lib.rs"

[sources.examples]
kind = "glob"
glob = "examples/*.rs"
"""

DOC = """# Doc

The language has <!-- num: all -->5 names: <!-- num: shapes -->3 shapes and <!-- num: ops -->2 ops (<!-- num: not_ops -->3 names are not ops).

## Shapes (<!-- num: shapes -->3)

There are <!-- num: rules -->3 rules, <!-- num: tests -->2 tests and <!-- num: examples -->2 examples.

Eval takes 4 ms per call. <!-- measured: 2026-10-11, cargo bench on one core -->

Budget: ≤ 10 ms, and 20 ms 未満 for the whole run.
"""


def tree(doc: str = DOC, config: str = CONFIG, src: str = SRC, extra: dict[str, str] | None = None) -> str:
    d = tempfile.mkdtemp(prefix="docnum-")
    files = {"src/lib.rs": src, "scripts/doc_numbers.toml": config, "README.md": doc,
             "examples/a.rs": "", "examples/b.rs": "", "CHANGELOG.md": "- 99 shapes in 0.1\n"}
    files.update(extra or {})
    for rel, text in files.items():
        os.makedirs(os.path.dirname(os.path.join(d, rel)) or d, exist_ok=True)
        with open(os.path.join(d, rel), "w", encoding="utf-8") as f:
            f.write(text)
    return d


def swap(text: str, old: str, new: str) -> str:
    """replace exactly one occurrence (a replace that changes nothing would test nothing)"""
    assert text.count(old) == 1, old
    return text.replace(old, new)


def run(**kw) -> tuple[list[str], dict[str, int]]:
    d = tree(**kw)
    try:
        with open(os.path.join(d, "scripts/doc_numbers.toml"), "rb") as f:
            return dn.check(tomllib.loads(f.read().decode()), d)
    finally:
        shutil.rmtree(d)


def errors(**kw) -> list[str]:
    return run(**kw)[0]


class Clean(unittest.TestCase):
    def test_a_clean_tree_passes_and_compares_every_class(self):
        errs, counts = run()
        self.assertEqual(errs, [])
        self.assertEqual(counts, {"docs": 1, "marked": 8, "countable": 5, "measured": 1, "bounds": 2})

    def test_sources(self):
        d = tree()
        try:
            with open(os.path.join(d, "scripts/doc_numbers.toml"), "rb") as f:
                vals = dn.resolve(tomllib.load(f)["sources"], d)
        finally:
            shutil.rmtree(d)
        self.assertEqual(vals, {"shapes": 3, "ops": 2, "all": 5, "not_ops": 3, "rules": 3, "tests": 2, "examples": 2})


class Marked(unittest.TestCase):
    def test_a_stale_number_fails(self):
        e = errors(doc=DOC.replace("<!-- num: shapes -->3 shapes", "<!-- num: shapes -->4 shapes"))
        self.assertTrue(any("`shapes` says 4, the source has 3" in x for x in e), e)

    def test_a_change_in_the_code_fails_the_old_number(self):
        e = errors(src=SRC.replace('"torus"]', '"torus", "cone"]'))
        self.assertTrue(any("`shapes` says 3, the source has 4" in x for x in e), e)
        self.assertTrue(any("`all` says 5, the source has 6" in x for x in e), e)

    def test_an_enum_variant_added_fails(self):
        e = errors(src=SRC.replace("    Thin,\n", "    Thin,\n    Thick(u8),\n"))
        self.assertTrue(any("`rules` says 3, the source has 4" in x for x in e), e)

    def test_a_marker_without_a_number_fails(self):
        e = errors(doc=DOC.replace("<!-- num: tests -->2 tests", "<!-- num: tests --> tests"))
        self.assertTrue(any("marker `tests` is not followed by a number" in x for x in e), e)

    def test_a_marker_with_an_unknown_key_fails(self):
        e = errors(doc=DOC + "\n<!-- num: shape -->3\n")
        self.assertTrue(any("marker `shape` has no source" in x for x in e), e)

    def test_a_removed_marker_makes_the_number_unmarked(self):
        e = errors(doc=DOC.replace("<!-- num: shapes -->3 shapes", "3 shapes"))
        self.assertTrue(any("unmarked count `3 shapes`" in x for x in e), e)

    def test_a_source_no_marker_uses_fails(self):
        e = errors(doc=DOC.replace("<!-- num: examples -->2 examples", "the examples"))
        self.assertTrue(any("source `examples` is not used by any marker" in x for x in e), e)


class Countable(unittest.TestCase):
    def test_an_unmarked_count_fails(self):
        e = errors(doc=DOC + "\nIt checks 7 rules.\n")
        self.assertTrue(any("README.md:" in x and "unmarked count `7 rules`" in x for x in e), e)

    def test_a_heading_count_without_a_marker_fails(self):
        e = errors(doc=DOC + "\n## Ops (2, since 0.2)\n")
        self.assertTrue(any("unmarked count `Ops (2,`" in x for x in e), e)

    def test_a_count_inside_a_code_block_is_still_read(self):
        e = errors(doc=DOC + "\n```text\n// 7 rules\n```\n")
        self.assertTrue(any("unmarked count `7 rules`" in x for x in e), e)

    def test_an_allowed_count_passes_and_needs_a_reason(self):
        allow = '\n[[allow]]\nfile = "README.md"\ntext = "In 0.1 there were 7 rules"\nreason = "history"\n'
        self.assertEqual(errors(doc=DOC + "\nIn 0.1 there were 7 rules.\n", config=CONFIG + allow), [])
        with self.assertRaisesRegex(dn.ConfigError, "without a reason"):
            run(doc=DOC + "\nIn 0.1 there were 7 rules.\n", config=CONFIG + allow.replace('"history"', '" "'))

    def test_an_allow_entry_that_matches_nothing_fails(self):
        allow = '\n[[allow]]\nfile = "README.md"\ntext = "In 0.1 there were 7 rules"\nreason = "history"\n'
        e = errors(config=CONFIG + allow)
        self.assertTrue(any("allow entry matched nothing" in x for x in e), e)

    def test_an_allow_entry_is_per_file(self):
        allow = '\n[[allow]]\nfile = "docs/other.md"\ntext = "In 0.1 there were 7 rules"\nreason = "history"\n'
        e = errors(doc=DOC + "\nIn 0.1 there were 7 rules.\n", config=CONFIG + allow)
        self.assertTrue(any("README.md:" in x and "unmarked count `7 rules`" in x for x in e), e)

    def test_excluded_files_are_not_read(self):
        _, counts = run()
        self.assertEqual(counts["docs"], 1)  # CHANGELOG.md says "99 shapes" and is excluded


class Measured(unittest.TestCase):
    def test_a_measured_value_without_a_note_fails(self):
        e = errors(doc=DOC + "\nThroughput is 5,000 samples/s.\n")
        self.assertTrue(any("measured value `5,000 samples/s` without" in x for x in e), e)

    def test_a_note_in_the_same_paragraph_covers_it(self):
        self.assertEqual(errors(doc=DOC + "\nThroughput is 5,000 samples/s.\n<!-- measured: 2026-10-11, datagen --n 20000 -->\n"), [])

    def test_a_note_needs_a_date_and_how(self):
        e = errors(doc=DOC + "\nThroughput is 5,000 samples/s. <!-- measured: last week -->\n")
        self.assertTrue(any("5,000 samples/s" in x for x in e), e)

    def test_a_note_on_one_table_row_does_not_cover_another(self):
        table = ("\n| step | time |\n|--|--|\n| parse | 3 ms <!-- measured: 2026-10-11, bench a --> |\n"
                 "| eval | 9 ms |\n")
        e = errors(doc=DOC + table)
        self.assertEqual([x for x in e if "measured value" in x and "9 ms" in x].__len__(), 1, e)
        self.assertFalse(any("3 ms" in x for x in e), e)

    def test_a_note_above_a_table_covers_all_rows(self):
        table = ("\n<!-- measured: 2026-10-11, bench a -->\n| step | time |\n|--|--|\n| parse | 3 ms |\n"
                 "| eval | 9 ms |\n")
        self.assertEqual(errors(doc=DOC + table), [])

    def test_a_note_above_a_code_block_covers_it_across_blank_lines(self):
        code = "\n<!-- measured: 2026-10-11, the run below -->\n```text\nstart\n\n// took 12 ms\n```\n"
        self.assertEqual(errors(doc=DOC + code), [])

    def test_a_bound_is_not_a_measurement(self):
        _, counts = run()
        self.assertEqual(counts["bounds"], 2)
        e = errors(doc=DOC.replace("≤ 10 ms", "10 ms"))
        self.assertTrue(any("measured value `10 ms`" in x for x in e), e)


class Empty(unittest.TestCase):
    def test_a_source_that_finds_nothing_is_a_config_error(self):
        with self.assertRaisesRegex(dn.ConfigError, "found 0 items"):
            run(src=SRC.replace('("ops", &["union", "subtract"]),', '("ops", &[]),'))

    def test_a_moved_source_file_is_a_config_error(self):
        with self.assertRaisesRegex(dn.ConfigError, "rules: FileNotFoundError"):
            run(config=CONFIG.replace('file = "src/lib.rs"\nname = "Rule"', 'file = "src/law.rs"\nname = "Rule"'))

    def test_a_renamed_enum_is_a_config_error(self):
        with self.assertRaisesRegex(dn.ConfigError, "enum Rule not found"):
            run(src=SRC.replace("pub enum Rule", "pub enum Rules"))

    def test_no_markers_fails(self):
        e = errors(doc="# Doc\n\nNothing counted here.\n", config=CONFIG.replace("[sources.examples]", "[sources.unused_but_listed]"))
        self.assertTrue(any("class `marked` compared nothing" in x for x in e), e)

    def test_no_documents_fails(self):
        e = errors(config=CONFIG.replace('include = ["**/*.md"]', 'include = ["docs/*.md"]'))
        self.assertTrue(any("class `docs` compared nothing" in x for x in e), e)

    def test_no_measured_value_fails_unless_declared(self):
        doc = DOC.replace("Eval takes 4 ms per call. <!-- measured: 2026-10-11, cargo bench on one core -->\n", "")
        self.assertTrue(any("class `measured` compared nothing" in x for x in errors(doc=doc)))
        cfg = swap(CONFIG, "[measured]\n", '[measured]\nnone = "no benchmarks in this repository"\n')
        cfg = swap(cfg, "bound_before = '''≤|<'''\nbound_after = '''未満'''\nunits = '''ms\\b|samples/s\\b'''\n", "")
        doc2 = swap(doc, "Budget: ≤ 10 ms, and 20 ms 未満 for the whole run.\n", "")
        self.assertEqual(errors(doc=doc2, config=cfg), [])

    def test_a_countable_pattern_without_group_n_is_a_config_error(self):
        with self.assertRaisesRegex(dn.ConfigError, "group named n"):
            run(config=swap(CONFIG, "(?P<n>\\d+)\\s+(?:shapes", "(\\d+)\\s+(?:shapes"))


class Cli(unittest.TestCase):
    def test_exit_codes(self):
        script = os.path.join(HERE, "doc_numbers.py")
        for doc, code in ((DOC, 0), (DOC + "\nIt checks 7 rules.\n", 1)):
            d = tree(doc=doc)
            try:
                p = subprocess.run([sys.executable, script, "--root", d], capture_output=True, text=True)
            finally:
                shutil.rmtree(d)
            self.assertEqual(p.returncode, code, p.stdout + p.stderr)
        d = tree(config=CONFIG.replace('kind = "glob"', 'kind = "globz"'))
        try:
            p = subprocess.run([sys.executable, script, "--root", d], capture_output=True, text=True)
        finally:
            shutil.rmtree(d)
        self.assertEqual(p.returncode, 2, p.stdout + p.stderr)


class ThisRepository(unittest.TestCase):
    def test_the_documents_agree_with_the_code(self):
        with open(os.path.join(ROOT, "scripts", "doc_numbers.toml"), "rb") as f:
            errs, counts = dn.check(tomllib.load(f), ROOT)
        self.assertEqual(errs, [])
        for name in ("docs", "marked", "countable", "measured", "bounds"):
            self.assertGreater(counts[name], 0, name)


if __name__ == "__main__":
    unittest.main()
