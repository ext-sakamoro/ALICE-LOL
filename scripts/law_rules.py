#!/usr/bin/env python3
"""Rule laws (`kind machine` / `kind predicate`, conformance/TASK.md "Rule laws"): reading,
answering a request, and the conformance vectors of a law.

Standard library only. Used by conformance/gen_corpus.py (the expected answers),
scripts/law_corpus_cover.py (what a corpus must contain) and conformance/ref_impl.py.
The Rust reader (alice-lol `rule_law`) is a separate implementation scored on the same
corpus, so a defect on either side shows as a failing vector.

A request is answered with one of
  ("outputs", {...})      the answer
  ("rejected", reason)    an input that does not have its type / is not declared
  ("request_error", why)  a missing input (exit status 2)
"""
from __future__ import annotations

import hashlib
import hmac
import json
import re

MAX_INT = 2 ** 53 - 1  # every JSON reader holds it exactly (TASK.md "Rule laws")
NAME = re.compile(r"[a-z][a-z0-9_]*\Z")
FUNCTIONS = {"len", "prefix", "utf8", "hmac_sha256"}
RESERVED = {"true", "false", "and", "or", "not", "state", "mode", "shape"} | FUNCTIONS
PEOPLE = {"law", "claim", "source", "verdict", "oracle"}
SCALARS = {"integer", "time", "text", "bytes", "bool"}
INT_TEXT = re.compile(r"(0|-?[1-9][0-9]*)\Z")
HEX = re.compile(r"(?:[0-9a-f]{2})*\Z")
# conformance/TASK.md: format characters that make a law file unreadable
FORMAT = [(0xAD, 0xAD), (0x600, 0x605), (0x61C, 0x61C), (0x6DD, 0x6DD), (0x70F, 0x70F),
          (0x890, 0x891), (0x8E2, 0x8E2), (0x180E, 0x180E), (0x200B, 0x200F), (0x202A, 0x202E),
          (0x2060, 0x2064), (0x2066, 0x206F), (0xFEFF, 0xFEFF), (0xFFF9, 0xFFFB), (0x110BD, 0x110BD),
          (0x110CD, 0x110CD), (0x13430, 0x1343F), (0x1BCA0, 0x1BCA3), (0x1D173, 0x1D17A),
          (0xE0001, 0xE0001), (0xE0020, 0xE007F), (0x2028, 0x2029)]


class RuleError(ValueError):
    """The law file does not read"""


# ------------------------------------------------------------------ text and kind

def rule_kind(text: str):
    """`machine` / `predicate` when the file is a rule law (no `kind audit` line), else None"""
    lines = [ln.split("#", 1)[0].strip() for ln in text.split("\n")]
    if "kind audit" in lines:
        return None
    for k in ("machine", "predicate"):
        if f"kind {k}" in lines:
            return k
    return None


def _check_text(text: str):
    segs = text.split("\n")
    for n, seg in enumerate(segs):
        body = seg[:-1] if n < len(segs) - 1 and seg.endswith("\r") else seg
        for c in body:
            cp = ord(c)
            if c == "\t":
                continue
            if cp < 0x20 or 0x7F <= cp <= 0x9F or any(lo <= cp <= hi for lo, hi in FORMAT):
                raise RuleError(f"line {n + 1}: U+{cp:04X} is not allowed in a law file")


def _lines(text: str):
    """(line number, text without comment, trimmed) of the non-empty lines"""
    _check_text(text)
    out = []
    for n, raw in enumerate(text.split("\n"), 1):
        raw = raw[:-1] if raw.endswith("\r") else raw
        ln = raw.split("#", 1)[0]
        if ln.strip():
            out.append((n, ln))
    return out


# ------------------------------------------------------------------ expressions

TOKEN = re.compile(r"""
    (?P<ws>[ \t]+)
  | (?P<bytes>0x[0-9a-f]*)
  | (?P<int>[0-9]+)
  | (?P<text>"[^"\\]*")
  | (?P<op>==|!=|<=|>=|\+\+|<|>|\(|\)|,|\.)
  | (?P<name>[a-z][a-z0-9_]*)
""", re.X)


def _tokens(src: str):
    out, i = [], 0
    while i < len(src):
        m = TOKEN.match(src, i)
        if not m:
            raise RuleError(f"`{src}`: unexpected `{src[i:]}`")
        i = m.end()
        k = m.lastgroup
        if k == "ws":
            continue
        out.append((k, m.group()))
    return out


def _literal(kind, tok):
    if kind == "int":
        if (len(tok) > 1 and tok[0] == "0") or int(tok) > MAX_INT:
            raise RuleError(f"integer literal `{tok}`")
        return ("int", "integer", int(tok))
    if kind == "bytes":
        if len(tok) % 2:
            raise RuleError(f"byte literal `{tok}` has an odd number of hex digits")
        return ("lit", "bytes", bytes.fromhex(tok[2:]))
    if kind == "text":
        return ("lit", "text", tok[1:-1])
    raise RuleError(f"`{tok}` is not a literal")


class _Parser:
    """Recursive descent over the token list; `atom` decides what a name means"""

    def __init__(self, src, atom):
        self.src, self.t, self.i, self.atom = src, _tokens(src), 0, atom

    def peek(self, off=0):
        j = self.i + off
        return self.t[j] if j < len(self.t) else (None, None)

    def take(self, val=None):
        k, v = self.peek()
        if k is None or (val is not None and v != val):
            raise RuleError(f"`{self.src}`: expected {val or 'more'}, got {v or 'end'}")
        self.i += 1
        return k, v

    def parse(self):
        e = self.or_()
        if self.i != len(self.t):
            raise RuleError(f"`{self.src}`: unexpected `{self.peek()[1]}`")
        return e

    def or_(self):
        e = self.and_()
        while self.peek() == ("name", "or"):
            self.take()
            e = ("or", e, self.and_())
        return e

    def and_(self):
        e = self.not_()
        while self.peek() == ("name", "and"):
            self.take()
            e = ("and", e, self.not_())
        return e

    def not_(self):
        if self.peek() == ("name", "not"):
            self.take()
            return ("not", self.not_())
        return self.cmp()

    def cmp(self):
        e = self.cat()
        if self.peek()[1] in ("==", "!=", "<", "<=", ">", ">="):
            op = self.take()[1]
            e = ("cmp", op, e, self.cat())
            if self.peek()[1] in ("==", "!=", "<", "<=", ">", ">="):
                raise RuleError(f"`{self.src}`: comparisons do not chain")
        return e

    def cat(self):
        e = self.primary()
        while self.peek()[1] == "++":
            self.take()
            e = ("++", e, self.primary())
        return e

    def primary(self):
        k, v = self.peek()
        if v == "(":
            self.take()
            e = self.or_()
            self.take(")")
            return e
        if k in ("int", "bytes", "text"):
            self.take()
            return _literal(k, v)
        if k == "name":
            self.take()
            if v in ("true", "false"):
                return ("lit", "bool", v == "true")
            if v in FUNCTIONS and self.peek()[1] == "(":
                self.take("(")
                args = [self.or_()]
                while self.peek()[1] == ",":
                    self.take()
                    args.append(self.or_())
                self.take(")")
                return ("call", v, args)
            if self.peek()[1] == ".":
                self.take()
                fk, fv = self.take()
                if fk != "name":
                    raise RuleError(f"`{self.src}`: field name expected after `.`")
                return self.atom(v, fv)
            return self.atom(v, None)
        raise RuleError(f"`{self.src}`: unexpected `{v or 'end'}`")


def _typeof(e, uses):
    """Type of a typed expression tree (raises RuleError); trees carry their type at [1]"""
    k = e[0]
    if k in ("lit", "int", "ref", "state", "mode", "guard"):
        return e[1]
    if k in ("or", "and"):
        if _typeof(e[1], uses) != "bool" or _typeof(e[2], uses) != "bool":
            raise RuleError(f"`{k}` needs booleans")
        return "bool"
    if k == "not":
        if _typeof(e[1], uses) != "bool":
            raise RuleError("`not` needs a boolean")
        return "bool"
    if k == "cmp":
        a, b = _typeof(e[2], uses), _typeof(e[3], uses)
        if a != b or a.startswith("record"):
            raise RuleError(f"`{e[1]}` compares {a} with {b}")
        if e[1] not in ("==", "!=") and a not in ("integer", "time"):
            raise RuleError(f"`{e[1]}` orders integers or times, not {a}")
        return "bool"
    if k == "++":
        if _typeof(e[1], uses) != "bytes" or _typeof(e[2], uses) != "bytes":
            raise RuleError("`++` joins bytes")
        return "bytes"
    if k == "call":
        name, args = e[1], e[2]
        ts = [_typeof(a, uses) for a in args]
        want = {"len": None, "prefix": ["bytes", "integer"], "utf8": ["text"],
                "hmac_sha256": ["bytes", "bytes"]}[name]
        if name == "len":
            if len(ts) != 1 or ts[0] not in ("text", "bytes"):
                raise RuleError("`len` takes one text or bytes")
            return "integer"
        if ts != want:
            raise RuleError(f"`{name}` takes {', '.join(want)}, got {', '.join(ts)}")
        if name == "hmac_sha256" and "hmac_sha256" not in uses:
            raise RuleError("`hmac_sha256` is called without `use alice-sign hmac_sha256`")
        return "bytes"
    raise RuleError(f"unknown node {k}")


# ------------------------------------------------------------------ types of values

def _parse_type(src: str):
    src = src.strip()
    if src in SCALARS:
        return src
    m = re.fullmatch(r"record\((.*)\)", src)
    if not m:
        raise RuleError(f"type `{src}`")
    fields, seen = [], set()
    for part in m.group(1).split(","):
        name, sep, ty = part.partition(":")
        name, ty = name.strip(), ty.strip()
        if not sep or not NAME.match(name) or name in seen or ty not in SCALARS:
            raise RuleError(f"record field `{part.strip()}`")
        seen.add(name)
        fields.append((name, ty))
    if not fields:
        raise RuleError("a record has at least one field")
    return ("record", fields)


def _type_name(ty):
    return ty if isinstance(ty, str) else "record"


def _has_type(v, ty) -> bool:
    if ty in ("integer", "time"):
        return (not isinstance(v, bool) and isinstance(v, (int, float))
                and v == v and abs(v) != float("inf") and float(v).is_integer() and abs(v) <= MAX_INT)
    if ty == "text":
        return isinstance(v, str)
    if ty == "bytes":
        return isinstance(v, str) and bool(HEX.match(v))
    if ty == "bool":
        return isinstance(v, bool)
    _, fields = ty
    return isinstance(v, dict) and all(f in v and _has_type(v[f], t) for f, t in fields)


def _value(v, ty):
    if ty in ("integer", "time"):
        return int(v)
    if ty == "bytes":
        return bytes.fromhex(v)
    if isinstance(ty, tuple):
        return {f: _value(v[f], t) for f, t in ty[1]}
    return v


def _const(ty, lit):
    toks = _tokens(lit)
    if ty == "bool" and toks in ([("name", "true")], [("name", "false")]):
        return toks[0][1] == "true"
    if len(toks) != 1:
        raise RuleError(f"`{lit}` is not one literal")
    k, tok = toks[0]
    want = {"integer": "int", "time": "int", "text": "text", "bytes": "bytes"}.get(ty)
    if k != want:
        raise RuleError(f"`{lit}` is not a literal of type {ty}")
    return _literal(k, tok)[2]


# ------------------------------------------------------------------ reading

def parse(text: str) -> dict:
    kind = rule_kind(text)
    if kind is None:
        raise RuleError("not a rule law")
    law = {"kind": kind, "consts": {}, "names": set(), "uses": set()}
    lines = _lines(text)
    kinds = [n for n, ln in lines if ln.split()[0] == "kind"]
    if len(kinds) != 1:
        raise RuleError("a rule law file has exactly one `kind` line")

    def declare(name, what):
        if not NAME.match(name) or name in RESERVED:
            raise RuleError(f"`{name}` is not a name ({what})")
        if name in law["names"]:
            raise RuleError(f"`{name}` is declared twice")
        law["names"].add(name)

    (_machine if kind == "machine" else _predicate)(law, lines, declare)
    del law["names"]
    return law


def _const_line(law, rest, declare):
    m = re.fullmatch(r"\s*(\S+)\s+(\S+)\s*=\s*(.*?)\s*", rest)
    if not m or m.group(2) not in SCALARS:
        raise RuleError(f"`const {rest.strip()}`: `const <name> <type> = <literal>` expected")
    name, ty, lit = m.groups()
    declare(name, "constant")
    law["consts"][name] = (ty, _const(ty, lit))


def _machine(law, lines, declare):
    law.update(modes=None, states=[], initial=None, events={}, on=[], allow=[])
    pending = []   # expressions read after every declaration
    for n, ln in lines:
        w = ln.split()
        head, rest = w[0], ln.split(None, 1)[1] if len(w) > 1 else ""
        if head in PEOPLE or head == "kind":
            continue
        if head == "const":
            _const_line(law, rest, declare)
        elif head == "mode":
            if law["modes"] is not None or len(w) < 2:
                raise RuleError(f"line {n}: at most one `mode <value>...` line")
            for v in w[1:]:
                declare(v, "mode value")
            law["modes"] = w[1:]
        elif head == "state":
            if len(w) == 3 and w[2] == "initial":
                if law["initial"] is not None:
                    raise RuleError(f"line {n}: a second initial state")
                law["initial"] = w[1]
            elif len(w) != 2:
                raise RuleError(f"line {n}: `state <name> [initial]`")
            declare(w[1], "state")
            law["states"].append(w[1])
        elif head == "event":
            if len(w) < 2 or (len(w) > 2 and (w[2] != "guard" or len(w) < 4)):
                raise RuleError(f"line {n}: `event <name> [guard <guard>...]`")
            declare(w[1], "event")
            for g in w[3:]:
                declare(g, "guard")
            law["events"][w[1]] = w[3:]
        elif head == "on":
            m = re.fullmatch(r"on\s+(\S+)\s+from\s+(.+?)\s+to\s+(\S+)(?:\s+when\s+(.+?))?\s*", ln.strip())
            if not m:
                raise RuleError(f"line {n}: `on <event> from <state>... to <state> [when <expression>]`")
            ev, frm, to, when = m.groups()
            pending.append(("on", n, ev, frm.split(), to, when))
        elif head == "allow":
            m = re.fullmatch(r"allow\s+(\S+)\s+when\s+(.+?)\s*", ln.strip())
            if not m:
                raise RuleError(f"line {n}: `allow <name> when <expression>`")
            declare(m.group(1), "allowance")
            pending.append(("allow", n, m.group(1), m.group(2)))
        else:
            raise RuleError(f"line {n}: `{head}` is not a line of a machine law")
    if not law["states"] or not law["events"]:
        raise RuleError("a machine law has at least one state and one event")
    if law["initial"] is None:
        raise RuleError("a machine law has exactly one initial state")
    states, modes = set(law["states"]), set(law["modes"] or [])

    def atom_for(guards):
        def atom(name, field):
            if field is not None:
                raise RuleError(f"`{name}.{field}`: a machine law has no records")
            if name in guards:
                return ("guard", "bool", name)
            if name in law["consts"] and law["consts"][name][0] == "bool":
                return ("lit", "bool", law["consts"][name][1])
            raise RuleError(f"`{name}` is not a guard of the event or a boolean constant here")
        return atom

    def expr(src, guards):
        p = _Parser(src, atom_for(guards))
        # `state == s` / `mode == v`: rewrite the two words into nodes before parsing
        e = _machine_expr(p, states, modes, law["modes"] is not None)
        _typeof(e, set())
        if _typeof(e, set()) != "bool":
            raise RuleError(f"`{src}` is not a boolean")
        return e

    for item in pending:
        if item[0] == "on":
            _, n, ev, frm, to, when = item
            if ev not in law["events"] or to not in states or not frm or any(s not in states for s in frm):
                raise RuleError(f"line {n}: an `on` line names an undeclared event or state")
            law["on"].append((ev, frm, to, None if when is None else expr(when, law["events"][ev])))
        else:
            _, n, name, src = item
            law["allow"].append((name, expr(src, [])))
    if not law["on"]:
        raise RuleError("a machine law has at least one `on` line")


def _machine_expr(p, states, modes, has_mode):
    """Parses a machine expression: `state ==|!= <state>` and `mode ==|!= <value>` are atoms"""
    orig = p.primary

    def primary():
        k, v = p.peek()
        if k == "name" and v in ("state", "mode") and p.peek(1)[1] in ("==", "!="):
            p.take()
            op = p.take()[1]
            nk, nv = p.take()
            if v == "state" and (nk != "name" or nv not in states):
                raise RuleError(f"`state {op} {nv}`: not a state")
            if v == "mode" and (not has_mode or nk != "name" or nv not in modes):
                raise RuleError(f"`mode {op} {nv}`: not a mode value")
            node = (v, "bool", nv)
            return ("not", node) if op == "!=" else node
        return orig()

    p.primary = primary
    e = p.parse()
    if _contains_order(e):
        raise RuleError(f"`{p.src}`: a machine expression has no comparisons other than state / mode")
    return e


def _contains_order(e):
    if e[0] in ("cmp", "++", "call", "int") or (e[0] == "lit" and e[1] != "bool"):
        return True
    return any(_contains_order(x) for x in e[1:] if isinstance(x, tuple))


def _predicate(law, lines, declare):
    law.update(inputs=[], steps=[], requires=[], example=None)
    types = {}
    for name, (ty, _) in law["consts"].items():
        types[name] = ty
    for n, ln in lines:
        w = ln.split()
        head = w[0]
        if head in PEOPLE or head == "kind":
            continue
        if head.lower().startswith("x-") or ln.lstrip().lower().startswith("x-"):
            m = re.match(r"(x-input|x-split|x-bind) ([^ \t].*)\Z", ln.rstrip(" \t"))
            if not m or ln[0] in " \t":
                raise RuleError(f"line {n}: `{ln.strip()}` is not an x- line of a predicate law")
            kw, rest = m.groups()
            if kw == "x-input":
                name, _, ty = rest.strip().partition(" ")
                declare(name, "input")
                ty = _parse_type(ty)
                types[name] = ty
                law["inputs"].append((name, ty))
            elif kw == "x-split":
                m2 = re.fullmatch(r"\s*(\S+)\s+(\"[^\"\\]*\")\s+(.+?)\s*", rest)
                if not m2:
                    raise RuleError(f"line {n}: `x-split <source> \"<separator>\" <name>...`")
                src, sep, names = m2.group(1), m2.group(2)[1:-1], m2.group(3).split()
                if types.get(src) != "text" or not sep or len(names) < 2:
                    raise RuleError(f"line {n}: a split takes a text, a separator that is not empty and two names or more")
                for nm in names:
                    declare(nm, "part")
                    types[nm] = "text"
                law["steps"].append(("split", src, sep, names))
            else:
                p = rest.split()
                if len(p) != 3 or p[2] not in ("integer", "time", "hex") or types.get(p[1]) != "text":
                    raise RuleError(f"line {n}: `x-bind <name> <part> integer|time|hex`")
                declare(p[0], "bound part")
                types[p[0]] = "bytes" if p[2] == "hex" else p[2]
                law["steps"].append(("bind", p[0], p[1], p[2]))
        elif head == "const":
            _const_line(law, ln.split(None, 1)[1] if len(w) > 1 else "", declare)
            nm = ln.split()[1]
            types[nm] = law["consts"][nm][0]
        elif head == "use":
            if w[1:] != ["alice-sign", "hmac_sha256"]:
                raise RuleError(f"line {n}: the only call is `use alice-sign hmac_sha256`")
            law["uses"].add("hmac_sha256")
        elif head == "require":
            m = re.fullmatch(r"require\s+([a-z][a-z0-9_]*):\s+(.+?)\s*", ln.strip())
            if not m:
                raise RuleError(f"line {n}: `require <label>: <expression>`")
            label, src = m.groups()
            # labels have a namespace of their own (TASK.md: a label names a condition)
            if label in RESERVED or label in (lb for lb, _ in law["requires"]):
                raise RuleError(f"line {n}: label `{label}` is reserved or used twice")

            def atom(name, field, types=types):
                if name not in types:
                    raise RuleError(f"`{name}` is not declared on an earlier line")
                ty = types[name]
                if field is None:
                    if name in law["consts"]:
                        return ("lit", ty, law["consts"][name][1])
                    return ("ref", _type_name(ty), name, None)
                if not isinstance(ty, tuple) or field not in dict(ty[1]):
                    raise RuleError(f"`{name}.{field}`: no such field")
                return ("ref", dict(ty[1])[field], name, field)
            e = _Parser(src, atom).parse()
            if _typeof(e, law["uses"]) != "bool":
                raise RuleError(f"line {n}: `{src}` is not a boolean")
            law["requires"].append((label, e))
        elif head == "example":
            try:
                ex = json.loads(ln.split(None, 1)[1])
            except (ValueError, IndexError):
                raise RuleError(f"line {n}: `example <JSON object>`")
            if not isinstance(ex, dict) or law["example"] is not None:
                raise RuleError(f"line {n}: one `example <JSON object>` line")
            law["example"] = ex
        else:
            raise RuleError(f"line {n}: `{head}` is not a line of a predicate law")
    if not law["requires"] or law["example"] is None:
        raise RuleError("a predicate law has at least one `require` line and one `example` line")
    if respond(law, law["example"]) != ("outputs", {"valid": True, "failed": None}):
        raise RuleError("the example does not hold")


# ------------------------------------------------------------------ answering

def respond(law, inputs, bug=None):
    """The answer to a request whose `inputs` is the given object (None: no inputs key)"""
    inputs = {} if inputs is None else inputs
    return (_machine_step if law["kind"] == "machine" else _predicate_answer)(law, inputs, bug)


def _machine_step(law, inp, bug):
    keys = ["state", "event"] + (["mode"] if law["modes"] is not None else [])
    for k in keys:
        if k not in inp:
            return ("request_error", f"{k} missing")
    state, event = inp["state"], inp["event"]
    if not isinstance(state, str) or state not in law["states"]:
        return ("rejected", "unknown state")
    if not isinstance(event, str) or event not in law["events"]:
        return ("rejected", "unknown event")
    mode = inp.get("mode")
    if law["modes"] is not None and (not isinstance(mode, str) or mode not in law["modes"]):
        return ("rejected", "unknown mode")
    guards = inp.get("guards", {})
    if not isinstance(guards, dict):
        return ("rejected", "guards is not an object")
    g = {}
    for name in law["events"][event]:
        if not isinstance(guards.get(name), bool):
            return ("rejected", f"guard {name} missing or not a boolean")
        g[name] = guards[name]

    def ev(e, st):
        k = e[0]
        if k == "lit":
            return e[2]
        if k == "guard":
            return g[e[2]]
        if k == "state":
            return st == e[2]
        if k == "mode":
            return mode == e[2]
        if k == "not":
            return not ev(e[1], st)
        if k == "and":
            return ev(e[1], st) and ev(e[2], st)
        if k == "or":
            return ev(e[1], st) or ev(e[2], st)
        raise ValueError(k)

    new, taken = state, False
    for on_ev, frm, to, when in law["on"]:
        if on_ev == event and state in frm and (when is None or ev(when, state)):
            new, taken = to, True
            if bug != "machine_last_match":
                break
    after = state if bug == "machine_allow_before_step" else new
    return ("outputs", {"state": new, "transition": "taken" if taken else "refused",
                        "allow": {name: ev(e, after) for name, e in law["allow"]}})


def _parse_int_text(s):
    if not INT_TEXT.match(s):
        return None
    v = int(s)
    return v if abs(v) <= MAX_INT else None


def _predicate_answer(law, inp, bug):
    for name, _ in law["inputs"]:
        if name not in inp:
            return ("request_error", f"{name} missing")
    env = {}
    for name, ty in law["inputs"]:
        if not _has_type(inp[name], ty):
            return ("rejected", f"{name} does not have its type")
        env[name] = _value(inp[name], ty)
    shape = ("outputs", {"valid": False, "failed": "shape"})
    for st in law["steps"]:
        if st[0] == "split":
            _, src, sep, names = st
            parts = env[src].split(sep)
            if len(parts) != len(names):
                return shape
            env.update(zip(names, parts))
        else:
            _, name, part, ty = st
            if ty == "hex":
                if not HEX.match(env[part]):
                    return shape
                env[name] = bytes.fromhex(env[part])
            else:
                v = _parse_int_text(env[part])
                if v is None:
                    return shape
                env[name] = v
    order = law["requires"][::-1] if bug == "predicate_reverse_order" else law["requires"]
    for label, e in order:
        if not _eval(e, env, bug):
            return ("outputs", {"valid": False, "failed": label})
    return ("outputs", {"valid": True, "failed": None})


def _eval(e, env, bug=None):
    k = e[0]
    if k in ("lit", "int"):
        return e[2]
    if k == "ref":
        v = env[e[2]]
        return v if e[3] is None else v[e[3]]
    if k == "not":
        return not _eval(e[1], env, bug)
    if k == "and":
        return _eval(e[1], env, bug) and _eval(e[2], env, bug)
    if k == "or":
        return _eval(e[1], env, bug) or _eval(e[2], env, bug)
    if k == "++":
        return _eval(e[1], env, bug) + _eval(e[2], env, bug)
    if k == "cmp":
        a, b = _eval(e[2], env, bug), _eval(e[3], env, bug)
        op = e[1]
        if bug == "predicate_lt_inclusive" and op == "<":
            op = "<="
        return {"==": a == b, "!=": a != b, "<": a < b, "<=": a <= b, ">": a > b, ">=": a >= b}[op]
    if k == "call":
        args = [_eval(a, env, bug) for a in e[2]]
        if e[1] == "len":
            return len(args[0].encode("utf-8")) if isinstance(args[0], str) else len(args[0])
        if e[1] == "prefix":
            return args[0][:max(0, args[1])]
        if e[1] == "utf8":
            return args[0].encode("utf-8")
        if e[1] == "hmac_sha256":
            return hmac.new(args[0], args[1], hashlib.sha256).digest()
    raise ValueError(k)


# ------------------------------------------------------------------ corpus

def _vec(name, inputs, note, answer):
    v = {"law": name, "inputs": inputs, "note": note, "tolerance": None}
    kind, val = answer
    if kind == "outputs":
        v.update(kind="exact", expected=val)
    elif kind == "rejected":
        v.update(kind="reject", expected=None)
    else:
        v.update(kind="request_error", expected=None, request_inputs=inputs)
    return v


def vectors(law, name):
    """The conformance vectors of a rule law: every answer is computed by `respond`"""
    return (_machine_vectors if law["kind"] == "machine" else _predicate_vectors)(law, name)


def _guard_sets(guards):
    out = [{}]
    for g in guards:
        out = [{**o, g: b} for o in out for b in (False, True)]
    return out


def _machine_vectors(law, name):
    V = []
    modes = law["modes"] or [None]
    for mode in modes:
        for state in law["states"]:
            for event, guards in law["events"].items():
                for gs in _guard_sets(guards):
                    inp = {"state": state, "event": event, "guards": gs}
                    if mode is not None:
                        inp["mode"] = mode
                    V.append(_vec(name, inp, f"step {mode} {state} {event} {gs}", respond(law, inp)))
    base = dict(V[0]["inputs"])
    ev_with_guard = next((e for e, g in law["events"].items() if g), None)
    rej = [({**base, "state": "no_such_state"}, "unknown state"),
           ({**base, "state": 1}, "state not text"),
           ({**base, "event": "no_such_event"}, "unknown event"),
           ({**base, "guards": []}, "guards not an object")]
    if law["modes"] is not None:
        rej.append(({**base, "mode": "no_such_mode"}, "unknown mode"))
    if ev_with_guard:
        g = law["events"][ev_with_guard][0]
        rej.append(({**base, "event": ev_with_guard, "guards": {}}, f"guard {g} missing"))
        rej.append(({**base, "event": ev_with_guard, "guards": {g: 1}}, f"guard {g} not a boolean"))
    else:
        nog = {k: v for k, v in base.items() if k != "guards"}
        V.append(_vec(name, nog, "no guards key", respond(law, nog)))
    for inp, note in rej:
        V.append(_vec(name, inp, note, respond(law, inp)))
    for k in ["state", "event"] + (["mode"] if law["modes"] is not None else []):
        inp = {kk: vv for kk, vv in base.items() if kk != k}
        V.append(_vec(name, inp, f"{k} missing", respond(law, inp)))
    return V


def _split_tree(law, ex):
    """The parts of the example: {name: text} for every split and bound part"""
    env = {n: ex[n] for n, _ in law["inputs"]}
    for st in law["steps"]:
        if st[0] == "split":
            env.update(zip(st[3], env[st[1]].split(st[2])))
    return env


def _render(law, env, top):
    """Re-joins the text input `top` from its (possibly changed) parts"""
    splits = {st[1]: st for st in law["steps"] if st[0] == "split"}

    def r(name):
        if name in splits and all(p in env for p in splits[name][3]):
            st = splits[name]
            return st[2].join(r(p) for p in st[3])
        return env[name]
    return r(top)


def _predicate_vectors(law, name):
    ex = law["example"]
    V = [_vec(name, ex, "example", respond(law, ex))]
    cands = []
    # boundaries: a comparison whose one side is an input (or a field of one) takes the value
    # of the other side, one less and one more, with everything else as in the example
    env = _example_env(law, ex)
    for label, e in law["requires"]:
        for node in _comparisons(e):
            _, op, a, b = node
            for side, other in ((a, b), (b, a)):
                if side[0] == "ref" and side[2] in ex and side[1] in ("integer", "time"):
                    val = _eval(other, env)
                    for d in (-1, 0, 1):
                        inp = json.loads(json.dumps(ex))
                        if side[3] is None:
                            inp[side[2]] = val + d
                        else:
                            inp[side[2]][side[3]] = val + d
                        cands.append((inp, f"{label}: {side[2]}{'.' + side[3] if side[3] else ''} = {val + d}"))
    # equality with a text, bytes or boolean input: the input takes the value of the other side
    for label, e in law["requires"]:
        for node in _comparisons(e):
            _, op, a, b = node
            for side, other in ((a, b), (b, a)):
                if side[0] == "ref" and side[2] in ex and side[1] in ("text", "bytes", "bool") and op in ("==", "!="):
                    val = _eval(other, env)
                    val = val.hex() if isinstance(val, bytes) else val
                    inp = json.loads(json.dumps(ex))
                    if side[3] is None:
                        inp[side[2]] = val
                    else:
                        inp[side[2]][side[3]] = val
                    cands.append((inp, f"{label}: {side[2]}{'.' + side[3] if side[3] else ''} = {val!r}"))
    # lengths: a comparison of `len(<text input>)` with a number gets texts of that many bytes,
    # one less and one more, written with two-byte characters where they fit (bytes, not characters)
    for label, e in law["requires"]:
        for node in _comparisons(e):
            _, op, a, b = node
            for side, other in ((a, b), (b, a)):
                if (side[0] == "call" and side[1] == "len" and side[2][0][0] == "ref"
                        and side[2][0][3] is None and side[2][0][1] == "text" and side[2][0][2] in ex):
                    k = _eval(other, env)
                    for n in (k - 1, k, k + 1):
                        if n >= 0:
                            cands.append(({**ex, side[2][0][2]: "\u00e9" * (n // 2) + "a" * (n % 2)},
                                          f"{label}: {side[2][0][2]} of {n} bytes"))
    # integer inputs at the largest magnitude a rule law reads, and one beyond
    for n, ty in law["inputs"]:
        fields = [(None, ty)] if ty in ("integer", "time") else (
            [(f, t) for f, t in ty[1] if t in ("integer", "time")] if isinstance(ty, tuple) else [])
        for f, _ in fields:
            for v in (MAX_INT, -MAX_INT, MAX_INT + 1, -MAX_INT - 1):
                inp = json.loads(json.dumps(ex))
                if f is None:
                    inp[n] = v
                else:
                    inp[n][f] = v
                cands.append((inp, f"{n}{'.' + f if f else ''} = {v}"))
    # byte inputs and hex parts: a bit flipped at either end, the last byte dropped, empty
    parts = _split_tree(law, ex)
    tops = [n for n, ty in law["inputs"] if ty == "text" and any(st[0] == "split" and st[1] == n for st in law["steps"])]
    for n, ty in law["inputs"]:
        if ty == "bytes":
            for mutated, note in _byte_mutations(bytes.fromhex(ex[n])):
                cands.append(({**ex, n: mutated.hex()}, f"{n}: {note}"))
    for st in law["steps"]:
        if st[0] != "bind":
            continue
        _, bname, part, ty = st
        top = next((t for t in tops if part in parts), None)
        if top is None:
            continue
        muts = []
        if ty == "hex":
            muts += [(m.hex(), note) for m, note in _byte_mutations(bytes.fromhex(parts[part]))]
            muts += [(parts[part].upper(), "upper-case hex"), (parts[part] + "0", "odd length")]
        else:
            v = int(parts[part])
            muts += [(str(v + 1), "plus one"), (str(v - 1), "minus one"),
                     ("0" + parts[part], "leading zero"), ("+" + parts[part], "plus sign"),
                     ("x", "not a number"), (str(MAX_INT), "the largest integer"), (str(MAX_INT + 1), "above 2^53 - 1")]
        for text, note in muts:
            env2 = {**parts, part: text}
            cands.append(({**ex, top: _render(law, env2, top)}, f"{part}: {note}"))
    # shape: each split with one part dropped, one separator more, the separator removed
    for st in law["steps"]:
        if st[0] != "split":
            continue
        src, sep, names = st[1], st[2], st[3]
        top = src if src in ex else next((t for t in tops if src in parts), None)
        if top is None:
            continue
        joined = sep.join(parts[p] for p in names)
        for text, note in [(sep.join(parts[p] for p in names[:-1]), "a part dropped"),
                           (joined + sep, "a separator more"),
                           (joined.replace(sep, "", 1), "a separator removed")]:
            env2 = {**parts, src: text}
            for p in names:
                env2.pop(p, None)
            cands.append(({**ex, top: _render(law, env2, top)}, f"{src}: {note}"))
    seen = {json.dumps(ex, sort_keys=True)}
    for inp, note in cands:
        key = json.dumps(inp, sort_keys=True)
        if key in seen:
            continue
        seen.add(key)
        V.append(_vec(name, inp, note, respond(law, inp)))
    # types and missing inputs
    for n, ty in law["inputs"]:
        wrong = {"integer": 1.5, "time": "1", "text": 1, "bytes": "zz", "bool": 1}.get(_type_name(ty), [])
        V.append(_vec(name, {**ex, n: wrong}, f"{n} of another type", respond(law, {**ex, n: wrong})))
        if isinstance(ty, tuple):
            f = ty[1][0][0]
            inp = {**ex, n: {k: v for k, v in ex[n].items() if k != f}}
            V.append(_vec(name, inp, f"{n} without field {f}", respond(law, inp)))
        inp = {k: v for k, v in ex.items() if k != n}
        V.append(_vec(name, inp, f"{n} missing", respond(law, inp)))
    return V


def _example_env(law, ex):
    env = {n: _value(ex[n], ty) for n, ty in law["inputs"]}
    for st in law["steps"]:
        if st[0] == "split":
            env.update(zip(st[3], env[st[1]].split(st[2])))
        elif st[3] == "hex":
            env[st[1]] = bytes.fromhex(env[st[2]])
        else:
            env[st[1]] = int(env[st[2]])
    return env


def _comparisons(e):
    if e[0] == "cmp":
        yield e
    for x in e[1:]:
        if isinstance(x, tuple):
            yield from _comparisons(x)
        elif isinstance(x, list):
            for y in x:
                yield from _comparisons(y)


def _byte_mutations(b):
    out = []
    if b:
        out.append((bytes([b[0] ^ 0x80]) + b[1:], "first bit flipped"))
        out.append((b[:-1] + bytes([b[-1] ^ 0x01]), "last bit flipped"))
        out.append((b[:-1], "last byte dropped"))
        out.append((b"", "empty"))
    out.append((b + b"\x00", "a byte more"))
    return out


# ------------------------------------------------------------------ coverage

def coverage(law, vecs):
    """What a corpus must contain for a rule law: the names of the missing items"""
    answered = [v for v in vecs if v["kind"] == "exact"]
    missing = []
    if law["kind"] == "machine":
        for mode in law["modes"] or [None]:
            for state in law["states"]:
                for event, guards in law["events"].items():
                    for gs in _guard_sets(guards):
                        if not any(v["inputs"].get("state") == state and v["inputs"].get("event") == event
                                   and v["inputs"].get("mode") == mode and v["inputs"].get("guards") == gs
                                   for v in answered):
                            missing.append(f"step {mode} {state} {event} {gs}")
        if not any(o["expected"]["transition"] == "refused" for o in answered):
            missing.append("a refused step")
    else:
        failed = {v["expected"]["failed"] for v in answered}
        shapes = ["shape"] if law["steps"] else []
        for label in [lb for lb, _ in law["requires"]] + shapes + [None]:
            if label not in failed:
                missing.append(f"a vector failing {label}" if label else "a vector that holds")
    if not any(v["kind"] == "reject" for v in vecs):
        missing.append("a rejection")
    if not any(v["kind"] == "request_error" for v in vecs):
        missing.append("a request error")
    return missing
