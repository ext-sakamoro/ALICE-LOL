#!/usr/bin/env python3
"""Every boundary where a published crate of this workspace takes a value without its Rust type is listed (ALICE-Law G7).

A value passed as a Rust type carries its crate version and features with it; text, bytes, a file, a
foreign call, `dyn Any` and a value computed at build time do not, so there the writer's identifier has
to travel with the value and the reader has to compare it. This check keeps the list of those places
complete for the published crates (the release roster, `ROSTER` of scripts/release_roster.py), in two parts:

  * entry points: every public function or method of alice-lol (rustdoc JSON with every feature, hidden
    and private items, the dated nightly of scripts/sdf_writers.toml; free functions reachable from the
    crate root, inherent methods and trait-impl methods of reachable types) with a parameter of a
    text, bytes or reader type (str, String, [u8], [u8; N], Vec<u8>, an iterator of char or u8, Read,
    BufRead, Path, PathBuf, OsStr, OsString, also behind a reference, a generic bound or `impl Trait`)
    must be in scripts/g7_boundaries.toml (a `pub use` of another workspace member, a whole crate or one
    item under any name, is followed: that member's public entry points are found the same way from its own
    rustdoc JSON and listed under the path this crate gives them, e.g. `alice_lol::layout::decode`) under
    [[entry]] with a class and a reason:
      exchange   reads a value another program or release wrote; names the test that feeds it another
                 identifier (`test = "<file>::<fn>"`, the function must exist in that file)
      authoring  reads source a person or a model writes (the identifier wrapper is optional there and
                 checked when present; the exchange form of the same reader is listed as exchange)
      local      takes text or bytes that carry no Law-bearing value of another crate (a name, a label)
    An entry point not listed is a finding, and so is a listed one no longer found
  * forms: the library sources rustc reads for the two crates (from the dep-info of a fresh `cargo check`
    with every feature) hold no `extern "C"`, `no_mangle`, `export_name`, wasm-bindgen, PyO3 or `dyn Any`
    / `downcast` (comments and string literals left out); no published member has a build script or a
    crate type other than lib and proc-macro; each proc-macro crate is listed under [[macro]] with
    the file that compares its identifier and the script that pairs it with another release

Exit 1 on a finding; 2 when it compared nothing (no entry point read, no source file read) or cannot read
what it needs.

    python3 scripts/g7_boundaries.py            check
    python3 scripts/g7_boundaries.py --list     print the entry points found, as table rows to classify
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = ROOT / "scripts" / "g7_boundaries.toml"
sys.path.insert(0, str(ROOT / "scripts"))
from release_roster import ROSTER  # noqa: E402  (the one place the published set is written)

PUBLISHED = tuple(sorted(ROSTER))
CLASSES = ("exchange", "authoring", "local")


def sdf_writers():
    spec = importlib.util.spec_from_file_location("sdf_writers", ROOT / "scripts" / "sdf_writers.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# ---------------------------------------------------------------- entry points

# text and bytes in any container: a slice or a fixed-size array of u8, a Vec<u8>, an iterator of char or u8
ERASED = re.compile(r"\b(str|String|Read|BufRead|Path|PathBuf|OsStr|OsString)\b|\[u8(; [^\]]*)?\]|Vec<u8>"
                    r"|\bItem = (char|u8)\b")


def type_text(ty, bounds: dict[str, str]) -> str:
    """a readable rendering of a rustdoc JSON type; a generic parameter is followed by its bounds"""
    if not isinstance(ty, dict):
        return str(ty)
    if "primitive" in ty:
        return ty["primitive"]
    if "generic" in ty:
        g = ty["generic"]
        return f"{g}: {bounds[g]}" if g in bounds else g
    if "borrowed_ref" in ty:
        r = ty["borrowed_ref"]
        return ("&mut " if r.get("is_mutable") else "&") + type_text(r["type"], bounds)
    if "raw_pointer" in ty:
        return "*" + type_text(ty["raw_pointer"]["type"], bounds)
    if "slice" in ty:
        return f"[{type_text(ty['slice'], bounds)}]"
    if "array" in ty:
        return f"[{type_text(ty['array']['type'], bounds)}; {ty['array'].get('len')}]"
    if "tuple" in ty:
        return "(" + ", ".join(type_text(t, bounds) for t in ty["tuple"]) + ")"
    if "resolved_path" in ty:
        return path_text(ty["resolved_path"], bounds)
    if "impl_trait" in ty:
        return "impl " + " + ".join(bound_text(b, bounds) for b in ty["impl_trait"])
    if "dyn_trait" in ty:
        return "dyn " + " + ".join(path_text(t["trait"], bounds) for t in ty["dyn_trait"].get("traits", []))
    if "qualified_path" in ty:
        q = ty["qualified_path"]
        return f"<{type_text(q.get('self_type'), bounds)}>::{q.get('name')}"
    if "function_pointer" in ty:
        return "fn(" + json.dumps(ty["function_pointer"]) + ")"
    return json.dumps(ty)


def path_text(p: dict, bounds: dict[str, str]) -> str:
    name = p.get("path") or p.get("name") or "?"
    args = (p.get("args") or {}).get("angle_bracketed")
    if not args:
        return name
    parts = []
    for a in args.get("args", []):
        if isinstance(a, dict) and "type" in a:
            parts.append(type_text(a["type"], bounds))
    for c in args.get("constraints", []):
        binding = c.get("binding") or {}
        if "equality" in binding and isinstance(binding["equality"], dict) and "type" in binding["equality"]:
            parts.append(f"{c.get('name')} = {type_text(binding['equality']['type'], bounds)}")
    return f"{name}<{', '.join(parts)}>" if parts else name


def bound_text(b, bounds: dict[str, str]) -> str:
    if isinstance(b, dict) and "trait_bound" in b:
        return path_text(b["trait_bound"]["trait"], bounds)
    return json.dumps(b)


def generic_bounds(generics: dict) -> dict[str, str]:
    out: dict[str, list[str]] = {}
    for p in generics.get("params", []):
        kind = p.get("kind", {})
        if "type" in kind:
            out.setdefault(p["name"], []).extend(bound_text(b, {}) for b in kind["type"].get("bounds", []))
    for w in generics.get("where_predicates", []):
        bp = w.get("bound_predicate")
        if bp and "generic" in (bp.get("type") or {}):
            out.setdefault(bp["type"]["generic"], []).extend(bound_text(b, {}) for b in bp.get("bounds", []))
    return {k: " + ".join(v) for k, v in out.items() if v}


def entry_points(doc: dict, reachable: dict[str, list[str]]) -> dict[str, str]:
    """`<path>` -> the rendered parameters that erase the type, for every public function or method of the
    crate a caller outside can name, with at least one such parameter"""
    out: dict[str, str] = {}

    def consider(fid: str, path: str) -> None:
        item = doc["index"].get(fid)
        if item is None or "function" not in item["inner"]:
            return
        fn = item["inner"]["function"]
        bounds = generic_bounds(fn.get("generics", {}))
        hits = []
        for name, ty in fn["sig"]["inputs"]:
            text = type_text(ty, bounds)
            if name != "self" and ERASED.search(text):
                hits.append(f"{name}: {text}")
        if hits:
            out[path] = "; ".join(hits)

    for iid, item in doc["index"].items():
        if item.get("crate_id") != 0:
            continue
        inner = item["inner"]
        if "function" in inner and item.get("visibility") == "public" and iid in reachable:
            consider(iid, "::".join(reachable[iid]))
        im = inner.get("impl")
        if not im or im.get("is_synthetic") or im.get("blanket_impl"):
            continue
        rp = (im.get("for") or {}).get("resolved_path")
        if not rp or str(rp["id"]) not in reachable:
            continue
        owner = "::".join(reachable[str(rp["id"])])
        trait = im.get("trait")
        for it in im["items"]:
            m = doc["index"].get(str(it))
            if m is None or "function" not in m["inner"]:
                continue
            if trait:
                consider(str(it), f"<{owner} as {path_text(trait, {})}>::{m['name']}")
            elif m.get("visibility") == "public":
                consider(str(it), f"{owner}::{m['name']}")
    return out


def member_reexports(doc: dict, members: set[str]) -> list[tuple[str, list[str], list[str]]]:
    """`(crate, path in that crate, path here)` for every public `use` reachable from this crate's root whose
    target is an item of another workspace member (`members`: crate names); the path in that crate is empty
    for the whole crate. rustdoc does not put an item of another crate in `index`, only in `paths`"""
    out = []
    root = str(doc["root"])
    seen = set()
    todo = [(root, [doc["index"][root]["name"]])]
    while todo:
        mid, prefix = todo.pop(0)
        if mid in seen:
            continue
        seen.add(mid)
        for child in doc["index"][mid]["inner"]["module"]["items"]:
            item = doc["index"].get(str(child))
            if item is None or item.get("visibility") != "public":
                continue
            inner = item["inner"]
            if "module" in inner:
                todo.append((str(child), prefix + [item["name"]]))
                continue
            use = inner.get("use")
            if not use or str(use.get("id")) in doc["index"]:
                continue
            target = doc.get("paths", {}).get(str(use.get("id")))
            if not target:
                continue
            crate = doc.get("external_crates", {}).get(str(target.get("crate_id")), {}).get("name")
            if crate not in members:
                continue
            path = list(target.get("path", []))
            inside = path[1:] if path and path[0] == crate else path
            if use.get("is_glob"):
                out.append((crate, inside, prefix))
            else:
                out.append((crate, inside, prefix + [use["name"]]))
    return out


def unread_published(published_libs: set[str], reexports: list[tuple[str, list[str], list[str]]]) -> list[str]:
    """the published library crates (other than alice-lol, crate names) whose entry points this check does not
    read: only a whole-crate re-export from alice-lol brings all of a member's entry points here"""
    whole = {c for c, inside, _ in reexports if not inside}
    return sorted(published_libs - whole)


def follow_reexports(found_in: dict[str, dict[str, str]],
                     reexports: list[tuple[str, list[str], list[str]]]) -> dict[str, str]:
    """the entry points of the re-exported members (`found_in`: crate -> its entry points) under the paths
    this crate gives them; a re-export of one item brings that item and the methods of a type"""
    out: dict[str, str] = {}
    for crate, inside, here in reexports:
        there = "::".join([crate, *inside])
        alias = "::".join(here)
        for path, params in found_in.get(crate, {}).items():
            inner = path[1:] if path.startswith("<") else path
            if inner == there or inner.startswith(there + "::") or inner.startswith(there + " as "):
                renamed = alias + inner[len(there):]
                out["<" + renamed if path.startswith("<") else renamed] = params
    return out


def judge_entries(found: dict[str, str], table: list[dict], root: Path = ROOT) -> list[str]:
    findings = []
    listed = {}
    for e in table:
        path, cls = e.get("path"), e.get("class")
        if not path or cls not in CLASSES or not e.get("reason"):
            findings.append(f"g7_boundaries.toml: [[entry]] {e!r} needs path, class ({'/'.join(CLASSES)}) and reason")
            continue
        if path in listed:
            findings.append(f"g7_boundaries.toml: {path} is listed twice")
        listed[path] = e
        if cls == "exchange":
            test = e.get("test", "")
            file, _, fn = test.partition("::")
            src = (root / file).read_text(encoding="utf-8") if file and (root / file).is_file() else ""
            if not fn or not re.search(rf"\bfn {re.escape(fn)}\s*\(", src):
                findings.append(f"{path}: an exchange entry names the test feeding another identifier "
                                f"(`test = \"<file>::<fn>\"`); {test!r} is not a function in that file")
    for path, params in sorted(found.items()):
        if path not in listed:
            findings.append(f"{path} ({params}) takes text, bytes or a reader and is not in g7_boundaries.toml: "
                            "list it as exchange (with the mismatch test), authoring or local, with the reason")
    for path in sorted(set(listed) - set(found)):
        findings.append(f"g7_boundaries.toml lists {path}, which is no longer a public entry point taking text, "
                        "bytes or a reader (remove the row or fix its path)")
    return findings


# ---------------------------------------------------------------- forms

# an ABI string is a string literal, so `extern "C"` is matched on the code with only the comments
# removed; the other forms on the code without comments and string literals
COMMENTS = re.compile(r"//[^\n]*|/\*.*?\*/", re.S)
ABI = ("extern ABI", re.compile(r'\bextern\s+"[^"]*"'))
FORMS = [
    ("no_mangle", re.compile(r"\bno_mangle\b")),
    ("export_name", re.compile(r"\bexport_name\b")),
    ("wasm-bindgen", re.compile(r"\bwasm_bindgen\b")),
    ("PyO3", re.compile(r"\bpyo3\b|#\s*\[\s*py(function|module|class|methods)\b")),
    ("dyn Any", re.compile(r"\bdyn\s+(?:(?:core|std)::any::)?Any\b")),
    ("downcast", re.compile(r"\bdowncast(?:_ref|_mut)?\b")),
]


def form_findings(files: dict[str, str], strip: re.Pattern) -> list[str]:
    out = []
    blank = lambda m: re.sub(r"[^\n]", " ", m.group(0))  # noqa: E731 (keeps line numbers)
    for rel, text in sorted(files.items()):
        code = strip.sub(blank, text)
        with_strings = COMMENTS.sub(blank, text)
        for label, rx, src in [(ABI[0], ABI[1], with_strings), *((lb, r, code) for lb, r in FORMS)]:
            for m in rx.finditer(src):
                line = src.count("\n", 0, m.start()) + 1
                out.append(f"{rel}:{line}: {label} is a boundary that erases the Rust type; "
                           "this check lists none, so a new one needs its identifier comparison and a row here")
    return out


def target_findings(meta: dict, table: dict, root: Path = ROOT) -> tuple[list[str], int]:
    out = []
    macros = {m.get("crate"): m for m in table.get("macro", [])}
    seen = 0
    for pkg in meta["packages"]:
        if pkg["name"] not in PUBLISHED:
            continue
        seen += 1
        for t in pkg["targets"]:
            kinds = set(t["kind"])
            if "custom-build" in kinds:
                out.append(f"{pkg['name']}: a build script computes values at build time; "
                           "this check lists none")
            if kinds & {"cdylib", "staticlib", "dylib"}:
                out.append(f"{pkg['name']}: crate type {sorted(kinds)} is a foreign boundary; this check lists none")
            if "proc-macro" in kinds:
                m = macros.pop(pkg["name"], None)
                if m is None:
                    out.append(f"{pkg['name']}: a proc-macro crate is not listed under [[macro]]")
                    continue
                for key in ("compared_in", "paired_by"):
                    if not m.get(key) or not (root / m[key]).is_file():
                        out.append(f"{pkg['name']}: [[macro]] {key} {m.get(key)!r} is not a file")
    for name in macros:
        out.append(f"g7_boundaries.toml lists the proc-macro crate {name}, which is not a published proc macro")
    return out, seen


# ---------------------------------------------------------------- main

def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    ap.add_argument("--list", action="store_true")
    args = ap.parse_args(argv)
    sw = sdf_writers()
    try:
        nightly = sw.load_config()["nightly"]
        doc = sw.rustdoc("alice-lol", nightly, ["--all-features"])
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    found = entry_points(doc, sw.reachable_paths(doc))
    if not found:
        print("error: no public entry point taking text, bytes or a reader was read (compared nothing)",
              file=sys.stderr)
        return 2
    meta = json.loads(subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=ROOT,
                                     capture_output=True, text=True, check=True).stdout)
    # library members only: a proc-macro member has no function a caller can pass text to (its
    # expansion is compared through [[macro]])
    packages = {p["name"].replace("-", "_"): p["name"] for p in meta["packages"]
                if not any("proc-macro" in t.get("kind", []) for t in p.get("targets", []))}
    reexports = member_reexports(doc, set(packages) - {"alice_lol"})
    unread = unread_published({c for c, n in packages.items() if n in PUBLISHED} - {"alice_lol"}, reexports)
    if unread:
        print(f"error: the published crate(s) {', '.join(packages[c] for c in unread)} are not re-exported whole "
              "by alice-lol, so their entry points are not read here", file=sys.stderr)
        return 2
    found_in = {}
    try:
        for crate in sorted({c for c, _, _ in reexports}):
            member = sw.rustdoc(packages[crate], nightly, ["--all-features"])
            if not any(i.get("crate_id") == 0 for i in member.get("index", {}).values()):
                print(f"error: {packages[crate]} is re-exported but its rustdoc holds no item (read nothing)",
                      file=sys.stderr)
                return 2
            found_in[crate] = entry_points(member, sw.reachable_paths(member))
    except (ValueError, OSError, subprocess.CalledProcessError) as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    followed = follow_reexports(found_in, reexports)
    if any(found_in.values()) and not followed:
        print("error: re-exported members have entry points but none of them maps here", file=sys.stderr)
        return 2
    found.update(followed)
    if args.list:
        for path, params in sorted(found.items()):
            print(f'[[entry]]\npath = "{path}"\nclass = ""\nreason = ""  # {params}\n')
        return 0
    try:
        table = tomllib.loads(TABLE.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as e:
        print(f"error: {TABLE.name}: {e}", file=sys.stderr)
        return 2
    findings = judge_entries(found, table.get("entry", []))

    targets, seen = target_findings(meta, table)
    findings += targets
    if seen != len(PUBLISHED):
        print(f"error: read {seen} of the {len(PUBLISHED)} published crates in cargo metadata", file=sys.stderr)
        return 2
    # the target directory cargo uses (CARGO_TARGET_DIR or a config may move it): cleaned there, so both
    # crates are checked afresh and their dep-info is written
    target = Path(meta["target_directory"])
    sw.clean(list(PUBLISHED), target)
    p = subprocess.run(["cargo", "check", "--message-format=json", "--lib", "--all-features",
                        *[a for c in PUBLISHED for a in ("-p", c)]], cwd=ROOT, capture_output=True, text=True)
    if p.returncode != 0:
        print(f"error: cargo check failed:\n{p.stderr[-3000:]}", file=sys.stderr)
        return 2
    msgs = [json.loads(line) for line in p.stdout.splitlines() if line.startswith("{")]
    try:
        files, units = sw.dep_info_files(msgs, set(PUBLISHED))
    except ValueError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    if units < len(PUBLISHED) or not files:
        print(f"error: read the sources of {units} of {len(PUBLISHED)} crates (compared nothing)", file=sys.stderr)
        return 2
    findings += form_findings(files, sw.RUST_NON_CODE)

    for f in findings:
        print(f"finding: {f}", file=sys.stderr)
    classes = {c: sum(1 for e in table.get("entry", []) if e.get("class") == c) for c in CLASSES}
    print(f"g7-boundaries: {len(followed)} of them through re-exports of {len(found_in)} member(s)")
    print(f"g7-boundaries: {len(found)} entry point(s) taking text, bytes or a reader "
          f"({', '.join(f'{v} {k}' for k, v in classes.items())}), {len(files)} source file(s) of "
          f"{units} unit(s) scanned for {len(FORMS) + 1} foreign forms, {len(findings)} finding(s)")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
