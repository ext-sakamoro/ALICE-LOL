#!/usr/bin/env python3
"""The crates this workspace can publish are exactly the release roster.

The release of this workspace is `alice-lol-macro` and `alice-lol`. Every other member says
`publish = false`, so that a publish of the whole workspace cannot send a crate that was
never meant to be released. A member that loses `publish = false`, or a roster crate that
gains it, changes what a release sends: both fail here and the message names the crate.

The manifests are read directly (`tomllib`), so the check needs neither cargo nor the
sibling checkouts. As cargo reads it: `publish` absent means publishable, `false` or an empty
list means not, a non-empty list of registries means publishable, and `publish.workspace =
true` takes the value of `[workspace.package] publish` (absent there: publishable). Any other
value, or a member path that is not there (a glob is not expanded here), is an error, never
"not publishable".

usage: release_roster.py
Exit 1 when the publishable set is not the roster, 2 when no workspace member was read.
"""
from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ROSTER = {"alice-lol-macro", "alice-lol"}


class RosterError(Exception):
    """a manifest this check cannot read as cargo would"""


def can_publish(flag, workspace_flag, name: str) -> bool:
    if flag == {"workspace": True}:
        flag = workspace_flag
    if isinstance(flag, bool):
        return flag
    if isinstance(flag, list):
        return bool(flag)
    raise RosterError(f"{name}: `publish = {flag!r}` is not a value cargo reads (true, false, a list "
                      f"of registries or {{ workspace = true }})")


def publishable(root: Path) -> tuple[list[str], set[str]]:
    """(every member's package name, the names that can be published)"""
    ws = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]
    workspace_flag = ws.get("package", {}).get("publish", True)
    names, out = [], set()
    for member in ws["members"]:
        manifest = root / member / "Cargo.toml"
        if not manifest.is_file():
            raise RosterError(f"workspace member `{member}` has no {manifest.relative_to(root)} "
                              f"(a glob is not expanded by this check: list the members)")
        pkg = tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]
        names.append(pkg["name"])
        if can_publish(pkg.get("publish", True), workspace_flag, pkg["name"]):
            out.add(pkg["name"])
    return names, out


def main(root: Path = ROOT) -> int:
    try:
        names, can = publishable(root)
    except RosterError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
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
