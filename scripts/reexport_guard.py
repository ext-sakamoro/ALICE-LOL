#!/usr/bin/env python3
"""No public re-export of alice-lol reaches a writer that skips the LOL producer identifier (ALICE-Law G8).

alice-sdf's `alice_sdf::io` writers (export_stl, export_stl_ascii, export_3mf, export_fbx, export_obj ...)
store a mesh without LOL's producer identifier; alice-lol's own writers (print_export::node_to_stl and the
others) store it through the `*_with_stamp` forms. A `pub use` that makes one of the unstamped writers part
of alice-lol's public API, under any name or path, would let a caller write a LOL mesh without it.

The check reads rustdoc's JSON for alice-lol with every feature, built by the nightly pinned in
scripts/reexport_guard.toml, and takes every public `use` item of the crate. Each one is resolved to the
canonical path rustdoc records for its target (so a rename, `pub use alice_sdf::io as sdf_io`, a glob or a
re-export through another module of alice-lol resolves to the same place). For each [[forbid]] rule it is a
finding when the target is in the rule's crate and is
  * a function under the rule's module whose name matches `function` and not `allow`, or
  * a module that is the rule's module, one of its submodules, or one of its ancestors (the crate root
    included): re-exporting it re-exports the writers inside.
It exits 1 on a finding, and 2 when it compared nothing: no rustdoc JSON, an unknown format version, no
public `use` item read, or no re-export into a rule's crate (the resolution would then be untested).

    python3 scripts/reexport_guard.py              build the JSON and check it
    python3 scripts/reexport_guard.py --json FILE  check an existing JSON (the tests)
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONFIG = ROOT / "scripts" / "reexport_guard.toml"
# rustdoc JSON versions whose `use` items carry {source, name, id, is_glob} and whose `paths` entries carry
# {crate_id, path, kind}; outside this range the layout is not known to the check
FORMAT_VERSIONS = range(45, 80)


def load_config(path: Path = CONFIG) -> dict:
    cfg = tomllib.loads(path.read_text(encoding="utf-8"))
    rules = cfg.get("forbid", [])
    if not rules:
        raise ValueError(f"{path}: no [[forbid]] rule")
    for r in rules:
        for key in ("crate", "module", "function", "allow", "reason"):
            if key not in r:
                raise ValueError(f"{path}: a [[forbid]] rule has no {key!r}")
    return cfg


def findings(doc: dict, rules: list[dict]) -> tuple[list[str], int, dict[str, int]]:
    """(findings, public use items read, re-exports into each rule's crate)"""
    if doc.get("format_version") not in FORMAT_VERSIONS:
        raise ValueError(f"rustdoc JSON format_version {doc.get('format_version')!r} is not one this check reads")
    crates = {cid: c["name"] for cid, c in doc["external_crates"].items()}
    out: list[str] = []
    read = 0
    into = {r["crate"]: 0 for r in rules}
    for item in doc["index"].values():
        if item.get("crate_id") != 0 or item.get("visibility") != "public" or "use" not in item["inner"]:
            continue
        use = item["inner"]["use"]
        read += 1
        target = doc["paths"].get(str(use.get("id"))) if use.get("id") is not None else None
        if target is None:
            continue  # a local item of alice-lol
        crate = crates.get(str(target["crate_id"]))
        path, kind = target["path"], target["kind"]
        where = f"{item['span']['filename']}:{item['span']['begin'][0]}" if item.get("span") else "?"
        shown = f"pub use {use['source']}{'::*' if use.get('is_glob') else ''}"
        for r in rules:
            if crate != r["crate"]:
                continue
            into[r["crate"]] += 1
            module = r["module"]
            if kind == "module" and (path[:len(module)] == module or module[:len(path)] == path):
                out.append(f"{where}: {shown} re-exports the module {'::'.join(path)}, which holds or contains "
                           f"{'::'.join(module)} ({r['reason']})")
            elif (kind == "function" and path[:len(module)] == module and re.search(r["function"], path[-1])
                  and not re.search(r["allow"], path[-1])):
                out.append(f"{where}: {shown} re-exports {'::'.join(path)} ({r['reason']})")
    return out, read, into


def build_json(nightly: str) -> Path:
    cmd = ["cargo", f"+{nightly}", "rustdoc", "-q", "-p", "alice-lol", "--lib", "--all-features", "--",
           "-Z", "unstable-options", "--output-format", "json"]
    p = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    if p.returncode != 0:
        raise ValueError(f"{' '.join(cmd)} failed:\n{p.stderr[-4000:]}")
    meta = json.loads(subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=ROOT,
                                     capture_output=True, text=True, check=True).stdout)
    return Path(meta["target_directory"]) / "doc" / "alice_lol.json"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--json", type=Path)
    ap.add_argument("--config", type=Path, default=CONFIG)
    a = ap.parse_args(argv)
    try:
        cfg = load_config(a.config)
        path = a.json or build_json(cfg["nightly"])
        doc = json.loads(path.read_text(encoding="utf-8"))
        found, read, into = findings(doc, cfg["forbid"])
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as e:
        print(f"reexport-guard: {e}", file=sys.stderr)
        return 2
    for f in found:
        print(f"error: {f}", file=sys.stderr)
    counts = ", ".join(f"{n} into {c}" for c, n in into.items())
    print(f"reexport-guard: {read} public use item(s), {counts}, {len(found)} finding(s)")
    if read == 0 or any(n == 0 for n in into.values()):
        print("reexport-guard: compared nothing (no public use item, or none into a rule's crate)", file=sys.stderr)
        return 2
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
