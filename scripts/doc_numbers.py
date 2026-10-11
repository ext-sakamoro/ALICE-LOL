#!/usr/bin/env python3
"""Check that the numbers the documents state are the numbers the code has.

The engine knows nothing about this repository: what to count and where to
look is in a config file (default `scripts/doc_numbers.toml`), so another
repository adopts the gate with its own config and one CI step.

A document states a number in one of three ways, and each has a rule:

  * marked     `<!-- num: KEY -->73`: the number right after the marker is
               recomputed from the source the config gives for KEY and must be
               equal. A marker with an unknown KEY, or with no number after it,
               fails
  * countable  a number the config's `[[countable]]` patterns recognise as a
               count of something in the code ("71 primitives", "Modifiers
               (19)", "テスト 11 件") must carry a marker. An unmarked one fails
               unless `[[allow]]` lists that file and text with a reason
  * measured   a number followed by one of the config's measured units
               (time, throughput, size, ...) must sit in a block (a paragraph,
               list or table: lines up to the next blank line) that carries
               `<!-- measured: YYYY-MM-DD, how -->` (in a table: on the row, or
               above the table for all rows), so the reader knows when
               and with what it was measured (inside a code block, put the
               note on the line right above the fence). A measured number cannot be
               recomputed, so this is the only check it gets. A value the
               config's `bound_before` / `bound_after` patterns mark as a limit
               ("≤ 10ns", "100MB 未満") is a requirement, not a measurement,
               and needs no note

Sources (`[sources."KEY"]`, `kind` = one of):

  * strings_in_block    `file`, `block` (regex, group 1 = the block),
                        optional `item` (regex, default a double-quoted string):
                        the number of items in the block
  * rust_enum_variants  `file`, `name`: the variants of `enum NAME { .. }`
  * rust_test_fns       `file`: the `#[test]` functions
  * glob                `glob` (one pattern or a list): matching tracked-or-not
                        files under the root
  * distinct_matches    `file` (or `files`), `pattern` (group 1): distinct values
  * sum                 `of` (list of KEYs), optional `minus` (list of KEYs)

Every class must compare at least one item, every source must be used by at
least one marker, and a source that finds 0 items fails: a moved file, a
renamed enum or a marker that stopped matching cannot turn the gate into a
no-op. `[measured] none = "reason"` declares a repository without measured
numbers.

Usage: `python scripts/doc_numbers.py [--config FILE] [--root DIR]`
(exit 1 on any finding, 2 on a broken config).
"""

from __future__ import annotations

import fnmatch
import glob as globmod
import os
import re
import sys

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    sys.exit("doc_numbers: Python 3.11+ is required (tomllib)")

MARKER = re.compile(r"<!--\s*num:\s*([A-Za-z0-9_.-]+)\s*-->\s*(\(?\s*)(\d[\d,]*)?")
MEASURED_NOTE = re.compile(r"<!--\s*measured:\s*(\d{4}-\d{2}-\d{2})\s*,\s*([^>]*?\S)\s*-->")


class ConfigError(Exception):
    pass


def read(root: str, rel: str) -> str:
    with open(os.path.join(root, rel), encoding="utf-8") as f:
        return f.read()


# ---------------------------------------------------------------- sources


def rust_enum_variants(src: str, name: str) -> int:
    m = re.search(r"\benum\s+" + re.escape(name) + r"\b[^{;]*\{", src)
    if not m:
        raise ConfigError(f"enum {name} not found")
    depth, i, start = 1, m.end(), m.end()
    while i < len(src) and depth:
        c = src[i]
        if c in "{([":
            depth += 1
        elif c in "})]":
            depth -= 1
        i += 1
    body = src[start:i - 1]
    # drop comments and attributes, then nested groups, then count top-level names
    body = re.sub(r"//[^\n]*", "", body)
    body = re.sub(r"/\*.*?\*/", "", body, flags=re.S)
    body = re.sub(r"#\[[^\]]*\]", "", body)
    flat, depth = [], 0
    for c in body:
        if c in "{([":
            depth += 1
        elif c in "})]":
            depth -= 1
        elif depth == 0:
            flat.append(c)
    names = re.findall(r"(?:^|,)\s*([A-Z][A-Za-z0-9_]*)\s*(?==|,|$)", "".join(flat).strip())
    return len(names)


def compute(key: str, spec: dict, root: str) -> int:
    kind = spec.get("kind")
    if kind == "strings_in_block":
        m = re.search(spec["block"], read(root, spec["file"]), re.S | re.M)
        if not m:
            raise ConfigError(f"{key}: block not found in {spec['file']}")
        return len(re.findall(spec.get("item", r'"(?:[^"\\]|\\.)*"'), m.group(1)))
    if kind == "rust_enum_variants":
        return rust_enum_variants(read(root, spec["file"]), spec["name"])
    if kind == "rust_test_fns":
        return len(re.findall(r"^\s*#\[test\]", read(root, spec["file"]), re.M))
    if kind == "glob":
        pats = spec["glob"] if isinstance(spec["glob"], list) else [spec["glob"]]
        found = set()
        for p in pats:
            found.update(os.path.relpath(f, root) for f in globmod.glob(os.path.join(root, p), recursive=True)
                         if os.path.isfile(f))
        return len(found)
    if kind == "distinct_matches":
        files = spec.get("files") or [spec["file"]]
        vals = set()
        for f in files:
            vals.update(re.findall(spec["pattern"], read(root, f)))
        return len(vals)
    if kind == "sum":
        raise ConfigError(f"{key}: sum is resolved by resolve()")
    raise ConfigError(f"{key}: unknown kind {kind!r}")


def resolve(sources: dict, root: str) -> dict[str, int]:
    vals: dict[str, int] = {}

    def get(key: str, stack: tuple[str, ...] = ()) -> int:
        if key in vals:
            return vals[key]
        if key not in sources:
            raise ConfigError(f"unknown source {key!r}" + (f" (in {stack[-1]})" if stack else ""))
        if key in stack:
            raise ConfigError(f"sum cycle: {' -> '.join(stack + (key,))}")
        spec = sources[key]
        if spec.get("kind") == "sum":
            v = sum(get(k, stack + (key,)) for k in spec["of"]) - sum(get(k, stack + (key,)) for k in spec.get("minus", []))
        else:
            try:
                v = compute(key, spec, root)
            except (OSError, KeyError, re.error) as e:
                raise ConfigError(f"{key}: {type(e).__name__}: {e}") from None
        if v <= 0:
            raise ConfigError(f"{key}: the source found {v} items (moved file, renamed item or wrong pattern?)")
        vals[key] = v
        return v

    for k in sources:
        get(k)
    return vals


# ---------------------------------------------------------------- documents


def doc_files(cfg: dict, root: str) -> list[str]:
    scan = cfg.get("scan", {})
    inc, exc = scan.get("include", ["**/*.md"]), scan.get("exclude", [])
    out = set()
    for p in inc:
        for f in globmod.glob(os.path.join(root, p), recursive=True):
            rel = os.path.relpath(f, root).replace(os.sep, "/")
            if os.path.isfile(f) and not any(fnmatch.fnmatch(rel, e) for e in exc):
                out.add(rel)
    return sorted(out)


FENCE = re.compile(r"^\s*(```|~~~)")


def blocks(lines: list[str]) -> list[tuple[int, int]]:
    """(first, last) line index of each run of non-blank lines; a fenced code block is one run
    with the lines right above it (blank lines inside the fence do not split it)"""
    out, start, fence = [], None, False
    for i, l in enumerate(lines + [""]):
        if FENCE.match(l):
            fence = not fence
        if l.strip() and start is None:
            start = i
        elif not l.strip() and start is not None and not fence:
            out.append((start, i - 1))
            start = None
    if start is not None:
        out.append((start, len(lines) - 1))
    return out


def check(cfg: dict, root: str) -> tuple[list[str], dict[str, int]]:
    errors: list[str] = []
    sources = cfg.get("sources", {})
    if not sources:
        raise ConfigError("no [sources]")
    vals = resolve(sources, root)
    countable = [re.compile(c["pattern"]) for c in cfg.get("countable", [])]
    if not countable:
        raise ConfigError("no [[countable]] patterns")
    for c in countable:
        if "n" not in c.groupindex:
            raise ConfigError(f"countable pattern without a group named n: {c.pattern}")
    meas = cfg.get("measured", {})
    unit_re = re.compile(r"(?<![\w.])(\d[\d,]*(?:\.\d+)?)\s*(?:" + meas["units"] + r")") if meas.get("units") else None
    if unit_re is None and not meas.get("none"):
        raise ConfigError("[measured] needs `units` (or `none = \"reason\"`)")
    bound_before = re.compile(r"(?:" + meas["bound_before"] + r")\s*$") if meas.get("bound_before") else None
    bound_after = re.compile(r"^\s*(?:" + meas["bound_after"] + r")") if meas.get("bound_after") else None
    allow = cfg.get("allow", [])
    for a in allow:
        if not a.get("reason", "").strip():
            raise ConfigError(f"allow entry without a reason: {a}")
    used_allow = [0] * len(allow)
    used_keys: set[str] = set()
    counts = {"docs": 0, "marked": 0, "countable": 0, "measured": 0, "bounds": 0}

    files = doc_files(cfg, root)
    counts["docs"] = len(files)
    for rel in files:
        lines = read(root, rel).split("\n")
        # a note covers its block; in a table, a note on one row covers only that row
        # (rows are often different measurements), a note above the table covers all of it
        noted = set()
        for a, b in blocks(lines):
            rows = [i for i in range(a, b + 1) if lines[i].lstrip().startswith("|")]
            if any(MEASURED_NOTE.search(lines[i]) for i in range(a, b + 1) if i not in rows):
                noted.update(range(a, b + 1))
            noted.update(i for i in rows if MEASURED_NOTE.search(lines[i]))
        for i, line in enumerate(lines):
            where = f"{rel}:{i + 1}"
            marked_at: set[int] = set()
            for m in MARKER.finditer(line):
                key, num = m.group(1), m.group(3)
                counts["marked"] += 1
                used_keys.add(key)
                if key not in vals:
                    errors.append(f"{where}: marker `{key}` has no source in the config")
                elif num is None:
                    errors.append(f"{where}: marker `{key}` is not followed by a number")
                elif int(num.replace(",", "")) != vals[key]:
                    errors.append(f"{where}: `{key}` says {num}, the source has {vals[key]}")
                if num is not None:
                    marked_at.add(m.start(3))
            for c in countable:
                for m in c.finditer(line):
                    counts["countable"] += 1
                    if m.start("n") in marked_at:
                        continue
                    hit = [k for k, a in enumerate(allow) if a["file"] == rel and a["text"] in line and
                           m.group(0) in a["text"]]
                    if hit:
                        for k in hit:
                            used_allow[k] += 1
                        continue
                    errors.append(f"{where}: unmarked count `{m.group(0).strip()}` (put `<!-- num: KEY -->` "
                                  f"before the number, or allow it with a reason)")
            if unit_re:
                for m in unit_re.finditer(line):
                    if (bound_before and bound_before.search(line[:m.start()])) or \
                            (bound_after and bound_after.search(line[m.end():])):
                        counts["bounds"] += 1
                        continue
                    counts["measured"] += 1
                    if i not in noted:
                        errors.append(f"{where}: measured value `{m.group(0).strip()}` without "
                                      f"`<!-- measured: YYYY-MM-DD, how -->` in its block")
    for k in sorted(set(vals) - used_keys):
        errors.append(f"source `{k}` is not used by any marker")
    for k, n in enumerate(used_allow):
        if n == 0:
            errors.append(f"allow entry matched nothing (stale): {allow[k]['file']}: {allow[k]['text']!r}")
    for name, n in counts.items():
        if n == 0 and not (name in ("measured", "bounds") and meas.get("none")):
            errors.append(f"class `{name}` compared nothing")
    return errors, counts


def main(argv: list[str]) -> int:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8")
        except (AttributeError, ValueError):
            pass
    root, config = ".", None
    args = list(argv)
    while args:
        a = args.pop(0)
        if a == "--root":
            root = args.pop(0)
        elif a == "--config":
            config = args.pop(0)
        elif a == "--check":
            pass
        else:
            print(f"doc_numbers: unknown argument {a}", file=sys.stderr)
            return 2
    config = config or os.path.join(root, "scripts", "doc_numbers.toml")
    try:
        with open(config, "rb") as f:
            cfg = tomllib.load(f)
        errors, counts = check(cfg, root)
    except (ConfigError, OSError, tomllib.TOMLDecodeError, re.error, KeyError) as e:
        print(f"doc_numbers: config error: {type(e).__name__}: {e}", file=sys.stderr)
        return 2
    print("doc_numbers compared: " + ", ".join(f"{k} {v}" for k, v in counts.items()))
    for e in errors:
        print(f"  {e}")
    if errors:
        print(f"doc_numbers: {len(errors)} finding(s)", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
