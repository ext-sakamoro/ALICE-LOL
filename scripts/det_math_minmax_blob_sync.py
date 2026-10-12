#!/usr/bin/env python3
"""Every crate-internal `det_math.rs`'s max/min bodies match the pinned reference.

`alice-physics` and, following it, each LOL-workspace member that needs a
`max`/`min`/`max64`/`min64` of its own (the std functions leave an equal pair
of zeros unspecified and the targets disagree — see the module doc of
`alice-lol/src/det_math.rs`) keep a crate-internal copy: not a shared
dependency, so nothing stops one copy from drifting from the others
silently. This script pins the function bodies as of the port
(`alice-physics` commit `98f104a6`) and fails if any copy differs from that
frozen text, forcing a deliberate re-sync (update both the pin below and the
file in the same commit) instead of a silent edit.

Each target lists only the functions it actually needs (`alice-lol` has all
four; `alice-lol-ui` / `alice-lol-datagen` only call `max`/`min` on `f32`
today; `alice-world-auditor` only calls `max64` on `f64`) — a target is not
required to carry a function it has no site for, but every function it does
carry must match the pin byte-for-byte, and a target file that cannot be
read, or that has none of its expected functions, is a hard failure so the
check is never silently skipped.

This cannot fetch `alice-physics`'s live file at CI time (no cross-repo
network access in CI) — re-pinning is a manual step when either side's rule
changes; what this guards against is one of LOL's own files quietly
diverging from what was last verified to match.

Usage: `python3 scripts/det_math_minmax_blob_sync.py` (exit 1 on any mismatch)
`--root DIR` to check a different tree (used by the test)
"""

from __future__ import annotations

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_ROOT = os.path.dirname(HERE)

# crate -> which of max/min/max64/min64 it is expected to carry
TARGETS = {
    os.path.join("alice-lol", "src", "det_math.rs"): ["max", "min", "max64", "min64"],
    os.path.join("alice-lol-ui", "src", "det_math.rs"): ["max", "min"],
    os.path.join("alice-lol-datagen", "src", "det_math.rs"): ["max", "min"],
    os.path.join("alice-world-auditor", "src", "det_math.rs"): ["max64"],
}

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


def check(root: str) -> tuple[list[str], int]:
    """Errors, and the total number of functions compared across all targets."""
    errors: list[str] = []
    compared = 0
    for target_rel, expected in TARGETS.items():
        path = os.path.join(root, target_rel)
        try:
            with open(path, encoding="utf-8") as f:
                text = f.read()
        except OSError:
            errors.append(f"error: {target_rel}: 読めなかった")
            continue

        found = 0
        for name in expected:
            actual = extract_fn(text, name)
            if actual is None:
                errors.append(f"error: {target_rel}: fn {name} が見つからない")
                continue
            found += 1
            compared += 1
            if actual != PINNED[name]:
                errors.append(
                    f"error: {target_rel}: fn {name} が pin と一致しない "
                    "(alice-physics の実体を確認し、意図的な差分なら pin も同じ commit で更新する)"
                )
        if found == 0:
            errors.append(
                f"error: {target_rel}: 期待する {len(expected)} 関数のうち 1 つも見つからなかった — 検査が空振りしている"
            )
    return errors, compared


def main(argv: list[str]) -> int:
    root = DEFAULT_ROOT
    if "--root" in argv:
        root = argv[argv.index("--root") + 1]
    errors, compared = check(root)
    for e in errors:
        print(e, file=sys.stderr)
    print(f"compared: targets {len(TARGETS)}, functions {compared}, mismatches {len(errors)}")
    if errors:
        return 1
    print("det-math-minmax-blob-sync: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
