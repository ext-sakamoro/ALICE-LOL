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

    def test_stale_dependency_version(self):
        e = errors({"README.md": R + '\n```toml\nalice-lol = "0.3"\n```\n'})
        self.assertTrue(any('alice-lol = "0.3"' in x for x in e), e)

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

