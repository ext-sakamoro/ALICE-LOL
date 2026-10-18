#!/usr/bin/env python3
"""Every `#[allow(clippy::disallowed_methods, reason = "...")]` names its pattern.

A per-site exemption from the `disallowed_methods` lint (libm direct calls, or
the `f32`/`f64` `max`/`min` equal-zero tie, see `clippy.toml` at the
repository root and `.github/clippy-lib/clippy.toml`) is only as good as the
proof behind it, and free-text `reason` strings cannot be checked by a
machine. This script scans every workspace member's `src/` (discovered from
the root `Cargo.toml`, not hand-maintained — a new member is covered the
moment it joins) and requires every reason to start with one of a fixed set
of prefixes, each naming the specific pattern that makes the exemption safe:

  * `nonzero-const:` — the other operand is a literal or an expression that is
    provably never zero, so the equal-zero tie cannot occur
  * `both-abs:`       — every operand compared is a `.abs()` result (or a
    quantity built only from `.abs()` results and a `+0.0`/positive seed), so
    it is always `+0` or positive, never `-0`
  * `int-cast:`        — the result is cast to an integer type immediately
    after, which erases the sign before anything observes it
  * `test-probe-alternative:` — a `#[cfg(test)]` function that deliberately
    calls the forbidden form as the comparison baseline for an oracle (e.g.
    `geometry_id.rs`'s `platform_trig`, which exists to show the routed and
    platform-libm forms disagree) — never production code. This prefix is
    only accepted when the allow's own line actually falls inside a
    `#[cfg(test)] mod ... { }` block in the SAME file, or the file itself is
    under a `tests/` directory — not on the word `reason` alone, which would
    accept the prefix on an un-gated production function that merely claims
    to be a test probe
  * `raw-helper:` — the one definition of a named `raw_max_f32`/`raw_min_f32`-
    style helper in `src/raw_minmax.rs`: every *caller* of that helper is a
    provably-safe site (one of the three reasons above, or an int-cast),
    counted and named by `scripts/raw_minmax_manifest.py`'s own manifest —
    this prefix is only accepted on that one file (`RAW_HELPER_FILE_REL`
    below), never on a caller claiming the same prefix elsewhere
  * `cli-report:` — a binary's own report of how long it ran (the wall clock read by
    `Instant::now` / `.elapsed()`, ALICE-Law G10), printed to the terminal and never written
    into what the binary produces — this prefix is only accepted in a file under a `src/bin/`
    directory, never in library code

Only `allow` with one of these reasons may lower the lint. `#[expect(...)]` and `#[warn(...)]` on
clippy::disallowed_methods, any of these inside `#[cfg_attr(...)]`, and an allow / expect / warn of a
group that contains it (`clippy::all`, `clippy::style`) are refused: each turns the library clippy's
`-D clippy::disallowed_methods` off without a reason this gate can read.

A reason with none of these prefixes fails the gate (an exemption whose
pattern cannot be named this way is not actually proven safe — fix the call
site or widen the pattern list deliberately, in the same commit, not by
leaving the reason unprefixed). So does a file with `#[allow(dead_code)]`'s
unrelated reason being miscounted here, or zero sites found at all.

⚠️ An `#[allow(clippy::disallowed_methods, ...)]` (an *outer* attribute, on one
item/statement) silences the lint for the *whole annotated statement*, not
just the one method call the reason is about — the libm ban (`f32`/`f64`
`sin`/`cos`/`tan`/`atan`/`exp`/`ln`/`log`/`powf`/... in `alice-lol/clippy.toml`)
shares the same lint name, so a `nonzero-const:`/`both-abs:`/`int-cast:` allow
sitting on a statement that *also* happens to call one of those would
silently let it through too. This script therefore also scans the statement
each such allow actually covers (a cheap text scan: from the end of the
attribute to the next top-level `;` or enclosing `}`, not a real parse) for
any libm method/path name, and fails if one is found — that statement is not
exempt from the libm ban, whatever the max/min reason says.
`test-probe-alternative:` is the one prefix this does not apply to: a probe
exists specifically to call the forbidden libm form as a comparison
baseline, so finding one there is the point, not a leak.

⚠️⚠️ An *inner* attribute (`#![allow(...)]`, no item/statement after it — the
form clippy accepts at module or crate root) silences the lint for the
*entire enclosing scope*, not just "the next statement": the same
statement-boundary scan would stop at the first `;`/`}` it finds and miss a
libm call several statements later that the same inner attribute still
covers. For an inner attribute this script scans to the end of the file
instead (the common case is a `mod`-level `#![allow(...)]` at the top of a
small file dedicated to one routine; scanning past the file into a sibling
module cannot happen, since each file is its own scan unit here).

The libm pattern list matches three spellings of each name, not just the
method-call form: `.sin(` (method call), `f32::sin`/`f32::sin(` (path,
called or passed bare as a function reference, the `.fold(acc, f32::sin)`
style — `max`/`min` are not in this libm list, they are the separate
raw_minmax reasons above, this is only about the libm names), and
`<f32>::sin(` (fully-qualified call syntax). Missing any of these would let
a statement whose reason is correctly named for the max/min exemption
separately pass a libm function by reference instead of calling it inline,
unnoticed.

No `svg-pipeline-exempt:`-style blanket "this output isn't covered by an
oracle so the platform difference doesn't matter" prefix exists here on
purpose: `laser_pattern.rs`'s libm calls were exempted that way once (the SVG
output is itself a Law-governed artifact — laser-cut instructions a platform
difference would silently change) and are routed through `alice_det_math`
instead as of the fix that retired it. A call that genuinely cannot be
routed is not exempted here at all; it goes on the 基準's explicit
out-of-scope list with a reason and a plan.

Usage: `python3 scripts/disallowed_methods_allow_reasons.py`
`--root DIR` to check a different tree (used by the test)
"""

from __future__ import annotations

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_ROOT = os.path.dirname(HERE)

PREFIXES = ["nonzero-const:", "both-abs:", "int-cast:", "test-probe-alternative:", "raw-helper:", "cli-report:"]

# the one file `raw-helper:` is accepted on; `test-probe-alternative:` is
# additionally accepted inside a `#[cfg(test)] mod ... { }` block in any
# scanned file, or in a file under a `tests/` path segment (not scanned by
# this script today — only `src/` is — but checked the same way if that ever
# changes, rather than silently accepting an unscoped claim)
RAW_HELPER_FILE_REL = "alice-lol/src/raw_minmax.rs"

MEMBERS_RE = re.compile(r'members\s*=\s*\[(?P<body>[^\]]*)\]')
MEMBER_NAME_RE = re.compile(r'"([^"]+)"')


def workspace_members(root: str) -> list[str]:
    """Every `[workspace] members` entry in the root `Cargo.toml` (the gate
    covers the whole workspace, not just alice-lol: other members had no
    clippy.toml at all until this was added, so this list is discovered, not
    hand-maintained, so a new member is covered the moment it joins the
    workspace)."""
    path = os.path.join(root, "Cargo.toml")
    try:
        with open(path, encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return []
    m = MEMBERS_RE.search(text)
    if not m:
        return []
    return MEMBER_NAME_RE.findall(m.group("body"))

# matches `#[allow(...)]` (outer, group 1 empty) / `#![allow(...)]` (inner,
# group 1 is `!`) spanning possibly several lines (rustfmt wraps long
# attribute lists), capturing everything between the outer parens so a
# reason inside is found regardless of argument order
ALLOW_RE = re.compile(r"#\s*(?P<inner>!?)\s*\[\s*allow\s*\((?P<body>.*?)\)\s*\]", re.S)
REASON_RE = re.compile(r'reason\s*=\s*"(?P<text>(?:[^"\\]|\\.)*)"')

# any attribute: `#[` / `#![` up to its matching `]` (found by bracket depth in `attributes`)
ATTR_START_RE = re.compile(r"#\s*!?\s*\[")
LEVEL_RE = re.compile(r"\b(?P<level>allow|expect|warn)\s*\(")
GROUPS_RE = re.compile(r"\bclippy\s*::\s*(?:all|style)\b")


def attributes(text: str):
    """(start, attribute text) of every `#[...]` / `#![...]`, bracket depth counted (a nested `cfg_attr(..)`
    or a reason string containing `]` does not end it early; strings are skipped)"""
    for m in ATTR_START_RE.finditer(text):
        depth, i, n = 0, m.end() - 1, len(text)
        while i < n:
            c = text[i]
            if c == '"':
                i += 1
                while i < n and text[i] != '"':
                    i += 2 if text[i] == "\\" else 1
            elif c == "[":
                depth += 1
            elif c == "]":
                depth -= 1
                if depth == 0:
                    yield m.start(), text[m.start():i + 1]
                    break
            i += 1


def level_findings(text: str, rel: str) -> list[str]:
    """lint-level attributes that lower clippy::disallowed_methods other than a plain `#[allow(...)]`"""
    out = []
    for start, attr in attributes(text):
        if not LEVEL_RE.search(attr):
            continue
        names_ban = "disallowed_methods" in attr
        names_group = bool(GROUPS_RE.search(attr))
        if not (names_ban or names_group):
            continue
        line = text.count("\n", 0, start) + 1
        body = re.sub(r"\s+", "", attr)
        if "cfg_attr(" in body:
            out.append(f"error: {rel}:{line}: cfg_attr の中で lint の段を下げている — clippy::disallowed_methods は "
                       "条件付きで下げられない")
        elif names_group:
            out.append(f"error: {rel}:{line}: clippy::all / clippy::style の段を下げると clippy::disallowed_methods も"
                       "下がる — 下げる lint を名指しする")
        elif not body.startswith(("#[allow(", "#![allow(")):
            out.append(f"error: {rel}:{line}: clippy::disallowed_methods を下げるのは理由付きの #[allow] だけ "
                       f"(expect / warn は不可): {attr[:80]!r}")
    return out



# `#[cfg(test)]` immediately followed by `mod <name> {` (whitespace/comments
# between the two are not handled — rustfmt never inserts either there) —
# used to scope `test-probe-alternative:` to an actually-gated location
# instead of trusting the reason text alone
CFG_TEST_MOD_RE = re.compile(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*mod\s+\w+\s*\{")

# the libm ban in alice-lol/clippy.toml, as method-call, path-call, bare
# path-reference (no call parens — `.fold(acc, f32::max)` passes the
# function itself, not a call) and fully-qualified-call forms; a cheap name
# list (not every entry there), matching the "sin/cos/tan/atan/exp/ln/log/
# powf" family plus the few close relatives already in that file. `max`/
# `min` are deliberately NOT in this list: those are the separate
# raw_minmax reasons, checked by the PREFIXES above, not the libm ban this
# scan is defending against leaking into the same statement.
LIBM_NAMES = (
    "sin_cos", "sin", "cos", "tan", "atan2", "atan", "asin", "acos",
    "exp2", "exp_m1", "exp", "ln_1p", "ln", "log2", "log10", "log",
    "powf", "cbrt", "hypot", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
)
_LIBM_ALT = "|".join(LIBM_NAMES)
LIBM_CALL_RE = re.compile(
    r"\.(?:" + _LIBM_ALT + r")\("  # a.sin(...)
    r"|\b(?:f32|f64)::(?:" + _LIBM_ALT + r")\b"  # f32::sin(...) or bare f32::sin
    r"|<(?:f32|f64)>::(?:" + _LIBM_ALT + r")\("  # <f32>::sin(...)
)

MAX_STATEMENT_SCAN = 3000


def statement_after(text: str, pos: int) -> str:
    """Cheap, non-parsing capture of the statement starting at `pos`: up to
    the next top-level `;` (bracket/paren/brace-depth 0), or the matching
    close of a `{` opened by this scan itself (the attribute sits directly
    on an item like `fn f() { ... }`, not inside an already-open block), or
    an enclosing `}` this scan never opened, reached back at depth 0 (the
    attribute is inside an already-open block, on a semicolon-less tail
    expression) — capped so a malformed file cannot make this scan the rest
    of the file.

    Depth goes negative exactly when a `}` closes a brace this scan did not
    open itself (it belongs to an outer, already-open block): that is the
    "enclosing `}`, no match of our own" case and excludes the brace.
    Depth returning to exactly 0 from 1 means a `{` this scan DID open just
    closed (the item's own body): that is "our own `{ }`" and includes the
    brace, since the item's own braces are part of its statement."""
    depth = 0
    n = min(len(text), pos + MAX_STATEMENT_SCAN)
    i = pos
    while i < n:
        c = text[i]
        if c in "([{":
            depth += 1
        elif c in ")]":
            depth -= 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return text[pos : i + 1]
            if depth < 0:
                return text[pos:i]
        elif c == ";" and depth == 0:
            return text[pos : i + 1]
        i += 1
    return text[pos:n]


def cfg_test_mod_spans(text: str) -> list[tuple[int, int]]:
    """`(start, end)` of every `#[cfg(test)] mod ... { ... }` block's body in
    `text` (brace-matched from the `{` the regex found to its closing `}`).
    A malformed/unbalanced file yields no span for that match rather than
    scanning past EOF."""
    spans = []
    for m in CFG_TEST_MOD_RE.finditer(text):
        depth = 1
        i = m.end()
        n = len(text)
        while i < n and depth > 0:
            if text[i] == "{":
                depth += 1
            elif text[i] == "}":
                depth -= 1
            i += 1
        if depth == 0:
            spans.append((m.end(), i - 1))
    return spans


def is_within(pos: int, spans: list[tuple[int, int]]) -> bool:
    return any(start <= pos < end for start, end in spans)


def iter_rs_files(base: str):
    for dirpath, _dirnames, filenames in os.walk(base):
        for name in sorted(filenames):
            if name.endswith(".rs"):
                yield os.path.join(dirpath, name)


def check(root: str) -> tuple[list[str], dict[str, int]]:
    errors: list[str] = []
    counts = {p: 0 for p in PREFIXES}
    files = 0
    sites = 0
    members = workspace_members(root)
    if not members:
        return ["error: Cargo.toml: [workspace] members を読めなかった"], counts
    src_dirs = [os.path.join(root, member, "src") for member in members]
    missing = [d for d in src_dirs if not os.path.isdir(d)]
    if missing:
        rels = ", ".join(os.path.relpath(d, root) for d in missing)
        return [f"error: {rels}: 読めなかった"], counts

    for base in src_dirs:
        for path in iter_rs_files(base):
            rel = os.path.relpath(path, root).replace("\\", "/")
            with open(path, encoding="utf-8") as f:
                text = f.read()
            files += 1
            test_mod_spans = cfg_test_mod_spans(text)
            file_under_tests_dir = "tests" in rel.split("/")
            errors.extend(level_findings(text, rel))
            for m in ALLOW_RE.finditer(text):
                body = m.group("body")
                if "disallowed_methods" not in body:
                    continue
                rm = REASON_RE.search(body)
                line = text.count("\n", 0, m.start()) + 1
                if rm is None:
                    errors.append(f"error: {rel}:{line}: disallowed_methods の allow に reason が無い")
                    continue
                sites += 1
                text_reason = rm.group("text")
                matched = [p for p in PREFIXES if text_reason.startswith(p)]
                if not matched:
                    errors.append(
                        f"error: {rel}:{line}: reason が既知の prefix で始まっていない "
                        f"({', '.join(PREFIXES)} のどれか) — 内容: {text_reason[:60]!r}"
                    )
                    continue
                prefix = matched[0]

                if prefix == "raw-helper:" and rel != RAW_HELPER_FILE_REL:
                    errors.append(
                        f"error: {rel}:{line}: raw-helper: は {RAW_HELPER_FILE_REL} 以外の file で"
                        "名乗れない (caller 側が同じ prefix を自称しても検査は通さない)"
                    )
                    continue
                if prefix == "cli-report:" and "/src/bin/" not in "/" + rel:
                    errors.append(
                        f"error: {rel}:{line}: cli-report: は src/bin/ 配下の file でしか名乗れない "
                        "(library の code が時計を読む理由にはならない)"
                    )
                    continue
                if prefix == "test-probe-alternative:" and not (
                    file_under_tests_dir or is_within(m.start(), test_mod_spans)
                ):
                    errors.append(
                        f"error: {rel}:{line}: test-probe-alternative: は #[cfg(test)] mod {{ }} の中、"
                        "または tests/ 配下の file でしか名乗れない (この箇所はどちらでもない — "
                        "reason の自己申告だけでは production 混入を防げない)"
                    )
                    continue

                counts[prefix] += 1
                if prefix != "test-probe-alternative:":
                    inner = m.group("inner") == "!"
                    stmt = text[m.end() :] if inner else statement_after(text, m.end())
                    lm = LIBM_CALL_RE.search(stmt)
                    if lm:
                        scope = "enclosing file (inner #![allow])" if inner else "statement"
                        errors.append(
                            f"error: {rel}:{line}: {prefix} の allow が隠す {scope} に libm 呼び出し "
                            f"({lm.group(0)!r}) が含まれる — この箇所は libm 禁止の対象外にならない、"
                            "libm 呼び出しを allow の外に出す"
                        )

    scanned = ", ".join(os.path.relpath(d, root) for d in src_dirs)
    if files == 0:
        errors.append(f"error: {scanned}: file を 1 つも読めなかった — 検査が空振りしている")
    elif sites == 0 and not errors:
        errors.append(f"error: {scanned}: disallowed_methods の allow が 1 件も見つからなかった — 検査が空振りしている")
    return errors, counts


def main(argv: list[str]) -> int:
    root = DEFAULT_ROOT
    if "--root" in argv:
        root = argv[argv.index("--root") + 1]
    errors, counts = check(root)
    for e in errors:
        print(e, file=sys.stderr)
    total = sum(counts.values())
    breakdown = ", ".join(f"{p} {n}" for p, n in counts.items())
    print(f"compared: sites {total} ({breakdown})")
    if errors:
        return 1
    print("disallowed-methods-allow-reasons: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
