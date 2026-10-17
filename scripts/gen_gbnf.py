#!/usr/bin/env python3
"""Generate the LOL grammar from the language definition

source:    alice-lol/language/constructs.toml (one row per construct: name + grammar rule)
template:  alice-lol/language/lol.gbnf.in   (the prose and the fixed rules; `@names <rule>@`
           marks where a rule's name list goes)
generated: alice-lol/lol.gbnf (the canonical grammar, read by `LOL_GBNF`) and every other
           tracked file named `lol.gbnf` (the copies shipped for distribution, found with
           `git ls-files`, so a new copy is covered without editing this script)

A name list is written one name per line, the `|` of each further line under the `::=`:

    name_1f ::= "sphere"
              | "octahedron"

usage: python3 scripts/gen_gbnf.py           write the generated files
       python3 scripts/gen_gbnf.py --check   compare instead (exit 1 on a difference)

Either mode fails when the definition and the template do not agree: no rows, a name twice,
a row whose rule has no place in the template, a `@names` place with no rows, an `intent_*`
row whose rule in the template spells a different name, no purpose values (or none named
`render`), no unit rows, a unit whose `mm` is not a positive float, or a row whose `args` (the kind of each
number argument) has another length than its rule's number count
"""
from __future__ import annotations

import re
import subprocess
import sys
import tomllib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from utf8_stdio import fix_encoding  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
DEFINITION = ROOT / "alice-lol" / "language" / "constructs.toml"
TEMPLATE = ROOT / "alice-lol" / "language" / "lol.gbnf.in"
CANONICAL = ROOT / "alice-lol" / "lol.gbnf"


def outputs() -> list[Path]:
    """the canonical grammar, then every other tracked `lol.gbnf` (sorted)"""
    tracked = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-z", "--", "*lol.gbnf"],
                             capture_output=True, text=True, check=True).stdout.split("\0")
    copies = sorted(ROOT / p for p in tracked if p and (ROOT / p) != CANONICAL)
    return [CANONICAL, *copies]

PLACE = re.compile(r"^@names ([a-z0-9_]+)@$", re.M)
NAME = re.compile(r"^[a-z][a-z0-9_]*$")


class DefinitionError(Exception):
    """the definition and the template do not agree"""


def name_list(rule: str, names: list[str]) -> str:
    pad = " " * (len(rule) + 3)
    first = f'{rule} ::= "{names[0]}"'
    return "\n".join([first] + [f'{pad}| "{n}"' for n in names[1:]])


def rule_numbers(rule: str) -> int | None:
    """how many number arguments a `name_*` rule takes before its children (None: not known here)"""
    m = re.fullmatch(r"name_(\d+)[fk](?:_child|_children|_ab)?", rule)
    if m:
        return int(m.group(1))
    if rule in ("name_k_children",):
        return 1
    if rule in ("name_ab", "name_variadic", "name_child_only"):
        return 0
    return None


def render(definition: dict, template: str) -> str:
    rows = definition.get("construct", [])
    if not rows:
        raise DefinitionError("the definition has no construct rows")
    lists: dict[str, list[str]] = {}
    seen: set[str] = set()
    for row in rows:
        name, rule = row["name"], row["rule"]
        if not NAME.match(name):
            raise DefinitionError(f"construct name {name!r} is not a lower-case identifier")
        if name in seen:
            raise DefinitionError(f"construct {name!r} is defined twice")
        seen.add(name)
        if "args" in row:
            args, count = row["args"], rule_numbers(rule)
            if not re.fullmatch(r"[HLIACRX]*", args) or (count is not None and len(args) != count):
                raise DefinitionError(f"construct {name!r}: args {args!r} do not fit rule {rule} "
                                      f"({count} numbers, kinds H L I A C R X)")
        if rule.startswith("intent_"):
            if not re.search(rf'^{re.escape(rule)} ::= "{re.escape(name)}"', template, re.M):
                raise DefinitionError(f"intent row {name!r}: the template's {rule} does not spell it")
            continue
        lists.setdefault(rule, []).append(name)
    hands = definition.get("hand", [])
    if not hands:
        raise DefinitionError("the definition has no hand values")
    lists["hand"] = list(hands)
    purposes = definition.get("purpose", [])
    if not purposes or "render" not in purposes:
        raise DefinitionError("the definition has no purpose values (render among them)")
    lists["purpose"] = list(purposes)
    units = definition.get("unit", [])
    if not units:
        raise DefinitionError("the definition has no unit rows")
    for u in units:
        if not NAME.match(u["name"]) or u["name"] in seen:
            raise DefinitionError(f"unit name {u['name']!r} is not a lower-case identifier or is taken")
        seen.add(u["name"])
        mm = u.get("mm")
        if mm is not None and not (isinstance(mm, float) and mm > 0):
            raise DefinitionError(f"unit {u['name']!r}: mm must be a positive float")
        lists.setdefault("unit_length" if mm is not None else "unit_render_only", []).append(u["name"])

    places = PLACE.findall(template)
    if len(places) != len(set(places)):
        raise DefinitionError("a @names place appears twice in the template")
    missing = sorted(set(lists) - set(places))
    if missing:
        raise DefinitionError(f"rules with rows but no @names place in the template: {missing}")
    empty = sorted(set(places) - set(lists))
    if empty:
        raise DefinitionError(f"@names places with no rows: {empty}")
    return PLACE.sub(lambda m: name_list(m.group(1), lists[m.group(1)]), template)


def generate() -> str:
    with DEFINITION.open("rb") as f:
        definition = tomllib.load(f)
    return render(definition, TEMPLATE.read_text(encoding="utf-8"))


def main(argv: list[str]) -> int:
    fix_encoding()
    check = "--check" in argv[1:]
    try:
        text = generate()
    except DefinitionError as e:
        print(f"gen_gbnf: {e}", file=sys.stderr)
        return 1
    stale = []
    paths = outputs()
    for path in paths:
        current = path.read_text(encoding="utf-8") if path.exists() else None
        if current == text:
            continue
        if check:
            stale.append(path.relative_to(ROOT))
        else:
            path.write_text(text, encoding="utf-8")
            print(f"wrote {path.relative_to(ROOT)}")
    if stale:
        print(f"gen_gbnf: not generated from the definition: {[str(p) for p in stale]}"
              " (run python3 scripts/gen_gbnf.py)", file=sys.stderr)
        return 1
    with DEFINITION.open("rb") as f:
        n = len(tomllib.load(f)["construct"])
    print(f"gen_gbnf: {n} constructs, {len(paths)} grammar files {'match' if check else 'up to date'}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
