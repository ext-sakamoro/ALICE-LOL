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
    ax, ay = lc * math.cos(th), lc * math.sin(th)
    dx, dy = lg - ax, -ay
    d = math.hypot(dx, dy)
    a = (d * d + lco * lco - lr * lr) / (2 * d)
    hh = math.sqrt(lco * lco - a * a)
    bx = ax + a * dx / d - hh * dy / d
    by = ay + a * dy / d + hh * dx / d
    # the sign of a zero argument is ignored (-0 reads as +0), so the angle is in (-pi, pi]
    return {"theta4": math.atan2(by + 0.0, (bx - lg) + 0.0)}


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
    p11 = p0 * (t11 / t0) ** k
    if hh <= 11000:
        t = t0 - lapse * hh
        p = p0 * (t / t0) ** k
    else:
        t = t11
        p = p11 * math.exp(-g0 * mm * (hh - 11000) / (rr * t11))
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


LAW_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.pardir, "laws", "spike")


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


def read_audit_law(name):
    """the audit block, x-input types, x-metric expressions and x-at-least lines of a law file"""
    path = os.path.join(LAW_DIR, f"{name}.law")
    if not re.fullmatch(r"[a-z0-9_]+", name) or not os.path.exists(path):
        return None
    lines = [l.split("#", 1)[0].strip() for l in open(path, encoding="utf-8")]
    if "kind audit" not in lines:
        return None
    law = {"clauses": [], "types": {}, "metrics": [], "at_least": []}
    inside = False
    for l in lines:
        w = l.split()
        if l == "begin audit":
            inside = True
        elif l == "end audit":
            inside = False
        elif inside and w and w[0] == "evidence":
            law["clauses"].append(("evidence", w[1]))
        elif inside and w and w[0] == "expect":
            tol = float(w[5]) if len(w) > 5 and w[4] == "within" else 0.0
            law["clauses"].append(("expect", w[1], float(w[3]), tol))
        elif inside and w and w[0] == "range":
            law["clauses"].append(("range", w[1], w[2:]))
        elif w and w[0] == "x-input":
            law["types"][w[1]] = parse_type(l.split(None, 2)[2])
        elif w and w[0] == "x-metric":
            name_, _, expr = l[len("x-metric"):].partition("=")
            m = METRIC_RE.match("".join(expr.split()))
            if not m:
                raise ValueError(f"x-metric: {expr!r}")
            law["metrics"].append((name_.strip(), ("count", m.group(2)) if m.group(1)
                                   else ("set" if m.group(3) else "distinct", m.group(4), m.group(5))))
        elif w and w[0] == "x-at-least":
            law["at_least"].append((w[1], float(w[2])))
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


LAWS = {
    "terminal_velocity_quadratic_drag": terminal,
    "first_order_decay": decay,
    "pd_step_overshoot": pd,
    "kepler_energy_bounded_kdk": kepler("kdk", 0.77),
    "kepler_energy_bounded_dkd": kepler("dkd", 0.765),
    "four_bar_rocker_angle": four_bar,
    "isa1976_lower_atmosphere": isa,
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
        audit_law = read_audit_law(name)
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
    except Reject as e:
        out = {"rejected": str(e)}
    except KeyError as e:
        request_error(f"missing input: {e}")
    json.dump(out, sys.stdout)


if __name__ == "__main__":
    main()
