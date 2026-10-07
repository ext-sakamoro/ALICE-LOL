#!/usr/bin/env python3
"""Check, from a saved `cargo test` log, that every law oracle target ran tests.

CI already runs the whole test suite once; running the law targets again just
to count them would repeat the slowest oracle (`law_corpus_oracle`). This script
reads the log of that run instead and fails when a listed target is missing
from it or ran zero tests. The target list is the one in scripts/law_tests.sh
(`DEFAULT_TARGETS` / `PHYSICS_TARGETS`), so the two cannot drift apart.

usage: scripts/law_tests_from_log.py <default|physics|all> <cargo-test.log>
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ANSI = re.compile(r"\x1b\[[0-9;]*m")
RUNNING = re.compile(r"^\s*Running (?:unittests )?(\S+) \(.*?deps[/\\]([A-Za-z0-9_]+)-[0-9a-f]+")
RESULT = re.compile(r"^test result: \w+\. (\d+) passed")
TEST_LINE = re.compile(r"^test (\S+) \.\.\. ok")


def targets(mode: str) -> list[tuple[str, str]]:
    text = (ROOT / "scripts/law_tests.sh").read_text(encoding="utf-8")
    groups = {}
    for name in ("DEFAULT_TARGETS", "PHYSICS_TARGETS"):
        body = re.search(name + r"=\(\n(.*?)\n\)", text, re.S)
        if not body:
            raise SystemExit(f"error: {name} not found in scripts/law_tests.sh")
        groups[name] = [tuple(line.strip().strip('"').split("|")[:2]) for line in body.group(1).splitlines() if line.strip()]
    if mode == "default":
        return groups["DEFAULT_TARGETS"]
    if mode == "physics":
        return groups["PHYSICS_TARGETS"]
    if mode == "all":
        return groups["DEFAULT_TARGETS"] + groups["PHYSICS_TARGETS"]
    raise SystemExit("usage: law_tests_from_log.py <default|physics|all> <log>")


def sections(log: str) -> list[tuple[str, str, list[str]]]:
    """(source path, crate name, lines) for each test binary in the log"""
    out: list[tuple[str, str, list[str]]] = []
    for raw in log.splitlines():
        line = ANSI.sub("", raw)
        m = RUNNING.match(line)
        if m:
            out.append((m.group(1).replace("\\", "/"), m.group(2), []))
        elif out:
            out[-1][2].append(line)
    return out


def passed(lines: list[str]) -> int:
    return sum(int(m.group(1)) for line in lines if (m := RESULT.match(line)))


def main() -> int:
    if len(sys.argv) != 3:
        raise SystemExit("usage: law_tests_from_log.py <default|physics|all> <log>")
    wanted = targets(sys.argv[1])
    secs = sections(Path(sys.argv[2]).read_text(encoding="utf-8", errors="replace"))
    if not secs:
        print("error: no test binaries found in the log")
        return 1
    errors = []
    for pkg, args in wanted:
        crate = pkg.replace("-", "_")
        if args.startswith("--test "):
            name = args.split()[1]
            hits = [s for s in secs if s[0] == f"tests/{name}.rs" and s[1] == name]
            n = sum(passed(s[2]) for s in hits)
            label = f"-p {pkg} {args}"
        elif args.startswith("--lib"):
            prefix = args.split()[1] if len(args.split()) > 1 else ""
            hits = [s for s in secs if s[0].endswith("src/lib.rs") and s[1] == crate]
            n = sum(1 for s in hits for line in s[2] if (m := TEST_LINE.match(line)) and m.group(1).startswith(prefix))
            label = f"-p {pkg} --lib {prefix}"
        else:
            errors.append(f"unsupported target arguments: {args}")
            continue
        if not hits:
            errors.append(f"{label}: not in the log (target missing or not built)")
        elif n == 0:
            errors.append(f"{label}: ran zero tests")
        else:
            print(f"ok: {label}: {n} passed")
    for e in errors:
        print(f"error: {e}")
    print(f"law targets checked: {len(wanted)}")
    return 1 if errors or not wanted else 0


if __name__ == "__main__":
    sys.exit(main())
