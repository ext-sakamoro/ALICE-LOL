#!/usr/bin/env python3
"""Every `#[allow(clippy::disallowed_methods, reason = "...")]` names its pattern.

A per-site exemption from the `disallowed_methods` lint (libm direct calls, or
the `f32`/`f64` `max`/`min` equal-zero tie, see `alice-lol/clippy.toml` and
`.github/clippy-lib/clippy.toml`) is only as good as the proof behind it, and
free-text `reason` strings cannot be checked by a machine. This script
requires every reason to start with one of a fixed set of prefixes, each
naming the specific pattern that makes the exemption safe:

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
    platform-libm forms disagree) — never production code

A reason with none of these prefixes fails the gate (an exemption whose
pattern cannot be named this way is not actually proven safe — fix the call
site or widen the pattern list deliberately, in the same commit, not by
leaving the reason unprefixed). So does a file with `#[allow(dead_code)]`'s
unrelated reason being miscounted here, or zero sites found at all.

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
SRC_DIR = os.path.join("alice-lol", "src")

PREFIXES = ["nonzero-const:", "both-abs:", "int-cast:", "test-probe-alternative:"]

# matches `#[allow(...)]` / `#![allow(...)]` spanning possibly several lines
# (rustfmt wraps long attribute lists), capturing everything between the
# outer parens so a reason inside is found regardless of argument order
ALLOW_RE = re.compile(r"#\s*!?\s*\[\s*allow\s*\((?P<body>.*?)\)\s*\]", re.S)
REASON_RE = re.compile(r'reason\s*=\s*"(?P<text>(?:[^"\\]|\\.)*)"')


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
    base = os.path.join(root, SRC_DIR)
    if not os.path.isdir(base):
        return [f"error: {SRC_DIR}: 読めなかった"], counts

    for path in iter_rs_files(base):
        rel = os.path.relpath(path, root).replace("\\", "/")
        with open(path, encoding="utf-8") as f:
            text = f.read()
        files += 1
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
            counts[matched[0]] += 1

    if files == 0:
        errors.append(f"error: {SRC_DIR}: file を 1 つも読めなかった — 検査が空振りしている")
    elif sites == 0 and not errors:
        errors.append(f"error: {SRC_DIR}: disallowed_methods の allow が 1 件も見つからなかった — 検査が空振りしている")
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
