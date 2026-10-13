#!/usr/bin/env python3
"""G13 (no-unspecified-order) closed static check for
`alice-lol/src/print_export.rs`'s two `HashMap`/`HashSet` locals
(`weld_coincident_vertices`'s `id`, `euler_characteristic`'s `edges`).

This is NOT a check that scans for a `HashMap`/`HashSet` type annotation and
then watches the name it binds: that shape is easy to dodge (an untyped
`let id = HashMap::new();`, a full path `std::collections::HashMap`, a type
alias, a struct field, a helper taking `&HashMap<...>`, ... each still a real
`HashMap`/`HashSet` the compiler accepts, but invisible to a regex anchored on
`: HashMap<`). This check is closed on two things instead, each covering the
file as a whole rather than one recognised declaration shape:

1. **The type.** Every appearance of the literal token `HashMap`, `HashSet`,
   `hash_map`, or `hash_set` anywhere in the file must be on the exact,
   pinned `use std::collections::{HashMap, HashSet};` import line, or on
   exactly one of the two pinned declaration lines below. Nowhere else in
   the file may this type (or its module) be named at all -- not in a
   second `let`, a type alias, a struct field, a function parameter, a full
   path, or a *different* `use` line, including an `as`-renamed import
   (`use std::collections::HashMap as Hm;`): that import line's own text
   does not match the one pinned line, so it fails rule 1 itself, and the
   `Hm` it introduces is never tracked by rule 2 either -- the two together
   are why pinning the import's exact text (not "any `use` statement") is
   load-bearing, not merely tidy.
2. **The two pinned names, `id` and `edges`.** Once their declaration line is
   found (the one and only place `PINNED_NAMES` may bind a `HashMap`/
   `HashSet`), every *other* appearance of that identifier in the file must
   be immediately followed by `.` and one of a short whitelist of lookup
   methods (`insert`, `contains`/`contains_key`, `get`/`get_mut`/
   `get_key_value`, `entry`, `len`, `is_empty`, `with_capacity`, `capacity`,
   `remove`, `clear`, `reserve`, `shrink_to_fit`) -- none of which exposes
   iteration order. A bare `&id`, `id` passed as a function argument,
   `IntoIterator::into_iter(&id)` (bypassing `.into_iter()` by calling the
   trait method directly), an alias (`let a = &id;`), or a chained call to
   anything not on the whitelist, is a violation.

Renaming either pinned local, even to something equally well-behaved, is
*also* a violation: rule 1 only recognises `id`/`edges` as "the" pinned
declaration by name, so a renamed local's `HashMap`/`HashSet` token falls
outside both pinned lines and fails rule 1. That is deliberate -- the check
is pinned to these two names today, not to "whichever local holds the map";
widening it is a conscious edit to `PINNED_NAMES` plus this module doc, not
something a rename should silently pass.

What this check does NOT claim: it says nothing about a `HashMap`/`HashSet`
anywhere else in this crate (only this one file). And it is a *textual*
check: a map obtained from another crate's own API, with no `HashMap`/
`HashSet` token anywhere in this file at all (`let m = alice_sdf::x(); for
_ in &m {}`), has nothing for either rule to find -- a residual class rules
1 and 2 cannot see by construction, not an oversight either could be
patched to close. `tests/print_export_determinism.rs` is the backstop for
exactly that class: it does not read this file's source at all, it diffs
the actual STL/3MF bytes two separate processes under different
`RAYON_NUM_THREADS` produce from the same scene, so it would still catch an
order dependency introduced through a map this script never sees (it
caught the hand-planted "decide output order from the map's own iteration"
mutant used to verify both checks -- the 3MF bytes differed between runs,
STL's did not; see that test's own module doc for why: STL writes each
triangle's three vertices in full and never shares them, so a `HashMap`-
bucket-order change to vertex numbering does not reach STL bytes at all,
only 3MF's shared, indexed vertex list).

It also fails when it compared nothing: the target file does not exist, or
it declares zero pinned `HashMap`/`HashSet` locals (the file this script
watches changed shape -- either is cause to look again, not to pass
silently).

Usage: `python3 scripts/print_export_no_hashmap_iteration.py`
"""

from __future__ import annotations

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)

TARGET_REL = os.path.join("alice-lol", "src", "print_export.rs")

# the only two locals this check recognises as a legitimate HashMap/HashSet
PINNED_NAMES = ("id", "edges")

# a pinned name immediately followed by `.<one of these>(` is a lookup, not an
# iteration; nothing in this list exposes a HashMap/HashSet's bucket order
LOOKUP_METHODS = frozenset(
    (
        "insert",
        "contains",
        "contains_key",
        "get",
        "get_mut",
        "get_key_value",
        "entry",
        "len",
        "is_empty",
        "with_capacity",
        "capacity",
        "remove",
        "clear",
        "reserve",
        "shrink_to_fit",
    )
)

TYPE_TOKEN_RE = re.compile(r"\bHash(?:Map|Set)\b|\bhash_(?:map|set)\b")

DECL_RE = re.compile(
    r"\blet\s+(?:mut\s+)?(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*:\s*Hash(?:Map|Set)\s*<"
)

# the only `use` line allowed to name HashMap/HashSet: pinned to its exact
# text (ignoring only leading/trailing whitespace) -- an `as`-renamed import,
# a second use statement, or a glob naming the collections module all differ
# from this text and so are NOT this line, however much they resemble it
CANONICAL_IMPORT_LINE = "use std::collections::{HashMap, HashSet};"

CALL_AFTER_RE = re.compile(r"\s*\.\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(")


def line_of(text: str, pos: int) -> int:
    return text.count("\n", 0, pos) + 1


def line_text(lines: list[str], lineno: int) -> str:
    return lines[lineno - 1].strip() if 1 <= lineno <= len(lines) else ""


def strip_comments_and_strings(text: str) -> str:
    """Blank out `//`/`/* */` comment bodies and string-literal contents with
    spaces, keeping every newline (and the string literal's own quotes) in
    place so a position in the result names the same line/column as in
    `text`. Plain-English prose in a doc comment ("...6177 non-manifold
    edges") must not look like a reference to the `edges` local to code that
    scans for word-boundary identifiers -- this is what keeps the two apart.
    Does not special-case raw strings/byte strings/char literals: none are
    used in print_export.rs today (checked by grep before writing this)."""
    out: list[str] = []
    i, n = 0, len(text)
    while i < n:
        two = text[i : i + 2]
        if two == "//":
            j = i
            while j < n and text[j] != "\n":
                out.append(" ")
                j += 1
            i = j
            continue
        if two == "/*":
            depth = 1
            out.append("  ")
            j = i + 2
            while j < n and depth > 0:
                pair = text[j : j + 2]
                if pair == "/*":
                    depth += 1
                    out.append("  ")
                    j += 2
                elif pair == "*/":
                    depth -= 1
                    out.append("  ")
                    j += 2
                else:
                    out.append("\n" if text[j] == "\n" else " ")
                    j += 1
            i = j
            continue
        if text[i] == '"':
            out.append('"')
            j = i + 1
            while j < n:
                if text[j] == "\\" and j + 1 < n:
                    out.append("  ")
                    j += 2
                    continue
                if text[j] == '"':
                    out.append('"')
                    j += 1
                    break
                out.append("\n" if text[j] == "\n" else " ")
                j += 1
            i = j
            continue
        out.append(text[i])
        i += 1
    return "".join(out)


def find_pinned_declarations(text: str) -> dict[str, re.Match[str]]:
    """PINNED_NAMES name -> its `let name: Hash(Map|Set)<` declaration match
    (first one found, by name); a name not declared this way is simply
    absent (checked by the caller, not here)."""
    found: dict[str, re.Match[str]] = {}
    for m in DECL_RE.finditer(text):
        name = m.group("name")
        if name in PINNED_NAMES and name not in found:
            found[name] = m
    return found


def scan(text: str) -> tuple[list[str], list[str]]:
    """(pinned names whose declaration was found, violation descriptions).

    All pattern matching runs against `text` with comments and string
    literals blanked out (`strip_comments_and_strings`); error messages still
    quote the real line from `text` itself."""
    code = strip_comments_and_strings(text)
    decls = find_pinned_declarations(code)
    lines = text.splitlines()
    violations: list[str] = []

    # Rule 1: the HashMap/HashSet type token appears only on the one pinned
    # import line (matched by exact text, not "any use statement") and on
    # exactly the pinned declarations' own lines -- nowhere else.
    pinned_decl_lines = {line_of(code, m.start()) for m in decls.values()}
    for lineno, code_line in enumerate(code.splitlines(), start=1):
        if not TYPE_TOKEN_RE.search(code_line):
            continue
        if code_line.strip() == CANONICAL_IMPORT_LINE or lineno in pinned_decl_lines:
            continue
        violations.append(
            f"line {lineno}: HashMap/HashSet named outside the pinned import "
            f"line and the pinned declarations: {line_text(lines, lineno)}"
        )

    # Rule 2: every other appearance of a pinned name is an immediate
    # whitelisted lookup call -- nothing else (reference, argument, alias,
    # trait-method-call-by-path, or a non-whitelisted method).
    for name, decl_match in decls.items():
        binding_span = decl_match.span("name")
        name_re = re.compile(r"\b" + re.escape(name) + r"\b")
        for m in name_re.finditer(code):
            if m.span() == binding_span:
                continue
            lineno = line_of(code, m.start())
            call = CALL_AFTER_RE.match(code, m.end())
            if not call:
                violations.append(
                    f"line {lineno}: `{name}` used without an immediate "
                    f"whitelisted lookup call: {line_text(lines, lineno)}"
                )
                continue
            method = call.group(1)
            if method not in LOOKUP_METHODS:
                violations.append(
                    f"line {lineno}: `{name}.{method}(...)` is not a "
                    f"whitelisted lookup method: {line_text(lines, lineno)}"
                )

    return sorted(decls), violations


def main(argv: list[str]) -> int:
    del argv
    target = os.path.join(ROOT, TARGET_REL)
    if not os.path.isfile(target):
        print(f"error: {TARGET_REL} does not exist (compared nothing)", file=sys.stderr)
        return 1
    with open(target, encoding="utf-8") as f:
        text = f.read()
    names, violations = scan(text)
    if not names:
        print(
            f"error: {TARGET_REL} declares zero pinned HashMap/HashSet locals "
            f"({'/'.join(PINNED_NAMES)}) any more (compared nothing -- the "
            "mechanism this check watches is gone or renamed)",
            file=sys.stderr,
        )
        return 1
    for v in violations:
        print(f"error: {TARGET_REL}: {v}", file=sys.stderr)
    if violations:
        return 1
    print(
        f"print_export_no_hashmap_iteration: {len(names)} pinned local(s) "
        f"({', '.join(names)}) in {TARGET_REL}, HashMap/HashSet named nowhere "
        "else, every other reference is a whitelisted lookup"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
