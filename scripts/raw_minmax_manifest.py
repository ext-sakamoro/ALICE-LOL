#!/usr/bin/env python3
"""Every module authorized to generate `alice-lol/src/raw_minmax.rs`'s raw f32/f64 max/min, named.

Which modules may call the platform `f32`/`f64` `max`/`min` directly is enforced by
the compiler (`clippy::disallowed_macros` on `crate::raw_minmax::raw_minmax_here!()`'s
own definition path; a module invoking it without a reasoned `#![allow(...)]` fails
clippy directly, see `alice-lol/src/raw_minmax.rs`'s module doc) — this script is not
that enforcement. Its job is review/drift tracking the OTHER direction, which the
compiler cannot see on its own:

  * a module invoking the macro that is not yet in `raw_minmax_manifest.toml` (a new
    site to review and add)
  * a listed module whose generated set (`generates = [...]`) differs from what it
    actually invokes (reviewed for fewer helpers than listed, or more than listed)
  * a listed module that no longer invokes the macro at all (a stale entry — the
    exemption it tracked is gone, remove the entry)
  * a module invoking the macro with no `#![allow(clippy::disallowed_macros, ...)]`
    of its own in the same file (clippy itself fails this build already; this script
    catches it too, as a cheaper, non-cargo check, for a checkout mid-edit that has
    not run clippy yet)
  * `alice-lol/src/raw_minmax.rs` itself: its macro's internal `@one` recursive
    self-invocation is the *definition*, not a caller, and must not be counted as one
  * zero files scanned, or the manifest listing zero entries (compared nothing)

Usage: `python3 scripts/raw_minmax_manifest.py`
`--root DIR` to check a different tree (used by the test)
`--write` to regenerate the manifest from the real modules (review the diff)
"""
from __future__ import annotations

import argparse
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_ROOT = os.path.dirname(HERE)
MANIFEST_REL = os.path.join("scripts", "raw_minmax_manifest.toml")
DEFINITION_REL = os.path.join("alice-lol", "src", "raw_minmax.rs")

MEMBERS_RE = re.compile(r'members\s*=\s*\[(?P<body>[^\]]*)\]')
MEMBER_NAME_RE = re.compile(r'"([^"]+)"')

VALID_WHICH = ("max_f32", "min_f32", "max_f64")

# `crate::raw_minmax::raw_minmax_here!(max_f32, min_f32);` (or `use`d and invoked
# bare, or with trailing whitespace/newlines inside the parens) — captures the
# argument list text
INVOCATION_RE = re.compile(r"raw_minmax_here\s*!\s*\((?P<args>[^)]*)\)")
DISALLOWED_MACROS_ALLOW_RE = re.compile(
    r"#!\s*\[\s*allow\s*\(\s*clippy::disallowed_macros\b"
)

TOML_HEADER_RE = re.compile(r'^\["?(?P<file>[^"\]]+)"?\]\s*$')
TOML_GENERATES_RE = re.compile(r'^generates\s*=\s*\[(?P<body>[^\]]*)\]\s*$')
TOML_STR_RE = re.compile(r'"([^"]+)"')


def workspace_members(root: str) -> list[str]:
    path = os.path.join(root, "Cargo.toml")
    try:
        with open(path, encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return []
    m = MEMBERS_RE.search(text)
    if not m:
        return []
    return MEMBER_NAME_RE.findall(m.group("body"))


def iter_rs_files(base: str):
    for dirpath, _dirnames, filenames in os.walk(base):
        for name in filenames:
            if name.endswith(".rs"):
                yield os.path.join(dirpath, name)


def read_manifest(path: str) -> dict[str, list[str]]:
    manifest: dict[str, list[str]] = {}
    current: str | None = None
    try:
        with open(path, encoding="utf-8") as f:
            lines = f.readlines()
    except OSError as e:
        raise RawMinmaxError(f"manifest を読めない ({path}): {e}") from e
    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        m = TOML_HEADER_RE.match(stripped)
        if m:
            current = m.group("file")
            continue
        m = TOML_GENERATES_RE.match(stripped)
        if m and current is not None:
            manifest[current] = sorted(TOML_STR_RE.findall(m.group("body")))
            current = None
    return manifest


class RawMinmaxError(Exception):
    """a manifest or source tree this check cannot read"""


def real_modules(root: str) -> tuple[dict[str, list[str]], list[str]]:
    """(module -> sorted generated-which list, list of modules invoking the
    macro without their own `disallowed_macros` allow in the same file)"""
    members = workspace_members(root)
    if not members:
        raise RawMinmaxError(f"{root}/Cargo.toml に [workspace] members が無い")
    src_dirs = [os.path.join(root, member, "src") for member in members]
    missing = [d for d in src_dirs if not os.path.isdir(d)]
    if missing:
        raise RawMinmaxError(f"member の src/ が無い: {', '.join(missing)}")

    modules: dict[str, list[str]] = {}
    missing_allow: list[str] = []
    definition_abs = os.path.join(root, DEFINITION_REL)
    for base in src_dirs:
        for path in iter_rs_files(base):
            if os.path.abspath(path) == os.path.abspath(definition_abs):
                continue  # the macro's own `@one` recursive call is the definition, not a caller
            with open(path, encoding="utf-8") as f:
                text = f.read()
            invocations = INVOCATION_RE.findall(text)
            if not invocations:
                continue
            which: set[str] = set()
            for args in invocations:
                for tok in args.split(","):
                    tok = tok.strip()
                    if tok and tok != "@one":
                        which.add(tok)
            rel = os.path.relpath(path, root).replace(os.sep, "/")
            modules[rel] = sorted(which)
            if not DISALLOWED_MACROS_ALLOW_RE.search(text):
                missing_allow.append(rel)
    return modules, missing_allow


def check(root: str) -> tuple[list[str], int]:
    manifest = read_manifest(os.path.join(root, MANIFEST_REL))
    if not manifest:
        raise RawMinmaxError("manifest が 0 件 (比較できない)")
    modules, missing_allow = real_modules(root)

    errors: list[str] = []
    for rel, which in sorted(modules.items()):
        unknown = [w for w in which if w not in VALID_WHICH]
        if unknown:
            errors.append(
                f"{rel}: raw_minmax_here!() に未知の引数 {unknown} (有効: {list(VALID_WHICH)})"
            )
        if rel not in manifest:
            errors.append(
                f"{rel}: raw_minmax_here!({', '.join(which)}) を呼んでいるが manifest に entry が無い (新規サイトを足す)"
            )
        elif manifest[rel] != which:
            errors.append(
                f"{rel}: 実際の generates = {which}、manifest は {manifest[rel]} (drift)"
            )
    for rel in sorted(set(manifest) - set(modules)):
        errors.append(f"{rel}: manifest に entry があるが raw_minmax_here!() を呼んでいない (使われなくなった entry、消す)")
    for rel in missing_allow:
        errors.append(
            f"{rel}: raw_minmax_here!() を呼んでいるが #![allow(clippy::disallowed_macros, ...)] が同じ file に無い (clippy も別途 fail する)"
        )
    return errors, sum(len(w) for w in modules.values())


def write_manifest(root: str) -> None:
    modules, _missing_allow = real_modules(root)
    path = os.path.join(root, MANIFEST_REL)
    lines = [
        "# Every module authorized to invoke `crate::raw_minmax::raw_minmax_here!()`",
        "# (alice-lol/src/raw_minmax.rs, the one place the disallowed_methods lint allows",
        "# f32/f64 max/min directly), and which helpers it generates. See",
        "# scripts/raw_minmax_manifest.py.",
        "#",
        "# A listing here is not a budget to spend: a new invocation means either a new",
        "# provably-safe module (add it here, with the same nonzero-const:/both-abs:/",
        "# int-cast: reasoning as alice-lol/src/raw_minmax.rs's module doc requires of",
        "# every caller, and its own #![allow(clippy::disallowed_macros, ...)]) or a site",
        "# that should route through crate::det_math::max/min instead (the Law form, not",
        "# this file).",
        "",
    ]
    for rel in sorted(modules):
        lines.append(f'["{rel}"]')
        generates = ", ".join(f'"{w}"' for w in modules[rel])
        lines.append(f"generates = [{generates}]")
        lines.append("")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines).rstrip() + "\n")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--root", default=DEFAULT_ROOT)
    ap.add_argument("--write", action="store_true")
    args = ap.parse_args()

    if args.write:
        write_manifest(args.root)
        print(f"{MANIFEST_REL}: 再生成した")
        return 0

    try:
        errors, total = check(args.root)
    except RawMinmaxError as e:
        print(f"error: {e}")
        return 2

    modules, _ = real_modules(args.root)
    if errors:
        for e in errors:
            print(f"error: {e}")
        print(f"compared: modules {len(modules)}, generated helpers {total}, mismatches {len(errors)}")
        return 1
    print(f"compared: modules {len(modules)}, generated helpers {total}, mismatches 0")
    print("raw-minmax-manifest: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
