#!/usr/bin/env python3
"""runtime_keywords.py の試験 (python3 scripts/test_runtime_keywords.py)"""
from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("rk", HERE / "runtime_keywords.py")
rk = importlib.util.module_from_spec(spec)
sys.modules["rk"] = rk
spec.loader.exec_module(rk)


PARSER_SNIPPET = """\
    fn parse_expr_inner(&mut self) -> Result<SdfNode, ParseError> {
        match name.as_str() {
            "sphere" => { Ok(SdfNode::Sphere { radius: 1.0 }) }
            "box3d" | "cube" => { Ok(SdfNode::Box3d { half_extents: Vec3::ZERO }) }
            // "commented_out" => { Ok(SdfNode::Sphere { radius: 1.0 }) }
            _ => Err(self.err("unknown LOL expression".to_string())),
        }
    }

    fn parse_intent(&mut self) -> Result<IntentNode, ParseError> {
        match verb.as_str() {
            "grasp" => { Ok(IntentNode::Grasp) }
            _ => Err(self.err("unknown intent verb".to_string())),
        }
    }
"""

TABLE_SNIPPET = """\
const SDF_SYNTAX: &[(&str, &[&str])] = &[
    (
        "primitives",
        &[
            "sphere", "box3d", "cube",
        ],
    ),
];
"""


class RuntimeKeywords(unittest.TestCase):
    def test_fn_body_extracts_up_to_the_next_fn(self):
        body = rk.fn_body(PARSER_SNIPPET, "parse_expr_inner")
        self.assertIn('"sphere"', body)
        self.assertNotIn("parse_intent", body)

    def test_dispatched_reads_single_and_or_arms(self):
        body = rk.fn_body(PARSER_SNIPPET, "parse_expr_inner")
        names = rk.dispatched(body)
        self.assertEqual(sorted(names), ["box3d", "cube", "sphere"])

    def test_dispatched_ignores_a_commented_out_arm(self):
        body = rk.fn_body(PARSER_SNIPPET, "parse_expr_inner")
        names = rk.dispatched(body)
        self.assertNotIn("commented_out", names)

    def test_fn_body_missing_function_raises(self):
        with self.assertRaises(SystemExit):
            rk.fn_body(PARSER_SNIPPET, "parse_nonexistent")

    def test_syntax_table_keywords_reads_groups(self):
        with tempfile.TemporaryDirectory() as d:
            table_path = Path(d, "syntax_table.rs")
            table_path.write_text(TABLE_SNIPPET, encoding="utf-8")
            old = rk.SYNTAX_TABLE
            try:
                rk.SYNTAX_TABLE = table_path
                groups = rk.syntax_table_keywords()
            finally:
                rk.SYNTAX_TABLE = old
        self.assertEqual(groups, {"primitives": {"sphere", "box3d", "cube"}})

    def _run_main(self, parser_text: str, table_text: str) -> int:
        with tempfile.TemporaryDirectory() as d:
            parser_path = Path(d, "runtime_parser.rs")
            table_path = Path(d, "syntax_table.rs")
            parser_path.write_text(parser_text, encoding="utf-8")
            table_path.write_text(table_text, encoding="utf-8")
            old_parser, old_table = rk.PARSER, rk.SYNTAX_TABLE
            try:
                rk.PARSER = parser_path
                rk.SYNTAX_TABLE = table_path
                return rk.main()
            finally:
                rk.PARSER, rk.SYNTAX_TABLE = old_parser, old_table

    def test_main_passes_when_the_sets_agree(self):
        self.assertEqual(self._run_main(PARSER_SNIPPET, TABLE_SNIPPET), 0)

    def test_main_fails_when_the_table_is_missing_a_parser_name(self):
        table_without_cube = TABLE_SNIPPET.replace('"cube",', "")
        self.assertEqual(self._run_main(PARSER_SNIPPET, table_without_cube), 1)

    def test_main_fails_when_the_table_has_a_name_the_parser_does_not(self):
        table_with_extra = TABLE_SNIPPET.replace('"cube",', '"cube", "ghost",')
        self.assertEqual(self._run_main(PARSER_SNIPPET, table_with_extra), 1)

    def test_main_fails_when_the_parser_file_has_no_keywords(self):
        self.assertEqual(self._run_main("fn parse_expr_inner() {}\n", TABLE_SNIPPET), 2)

    def test_main_fails_when_the_table_is_empty(self):
        empty_table = 'const SDF_SYNTAX: &[(&str, &[&str])] = &[\n];\n'
        self.assertEqual(self._run_main(PARSER_SNIPPET, empty_table), 2)

    def test_main_succeeds_against_the_real_committed_files(self):
        # the real regression this gate exists for: re-running it here (not just
        # trusting the CI job) catches drift in this test suite itself
        self.assertEqual(rk.main(), 0)


if __name__ == "__main__":
    unittest.main()
