#!/usr/bin/env python3
"""Compare two dumps of tests/parse_corpus_dump.rs taken before and after a rename of constructs.

usage: python3 scripts/compare_corpus_dumps.py BEFORE AFTER OLD=NEW [OLD=NEW ...]

Each line of a dump is `text <TAB> entry <TAB> result`. The AFTER dump is read back into the old
names (every NEW replaced by its OLD, longest first) and must then equal BEFORE line for line, with
one allowance: an error position (`@ N`) moves by the difference in length of the renamed names
that stand in the text before it. Anything else that differs is a change of reading, and fails.
Exits 2 when nothing was compared or no line contained a renamed name.
"""

import re
import sys

POS = re.compile(r"@ (\d+)\)")


def main() -> int:
    if len(sys.argv) < 4:
        print(__doc__, file=sys.stderr)
        return 2
    before = open(sys.argv[1], encoding="utf-8").read().splitlines()
    after = open(sys.argv[2], encoding="utf-8").read().splitlines()
    pairs = [a.split("=", 1) for a in sys.argv[3:]]
    pairs.sort(key=lambda p: -len(p[1]))
    if len(before) != len(after):
        print(f"compare: {len(before)} lines before, {len(after)} after", file=sys.stderr)
        return 1

    def back(line: str) -> str:
        for old, new in pairs:
            line = line.replace(new, old)
        return line

    renamed = moved = 0
    bad = []
    for i, (b, a) in enumerate(zip(before, after), 1):
        if a != b:
            renamed += 1
        a_old = back(a)
        if a_old == b:
            continue
        text = a.split("\t", 1)[0]
        m = POS.search(a)
        if m:
            pos = int(m.group(1))
            shift = 0
            for old, new in pairs:
                start = 0
                while (k := text.find(new, start)) != -1:
                    # the text is a Rust string literal: its first character is the quote
                    if k - 1 < pos:
                        shift += len(old) - len(new)
                    start = k + len(new)
            fixed = POS.sub(f"@ {pos + shift})", a_old, count=1)
            if fixed == b:
                moved += 1
                continue
        bad.append(f"line {i}:\n  before {b}\n  after  {a}")

    print(
        f"compare: {len(before)} lines, {renamed} with a renamed name, "
        f"{moved} equal after the position shift, {len(bad)} different",
        file=sys.stderr,
    )
    for x in bad:
        print(x, file=sys.stderr)
    if not before or renamed == 0:
        return 2
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
