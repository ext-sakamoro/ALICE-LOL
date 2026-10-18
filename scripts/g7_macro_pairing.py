#!/usr/bin/env python3
"""A `lol!` expanded by another alice-lol-macro release does not compile (ALICE-Law G7).

`alice-lol` requires `alice-lol-macro` by a caret range, so a build may pair this
`alice-lol` with any release in that range. Every expansion carries the macro's
identifier (`EXPANSION_ID_HEX` in `alice-lol-macro/src/expansion_id.rs`) and
`alice_lol::__expect_macro_id!` accepts only the one in `alice-lol/src/macro_id.rs`.

This builds two copies of the macro crate under another package name, each used by a
probe binary that depends on this `alice-lol` and calls the copy's `lol!`:

  control  the identifier unchanged: must compile (the setup itself works)
  mutant   another identifier (as a release with a different expansion would carry):
           must fail, with an error that names both identifiers

usage: g7_macro_pairing.py
Exit 0 when both hold, 1 when one does not, 2 when the inputs are missing.
"""
from __future__ import annotations

import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MACRO = ROOT / "alice-lol-macro"
MACRO_ID = MACRO / "src" / "expansion_id.rs"
LOL = ROOT / "alice-lol"
EXPECTED = LOL / "src" / "macro_id.rs"
PROBE_NAME = "alice-lol-macro-g7-probe"

ID_RE = re.compile(r'pub const EXPANSION_ID_HEX: &str =\s*"([0-9a-f]{64})";')
EXPECTED_RE = re.compile(r'hex32\("([0-9a-f]{64})"\)')


def read_ids() -> tuple[str, str] | None:
    if not MACRO_ID.is_file() or not EXPECTED.is_file():
        return None
    macro = ID_RE.findall(MACRO_ID.read_text(encoding="utf-8"))
    expected = EXPECTED_RE.findall(EXPECTED.read_text(encoding="utf-8"))
    if len(macro) != 1 or len(expected) != 1:
        return None
    return macro[0], expected[0]


def copy_macro(dest: Path, ident: str) -> None:
    shutil.copytree(MACRO, dest, ignore=shutil.ignore_patterns("target"))
    manifest = dest / "Cargo.toml"
    text = manifest.read_text(encoding="utf-8")
    text = text.replace('name = "alice-lol-macro"', f'name = "{PROBE_NAME}"', 1)
    manifest.write_text(text, encoding="utf-8")
    src = dest / "src" / "expansion_id.rs"
    src.write_text(ID_RE.sub(f'pub const EXPANSION_ID_HEX: &str = "{ident}";',
                             src.read_text(encoding="utf-8")), encoding="utf-8")


def write_probe(dest: Path, macro_dir: Path) -> None:
    (dest / "src").mkdir(parents=True)
    crate = PROBE_NAME.replace("-", "_")
    (dest / "Cargo.toml").write_text(
        "[package]\n"
        'name = "g7-pairing-probe"\n'
        'version = "0.0.0"\n'
        'edition = "2021"\n'
        "publish = false\n\n"
        "[dependencies]\n"
        f'alice-lol = {{ path = "{LOL.as_posix()}" }}\n'
        f'{PROBE_NAME} = {{ path = "{macro_dir.as_posix()}" }}\n\n'
        "[workspace]\n",
        encoding="utf-8",
    )
    (dest / "src" / "main.rs").write_text(
        "fn main() {\n"
        f"    let node: alice_lol::SdfNode = {crate}::lol! {{ sphere(1.0) }};\n"
        "    assert!(node.node_count() > 0);\n"
        "}\n",
        encoding="utf-8",
    )
    shutil.copyfile(ROOT / "Cargo.lock", dest / "Cargo.lock")


def check(tmp: Path, name: str, ident: str) -> subprocess.CompletedProcess[str]:
    macro_dir = tmp / f"{name}-macro"
    probe_dir = tmp / f"{name}-probe"
    copy_macro(macro_dir, ident)
    write_probe(probe_dir, macro_dir)
    env = dict(os.environ)
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target" / "g7-pairing"))
    return subprocess.run(
        ["cargo", "check", "--quiet", "--manifest-path", str(probe_dir / "Cargo.toml")],
        capture_output=True, text=True, env=env, check=False,
    )


def main(argv: list[str]) -> int:
    if argv:
        print(__doc__.strip().splitlines()[-2], file=sys.stderr)
        return 2
    ids = read_ids()
    if ids is None:
        print("error: could not read one EXPANSION_ID_HEX and one EXPECTED_MACRO_EXPANSION_ID",
              file=sys.stderr)
        return 2
    macro_id, expected_id = ids
    if macro_id != expected_id:
        print(f"error: alice-lol-macro carries {macro_id} but alice-lol expects {expected_id}",
              file=sys.stderr)
        return 1
    other = hashlib.sha256(b"another alice-lol-macro release" + macro_id.encode()).hexdigest()
    failures = []
    with tempfile.TemporaryDirectory() as tmp_name:
        tmp = Path(tmp_name)
        control = check(tmp, "control", macro_id)
        if control.returncode != 0:
            failures.append(f"control (same identifier) does not compile:\n{control.stderr[-3000:]}")
        mutant = check(tmp, "mutant", other)
        if mutant.returncode == 0:
            failures.append("mutant (another identifier) compiled: the comparison did not run")
        else:
            missing = [i for i in (other, expected_id) if i not in mutant.stderr]
            if missing:
                failures.append("mutant failed without naming both identifiers "
                                f"(missing {missing}):\n{mutant.stderr[-3000:]}")
    for f in failures:
        print(f"error: {f}", file=sys.stderr)
    if failures:
        return 1
    print(f"g7 macro pairing: 2 builds; the same identifier compiles, another ({other[:12]}) "
          "is refused naming both")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
