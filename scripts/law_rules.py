#!/usr/bin/env python3
"""Rule laws (`kind machine` / `kind predicate` / `kind table`, conformance/TASK.md "Rule laws"): reading,
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
from pathlib import Path

MAX_INT = 2 ** 53 - 1  # every JSON reader holds it exactly (TASK.md "Rule laws")
NAME = re.compile(r"[a-z][a-z0-9_]*\Z")
FUNCTIONS = {"len", "prefix", "utf8", "hmac_sha256"}
RESERVED = {"true", "false", "and", "or", "not", "state", "mode", "shape"} | FUNCTIONS
PEOPLE = {"law", "claim", "source", "verdict", "oracle"}
SCALARS = {"integer", "time", "text", "bytes", "bool", "money"}
MONEY_MIN, MONEY_MAX = -(2 ** 127), 2 ** 127 - 1  # an amount of minor units fits an i128
MONEY = re.compile(r"(0|-?[1-9][0-9]*) ([A-Z]{3})\Z")
EXPONENT = re.compile(r"e[0-9]+\Z")
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
    """`machine` / `predicate` / `table` when the file is a rule law (no `kind audit` line), else None"""
    lines = [ln.split("#", 1)[0].strip() for ln in text.split("\n")]
    if "kind audit" in lines:
        return None
    for k in ("machine", "predicate", "table", "statute", "procedure"):
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
  | (?P<op>==|!=|<=|>=|\+\+|\+|-|<|>|\(|\)|,|\.)
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
        e = self.add()
        if self.peek()[1] in ("==", "!=", "<", "<=", ">", ">="):
            op = self.take()[1]
            e = ("cmp", op, e, self.add())
            if self.peek()[1] in ("==", "!=", "<", "<=", ">", ">="):
                raise RuleError(f"`{self.src}`: comparisons do not chain")
        return e

    def add(self):
        e = self.cat()
        while self.peek()[1] in ("+", "-"):
            op = self.take()[1]
            e = (op, e, self.cat())
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
        if e[1] not in ("==", "!=") and a not in ("integer", "time", "money"):
            raise RuleError(f"`{e[1]}` orders integers, times or money, not {a}")
        _same_currency(e[2], e[3])
        return "bool"
    if k in ("+", "-"):
        if _typeof(e[1], uses) != "money" or _typeof(e[2], uses) != "money":
            raise RuleError(f"`{k}` adds money")
        _same_currency(e[1], e[2])
        return "money"
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


class Money:
    """An amount of minor units and its currency; compared and added only within one currency"""
    __slots__ = ("amount", "currency")

    def __init__(self, amount: int, currency: str):
        self.amount, self.currency = amount, currency

    def __eq__(self, other):
        return isinstance(other, Money) and (self.amount, self.currency) == (other.amount, other.currency)

    def __hash__(self):
        return hash((self.amount, self.currency))

    def __repr__(self):
        return f"Money({self.amount}, {self.currency!r})"


class _Rejected(Exception):
    """A request rejected while evaluating (money of two currencies met, or an amount out of range)"""


def _money_text(v: str):
    m = MONEY.fullmatch(v) if isinstance(v, str) else None
    if not m:
        return None
    amount = int(m.group(1))
    if not MONEY_MIN <= amount <= MONEY_MAX:
        return None
    return Money(amount, m.group(2))


def _same_currency(a, b):
    """Two money constants of different currencies never meet (the law does not read)"""
    ca = a[2].currency if a[0] == "lit" and isinstance(a[2], Money) else None
    cb = b[2].currency if b[0] == "lit" and isinstance(b[2], Money) else None
    if ca and cb and ca != cb:
        raise RuleError(f"money of {ca} and of {cb} are compared or added")


def _has_type(v, ty, known=None) -> bool:
    if ty == "money":
        m = _money_text(v)
        return m is not None and known is not None and known(m.currency)
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
    return isinstance(v, dict) and all(f in v and _has_type(v[f], t, known) for f, t in fields)


def _value(v, ty):
    if ty == "money":
        return _money_text(v)
    if ty in ("integer", "time"):
        return int(v)
    if ty == "bytes":
        return bytes.fromhex(v)
    if isinstance(ty, tuple):
        return {f: _value(v[f], t) for f, t in ty[1]}
    return v


def _const(ty, lit):
    if ty == "money":
        m = _money_text(lit.strip())
        if m is None or "  " in lit.strip():
            raise RuleError(f"`{lit}` is not money (`<amount> <currency>`)")
        return m
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

HEADER = ("domain", "edition", "supersedes", "effective", "semantics")
EDITION_NAME = re.compile(r"[a-z][a-z0-9_-]*\Z")
ID_HEX = re.compile(r"[0-9a-f]{64}\Z")


def _time_literal(tok, n):
    if not re.fullmatch(r"0|[1-9][0-9]*", tok) or int(tok) > MAX_INT:
        raise RuleError(f"line {n}: `{tok}` is not a time (an integer literal, at most 2^53 - 1)")
    return int(tok)


def _kind_semantics(semantics, kind, money):
    """The semantics identifier a law of `kind`, with or without money, is evaluated under"""
    if kind in ("statute", "procedure"):
        return semantics[kind]
    return semantics[("money_" if money else "") + ("table" if kind == "table" else "rule")]


def _edition(lines, semantics, kind, money=False):
    """The five header lines (TASK.md "Editions of a rule set"): (edition or None, the other lines)"""
    found, rest = {}, []
    for n, ln in lines:
        w = ln.split()
        if w[0] not in HEADER:
            rest.append((n, ln))
            continue
        if w[0] in found:
            raise RuleError(f"line {n}: a second `{w[0]}` line")
        found[w[0]] = (n, w[1:])
    if not found:
        return None, rest
    missing = [h for h in HEADER if h not in found]
    if missing:
        raise RuleError(f"an edition has all five header lines; `{missing[0]}` is missing")
    ed = {}
    for key in ("domain", "edition"):
        n, w = found[key]
        if len(w) != 1 or not EDITION_NAME.match(w[0]):
            raise RuleError(f"line {n}: `{key} <name>` (a lower-case letter, then lower-case letters, digits, `_`, `-`)")
        ed["domain" if key == "domain" else "name"] = w[0]
    n, w = found["supersedes"]
    if len(w) != 1 or not (w[0] == "first" or ID_HEX.match(w[0])):
        raise RuleError(f"line {n}: `supersedes first` or `supersedes <64 lower-case hex>`")
    ed["supersedes"] = None if w[0] == "first" else bytes.fromhex(w[0])
    n, w = found["effective"]
    if len(w) not in (1, 3) or (len(w) == 3 and w[1] != "until"):
        raise RuleError(f"line {n}: `effective <time>` or `effective <time> until <time>`")
    ed["from"] = _time_literal(w[0], n)
    ed["until"] = _time_literal(w[2], n) if len(w) == 3 else None
    if ed["until"] is not None and ed["until"] <= ed["from"]:
        raise RuleError(f"line {n}: `until` is not after the start")
    n, w = found["semantics"]
    if len(w) != 1 or not ID_HEX.match(w[0]):
        raise RuleError(f"line {n}: `semantics <64 lower-case hex>`")
    ed["semantics"] = bytes.fromhex(w[0])
    if semantics is not None and ed["semantics"] != _kind_semantics(semantics, kind, money):
        raise RuleError(f"line {n}: written against the semantics {w[0]}, which this reader does not evaluate")
    return ed, rest


def _uses_money(law) -> bool:
    def has(ty):
        return ty == "money" or (isinstance(ty, tuple) and any(t == "money" for _, t in ty[1]))
    return any(ty == "money" for ty, _ in law["consts"].values()) or any(has(t) for _, t in law.get("inputs", []))


def _bind_currencies(law, cline, currencies, semantics):
    """Checks the `currencies` line against the law's money and the table given (TASK.md)"""
    money = _uses_money(law)
    if not money:
        if cline is not None:
            raise RuleError(f"line {cline[0]}: a law without money has no `currencies` line")
        return
    if cline is None:
        raise RuleError("a law with money has one `currencies <id>` line")
    n, w = cline
    if len(w) != 1 or not ID_HEX.match(w[0]):
        raise RuleError(f"line {n}: `currencies <64 lower-case hex>`")
    law["currencies"] = bytes.fromhex(w[0])
    if currencies is None or semantics is None:
        raise RuleError("a law with money is read with its currency table and the semantics it is evaluated under")
    if currencies["kind"] != "table" or currencies["inputs"] != [("code", "text")]:
        raise RuleError("the currency table is a table law with the one input `x-input code text`")
    if law_id(currencies, semantics) != law["currencies"]:
        raise RuleError(f"line {n}: the law names another currency table than the one given")

    def known(code):
        return bool(EXPONENT.match(respond(currencies, {"code": code})[1]["value"]))
    law["known"] = known
    # the codes the table lists as known (the values its rows compare `code` with)
    law["codes"] = sorted(c for c in {inp["code"] for inp in _table_inputs(currencies)}
                          if MONEY.fullmatch(f"0 {c}") and known(c))
    for name, (ty, v) in law["consts"].items():
        if ty == "money" and not known(v.currency):
            raise RuleError(f"constant `{name}`: the currency table does not know {v.currency}")


def currencies_named(text: str):
    """The identifier a law file's `currencies` line names (bytes), or None without one"""
    for ln in text.splitlines():
        w = ln.split("#", 1)[0].split()
        if w[:1] == ["currencies"] and len(w) == 2 and ID_HEX.match(w[1]):
            return bytes.fromhex(w[1])
    return None


def parse_file(path, semantics: dict[str, bytes] | None = None) -> dict:
    """Reads the law file `path`; a law with money is read with its currency table, the rule law
    file of the same directory whose identifier its `currencies` line names (conformance/TASK.md
    "The currency table")"""
    path = Path(path)
    text = path.read_text(encoding="utf-8")
    want = currencies_named(text)
    if want is None:
        return parse(text)
    if semantics is None:
        raise RuleError(f"{path.name}: a law with money is read under the semantics identifiers")
    for f in sorted(path.parent.glob("*.law")):
        t = f.read_text(encoding="utf-8")
        if f == path or rule_kind(t) != "table" or currencies_named(t) is not None:
            continue
        try:
            table = parse(t, semantics)
        except RuleError:
            continue
        if law_id(table, semantics) == want:
            return parse(text, semantics, currencies=table)
    raise RuleError(f"{path.name}: no rule law file of its directory is the currency table {want.hex()}")


def parse(text: str, semantics: dict[str, bytes] | None = None, currencies: dict | None = None) -> dict:
    """A rule law; `semantics` ("rule" / "table" -> identifier), when given, is what this reader
    evaluates, and an edition written against another semantics does not read; `currencies` is the
    parsed currency table a law with money reads"""
    kind = rule_kind(text)
    if kind is None:
        raise RuleError("not a rule law")
    law = {"kind": kind, "consts": {}, "names": set(), "uses": set(), "currencies": None, "known": None}
    # a law with money is evaluated under the money semantics (MONEY_SEMANTICS_PINS)
    money = any(ln.split()[0] == "currencies" for _, ln in _lines(text))
    law["edition"], lines = _edition(_lines(text), semantics, kind, money)
    clines = [(n, ln.split()[1:]) for n, ln in lines if ln.split()[0] == "currencies"]
    if len(clines) > 1:
        raise RuleError(f"line {clines[1][0]}: a second `currencies` line")
    lines = [(n, ln) for n, ln in lines if ln.split()[0] != "currencies"]
    kinds = [n for n, ln in lines if ln.split()[0] == "kind"]
    if len(kinds) != 1:
        raise RuleError("a rule law file has exactly one `kind` line")

    def declare(name, what):
        if not NAME.match(name) or name in RESERVED:
            raise RuleError(f"`{name}` is not a name ({what})")
        if name in law["names"]:
            raise RuleError(f"`{name}` is declared twice")
        law["names"].add(name)

    {"machine": _machine, "predicate": _predicate, "table": _table, "statute": _statute,
     "procedure": _procedure}[kind](law, lines, declare)
    del law["names"]
    if kind in ("machine", "procedure") and _uses_money(law):
        raise RuleError(f"a {kind} law has no money")
    if kind == "statute" and clines:
        raise RuleError("a statute law has no money")
    _bind_currencies(law, clines[0] if clines else None, currencies, semantics)
    if kind == "predicate" and respond(law, law["example"]) != ("outputs", {"valid": True, "failed": None}):
        raise RuleError("the example does not hold")
    return law


# ALICE-Legal 9490cf2 src/procedure.rs: ProcedureStatus and StepKind, and the status add_step gives
PROCEDURE_STATUSES = ("pending", "in_progress", "approved", "rejected", "completed")
STEP_KINDS = ("application", "review", "approval", "rejection", "amendment", "notification", "completion")


def next_status(status: str, step: str) -> str:
    """The status a step leads to (Procedure::add_step): a decision or a completion sets it, any
    other step moves a pending procedure into progress and leaves the rest"""
    return {"approval": "approved", "rejection": "rejected", "completion": "completed"}.get(
        step, "in_progress" if status == "pending" else status)


def _procedure(law, lines, declare):
    """`kind procedure`: a machine whose states are a procedure's statuses (starting `pending`) and
    whose events are kinds of step, every `on` line going where ALICE-Legal's add_step goes"""
    _machine(law, lines, declare)
    for st in law["states"]:
        if st not in PROCEDURE_STATUSES:
            raise RuleError(f"`{st}` is not a status of a procedure")
    if law["initial"] != "pending":
        raise RuleError("a procedure starts `pending`")
    for ev in law["events"]:
        if ev not in STEP_KINDS:
            raise RuleError(f"`{ev}` is not a kind of step")
    for ev, frm, to, _ in law["on"]:
        for f in frm:
            if next_status(f, ev) != to:
                raise RuleError(f"`on {ev} from {f} to {to}`: a {ev} step from {f} leads to {next_status(f, ev)}")


CLAUSE_KINDS = ("obligation", "prohibition", "permission", "definition", "penalty", "exception")
LABEL = re.compile(r"[a-z][a-z0-9_]*\Z")
MAX_STATUTE_TIME = (2 ** 64 - 1) // 10 ** 9  # the nanoseconds of every statute time fit 64 bits


def _statute_time(tok, n):
    t = _time_literal(tok, n)
    if t > MAX_STATUTE_TIME:
        raise RuleError(f"line {n}: `{tok}` is after the latest time a statute names ({MAX_STATUTE_TIME})")
    return t


def _statute(law, lines, declare):
    """`kind statute`: one `title <text>` line and `clause <label> <kind> [of <clause>] [from <time>]
    [until <time>]: <text>` lines (conformance/TASK.md "Statute laws")"""
    start = 0 if law["edition"] is None else law["edition"]["from"]
    law["title"], law["clauses"] = None, []
    for n, ln in lines:
        w = ln.split()
        if w[0] in PEOPLE or w[0] == "kind":
            continue
        if w[0] == "title":
            if law["title"] is not None:
                raise RuleError(f"line {n}: a second `title` line")
            t = ln[len("title"):].strip(" \t")
            if not t:
                raise RuleError(f"line {n}: `title <text>`")
            law["title"] = t
        elif w[0] == "clause":
            if ":" not in ln:
                raise RuleError(f"line {n}: `clause <label> <kind> [of <clause>] [from <time>] [until <time>]: <text>`")
            head, text = ln.split(":", 1)
            text = text.strip(" \t")
            if not text:
                raise RuleError(f"line {n}: a clause has a text after the `:`")
            h = head.split()
            if len(h) < 3:
                raise RuleError(f"line {n}: a clause has a label and a kind")
            label, kind, rest = h[1], h[2], h[3:]
            if not LABEL.match(label):
                raise RuleError(f"line {n}: `{label}` is not a clause label")
            labels = [c["label"] for c in law["clauses"]]
            if label in labels:
                raise RuleError(f"line {n}: a second clause `{label}`")
            if kind not in CLAUSE_KINDS:
                raise RuleError(f"line {n}: `{kind}` is not a clause kind")
            parent = None
            if rest[:1] == ["of"] and len(rest) >= 2:
                if rest[1] not in labels:
                    raise RuleError(f"line {n}: `of {rest[1]}`: no earlier clause `{rest[1]}`")
                parent, rest = rest[1], rest[2:]
            frm = start
            if rest[:1] == ["from"] and len(rest) >= 2:
                frm, rest = _statute_time(rest[1], n), rest[2:]
            until = None
            if rest[:1] == ["until"] and len(rest) >= 2:
                until, rest = _statute_time(rest[1], n), rest[2:]
                if until <= frm:
                    raise RuleError(f"line {n}: `until` is not after the clause's start")
            if rest:
                raise RuleError(f"line {n}: `{' '.join(rest)}` is not part of a clause line")
            if kind == "exception" and parent is None:
                raise RuleError(f"line {n}: an exception is `of` the clause it carves out of")
            law["clauses"].append({"label": label, "kind": kind, "parent": parent, "from": frm,
                                   "until": until, "text": text})
        else:
            raise RuleError(f"line {n}: `{w[0]}` is not a line of a statute law")
    if law["title"] is None:
        raise RuleError("a statute law has one `title <text>` line")
    if not law["clauses"]:
        raise RuleError("a statute law has at least one `clause` line")


def _statute_answer(law, inp, bug):
    if "at" not in inp:
        return ("request_error", "at missing")
    if not _has_type(inp["at"], "time"):
        return ("rejected", "at does not have its type")
    at = int(inp["at"])
    return ("outputs", {"in_force": [c["label"] for c in law["clauses"]
                                     if c["from"] <= at and (c["until"] is None or at < c["until"])]})


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


TABLE_MAX_COMBINATIONS = 4096  # the vectors of a table law are every combination; more is an error


def _table(law, lines, declare):
    law.update(inputs=[], values=None, rows=[], otherwise=None)
    types = {}
    for n, ln in lines:
        w = ln.split()
        head = w[0]
        if head in PEOPLE or head == "kind":
            continue
        if ln.lstrip().lower().startswith("x-"):
            m = re.match(r"x-input ([^ \t].*)\Z", ln.rstrip(" \t"))
            if not m or ln[0] in " \t":
                raise RuleError(f"line {n}: `{ln.strip()}` is not an x- line of a table law")
            name, _, ty = m.group(1).strip().partition(" ")
            declare(name, "input")
            ty = _parse_type(ty)
            types[name] = ty
            law["inputs"].append((name, ty))
        elif head == "const":
            _const_line(law, ln.split(None, 1)[1] if len(w) > 1 else "", declare)
            types[w[1]] = law["consts"][w[1]][0]
        elif head == "value":
            if law["values"] is not None or len(w) < 3:
                raise RuleError(f"line {n}: one `value <name> <name>...` line, two values or more")
            for v in w[1:]:
                declare(v, "value")
            law["values"] = w[1:]
        elif head == "row":
            m = re.fullmatch(r"row\s+([a-z][a-z0-9_]*):\s+(\S+)\s+when\s+(.+?)\s*", ln.strip())
            if not m:
                raise RuleError(f"line {n}: `row <label>: <value> when <expression>`")
            label, value, src = m.groups()
            if law["otherwise"] is not None:
                raise RuleError(f"line {n}: a `row` line after the `otherwise` line")
            if label in RESERVED or label in (lb for lb, _, _ in law["rows"]):
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
            if _typeof(e, set()) != "bool":
                raise RuleError(f"line {n}: `{src}` is not a boolean")
            law["rows"].append((label, value, e))
        elif head == "otherwise":
            if len(w) != 2 or law["otherwise"] is not None:
                raise RuleError(f"line {n}: one `otherwise <value>` line")
            law["otherwise"] = w[1]
        else:
            raise RuleError(f"line {n}: `{head}` is not a line of a table law")
    if law["values"] is None or not law["rows"] or law["otherwise"] is None:
        raise RuleError("a table law has one `value` line, at least one `row` line and one `otherwise` line")
    given = {v for _, v, _ in law["rows"]} | {law["otherwise"]}
    stray = sorted(given - set(law["values"]))
    if stray:
        raise RuleError(f"`{stray[0]}` is not a value of the `value` line")
    unused = [v for v in law["values"] if v not in given]
    if unused:
        raise RuleError(f"value `{unused[0]}` is given by no row")


# ------------------------------------------------------------------ editions written as deltas

def _body(lines):
    """The body (TASK.md "A later edition written as a delta"): the lines that are read (not the header,
    not the lines for people), ends trimmed"""
    return [ln.rstrip(" \t") for _, ln in lines if ln.split()[0] not in HEADER and ln.split()[0] not in PEOPLE]


KEYED = ("const", "x-input", "x-bind", "allow", "state", "event")


def _key(line: str) -> str:
    w = line.split()
    if w[0] in ("row", "require") and len(w) > 1 and w[1].endswith(":"):
        return f"{w[0]} {w[1][:-1]}"
    if w[0] in KEYED + ("x-split", "clause") and len(w) > 1:
        return f"{w[0]} {w[1]}"
    if w[0] == "on" and "to" in w:
        return " ".join(w[:w.index("to")])
    return w[0]


def _header_lines(ed: dict) -> list[str]:
    sup = "first" if ed["supersedes"] is None else ed["supersedes"].hex()
    eff = f"{ed['from']}" + ("" if ed["until"] is None else f" until {ed['until']}")
    return [f"domain {ed['domain']}", f"edition {ed['name']}", f"supersedes {sup}", f"effective {eff}",
            f"semantics {ed['semantics'].hex()}"]


def _resolved(ed: dict, body: list[str]) -> str:
    keys = [_key(b) for b in body]
    dup = sorted({k for k in keys if keys.count(k) > 1})
    if dup:
        raise RuleError(f"two body lines have the key `{dup[0]}`")
    return "".join(line + "\n" for line in _header_lines(ed) + body)


def resolved_text(text: str, semantics: dict[str, bytes] | None = None, currencies: dict | None = None) -> str:
    """The resolved text of a full edition file"""
    law = parse(text, semantics, currencies)
    if law["edition"] is None:
        raise RuleError("not an edition (no header)")
    return _resolved(law["edition"], _body(_lines(text)))


def resolve(previous: str, delta: str, semantics: dict[str, bytes],
            currencies: dict | None = None) -> tuple[str, list[str]]:
    """The resolved text of `delta` over the resolved text `previous`, and its `changes` names
    (`currencies`: the currency table of a rule set with money)"""
    prev = parse(previous, semantics, currencies)
    if prev["edition"] is None:
        raise RuleError("the superseded file is not an edition")
    ed, ops = _edition(_lines(delta), semantics, prev["kind"], prev["currencies"] is not None)
    if ed is None:
        raise RuleError("a delta has the five header lines")
    if ed["domain"] != prev["edition"]["domain"]:
        raise RuleError(f"the delta is in domain {ed['domain']}, the superseded edition in {prev['edition']['domain']}")
    if ed["supersedes"] != law_id(prev, semantics):
        raise RuleError("`supersedes` is not the identifier of the superseded edition")
    body = _body(_lines(previous))
    changes = []

    def where(key, n):
        hits = [i for i, b in enumerate(body) if _key(b) == key]
        if not hits:
            raise RuleError(f"line {n}: no body line has the key `{key}`")
        return hits[0]

    for n, raw in ops:
        op = raw.strip()
        w = op.split()
        if w[0] == "changes":
            if len(w) != 2:
                raise RuleError(f"line {n}: `changes <name>`")
            changes.append(w[1])
        elif w[0] == "remove":
            del body[where(op.split(None, 1)[1].strip(), n)]
        elif w[0] == "replace":
            key, sep, line = op[len("replace"):].partition(":")
            key, line = key.strip(), line.strip(" \t")
            if not sep or not line or _key(line) != key:
                raise RuleError(f"line {n}: `replace <key>: <line>` with a line of the same key")
            body[where(key, n)] = line
        elif w[0] == "add":
            rest = op[len("add"):].strip()
            pos = None
            if rest.split()[0] in ("after", "before") and ":" in rest:
                side, _, tail = rest.partition(" ")
                key, _, line = tail.partition(":")
                key, line = key.strip(), line.strip(" \t")
                at = where(key, n)
                pos = at + 1 if side == "after" else at
            else:
                line = rest
            if not line or any(_key(b) == _key(line) for b in body):
                raise RuleError(f"line {n}: `add` of a key the body already has, or of nothing")
            if pos is None:
                same = [i for i, b in enumerate(body) if b.split()[0] == line.split()[0]]
                pos = same[-1] + 1 if same else len(body)
            body.insert(pos, line)
        else:
            raise RuleError(f"line {n}: `{w[0]}` is not an operation of a delta")
    text = _resolved(ed, body)
    parse(text, semantics, currencies)  # the resolved text reads as a rule law
    return text, changes


# ------------------------------------------------------------------ identifier

# The law identifier v2 of ALICE-Zip (`alice_zip::law_id`), written here from its layout and not
# from the Rust code: SHA-256 over len(domain) domain len(kind) kind semantics, then each value
# with a one-byte tag (0x01 bytes, 0x02 u32, 0x03 u64, 0x05 str, 0x06 id), every length and integer a
# big-endian u64 / u32. What a rule law hashes, and in which order, is conformance/TASK.md's
# "Rule laws" read as alice-lol `rule_law` hashes it; scripts/rule_law_ids.py records the values.
LAW_ID_DOMAIN_V2 = b"alice-law-id/v2"
LAW_KINDS = {"machine": b"lol.law.machine/v1", "predicate": b"lol.law.predicate/v1", "table": b"lol.law.table/v1",
             "statute": b"lol.law.statute/v1", "procedure": b"lol.law.procedure/v1"}


class _LawId:
    def __init__(self, kind: bytes, semantics: bytes):
        if len(semantics) != 32:
            raise ValueError("a semantics identifier is 32 bytes")
        self._h = hashlib.sha256()
        self._prefixed(LAW_ID_DOMAIN_V2)
        self._prefixed(kind)
        self._h.update(semantics)

    def _prefixed(self, b: bytes) -> None:
        self._h.update(len(b).to_bytes(8, "big"))
        self._h.update(b)

    def raw(self, b: bytes) -> "_LawId":
        self._h.update(b"\x01")
        self._prefixed(b)
        return self

    def u32(self, v: int) -> "_LawId":
        self._h.update(b"\x02" + v.to_bytes(4, "big"))
        return self

    def u64(self, v: int) -> "_LawId":
        self._h.update(b"\x03" + v.to_bytes(8, "big"))
        return self

    def ident(self, v: bytes) -> "_LawId":
        if len(v) != 32:
            raise ValueError("an identifier is 32 bytes")
        self._h.update(b"\x06" + v)
        return self

    def text(self, t: str) -> "_LawId":
        self._h.update(b"\x05")
        self._prefixed(t.encode("utf-8"))
        return self

    def finish(self) -> bytes:
        return self._h.digest()


def _hash_type(h, ty):
    if isinstance(ty, str):
        return h.text(ty)
    h = h.text("record").u64(len(ty[1]))
    for name, t in ty[1]:
        h = _hash_type(h.text(name), t)
    return h


def _hash_value(h, v):
    if isinstance(v, Money):  # the amount as 16 bytes, two's complement, big-endian (an i128)
        return h.text("money").raw(v.amount.to_bytes(16, "big", signed=True)).text(v.currency)
    # bool before int: a Python bool is an int
    if isinstance(v, bool):
        return h.text("bool").u32(int(v))
    if isinstance(v, int):
        return h.text("int").u64(v % 2 ** 64)  # the two's complement bits of an i64
    if isinstance(v, str):
        return h.text("text").text(v)
    if isinstance(v, bytes):
        return h.text("bytes").raw(v)
    raise RuleError(f"no identifier for the value {v!r}")


def _hash_expr(h, e):
    k = e[0]
    if k in ("lit", "int"):
        return _hash_value(h.text("lit"), e[2])
    if k == "ref":
        return h.text("ref").text(e[2]).text(e[3] or "").u32(int(e[3] is not None))
    if k in ("state", "mode", "guard"):
        return h.text(k).text(e[2])
    if k == "not":
        return _hash_expr(h.text("not"), e[1])
    if k in ("and", "or", "++", "+", "-"):
        return _hash_expr(_hash_expr(h.text(k), e[1]), e[2])
    if k == "cmp":
        return _hash_expr(_hash_expr(h.text("cmp").text(e[1]), e[2]), e[3])
    if k == "call":
        h = h.text("call").text(e[1]).u64(len(e[2]))
        for a in e[2]:
            h = _hash_expr(h, a)
        return h
    raise RuleError(f"no identifier for the expression {e!r}")


def _hash_consts(h, law):
    h = h.text("consts").u64(len(law["consts"]))
    for name, (ty, v) in law["consts"].items():
        h = _hash_value(_hash_type(h.text(name), ty), v)
    return h


def _hash_inputs(h, law):
    h = h.text("inputs").u64(len(law["inputs"]))
    for name, ty in law["inputs"]:
        h = _hash_type(h.text(name), ty)
    return h


def law_id(law: dict, semantics: dict[str, bytes]) -> bytes:
    """The v2 identifier of a parsed rule law; `semantics` maps "rule" and "table" to the
    semantics identifiers the laws are evaluated under"""
    kind = law["kind"]
    ed = law.get("edition")
    tag = LAW_KINDS[kind] if ed is None else f"lol.law.{ed['domain']}.{kind}/v1".encode()
    h = _LawId(tag, _kind_semantics(semantics, kind, law.get("currencies") is not None))
    if ed is not None:
        h = h.text("edition").text(ed["name"]).u32(int(ed["supersedes"] is not None))
        if ed["supersedes"] is not None:
            h = h.ident(ed["supersedes"])
        h = h.u64(ed["from"]).u32(int(ed["until"] is not None))
        if ed["until"] is not None:
            h = h.u64(ed["until"])
    if kind == "statute":
        h = h.text("title").text(law["title"]).text("clauses").u64(len(law["clauses"]))
        for c in law["clauses"]:
            h = h.text(c["label"]).text(c["kind"]).u32(int(c["parent"] is not None))
            if c["parent"] is not None:
                h = h.text(c["parent"])
            h = h.u64(c["from"]).u32(int(c["until"] is not None))
            if c["until"] is not None:
                h = h.u64(c["until"])
            h = h.text(c["text"])
        return h.finish()
    h = _hash_consts(h, law)
    if law.get("currencies") is not None:
        h = h.text("currencies").ident(law["currencies"])
    if kind in ("machine", "procedure"):
        h = h.text("modes").u32(int(law["modes"] is not None))
        for m in law["modes"] or []:
            h = h.text(m)
        h = h.text("states").u64(len(law["states"]))
        for s in law["states"]:
            h = h.text(s).u32(int(s == law["initial"]))
        h = h.text("events").u64(len(law["events"]))
        for ev, guards in law["events"].items():
            h = h.text(ev).u64(len(guards))
            for g in guards:
                h = h.text(g)
        h = h.text("on").u64(len(law["on"]))
        for ev, frm, to, when in law["on"]:
            h = h.text(ev).u64(len(frm))
            for f in frm:
                h = h.text(f)
            h = h.text(to).u32(int(when is not None))
            if when is not None:
                h = _hash_expr(h, when)
        h = h.text("allow").u64(len(law["allow"]))
        for name, e in law["allow"]:
            h = _hash_expr(h.text(name), e)
    elif kind == "predicate":
        h = _hash_inputs(h.text("use hmac_sha256").u32(int("hmac_sha256" in law["uses"])), law)
        h = h.text("steps").u64(len(law["steps"]))
        for step in law["steps"]:
            if step[0] == "split":
                _, src, sep, names = step
                h = h.text("split").text(src).text(sep).u64(len(names))
                for n in names:
                    h = h.text(n)
            else:
                _, name, part, as_ = step
                h = h.text("bind").text(name).text(part).text(as_)
        h = h.text("requires").u64(len(law["requires"]))
        for label, e in law["requires"]:
            h = _hash_expr(h.text(label), e)
    else:
        h = _hash_inputs(h, law)
        h = h.text("values").u64(len(law["values"]))
        for v in law["values"]:
            h = h.text(v)
        h = h.text("rows").u64(len(law["rows"]))
        for label, value, e in law["rows"]:
            h = _hash_expr(h.text(label).text(value), e)
        h = h.text("otherwise").text(law["otherwise"])
    return h.finish()


# ------------------------------------------------------------------ answering

def respond(law, inputs, bug=None):
    """The answer to a request whose `inputs` is the given object (None: no inputs key)"""
    inputs = {} if inputs is None else inputs
    return {"machine": _machine_step, "procedure": _machine_step, "predicate": _predicate_answer,
            "table": _table_answer, "statute": _statute_answer}[law["kind"]](law, inputs, bug)


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
        if not _has_type(inp[name], ty, law.get("known")):
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
    try:
        for label, e in order:
            if not _eval(e, env, bug):
                return ("outputs", {"valid": False, "failed": label})
    except _Rejected as why:
        return ("rejected", str(why))
    return ("outputs", {"valid": True, "failed": None})


def _table_answer(law, inp, bug):
    for name, _ in law["inputs"]:
        if name not in inp:
            return ("request_error", f"{name} missing")
    env = {}
    for name, ty in law["inputs"]:
        if not _has_type(inp[name], ty, law.get("known")):
            return ("rejected", f"{name} does not have its type")
        env[name] = _value(inp[name], ty)
    answer = {"value": law["otherwise"], "row": None}
    try:
        for label, value, e in law["rows"]:
            if _eval(e, env, bug):
                answer = {"value": value, "row": label}
                if bug != "table_last_match":
                    break
    except _Rejected as why:
        return ("rejected", str(why))
    return ("outputs", answer)


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
    if k in ("+", "-"):
        a, b = _eval(e[1], env, bug), _eval(e[2], env, bug)
        if a.currency != b.currency:
            raise _Rejected(f"money of {a.currency} and of {b.currency} meet")
        v = a.amount + b.amount if k == "+" else a.amount - b.amount
        if not MONEY_MIN <= v <= MONEY_MAX:
            raise _Rejected("an amount outside -2^127 to 2^127 - 1")
        return Money(v, a.currency)
    if k == "cmp":
        a, b = _eval(e[2], env, bug), _eval(e[3], env, bug)
        if isinstance(a, Money):
            if a.currency != b.currency:
                raise _Rejected(f"money of {a.currency} and of {b.currency} meet")
            a, b = a.amount, b.amount
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
    return {"machine": _machine_vectors, "procedure": _machine_vectors, "predicate": _predicate_vectors,
            "table": _table_vectors, "statute": _statute_vectors}[law["kind"]](law, name)


def _statute_times(law):
    """Each clause's start and end, one before each, and the ends of the range of a time"""
    ts = {0, -1, MAX_INT, -MAX_INT}
    for c in law["clauses"]:
        for t in (c["from"], c["until"]):
            if t is not None:
                ts |= {t - 1, t}
    return sorted(t for t in ts if abs(t) <= MAX_INT)


def _statute_vectors(law, name):
    V = [_vec(name, {"at": t}, f"at {t}", respond(law, {"at": t})) for t in _statute_times(law)]
    first = law["clauses"][0]["from"]
    for v, note in ((float(first), "at written as a number with a fraction of 0"), (first + 0.5, "at with a fraction"),
                    (str(first), "at as text"), (None, "at null"), (True, "at true"),
                    (MAX_INT + 1, "at beyond 2^53 - 1"), (-MAX_INT - 1, "at below -(2^53 - 1)")):
        V.append(_vec(name, {"at": v}, note, respond(law, {"at": v})))
    V.append(_vec(name, {}, "at missing", respond(law, {})))
    return V


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


def _is_constant(e):
    if e[0] == "ref":
        return False
    return all(_is_constant(x) for x in e[1:] if isinstance(x, tuple)) and \
        all(_is_constant(y) for x in e[1:] if isinstance(x, list) for y in x)


def _fresh(taken, make):
    """A value made by `make(k)` for the smallest k that is not in `taken`"""
    k = 0
    while make(k) in taken:
        k += 1
    return make(k)


def _money_json(v):
    return f"{v.amount} {v.currency}"


def _money_variants(law, base):
    """Each top-level money input of `base` at the ends of the range of an amount and one beyond,
    in each other currency the table knows, in a currency it does not know, and in forms that are
    not money (lower-case code, a plus sign, a leading zero, a JSON number)"""
    out = []
    codes = law.get("codes") or []
    for n, ty in law["inputs"]:
        if ty != "money":
            continue
        amount, cur = base[n].split(" ")
        for v in (MONEY_MAX, MONEY_MIN, MONEY_MAX + 1, MONEY_MIN - 1):
            out.append(({**base, n: f"{v} {cur}"}, f"{n} = {v} {cur}"))
        for c in codes:
            if c != cur:
                out.append(({**base, n: f"{amount} {c}"}, f"{n} in {c}"))
        unknown = _fresh(set(codes), lambda k: "Z" * 2 + "ZYXWVUTSRQPONMLKJIHGFEDCBA"[k])
        out.append(({**base, n: f"{amount} {unknown}"}, f"{n} in {unknown}, a currency the table does not know"))
        digits = amount.lstrip("-")
        out += [({**base, n: f"{amount} {cur.lower()}"}, f"{n}: lower-case code"),
                ({**base, n: f"+{digits} {cur}"}, f"{n}: plus sign"),
                ({**base, n: f"0{digits} {cur}"}, f"{n}: leading zero"),
                ({**base, n: f"{amount}  {cur}"}, f"{n}: two spaces"),
                ({**base, n: int(amount)}, f"{n}: a JSON number")]
    return out


def _table_inputs(law):
    """Every combination of the values the rows compare each input (or field) with

    An integer or time takes each value it is compared with, one less and one more; a text,
    bytes or boolean takes each value it is compared with, and a value it is never compared
    with; `len(x)` compared with n gives x of n - 1, n and n + 1 bytes. Two inputs compared with
    each other take each other's values too.
    """
    slots = []  # (input, field, type)
    for name, ty in law["inputs"]:
        if isinstance(ty, tuple):
            slots += [(name, f, t) for f, t in ty[1]]
        else:
            slots.append((name, None, ty))
    cand = {(n, f): set() for n, f, _ in slots}
    pairs = []
    for _, _, e in law["rows"]:
        for _, op, a, b in _comparisons(e):
            for side, other in ((a, b), (b, a)):
                if side[0] == "ref" and _is_constant(other):
                    v = _eval(other, {})
                    if side[1] in ("integer", "time"):
                        cand[(side[2], side[3])] |= {x for x in (v - 1, v, v + 1) if abs(x) <= MAX_INT}
                    elif side[1] == "money":
                        cand[(side[2], side[3])] |= {Money(x, v.currency) for x in (v.amount - 1, v.amount, v.amount + 1)
                                                     if MONEY_MIN <= x <= MONEY_MAX}
                    else:
                        cand[(side[2], side[3])].add(v)
                elif side[0] == "call" and side[1] == "len" and side[2][0][0] == "ref" and _is_constant(other):
                    ref, v = side[2][0], _eval(other, {})
                    mk = (lambda k: "a" * k) if ref[1] == "text" else (lambda k: b"a" * k)
                    cand[(ref[2], ref[3])] |= {mk(k) for k in (v - 1, v, v + 1) if 0 <= k <= 4096}
                elif side[0] == "ref" and other[0] == "ref":
                    pairs.append(((side[2], side[3]), (other[2], other[3])))
    for x, y in pairs:
        cand[x] |= cand[y]
    dims = []
    for n, f, ty in slots:
        c = cand[(n, f)]
        if ty in ("integer", "time"):
            c = c or {0}
        elif ty == "text":
            c = c | {"", _fresh(c, lambda k: "other" + "_" * k)}
        elif ty == "bytes":
            c = c | {b"", _fresh(c, lambda k: b"\xfe" * (k + 1))}
        elif ty == "money":
            c = c or {Money(0, law["codes"][0])}
        else:
            c = {False, True}
        key = (lambda v: (v.currency, v.amount)) if ty == "money" else (lambda v: (str(type(v)), v))
        dims.append((n, f, ty, sorted(c, key=key)))
    size = 1
    for d in dims:
        size *= len(d[3])
    if size > TABLE_MAX_COMBINATIONS:
        raise RuleError(f"a table law with {size} combinations of its compared values "
                        f"(more than {TABLE_MAX_COMBINATIONS}): split it")
    out = [{}]
    for n, f, ty, vals in dims:
        js = [v.hex() if isinstance(v, bytes) else _money_json(v) if isinstance(v, Money) else v for v in vals]
        nxt = []
        for o in out:
            for v in js:
                o2 = json.loads(json.dumps(o))
                if f is None:
                    o2[n] = v
                else:
                    o2.setdefault(n, {})[f] = v
                nxt.append(o2)
        out = nxt
    return out


def _table_vectors(law, name):
    V = []
    for inp in _table_inputs(law):
        V.append(_vec(name, inp, f"combination {json.dumps(inp, sort_keys=True)}", respond(law, inp)))
    reached = {v["expected"]["row"] for v in V}
    for label, _, _ in law["rows"]:
        if label not in reached:
            raise RuleError(f"row `{label}` is reached by no combination of the compared values "
                            f"(an earlier row decides every input that makes it hold)")
    base = V[0]["inputs"]
    other = {"integer": "1", "time": "1", "text": 1, "bytes": "zz", "bool": 1, "money": 1}
    for inp, note in _money_variants(law, base):
        V.append(_vec(name, inp, note, respond(law, inp)))
    for n, ty in law["inputs"]:
        bad = 1 if isinstance(ty, tuple) else other[ty]
        inp = {**base, n: bad}
        V.append(_vec(name, inp, f"{n} of another type", respond(law, inp)))
        if ty in ("integer", "time"):
            inp = {**base, n: float(base[n])}
            V.append(_vec(name, inp, f"{n} written as a number with a fraction of 0", respond(law, inp)))
        inp = {k: v for k, v in base.items() if k != n}
        V.append(_vec(name, inp, f"{n} missing", respond(law, inp)))
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
    # money: a comparison whose one side is a money input, or a sum / difference with a money
    # input first, takes the amount that makes the two sides equal, one less and one more
    for label, e in law["requires"]:
        for node in _comparisons(e):
            _, op, a, b = node
            for side, other in ((a, b), (b, a)):
                target = None
                try:
                    if side[0] == "ref" and side[1] == "money" and side[2] in ex:
                        ref, target = side, _eval(other, env).amount
                    elif side[0] in ("+", "-") and side[1][0] == "ref" and side[1][2] in ex:
                        ref, rest = side[1], _eval(side[2], env).amount
                        o = _eval(other, env).amount
                        target = o - rest if side[0] == "+" else o + rest
                except (_Rejected, AttributeError):
                    target = None
                if target is None:
                    continue
                cur = (ex[ref[2]] if ref[3] is None else ex[ref[2]][ref[3]]).split(" ")[1]
                for d in (-1, 0, 1):
                    if MONEY_MIN <= target + d <= MONEY_MAX:
                        inp = json.loads(json.dumps(ex))
                        v = f"{target + d} {cur}"
                        if ref[3] is None:
                            inp[ref[2]] = v
                        else:
                            inp[ref[2]][ref[3]] = v
                        cands.append((inp, f"{label}: {ref[2]}{'.' + ref[3] if ref[3] else ''} = {v}"))
    cands += _money_variants(law, ex)
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
        wrong = {"integer": 1.5, "time": "1", "text": 1, "bytes": "zz", "bool": 1, "money": 1}.get(_type_name(ty), [])
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
    if law["kind"] in ("machine", "procedure"):
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
    elif law["kind"] == "statute":
        # every clause both in force and not in force at some answered time
        for c in law["clauses"]:
            seen = {c["label"] in v["expected"]["in_force"] for v in answered}
            for state in (True, False):
                if state not in seen:
                    missing.append(f"a vector with {c['label']} {'in force' if state else 'not in force'}")
        # each start and end of a clause, and the time just before it
        times = {v["inputs"].get("at") for v in answered}
        for c in law["clauses"]:
            for t in (c["from"], c["until"]):
                if t is None:
                    continue
                for x in (t - 1, t):
                    if abs(x) <= MAX_INT and x not in times:
                        missing.append(f"a vector at {x} (a bound of {c['label']})")
    elif law["kind"] == "table":
        # every row (and the otherwise) that some combination of the compared values reaches
        want = {respond(law, inp)[1]["row"] for inp in _table_inputs(law)}
        got = {v["expected"]["row"] for v in answered}
        for row in sorted(want - got, key=str):
            missing.append(f"a vector decided by {row}" if row else "a vector that no row decides")
    else:
        failed = {v["expected"]["failed"] for v in answered}
        shapes = ["shape"] if law["steps"] else []
        for label in [lb for lb, _ in law["requires"]] + shapes + [None]:
            if label not in failed:
                missing.append(f"a vector failing {label}" if label else "a vector that holds")
    if not any(v["kind"] == "reject" for v in vecs):
        missing.append("a rejection")
    for n, ty in law.get("inputs", []):
        if ty != "money":
            continue
        texts = [v["inputs"][n] for v in vecs if isinstance(v.get("inputs"), dict)
                 and isinstance(v["inputs"].get(n), str)]
        forms = [_money_text(t) for t in texts]
        if not any(m is not None and not (MONEY_MIN < m.amount < MONEY_MAX) for m in forms):
            missing.append(f"{n} at an end of the range of an amount")
        if not any(m is None for m in forms):
            missing.append(f"{n} in a form that is not money")
        if len({m.currency for m in forms if m is not None and law["known"](m.currency)}) < 2:
            missing.append(f"{n} in two currencies the table knows")
        if not any(m is not None and not law["known"](m.currency) for m in forms):
            missing.append(f"{n} in a currency the table does not know")
    if not any(v["kind"] == "request_error" for v in vecs):
        missing.append("a request error")
    return missing
