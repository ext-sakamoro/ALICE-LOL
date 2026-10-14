#!/usr/bin/env python3
"""No build of this workspace changes the floating-point semantics (ALICE-Law G14).

`-C target-cpu=native` lets the compiler pick instructions of the build machine (a fused
multiply-add on one machine, separate rounding on another), and fast-math-style flags let
it reassociate or contract floating-point expressions: either makes the same source give
different bits on different machines. This reads every tracked file that can carry build
flags (`Cargo.toml` of every package, `.cargo/config` / `.cargo/config.toml`,
`rust-toolchain(.toml)`, `build.rs`, `.github/workflows/*`, `scripts/preflight.sh`) and
fails on such a flag, and when it read no file at all (a check that compared nothing).

usage: float_build_flags.py [--root DIR]
"""
from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FLAG_RE = re.compile(
    r"target-cpu\s*=\s*[\"']?native"  # -C target-cpu=native, target-cpu = "native"
    r"|-C\s*target-cpu\s+native"
    r"|fast[-_]math",  # -ffast-math, fast-math, fast_math
    re.I,
)


def build_files(root: Path) -> list[str]:
    out = subprocess.run(["git", "-C", str(root), "ls-files", "-z"], capture_output=True, check=True).stdout
    files = []
    for f in out.decode("utf-8", "replace").split("\0"):
        if not f:
            continue
        name = f.rsplit("/", 1)[-1]
        if (name in ("Cargo.toml", "build.rs", "rust-toolchain", "rust-toolchain.toml")
                or f in (".cargo/config", ".cargo/config.toml", "scripts/preflight.sh")
                or f.startswith(".github/workflows/")):
            files.append(f)
    return sorted(files)


def offences(root: Path, files: list[str]) -> list[str]:
    found = []
    for f in files:
        text = (root / f).read_text(encoding="utf-8", errors="replace")
        for n, line in enumerate(text.splitlines(), 1):
            if line.lstrip().startswith("#"):
                continue  # a comment in TOML / YAML / shell
            m = FLAG_RE.search(line)
            if m:
                found.append(f"{f}:{n}: `{m.group(0)}`")
    return found


def main(argv: list[str]) -> int:
    root = Path(argv[argv.index("--root") + 1]) if "--root" in argv else ROOT
    files = build_files(root)
    bad = offences(root, files)
    print(f"float build flags: {len(files)} build files read, {len(bad)} flags that change floating-point semantics")
    for b in bad:
        print(f"error: {b} changes floating-point semantics (ALICE-Law G14)", file=sys.stderr)
    if not files:
        print("error: no build file read (compared nothing)", file=sys.stderr)
        return 2
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
