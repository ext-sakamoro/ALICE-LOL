#!/usr/bin/env python3
"""The boundary manifest of the rule law limits, generated from conformance/rule-limits.toml.

Each entry is a law file text (as parts `[text, count]`, joined in order, so the 4 MiB entries stay
small here), the outcome both readers give (`reads` / `unreadable`) and, for an unreadable one, a text
its error names (the limit, not another fault of the file). For every limit: one below, at and one
past it; for the example's JSON also the forms TASK.md refuses (NaN, Infinity, a lone surrogate) and
the last of a repeated key. alice-lol/tests/rule_law_limits.rs and scripts/test_law_rules.py `Limits`
read the same manifest and require the outcome of every entry. The entries of at most 64 KiB are
also the seeds of the fuzz target fuzz_rule_law_read.

usage: gen_rule_limits.py (--write | --check)
Exit 1 when --check finds the files differ, 2 when the table does not read or nothing was generated.
"""
from __future__ import annotations

import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = ROOT / "conformance" / "rule-limits.toml"
MANIFEST = ROOT / "alice-lol" / "tests" / "data" / "rule_limits.json"
SEEDS = ROOT / "fuzz" / "seeds" / "fuzz_rule_law_read"
SEED_MAX = 64 * 1024

HEAD = ("law limit_probe\nclaim a boundary of the rule law limits (conformance/TASK.md \"Limits of a rule law file\")\n"
        "source scripts/gen_rule_limits.py\n")


def pred(inputs, expr, example):
    lines = "".join(f"x-input {n} {t}\n" for n, t in inputs)
    return [[HEAD + "kind predicate\n" + lines + "require r: ", 1], *expr, [f"\nexample {example}\n", 1]]


def entries(lim):
    out = []

    def add(name, expect, why, parts):
        out.append({"name": name, "expect": expect, "why": why, "parts": parts})

    B, EX = [("a", "bool")], '{"a": true}'
    # size of the file: a statute whose clause text fills it to n bytes
    head = "kind statute\ntitle T\nclause c obligation: "
    for n, expect in ((lim["law_bytes"] - 1, "reads"), (lim["law_bytes"], "reads"), (lim["law_bytes"] + 1, "unreadable")):
        add(f"bytes_{n}", expect, "" if expect == "reads" else f"at most {lim['law_bytes']} bytes",
            [[head, 1], ["x", n - len(head) - 1], ["\n", 1]])
    # lines read: the kind, the title, one clause and n - 3 lines for people (`claim`, read but not
    # interpreted); the comment and the empty line are not counted
    for n, expect in ((lim["law_lines"] - 1, "reads"), (lim["law_lines"], "reads"), (lim["law_lines"] + 1, "unreadable")):
        add(f"lines_{n}", expect, "" if expect == "reads" else f"at most {lim['law_lines']} lines",
            [["kind statute\ntitle T\nclause c obligation: x\n# a comment\n\n", 1], ["claim a line for people\n", n - 3]])
    # tokens of an expression: `a or a or ...` has 2k - 1 tokens, a leading `not` adds one
    t = lim["expression_tokens"]
    k = (t // 2)
    chain = [["a", 1], [" or a", k - 1]]
    add(f"tokens_{2 * k - 1}", "reads", "", pred(B, chain, EX))
    add(f"tokens_{2 * k}", "reads", "", pred(B, [["not ", 1], *chain], EX))
    add(f"tokens_{2 * k + 1}", "unreadable", f"at most {t} tokens", pred(B, [["not not ", 1], *chain], EX))
    # nesting: parentheses, `not` and a call are levels
    d = lim["expression_depth"]
    for n in (d - 1, d, d + 1):
        why = "" if n <= d else f"nests at most {d} levels"
        expect = "reads" if n <= d else "unreadable"
        add(f"parens_{n}", expect, why, pred(B, [["(", n], ["a", 1], [")", n]], EX))
        add(f"not_{n}", expect, why, pred(B, [["not ", n], ["a", 1]], '{"a": %s}' % ("true" if n % 2 == 0 else "false")))
        add(f"call_in_parens_{n}", expect, why,
            pred([("b", "bytes")], [["(", n - 1], ["prefix(b, 1)", 1], [")", n - 1], [" == 0x00", 1]], '{"b": "00"}'))
    # a numeral in the example's JSON (of a key the law does not read)
    c = lim["numeral_chars"]
    for n in (c - 1, c, c + 1, 5000):
        add(f"example_numeral_{n}", "reads" if n <= c else "unreadable", "" if n <= c else f"more than {c} characters",
            pred(B, [["a", 1]], '{"a": true, "z": ' + "1" * n + "}"))
    add(f"example_negative_numeral_{c}", "reads", "", pred(B, [["a", 1]], '{"a": true, "z": -' + "1" * (c - 1) + "}"))
    add(f"example_negative_numeral_{c + 1}", "unreadable", f"more than {c} characters",
        pred(B, [["a", 1]], '{"a": true, "z": -' + "1" * c + "}"))
    # nesting of the example's JSON: the object is level 1, `"z": [[...]]` adds its arrays
    j = lim["json_depth"]
    for n in (j - 1, j, j + 1, 100_000):
        add(f"example_depth_{n}", "reads" if n <= j else "unreadable", "" if n <= j else f"more than {j} levels",
            [[HEAD + "kind predicate\nx-input a bool\nrequire r: a\nexample {\"a\": true, \"z\": ", 1],
             ["[", n - 1], ["]", n - 1], ["}\n", 1]])
    # the forms of JSON TASK.md refuses, and the last of a repeated key
    for name, text in (("nan", 'NaN'), ("infinity", 'Infinity'), ("minus_infinity", '-Infinity'),
                       ("lone_surrogate", '"\\ud800"'), ("lone_low_surrogate", '"\\udc00"')):
        add(f"example_{name}", "unreadable", "example <JSON object>",
            pred(B, [["a", 1]], '{"a": true, "z": ' + text + "}"))
    add("example_surrogate_pair", "reads", "", pred(B, [["a", 1]], '{"a": true, "z": "\\ud83d\\ude00"}'))
    add("example_repeated_key_last", "reads", "", pred(B, [["a", 1]], '{"a": false, "a": true}'))
    add("example_repeated_key_last_fails", "unreadable", "does not hold", pred(B, [["a", 1]], '{"a": true, "a": false}'))
    add("example_large_double_is_infinity", "reads", "", pred(B, [["a", 1]], '{"a": true, "z": 1e400}'))
    return out


def text_of(entry) -> str:
    return "".join(t * c for t, c in entry["parts"])


def render(lim) -> tuple[str, dict[str, bytes]]:
    es = entries(lim)
    manifest = json.dumps({"table": TABLE.relative_to(ROOT).as_posix(), "limits": lim, "entries": es},
                          ensure_ascii=True, indent=1) + "\n"
    seeds = {}
    for e in es:
        t = text_of(e).encode("utf-8")
        if len(t) <= SEED_MAX:
            seeds[e["name"]] = t
    return manifest, seeds


def main(argv=None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    if argv not in (["--write"], ["--check"]):
        print(__doc__.strip().splitlines()[-2], file=sys.stderr)
        return 2
    try:
        lim = tomllib.loads(TABLE.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as e:
        print(f"{TABLE}: {e}", file=sys.stderr)
        return 2
    manifest, seeds = render(lim)
    if not seeds or '"entries": []' in manifest:
        print("nothing generated (compared nothing)", file=sys.stderr)
        return 2
    if argv == ["--write"]:
        MANIFEST.write_text(manifest, encoding="utf-8")
        SEEDS.mkdir(parents=True, exist_ok=True)
        for old in SEEDS.iterdir():
            if old.name not in seeds:
                old.unlink()
        for name, data in seeds.items():
            (SEEDS / name).write_bytes(data)
        print(f"gen_rule_limits: wrote {MANIFEST.relative_to(ROOT)} and {len(seeds)} seed(s)")
        return 0
    problems = []
    if not MANIFEST.is_file() or MANIFEST.read_text(encoding="utf-8") != manifest:
        problems.append(f"{MANIFEST.relative_to(ROOT)} is not what {TABLE.relative_to(ROOT)} generates")
    have = {p.name: p.read_bytes() for p in SEEDS.iterdir()} if SEEDS.is_dir() else {}
    if have != seeds:
        problems.append(f"{SEEDS.relative_to(ROOT)} is not the generated seed set ({len(have)} vs {len(seeds)})")
    for p in problems:
        print(f"gen_rule_limits: {p} (run scripts/gen_rule_limits.py --write)", file=sys.stderr)
    print(f"gen_rule_limits: {len(json.loads(manifest)['entries'])} entries, {len(seeds)} seeds compared")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
