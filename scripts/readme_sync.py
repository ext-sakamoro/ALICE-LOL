#!/usr/bin/env python3
"""Check that README.md and README_JP.md agree with the code.

The READMEs avoid version numbers (badges read crates.io, installation uses
`cargo add`). What they still state is compared with its source here, so a
change to the parser, the law types or the manifest that the READMEs do not
follow fails CI instead of drifting:

  * syntax    each block after `<!-- readme-sync: syntax-<group> -->` lists
              exactly the names of that group in alice-lol/src/syntax_table.rs
              (`SDF_SYNTAX`, and `INTENT_SYNTAX` for `syntax-intent`); every
              group has a block and no name is listed twice. A Rust test keeps
              that table equal to the names the runtime parser dispatches on
  * macro     the names the `lol!` macro (alice-lol-macro/src/parser.rs)
              dispatches on are the SDF groups except `stdlib` and
              `RUNTIME_ONLY`, so the README's statement about the macro holds
  * laws      the table after `<!-- readme-sync: laws -->` lists exactly the
              variants of `law::Constraint`, and each row's evidence cell names
              the class `Constraint::evidence_class` returns for it
  * verdicts  the table after `<!-- readme-sync: research-verdicts -->` lists
              exactly the variants of `research_law::ResearchVerdict`
  * features  the table after `<!-- readme-sync: features -->` lists exactly
              the `[features]` of alice-lol/Cargo.toml (minus `default`)
  * msrv      every line carrying `<!-- readme-sync: msrv -->` states
              `rust-version` as `**X.Y**`
  * version   any `alice-lol = "X"` dependency line is caret-compatible with
              the package version (zero such lines is fine)
  * example   the first ```rust block of each README is the ```rust block of
              the crate documentation in alice-lol/src/lib.rs (a doctest)
  * links     every relative link in both READMEs points at a file that exists
  * sections  README_JP.md has the same number of `##` sections as README.md

Every check must compare at least one item; a check that compared nothing
fails, so a renamed marker or table cannot turn the gate into a no-op.

Usage: `python scripts/readme_sync.py --check` (exit 1 on any mismatch).
`--root DIR` runs against another tree (used by scripts/test_readme_sync.py).
"""

from __future__ import annotations

import os
import re
import sys

READMES = ("README.md", "README_JP.md")
CRATE_TOML = "alice-lol/Cargo.toml"
LIB_RS = "alice-lol/src/lib.rs"
SYNTAX_RS = "alice-lol/src/syntax_table.rs"
MACRO_RS = "alice-lol-macro/src/parser.rs"
LAW_RS = "alice-lol/src/law.rs"
RESEARCH_RS = "alice-lol/src/research_law.rs"
EVIDENCE = ("Proved", "Witnessed", "Modelled")


def read(root: str, rel: str) -> str:
    with open(os.path.join(root, rel), encoding="utf-8") as f:
        return f.read()


def strings(text: str) -> list[str]:
    return re.findall(r'"([^"\\]*)"', text)


# ── sources ────────────────────────────────────────────────────────────


def syntax_table(src: str) -> tuple[list[tuple[str, list[str]]], list[str], list[str]]:
    """(SDF groups in order, intent verbs, runtime-only names) from syntax_table.rs."""
    sdf = re.search(r"^(?:pub(?:\([a-z]+\))? )?const SDF_SYNTAX[^=]*=\s*&\[(.*?)^\];", src, re.M | re.S)
    groups = []
    if sdf:
        for m in re.finditer(r'\(\s*"([a-z0-9_]+)"\s*,\s*&\[(.*?)\]\s*,?\s*\)', sdf.group(1), re.S):
            groups.append((m.group(1), strings(m.group(2))))
    intent = re.search(r"^(?:pub(?:\([a-z]+\))? )?const INTENT_SYNTAX[^=]*=\s*&\[(.*?)\];", src, re.M | re.S)
    only = re.search(r"^(?:pub(?:\([a-z]+\))? )?const RUNTIME_ONLY[^=]*=\s*&\[(.*?)\];", src, re.M | re.S)
    return groups, strings(intent.group(1)) if intent else [], strings(only.group(1)) if only else []


def macro_names(src: str) -> list[str]:
    names = []
    for line in src.splitlines():
        m = re.match(r'\s*("[a-z0-9_]+"(?:\s*\|\s*"[a-z0-9_]+")*)\s*=>', line)
        if m:
            names += strings(m.group(1))
    return names


def enum_variants(src: str, name: str) -> list[str]:
    """Variant names of `pub enum <name>` (4-space indented, `Name {` / `Name,` / `Name(`)."""
    m = re.search(rf"^pub enum {name}\b[^{{]*\{{\n(.*?)^\}}", src, re.M | re.S)
    if not m:
        return []
    return re.findall(r"^    ([A-Z][A-Za-z0-9]*)\s*(?:\{|,|\()", m.group(1), re.M)


def evidence_classes(src: str) -> dict[str, str]:
    """variant -> evidence class, from the `match` of `Constraint::evidence_class`."""
    m = re.search(r"fn evidence_class\(&self\) -> Evidence \{(.*?)^    \}", src, re.M | re.S)
    out: dict[str, str] = {}
    if not m:
        return out
    pending: list[str] = []
    for line in m.group(1).splitlines():
        pending += re.findall(r"Self::([A-Z][A-Za-z0-9]*)", line)
        c = re.search(r"=>\s*Evidence::([A-Z][a-z]+)", line)
        if c:
            for v in pending:
                out[v] = c.group(1)
            pending = []
    return out


def cargo_package(toml: str) -> dict[str, str]:
    pkg = re.search(r"^\[package\]\s*$(.*?)^\[", toml, re.M | re.S)
    body = pkg.group(1) if pkg else ""
    out = {}
    for key in ("version", "rust-version"):
        m = re.search(rf'^{key}\s*=\s*"([^"]+)"', body, re.M)
        if m:
            out[key] = m.group(1)
    return out


def cargo_features(toml: str) -> set[str]:
    sec = re.search(r"^\[features\]\s*$(.*?)(?=^\[|\Z)", toml, re.M | re.S)
    if not sec:
        return set()
    names = re.findall(r"^([A-Za-z0-9_-]+)\s*=", sec.group(1), re.M)
    return {n for n in names if n != "default"}


def lib_doc(lib: str) -> str:
    lines = []
    for line in lib.splitlines():
        s = line.strip()
        if s.startswith("//!"):
            lines.append(s[4:] if s.startswith("//! ") else s[3:])
    return "\n".join(lines) + "\n"


# ── README parsing ─────────────────────────────────────────────────────


def table_after(text: str, marker: str) -> list[str] | None:
    i = text.find(marker)
    if i < 0:
        return None
    rows = []
    started = False
    for line in text[i + len(marker):].splitlines():
        if line.startswith("|"):
            started = True
            rows.append(line)
        elif started:
            break
    return rows


def cells(row: str) -> list[str]:
    return [c.strip() for c in row.strip().strip("|").split("|")]


def first_cell_names(rows: list[str]) -> list[str]:
    names = []
    for r in rows:
        m = re.match(r"\|\s*`([^`]+)`[^|]*\|", r)  # `name` or `name` (note)
        if m:
            names.append(m.group(1))
    return names


def block_after(text: str, marker: str) -> list[str] | None:
    """Whitespace-separated names of the fenced block that follows the marker."""
    i = text.find(marker)
    if i < 0:
        return None
    m = re.match(r"\s*```[^\n]*\n(.*?)^```", text[i + len(marker):], re.M | re.S)
    if not m:
        return []
    return m.group(1).split()


def syntax_markers(text: str) -> list[str]:
    return re.findall(r"<!-- readme-sync: syntax-([a-z0-9_]+) -->", text)


def first_rust_block(text: str) -> str | None:
    m = re.search(r"^```rust\n(.*?)^```", text, re.M | re.S)
    return m.group(1) if m else None


def caret_ok(req: str, version: str) -> bool:
    def parts(v: str) -> list[int]:
        return [int(x) for x in re.findall(r"\d+", v.split("-")[0])][:3]

    r, v = parts(req), parts(version)
    if not r:
        return False
    r += [0] * (3 - len(r))
    v += [0] * (3 - len(v))
    if v < r:
        return False
    if r[0] > 0:
        return v[0] == r[0]
    if r[1] > 0:
        return v[0] == 0 and v[1] == r[1]
    return v[:3] == r[:3]


# ── the check ──────────────────────────────────────────────────────────


def check(root: str) -> tuple[list[str], dict[str, int]]:
    errors: list[str] = []
    counts: dict[str, int] = {}
    docs = {}
    for rel in READMES:
        if os.path.exists(os.path.join(root, rel)):
            docs[rel] = read(root, rel)
        else:
            errors.append(f"{rel}: missing")
            docs[rel] = ""

    # syntax
    groups, intent, runtime_only = syntax_table(read(root, SYNTAX_RS))
    want = dict(groups)
    want["intent"] = intent
    if not groups or not intent:
        errors.append(f"{SYNTAX_RS}: SDF_SYNTAX / INTENT_SYNTAX not found")
    n = 0
    for rel, text in docs.items():
        listed_anywhere: list[str] = []
        for g in syntax_markers(text):
            if g not in want:
                errors.append(f"{rel}: `syntax-{g}` is not a group of {SYNTAX_RS} ({sorted(want)})")
        for g, names in want.items():
            got = block_after(text, f"<!-- readme-sync: syntax-{g} -->")
            if got is None:
                errors.append(f"{rel}: no `<!-- readme-sync: syntax-{g} -->` block")
                continue
            n += len(got)
            listed_anywhere += got
            dup = sorted({x for x in got if got.count(x) > 1})
            if dup:
                errors.append(f"{rel}: syntax-{g}: listed twice: {dup}")
            missing = [x for x in names if x not in got]
            extra = sorted(set(got) - set(names))
            if missing or extra:
                errors.append(f"{rel}: syntax-{g}: missing {missing}, extra {extra} (vs {SYNTAX_RS})")
        cross = sorted({x for x in listed_anywhere if listed_anywhere.count(x) > 1})
        if cross:
            errors.append(f"{rel}: names listed in more than one syntax group: {cross}")
    counts["syntax"] = n

    # macro
    mac = macro_names(read(root, MACRO_RS))
    geometry = {x for g, names in groups if g != "stdlib" for x in names} - set(runtime_only)
    if set(mac) != geometry:
        errors.append(
            f"{MACRO_RS}: `lol!` names differ from the SDF groups without `stdlib` / RUNTIME_ONLY"
            f" (macro only {sorted(set(mac) - geometry)}, missing from macro {sorted(geometry - set(mac))})"
        )
    counts["macro"] = len(mac)

    # laws
    law_src = read(root, LAW_RS)
    variants = enum_variants(law_src, "Constraint")
    classes = evidence_classes(law_src)
    if set(classes) != set(variants):
        errors.append(f"{LAW_RS}: evidence_class covers {sorted(classes)}, Constraint has {sorted(variants)}")
    n = 0
    for rel, text in docs.items():
        rows = table_after(text, "<!-- readme-sync: laws -->")
        if rows is None:
            errors.append(f"{rel}: no `<!-- readme-sync: laws -->` table")
            continue
        got = first_cell_names(rows)
        n += len(got)
        dup = sorted({x for x in got if got.count(x) > 1})
        if dup:
            errors.append(f"{rel}: laws listed twice: {dup}")
        if set(got) != set(variants):
            errors.append(
                f"{rel}: laws table {sorted(set(got))} != law::Constraint {sorted(variants)}"
                f" (missing {sorted(set(variants) - set(got))}, extra {sorted(set(got) - set(variants))})"
            )
        for r in rows:
            m = re.match(r"\|\s*`([^`]+)`", r)
            if not m or m.group(1) not in classes:
                continue
            stated = [e for e in EVIDENCE if any(e in c for c in cells(r)[1:])]
            if stated != [classes[m.group(1)]]:
                errors.append(
                    f"{rel}: law `{m.group(1)}` row states evidence {stated}, "
                    f"evidence_class returns {classes[m.group(1)]}"
                )
    counts["laws"] = n

    # verdicts
    verdicts = enum_variants(read(root, RESEARCH_RS), "ResearchVerdict")
    n = 0
    for rel, text in docs.items():
        rows = table_after(text, "<!-- readme-sync: research-verdicts -->")
        if rows is None:
            errors.append(f"{rel}: no `<!-- readme-sync: research-verdicts -->` table")
            continue
        got = first_cell_names(rows)
        n += len(got)
        dup = sorted({x for x in got if got.count(x) > 1})
        if dup:
            errors.append(f"{rel}: verdicts listed twice: {dup}")
        if set(got) != set(verdicts) or not verdicts:
            errors.append(
                f"{rel}: verdicts table {sorted(set(got))} != ResearchVerdict {sorted(verdicts)}"
            )
    counts["verdicts"] = n

    # features
    toml = read(root, CRATE_TOML)
    pkg = cargo_package(toml)
    want_f = cargo_features(toml)
    n = 0
    for rel, text in docs.items():
        rows = table_after(text, "<!-- readme-sync: features -->")
        if rows is None:
            errors.append(f"{rel}: no `<!-- readme-sync: features -->` table")
            continue
        got = first_cell_names(rows)
        n += len(got)
        dup = sorted({x for x in got if got.count(x) > 1})
        if dup:
            errors.append(f"{rel}: features listed twice: {dup}")
        if set(got) != want_f:
            errors.append(
                f"{rel}: features table {sorted(set(got))} != {CRATE_TOML} {sorted(want_f)}"
                f" (missing {sorted(want_f - set(got))}, extra {sorted(set(got) - want_f)})"
            )
    counts["features"] = n

    # msrv
    n = 0
    msrv = pkg.get("rust-version")
    if not msrv:
        errors.append(f"{CRATE_TOML}: no rust-version")
    for rel, text in docs.items():
        lines = [l for l in text.splitlines() if "readme-sync: msrv" in l]
        if not lines:
            errors.append(f"{rel}: no line marked `<!-- readme-sync: msrv -->`")
        for l in lines:
            m = re.search(r"\*\*(\d+\.\d+(?:\.\d+)?)\*\*", l)
            n += 1
            if not m or m.group(1) != msrv:
                errors.append(f"{rel}: MSRV line says {m.group(1) if m else '?'}, rust-version is {msrv}")
    counts["msrv"] = n

    # version literals (optional)
    n = 0
    version = pkg.get("version", "")
    pat = re.compile(r'alice-lol\s*=\s*(?:"([^"]+)"|\{[^}\n]*?version\s*=\s*"([^"]+)")')
    for rel, text in docs.items():
        for m in pat.finditer(text):
            req = m.group(1) or m.group(2)
            n += 1
            if not caret_ok(req, version):
                errors.append(f"{rel}: `alice-lol = \"{req}\"` does not match package version {version}")
    counts["version"] = n

    # example
    n = 0
    doc_block = first_rust_block(lib_doc(read(root, LIB_RS)))
    if doc_block is None:
        errors.append(f"{LIB_RS}: the crate documentation has no ```rust block")
    for rel, text in docs.items():
        block = first_rust_block(text)
        if block is None:
            errors.append(f"{rel}: no ```rust block")
            continue
        n += 1
        if block != doc_block:
            errors.append(f"{rel}: first ```rust block is not the crate-level doctest in {LIB_RS}")
    counts["example"] = n

    # links
    n = 0
    for rel, text in docs.items():
        base = os.path.dirname(os.path.join(root, rel))
        for target in re.findall(r"\]\(([^)\s]+)\)", text):
            if re.match(r"[a-z]+:", target) or target.startswith("#"):
                continue
            path = target.split("#", 1)[0]
            n += 1
            if not os.path.exists(os.path.normpath(os.path.join(base, path))):
                errors.append(f"{rel}: link target does not exist: {target}")
    counts["links"] = n

    # sections
    secs = {rel: len(re.findall(r"^## ", docs[rel], re.M)) for rel in READMES}
    if secs["README.md"] != secs["README_JP.md"]:
        errors.append(f"README.md has {secs['README.md']} `##` sections, README_JP.md has {secs['README_JP.md']}")
    counts["sections"] = secs["README.md"]

    for name, c in counts.items():
        if c == 0 and name != "version":
            errors.append(f"check `{name}` compared nothing")
    return errors, counts


def main() -> int:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(encoding="utf-8")
        except (AttributeError, ValueError):
            pass
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if "--root" in sys.argv:
        root = sys.argv[sys.argv.index("--root") + 1]
    errors, counts = check(root)
    print("compared: " + ", ".join(f"{k} {v}" for k, v in counts.items()))
    for e in errors:
        print(f"error: {e}", file=sys.stderr)
    if "--check" in sys.argv and errors:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
