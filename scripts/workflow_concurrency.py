#!/usr/bin/env python3
"""Every workflow run by a push to a branch has a top-level `concurrency` group.

Without one, each push to a branch queues a full run of its own and a superseded run
holds a shared runner until it finishes, delaying every other run in the account.
A workflow whose push trigger names only tags (a release) is exempt, as is one with
no push trigger. Fails when such a workflow lacks the key, and when no workflow was read.

usage: workflow_concurrency.py [workflows dir]
"""
from __future__ import annotations

import re
import sys
from pathlib import Path


def push_to_branches(text: str) -> bool:
    """whether `on:` has a push trigger that is not limited to tags"""
    lines = text.splitlines()
    try:
        start = next(i for i, l in enumerate(lines) if re.match(r"^on:\s*$", l))
    except StopIteration:
        return bool(re.search(r"^on:.*\bpush\b", text, re.M))
    block = []
    for l in lines[start + 1:]:
        if re.match(r"^\S", l):
            break
        block.append(l)
    push = [i for i, l in enumerate(block) if re.match(r"^  push:", l)]
    if not push:
        return False
    body = []
    for l in block[push[0] + 1:]:
        if re.match(r"^  \S", l):
            break
        body.append(l)
    keys = {m.group(1) for l in body if (m := re.match(r"^    ([a-z-]+):", l))}
    return not (keys and keys <= {"tags", "tags-ignore"})


def main(argv: list[str]) -> int:
    root = Path(argv[0]) if argv else Path(__file__).resolve().parent.parent / ".github" / "workflows"
    files = sorted(list(root.glob("*.yml")) + list(root.glob("*.yaml")))
    missing, checked = [], 0
    for f in files:
        text = f.read_text(encoding="utf-8")
        if not push_to_branches(text):
            continue
        checked += 1
        if not re.search(r"^concurrency:", text, re.M):
            missing.append(f"{f.name}: runs on a push to a branch but has no top-level concurrency group")
    for m in missing:
        print(m)
    if not files:
        print("error: no workflow read (compared nothing)", file=sys.stderr)
        return 1
    if missing:
        return 1
    print(f"ok: {checked} of {len(files)} workflows run on a branch push, each has a concurrency group")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
