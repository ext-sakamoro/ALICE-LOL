#!/usr/bin/env python3
"""The shell commands a GitHub Actions workflow actually runs (used by readme_sync.py and doc_numbers.py).

`active_runs(text)` returns the `run:` strings of the steps that can run: the workflow has a trigger
(`on:` is not empty), the job and the step have no `if:` that is literally false, and the command
lines of the script are not shell comments. YAML comments are dropped by the parser.

The parser reads the subset of YAML that workflow files use (block mappings, block sequences,
`|` / `>` block scalars, plain / quoted scalars, flow collections kept as text). Anything else
(anchors, aliases, tags, several documents, tabs in indentation, a line it cannot place) raises
`Unsupported`; callers treat that as "no active step", so an unreadable file can only make a gate
stricter, never hide a region.
"""

from __future__ import annotations

import re


class Unsupported(ValueError):
    pass


def _strip_comment(s: str) -> str:
    """a plain-scalar line without its trailing ` # comment` (quotes respected)"""
    q = None
    for i, c in enumerate(s):
        if q:
            if c == q:
                q = None
        elif c in "'\"":
            q = c
        elif c == "#" and (i == 0 or s[i - 1] in " \t"):
            return s[:i].rstrip()
    return s.rstrip()


def _scalar(s: str) -> str:
    s = s.strip()
    if len(s) >= 2 and s[0] == s[-1] == "'":
        return s[1:-1].replace("''", "'")
    if len(s) >= 2 and s[0] == s[-1] == '"':
        return s[1:-1].encode().decode("unicode_escape")
    return s


KEY = re.compile(r"""^((?:"[^"]*"|'[^']*'|[^\s:#'"][^:#]*?))\s*:(?:\s+(.*))?$""")


def parse(text: str):
    if "\t" in "".join(l[: len(l) - len(l.lstrip(" \t"))] for l in text.split("\n")):
        raise Unsupported("tab in indentation")
    lines = text.split("\n")
    pos = 0

    def skip_blank():
        nonlocal pos
        while pos < len(lines) and (not lines[pos].strip() or lines[pos].lstrip().startswith("#")):
            if lines[pos].strip() in ("---", "..."):
                raise Unsupported("document marker")
            pos += 1

    def indent(l: str) -> int:
        return len(l) - len(l.lstrip(" "))

    def block_scalar(head: str, parent: int) -> str:
        nonlocal pos
        if not re.fullmatch(r"[|>][+-]?", head.split(" #")[0].strip()):
            raise Unsupported(f"block scalar header {head!r}")
        body: list[str] = []
        ind = None
        while pos < len(lines):
            l = lines[pos]
            if l.strip() and indent(l) <= parent:
                break
            if l.strip() and ind is None:
                ind = indent(l)
            body.append(l[ind:] if ind is not None and l.strip() else "")
            pos += 1
        while body and not body[-1]:
            body.pop()
        return "\n".join(body) + "\n"

    def value(rest: str | None, parent: int):
        nonlocal pos
        if rest is None or not _strip_comment(rest):
            skip_blank()
            if pos < len(lines) and indent(lines[pos]) > parent:
                return node(indent(lines[pos]))
            if pos < len(lines) and indent(lines[pos]) == parent and lines[pos].lstrip().startswith("- "):
                return node(parent)  # a sequence at the key's own indentation
            return None
        r = rest.strip()
        if r[0] in "|>":
            return block_scalar(r, parent)
        if r[0] in "&*!":
            raise Unsupported(f"anchor / alias / tag {r[:20]!r}")
        return _scalar(_strip_comment(r))

    def node(ind: int):
        nonlocal pos
        skip_blank()
        first = lines[pos]
        if first.lstrip().startswith("- ") or first.strip() == "-":
            out = []
            while True:
                skip_blank()
                if pos >= len(lines) or indent(lines[pos]) != ind or not (
                        lines[pos].lstrip().startswith("- ") or lines[pos].strip() == "-"):
                    break
                item = lines[pos][ind + 2:]
                if KEY.match(_strip_comment(item)):
                    lines[pos] = " " * (ind + 2) + item  # the mapping starts on the dash's line
                    out.append(node(ind + 2))
                else:
                    pos += 1
                    out.append(value(item, ind))
            return out
        out = {}
        while True:
            skip_blank()
            if pos >= len(lines) or indent(lines[pos]) < ind:
                break
            if indent(lines[pos]) > ind:
                raise Unsupported(f"line {pos + 1}: unexpected indentation")
            l = lines[pos].strip()
            if l.startswith("- "):
                break
            m = KEY.match(_strip_comment(l)) or KEY.match(l)
            if not m:
                raise Unsupported(f"line {pos + 1}: not a key")
            pos += 1
            out[_scalar(m.group(1))] = value(m.group(2), ind)
        return out

    skip_blank()
    if pos >= len(lines):
        return {}
    doc = node(0)
    skip_blank()
    if pos < len(lines):
        raise Unsupported(f"line {pos + 1}: not placed")
    return doc


def _false(cond) -> bool:
    return isinstance(cond, str) and re.fullmatch(r"\s*(?:false|\$\{\{\s*false\s*\}\})\s*", cond) is not None


def active_runs(text: str) -> list[str]:
    """the shell lines of the steps that can run (comment lines of the scripts removed); [] when the
    file cannot be read as a workflow"""
    try:
        doc = parse(text)
    except Unsupported:
        return []
    if not isinstance(doc, dict):
        return []
    on = doc.get("on", doc.get(True))
    if on in (None, "", "{}", "[]") or on == {} or on == []:
        return []
    jobs = doc.get("jobs")
    if not isinstance(jobs, dict):
        return []
    runs = []
    for job in jobs.values():
        if not isinstance(job, dict) or _false(job.get("if")):
            continue
        for step in job.get("steps") or []:
            if not isinstance(step, dict) or _false(step.get("if")) or not isinstance(step.get("run"), str):
                continue
            body = "\n".join(l for l in step["run"].split("\n") if not l.lstrip().startswith("#"))
            runs.append(body.replace("\\\n", " "))
    return runs
