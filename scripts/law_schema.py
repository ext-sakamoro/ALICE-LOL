#!/usr/bin/env python3
"""The type of an `x-input` line, and whether a JSON value matches it.

    type   := "number" | "integer" | "text" | "list of " type | "record(" field ("," field)* ")"
    field  := name ":" ["optional "] type

`optional` lets the field be absent; a field that is present must match its type
(`null` is not absent). A value that does not match is not measured (audit) or
rejected (quantitative law), as a whole: the unit is the input, not the part.

usage: law_schema.py <laws dir>   parses the type of every x-input line (0 lines is a failure)
"""
from __future__ import annotations

import math
import sys
from pathlib import Path


class SchemaError(ValueError):
    pass


def parse(text: str):
    t, rest = _type(text.strip())
    if rest.strip():
        raise SchemaError(f"trailing text after the type: {rest.strip()!r}")
    return t


def _type(s: str):
    s = s.lstrip()
    for word in ("number", "integer", "text"):
        if s.startswith(word) and not s[len(word):len(word) + 1].isalnum():
            return (word,), s[len(word):]
    if s.startswith("list of "):
        inner, rest = _type(s[len("list of "):])
        return ("list", inner), rest
    if s.startswith("record("):
        s = s[len("record("):]
        fields = []
        while True:
            name, sep, s = s.lstrip().partition(":")
            name = name.strip()
            if not sep or not name or not name.replace("-", "_").isidentifier():
                raise SchemaError(f"field name expected, got {name!r}")
            if any(f[0] == name for f in fields):
                raise SchemaError(f"field {name!r} appears twice")
            s = s.lstrip()
            optional = s.startswith("optional ")
            if optional:
                s = s[len("optional "):]
            t, s = _type(s)
            fields.append((name, optional, t))
            s = s.lstrip()
            if s.startswith(","):
                s = s[1:]
                continue
            if s.startswith(")"):
                return ("record", tuple(fields)), s[1:]
            raise SchemaError(f"`,` or `)` expected, got {s[:10]!r}")
    raise SchemaError(f"type expected, got {s[:20]!r}")


def matches(value, t) -> bool:
    kind = t[0]
    if kind in ("number", "integer"):
        if not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value):
            return False
        return kind == "number" or float(value).is_integer()
    if kind == "text":
        return isinstance(value, str)
    if kind == "list":
        return isinstance(value, list) and all(matches(v, t[1]) for v in value)
    if kind == "record":
        if not isinstance(value, dict):
            return False
        for name, optional, ft in t[1]:
            if name not in value:
                if not optional:
                    return False
            elif not matches(value[name], ft):
                return False
        return True
    raise ValueError(kind)


def x_input_types(law_text: str) -> dict:
    out = {}
    for line in law_text.splitlines():
        w = line.split("#", 1)[0].strip()
        if w.startswith("x-input "):
            name, _, text = w[len("x-input "):].partition(" ")
            out[name] = parse(text)
    return out


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    n = 0
    for f in sorted(Path(argv[0]).glob("*.law")):
        try:
            n += len(x_input_types(f.read_text(encoding="utf-8")))
        except SchemaError as e:
            print(f"{f.name}: {e}", file=sys.stderr)
            return 1
    if n == 0:
        print("error: no x-input line parsed (compared nothing)", file=sys.stderr)
        return 1
    print(f"{n} x-input types parsed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
