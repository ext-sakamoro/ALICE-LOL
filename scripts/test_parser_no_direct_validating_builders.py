#!/usr/bin/env python3
"""parser_no_direct_validating_builders.py の試験 (python3 scripts/test_parser_no_direct_validating_builders.py)"""
from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location(
    "gate", HERE / "parser_no_direct_validating_builders.py"
)
gate = importlib.util.module_from_spec(spec)
sys.modules["gate"] = gate
spec.loader.exec_module(gate)


class Gate(unittest.TestCase):
    def test_a_direct_call_to_an_infallible_validating_builder_is_a_violation(self):
        text = (
            '"gridfinity_bin" => {\n'
            "    Ok(crate::stdlib::hardsurface::pattern_sdf::gridfinity_bin(&spec))\n"
            "}\n"
        )
        violations = gate.find_violations(text)
        self.assertEqual(len(violations), 1)
        self.assertEqual(violations[0][0], "gridfinity_bin")

    def test_a_call_to_the_try_variant_is_not_a_violation(self):
        text = (
            '"gridfinity_bin" => {\n'
            "    crate::stdlib::hardsurface::pattern_sdf::try_gridfinity_bin(&spec)\n"
            "        .map_err(|e| self.spec_error(e))\n"
            "}\n"
        )
        self.assertEqual(gate.find_violations(text), [])

    def test_an_aliased_use_import_is_a_violation(self):
        # the direct-call check alone cannot see this: `gb(&spec)` is neither
        # `gridfinity_bin(` nor `::gridfinity_bin(` once imported under an alias
        text = (
            "use crate::stdlib::hardsurface::pattern_sdf::gridfinity_bin as gb;\n"
            "fn f() { Ok(gb(&spec)) }\n"
        )
        violations = gate.find_violations(text)
        self.assertEqual(len(violations), 1)
        self.assertIn("use import", violations[0][0])

    def test_a_plain_use_import_without_an_alias_is_also_a_violation(self):
        text = "use crate::stdlib::hardsurface::pattern_sdf::gridfinity_bin;\n"
        violations = gate.find_violations(text)
        self.assertEqual(len(violations), 1)
        self.assertIn("use import", violations[0][0])

    def test_importing_the_try_variant_is_not_a_violation(self):
        # `\b` must not match inside "try_gridfinity_bin" as if it were a bare
        # "gridfinity_bin" suffix -- "_" is a word character, so there is no
        # boundary there
        text = "use crate::stdlib::hardsurface::pattern_sdf::try_gridfinity_bin;\n"
        self.assertEqual(gate.find_violations(text), [])

    def test_a_lol_text_string_literal_with_the_keyword_name_is_not_a_violation(self):
        # test fixtures write `.lol` source text like "gridfinity_bin(2, 2, 6)" as a
        # Rust string literal -- the keyword name there is not a Rust function call
        text = 'let node = parse_lol("gridfinity_bin(2, 2, 6)").unwrap();\n'
        self.assertEqual(gate.find_violations(text), [])

    def test_a_comment_mentioning_the_name_is_not_a_violation(self):
        text = "// ::gridfinity_bin(..) used to be called directly here\n"
        self.assertEqual(gate.find_violations(text), [])

    def test_every_known_validating_builder_is_individually_detected(self):
        for name in gate.VALIDATING_BUILDERS:
            text = f"Ok(crate::stdlib::hardsurface::x::{name}(&spec))\n"
            violations = gate.find_violations(text)
            self.assertEqual(len(violations), 1, f"{name} was not detected")
            self.assertEqual(violations[0][0], name)

    def test_main_fails_on_a_missing_file(self):
        old_target = gate.TARGET
        try:
            gate.TARGET = Path("/nonexistent/runtime_parser.rs")
            self.assertEqual(gate.main(), 2)
        finally:
            gate.TARGET = old_target

    def test_main_fails_on_an_empty_file(self):
        old_target = gate.TARGET
        try:
            with tempfile.TemporaryDirectory() as d:
                empty = Path(d, "runtime_parser.rs")
                empty.write_text("", encoding="utf-8")
                gate.TARGET = empty
                self.assertEqual(gate.main(), 2)
        finally:
            gate.TARGET = old_target

    def test_main_succeeds_against_the_real_committed_file(self):
        # the real regression this gate exists for: re-running it here (not just
        # trusting the CI job) catches drift in this test suite itself
        self.assertEqual(gate.main(), 0)


if __name__ == "__main__":
    unittest.main()
