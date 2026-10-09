#!/usr/bin/env python3
"""Flags Law prose that existing parsers do not validate.

research_law.rs checks units and ranges for the simple `output = f(inputs)`
form. The `x-state`/`x-ode`/`x-method`/`x-invariant` extensions are free text
that existing parsers only count, never check the content of (see
alice-lol/tests/spike_law_files_parse.rs's module doc). A law judged by an
`x-invariant` over a trajectory is only well-defined for the method and the
input range it was actually measured against; the historical
laws/spike/kepler_energy_bounded.law (before it was split into
kepler_energy_bounded_kdk.law / _dkd.law) stated no method scope and no range
provenance, and its single `last(n) <= 1.2 * first(n)` criterion silently
covered every method and every n/e pair at once -- including some where the
criterion is order/parity-sensitive and the stated bound does not hold.

Rules (independent; each is one function below plus a dedicated must-red
fixture in scripts/test_law_ambiguity_lint.py):

  method-scope       a law with >=1 `x-invariant` line must have a `claim`
                      line mentioning "method": an invariant judged by
                      trajectory shape is specific to the stated integration
                      method, not every method the law's x-ode could describe.

  range-provenance    a law with >=1 `x-invariant` line and >=1 `input ...
                      range` must have a comment documenting how the range
                      was measured (a "measured"/"sweep" word and something
                      that looks like a script path): an unexplained numeric
                      range cannot be checked against how it was derived.

  language-neutral    no law's claim/verdict/comment prose may contain
                      Rust-specific vocabulary (primitive or std type names,
                      `::` paths, `.method()` call syntax, Rust literal
                      notation, an alice-* crate name): LOL states laws
                      independent of the language instrumented against them.
                      Applies to every law, not only ones with x-invariant.
                      `source` lines are citations to the Rust test that
                      measured the law and are exempt.

Neither precondition rule depends on what the x-invariant content says (only
that it exists and how many there are), and the vocabulary rule does not read
x-invariant content at all, so both generalize past physics laws to the
coding-rule laws a later pass is expected to add (P4): a rule here is a
function of the lines a .law file has, not of physics semantics.

Exit 0 if every law passes every applicable rule, 1 if any fails, 2 if no law
file was read (compared nothing: a bad glob or directory must not pass
silently). A directory argument is expanded to its `*.law` files.

  scripts/law_ambiguity_lint.py laws/spike
  scripts/law_ambiguity_lint.py laws/spike/kepler_energy_bounded_kdk.law
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# Rust-specific vocabulary a law's prose must not contain (CLAUDE.md policy:
# LOL states laws independent of the implementation language; these are
# common technical terms, not confidential names, so a plain list is fine).
RUST_PRIMITIVE_TYPES = frozenset({
    "f32", "f64", "u8", "u16", "u32", "u64", "u128", "usize",
    "i8", "i16", "i32", "i64", "i128", "isize",
})
RUST_STD_TYPES = frozenset({
    "Vec", "Option", "Result", "HashMap", "HashSet", "BTreeMap", "BTreeSet",
    "Box", "String", "Arc", "Rc", "RefCell", "Cow",
})
RUST_WORD_RE = re.compile(
    r"\b(" + "|".join(sorted(RUST_PRIMITIVE_TYPES | RUST_STD_TYPES)) + r")\b"
)
RUST_PATH_RE = re.compile(r"::")
RUST_METHOD_CALL_RE = re.compile(r"\.[a-zA-Z_][a-zA-Z0-9_]*\(")
# a Rust integer/float literal with a type suffix (`1u32`, `3.14f64`) or a
# digit-group underscore separator (`1_000_000`); either form on its own is
# this notation, so neither requires the other
RUST_LITERAL_RE = re.compile(
    r"\b\d+(?:_\d+)+\b"
    r"|\b\d+(?:\.\d+)?(?:u8|u16|u32|u64|u128|usize|i8|i16|i32|i64|i128|isize|f32|f64)\b"
)
ALICE_CRATE_RE = re.compile(r"\balice-[a-z][a-z0-9-]*\b")

RANGE_PROVENANCE_RE = re.compile(r"\b(measured|sweep)\b", re.IGNORECASE)
SCRIPT_PATH_RE = re.compile(r"[\w./-]+\.(?:py|rs|sh)\b")
METHOD_CLAIM_RE = re.compile(r"\bmethod\b", re.IGNORECASE)


class LintError(Exception):
    """A law file this script cannot read as a law (not a rule failure)."""


class Law:
    def __init__(self, path: Path):
        self.path = path
        self.name = path.stem
        self.claims: list[tuple[int, str]] = []
        self.verdicts: list[tuple[int, str]] = []
        self.comments: list[tuple[int, str]] = []
        self.x_invariant_count = 0
        self.input_range_count = 0
        text = path.read_text(encoding="utf-8")
        for lineno, line in enumerate(text.split("\n"), start=1):
            stripped = line.strip()
            if stripped.startswith("#"):
                self.comments.append((lineno, stripped[1:].strip()))
            elif stripped.startswith("claim "):
                self.claims.append((lineno, stripped[len("claim "):].strip()))
            elif stripped.startswith("verdict "):
                self.verdicts.append((lineno, stripped[len("verdict "):].strip()))
            elif stripped.startswith("x-invariant "):
                self.x_invariant_count += 1
            elif stripped.startswith("input ") and " range " in stripped:
                self.input_range_count += 1
        if not self.claims and not self.verdicts and self.x_invariant_count == 0:
            raise LintError(f"{path}: no `claim`/`verdict`/`x-invariant` line (not a law file?)")

    def prose(self) -> list[tuple[int, str]]:
        """Every line a reader would read as the law's own words: claims,
        verdicts, and comments. Excludes `source` (a citation, not a claim)."""
        return sorted(self.claims + self.verdicts + self.comments)


def rule_method_scope(law: Law) -> list[str]:
    if law.x_invariant_count == 0:
        return []
    if any(METHOD_CLAIM_RE.search(text) for _, text in law.claims):
        return []
    return [
        f"{law.path}: {law.x_invariant_count} x-invariant line(s) but no `claim` "
        f"mentions \"method\" (an invariant judged over a trajectory is specific "
        f"to one method; state which one it is scoped to)"
    ]


def rule_range_provenance(law: Law) -> list[str]:
    if law.x_invariant_count == 0 or law.input_range_count == 0:
        return []
    for _, text in law.comments:
        if RANGE_PROVENANCE_RE.search(text) and SCRIPT_PATH_RE.search(text):
            return []
    return [
        f"{law.path}: {law.input_range_count} ranged input(s) and "
        f"{law.x_invariant_count} x-invariant line(s) but no comment documents how "
        f"the range was measured (a \"measured\"/\"sweep\" word plus a script path)"
    ]


def rule_language_neutral(law: Law) -> list[str]:
    errors = []
    for lineno, text in law.prose():
        hits = []
        hits += RUST_WORD_RE.findall(text)
        if RUST_PATH_RE.search(text):
            hits.append("::")
        if RUST_METHOD_CALL_RE.search(text):
            hits.append(RUST_METHOD_CALL_RE.search(text).group(0))
        lit = RUST_LITERAL_RE.search(text)
        if lit:
            hits.append(lit.group(0))
        crate = ALICE_CRATE_RE.search(text)
        if crate:
            hits.append(crate.group(0))
        if hits:
            errors.append(
                f"{law.path}:{lineno}: Rust-specific vocabulary {hits!r} in "
                f"law prose (write numeric types and APIs in language-neutral "
                f"terms, e.g. \"double-precision floating point\", \"non-negative "
                f"integer\")"
            )
    return errors


RULES = (rule_method_scope, rule_range_provenance, rule_language_neutral)


def lint(paths: list[Path]) -> list[str]:
    """Findings from every path that could be read as a law file. A path that
    does not exist, or does exist but has no `claim`/`verdict`/`x-invariant`
    line, is itself a finding rather than being silently skipped: main()
    additionally fails (exit 2) when none of `paths` could be read this way,
    so an empty glob cannot pass."""
    errors = []
    for p in paths:
        if not p.exists():
            errors.append(f"error: {p}: no such file")
            continue
        try:
            law = Law(p)
        except LintError as e:
            errors.append(str(e))
            continue
        for rule in RULES:
            errors.extend(rule(law))
    return errors


def checked_count(paths: list[Path]) -> int:
    """How many of `paths` are law files lint() could evaluate (read + had at
    least one claim/verdict/x-invariant line), independent of whether any
    rule then failed on them."""
    n = 0
    for p in paths:
        if not p.exists():
            continue
        try:
            Law(p)
        except LintError:
            continue
        n += 1
    return n


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    if not argv:
        print("usage: scripts/law_ambiguity_lint.py <law file or dir>...", file=sys.stderr)
        return 2
    paths: list[Path] = []
    for a in argv:
        p = Path(a)
        paths += sorted(p.glob("*.law")) if p.is_dir() else [p]
    errors = lint(paths)
    for e in errors:
        print(e, file=sys.stderr)
    checked = checked_count(paths)
    if checked == 0:
        print("error: no law file read (compared nothing)", file=sys.stderr)
        return 2
    if errors:
        return 1
    print(f"ok: {checked} law file(s), {len(RULES)} rule(s), no finding", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
