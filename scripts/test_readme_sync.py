#!/usr/bin/env python3
"""Tests for scripts/readme_sync.py.

Each case builds a small tree in a temporary directory, breaks exactly one
thing, and asserts that the checker reports it. The first case runs the
checker against this repository, so a change that makes the real READMEs
disagree with the code fails here as well as in the CI step.
"""

from __future__ import annotations

import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import readme_sync as rs  # noqa: E402
import workflow_steps as ws  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

CARGO = """[package]
name = "alice-lol"
version = "0.4.0"
rust-version = "1.90"

[features]
default = ["glsl"]
glsl = []
physics = ["dep:alice-physics"]

[dependencies]
"""

SYNTAX = """//! table
const SDF_SYNTAX: &[(&str, &[&str])] = &[
    ("primitives", &["sphere", "box3d", "capsule_ab"]),
    (
        "csg",
        &[
            "union", "subtract",
        ],
    ),
    ("stdlib", &["mug"]),
];

const INTENT_SYNTAX: &[&str] = &["walk", "grasp"];

const LAW_SYNTAX: &[&str] = &["audit", "evidence"];

const RUNTIME_ONLY: &[&str] = &["capsule_ab"];
"""

MACRO = """fn parse_expr() {
    match name.as_str() {
        "sphere" => a(),
        "box3d" => b(),
        "union" | "subtract" => c(),
    }
}
"""

LAW = """pub enum Constraint {
    /// a
    NonOverlap {
        a: u8,
    },
    Reachable {
        b: u8,
    },
    #[cfg(feature = "physics")]
    ThermalField {
        c: u8,
    },
}

impl Constraint {
    pub const fn evidence_class(&self) -> Evidence {
        match self {
            Self::Reachable { .. } => Evidence::Proved,
            Self::NonOverlap { .. } => Evidence::Witnessed,
            #[cfg(feature = "physics")]
            Self::ThermalField { .. } => Evidence::Modelled {
                model: M,
            },
        }
    }
}
"""

RESEARCH = """pub enum ResearchVerdict {
    NoEvidence,
    Supports {
        rms: f64,
    },
}
"""

EXAMPLE = "let x = 1;\nassert_eq!(x, 1);\n"
LIB = "//! Crate doc.\n//!\n//! ```rust\n//! let x = 1;\n//! assert_eq!(x, 1);\n//! ```\n\npub mod a;\n"

README = """# t

## Example

```rust
{example}```

See [lib](alice-lol/src/lib.rs).

| Example | Shows |
|---------|-------|
| [`one`](alice-lol/examples/one.rs) | one |
| [`two`](alice-lol/examples/two.rs) | two |

## Syntax

<!-- readme-sync: syntax-primitives -->
```text
sphere box3d
capsule_ab
```

<!-- readme-sync: syntax-csg -->
```text
union subtract
```

<!-- readme-sync: syntax-stdlib -->
```text
mug
```

<!-- readme-sync: syntax-intent -->
```text
walk grasp
```

<!-- readme-sync: syntax-law -->
```text
audit evidence
```

## Laws

<!-- readme-sync: laws -->
| Constraint | Checks | Evidence |
|------------|--------|----------|
| `NonOverlap` | no overlap | `Witnessed` |
| `Reachable` | connected | `Proved` |
| `ThermalField` (feature `physics`) | heat | `Modelled` |

<!-- readme-sync: research-verdicts -->
| Verdict | Meaning |
|---------|---------|
| `NoEvidence` | none |
| `Supports` | agrees |

## Features

<!-- readme-sync: features -->
| Feature | Default | Description |
|---------|---------|-------------|
| `glsl` | yes | glsl |
| `physics` | no | physics |

## MSRV

Minimum supported Rust version: **1.90** <!-- readme-sync: msrv -->
"""


def crate(overrides: dict[str, str] | None = None, drop: tuple[str, ...] = ()) -> str:
    files = {
        rs.CRATE_TOML: CARGO,
        rs.LIB_RS: LIB,
        rs.SYNTAX_RS: SYNTAX,
        rs.MACRO_RS: MACRO,
        rs.LAW_RS: LAW,
        rs.RESEARCH_RS: RESEARCH,
        "README.md": README.format(example=EXAMPLE),
        "README_JP.md": README.format(example=EXAMPLE),
        "alice-lol/examples/one.rs": "fn main() {}\n",
        "alice-lol/examples/two.rs": "fn main() {}\n",
    }
    files.update(overrides or {})
    d = tempfile.mkdtemp()
    for rel, text in files.items():
        if rel in drop:
            continue
        p = os.path.join(d, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w", encoding="utf-8") as f:
            f.write(text)
    return d


def errors(overrides: dict[str, str] | None = None, drop: tuple[str, ...] = ()) -> list[str]:
    return rs.check(crate(overrides, drop))[0]


def both(text: str) -> dict[str, str]:
    return {"README.md": text, "README_JP.md": text}


R = README.format(example=EXAMPLE)
BLOCK = ("<!-- law-integration:begin law=" + "5" * 40 + " -->\n## ALICE-Law integration\n\n"
         "| Release train | alice-lol 0.4.0 (published) |\n| End to end check | covered (`alice-lol = \"0.4\"`) |\n"
         "<!-- law-integration:end -->\n")
WORKFLOW = ".github/workflows/law-readme-block.yml"
WORKFLOW_TEXT = ('on:\n  push:\n    branches: [main]\njobs:\n  check:\n    runs-on: ubuntu-24.04\n    steps:\n'
                 '      - uses: actions/checkout@v5\n      - name: Check the block\n        run: |\n'
                 '          python3 "$RUNNER_TEMP/gen-law-readme-block.py" check --law "$RUNNER_TEMP/law" --crate alice-lol \\\n'
                 '            --crate-repo . --law-main origin/main --lint warn\n')
CMD = 'python3 "$RUNNER_TEMP/gen-law-readme-block.py" check --law "$RUNNER_TEMP/law" --crate alice-lol \\'
# the same command present in the file but not run: each must leave the block unchecked
INACTIVE = {
    "in a YAML comment": WORKFLOW_TEXT.replace("        run: |\n          " + CMD, "        run: |\n          echo\n#         " + CMD),
    "in a shell comment": WORKFLOW_TEXT.replace("          " + CMD, "          # " + CMD),
    "job if: false": WORKFLOW_TEXT.replace("    runs-on: ubuntu-24.04\n", "    if: false\n    runs-on: ubuntu-24.04\n"),
    "step if: false": WORKFLOW_TEXT.replace("      - name: Check the block\n", "      - name: Check the block\n        if: ${{ false }}\n"),
    "no trigger": WORKFLOW_TEXT.replace("on:\n  push:\n    branches: [main]\n", "on: {}\n"),
    "unreadable YAML": WORKFLOW_TEXT.replace("jobs:\n", "jobs: &anchor\n"),
    # the check runs, but its failure does not fail the workflow
    "step continue-on-error": WORKFLOW_TEXT.replace("      - name: Check the block\n", "      - name: Check the block\n        continue-on-error: true\n"),
    "job continue-on-error": WORKFLOW_TEXT.replace("    runs-on: ubuntu-24.04\n", "    continue-on-error: true\n    runs-on: ubuntu-24.04\n"),
    "|| true": WORKFLOW_TEXT.replace("--lint warn\n", "--lint warn || true\n"),
    "set +e": WORKFLOW_TEXT.replace("        run: |\n", "        run: |\n          set +e\n"),
    "another shell": WORKFLOW_TEXT.replace("        run: |\n", "        shell: sh {0}\n        run: |\n"),
    "step continue-on-error expression": WORKFLOW_TEXT.replace("      - name: Check the block\n", "      - name: Check the block\n        continue-on-error: ${{ matrix.flaky }}\n"),
    "job continue-on-error expression": WORKFLOW_TEXT.replace("    runs-on: ubuntu-24.04\n", "    continue-on-error: ${{ matrix.experimental }}\n    runs-on: ubuntu-24.04\n"),
    "in the background": WORKFLOW_TEXT.replace("--lint warn\n", "--lint warn &\n          wait\n"),
    "workflow default shell": WORKFLOW_TEXT.replace("jobs:\n", "defaults:\n  run:\n    shell: bash {0}\njobs:\n"),
    "job default shell": WORKFLOW_TEXT.replace("    runs-on: ubuntu-24.04\n", "    defaults:\n      run:\n        shell: pwsh\n    runs-on: ubuntu-24.04\n"),
    # the command is not the check of this crate
    "echoed": WORKFLOW_TEXT.replace("          " + CMD, "          echo " + CMD),
    "another crate": WORKFLOW_TEXT.replace("--crate alice-lol ", "--crate alice-lol-ui "),
    # the workflow does not run on every push to main
    "dispatch only": WORKFLOW_TEXT.replace("on:\n  push:\n    branches: [main]\n", "on:\n  workflow_dispatch:\n"),
    "schedule only": WORKFLOW_TEXT.replace("on:\n  push:\n    branches: [main]\n", "on:\n  schedule:\n    - cron: '0 0 * * 1'\n"),
    "every branch ignored": WORKFLOW_TEXT.replace("    branches: [main]\n", "    branches-ignore: ['**']\n"),
    "another branch": WORKFLOW_TEXT.replace("    branches: [main]\n", "    branches: [no-such-branch]\n"),
    "path filter": WORKFLOW_TEXT.replace("    branches: [main]\n", "    branches: [main]\n    paths: ['src/**']\n"),
    "tags only": WORKFLOW_TEXT.replace("    branches: [main]\n", "    tags: ['v*']\n"),
    "negated branch": WORKFLOW_TEXT.replace("    branches: [main]\n", "    branches: ['**', '!main']\n"),
}
# forms that still gate on every push to main
GATING = {
    "as written": WORKFLOW_TEXT,
    "every branch": WORKFLOW_TEXT.replace("    branches: [main]\n", "    branches: ['**']\n"),
    "no filter": WORKFLOW_TEXT.replace("on:\n  push:\n    branches: [main]\n", "on: [push, pull_request]\n"),
    "block list": WORKFLOW_TEXT.replace("    branches: [main]\n", "    branches:\n      - main\n      - 'ci/**'\n"),
    "other branches ignored": WORKFLOW_TEXT.replace("    branches: [main]\n", "    branches-ignore: ['wip/**']\n"),
    "explicit bash": WORKFLOW_TEXT.replace("        run: |\n", "        shell: bash\n        run: |\n"),
    "redirection": WORKFLOW_TEXT.replace("--lint warn\n", "--lint warn 2>&1 | tee out.txt\n"),
    "and-list": WORKFLOW_TEXT.replace("--lint warn\n", "--lint warn && echo done\n"),
    "workflow default bash": WORKFLOW_TEXT.replace("jobs:\n", "defaults:\n  run:\n    shell: bash\njobs:\n"),
    "continue-on-error false": WORKFLOW_TEXT.replace("      - name: Check the block\n", "      - name: Check the block\n        continue-on-error: false\n"),
}


class RealRepo(unittest.TestCase):
    def test_this_repository_is_in_sync(self):
        errs, counts = rs.check(ROOT)
        self.assertEqual(errs, [])
        self.assertGreater(counts["syntax"], 400)
        self.assertGreater(counts["macro"], 100)
        self.assertGreaterEqual(counts["laws"], 20)
        self.assertGreater(counts["links"], 0)


class Checks(unittest.TestCase):
    def test_clean_tree_passes(self):
        errs, counts = rs.check(crate())
        self.assertEqual(errs, [])
        self.assertEqual(counts["syntax"], 20)
        self.assertEqual(counts["laws"], 6)
        self.assertEqual(counts["verdicts"], 4)
        self.assertEqual(counts["macro"], 4)

    # syntax
    def test_name_dropped_from_readme(self):
        e = errors({"README.md": R.replace("sphere box3d\n", "sphere\n")})
        self.assertTrue(any("README.md: syntax-primitives: missing ['box3d']" in x for x in e), e)

    def test_fake_name_added_to_readme(self):
        e = errors({"README_JP.md": R.replace("union subtract\n", "union subtract warp_drive\n")})
        self.assertTrue(any("syntax-csg" in x and "extra ['warp_drive']" in x for x in e), e)

    def test_name_listed_twice_in_a_group(self):
        e = errors({"README.md": R.replace("union subtract\n", "union subtract union\n")})
        self.assertTrue(any("syntax-csg: listed twice: ['union']" in x for x in e), e)

    def test_name_listed_in_two_groups(self):
        e = errors({"README.md": R.replace("```text\nmug\n```", "```text\nmug sphere\n```")})
        self.assertTrue(any("more than one syntax group: ['sphere']" in x for x in e), e)

    def test_new_parser_name_not_in_readme(self):
        e = errors({rs.SYNTAX_RS: SYNTAX.replace('"mug"]', '"mug", "kettle"]')})
        self.assertTrue(any("syntax-stdlib: missing ['kettle']" in x for x in e), e)

    def test_group_marker_removed(self):
        e = errors({"README.md": R.replace("<!-- readme-sync: syntax-intent -->\n", "")})
        self.assertTrue(any("no `<!-- readme-sync: syntax-intent -->` block" in x for x in e), e)

    def test_unknown_group_marker(self):
        e = errors({"README.md": R + "\n<!-- readme-sync: syntax-teleport -->\n```text\nx\n```\n"})
        self.assertTrue(any("`syntax-teleport` is not a group" in x for x in e), e)

    def test_all_syntax_markers_removed_compares_nothing(self):
        text = R
        for g in ("primitives", "csg", "stdlib", "intent", "law"):
            text = text.replace(f"<!-- readme-sync: syntax-{g} -->\n", "")
        e = errors(both(text))
        self.assertTrue(any("`syntax` compared nothing" in x for x in e), e)

    # macro
    def test_macro_accepts_an_undocumented_name(self):
        e = errors({rs.MACRO_RS: MACRO.replace('"box3d" => b(),', '"box3d" => b(),\n        "blob" => d(),')})
        self.assertTrue(any("macro only ['blob']" in x for x in e), e)

    def test_macro_lost_a_geometry_name(self):
        e = errors({rs.MACRO_RS: MACRO.replace('        "box3d" => b(),\n', "")})
        self.assertTrue(any("missing from macro ['box3d']" in x for x in e), e)

    # laws
    def test_law_row_removed(self):
        e = errors({"README.md": R.replace("| `Reachable` | connected | `Proved` |\n", "")})
        self.assertTrue(any("README.md: laws table" in x and "missing ['Reachable']" in x for x in e), e)

    def test_constraint_variant_added_in_code(self):
        law = LAW.replace("    Reachable {", "    Bounded {\n        d: u8,\n    },\n    Reachable {")
        law = law.replace("Self::Reachable { .. } => Evidence::Proved,",
                          "Self::Reachable { .. } => Evidence::Proved,\n            Self::Bounded { .. } => Evidence::Proved,")
        e = errors({rs.LAW_RS: law})
        self.assertTrue(any("laws table" in x and "missing ['Bounded']" in x for x in e), e)

    def test_wrong_evidence_class_in_readme(self):
        e = errors({"README_JP.md": R.replace("| connected | `Proved` |", "| connected | `Witnessed` |")})
        self.assertTrue(any("`Reachable` row states evidence ['Witnessed']" in x for x in e), e)

    def test_evidence_class_changed_in_code(self):
        e = errors({rs.LAW_RS: LAW.replace("Self::Reachable { .. } => Evidence::Proved",
                                           "Self::Reachable { .. } => Evidence::Witnessed")})
        self.assertTrue(any("evidence_class returns Witnessed" in x for x in e), e)

    def test_variant_without_evidence_class_arm(self):
        law = LAW.replace("    Reachable {", "    Bounded {\n        d: u8,\n    },\n    Reachable {")
        e = errors({rs.LAW_RS: law})
        self.assertTrue(any("evidence_class covers" in x for x in e), e)

    def test_verdict_variant_removed_from_readme(self):
        e = errors({"README.md": R.replace("| `Supports` | agrees |\n", "")})
        self.assertTrue(any("verdicts table" in x for x in e), e)

    # features / msrv / example / links / sections
    def test_feature_added_to_cargo_but_not_to_readme(self):
        e = errors({rs.CRATE_TOML: CARGO.replace("glsl = []", "glsl = []\nwgsl = []")})
        self.assertTrue(any("missing ['wgsl']" in x for x in e), e)

    def test_msrv_bumped_in_cargo_only(self):
        e = errors({rs.CRATE_TOML: CARGO.replace('rust-version = "1.90"', 'rust-version = "1.92"')})
        self.assertTrue(any("MSRV line says 1.90" in x for x in e), e)

    def test_msrv_marker_removed_compares_nothing(self):
        e = errors(both(R.replace(" <!-- readme-sync: msrv -->", "")))
        self.assertTrue(any("`msrv` compared nothing" in x for x in e), e)

    def test_a_dependency_version_fails_even_when_current(self):
        # the READMEs carry no version, so even the current one would go stale on the next release
        for line in ('alice-lol = "0.4"', 'alice-lol = { version = "0.4", features = ["llm"] }',
                     'alice-lol-macro = "0.2.1"', "Install alice-lol 0.4.0 from crates.io"):
            with self.subTest(line=line):
                e = errors({"README_JP.md": R + "\n```toml\n" + line + "\n```\n"})
                self.assertTrue(any(x.startswith("README_JP.md:") and "states a version" in x for x in e), e)

    def test_a_crate_name_without_a_version_is_not_a_version(self):
        self.assertEqual(errors({"README.md": R + "\nRun `cargo add alice-lol`, see alice-lol-macro.\n"}), [])

    def test_the_version_check_reads_every_readme_line(self):
        _, counts = rs.check(crate())
        self.assertEqual(counts["version"], 2 * len(R.split("\n")))

    # the generated ALICE-Law block: skipped by the version check only as an exact, checked span
    def test_a_version_inside_the_law_block_passes_when_the_workflow_checks_it(self):
        self.assertEqual(errors({**both(R + BLOCK), WORKFLOW: WORKFLOW_TEXT}), [])

    def test_a_version_after_the_law_block_still_fails(self):
        e = errors({**both(R + BLOCK + 'alice-lol = "0.4"\n'), WORKFLOW: WORKFLOW_TEXT})
        self.assertTrue(any("states a version" in x for x in e), e)

    def test_a_version_before_the_law_block_still_fails(self):
        e = errors({**both(R + "Install alice-lol 0.4.0\n" + BLOCK), WORKFLOW: WORKFLOW_TEXT})
        self.assertTrue(any("states a version" in x for x in e), e)

    def test_a_law_block_without_its_end_marker_is_not_skipped(self):
        e = errors({**both(R + BLOCK.replace(rs.LAW_END + "\n", "")), WORKFLOW: WORKFLOW_TEXT})
        self.assertTrue(any("law-integration markers" in x for x in e), e)
        self.assertTrue(any("states a version" in x for x in e), e)

    def test_two_law_blocks_or_reversed_markers_are_not_skipped(self):
        for text in (R + BLOCK + BLOCK, R + rs.LAW_END + "\n" + BLOCK.replace(rs.LAW_END + "\n", "")):
            with self.subTest(text=text[-80:]):
                e = errors({**both(text), WORKFLOW: WORKFLOW_TEXT})
                self.assertTrue(any("law-integration markers" in x for x in e), e)
                self.assertTrue(any("states a version" in x for x in e), e)

    def test_a_marker_sharing_its_line_with_text_is_not_skipped(self):
        e = errors({**both(R + BLOCK.replace(rs.LAW_END, rs.LAW_END + " alice-lol 0.4")), WORKFLOW: WORKFLOW_TEXT})
        self.assertTrue(any("law-integration markers" in x for x in e), e)

    def test_a_law_block_without_the_checking_workflow_is_not_skipped(self):
        e = errors(both(R + BLOCK))
        self.assertTrue(any("block would go unchecked" in x for x in e), e)
        self.assertTrue(any("states a version" in x for x in e), e)
        other = WORKFLOW_TEXT.replace("--crate alice-lol", "--crate alice-sdf")
        self.assertTrue(any("block would go unchecked" in x for x in errors({**both(R + BLOCK), WORKFLOW: other})))

    def test_a_command_that_does_not_run_leaves_the_block_unchecked(self):
        for why, text in INACTIVE.items():
            with self.subTest(why=why):
                self.assertNotEqual(text, WORKFLOW_TEXT)
                self.assertIn("gen-law-readme-block.py", text)
                e = errors({**both(R + BLOCK), WORKFLOW: text})
                self.assertTrue(any("block would go unchecked" in x for x in e), e)
                self.assertTrue(any("states a version" in x for x in e), e)

    def test_the_law_block_lines_are_not_counted_as_compared(self):
        _, counts = rs.check(crate({**both(R + BLOCK), WORKFLOW: WORKFLOW_TEXT}))
        self.assertEqual(counts["version"], 2 * len(R.split("\n")))

    def test_readme_example_diverges_from_doctest(self):
        e = errors({"README_JP.md": README.format(example=EXAMPLE.replace("x, 1", "x, 2"))})
        self.assertTrue(any("README_JP.md: first ```rust block" in x for x in e), e)

    def test_broken_relative_link(self):
        e = errors({"README.md": R + "\n[x](alice-lol/examples/gone.rs)\n"})
        self.assertTrue(any("alice-lol/examples/gone.rs" in x for x in e), e)

    def test_external_and_anchor_links_are_not_files(self):
        self.assertEqual(errors({"README.md": R + "\n[a](https://example.org/x) [b](#msrv)\n"}), [])

    def test_japanese_readme_missing_a_section(self):
        e = errors({"README_JP.md": R.replace("## MSRV\n", "")})
        self.assertTrue(any("`##` sections" in x for x in e), e)

    def test_missing_japanese_readme(self):
        e = errors(drop=("README_JP.md",))
        self.assertTrue(any("README_JP.md: missing" in x for x in e), e)



class WorkflowSteps(unittest.TestCase):
    """scripts/workflow_steps.py: the steps that can run, from the YAML structure"""

    def test_every_workflow_of_this_repository_is_read(self):
        d = os.path.join(ROOT, ".github", "workflows")
        names = [n for n in sorted(os.listdir(d)) if n.endswith((".yml", ".yaml"))]
        self.assertGreater(len(names), 0)
        for n in names:
            with self.subTest(n=n), open(os.path.join(d, n), encoding="utf-8") as f:
                self.assertIn("jobs", ws.parse(f.read()))
        with open(os.path.join(d, "law-readme-block.yml"), encoding="utf-8") as f:
            self.assertTrue(any(rs.LAW_BLOCK_RUN.match(l) for l in ws.gating_lines(f.read())))

    def test_steps_keys_and_scalars(self):
        doc = ws.parse('name: x  # c\non:\n  push:\njobs:\n  a:\n    steps:\n      - run: echo "a # b"\n'
                       "      - name: 'q'\n        run: >-\n          one\n          two\n")
        self.assertEqual(doc["name"], "x")
        self.assertEqual(doc["jobs"]["a"]["steps"][0]["run"], 'echo "a # b"')
        self.assertEqual(doc["jobs"]["a"]["steps"][1]["name"], "q")
        self.assertEqual(ws.gating_lines("on: push\njobs:\n  a:\n    if: github.ref == 'refs/heads/main'\n"
                                         "    steps:\n      - run: make\n"), ["make"])

    def test_forms_that_do_not_gate_have_no_check_line(self):
        for why, text in INACTIVE.items():
            with self.subTest(why=why):
                self.assertNotEqual(text, WORKFLOW_TEXT)
                self.assertFalse(any(rs.LAW_BLOCK_RUN.match(l) for l in ws.gating_lines(text)))

    def test_forms_that_gate_have_the_check_line(self):
        for why, text in GATING.items():
            with self.subTest(why=why):
                self.assertTrue(any(rs.LAW_BLOCK_RUN.match(l) for l in ws.gating_lines(text)))


class Examples(unittest.TestCase):
    """`examples` 検査の歯 — 表と disk の過不足を双方向で見ていること"""

    def test_an_example_missing_from_the_table_is_an_error(self):
        errs = errors({"alice-lol/examples/three.rs": "fn main() {}\n"})
        self.assertTrue(
            any("not listed in the table: three" in e for e in errs), errs
        )

    def test_a_table_row_without_a_file_is_an_error(self):
        errs = errors(
            both(R.replace("| [`two`](alice-lol/examples/two.rs) | two |", ""))
        )
        # file は在るのに行が消えた ⇒ 「表に無い」側で出る
        self.assertTrue(any("not listed in the table: two" in e for e in errs), errs)

    def test_a_row_pointing_at_a_deleted_example_is_an_error(self):
        errs = errors(drop=("alice-lol/examples/two.rs",))
        self.assertTrue(
            any("does not exist: two" in e for e in errs), errs
        )

    def test_a_missing_examples_directory_is_an_error(self):
        # ⚠️ 空振り防止 — ディレクトリごと無い時に素通りしない
        # ⚠️ ここは「compared nothing」では捕まらない: 表の行は数えられるので
        #    counts["examples"] が非 0 になる ⇒ 名指しの error が要る
        errs = errors(
            drop=("alice-lol/examples/one.rs", "alice-lol/examples/two.rs")
        )
        self.assertTrue(
            any("alice-lol/examples is missing" in e for e in errs), errs
        )


if __name__ == "__main__":
    unittest.main()

