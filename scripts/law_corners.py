#!/usr/bin/env python3
"""Enumerates the corner points of the valid range of a law file.

usage: law_corners.py <file.law | dir>... [--json]   (a dir means every *.law in it)

A law file (laws/spike/*.law) states where the law applies: `input <name>
<unit> range <lo> <hi>` bounds an input, `x-integer <name>` makes it an
integer, `x-range <q> <lo> <hi>` and `x-range <q> > <v>` bound a derived
quantity (a `let` or `x-expr`), and `x-piece <out> <unit> <input> <lo> <hi>`
splits an output at input values. The corners are read from those lines
only; nothing is chosen by hand:

- `bound`: an accepted request with the input at its bound; for an integer
  input also the neighbour of the other parity (lo + 1, hi - 1)
- `outside`: a rejected request with the input just below lo / above hi
- `non-integer`: a rejected request with a non-integer value
- `edge`: an accepted request with a derived quantity at an edge of its
  x-range (for a strict `> v`, accepted with v < q <= v + 0.01 * scale,
  where scale is the largest numeric input of the request)
- `beyond`: a rejected request with a derived quantity outside its x-range
  (for a strict `> v`, at q <= v)
- `piece`: an accepted request with the input at an inner piece edge
- `vertex`: an accepted (or, when a scalar x-range rules it out, rejected)
  request with every ranged input at one of its bounds, one entry per
  combination (integer inputs use both values of their bound pair)

Each corner is a dict that `matches(corner, law, inputs, rejected)` decides
for one corpus vector. The module also evaluates law expressions (with
`math` or with any module that has the same function names, e.g. mpmath).
Standard library only.
"""

from __future__ import annotations

import itertools
import json
import math
import re
import sys
from pathlib import Path

# ---------------------------------------------------------------- expressions

_TOKEN = re.compile(r"\s*(?:(\d+\.?\d*(?:[eE][+-]?\d+)?|\.\d+(?:[eE][+-]?\d+)?)|([A-Za-z_]\w*)|(.))")


class ExprError(ValueError):
    pass


def _tokens(src: str) -> list[tuple[str, str]]:
    out = []
    pos = 0
    src = src.strip()
    while pos < len(src):
        m = _TOKEN.match(src, pos)
        if not m or m.end() == pos:
            raise ExprError(f"cannot read `{src[pos:]}`")
        pos = m.end()
        if m.group(1):
            out.append(("num", m.group(1)))
        elif m.group(2):
            out.append(("name", m.group(2)))
        elif m.group(3).strip():
            out.append(("op", m.group(3)))
    return out


def parse_expr(src: str):
    """Parses the law expression language into a tuple tree.

    `^` binds tighter than unary minus and is right associative; functions
    may take several arguments (the `x-expr` functions atan2, min, max).
    """
    toks = _tokens(src)
    i = 0

    def peek():
        return toks[i] if i < len(toks) else ("end", "")

    def take(kind=None, val=None):
        nonlocal i
        t = peek()
        if (kind and t[0] != kind) or (val and t[1] != val):
            raise ExprError(f"`{src}`: expected {val or kind}, got {t[1] or 'end'}")
        i += 1
        return t

    def primary():
        t = peek()
        if t[0] == "num":
            take()
            return ("num", float(t[1]))
        if t[0] == "name":
            take()
            if peek() == ("op", "("):
                take()
                args = [additive()]
                while peek() == ("op", ","):
                    take()
                    args.append(additive())
                take("op", ")")
                return ("call", t[1], args)
            return ("var", t[1])
        if t == ("op", "("):
            take()
            e = additive()
            take("op", ")")
            return e
        raise ExprError(f"`{src}`: unexpected {t[1] or 'end'}")

    def power():
        base = primary()
        if peek() == ("op", "^"):
            take()
            return ("pow", base, unary())
        return base

    def unary():
        if peek() == ("op", "-"):
            take()
            return ("neg", unary())
        if peek() == ("op", "+"):
            take()
            return unary()
        return power()

    def mult():
        e = unary()
        while peek() in (("op", "*"), ("op", "/")):
            op = take()[1]
            e = (op, e, unary())
        return e

    def additive():
        e = mult()
        while peek() in (("op", "+"), ("op", "-")):
            op = take()[1]
            e = (op, e, mult())
        return e

    tree = additive()
    if i != len(toks):
        raise ExprError(f"`{src}`: trailing `{peek()[1]}`")
    return tree


def evaluate(tree, env: dict, lib=math):
    kind = tree[0]
    if kind == "num":
        return lib.mpf(tree[1]) if hasattr(lib, "mpf") else tree[1]
    if kind == "var":
        if tree[1] not in env:
            raise ExprError(f"unknown name `{tree[1]}`")
        return env[tree[1]]
    if kind == "neg":
        return -evaluate(tree[1], env, lib)
    if kind == "call":
        args = [evaluate(a, env, lib) for a in tree[2]]
        name = tree[1]
        if name in ("min", "max"):
            return (min if name == "min" else max)(args)
        fn = {"ln": "log"}.get(name, name)
        return getattr(lib, fn)(*args)
    a = evaluate(tree[1], env, lib)
    b = evaluate(tree[2], env, lib)
    if kind == "+":
        return a + b
    if kind == "-":
        return a - b
    if kind == "*":
        return a * b
    if kind == "/":
        return a / b
    if kind == "pow":
        return a**b
    raise ExprError(f"unknown node {kind}")


# ---------------------------------------------------------------- law files


def _def(rest: str) -> tuple[str, str, str]:
    lhs, expr = rest.split("=", 1)
    w = lhs.split()
    return w[0], " ".join(w[1:]), expr.strip()


def parse_law(text: str) -> dict:
    law = {
        "name": None, "kind": None, "inputs": [], "integers": [], "lists": [],
        "x_inputs": [], "params": {}, "lets": [], "x_exprs": [], "x_ranges": [],
        "pieces": [], "outputs": [], "tolerances": [], "method": None,
        "states": [], "initial": {}, "odes": {}, "audit": [],
    }
    in_audit = False
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        if in_audit:
            if line == "end audit":
                in_audit = False
            else:
                law["audit"].append(line)
            continue
        head, _, rest = line.partition(" ")
        rest = rest.strip()
        w = rest.split()
        if head == "law":
            law["name"] = rest
        elif head == "kind":
            law["kind"] = rest
        elif head == "input":
            rng = (float(w[3]), float(w[4])) if len(w) == 5 and w[2] == "range" else None
            law["inputs"].append({"name": w[0], "unit": w[1], "range": rng})
        elif head == "x-integer":
            law["integers"].append(rest)
        elif head == "x-list":
            law["lists"].append(rest)
        elif head == "x-input":
            law["x_inputs"].append(w[0])
        elif head == "param":
            law["params"][w[0]] = w[1]
        elif head == "let":
            name, _, expr = _def(rest)
            law["lets"].append((name, parse_expr(expr)))
        elif head == "x-expr":
            name, _, expr = _def(rest)
            law["x_exprs"].append((name, parse_expr(expr)))
        elif head == "x-range":
            if w[1] == ">":
                law["x_ranges"].append({"q": w[0], "op": ">", "lo": float(w[2]), "hi": None})
            else:
                law["x_ranges"].append({"q": w[0], "op": "in", "lo": float(w[1]), "hi": float(w[2])})
        elif head == "x-piece":
            lhs, expr = rest.split("=", 1)
            p = lhs.split()
            law["pieces"].append({"out": p[0], "input": p[2], "lo": float(p[3]),
                                  "hi": float(p[4]), "expr": parse_expr(expr)})
        elif head == "output":
            name, _, expr = _def(rest)
            law["outputs"].append((name, parse_expr(expr)))
        elif head == "tolerance":
            name, _, expr = _def(rest)
            law["tolerances"].append((name, parse_expr(expr)))
        elif head == "x-method":
            law["method"] = w[0]
        elif head == "x-state":
            law["states"].append(w[0])
        elif head == "x-initial":
            name, _, expr = _def(rest)
            law["initial"][name] = parse_expr(expr)
        elif head == "x-ode":
            name, _, expr = _def(rest)
            law["odes"][name] = parse_expr(expr)
        elif head == "begin" and rest == "audit":
            in_audit = True
    if not law["name"]:
        raise ValueError("no `law` line")
    return law


def load(path) -> dict:
    return parse_law(Path(path).read_text(encoding="utf-8"))


def env_for(law: dict, inputs: dict, lib=math) -> dict:
    """Parameters, inputs, `let` and `x-expr` values for scalar inputs.

    A `let` that needs a value that is not there (a list element, a state)
    is left out.
    """
    conv = (lambda v: lib.mpf(v)) if hasattr(lib, "mpf") else float
    env = {k: conv(v) for k, v in law["params"].items()}
    for k, v in inputs.items():
        if isinstance(v, (int, float)) and not isinstance(v, bool):
            env[k] = conv(v)
    for name, tree in law["lets"] + law["x_exprs"]:
        try:
            env[name] = evaluate(tree, env, lib)
        except (ExprError, ValueError, ZeroDivisionError, TypeError):
            pass
    return env


def list_envs(law: dict, inputs: dict, lib=math) -> list[dict]:
    """One environment per element of the list input (or one, without a list)"""
    lists = [n for n in law["lists"] if isinstance(inputs.get(n), list)]
    if not lists:
        return [env_for(law, inputs, lib)]
    name = lists[0]
    return [env_for(law, {**inputs, name: x}, lib) for x in inputs[name]]


# ---------------------------------------------------------------- corners

EDGE_REL = 1e-12  # a derived quantity counts as "at the edge" within this relative distance
NEAR_REL = 0.01   # a strict `> v` edge: accepted within this fraction of the request's scale


def corners(law: dict) -> list[dict]:
    out = []
    ranged = [v for v in law["inputs"] if v["range"]]
    for v in ranged:
        lo, hi = v["range"]
        name = v["name"]
        integer = name in law["integers"]
        vals = [("lo", lo), ("hi", hi)]
        if integer:
            vals += [("lo+1", lo + 1), ("hi-1", hi - 1)]
        for tag, x in vals:
            out.append({"id": f"{name}@{tag}", "type": "bound", "input": name, "value": x,
                        "accept": True})
        out.append({"id": f"{name}<lo", "type": "outside", "input": name, "below": lo,
                    "accept": False})
        out.append({"id": f"{name}>hi", "type": "outside", "input": name, "above": hi,
                    "accept": False})
        if integer:
            out.append({"id": f"{name}:non-integer", "type": "non-integer", "input": name,
                        "accept": False})
    for r in law["x_ranges"]:
        q = r["q"]
        if r["op"] == "in":
            for tag, x in (("lo", r["lo"]), ("hi", r["hi"])):
                out.append({"id": f"{q}@{tag}", "type": "edge", "q": q, "value": x, "accept": True})
            out.append({"id": f"{q}<lo", "type": "beyond", "q": q, "below": r["lo"], "accept": False})
            out.append({"id": f"{q}>hi", "type": "beyond", "q": q, "above": r["hi"], "accept": False})
        else:
            out.append({"id": f"{q}@>{r['lo']:g}", "type": "edge", "q": q, "near_above": r["lo"],
                        "accept": True})
            out.append({"id": f"{q}<={r['lo']:g}", "type": "beyond", "q": q, "at_most": r["lo"],
                        "accept": False})
    seen = set()
    for p in law["pieces"]:
        rng = next((v["range"] for v in law["inputs"] if v["name"] == p["input"]), None)
        for x in (p["lo"], p["hi"]):
            if rng and x in rng or (p["input"], x) in seen:
                continue
            seen.add((p["input"], x))
            out.append({"id": f"{p['input']}@piece{x:g}", "type": "piece", "input": p["input"],
                        "value": x, "accept": True})
    if len(ranged) >= 2:
        choices = []
        for v in ranged:
            lo, hi = v["range"]
            if v["name"] in law["integers"]:
                choices.append([(v["name"], x) for x in (lo, lo + 1, hi - 1, hi)])
            else:
                choices.append([(v["name"], lo), (v["name"], hi)])
        for combo in itertools.product(*choices):
            point = dict(combo)
            accept = scalar_ranges_hold(law, point)
            tag = ",".join(f"{k}={_fmt(x)}" for k, x in combo)
            out.append({"id": f"vertex[{tag}]", "type": "vertex", "point": point,
                        "accept": accept is not False})
    return out


def _fmt(x: float) -> str:
    return str(int(x)) if float(x).is_integer() else repr(x)


def scalar_ranges_hold(law: dict, point: dict):
    """False when an x-range that the point decides fails, None when some cannot be decided, else True"""
    env = env_for(law, point)
    verdict = True
    for r in law["x_ranges"]:
        if r["q"] not in env:
            verdict = None
            continue
        q = env[r["q"]]
        if not (q > r["lo"] if r["op"] == ">" else r["lo"] <= q <= r["hi"]):
            return False
    return verdict


def _derived(law: dict, inputs: dict, q: str) -> list[float]:
    return [e[q] for e in list_envs(law, inputs) if q in e]


def _scale(inputs: dict) -> float:
    nums = [abs(v) for v in inputs.values() if isinstance(v, (int, float)) and not isinstance(v, bool)]
    return max(nums + [1.0])


def matches(c: dict, law: dict, inputs: dict, rejected: bool) -> bool:
    """Whether one corpus vector (its inputs, and whether it expects a rejection) covers the corner"""
    if c["accept"] == rejected:
        return False
    t = c["type"]
    if t in ("bound", "piece"):
        x = inputs.get(c["input"])
        xs = x if isinstance(x, list) else [x]
        return any(isinstance(v, (int, float)) and v == c["value"] for v in xs)
    if t == "outside":
        x = inputs.get(c["input"])
        if not isinstance(x, (int, float)) or isinstance(x, bool):
            return False
        return x < c["below"] if "below" in c else x > c["above"]
    if t == "non-integer":
        x = inputs.get(c["input"])
        return isinstance(x, float) and math.isfinite(x) and not x.is_integer()
    if t == "edge":
        vals = _derived(law, inputs, c["q"])
        if "near_above" in c:
            v = c["near_above"]
            return any(v < q <= v + NEAR_REL * _scale(inputs) for q in vals)
        e = c["value"]
        return any(abs(q - e) <= EDGE_REL * max(1.0, abs(e)) for q in vals)
    if t == "beyond":
        vals = _derived(law, inputs, c["q"])
        if "at_most" in c:
            return any(q <= c["at_most"] for q in vals)
        if "below" in c:
            return any(q < c["below"] for q in vals)
        return any(q > c["above"] for q in vals)
    if t == "vertex":
        return all(inputs.get(k) == x for k, x in c["point"].items())
    raise ValueError(t)


def main(argv: list[str]) -> int:
    as_json = "--json" in argv
    files = []
    for a in argv:
        if a == "--json":
            continue
        files += sorted(Path(a).glob("*.law")) if Path(a).is_dir() else [Path(a)]
    if not files:
        print(__doc__.splitlines()[2], file=sys.stderr)
        return 2
    result = {}
    for f in files:
        law = load(f)
        result[law["name"]] = corners(law)
    if as_json:
        json.dump(result, sys.stdout, indent=1)
        print()
    else:
        for name, cs in result.items():
            print(f"{name}: {len(cs)} corners")
            for c in cs:
                print(f"  {'accept' if c['accept'] else 'reject'}  {c['id']}")
    total = sum(len(cs) for cs in result.values())
    if total == 0:
        print("error: 0 corners enumerated", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
