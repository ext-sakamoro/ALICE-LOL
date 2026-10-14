#!/usr/bin/env python3
"""No mesh export is tracked at the repository root.

Examples that export meshes write under the system temp dir or a path given on the command
line; four exports (crown.obj, royal_crown.obj, gem.fbx, shield.fbx) were once committed at
the root by examples that wrote to the current directory. This check reads the tracked
files of the root (`git ls-files`, not the working tree) and fails when one has a mesh
export extension, and when it read no file at all (a check that compared nothing).

usage: root_artifacts.py [--root DIR]
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MESH_EXTENSIONS = (".obj", ".fbx", ".stl", ".3mf")


def root_files(root: Path) -> list[str]:
    out = subprocess.run(["git", "-C", str(root), "ls-files", "-z"], capture_output=True, check=True).stdout
    return [p for p in out.decode("utf-8", "replace").split("\0") if p and "/" not in p]


def offenders(files: list[str]) -> list[str]:
    return sorted(f for f in files if f.lower().endswith(MESH_EXTENSIONS))


def main(argv: list[str]) -> int:
    root = Path(argv[argv.index("--root") + 1]) if "--root" in argv else ROOT
    files = root_files(root)
    bad = offenders(files)
    print(f"root artifacts: {len(files)} tracked root files read, {len(bad)} mesh exports")
    for f in bad:
        print(f"error: {f} is a mesh export tracked at the repository root (write it under the temp dir or a given path)", file=sys.stderr)
    if not files:
        print("error: no tracked root file read (compared nothing)", file=sys.stderr)
        return 2
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
