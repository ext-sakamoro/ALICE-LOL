#!/usr/bin/env python3
"""Check that the numbers the documents state are the numbers the code has.

The engine knows nothing about this repository: what to count, which nouns
name a count and where to look is in a config file (default
`scripts/doc_numbers.toml`), so another repository adopts the gate with its own
config and one CI step.

A document states a number in one of these ways, and each has a rule:

  * marked     `<!-- num: KEY -->73`: the number right after the marker is
               recomputed from the source the config gives for KEY and must be
               equal. The marker must also stand next to the noun the source
               counts (`noun` in the source; "next to" = the text between the
               neighbouring markers on the line), so a marker with the wrong
               key cannot pass because the two numbers happen to be equal. A
               marker with an unknown KEY, or with no number after it, fails
  * countable  a number the config's `[countable]` nouns and counters make a
               count must carry a marker. The engine recognises these forms
               (NOUN = `nouns`, CTR = `counters`):
                 `71 primitives`, `128 exhaustive SdfNode variants` (two words between),
                 `71-primitive`, `Primitives (71)`, `Primitives (71,`,
                 `Primitives: 71`, `tests: **123**`, `(124 total)`, a table row
                 whose first cell names a NOUN and whose cell is a bare number,
                 `71 個の primitive`, `primitive は 71 種類`, `テスト 7件`, `5種`,
                 and a count written in words (`seventy-one primitives`, `a dozen
                 primitives`, `五種`),
                 which must be written in digits with a marker. A number after
                 `ignore_before` (an ordinal: "Phase 2 Law") or below `min`
                 (default 2: "1 件" reads as a singular) is not a count. An unmarked count fails unless
                 `[[allow]]` names its file and a text of its line with a reason,
                 or its block carries `<!-- snapshot: YYYY-MM-DD, what -->`
                 (counts that record the past, not claims about now)
  * measured   a number followed by one of the config's measured units
               (time, throughput, size, ...) must sit in a block (a paragraph,
               list, table or fenced code block with the lines right above it)
               that carries `<!-- measured: YYYY-MM-DD, how -->` (in a table: on
               the row, or above the table for all rows), so the reader knows
               when and with what it was measured
  * bound      a measured value or a count the config's `bound_before` /
               `bound_after` patterns mark as a limit ("≤ 10ns", "100MB 未満",
               "200+") is a requirement, not a statement, and needs neither

Sources (`[sources."KEY"]`, every source has `noun` (regex) and `kind`):

  * strings_in_block    `file`, `block` (regex, group 1 = the block),
                        optional `item` (regex, default a double-quoted string):
                        the number of items in the block
  * rust_enum_variants  `file`, `name`: the variants of `enum NAME { .. }`
  * rust_test_fns       `file`: the `#[test]` functions
  * glob                `glob` (one pattern or a list): tracked files that match
  * distinct_matches    `file` (or `files`), `pattern` (group 1): distinct values
  * table_rows          `file`, `heading` (regex of a heading line): the body
                        rows of the first table after that heading
  * sum                 `of` (list of KEYs), optional `minus` (list of KEYs)

`[[complete]] rows = KEY, of = KEY` states that a table lists every item: the
two counts must be equal.

`[generated]` (optional) names a block another gate writes and checks: `begin` and `end`
(regexes, each matching a whole line) and `checked_by = { file, pattern }` (a tracked file,
usually a workflow, whose text matches `pattern`). The lines from the begin line to the end
line are not read by this gate, but only when the file has exactly one begin line and one end
line after it and `checked_by` holds; otherwise the markers are a finding and every line is
read, so a missing end marker cannot hide the rest of the file and a block nothing checks
cannot hide its numbers.

Every class must compare at least one item unless `[not_applicable]` gives the
class a reason, every source must be used by at least one marker, every allow
entry must match, and a source that finds 0 items fails: a moved file, a
renamed enum or a marker that stopped matching cannot turn the gate into a
no-op.

Usage: `python scripts/doc_numbers.py [--config FILE] [--root DIR]`
(exit 1 on any finding, 2 on a broken config).
"""

from __future__ import annotations

import fnmatch
import glob as globmod
import os
import re
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    sys.exit("doc_numbers: Python 3.11+ is required (tomllib)")

MARKER = re.compile(r"<!--\s*num:\s*([A-Za-z0-9_.-]+)\s*-->\s*(\(?\s*)(\d[\d,]*)?")
NOTE = re.compile(r"<!--\s*(measured|snapshot):\s*(\d{4}-\d{2}-\d{2})\s*,\s*([^>]*?\S)\s*-->")
FENCE = re.compile(r"^\s*(```|~~~)")
CONNECTIVES = {"of", "and", "with", "plus", "in", "the", "total", "or", "out", "all", "と", "の", "うち"}
CLASSES = ("docs", "marked", "countable", "measured", "bounds", "history")

# "one" and a lone 一 read as a singular ("adds one intent tree"), not as a count
EN_UNITS = ("two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen|"
            "sixteen|seventeen|eighteen|nineteen")
EN_TENS = "twenty|thirty|forty|fifty|sixty|seventy|eighty|ninety"
EN_NUMBER = rf"(?:(?:{EN_TENS})(?:-(?:one|{EN_UNITS}))?|{EN_UNITS}|hundreds?|(?:a\s+)?dozens?(?:\s+of)?|a\s+score\s+of)"
KANJI_NUMBER = "(?:[二三四五六七八九十百千]|一(?=[一二三四五六七八九十百千]))[一二三四五六七八九十百千]*"


class ConfigError(Exception):
    pass


def read(root: str, rel: str) -> str:
    with open(os.path.join(root, rel), encoding="utf-8") as f:
        return f.read()


def tracked(root: str) -> list[str]:
    p = subprocess.run(["git", "ls-files", "-z"], cwd=root, capture_output=True)
    if p.returncode != 0:
        raise ConfigError(f"git ls-files failed in {root}: {p.stderr.decode(errors='replace').strip()}")
    return [f for f in p.stdout.decode("utf-8").split("\0") if f]


def generated_span(lines: list[str], begin: re.Pattern, end: re.Pattern) -> tuple[range | None, str | None]:
    """the 0-based lines of the one generated block (markers included), or a finding"""
    b = [i for i, l in enumerate(lines) if begin.fullmatch(l.strip())]
    e = [i for i, l in enumerate(lines) if end.fullmatch(l.strip())]
    if not b and not e:
        return None, None
    if len(b) == 1 and len(e) == 1 and b[0] < e[0]:
        return range(b[0], e[0] + 1), None
    at = ", ".join(str(i + 1) for i in sorted(b + e))
    return None, f"generated-block markers on lines {at}: expected one begin line and one end line after it"


def generated_rule(cfg: dict, root: str, files: list[str]) -> tuple[re.Pattern, re.Pattern, str | None] | None:
    """the begin and end patterns of [generated], and why the block is not checked (None = it is)"""
    g = cfg.get("generated")
    if g is None:
        return None
    cb = g.get("checked_by", {})
    if not g.get("begin") or not g.get("end") or not cb.get("file") or not cb.get("pattern"):
        raise ConfigError("[generated] needs `begin`, `end` and `checked_by = { file, pattern }`")
    why = None
    if cb["file"] not in files:
        why = f"{cb['file']} is not tracked"
    elif not re.search(cb["pattern"], read(root, cb["file"])):
        why = f"{cb['file']} does not match `{cb['pattern']}`"
    return re.compile(g["begin"]), re.compile(g["end"]), why


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


def table_rows(text: str, heading: str) -> int:
    lines = text.split("\n")
    head = re.compile(heading)
    for i, l in enumerate(lines):
        if head.search(l):
            j = i + 1
            while j < len(lines) and not lines[j].lstrip().startswith("|"):
                if lines[j].lstrip().startswith("#"):
                    raise ConfigError(f"no table under the heading {heading!r}")
                j += 1
            rows = 0
            k = j
            while k < len(lines) and lines[k].lstrip().startswith("|"):
                if k > j + 1:  # header and separator are not rows
                    rows += 1
                k += 1
            return rows
    raise ConfigError(f"heading {heading!r} not found")


def compute(key: str, spec: dict, root: str, files: list[str]) -> int:
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
        return len({f for f in files for p in pats if fnmatch.fnmatchcase(f, p)})
    if kind == "distinct_matches":
        vals = set()
        for f in spec.get("files") or [spec["file"]]:
            vals.update(re.findall(spec["pattern"], read(root, f)))
        return len(vals)
    if kind == "table_rows":
        return table_rows(read(root, spec["file"]), spec["heading"])
    raise ConfigError(f"{key}: unknown kind {kind!r}")


def resolve(sources: dict, root: str, files: list[str]) -> dict[str, int]:
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
                v = compute(key, spec, root, files)
            except (OSError, KeyError, re.error) as e:
                raise ConfigError(f"{key}: {type(e).__name__}: {e}") from None
        if v <= 0:
            raise ConfigError(f"{key}: the source found {v} items (moved file, renamed item or wrong pattern?)")
        vals[key] = v
        return v

    for k, spec in sources.items():
        if not str(spec.get("noun", "")).strip():
            raise ConfigError(f"{k}: every source needs `noun` (what a marker for it must stand next to)")
        get(k)
    return vals


# ---------------------------------------------------------------- documents


def doc_files(cfg: dict, files: list[str]) -> list[str]:
    scan = cfg.get("scan", {})
    inc, exc = scan.get("include", ["**/*.md"]), scan.get("exclude", [])

    def match(rel: str, pat: str) -> bool:
        # `**/` also matches nothing, so "**/*.md" covers top-level files
        return fnmatch.fnmatchcase(rel, pat) or (pat.startswith("**/") and fnmatch.fnmatchcase(rel, pat[3:]))

    return sorted(f for f in files if any(match(f, p) for p in inc) and not any(match(f, e) for e in exc))


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


def countable_patterns(cfg: dict) -> list[re.Pattern]:
    c = cfg.get("countable", {})
    nouns, counters = c.get("nouns", ""), c.get("counters", "")
    if not nouns or not counters:
        raise ConfigError("[countable] needs `nouns` and `counters`")
    num = r"(?P<n>\d+(?:,\d{3})*)"
    mark = r"(?:<!--[^>]*-->\s*)?"
    word = r"[^\s|()\[\]:,;*`-][^\s|()\[\]:,;*`]*"
    lead = r"(?<![A-Za-z0-9_.,/#-])"  # ASCII only: 「は5種」 has a Japanese letter before the number
    pats = [
        # 71 primitives / 128 SdfNode variants / 128 exhaustive SdfNode variants (up to two words between)
        rf"{lead}{num}\+?\s+(?:{word}\s+){{0,2}}?(?:{nouns})(?![\w-])",
        # 71-primitive / 124-construct
        rf"{lead}{num}-(?:{nouns})(?![\w-])",
        # Primitives (71) / Primitives (71, / Primitives: 71 / tests: **123**
        rf"(?<![\w-])(?:{nouns})\s*(?:\(|:|：)\s*{mark}\**{num}(?![\w.])",
        # (124 total)
        rf"\(\s*{mark}{num}\s+total\b",
        # 71 個の primitive / 3 件のテスト
        rf"{lead}{num}\s*(?:{counters})\s*の\s*(?:{word}\s*)?(?:{nouns})",
        # primitive は 71 種類 / テスト 7件
        rf"(?<![\w-])(?:{nouns})\s*(?:は|が|を|も)?\s*{mark}{num}\s*(?:{counters})",
        # 5種 (a number of kinds needs no noun to be a count)
        rf"{lead}{num}\s*種類?",
    ]
    pats += [p for p in c.get("extra", [])]
    out = []
    for p in pats:
        r = re.compile(p, re.I)
        if "n" not in r.groupindex:
            raise ConfigError(f"countable pattern without a group named n: {p}")
        out.append(r)
    return out


def word_numbers(cfg: dict) -> list[re.Pattern]:
    c = cfg["countable"]
    nouns, counters = c["nouns"], c["counters"]
    return [re.compile(rf"\b{EN_NUMBER}\s+(?:[^\s|()\[\]:,;]+\s+){{0,2}}?(?:{nouns})(?![\w-])", re.I),
            re.compile(rf"{KANJI_NUMBER}\s*(?:{counters})")]


def table_cells(line: str, nouns: str) -> list[tuple[int, str]]:
    """(offset, number) of bare-number cells in a table row whose first cell is a countable noun"""
    if not line.lstrip().startswith("|"):
        return []
    cells, out, pos = [], [], line.index("|") + 1
    for part in line[pos:].split("|"):
        cells.append((pos, part))
        pos += len(part) + 1
    if not cells:
        return []
    label = re.sub(r"[`*_]", "", cells[0][1]).strip()
    # the noun anywhere in the label, as a whole ASCII word ("Latest" is not "test"); a CJK noun may be
    # followed by more CJK ("テスト数")
    if len(label.split()) > 6 or not re.search(rf"(?<![A-Za-z])(?:{nouns})(?![a-z])", label, re.I):
        return []
    for off, text in cells[1:]:
        m = re.fullmatch(r"\s*(?:<!--[^>]*-->\s*)?\**(\d+(?:,\d{3})*)\+?\**\s*", text)
        if m:
            out.append((off + m.start(1), m.group(1)))
    return out


def check(cfg: dict, root: str) -> tuple[list[str], dict[str, int]]:
    errors: list[str] = []
    sources = cfg.get("sources", {})
    if not sources:
        raise ConfigError("no [sources]")
    files = tracked(root)
    vals = resolve(sources, root, files)
    nouns = cfg.get("countable", {}).get("nouns", "")
    countable = countable_patterns(cfg)
    ib = cfg["countable"].get("ignore_before")
    ignore_before = re.compile(rf"(?:{ib})\s*$", re.I) if ib else None
    min_count = int(cfg["countable"].get("min", 2))
    in_words = word_numbers(cfg)
    meas = cfg.get("measured", {})
    if not meas.get("units"):
        raise ConfigError("[measured] needs `units` (declare the class not applicable in [not_applicable] instead)")
    unit_re = re.compile(r"(?<![\w.])(\d[\d,]*(?:\.\d+)?)\s*(?:" + meas["units"] + r")")
    bound_before = re.compile(r"(?:" + meas["bound_before"] + r")\s*$") if meas.get("bound_before") else None
    bound_after = re.compile(r"^\s*(?:" + meas["bound_after"] + r")") if meas.get("bound_after") else None
    na = cfg.get("not_applicable", {})
    for k, why in na.items():
        if k not in CLASSES or not str(why).strip():
            raise ConfigError(f"[not_applicable] {k}: a known class with a reason is required")
    allow = cfg.get("allow", [])
    for a in allow:
        if not a.get("reason", "").strip() or not a.get("text", "").strip():
            raise ConfigError(f"allow entry without a text or a reason: {a}")
    for c in cfg.get("complete", []):
        if c.get("rows") not in vals or c.get("of") not in vals:
            raise ConfigError(f"[[complete]] names an unknown source: {c}")
        if vals[c["rows"]] != vals[c["of"]]:
            errors.append(f"table `{c['rows']}` has {vals[c['rows']]} rows but claims to list all "
                          f"{vals[c['of']]} of `{c['of']}`")
    used_allow = [0] * len(allow)
    used_keys: set[str] = set()
    counts = dict.fromkeys(CLASSES, 0)

    def bounded(line: str, start: int, end: int) -> bool:
        return bool((bound_before and bound_before.search(line[:start])) or
                    (bound_after and bound_after.search(line[end:])))

    docs = doc_files(cfg, files)
    counts["docs"] = len(docs)
    rule = generated_rule(cfg, root, files)
    for rel in docs:
        lines = read(root, rel).split("\n")
        skip: range = range(0)
        if rule is not None:
            span, err = generated_span(lines, rule[0], rule[1])
            if err:
                errors.append(f"{rel}: {err}")
            elif span is not None and rule[2] is not None:
                errors.append(f"{rel}: has a generated block, but nothing checks it ({rule[2]})")
            elif span is not None:
                skip = span
        measured_ok, history = set(), set()
        for a, b in blocks(lines):
            rows = {i for i in range(a, b + 1) if lines[i].lstrip().startswith("|")}
            for kind, cover in (("measured", measured_ok), ("snapshot", history)):
                if any(m.group(1) == kind for i in range(a, b + 1) if i not in rows for m in NOTE.finditer(lines[i])):
                    cover.update(range(a, b + 1))
                cover.update(i for i in rows if any(m.group(1) == kind for m in NOTE.finditer(lines[i])))
        for i, line in enumerate(lines):
            if i in skip:
                continue  # written and checked by the gate [generated] names
            where = f"{rel}:{i + 1}"
            markers = list(MARKER.finditer(line))
            marked_at: set[int] = set()
            for j, m in enumerate(markers):
                key, num = m.group(1), m.group(3)
                counts["marked"] += 1
                used_keys.add(key)
                if num is not None:
                    marked_at.add(m.start(3))
                if key not in vals:
                    errors.append(f"{where}: marker `{key}` has no source in the config")
                    continue
                if num is None:
                    errors.append(f"{where}: marker `{key}` is not followed by a number")
                    continue
                if int(num.replace(",", "")) != vals[key]:
                    errors.append(f"{where}: `{key}` says {num}, the source has {vals[key]}")
                hi = markers[j + 1].start() if j + 1 < len(markers) else len(line)
                # the text between the neighbouring markers; when it holds only connectives
                # ("Primitives (28 of 73"), the noun before the earlier marker is the one it shares
                k = j
                while True:
                    lo = markers[k - 1].end() if k else 0
                    seg = line[lo:m.start()] + " " + line[m.end():hi]
                    words = {w.lower() for w in re.findall(r"[^\W\d_]+", MARKER.sub(" ", seg))} - CONNECTIVES
                    if words or k == 0:
                        break
                    k -= 1
                context = MARKER.sub(" ", line[lo:m.start()] + " " + line[m.end():hi])
                if not re.search(sources[key]["noun"], context, re.I):
                    errors.append(f"{where}: marker `{key}` is not next to what it counts "
                                  f"(`{sources[key]['noun']}` not in {context.strip()[:60]!r})")
            hits: dict[int, str] = {}
            for c in countable:
                for m in c.finditer(line):
                    hits.setdefault(m.start("n"), m.group(0).strip())
            for off, n in table_cells(line, nouns):
                hits.setdefault(off, f"| … | {n} |")
            for off, text in sorted(hits.items()):
                num_end = off + len(re.match(r"\d+(?:,\d{3})*", line[off:]).group(0))
                if ignore_before and ignore_before.search(line[:off]):
                    continue  # an ordinal ("Phase 2 Law"), not a count
                if int(line[off:num_end].replace(",", "")) < min_count:
                    continue  # a singular ("1 件も比べていない"), not a count
                counts["countable"] += 1
                if off in marked_at:
                    continue
                if bounded(line, off, num_end):
                    counts["bounds"] += 1
                    continue
                if i in history:
                    counts["history"] += 1
                    continue
                hit = [k for k, a in enumerate(allow) if a["file"] == rel and a["text"] in line and
                       a["text"].find(line[off:num_end]) >= 0]
                if hit:
                    for k in hit:
                        used_allow[k] += 1
                    continue
                errors.append(f"{where}: unmarked count `{text}` (put `<!-- num: KEY -->` before the number, "
                              f"mark the block as a snapshot, or allow it with a reason)")
            for w in in_words:
                for m in w.finditer(line):
                    counts["countable"] += 1
                    if i in history:
                        counts["history"] += 1
                        continue
                    if any(a["file"] == rel and a["text"] in line and m.group(0) in a["text"]
                           for a in allow):
                        for k, a in enumerate(allow):
                            if a["file"] == rel and a["text"] in line and m.group(0) in a["text"]:
                                used_allow[k] += 1
                        continue
                    errors.append(f"{where}: count in words `{m.group(0).strip()}` (write it in digits "
                                  f"with a marker)")
            for m in unit_re.finditer(line):
                if bounded(line, m.start(), m.end()):
                    counts["bounds"] += 1
                    continue
                counts["measured"] += 1
                if i not in measured_ok:
                    errors.append(f"{where}: measured value `{m.group(0).strip()}` without "
                                  f"`<!-- measured: YYYY-MM-DD, how -->` in its block")
    for k in sorted(set(vals) - used_keys):
        refs = [c for c in cfg.get("complete", []) if k in (c.get("rows"), c.get("of"))]
        if not refs:
            errors.append(f"source `{k}` is not used by any marker")
    for k, n in enumerate(used_allow):
        if n == 0:
            errors.append(f"allow entry matched nothing (stale): {allow[k]['file']}: {allow[k]['text']!r}")
    for name, n in counts.items():
        if n == 0 and name not in na:
            errors.append(f"class `{name}` compared nothing (if it cannot occur here, give the reason in "
                          f"[not_applicable])")
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
