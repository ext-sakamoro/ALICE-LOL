#!/usr/bin/env python3
"""Each part template of the language at a size a person would print, for the shape oracles.

The grammar corpus (alice-lol/tests/common/corpus.rs) calls a template with small stand-in numbers
(3, 2, 1, ...), which for many parts makes a cavity, a slot or a wall empty: a check run there sees a
different shape from the one people print. This writes alice-lol/tests/data/template_sizes.txt, one
`name(args)  # source` line per part template:

- generated: the arm of `runtime_parser.rs` maps its arguments to the fields of a `...Spec`, and the
  spec's documented constructor (its first `pub const fn ...() -> Self` or `pub const X: Self`) gives
  the field values; the line's source names that constructor;
- by hand (HAND below): parts without a spec, or whose arguments are kinds (servo type, board, size
  class); each with the place its numbers come from (the builder's doc example or test).

Fails when no arm was read, when a hand row names a template the parser does not have, and with
--check when the file differs from what this would write.

usage: gen_template_sizes.py [--check]
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOL = ROOT / "alice-lol"
OUT = LOL / "tests" / "data" / "template_sizes.txt"

# name -> (call arguments, where the numbers come from)
HAND = {
    "arduino_mount_plate": ("1, 0", "board 1 = Uno, no extras (runtime_parser test)"),
    "bearing_seat": ("22, 5, 1", "608 bearing OD 22, 5 mm plate, slip fit (runtime_parser test)"),
    "bolt": ("4, 20", "fastener::bolt doc example (M4, 20 mm)"),
    "boss": ("3, 8", "reinforcement::boss doc example"),
    "bracket_l": ("60, 40, 4, 40, 3", "mount::bracket_l doc example"),
    "counterbore": ("4, 8", "fastener::counterbore doc example (M4, 8 mm plate)"),
    "countersink": ("4, 8", "fastener::countersink doc example (M4, 8 mm plate)"),
    "dovetail": ("10, 5, 20", "joint::dovetail doc example"),
    "dovetail_pair": ("20, 15, 10, 1", "runtime_parser test (female)"),
    "flange_circular": ("60, 20, 6, 45, 4, 4.2", "mount::flange_circular doc example"),
    "gridfinity_bin_ex": ("3, 3, 6, 2, 2, 1.5, 2", "runtime_parser test"),
    "mug": ("80, 120", "products::mug_sdf doc example"),
    "pin_hinge_knuckle": ("3, 8, 8", "joint::pin_hinge_knuckle doc example"),
    "pixhawk_mount": ("45, 1", "45 mm hole pattern with dampers (runtime_parser test)"),
    "profile_2020": ("100", "the profiles' tests (100 mm)"),
    "profile_3030": ("100", "the profiles' tests (100 mm)"),
    "rack_shelf": ("200, 5, 30, 25, 6, 3", "mount::rack_shelf doc example"),
    "servo_mount": ("2", "MG996R (runtime_parser test)"),
    "skadis_panel": ("300, 5, 5", "the SKADIS canonical panel (parser doc)"),
    "slot": ("20, 4, 6", "joint::slot doc example"),
    "snap_fit_annular": ("8, 20, 0.4, 7", "joint::snap_fit_annular doc example"),
    "t_slot_2020": ("100", "joint::t_slot_2020 doc example"),
}


def ctor_fields(spec, sources):
    for text in sources:
        m = re.search(r"impl\s+" + spec + r"\s*\{(.*?)\n\}", text, re.S)
        if not m:
            continue
        c = re.search(
            r"pub const fn (\w+)\(\)\s*->\s*Self\s*\{\s*Self\s*\{(.*?)\n\s*\}"
            r"|pub const (\w+): Self = Self \{(.*?)\n\s*\}",
            m.group(1),
            re.S,
        )
        if not c:
            continue
        ctor = c.group(1) or c.group(3)
        body = c.group(2) or c.group(4)
        fields = {}
        for k, v in re.findall(r"(\w+):\s*([^,\n]+),", body):
            v = v.strip()
            v = re.sub(r"^Some\((.*)\)$", r"\1", v)
            v = re.sub(r"^.*MetricSize::M", "", v)
            fields[k] = v
        return f"{spec}::{ctor}", fields
    return None, None


def generated(parser, sources):
    rows, unresolved = {}, []
    for m in re.finditer(r'\n\s*"([a-z0-9_]+)" => \{(.*?)\n            \}', parser, re.S):
        name, body = m.groups()
        t = re.search(r"let \(([^)]*)\) = self\.parse_\df\(\)\?;", body) or re.search(
            r"let (\w+) = self\.parse_1f\(\)\?;", body
        )
        sp = re.search(r"(\w+Spec)\s*\{", body)
        if not t or not sp:
            continue
        ctor, d = ctor_fields(sp.group(1), sources)
        if d is None:
            unresolved.append(name)
            continue
        args = []
        for v in (x.strip() for x in t.group(1).split(",")):
            names = [v] + re.findall(r"let (\w+) = [^;]*\b" + re.escape(v) + r"\b[^;]*;", body)
            field = None
            for nm in names:
                if re.search(r"\b" + re.escape(nm) + r",\s*\n", body) and nm in d:
                    field = nm
                    break
                cands = re.findall(r"(\w+):[^\n]*\b" + re.escape(nm) + r"\b", body)
                if nm in cands:
                    field = nm
                    break
                if cands:
                    field = cands[0]
                    break
            val = d.get(field) if field else None
            if val is None or not re.fullmatch(r"-?[0-9.]+", val):
                args = None
                break
            args.append(val.rstrip("0").rstrip(".") if "." in val else val)
        if args is None:
            unresolved.append(name)
        else:
            rows[name] = (", ".join(args), ctor)
    return rows, unresolved


def render():
    parser = (LOL / "src" / "runtime_parser.rs").read_text(encoding="utf-8")
    sources = [p.read_text(encoding="utf-8") for p in sorted((LOL / "src" / "stdlib").rglob("*.rs"))]
    rows, unresolved = generated(parser, sources)
    if not rows:
        sys.exit("no parser arm was read")
    for name in HAND:
        if f'"{name}" =>' not in parser:
            sys.exit(f"hand row {name}: no such template in runtime_parser.rs")
    rows.update(HAND)
    missing = [n for n in unresolved if n not in HAND]
    if missing:
        sys.exit("arms without a documented size (add a hand row): " + ", ".join(missing))
    lines = [
        "# each part template at a size a person would print; written by scripts/gen_template_sizes.py",
        "# name(args)  # where the numbers come from",
    ]
    lines += [f"{n}({a})  # {src}" for n, (a, src) in sorted(rows.items())]
    return "\n".join(lines) + "\n"


def main():
    text = render()
    if "--check" in sys.argv[1:]:
        if OUT.read_text(encoding="utf-8") != text:
            sys.exit(f"{OUT.relative_to(ROOT)} is stale; run scripts/gen_template_sizes.py")
        print(f"template sizes: {text.count(chr(10)) - 2} rows, current")
        return
    OUT.write_text(text, encoding="utf-8")
    print(f"wrote {text.count(chr(10)) - 2} rows")


if __name__ == "__main__":
    main()
