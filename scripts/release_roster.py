#!/usr/bin/env python3
"""No crate of this workspace can be published, the release is retired, and every copy of the library set agrees.

Every crate is `LicenseRef-Proprietary` with `publish = false`, so the release roster
(`ROSTER` below) is empty: a member that loses `publish = false` fails here and the message
names it. The release workflow is gone and no workflow runs `cargo publish`; CI and
preflight no longer run the packaging check of a crate (`package_integrity.py --member`
without `--patches`).

The library crates are `alice-lol-macro`, `alice-lol` and `alice-lol-layout` (`LIBRARY`
below, the one place the set is written; `alice-lol` re-exports the other two). They carry the
Law-bearing output (ALICE-Law G7 / G10 / G13): g7_boundaries.py imports `LIBRARY`. The places
that cannot import it are compared with it, and any one that disagrees fails with its file named:

  * .github/entropy-deps.toml: `members` (the members entropy_deps_guard.py checks) is exactly
    the library set;

  * ci.yml and preflight.sh: every library crate linted by a library clippy command
    (`CLIPPY_CONF_DIR` .github/clippy-lib, by `-p` or by `--workspace` without `--exclude` of
    it), and no `package_integrity.py --member <crate>` packaging command;
  * alice-law-conformance.toml, README.md, README_JP.md: every "library crate(s) (...)"
    names exactly the library set, and no "published crate(s) (...)" list is left; the
    crate table of each README lists exactly the members;
  * .github/paths/semver-checks.txt: `<crate>/src/**` and `<crate>/Cargo.toml` for each library
    crate (alice-lol's public API includes what it re-exports, so a change there must run the
    semver check);
  * each library manifest: license, edition, rust-version, authors, repository and homepage
    equal alice-lol's, documentation is its docs.rs page, a description and a readme file;
  * .github/workflows/: no release.yml, and no workflow line runs `cargo publish`.

A file that is not there, or a comparison set that finds nothing to compare in it, is an
error (exit 2), never a pass.

The manifests are read directly (`tomllib`), so the check needs neither cargo nor the
sibling checkouts. As cargo reads it: `publish` absent means publishable, `false` or an empty
list means not, a non-empty list of registries means publishable, and `publish.workspace =
true` takes the value of `[workspace.package] publish` (absent there: publishable). Any other
value, or a member path that is not there (a glob is not expanded here), is an error, never
"not publishable".

usage: release_roster.py
Exit 1 when a member can be published or a copy disagrees with the library set, 2 when no
workspace member was read or a copy is missing or holds nothing to compare.
"""
from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# the crates a release may publish: none (every crate is LicenseRef-Proprietary, publish = false)
ROSTER: frozenset[str] = frozenset()
# the library crates: alice-lol and the two crates it re-exports (the Law-bearing output)
LIBRARY = frozenset({"alice-lol-macro", "alice-lol", "alice-lol-layout"})


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


# the metadata every library crate shares with alice-lol
SHARED = ("license", "edition", "rust-version", "authors", "repository", "homepage")
NOTES = ("alice-law-conformance.toml", "README.md", "README_JP.md")
TABLES = ("README.md", "README_JP.md")
LIBRARY_LIST = re.compile(r"library crates? \(([^)]*)\)")
PUBLISHED_LIST = re.compile(r"published crates? \(([^)]*)\)")
CRATE_ROW = re.compile(r"^\| `([a-z0-9-]+)` \|")


def _read(root: Path, rel: str) -> str:
    p = root / rel
    if not p.is_file():
        raise RosterError(f"{rel} is not there (the roster is compared with it)")
    return p.read_text(encoding="utf-8")


def _differs(where: str, what: str, have: set[str], want: set[str], of: str = "the library set") -> list[str]:
    out = [f"{where}: {what} lacks {n}" for n in sorted(want - have)]
    return out + [f"{where}: {what} has {n}, which is not in {of}" for n in sorted(have - want)]


RELEASE = ".github/workflows/release.yml"
PUBLISH = re.compile(r"^(?!\s*#).*\bcargo\s+publish\b", re.M)


def workflows(root: Path) -> tuple[int, list[str]]:
    """(workflows read, one problem per release workflow or `cargo publish` line)"""
    files = sorted((root / ".github" / "workflows").glob("*.yml"))
    if not files:
        raise RosterError(".github/workflows/: no workflow read (compared nothing)")
    out = [f"{RELEASE} exists (nothing is released: every crate has `publish = false`)"] if (root / RELEASE).exists() else []
    for f in files:
        for m in PUBLISH.finditer(f.read_text(encoding="utf-8")):
            line = f.read_text(encoding="utf-8").count("\n", 0, m.start()) + 1
            out.append(f"{f.relative_to(root).as_posix()}:{line}: runs `cargo publish` (nothing is released)")
    return len(files), out


def _steps(text: str, rel: str) -> list[str]:
    """the commands of a file: one per step of a workflow, one per line of a shell script"""
    if rel.endswith(".yml"):
        return re.split(r"^\s*- (?:name|uses):", text, flags=re.M)
    return text.splitlines()


def clippy_lib_covered(text: str, rel: str, members: set[str]) -> set[str] | None:
    """the members the library clippy commands of a file lint (None: no such command)"""
    covered, read = set(), False
    for step in _steps(text, rel):
        if not re.search(r"CLIPPY_CONF_DIR[:=]\s*\.github/clippy-lib", step):
            continue
        for line in step.splitlines():
            if "cargo clippy" not in line or line.lstrip().startswith("#"):
                continue
            read = True
            words = line.split()
            names = {words[i + 1] for i, x in enumerate(words[:-1]) if x in ("-p", "--package")}
            excluded = {words[i + 1] for i, x in enumerate(words[:-1]) if x == "--exclude"}
            covered |= names | ((members - excluded) if "--workspace" in words else set())
    return covered if read else None


def ci_commands(root: Path, members: set[str]) -> tuple[int, list[str]]:
    compared, out = 0, []
    for rel in (".github/workflows/ci.yml", "scripts/preflight.sh"):
        text = _read(root, rel)
        packaged = {m for line in text.splitlines() if "package_integrity.py" in line
                    and "--patches" not in line and not line.lstrip().startswith("#")
                    for m in re.findall(r"--member (\S+)", line)}
        covered = clippy_lib_covered(text, rel, members)
        if covered is None:
            raise RosterError(f"{rel}: no library clippy command read (compared nothing)")
        out += [f"{rel}: runs the packaging check package_integrity.py --member {n} (nothing is published)"
                for n in sorted(packaged)]
        out += [f"{rel}: no library clippy command (.github/clippy-lib) lints {n}" for n in sorted(LIBRARY - covered)]
        compared += 2
    return compared, out


SEMVER_PATHS = ".github/paths/semver-checks.txt"
ENTROPY_CONFIG = ".github/entropy-deps.toml"


def entropy_members(text: str) -> tuple[int, list[str]]:
    try:
        members = tomllib.loads(text).get("members")
    except tomllib.TOMLDecodeError as e:
        raise RosterError(f"{ENTROPY_CONFIG}: {e}") from e
    if not isinstance(members, list) or not members:
        raise RosterError(f"{ENTROPY_CONFIG}: no `members` read (compared nothing)")
    return 1, _differs(ENTROPY_CONFIG, "members", set(members), LIBRARY)


def semver_paths(text: str) -> tuple[int, list[str]]:
    lines = {x.strip() for x in text.splitlines() if x.strip() and not x.lstrip().startswith("#")}
    crates = {x.split("/", 1)[0] for x in lines if x.endswith(("/src/**", "/Cargo.toml"))}
    if not crates:
        raise RosterError(f"{SEMVER_PATHS}: no `<crate>/src/**` or `<crate>/Cargo.toml` read (compared nothing)")
    out = [f"{SEMVER_PATHS}: lacks {c}/{tail}" for c in sorted(LIBRARY) for tail in ("src/**", "Cargo.toml")
           if f"{c}/{tail}" not in lines]
    return 2 * len(LIBRARY), out


def crate_table(text: str) -> set[str]:
    """the crates of the README table that holds the `alice-lol` row"""
    lines = text.splitlines()
    at = next((i for i, x in enumerate(lines) if x.startswith("| `alice-lol` |")), None)
    if at is None:
        return set()
    lo = at
    while lo > 0 and lines[lo - 1].startswith("|"):
        lo -= 1
    hi = at
    while hi + 1 < len(lines) and lines[hi + 1].startswith("|"):
        hi += 1
    return {m.group(1) for x in lines[lo:hi + 1] if (m := CRATE_ROW.match(x))}


def documents(root: Path, members: set[str]) -> tuple[int, list[str]]:
    compared, out = 0, []
    for rel in NOTES:
        text = _read(root, rel)
        lists = LIBRARY_LIST.findall(text)
        if not lists:
            raise RosterError(f'{rel}: no "library crate(s) (...)" read (compared nothing)')
        for i, names in enumerate(lists, 1):
            out += _differs(rel, f'"library crates" list {i}', {n.strip() for n in names.split(",")}, LIBRARY)
            compared += 1
        out += [f'{rel}: names "published crates ({names})", but nothing is published'
                for names in PUBLISHED_LIST.findall(text)]
    for rel in TABLES:
        table = crate_table(_read(root, rel))
        if not table:
            raise RosterError(f"{rel}: no crate table read (compared nothing)")
        out += _differs(rel, "the crate table", table, members, "the workspace members")
        compared += 1
    return compared, out


def manifests(root: Path, paths: dict[str, Path]) -> tuple[int, list[str]]:
    pkgs = {n: tomllib.loads((paths[n] / "Cargo.toml").read_text(encoding="utf-8"))["package"]
            for n in LIBRARY if n in paths}
    if "alice-lol" not in pkgs:
        raise RosterError("alice-lol is not a member (the library metadata is compared with it)")
    base, compared, out = pkgs["alice-lol"], 0, []
    for name, pkg in sorted(pkgs.items()):
        for key in SHARED:
            compared += 1
            if pkg.get(key) != base.get(key):
                out.append(f"{name}/Cargo.toml: {key} = {pkg.get(key)!r}, alice-lol has {base.get(key)!r}")
        if pkg.get("documentation") != f"https://docs.rs/{name}":
            out.append(f"{name}/Cargo.toml: documentation is not https://docs.rs/{name}")
        if not pkg.get("description"):
            out.append(f"{name}/Cargo.toml: no description")
        readme = pkg.get("readme")
        if not readme or not (paths[name] / readme).is_file():
            out.append(f"{name}/Cargo.toml: readme {readme!r} is not a file")
        compared += 3
    return compared, out


def copies(root: Path, members: dict[str, Path]) -> tuple[int, list[str]]:
    """(comparisons made, one problem per copy of the library set that disagrees with it)"""
    total, out = 0, []
    for n, problems in (workflows(root), entropy_members(_read(root, ENTROPY_CONFIG)),
                        semver_paths(_read(root, SEMVER_PATHS)),
                        ci_commands(root, set(members)), documents(root, set(members)),
                        manifests(root, members)):
        total += n
        out += problems
    return total, out


def member_dirs(root: Path) -> dict[str, Path]:
    ws = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]
    out = {}
    for member in ws["members"]:
        pkg = tomllib.loads((root / member / "Cargo.toml").read_text(encoding="utf-8"))["package"]
        out[pkg["name"]] = root / member
    return out


def main(root: Path = ROOT) -> int:
    try:
        names, can = publishable(root)
    except RosterError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    if not names:
        print("error: read 0 workspace members", file=sys.stderr)
        return 2
    bad = [f"{n} can be published, but nothing is released (add `publish = false`)"
           for n in sorted(can - ROSTER)]
    bad += [f"{n} is in the library set but is not a workspace member" for n in sorted(LIBRARY - set(names))]
    try:
        compared, drift = copies(root, member_dirs(root))
    except RosterError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    if compared == 0:
        print("error: compared no copy of the roster", file=sys.stderr)
        return 2
    bad += drift
    for b in bad:
        print(f"error: {b}", file=sys.stderr)
    print(f"release roster: read {len(names)} members, {len(can)} publishable (expected {len(ROSTER)}), "
          f"{compared} comparison(s) with the copies of the library set, {len(bad)} difference(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
