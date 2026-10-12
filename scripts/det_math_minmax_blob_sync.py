#!/usr/bin/env python3
"""`alice-lol/src/det_math.rs`'s max/min/max64/min64 bodies match the pinned reference.

`alice-lol` and `alice-physics` each keep their own crate-internal
`max`/`min`/`max64`/`min64` (the std functions leave an equal pair of zeros
unspecified and the targets disagree, see the module doc of
`alice-lol/src/det_math.rs`) — independent implementations of the same IEEE
754-2019 rule, not a shared dependency, so nothing stops one repo's copy from
drifting from the other's silently. This script pins the function bodies as
of the port (`alice-physics` commit `98f104a6`) and fails if LOL's copy
differs from that frozen text, forcing a deliberate re-sync (update both the
pin below and LOL's file in the same commit) instead of a silent edit.

This cannot fetch `alice-physics`'s live file at CI time (no cross-repo
network access in CI) — re-pinning is a manual step when either side's rule
changes; what this guards against is LOL's own file quietly diverging from
what was last verified to match.

Usage: `python3 scripts/det_math_minmax_blob_sync.py` (exit 1 on any mismatch)
`--root DIR` to check a different tree (used by the test)
"""

from __future__ import annotations

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_ROOT = os.path.dirname(HERE)
TARGET_REL = os.path.join("alice-lol", "src", "det_math.rs")

FUNCS = ["max", "min", "max64", "min64"]

# pinned as of alice-physics 98f104a6 (`pub mod det_math { ... }` in its
# src/lib.rs) — signature + body text from `fn <name>(` onward; the leading
# visibility keyword (`pub(crate)` there, since its module is `pub`; `pub`
# here, since LOL's `mod det_math;` is already private) is each repo's own
# module-structure choice, not part of the ported rule, so it is excluded
# from the comparison
PINNED = {
    "max": """fn max(a: f32, b: f32) -> f32 {
    if a.is_nan() || b > a {
        b
    } else if b.is_nan() || a > b || !a.is_sign_negative() {
        a
    } else {
        b
    }
}""",
    "min": """fn min(a: f32, b: f32) -> f32 {
    if a.is_nan() || b < a {
        b
    } else if b.is_nan() || a < b || a.is_sign_negative() {
        a
    } else {
        b
    }
}""",
    "max64": """fn max64(a: f64, b: f64) -> f64 {
    if a.is_nan() || b > a {
        b
    } else if b.is_nan() || a > b || !a.is_sign_negative() {
        a
    } else {
        b
    }
}""",
    "min64": """fn min64(a: f64, b: f64) -> f64 {
    if a.is_nan() || b < a {
        b
    } else if b.is_nan() || a < b || a.is_sign_negative() {
        a
    } else {
        b
    }
}""",
}


def extract_fn(text: str, name: str) -> str | None:
    """The `fn <name>(...) -> ... { ... }` definition, brace-balanced, visibility excluded."""
    m = re.search(rf"\bfn\s+{re.escape(name)}\s*\(", text)
    if not m:
        return None
    start = m.start()
    brace = text.index("{", m.end())
    depth = 1
    p = brace + 1
    n = len(text)
    while p < n and depth > 0:
        if text[p] == "{":
            depth += 1
        elif text[p] == "}":
            depth -= 1
        p += 1
    if depth != 0:
        return None
    return text[start:p]


def check(root: str) -> list[str]:
    path = os.path.join(root, TARGET_REL)
    errors: list[str] = []
    try:
        with open(path, encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return [f"error: {TARGET_REL}: 読めなかった"]

    found = 0
    for name in FUNCS:
        actual = extract_fn(text, name)
        if actual is None:
            errors.append(f"error: {TARGET_REL}: fn {name} が見つからない")
            continue
        found += 1
        if actual != PINNED[name]:
            errors.append(
                f"error: {TARGET_REL}: fn {name} が pin と一致しない "
                "(alice-physics の実体を確認し、意図的な差分なら pin も同じ commit で更新する)"
            )
    if found == 0:
        errors.append(f"error: {TARGET_REL}: {len(FUNCS)} 関数のうち 1 つも見つからなかった — 検査が空振りしている")
    return errors


def main(argv: list[str]) -> int:
    root = DEFAULT_ROOT
    if "--root" in argv:
        root = argv[argv.index("--root") + 1]
    errors = check(root)
    for e in errors:
        print(e, file=sys.stderr)
    print(f"compared: functions {len(FUNCS)}, mismatches {len(errors)}")
    if errors:
        return 1
    print("det-math-minmax-blob-sync: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
