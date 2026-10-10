#!/usr/bin/env python3
"""Tests for scripts/law_ambiguity_lint.py.

Each rule gets a dedicated must-red fixture (a small synthetic law violating
only that rule) and a must-green check that the same law passes once fixed.
The historical laws/spike/kepler_energy_bounded.law (commit 8d92276~1, kept as
scripts/testdata/kepler_energy_bounded_pre_split.law, before it was split into _kdk.law / _dkd.law) is the real-world case the method-scope
and range-provenance rules exist for: it stated no method scope and no range
provenance, and its single max-ratio criterion silently covered every method
and the whole input range at once.
"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE))
import law_ambiguity_lint as lint  # noqa: E402

HEADER = (
    "# demo law\n"
    "law demo\n"
    "kind research\n"
)


def write(tmp: Path, name: str, text: str) -> Path:
    p = tmp / name
    p.write_text(text, encoding="utf-8")
    return p


class MethodScopeRule(unittest.TestCase):
    def test_x_invariant_without_a_method_claim_is_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER +
                      'claim the energy error stays bounded\n'
                      'x-method kdk\n'
                      'x-invariant last(n) <= 1.2 * first(n)\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)
        self.assertIn("names a specific method", errs[0])

    def test_a_claim_that_merely_contains_the_word_method_is_not_enough(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER +
                      'claim the method is unclear\n'
                      'x-method free\n'
                      'x-invariant last(n) <= 1.2 * first(n)\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)
        self.assertIn("names a specific method", errs[0])

    def test_x_invariant_with_a_method_claim_passes(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER +
                      'claim the energy error stays bounded\n'
                      'claim the method is velocity Verlet (kick-drift-kick), exactly as '
                      'written on the x-method line\n'
                      'x-method kdk\n'
                      'x-invariant last(n) <= 1.2 * first(n)\n')
            errs = lint.lint([p])
        self.assertEqual(errs, [])

    def test_no_x_invariant_does_not_require_a_method_claim(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER +
                      'claim c = c0 * exp(-k*t)\n'
                      'output c mol/L = c0*exp(-k*t)\n')
            errs = lint.lint([p])
        self.assertEqual(errs, [])


class RangeProvenanceRule(unittest.TestCase):
    def test_a_ranged_input_with_invariants_and_no_measurement_comment_is_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER +
                      'claim the method is kdk\n'
                      '# chosen because it looked right\n'
                      'input e 1 range 0.2 0.8\n'
                      'x-invariant last(n) <= 1.2 * first(n)\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)
        self.assertIn("how the range was measured", errs[0])

    def test_a_measured_range_comment_with_a_script_path_passes(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER +
                      'claim the method is kdk\n'
                      '# Valid range, measured with conformance/kepler_sweep.py\n'
                      'input e 1 range 0.2 0.8\n'
                      'x-invariant last(n) <= 1.2 * first(n)\n')
            errs = lint.lint([p])
        self.assertEqual(errs, [])

    def test_a_measured_comment_with_a_dangling_path_still_fails(self):
        # a path that looks real but does not exist must not satisfy the rule
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER +
                      'claim the method is kdk\n'
                      '# Valid range, measured with conformance/does_not_exist.py\n'
                      'input e 1 range 0.2 0.8\n'
                      'x-invariant last(n) <= 1.2 * first(n)\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)
        self.assertIn("how the range was measured", errs[0])

    def test_a_measured_word_without_a_script_path_still_fails(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER +
                      'claim the method is kdk\n'
                      '# the range above was measured by hand\n'
                      'input e 1 range 0.2 0.8\n'
                      'x-invariant last(n) <= 1.2 * first(n)\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)

    def test_no_ranged_input_does_not_require_a_measurement_comment(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER +
                      'claim the method is kdk\n'
                      'x-invariant last(n) <= 1.2 * first(n)\n')
            errs = lint.lint([p])
        self.assertEqual(errs, [])


class LanguageNeutralRule(unittest.TestCase):
    def test_a_primitive_type_name_in_a_claim_is_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER + 'claim c is a f64 value\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)
        self.assertIn("f64", errs[0])

    def test_a_language_neutral_phrasing_of_the_same_claim_passes(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER +
                      'claim c is a double-precision floating point value\n')
            errs = lint.lint([p])
        self.assertEqual(errs, [])

    def test_a_std_container_type_name_is_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER + 'claim the result is an Option\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)

    def test_a_double_colon_path_is_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER + 'claim see std::f64::consts::PI\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)
        self.assertIn("::", errs[0])

    def test_a_method_call_syntax_is_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER + 'claim call x.sqrt() to get the root\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)

    def test_an_underscore_grouped_literal_is_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER + 'claim the limit is 1_000_000\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)

    def test_a_type_suffixed_literal_is_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER + 'claim the step is 3.14f64\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)

    def test_a_digit_grouped_and_type_suffixed_literal_together_is_flagged(self):
        # neither alternative alone matched this combined form (regex gap)
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER + 'claim the limit is 1_000u32\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)
        self.assertIn("1_000u32", errs[0])

    def test_a_plain_decimal_without_suffix_or_grouping_passes(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER + 'claim the step is 3.14159\n')
            errs = lint.lint([p])
        self.assertEqual(errs, [])

    def test_an_alice_crate_name_is_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER + 'claim see alice-physics for the reference\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)

    def test_a_source_line_citing_a_rust_test_file_is_exempt(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER +
                      'claim c = c0 * exp(-k*t)\n'
                      'source ALICE-Physics tests/analytic_scalar_field.rs:25 (k), :77 (tol)\n'
                      'output c mol/L = c0*exp(-k*t)\n')
            errs = lint.lint([p])
        self.assertEqual(errs, [])

    def test_applies_even_without_any_x_invariant(self):
        # the vocabulary rule has no x-invariant precondition, unlike the other two
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER + 'claim the result is a u32\n'
                      'output c mol/L = c0*exp(-k*t)\n')
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)


class HistoricalKeplerLaw(unittest.TestCase):
    """The real law this lint exists for: laws/spike/kepler_energy_bounded.law
    before commit 8d92276 split it, kept verbatim in scripts/testdata/ (the test
    does not read the repository history, which a shallow clone lacks); the
    designated must-red fixture for the method-scope and range-provenance rules."""

    def test_the_pre_split_law_fails_both_rules(self):
        out = (HERE / "testdata" / "kepler_energy_bounded_pre_split.law").read_text(encoding="utf-8")
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "kepler_energy_bounded.law", out)
            errs = lint.lint([p])
        kinds = {"method" if "method" in e else "range" for e in errs}
        self.assertEqual(len(errs), 2, errs)
        self.assertEqual(kinds, {"method", "range"})

    def test_the_current_split_laws_pass(self):
        for name in ("kepler_energy_bounded_kdk.law", "kepler_energy_bounded_dkd.law"):
            errs = lint.lint([ROOT / "laws" / "spike" / name])
            self.assertEqual(errs, [], name)


class RealRepoLaws(unittest.TestCase):
    # identifier_feature_independent.law:4 used to read "the crate is built
    # once per feature set" (a Rust-jargon "crate" this lint's tightened
    # language-neutral rule caught); fixed upstream (laws/ is owned
    # separately from this lint) to "the package is built...", so the
    # corpus is clean again.
    def test_every_current_law_passes_every_rule(self):
        files = sorted((ROOT / "laws" / "spike").glob("*.law"))
        self.assertGreater(len(files), 0)
        errs = lint.lint(files)
        self.assertEqual(errs, [])


class CliAndExitCodes(unittest.TestCase):
    def test_no_files_given_is_usage_error(self):
        self.assertEqual(lint.main([]), 2)

    def test_a_glob_matching_nothing_compares_nothing_and_fails(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(lint.main([str(Path(d) / "none.law")]), 2)

    def test_a_clean_law_exits_0(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER + 'claim c = c0 * exp(-k*t)\n')
            self.assertEqual(lint.main([str(p)]), 0)

    def test_a_flagged_law_exits_1(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", HEADER + 'claim the result is a u32\n')
            self.assertEqual(lint.main([str(p)]), 1)

    def test_a_file_with_no_law_content_is_an_error_not_a_silent_pass(self):
        with tempfile.TemporaryDirectory() as d:
            p = write(Path(d), "a.law", "# just a comment, no claim/verdict/x-invariant\n"
                      "law demo\nkind research\n")
            errs = lint.lint([p])
        self.assertEqual(len(errs), 1)
        self.assertIn("not a law file", errs[0])


if __name__ == "__main__":
    unittest.main()
