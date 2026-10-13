#!/usr/bin/env python3
"""No platform libm call hides under a disallowed_methods allow (type-resolved, not regex).

`scripts/disallowed_methods_allow_reasons.py` checks the allow reason's *text*: that it
starts with a known prefix, and (by a cheap statement-boundary scan) that the covered
statement does not also mention a libm method/path name. A regex is defeatable in ways
a real clippy run is not (a reason whose known-prefix text happens to sit in a comment
or a string, an attribute-scope edge case the scan heuristic gets wrong, ...).

This script asks the compiler instead: it temporarily neutralises every
`clippy::disallowed_methods` mention (function-level and module-level `#![allow]` alike)
in this workspace's own tracked `.rs` files under `src/` (not `tests/`, and not
`#[cfg(test)]` code — `cargo clippy --lib` never compiles that), reruns clippy with the
library-code configuration CI and preflight already use
(`CLIPPY_CONF_DIR=.github/clippy-lib`), and requires **zero** `disallowed_methods`
diagnostics whose message names a libm function (`sin`/`cos`/`tan`/`atan`/`exp`/`ln`/
`log`/`powf`/... — see `alice-lol/clippy.toml`'s `disallowed-methods` list) anywhere in
that scope. `f32`/`f64` `max`/`min` diagnostics are deliberately NOT judged here: those
are `scripts/disallowed_methods_allow_reasons.py`'s `nonzero-const:`/`both-abs:`/
`int-cast:`/`raw-helper:` classes, which have real, individually-reviewed exemptions
scattered outside `raw_minmax.rs` (e.g. `alice-lol-datagen`) that this blanket check
would otherwise misjudge as "hidden".

It also fails when it compared nothing: no allow was neutralised, or the rewritten
tree produced no `disallowed_methods` diagnostic at all (every allow exists *because*
there is a real forbidden call under it somewhere — lifting it all must show at least
one, or this check is not testing anything).

The rewrite happens in a throwaway copy of the working tree (every tracked file, with
the working-copy contents) under the build directory, never in the checked-out files:
a run that is killed part way leaves the repository untouched, and at worst a stale
copy under `target/` that the next run replaces. The copy sits next to symlinks of the
sibling checkouts, so the `../ALICE-*` path dependencies resolve the same way.

Usage: `python3 scripts/libm_hidden_check.py`
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

LINT = "clippy::disallowed_methods"
STAND_IN = "clippy::disallowed_types"
CLIPPY_CONF_DIR = ".github/clippy-lib"

LIBM_NAMES = (
    "sin_cos", "sin", "cos", "tan", "atan2", "atan", "asin", "acos",
    "exp2", "exp_m1", "exp", "ln_1p", "ln", "log2", "log10", "log",
    "powf", "cbrt", "hypot", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
)
LIBM_MSG_RE = re.compile(
    r"`(?:f32|f64)::(?:" + "|".join(LIBM_NAMES) + r")`"
)

CLIPPY_INVOCATIONS = [
    ["cargo", "clippy", "--message-format=json", "--quiet", "-p", "alice-lol", "--lib", "--all-features"],
    [
        "cargo", "clippy", "--message-format=json", "--quiet",
        "--workspace", "--exclude", "alice-lol", "--lib", "--bins", "--examples", "--all-features",
    ],
]


def tracked_files(root: str) -> list[str]:
    out = subprocess.run(
        ["git", "-C", root, "ls-files", "-z"], check=True, capture_output=True,
    ).stdout
    return sorted({p for p in out.decode().split("\0") if p})


def is_src_rs(rel: str) -> bool:
    parts = rel.split("/")
    return rel.endswith(".rs") and "src" in parts[:-1]


def make_copy(root: str, parent: str) -> str:
    """Copy the working tree's tracked files into `parent/<repo name>` and symlink
    every sibling directory of `root` next to it. Returns the copy's root."""
    name = os.path.basename(os.path.abspath(root))
    copy_root = os.path.join(parent, name)
    for rel in tracked_files(root):
        src = os.path.join(root, rel)
        if not os.path.isfile(src):
            continue  # deleted in the working copy
        dst = os.path.join(copy_root, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copy2(src, dst)
    siblings = os.path.dirname(os.path.abspath(root))
    for entry in sorted(os.listdir(siblings)):
        full = os.path.join(siblings, entry)
        if entry != name and os.path.isdir(full) and not os.path.exists(os.path.join(parent, entry)):
            os.symlink(full, os.path.join(parent, entry))
    return copy_root


def neutralise_tree(copy_root: str) -> int:
    """Neutralise the lint in every tracked src/ .rs file of the copy; returns the count."""
    count = 0
    for dirpath, dirnames, filenames in os.walk(copy_root):
        dirnames[:] = [d for d in dirnames if d not in ("target", ".git")]
        for fname in filenames:
            path = os.path.join(dirpath, fname)
            rel = os.path.relpath(path, copy_root).replace(os.sep, "/")
            if not is_src_rs(rel):
                continue
            with open(path, encoding="utf-8") as f:
                text = f.read()
            new_text, n = neutralise(text)
            if n:
                with open(path, "w", encoding="utf-8") as f:
                    f.write(new_text)
                count += n
    return count


def neutralise(text: str) -> tuple[str, int]:
    return text.replace(LINT, STAND_IN), text.count(LINT)


def run_clippy(cmd: list[str], cwd: str, target_dir: str) -> list[dict]:
    env = dict(
        os.environ,
        CLIPPY_CONF_DIR=CLIPPY_CONF_DIR,
        CARGO_TERM_COLOR="never",
        CARGO_TARGET_DIR=target_dir,
    )
    r = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, check=False)
    messages = []
    for line in r.stdout.splitlines():
        try:
            m = json.loads(line)
        except json.JSONDecodeError:
            continue
        if m.get("reason") == "compiler-message":
            messages.append(m["message"])
    return messages


def judge(messages: list[dict]) -> tuple[list[str], int]:
    """(libm diagnostics found, total disallowed_methods diagnostics seen)"""
    libm_hits, seen = [], 0
    for msg in messages:
        if (msg.get("code") or {}).get("code") != LINT:
            continue
        seen += 1
        if LIBM_MSG_RE.search(msg.get("message", "")):
            spans = [s for s in msg.get("spans", []) if s.get("is_primary")]
            where = f"{spans[0]['file_name']}:{spans[0]['line_start']}" if spans else "?"
            libm_hits.append(f"{where}: {msg.get('message', '')}")
    return libm_hits, seen


def main(argv: list[str]) -> int:
    del argv
    target_dir = os.environ.get("CARGO_TARGET_DIR") or os.path.join(ROOT, "target")
    work = os.path.join(target_dir, "libm-hidden-check")
    os.makedirs(work, exist_ok=True)
    parent = tempfile.mkdtemp(prefix="tree-", dir=work)
    try:
        copy_root = make_copy(ROOT, parent)
        rewritten = neutralise_tree(copy_root)
        if rewritten == 0:
            print(f"error: no {LINT} allow under src/ was found (compared nothing)", file=sys.stderr)
            return 1
        all_libm_hits: list[str] = []
        total_seen = 0
        for cmd in CLIPPY_INVOCATIONS:
            libm_hits, seen = judge(run_clippy(cmd, copy_root, os.path.join(work, "target")))
            all_libm_hits.extend(libm_hits)
            total_seen += seen
    finally:
        shutil.rmtree(parent, ignore_errors=True)

    if total_seen == 0:
        print(
            f"error: {rewritten} allow(s) neutralised but clippy reported no {LINT} at all "
            "(compared nothing)",
            file=sys.stderr,
        )
        return 1
    for hit in all_libm_hits:
        print(f"error: libm call hidden under an allow: {hit}", file=sys.stderr)
    if all_libm_hits:
        return 1
    print(
        f"libm hidden check: {rewritten} allow(s) neutralised, {total_seen} "
        f"{LINT} diagnostic(s) total, 0 libm diagnostics outside src/ scope"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
