#!/usr/bin/env python3
"""The `clippy::iter_over_hash_type` lint fires on a `for` loop over a HashMap / HashSet (ALICE-Law G10).

The library clippy runs with `-D clippy::iter_over_hash_type`: a `for` loop over a HashMap or HashSet
visits the entries in the order of their RandomState seed, which differs per run. This runs only that
lint on scripts/clippy-lib-canary and requires a diagnostic on every line of the canary that carries
the marker `// for-over-hash` and on no other line, so the flag cannot be dropped from the library
clippy without the canary noticing that nothing fires (a lint whose name is mistyped is reported as
unknown and fails too).

What the lint does not see, and what covers it: the iteration methods (`iter`, `keys`, `values`,
`drain`, `into_keys`, ... of HashMap / HashSet) are disallowed methods of the library-only clippy
configuration (scripts/clippy_lib_gate.toml); an explicit `.into_iter()` on a HashMap is neither a
`for` loop nor a listed method, and a HashMap reached through a generic `IntoIterator` bound is
not seen by either.

    python3 scripts/hash_iter_canary.py
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
CANARY = HERE / "clippy-lib-canary"
MARK = "// for-over-hash"
# (clippy prints the path with the platform's separator: src/lib.rs, or src\\lib.rs on Windows)
DIAG = re.compile(r"^\s*--> src[\\/]lib\.rs:(\d+):\d+", re.M)


def main() -> int:
    src = (CANARY / "src" / "lib.rs").read_text(encoding="utf-8").splitlines()
    marked = {i + 1 for i, line in enumerate(src) if MARK in line}
    if not marked:
        print(f"hash-iter-canary: no line of the canary carries {MARK!r} (compared nothing)", file=sys.stderr)
        return 2
    p = subprocess.run(["cargo", "clippy", "--quiet", "--manifest-path", str(CANARY / "Cargo.toml"), "--",
                        "-A", "clippy::all", "-A", "clippy::disallowed_methods", "-A", "clippy::disallowed_types",
                        "-D", "unknown_lints", "-D", "clippy::iter_over_hash_type"],
                       capture_output=True, text=True, env={**__import__("os").environ, "CARGO_TERM_COLOR": "never"})
    out = p.stdout + p.stderr
    flagged: set[int] = set()
    for block in out.split("\n\n"):
        if "iter_over_hash_type" in block or "iterating over unordered hash-based" in block:
            flagged |= {int(n) for n in DIAG.findall(block)}
    if "unknown lint" in out:
        print(f"hash-iter-canary: clippy does not know the lint:\n{out}", file=sys.stderr)
        return 1
    missing, extra = sorted(marked - flagged), sorted(flagged - marked)
    for n in missing:
        print(f"error: src/lib.rs:{n}: a for loop over a hash type without an iter_over_hash_type diagnostic",
              file=sys.stderr)
    for n in extra:
        print(f"error: src/lib.rs:{n}: an iter_over_hash_type diagnostic on a line without {MARK!r}", file=sys.stderr)
    if p.returncode == 0:
        print("error: clippy exited 0 (the lint did not deny anything)", file=sys.stderr)
    print(f"hash-iter-canary: {len(marked)} marked line(s), {len(flagged)} flagged")
    return 1 if missing or extra or p.returncode == 0 else 0


if __name__ == "__main__":
    sys.exit(main())
