#!/usr/bin/env python3
"""Tests of scripts/doc_numbers.py (synthetic git trees in a temp dir, and this repository)"""

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

[countable]
nouns = '''shapes?|ops?|rules?|tests?|examples?|names'''
counters = '''個|種類?|件'''
ignore_before = '''Phase'''

[measured]
bound_before = '''≤|<'''
bound_after = '''未満|\\+'''
units = '''ms\\b|s\\b(?!\\w)|samples/s\\b|fps\\b|lines\\b|x\\s+faster\\b|%\\s+smaller\\b'''

[sources.shapes]
noun = '''shape'''
kind = "strings_in_block"
file = "src/lib.rs"
block = '''\\("shapes",\\s*&\\[(.*?)\\]'''

[sources.ops]
noun = '''\\bops?\\b'''
kind = "strings_in_block"
file = "src/lib.rs"
block = '''\\("ops",\\s*&\\[(.*?)\\]'''

[sources.all]
noun = '''names'''
kind = "sum"
of = ["shapes", "ops"]

[sources.not_ops]
noun = '''names'''
kind = "sum"
of = ["all"]
minus = ["ops"]

[sources.rules]
noun = '''rule'''
kind = "rust_enum_variants"
file = "src/lib.rs"
name = "Rule"

[sources.tests]
noun = '''test'''
kind = "rust_test_fns"
file = "src/lib.rs"

[sources.examples]
noun = '''example'''
kind = "glob"
glob = "examples/*.rs"

[sources.shape_rows]
noun = '''shape'''
kind = "table_rows"
file = "README.md"
heading = '''^## Shapes'''

[[complete]]
rows = "shape_rows"
of = "shapes"
"""

DOC = """# Doc

The language has <!-- num: all -->5 names: <!-- num: shapes -->3 shapes and <!-- num: ops -->2 ops (<!-- num: not_ops -->3 names are not ops).

## Shapes (<!-- num: shape_rows -->3 of <!-- num: shapes -->3 shapes)

| Shape | Args |
|--|--|
| sphere | r |
| box3d | x, y, z |
| torus | R, r |

There are <!-- num: rules -->3 rules, <!-- num: tests -->2 tests and <!-- num: examples -->2 examples.

Eval takes 4 ms per call. <!-- measured: 2026-10-11, cargo bench on one core -->

Budget: ≤ 10 ms, and 20 ms 未満 for the whole run; at least 100+ tests by 1.0.

## Phase 2 Shapes

<!-- snapshot: 2026-01-01, what 0.1 had -->
- 0.1 had 2 shapes and 1 op
"""


def swap(text: str, old: str, new: str) -> str:
    """replace exactly one occurrence (a replace that changes nothing would test nothing)"""
    assert text.count(old) == 1, old
    return text.replace(old, new)


def tree(doc: str = DOC, config: str = CONFIG, src: str = SRC, extra: dict[str, str] | None = None,
         untracked: dict[str, str] | None = None) -> str:
    d = tempfile.mkdtemp(prefix="docnum-")
    files = {"src/lib.rs": src, "scripts/doc_numbers.toml": config, "README.md": doc,
             "examples/a.rs": "", "examples/b.rs": "", "CHANGELOG.md": "- 99 shapes in 0.1\n"}
    files.update(extra or {})

    def write(rel: str, text: str) -> None:
        os.makedirs(os.path.dirname(os.path.join(d, rel)) or d, exist_ok=True)
        with open(os.path.join(d, rel), "w", encoding="utf-8") as f:
            f.write(text)

    for rel, text in files.items():
        write(rel, text)
    subprocess.run(["git", "init", "-q"], cwd=d, check=True)
    subprocess.run(["git", "add", "-A"], cwd=d, check=True)
    for rel, text in (untracked or {}).items():
        write(rel, text)
    return d


def run(**kw) -> tuple[list[str], dict[str, int]]:
    d = tree(**kw)
    try:
        with open(os.path.join(d, "scripts/doc_numbers.toml"), "rb") as f:
            return dn.check(tomllib.loads(f.read().decode()), d)
    finally:
        shutil.rmtree(d)


def errors(**kw) -> list[str]:
    return run(**kw)[0]


def unmarked(doc_extra: str, **kw) -> list[str]:
    return [x for x in errors(doc=DOC + doc_extra, **kw) if "unmarked count" in x or "count in words" in x]


class Clean(unittest.TestCase):
    def test_a_clean_tree_passes_and_compares_every_class(self):
        errs, counts = run()
        self.assertEqual(errs, [])
        self.assertEqual(counts, {"docs": 1, "marked": 9, "countable": 11, "measured": 1, "bounds": 3, "history": 1})

    def test_sources(self):
        d = tree()
        try:
            with open(os.path.join(d, "scripts/doc_numbers.toml"), "rb") as f:
                vals = dn.resolve(tomllib.load(f)["sources"], d, dn.tracked(d))
        finally:
            shutil.rmtree(d)
        self.assertEqual(vals, {"shapes": 3, "ops": 2, "all": 5, "not_ops": 3, "rules": 3, "tests": 2,
                                "examples": 2, "shape_rows": 3})


class Marked(unittest.TestCase):
    def test_a_stale_number_fails(self):
        e = errors(doc=swap(DOC, "<!-- num: rules -->3 rules", "<!-- num: rules -->4 rules"))
        self.assertTrue(any("`rules` says 4, the source has 3" in x for x in e), e)

    def test_a_change_in_the_code_fails_the_old_number(self):
        e = errors(src=swap(SRC, '"torus"]', '"torus", "cone"]'))
        self.assertTrue(any("`shapes` says 3, the source has 4" in x for x in e), e)
        self.assertTrue(any("`all` says 5, the source has 6" in x for x in e), e)

    def test_an_enum_variant_added_fails(self):
        e = errors(src=swap(SRC, "    Thin,\n", "    Thin,\n    Thick(u8),\n"))
        self.assertTrue(any("`rules` says 3, the source has 4" in x for x in e), e)

    def test_a_marker_without_a_number_fails(self):
        e = errors(doc=swap(DOC, "<!-- num: tests -->2 tests", "<!-- num: tests --> tests"))
        self.assertTrue(any("marker `tests` is not followed by a number" in x for x in e), e)

    def test_a_marker_with_an_unknown_key_fails(self):
        e = errors(doc=DOC + "\n<!-- num: shape -->3 shapes\n")
        self.assertTrue(any("marker `shape` has no source" in x for x in e), e)

    def test_a_removed_marker_makes_the_number_unmarked(self):
        e = errors(doc=swap(DOC, "<!-- num: rules -->3 rules", "3 rules"))
        self.assertTrue(any("unmarked count `3 rules`" in x for x in e), e)

    def test_a_source_no_marker_uses_fails(self):
        e = errors(doc=swap(DOC, "<!-- num: examples -->2 examples", "the examples"))
        self.assertTrue(any("source `examples` is not used by any marker" in x for x in e), e)

    def test_a_marker_with_the_wrong_key_but_the_same_number_fails(self):
        # rules and shapes are both 3: the number alone cannot tell them apart, the noun can
        e = errors(doc=swap(DOC, "<!-- num: rules -->3 rules", "<!-- num: shapes -->3 rules"))
        self.assertTrue(any("marker `shapes` is not next to what it counts" in x for x in e), e)

    def test_a_marker_shares_the_noun_before_it_across_connectives(self):
        self.assertEqual(errors(), [])  # "## Shapes (3 of 3 shapes)" and "(3 names are not ops)"
        e = errors(doc=swap(DOC, "<!-- num: shapes -->3 shapes)", "<!-- num: shapes -->3 in total)"))
        self.assertEqual(e, [])  # "Shapes (3 of 3 in total)": only connectives, the heading's noun counts
        e = errors(doc=swap(DOC, "<!-- num: shapes -->3 shapes)", "<!-- num: shapes -->3 widgets)"))
        self.assertTrue(any("marker `shapes` is not next to what it counts" in x for x in e), e)


class Tables(unittest.TestCase):
    def test_a_complete_table_must_list_every_item(self):
        e = errors(src=swap(SRC, '"torus"]', '"torus", "cone"]'))
        self.assertTrue(any("table `shape_rows` has 3 rows but claims to list all 4 of `shapes`" in x for x in e), e)

    def test_rows_are_counted_without_the_header(self):
        e = errors(doc=swap(DOC, "| torus | R, r |\n", ""))
        self.assertTrue(any("`shape_rows` says 3, the source has 2" in x for x in e), e)

    def test_a_missing_heading_is_a_config_error(self):
        with self.assertRaisesRegex(dn.ConfigError, "heading"):
            run(doc=DOC.replace("## Shapes (", "## Forms ("))


class Countable(unittest.TestCase):
    def test_every_form_of_an_unmarked_count_fails(self):
        forms = {
            "It checks 7 rules.": "7 rules",
            "It checks 7 hard rules.": "7 hard rules",  # one word between
            "A 7-rule checker.": "7-rule",
            "## Rules (7)": "Rules (7",
            "## Rules (7, since 0.2)": "Rules (7",
            "Rules: 7": "Rules: 7",
            "- unit tests: **123**": "tests: **123",
            "Available widgets (9 total)": "(9 total",
            "| Rules | 7 | NonOverlap |": "| … | 7 |",
            "7 個の rule": "7 個の rule",
            "7個のrule": "7個のrule",
            "rule は 7 種類": "rule は 7 種類",
            "テスト rules 7件": "rules 7件",
            "形状は5種": "5種",
            "seventy-one shapes": "seventy-one shapes",
            "五種": "五種",
        }
        for line, shown in forms.items():
            with self.subTest(line=line):
                e = unmarked("\n" + line + "\n")
                self.assertTrue(any(f"`{shown}" in x for x in e), e)

    def test_ordinals_singulars_and_other_numbers_are_not_counts(self):
        for line in ("## Phase 2 Rules", "1 件も比べていない", "one rule per file", "8-byte rule packet",
                     "a 3-valued rule", "in 2026-10-10 a test job", "(0, shape)"):
            with self.subTest(line=line):
                self.assertEqual(unmarked("\n" + line + "\n"), [])

    def test_a_count_inside_a_code_block_is_still_read(self):
        e = unmarked("\n```text\n// 7 rules\n```\n")
        self.assertTrue(any("unmarked count `7 rules`" in x for x in e), e)

    def test_a_snapshot_block_records_the_past(self):
        _, counts = run()
        self.assertEqual(counts["history"], 1)
        e = errors(doc=swap(DOC, "<!-- snapshot: 2026-01-01, what 0.1 had -->\n", ""))
        self.assertTrue(any("unmarked count `2 shapes`" in x for x in e), e)

    def test_a_bound_count_is_a_requirement(self):
        self.assertEqual(unmarked("\n| Tests | 200+ |\n"), [])
        self.assertTrue(unmarked("\n| Tests | 200 |\n"))
        e = errors(doc=swap(DOC, "100+ tests", "100 tests"))
        self.assertTrue(any("unmarked count `100 tests`" in x for x in e), e)

    def test_an_allowed_count_passes_and_needs_a_reason(self):
        allow = '\n[[allow]]\nfile = "README.md"\ntext = "In 0.1 there were 7 rules"\nreason = "history"\n'
        self.assertEqual(errors(doc=DOC + "\nIn 0.1 there were 7 rules.\n", config=CONFIG + allow), [])
        with self.assertRaisesRegex(dn.ConfigError, "without a text or a reason"):
            run(doc=DOC + "\nIn 0.1 there were 7 rules.\n", config=CONFIG + swap(allow, '"history"', '" "'))

    def test_an_allow_entry_covers_only_its_line(self):
        allow = '\n[[allow]]\nfile = "README.md"\ntext = "In 0.1 there were 7 rules"\nreason = "history"\n'
        e = errors(doc=DOC + "\nIn 0.1 there were 7 rules.\nNow 9 rules apply.\n", config=CONFIG + allow)
        self.assertTrue(any("unmarked count `9 rules`" in x for x in e), e)
        self.assertFalse(any("7 rules" in x for x in e), e)

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
    def test_every_unit_needs_a_note(self):
        for line, shown in {"Throughput is 5,000 samples/s.": "5,000 samples/s", "It runs at 60 fps.": "60 fps",
                            "It is 2x faster.": "2x faster", "Output is 93% smaller.": "93% smaller",
                            "A build takes 8 s.": "8 s", "About ~600 lines of GLSL.": "600 lines"}.items():
            with self.subTest(line=line):
                e = errors(doc=DOC + "\n" + line + "\n")
                self.assertTrue(any(f"measured value `{shown}` without" in x for x in e), e)

    def test_a_note_in_the_same_paragraph_covers_it(self):
        self.assertEqual(errors(doc=DOC + "\nThroughput is 5,000 samples/s.\n<!-- measured: 2026-10-11, datagen --n 20000 -->\n"), [])

    def test_a_note_needs_a_date_and_how(self):
        e = errors(doc=DOC + "\nThroughput is 5,000 samples/s. <!-- measured: last week -->\n")
        self.assertTrue(any("5,000 samples/s" in x for x in e), e)

    def test_a_snapshot_note_does_not_cover_a_measurement(self):
        e = errors(doc=DOC + "\n<!-- snapshot: 2026-10-11, old -->\nIt took 9 ms.\n")
        self.assertTrue(any("measured value `9 ms`" in x for x in e), e)

    def test_a_note_on_one_table_row_does_not_cover_another(self):
        table = ("\n| step | time |\n|--|--|\n| parse | 3 ms <!-- measured: 2026-10-11, bench a --> |\n"
                 "| eval | 9 ms |\n")
        e = errors(doc=DOC + table)
        self.assertEqual(len([x for x in e if "measured value" in x and "9 ms" in x]), 1, e)
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
        self.assertEqual(counts["bounds"], 3)
        e = errors(doc=swap(DOC, "≤ 10 ms", "10 ms"))
        self.assertTrue(any("measured value `10 ms`" in x for x in e), e)


class Empty(unittest.TestCase):
    def test_a_source_that_finds_nothing_is_a_config_error(self):
        with self.assertRaisesRegex(dn.ConfigError, "found 0 items"):
            run(src=swap(SRC, '("ops", &["union", "subtract"]),', '("ops", &[]),'))

    def test_a_moved_source_file_is_a_config_error(self):
        with self.assertRaisesRegex(dn.ConfigError, "rules: FileNotFoundError"):
            run(config=swap(CONFIG, 'file = "src/lib.rs"\nname = "Rule"', 'file = "src/law.rs"\nname = "Rule"'))

    def test_a_renamed_enum_is_a_config_error(self):
        with self.assertRaisesRegex(dn.ConfigError, "enum Rule not found"):
            run(src=swap(SRC, "pub enum Rule", "pub enum Rules"))

    def test_a_source_without_a_noun_is_a_config_error(self):
        with self.assertRaisesRegex(dn.ConfigError, "needs `noun`"):
            run(config=swap(CONFIG, "noun = '''rule'''\n", ""))

    def test_glob_counts_tracked_files_only(self):
        _, counts = run(untracked={"examples/scratch.rs": ""})
        self.assertEqual(counts["marked"], 9)
        self.assertEqual(errors(untracked={"examples/scratch.rs": ""}), [])  # still 2 examples

    def test_no_documents_fails(self):
        e = errors(config=swap(CONFIG, 'include = ["**/*.md"]', 'include = ["docs/*.md"]'))
        self.assertTrue(any("class `docs` compared nothing" in x for x in e), e)

    def test_an_empty_class_fails_unless_declared_not_applicable_with_a_reason(self):
        doc = swap(DOC, "<!-- snapshot: 2026-01-01, what 0.1 had -->\n- 0.1 had 2 shapes and 1 op\n", "")
        self.assertTrue(any("class `history` compared nothing" in x for x in errors(doc=doc)))
        na = '\n[not_applicable]\nhistory = "these documents record no past counts"\n'
        self.assertEqual(errors(doc=doc, config=CONFIG + na), [])
        with self.assertRaisesRegex(dn.ConfigError, "not_applicable"):
            run(doc=doc, config=CONFIG + '\n[not_applicable]\nhistory = " "\n')

    def test_a_marker_less_tree_fails(self):
        doc = "# Doc\n\n## Shapes\n\n| Shape |\n|--|\n| sphere |\n| box3d |\n| torus |\n"
        e = errors(doc=doc)
        self.assertTrue(any("class `marked` compared nothing" in x for x in e), e)


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
        d = tree(config=swap(CONFIG, 'kind = "glob"', 'kind = "globz"'))
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
        for name in dn.CLASSES:
            self.assertGreater(counts[name], 0, name)


if __name__ == "__main__":
    unittest.main()
