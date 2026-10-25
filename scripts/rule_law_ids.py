#!/usr/bin/env python3
"""The v2 identifiers of the rule laws under laws/, from scripts/law_rules.py (not from the Rust code).

alice-lol's tests compare `rule_law_from_file(..).law_id()` with the values this writes to
alice-lol/tests/data/rule_law_ids_v2.txt, so the expected values come from a second
implementation of the identifier. The semantics identifiers the laws are hashed under are read
from the definitions of `RULE_SEMANTICS_ID` and `TABLE_SEMANTICS_ID` in alice-lol/src/rule_law.rs
(the value written there; the fold that gives it is checked by the Rust oracles) and written to
the file's header, which the Rust test compares with the constants.

    python3 scripts/rule_law_ids.py            write the file
    python3 scripts/rule_law_ids.py --check    fail when the file is not what this writes
    python3 scripts/rule_law_ids.py --self-test

Fails when no rule law is found (the check compared nothing).
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import law_rules  # noqa: E402

OUT = ROOT / "alice-lol" / "tests" / "data" / "rule_law_ids_v2.txt"
RUST = ROOT / "alice-lol" / "src" / "rule_law.rs"
HEADER = ("# v2 identifiers of the rule laws under laws/ (alice_zip::law_id layout), written by\n"
          "# scripts/rule_law_ids.py from scripts/law_rules.py: do not edit\n")


def semantics(rust_src: str) -> dict[str, bytes]:
    out = {}
    for key, name in (("rule", "RULE_SEMANTICS_ID"), ("table", "TABLE_SEMANTICS_ID")):
        m = re.search(rf"pub const {name}: \[u8; 32\] =\s*hex32\(\"([0-9a-f]{{64}})\"\);", rust_src)
        if not m:
            raise SystemExit(f"{RUST}: no `pub const {name}: [u8; 32] = hex32(\"<64 hex>\");`")
        out[key] = bytes.fromhex(m.group(1))
    return out


def render(root: Path, sem: dict[str, bytes]) -> str:
    rows = []
    for path in sorted(root.glob("laws/*/*.law")):
        text = path.read_text(encoding="utf-8")
        if law_rules.rule_kind(text) is None:
            continue
        law = law_rules.parse(text)
        rows.append(f"{path.relative_to(root).as_posix()} {law['kind']} {law_rules.law_id(law, sem).hex()}")
    if not rows:
        raise SystemExit("no rule law under laws/ (compared nothing)")
    head = "".join(f"semantics {k} {v.hex()}\n" for k, v in sorted(sem.items()))
    return HEADER + head + "".join(r + "\n" for r in rows)


def self_test() -> int:
    sem = {"rule": bytes(32), "table": bytes([1]) * 32}
    base = "kind predicate\nx-input s text\nrequire short: len(s) <= 3\nexample {\"s\": \"ab\"}\n"
    same = "law for people\n# a comment\nkind predicate\nx-input s   text\nrequire short:   len(s) <= 3\nexample {\"s\": \"a\"}\n"
    cases = [
        ("layout, comments, people lines and the example are not part of the id", base, same, True),
        ("a changed bound changes the id", base, base.replace("<= 3", "<= 4"), False),
        ("a renamed label changes the id", base, base.replace("short:", "small:"), False),
        ("the kind's semantics is part of the id", base, None, False),
    ]
    bad = 0
    for name, a, b, equal in cases:
        ia = law_rules.law_id(law_rules.parse(a), sem)
        ib = law_rules.law_id(law_rules.parse(a), {"rule": bytes([2]) * 32, "table": sem["table"]}) if b is None \
            else law_rules.law_id(law_rules.parse(b), sem)
        if (ia == ib) != equal:
            print(f"FAIL {name}")
            bad += 1
    # the layout of the v2 encoding, against a value written out by hand from alice_zip::law_id
    import hashlib
    want = hashlib.sha256(
        (15).to_bytes(8, "big") + b"alice-law-id/v2" + (4).to_bytes(8, "big") + b"kind" + bytes(32)
        + b"\x05" + (1).to_bytes(8, "big") + b"a" + b"\x02" + (7).to_bytes(4, "big")
        + b"\x03" + (9).to_bytes(8, "big") + b"\x01" + (2).to_bytes(8, "big") + b"\x00\xff").digest()
    got = law_rules._LawId(b"kind", bytes(32)).text("a").u32(7).u64(9).raw(b"\x00\xff").finish()
    if got != want:
        print("FAIL the v2 byte layout")
        bad += 1
    total = len(cases) + 1
    print(f"self-test: {total - bad}/{total} pass")
    return 1 if bad else 0


def main(argv: list[str]) -> int:
    if argv == ["--self-test"]:
        return self_test()
    text = render(ROOT, semantics(RUST.read_text(encoding="utf-8")))
    n = text.count("\n") - text.count("#") - 2
    if argv == ["--check"]:
        current = OUT.read_text(encoding="utf-8") if OUT.is_file() else ""
        if current != text:
            print(f"error: {OUT.relative_to(ROOT)} is not what scripts/rule_law_ids.py writes; run it", file=sys.stderr)
            return 1
        print(f"rule_law_ids: {n} rule law(s) compared, file up to date")
        return 0
    if argv:
        print(__doc__, file=sys.stderr)
        return 2
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(text, encoding="utf-8")
    print(f"rule_law_ids: wrote {n} rule law(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
