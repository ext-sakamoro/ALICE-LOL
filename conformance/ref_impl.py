#!/usr/bin/env python3
"""Reference implementation of the 9 laws in laws/spike, used to test the runner and the corpus.

Reads {"law": ..., "inputs": {...}} on stdin, prints {"outputs": {...}} or {"rejected": "..."}.
REF_BUG=1: terminal velocity drag force without the factor 1/2
REF_BUG=2: ISA altitude range not enforced
REF_BUG=3: audits ignore the x-at-least lines
REF_BUG=4: Kepler integrated with RK4 (not the stated method)
REF_BUG=5: Kepler drops the last step (n * periods - 1 states)
REF_BUG=6: Kepler evaluates r^3 as r2 ** 1.5 (another rounding; must still conform)
REF_BUG=7: Kepler reports the state before each step (initial state first, last step missing)
REF_BUG=8: Kepler evaluates r^3 as sqrt(r2) ** 3 (another rounding; must still conform)
REF_BUG=9: audit evidence read as "not 0" over every non-array value (a negative count is evidence)
REF_BUG=10: audit ranges compared with their duplicates (multisets, not sets)
REF_BUG=11: a number that is not finite after parsing (1e400) is read as a number
REF_BUG=12: a list input of the wrong shape is read with each malformed element as an empty one
REF_BUG=13: a request without the inputs key is a request error (exit 2) instead of inputs {}
REF_BUG=14: inputs that is null or not an object is passed to the law unchecked
REF_BUG=15: a list input of the wrong shape is read element by element (still counted) instead of as a whole

An unknown law, a missing input, inputs that is not an object (null is read as {}) or a
request that is not a JSON object exits with status 2 and writes nothing on stdout.
"""
import json, math, os, re, sys

BUG = os.environ.get("REF_BUG", "")


class Reject(Exception):
    pass


# Elementary functions, correctly rounded: computed by mpmath at 60 digits and rounded once
# to double. The platform libm may be an ulp away (macOS sin is 1 ulp above at
# x = 0.7891896992570689, where 1000 sin(x) crosses the overflow of exp), and a verdict near
# a finiteness crossing would then depend on the platform; correctly rounded values are the
# same everywhere. alice-det-math (the Rust side) is not correctly rounded: it is
# deterministic and faithful (within about 1 ulp; powf64 within 13 ulp for |y| <= 8), and
# verdicts agree because the corpus and the probes keep 64 ulp away from every crossing
# (TASK.md). A result that is not finite in double raises
# OverflowError, as math.exp does, and the request is rejected (main). mpmath is loaded only
# when a quantitative law is evaluated (conformance/requirements.txt).
_MP = None


def _mp():
    global _MP
    if _MP is None:
        import mpmath
        mpmath.mp.dps = 60
        _MP = mpmath
    return _MP


def _cr(v):
    x = float(v)
    if not math.isfinite(x):
        raise OverflowError("not finite in double")
    return x


def cr_exp(x):
    return _cr(_mp().exp(x))


def cr_sin(x):
    return _cr(_mp().sin(x))


def cr_cos(x):
    return _cr(_mp().cos(x))


def cr_atan2(y, x):
    # the sign of a zero argument is ignored (TASK.md): mpmath has no signed zero anyway
    return _cr(_mp().atan2(y, x))


def cr_pow(x, y):
    """x^y for x > 0 (the laws raise a positive base to a non-integer power)"""
    return _cr(_mp().power(_mp().mpf(x), y))


def rng(name, x, lo, hi):
    if not isinstance(x, (int, float)) or isinstance(x, bool) or not math.isfinite(x) or x < lo or x > hi:
        raise Reject(f"{name}={x} outside [{lo}, {hi}]")


def is_num(x):
    return isinstance(x, (int, float)) and not isinstance(x, bool) and (BUG == "11" or math.isfinite(x))


def time_list(ts):
    """An x-list input: an array of numbers, otherwise the request is rejected"""
    if not isinstance(ts, list) or not all(is_num(t) for t in ts):
        raise Reject("t must be an array of numbers")
    return ts


def rk4(f, y, h):
    k1 = f(y)
    k2 = f([a + h / 2 * b for a, b in zip(y, k1)])
    k3 = f([a + h / 2 * b for a, b in zip(y, k2)])
    k4 = f([a + h * b for a, b in zip(y, k3)])
    return [a + h / 6 * (b + 2 * c + 2 * d + e) for a, b, c, d, e in zip(y, k1, k2, k3, k4)]


def integrate_to(f, y, t_from, t_to, h_max):
    span = t_to - t_from
    if span <= 0:
        return y
    n = max(1, math.ceil(span / h_max))
    h = span / n
    for _ in range(n):
        y = rk4(f, y, h)
    return y


def terminal(i):
    m, g, rho, cd, area = (i[k] for k in ("m", "g", "rho", "cd", "area"))
    rng("m", m, 0.1, 1000); rng("g", g, 1, 30); rng("rho", rho, 0.1, 2)
    rng("cd", cd, 0.1, 3); rng("area", area, 0.01, 10)
    vt = math.sqrt(2 * m * g / (rho * cd * area)); tau = vt / g
    ts = time_list(i["t"])
    for t in ts:
        rng("t/tau", t / tau, 0, 8)
    half = 1.0 if BUG == "1" else 0.5
    f = lambda y: [g - half * rho * cd * area * y[0] ** 2 / m]
    out, y, tc = [], [0.0], 0.0
    for t in sorted(set(ts)):
        y = integrate_to(f, y, tc, t, tau / 200); tc = t
        out.append((t, y[0]))
    d = dict(out)
    return {"v": [d[t] for t in ts]}


def decay(i):
    c0, k = i["c0"], i["k"]
    rng("c0", c0, 0.001, 1000); rng("k", k, 0.01, 10)
    ts = time_list(i["t"])
    for t in ts:
        rng("k t", k * t, 0, 4)
    f = lambda y: [-k * y[0]]
    out, y, tc = {}, [c0], 0.0
    for t in sorted(set(ts)):
        y = integrate_to(f, y, tc, t, 0.01 / k); tc = t
        out[t] = y[0]
    return {"c": [out[t] for t in ts]}


def pd(i):
    m, kp, kd, target = (i[k] for k in ("m", "kp", "kd", "target"))
    rng("m", m, 0.1, 100); rng("kp", kp, 0.1, 1000); rng("kd", kd, 0.01, 1000); rng("target", target, 0.01, 100)
    zeta = kd / (2 * math.sqrt(kp * m)); rng("zeta", zeta, 0.05, 0.95)
    wn = math.sqrt(kp / m)
    f = lambda y: [y[1], (kp * (target - y[0]) - kd * y[1]) / m]
    h = 2 * math.pi / wn / 2000
    y, peak = [0.0, 0.0], 0.0
    # run until the velocity turns negative after moving (first maximum), then a bit more
    for _ in range(200000):
        y2 = rk4(f, y, h)
        peak = max(peak, y2[0])
        if y[1] > 0 and y2[1] <= 0:
            # refine the maximum with a cubic-free bisection on the step
            lo, hi, ylo = 0.0, h, y
            for _ in range(60):
                mid = (lo + hi) / 2
                ym = rk4(f, ylo, mid)
                if ym[1] > 0:
                    lo = mid
                else:
                    hi = mid
            peak = max(peak, rk4(f, ylo, lo)[0])
            break
        y = y2
    return {"peak_x": peak}


def kepler(method, e_hi):
    def run(i):
        e, n, periods = i["e"], i["n"], i["periods"]
        rng("e", e, 0.2, e_hi); rng("n", n, 100, 2000); rng("periods", periods, 2, 40)
        if n != int(n) or periods != int(periods):
            raise Reject("n and periods must be integers")
        n, periods = int(n), int(periods)
        h = 2 * math.pi / n
        x, y, vx, vy = 1 + e, 0.0, 0.0, math.sqrt((1 - e) / (1 + e))

        def inv_r3(r2):
            if BUG == "6":
                return 1 / r2 ** 1.5
            if BUG == "8":
                return 1 / math.sqrt(r2) ** 3
            return 1 / (r2 * math.sqrt(r2))

        def acc(x, y):
            k = inv_r3(x * x + y * y)
            return -x * k, -y * k

        steps = n * periods - (1 if BUG in ("5", "7") else 0)
        states = [[x, y, vx, vy]] if BUG == "7" else []
        if BUG == "4":
            f = lambda s: [s[2], s[3], *acc(s[0], s[1])]
            s = [x, y, vx, vy]
            for _ in range(steps):
                s = rk4(f, s, h); states.append(list(s))
            return {"states": states}
        ax, ay = acc(x, y)
        for _ in range(steps):
            if method == "kdk":
                vx += h / 2 * ax; vy += h / 2 * ay
                x += h * vx; y += h * vy
                ax, ay = acc(x, y)
                vx += h / 2 * ax; vy += h / 2 * ay
            else:
                x += h / 2 * vx; y += h / 2 * vy
                bx, by = acc(x, y)
                vx += h * bx; vy += h * by
                x += h / 2 * vx; y += h / 2 * vy
            states.append([x, y, vx, vy])
        return {"states": states}
    return run


def four_bar(i):
    lc, lco, lr, lg, th = (i[k] for k in ("lc", "lco", "lr", "lg", "theta2"))
    for k, v in (("lc", lc), ("lco", lco), ("lr", lr), ("lg", lg)):
        rng(k, v, 0.01, 100)
    rng("theta2", th, 0, 2 * math.pi)
    others = (lco, lr, lg)
    if not min(others) - lc > 0:
        raise Reject("crank is not the shortest link")
    if not sum(others) - lc - 2 * max(others) > 0:
        raise Reject("not a Grashof crank-rocker")
    ax, ay = lc * cr_cos(th), lc * cr_sin(th)
    dx, dy = lg - ax, -ay
    d = math.sqrt(dx * dx + dy * dy)  # as written in the law: sqrt(dx^2 + dy^2)
    a = (d * d + lco * lco - lr * lr) / (2 * d)
    hh = math.sqrt(lco * lco - a * a)
    bx = ax + a * dx / d - hh * dy / d
    by = ay + a * dy / d + hh * dx / d
    # the sign of a zero argument is ignored (-0 reads as +0), so the angle is in (-pi, pi]
    return {"theta4": cr_atan2(by + 0.0, (bx - lg) + 0.0)}


def isa(i):
    hh = i["hh"]
    if BUG == "2":
        if not math.isfinite(hh):
            raise Reject("not finite")
    else:
        rng("hh", hh, 0, 20000)
    t0, p0, lapse, g0, mm, rr = 288.15, 101325.0, 0.0065, 9.80665, 0.0289644, 8.31432
    k = g0 * mm / (rr * lapse)
    t11 = t0 - lapse * 11000
    p11 = p0 * cr_pow(t11 / t0, k)
    if hh <= 11000:
        t = t0 - lapse * hh
        p = p0 * cr_pow(t / t0, k)
    else:
        t = t11
        p = p11 * cr_exp(-g0 * mm * (hh - 11000) / (rr * t11))
    return {"temperature": t, "pressure": p, "density": p * mm / (rr * t)}


def vkey(v):
    out = []
    for c in v.replace("-", ".").replace("+", ".").split("."):
        out.append((0, int(c), "") if c.isdigit() else (1, 0, c))
    return out


def audit(clauses, nums, ranges, raw=None):
    """clauses: list of (kind, ...) in file order; returns (verdict, subject)

    nums: the measured numbers (finite numbers only); ranges: the measured arrays of text;
    raw: the request inputs (used only by REF_BUG=9)
    """
    for c in clauses:
        if BUG == "9":
            # the old reading: any non-array value that is not 0 is evidence (-1, "12", true, null)
            old = {k: v for k, v in (nums if raw is None else raw).items() if not isinstance(v, list)}
            if c[0] == "evidence" and old.get(c[1], 0) == 0:
                return "no_evidence", c[1]
        elif c[0] == "evidence" and not nums.get(c[1], 0) > 0:
            return "no_evidence", c[1]
    for c in clauses:
        if c[0] == "range":
            got = ranges.get(c[1])
            if got is None:
                return "out_of_range", c[1]
            same = (sorted(got, key=vkey) == sorted(c[2], key=vkey)) if BUG == "10" else set(got) == set(c[2])
            if not same:
                return "parameter_update", c[1]
    for c in clauses:
        if c[0] == "expect":
            got = nums.get(c[1])
            if got is None:
                return "undecided", c[1]
            if abs(got - c[2]) > c[3]:
                return "breaks", c[1]
    return "supports", None


LAW_DIR = os.environ.get("LOL_LAW_DIR") or os.path.join(
    os.path.dirname(os.path.abspath(__file__)), os.pardir, "laws", "spike")


# -- types of x-input (written here, not imported: this file is an independent implementation)
def parse_type(text):
    text = text.strip()
    if text in ("number", "integer", "text"):
        return (text,)
    if text.startswith("list of "):
        return ("list", parse_type(text[len("list of "):]))
    if text.startswith("record(") and text.endswith(")"):
        fields, depth, cur = [], 0, ""
        for c in text[len("record("):-1] + ",":
            if c == "," and depth == 0:
                name, _, t = cur.partition(":")
                t = t.strip()
                opt = t.startswith("optional ")
                fields.append((name.strip(), opt, parse_type(t[len("optional "):] if opt else t)))
                cur = ""
                continue
            depth += (c == "(") - (c == ")")
            cur += c
        return ("record", fields)
    raise ValueError(f"type: {text!r}")


def fits(v, t):
    if t[0] in ("number", "integer"):
        if not isinstance(v, (int, float)) or isinstance(v, bool) or not math.isfinite(v):
            return False
        return t[0] == "number" or float(v).is_integer()
    if t[0] == "text":
        return isinstance(v, str)
    if t[0] == "list":
        return isinstance(v, list) and all(fits(x, t[1]) for x in v)
    return isinstance(v, dict) and all(
        (n in v and fits(v[n], ft)) or (n not in v and opt) for n, opt, ft in t[1])


METRIC_RE = re.compile(r"^(count)\(([\w-]+)\)$|^distinct\((set\()?([\w-]+)\[\]\.([\w-]+)\)?\)$")


# format characters (category Cf, Unicode 16.0), inclusive ranges: written out here so the
# answer does not depend on the unicodedata version of the Python that runs this file
FORMAT_CHARS = [(0x00AD, 0x00AD), (0x0600, 0x0605), (0x061C, 0x061C), (0x06DD, 0x06DD),
                (0x070F, 0x070F), (0x0890, 0x0891), (0x08E2, 0x08E2), (0x180E, 0x180E),
                (0x200B, 0x200F), (0x202A, 0x202E), (0x2060, 0x2064), (0x2066, 0x206F),
                (0xFEFF, 0xFEFF), (0xFFF9, 0xFFFB), (0x110BD, 0x110BD), (0x110CD, 0x110CD),
                (0x13430, 0x1343F), (0x1BCA0, 0x1BCA3), (0x1D173, 0x1D17A), (0xE0001, 0xE0001),
                (0xE0020, 0xE007F)]
# characters whose NFKC form, case-folded, is "x" / is "-" (Unicode 16.0)
X_LIKE = {0x58, 0x78, 0x2E3, 0x2093, 0x2169, 0x2179, 0x24CD, 0x24E7, 0xFF38, 0xFF58, 0x1CCED,
          0x1D417, 0x1D431, 0x1D44B, 0x1D465, 0x1D47F, 0x1D499, 0x1D4B3, 0x1D4CD, 0x1D4E7,
          0x1D501, 0x1D51B, 0x1D535, 0x1D54F, 0x1D569, 0x1D583, 0x1D59D, 0x1D5B7, 0x1D5D1,
          0x1D5EB, 0x1D605, 0x1D61F, 0x1D639, 0x1D653, 0x1D66D, 0x1D687, 0x1D6A1, 0x1F147}
DASH_LIKE = {0x2D, 0xFE63, 0xFF0D}


# the number form of a law file: ASCII decimal, finite (not float()'s spellings: inf, nan,
# underscores, other digits)
LAW_NUMBER = re.compile(r"[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?")


def law_number(token):
    if not LAW_NUMBER.fullmatch(token):
        return None
    v = float(token)
    return v if math.isfinite(v) else None


def allowed_char(c):
    """TAB and every non-control character, except format and line / paragraph separators"""
    cp = ord(c)
    if c == "\t":
        return True
    if cp < 0x20 or 0x7F <= cp <= 0x9F or cp in (0x2028, 0x2029):
        return False
    return not any(lo <= cp <= hi for lo, hi in FORMAT_CHARS)


def read_audit_law(name):
    """the audit block, x-input types, x-metric expressions and x-at-least lines of a law file"""
    path = os.path.join(LAW_DIR, f"{name}.law")
    if not re.fullmatch(r"[a-z0-9_]+", name) or not os.path.exists(path):
        return None
    # read without newline translation, split only on LF; a CR is a line end only directly
    # before an LF; every other control character except TAB, every format character and
    # the line / paragraph separators make the file unreadable
    with open(path, encoding="utf-8", newline="") as fh:
        text = fh.read()
    segments = text.split("\n")
    bodies = [seg[:-1] if k < len(segments) - 1 and seg.endswith("\r") else seg
              for k, seg in enumerate(segments)]
    for b in bodies:
        for c in b:
            if not allowed_char(c):
                raise ValueError(f"U+{ord(c):04X} is not allowed in a law file")
    raws = [b.split("#", 1)[0] for b in bodies]
    lines = [r.strip() for r in raws]
    if "kind audit" not in lines:
        return None
    law = {"clauses": [], "types": {}, "metrics": [], "at_least": []}
    inside = closed = False
    for raw, l in zip(raws, lines):
        # the `x-` prefix is reserved: a line starting with it (trimmed, any case) is a known
        # declaration written in lowercase from the first column, its keyword followed by
        # exactly one ASCII space; anything else is refused, never skipped
        if len(l) >= 2 and ord(l[0]) in X_LIKE and ord(l[1]) in DASH_LIKE and not raw.startswith("x-"):
            raise ValueError("a line starting with `x-` is a declaration: lowercase, from the first column")
        if l.startswith("x-"):
            kw = re.match(r"\S*", l).group(0)
            if kw not in ("x-input", "x-metric", "x-at-least"):
                raise ValueError(f"unknown declaration `{kw}`")
            after = l[len(kw):]
            if not after.startswith(" ") or after[1:2].isspace() or not after.strip():
                raise ValueError(f"`{kw}` must be followed by exactly one space")
        w = l.split()
        if inside:
            # the audit block: one clause per line, each with its exact tokens
            if l == "end audit":
                inside = False
                closed = True
            elif not w:
                continue
            elif w[0] == "audit" and len(w) == 2:
                if "name" in law:
                    raise ValueError("a second `audit <name>` line")
                law["name"] = w[1]
            elif w[0] == "evidence" and len(w) == 2:
                law["clauses"].append(("evidence", w[1]))
            elif w[0] == "expect" and len(w) in (4, 6) and w[2] == "==" and (len(w) == 4 or w[4] == "within"):
                value = law_number(w[3])
                tol = law_number(w[5]) if len(w) == 6 else 0.0
                if value is None or tol is None or tol < 0:
                    raise ValueError(f"expect: not a number of a law file: {l!r}")
                law["clauses"].append(("expect", w[1], value, tol))
            elif w[0] == "range" and len(w) >= 3:
                law["clauses"].append(("range", w[1], w[2:]))
            else:
                raise ValueError(f"audit block: not a clause: {l!r}")
        elif l == "begin audit":
            if closed:
                raise ValueError("a second audit block")
            inside = True
        elif w and w[0] == "x-input":
            if w[1] in law["types"]:
                raise ValueError(f"x-input `{w[1]}` is declared twice")
            law["types"][w[1]] = parse_type(l.split(None, 2)[2])
        elif w and w[0] == "x-metric":
            name_, _, expr = l[len("x-metric"):].partition("=")
            m = METRIC_RE.match("".join(expr.split()))
            if not m:
                raise ValueError(f"x-metric: {expr!r}")
            if any(n == name_.strip() for n, _ in law["metrics"]):
                raise ValueError(f"x-metric `{name_.strip()}` is declared twice")
            law["metrics"].append((name_.strip(), ("count", m.group(2)) if m.group(1)
                                   else ("set" if m.group(3) else "distinct", m.group(4), m.group(5))))
        elif w and w[0] == "x-at-least":
            if any(n == w[1] for n, _ in law["at_least"]):
                raise ValueError(f"x-at-least for `{w[1]}` is declared twice")
            floor = law_number(w[2]) if len(w) == 3 else None
            if floor is None or floor < 0:
                raise ValueError(f"`x-at-least <metric> <number>` expected: {l!r}")
            law["at_least"].append((w[1], floor))
    if inside or not closed:
        raise ValueError("no closed `begin audit` ... `end audit` block")
    if "name" not in law:
        raise ValueError("no `audit <name>` line")
    if not law["clauses"]:
        raise ValueError("the audit block states nothing")
    # a floor on a metric no x-metric line defines is a typo, not a declaration
    defined = {n for n, _ in law["metrics"]}
    for name, _ in law["at_least"]:
        if name not in defined:
            raise ValueError(f"`x-at-least {name}` names no x-metric")
    return law


def value_key(v):
    # with the type: text "1" and the number 1 differ; -0 reads as 0
    if isinstance(v, bool):
        return ("bool", v)
    if isinstance(v, (int, float)):
        return ("num", float(v) + 0.0)
    if isinstance(v, str):
        return ("text", v)
    if isinstance(v, list):
        return ("list", tuple(value_key(x) for x in v))
    return ("other", repr(v))


def derive(expr, value, raw):
    """value: the input when it matches its type, else None; raw: the input as given"""
    if BUG == "15" and isinstance(raw, list):
        # the older field-level reading: a malformed element still counts
        value = raw
        if expr[0] != "count" and not all(isinstance(b, dict) and expr[2] in b and
                                          (expr[0] != "set" or isinstance(b[expr[2]], list)) for b in raw):
            return None
    if BUG == "12" and value is None and isinstance(raw, list):
        # the reading that takes a malformed element as an empty one
        value = [b if isinstance(b, dict) and isinstance(b.get(expr[-1]), list) else {expr[-1]: []}
                 for b in raw] if expr[0] == "set" else raw
    if not isinstance(value, list):
        return None
    if expr[0] == "count":
        return len(value)
    keys = set()
    for b in value:
        if not isinstance(b, dict) or expr[2] not in b:
            return None
        keys.add(frozenset(value_key(x) for x in b[expr[2]]) if expr[0] == "set" else value_key(b[expr[2]]))
    return len(keys)


def generic_audit(law):
    def run(i):
        metric_names = {n for n, _ in law["metrics"]}
        nums = {k: v for k, v in i.items()
                if is_num(v) and k not in law["types"] and k not in metric_names}
        typed = {k: v for k, v in i.items() if k in law["types"] and fits(v, law["types"][k])}
        ranges = {c[1]: typed[c[1]] for c in law["clauses"] if c[0] == "range" and c[1] in typed}
        for name, expr in law["metrics"]:
            v = derive(expr, typed.get(expr[1]), i.get(expr[1]))
            if v is not None:
                nums[name] = v
        if BUG != "3":
            for name, n in law["at_least"]:
                if nums.get(name, 0) < n:
                    nums[name] = 0
        v, s = audit(law["clauses"], nums, ranges, i)
        return {"verdict": v, "subject": s}
    return run


def finite_probe(i):
    x = i["x"]
    rng("x", x, 0, math.pi)
    # exp raises OverflowError past ~709.78: the intermediate is not finite, and the
    # request is rejected (main) although 1/exp(...) would be 0 under IEEE overflow
    e = cr_exp(1000 * cr_sin(x))
    return {"y": 1 / e}


def finite_numbers(v):
    """every number in an output (lists included) is finite"""
    if isinstance(v, list):
        return all(finite_numbers(x) for x in v)
    if isinstance(v, dict):
        return all(finite_numbers(x) for x in v.values())
    return not isinstance(v, float) or math.isfinite(v)


LAWS = {
    "terminal_velocity_quadratic_drag": terminal,
    "first_order_decay": decay,
    "pd_step_overshoot": pd,
    "kepler_energy_bounded_kdk": kepler("kdk", 0.77),
    "kepler_energy_bounded_dkd": kepler("dkd", 0.765),
    "four_bar_rocker_angle": four_bar,
    "isa1976_lower_atmosphere": isa,
    "finite_evaluation_probe": finite_probe,
}


def request_error(msg):
    """An error of the request: exit status 2, nothing on standard output"""
    print(msg, file=sys.stderr)
    sys.exit(2)


MAX_DEPTH = 512  # arrays and objects, the request object is level 1


def _no_constant(name):
    raise ValueError(f"{name} is not JSON")


def _number(text):
    # every number is a double; a literal too large for one is +-infinity (float() of
    # the text gives that, int() of a long integer would not)
    return int(text) if len(text.lstrip("-")) <= 15 else float(text)


def read_request(text):
    """JSON text as TASK.md reads it: no NaN / Infinity literals, numbers as doubles,
    the last of a repeated key, at most MAX_DEPTH levels, no lone surrogate"""
    try:
        req = json.loads(text, parse_constant=_no_constant, parse_int=_number)
    except (ValueError, RecursionError) as e:
        request_error(f"request is not JSON: {e}")
    stack = [(req, 1)]
    while stack:
        v, depth = stack.pop()
        if isinstance(v, (list, dict)):
            if depth > MAX_DEPTH:
                request_error(f"request nests more than {MAX_DEPTH} levels")
            items = v.items() if isinstance(v, dict) else enumerate(v)
            for k, x in items:
                if isinstance(k, str):
                    stack.append((k, depth))
                stack.append((x, depth + 1))
        elif isinstance(v, str) and any("\ud800" <= c <= "\udfff" for c in v):
            request_error("request has a lone surrogate")
    return req


def main():
    data = sys.stdin.buffer.read()
    # the request is UTF-8; bytes that are not are an error of the request, decided
    # here and not by the text layer of sys.stdin (which raises outside this path)
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as e:
        request_error(f"request is not UTF-8: {e}")
    req = read_request(text)
    if not isinstance(req, dict):
        request_error("request is not a JSON object")
    name = req.get("law")
    law = LAWS.get(name) if isinstance(name, str) else None
    if law is None and isinstance(name, str):
        # a law file that does not read (a malformed or repeated declaration) is an error
        # of the request, like an unknown law
        try:
            audit_law = read_audit_law(name)
        except (ValueError, IndexError) as e:
            request_error(f"law file `{name}` does not read: {e}")
        law = generic_audit(audit_law) if audit_law else None
    if law is None:
        request_error(f"unknown law: {req.get('law')!r}")
    # no inputs key and inputs null are the same as inputs {}: an audit measures
    # nothing, a quantitative law then misses its inputs (exit 2 below)
    inputs = req["inputs"] if BUG == "13" else req.get("inputs")
    if BUG != "14":
        if inputs is None:
            inputs = {}
        elif not isinstance(inputs, dict):
            request_error(f"inputs is not a JSON object: {type(inputs).__name__}")
    try:
        out = {"outputs": law(inputs)}
        # a value that is not finite is not a number (TASK.md): never written as output
        if not finite_numbers(out["outputs"]):
            out = {"rejected": "non-finite value"}
    except Reject as e:
        out = {"rejected": str(e)}
    except (OverflowError, ValueError, ZeroDivisionError):
        # an intermediate value of a quantitative law is not finite (overflow, a domain
        # error, a division by 0); an audit never rejects, so there it stays an error
        if name not in LAWS:
            raise
        out = {"rejected": "non-finite value"}
    except KeyError as e:
        request_error(f"missing input: {e}")
    json.dump(out, sys.stdout)


if __name__ == "__main__":
    main()
