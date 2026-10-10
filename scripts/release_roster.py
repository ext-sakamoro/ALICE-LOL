#!/usr/bin/env python3
"""The crates this workspace can publish are exactly the release roster.

The release of this workspace is `alice-lol-macro` and `alice-lol`. Every other member says
`publish = false`, so that a publish of the whole workspace cannot send a crate that was
never meant to be released. A member that loses `publish = false`, or a roster crate that
gains it, changes what a release sends: both fail here and the message names the crate.

The manifests are read directly (`tomllib`), so the check needs neither cargo nor the
sibling checkouts. `publish` absent means publishable; `false` or an empty list means not.

usage: release_roster.py
Exit 1 when the publishable set is not the roster, 2 when no workspace member was read.
"""
from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ROSTER = {"alice-lol-macro", "alice-lol"}


def publishable(root: Path) -> tuple[list[str], set[str]]:
    """(every member's package name, the names that can be published)"""
    ws = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["members"]
    names, out = [], set()
    for member in ws:
        pkg = tomllib.loads((root / member / "Cargo.toml").read_text(encoding="utf-8"))["package"]
        names.append(pkg["name"])
        flag = pkg.get("publish", True)
        if flag is True or (isinstance(flag, list) and flag):
            out.add(pkg["name"])
    return names, out


def main(root: Path = ROOT) -> int:
    names, can = publishable(root)
    if not names:
        print("error: read 0 workspace members", file=sys.stderr)
        return 2
    bad = [f"{n} can be published but is not in the release roster (add `publish = false`)"
           for n in sorted(can - ROSTER)]
    bad += [f"{n} is in the release roster but cannot be published (it has `publish = false`)"
            for n in sorted(ROSTER - can)]
    for b in bad:
        print(f"error: {b}", file=sys.stderr)
    print(f"release roster: read {len(names)} members, publishable {sorted(can)}, {len(bad)} difference(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
