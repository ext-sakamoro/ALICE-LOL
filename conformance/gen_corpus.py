#!/usr/bin/env python3
"""Builds a conformance corpus from law files.

usage: gen_corpus.py --out <corpus.json> [--laws <dir of *.law>] [--tools <dir with law_corners.py>] [--jobs N]
       (defaults: laws/spike and scripts/ of this repository; needs mpmath, see requirements.txt)

What comes from the law files (read with law_corners.py):
- the valid range of every vector: an accepted vector must satisfy every input
  range, x-integer and x-range line; a rejected vector must break one. The
  generator stops when a designed vector disagrees with the law file.
- every corner of the valid range (law_corners.corners); one vector per
  corner that no other vector already covers.
- expected values: the `output` / `x-piece` / `x-expr` expression and the
  `tolerance` expression, evaluated with mpmath at 40 digits.
- Kepler laws: the method (x-method kdk / dkd), the start (x-initial) and the
  acceleration (x-ode) are integrated with mpmath at 30 digits to give the
  state after step 1 and after the last step.
- audit laws: the verdict follows the clauses of the audit block.

What the generator holds itself (it cannot be read from a law file):
- the interior points it chooses (the "designed" cases below), a base point
  per law for the inputs a corner does not fix, and the request for each
  x-range edge (a value of a derived quantity, solved by hand); a bound corner
  uses the witness point law_corners found, so it needs nothing here;
- nothing law specific: x-metric lines are derivation expressions (scripts/law_schema.py).

Nothing here calls an implementation.
"""
import argparse, json, math, sys
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

try:
    import mpmath as mp
except ImportError:
    raise SystemExit("gen_corpus.py needs mpmath: python3 -m pip install -r conformance/requirements.txt")

lc = None  # law_corners, loaded from --tools
ls = None  # law_schema, loaded from --tools


# ------------------------------------------------------------------ validity

def is_num(x):
    return isinstance(x, (int, float)) and not isinstance(x, bool) and math.isfinite(x)


def valid(law, inputs):
    """Whether the law applies, judged from the law file in double and at 40 digits;
    a request whose verdict differs between the two sits on an edge within rounding
    and is not used"""
    mp.mp.dps = 40
    a, b = valid_in(law, inputs, math), valid_in(law, inputs, mp)
    if a != b:
        raise SystemExit(f"{law['name']}: {inputs} is {'in' if a else 'out of'} range in double "
                         f"and {'in' if b else 'out of'} range at 40 digits")
    return a


def valid_in(law, inputs, lib):
    for v in law["inputs"]:
        x = inputs.get(v["name"])
        if v["name"] in law["lists"]:
            if not isinstance(x, list) or not all(is_num(t) for t in x):
                return False
            continue
        if not is_num(x):
            return False
        if v["range"] and not v["range"][0] <= x <= v["range"][1]:
            return False
        if v["name"] in law["integers"] and not float(x).is_integer():
            return False
    for env in lc.list_envs(law, inputs, lib):
        for r in law["x_ranges"]:
            if r["q"] not in env:
                return False
            q = env[r["q"]]
            if not (q > r["lo"] if r["op"] == ">" else r["lo"] <= q <= r["hi"]):
                return False
    return True


# ------------------------------------------------------------------ expected values

def closed_form(law, inputs, periodic):
    """{output: expected}, {output: tolerance} with mpmath at 40 digits"""
    mp.mp.dps = 40
    tol_trees = dict(law["tolerances"])
    exp, tol = {}, {}
    names = list(tol_trees)
    for name in names:
        vals, tols = [], []
        for env in lc.list_envs(law, inputs, mp):
            tree = dict(law["outputs"]).get(name) or dict(law["x_exprs"]).get(name)
            if tree is None:
                piece = next(p for p in law["pieces"] if p["out"] == name
                             and p["lo"] <= env[p["input"]] <= p["hi"])
                tree = piece["expr"]
            val = lc.evaluate(tree, env, mp)
            vals.append(val)
            tols.append(lc.evaluate(tol_trees[name], {**env, name: val}, mp))
        if any(isinstance(inputs.get(n), list) for n in law["lists"]):
            exp[name] = [float(v) for v in vals]
            tol[name] = float(max(tols))
        else:
            exp[name] = float(vals[0])
            tol[name] = float(tols[0])
    return exp, tol


def kepler_states(law, inputs, steps_wanted):
    """State after step 1 and after the last step, integrated at 30 digits"""
    mp.mp.dps = 30
    env = lc.env_for(law, inputs, mp)
    zero = mp.mpf(0)
    s = {k: (zero if t == ("num", 0.0) else lc.evaluate(t, env, mp)) for k, t in law["initial"].items()}
    pos = [k for k, t in law["odes"].items() if t[0] == "var"]       # x' = vx
    vel = {k: law["odes"][k][1] for k in pos}                         # x -> vx
    acc = {vel[k]: law["odes"][vel[k]] for k in pos}                  # vx' = a(x)
    h = env["h"]
    n_total = int(inputs["n"]) * int(inputs["periods"])

    def a():
        e = {**env, **s}
        return {v: lc.evaluate(t, e, mp) for v, t in acc.items()}

    out = {}
    ak = a()
    for i in range(1, n_total + 1):
        if law["method"] == "kdk":
            for v in acc: s[v] += h / 2 * ak[v]
            for x in pos: s[x] += h * s[vel[x]]
            ak = a()
            for v in acc: s[v] += h / 2 * ak[v]
        elif law["method"] == "dkd":
            for x in pos: s[x] += h / 2 * s[vel[x]]
            ak = a()
            for v in acc: s[v] += h * ak[v]
            for x in pos: s[x] += h / 2 * s[vel[x]]
        else:
            raise SystemExit(f"{law['name']}: x-method {law['method']} has no fixed trajectory")
        if i in steps_wanted:
            out[i] = [float(s[k]) for k in law["states"]]
    return out


def audit_verdict(law, metrics, ranges):
    clauses = [l.split() for l in law["audit"]]
    for c in clauses:
        # evidence is a finite number greater than 0 (metrics holds finite numbers only)
        if c[0] == "evidence" and not metrics.get(c[1], 0) > 0:
            return "no_evidence", c[1]
    for c in clauses:
        if c[0] == "range":
            if c[1] not in ranges:
                return "out_of_range", c[1]
            if set(ranges[c[1]]) != set(c[2:]):
                return "parameter_update", c[1]
    for c in clauses:
        if c[0] == "expect":
            want = float(c[3]); tol = float(c[5]) if len(c) > 5 and c[4] == "within" else 0.0
            if c[1] not in metrics:
                return "undecided", c[1]
            if abs(metrics[c[1]] - want) > tol:
                return "breaks", c[1]
    return "supports", None


def x_metrics(law, inputs):
    """The measurements of an audit request: finite numbers, ranges, and the x-metric derivations"""
    # a value that is not a finite number is not measured; a range that is not an
    # array of text (null included) is not measured; ranges compare as sets
    nums = {k: v for k, v in inputs.items() if is_num(v)}
    ranges = {k: v for k, v in inputs.items() if isinstance(v, list) and all(isinstance(s, str) for s in v)}
    # an x-input that does not match its type is not measured, as a whole
    types = ls.x_input_types(Path(law["_file"]).read_text())
    inputs = {k: v for k, v in inputs.items() if k not in types or ls.matches(v, types[k])}
    ranges = {k: v for k, v in ranges.items() if k in inputs}
    # the x-metric lines (derivation expressions) on the inputs that match their type
    metrics, at_least = ls.law_metrics(Path(law["_file"]).read_text())
    # a name defined by x-metric is never read from the request
    for name, _ in metrics:
        nums.pop(name, None)
        ranges.pop(name, None)
    for name, expr in metrics:
        v = ls.eval_metric(expr, inputs.get(expr[1]))
        if v is not None:
            nums[name] = v
    for name, n in at_least:
        if nums.get(name, 0) < n:
            nums[name] = 0
    return nums, ranges


# ------------------------------------------------------------------ designed cases and corner realisation

def terminal_t(m, g, rho, cd, area, fracs=(0, 0.1, 0.5, 1, 2, 4, 7.9)):
    vt = math.sqrt(2 * m * g / (rho * cd * area)); tau = vt / g
    return [f * tau for f in fracs]


def T(m, g, rho, cd, area, t=None):
    return {"m": m, "g": g, "rho": rho, "cd": cd, "area": area,
            "t": t if t is not None else terminal_t(m, g, rho, cd, area)}


def D(c0, k, t=None):
    return {"c0": c0, "k": k, "t": t if t is not None else [0, 1 / k, 3.9 / k]}


DESIGN = {
    "terminal_velocity_quadratic_drag": {
        "base": T(10, 10, 1, 2, 1),
        "rebuild": lambda i: {**i, "t": terminal_t(i["m"], i["g"], i["rho"], i["cd"], i["area"])},
        "cases": [
            (T(10, 10, 1, 2, 1, [0, 0.05, 0.25, 0.5, 1, 1.5, 2, 3, 5, 8]), "source configuration, t up to 8 tau"),
            (T(1, 9.80665, 1.225, 0.47, 0.01), "sphere-like"),
            (T(80, 9.80665, 1.225, 1.0, 0.7), "skydiver-like"),
            (T(1000, 30, 0.1, 0.1, 0.01), "very large v_t"),
            (T(5, 3.7, 0.1, 1.2, 2.0), "low gravity"),
            (T(250, 24.8, 1.8, 0.8, 4.0), "high gravity"),
            (T(10, 10, 1, 2, 1, [3, 0.5, 3, 1]), "unsorted, repeated times"),
            (T(10, 10, 1, 2, 1, 1.0), "x-list given a number, not an array"),
            (T(10, 10, 1, 2, 1, [0.5, "1"]), "x-list with an element that is not a number"),
        ],
        "edges": {
            "t_over_tau@lo": T(10, 10, 1, 2, 1, [0]), "t_over_tau@hi": T(10, 10, 1, 2, 1, [8]),
            "t_over_tau<lo": T(10, 10, 1, 2, 1, [-0.1]), "t_over_tau>hi": T(10, 10, 1, 2, 1, [1, 8.5]),
        },
    },
    "first_order_decay": {
        "base": D(10, 0.5),
        "rebuild": lambda i: {**i, "t": [0, 1 / i["k"], 3.9 / i["k"]]},
        "cases": [
            (D(10, 0.5, [0, 1, 2, 4, 8]), "source checkpoints"),
            (D(10, 0.25, [float(mp.log(2) / mp.mpf(0.25)), 16]), "source half-life"),
            (D(3.5, 1.7, [0.2, 0.9, 2.3]), "mid range"),
            (D(42, 0.08, [5, 25, 49.9]), "slow decay"),
            (D(7, 0.5, [8, 1, 4, 1]), "unsorted, repeated times"),
            (D(10, 0.5, 2.0), "x-list given a number, not an array"),
            (D(10, 0.5, {"t": 2.0}), "x-list given an object, not an array"),
        ],
        "edges": {"kt@lo": D(10, 0.5, [0]), "kt@hi": D(10, 0.5, [8]),
                  "kt<lo": D(10, 0.5, [-1]), "kt>hi": D(10, 0.5, [9])},
    },
    "pd_step_overshoot": {
        "base": {"m": 1, "kp": 1, "kd": 1, "target": 1},
        "cases": [({"m": 1, "kp": 1, "kd": 1, "target": 1}, "source configuration, zeta = 0.5"),
                  ({"m": 2, "kp": 50, "kd": 3, "target": 0.25}, "zeta = 0.15"),
                  ({"m": 0.5, "kp": 8, "kd": 2.4, "target": 1.5}, "zeta = 0.6"),
                  ({"m": 12, "kp": 300, "kd": 90, "target": 0.4}, "zeta = 0.75"),
                  ({"m": 1, "kp": 1, "kd": 2, "target": 1}, "zeta = 1 (critically damped)")],
        "edges": {"zeta@lo": {"m": 1, "kp": 1, "kd": 0.1, "target": 1},
                  "zeta@hi": {"m": 1, "kp": 1, "kd": 1.9, "target": 1},
                  "zeta<lo": {"m": 1, "kp": 1, "kd": 0.08, "target": 1},
                  "zeta>hi": {"m": 1, "kp": 1, "kd": 1.92, "target": 1}},
    },
    "finite_evaluation_probe": {
        # 1000 sin(x) passes 709.78 (the largest exp argument that is finite in double) at
        # x = 0.789 and x = 2.353; the cases stay away from both
        "base": {"x": 0.5},
        "cases": [
            ({"x": 0.5}, "exp(479): finite"),
            ({"x": 0.7}, "exp(644): finite, y near the smallest normal double"),
            ({"x": 2.6}, "exp(516) after the peak: finite"),
            ({"x": 1.0}, "exp(841) overflows: rejected"),
            ({"x": 1.5707963267948966}, "exp(1000) overflows: rejected (1/exp(1000) would be 0)"),
            ({"x": 2.0}, "exp(909) overflows: rejected"),
        ],
        "edges": {},
    },
    "four_bar_rocker_angle": {
        "base": {"lc": 1.0, "lco": 2.0, "lr": 1.5, "lg": 2.3, "theta2": 1.0},
        "cases": [({"lc": 1.0, "lco": 2.0, "lr": 1.5, "lg": 2.3, "theta2": float(2 * mp.pi * i / 24)},
                   "source linkage") for i in range(0, 24, 3)]
        + [({"lc": 0.5, "lco": 3.0, "lr": 2.0, "lg": 2.8, "theta2": 2.0}, "long coupler"),
           ({"lc": 10, "lco": 40, "lr": 30, "lg": 35, "theta2": 3.5}, "scaled"),
           ({"lc": 1, "lco": 2, "lr": 1.5, "lg": 2.5, "theta2": 1.0}, "shortest + longest = sum of the others"),
           ({"lc": 1, "lco": 2, "lr": 1.2, "lg": 3.0, "theta2": 1.0}, "shortest + longest > sum of the others"),
           # Grashof margin about 1e-13: the radicand of `across` is -1.8e-12 in double and
           # +1.2e-12 exactly; no clamping, so the double rule (finite_in_double) rejects it
           ({"lc": 1.7939999999998988, "lco": 94.027, "lr": 5.407, "lg": 90.414, "theta2": 0.0},
            "negative radicand by rounding: rejected")],
        "edges": {"crank_margin@>0": {"lc": 1, "lco": 1.01, "lr": 2.0, "lg": 2.0, "theta2": 1.0},
                  "grashof_margin@>0": {"lc": 1, "lco": 1.01, "lr": 2.0, "lg": 2.0, "theta2": 1.0},
                  "crank_margin<=0": {"lc": 2, "lco": 1, "lr": 2.5, "lg": 2.3, "theta2": 1.0},
                  "grashof_margin<=0": {"lc": 1, "lco": 2, "lr": 1.5, "lg": 2.5, "theta2": 1.0}},
    },
    "isa1976_lower_atmosphere": {
        "base": {"hh": 5000},
        "cases": [({"hh": h}, "") for h in (500, 2500, 7777, 10999, 11001, 13500, 17500, 19999)],
        "edges": {},
    },
    "gate_compares_nonzero": {"cases": [], "edges": {}},
    "identifier_feature_independent": {"cases": [], "edges": {}},
}

INF = float("inf")  # written on the wire as the literal 1e400 (run_conformance.py)
K42 = ["case-17", "case-42"]
DESIGN["gate_compares_nonzero"]["cases"] = [(i, n) for i, n in [
    ({"compared": 12, "mismatches": 0, "known-mismatches": K42}, "all good"),
    ({"compared": 12, "mismatches": 0, "known-mismatches": ["case-42", "case-17"]}, "known list in another order"),
    ({"compared": 0, "mismatches": 0, "known-mismatches": K42}, "compared nothing"),
    ({"mismatches": 0, "known-mismatches": K42}, "compared not reported"),
    ({"compared": 0, "mismatches": 3}, "evidence first"),
    ({"compared": 5, "mismatches": 0}, "known list missing"),
    ({"compared": 5, "mismatches": 2}, "range before expect"),
    ({"compared": 5, "mismatches": 0, "known-mismatches": ["case-17"]}, "one known case fixed"),
    ({"compared": 5, "mismatches": 0, "known-mismatches": ["case-9", "case-17", "case-42"]}, "a new known case"),
    ({"compared": 5, "mismatches": 0, "known-mismatches": []}, "empty known list"),
    ({"compared": 5, "known-mismatches": K42}, "mismatches not reported"),
    ({"compared": 5, "mismatches": 1, "known-mismatches": K42}, "one mismatch"),
    ({"compared": 1, "mismatches": 7, "known-mismatches": K42}, ""),
    ({"compared": 1, "mismatches": 0, "known-mismatches": K42}, "one item compared"),
    # evidence is a finite number greater than 0; JSON carries no NaN or infinity,
    # so a value that is not a number stands for them
    ({"compared": -1, "mismatches": 0, "known-mismatches": K42}, "negative count is not evidence"),
    ({"compared": -0.5, "mismatches": 1, "known-mismatches": K42}, "negative count before expect"),
    ({"compared": "12", "mismatches": 0, "known-mismatches": K42}, "count given as text is not evidence"),
    ({"compared": True, "mismatches": 0, "known-mismatches": K42}, "true is not a number"),
    ({"compared": None, "mismatches": 0, "known-mismatches": K42}, "null count is not evidence"),
    ({"compared": 0.5, "mismatches": 0, "known-mismatches": K42}, "a positive fraction is evidence"),
    # ranges are sets; a value that is not an array of text is not measured
    ({"compared": 5, "mismatches": 0, "known-mismatches": ["case-17", "case-42", "case-17"]},
     "duplicate in the known list, same set"),
    ({"compared": 5, "mismatches": 0, "known-mismatches": ["case-42", "case-42", "case-17", "case-42"]},
     "duplicates and another order, same set"),
    ({"compared": 5, "mismatches": 0, "known-mismatches": ["case-17", "case-17"]},
     "duplicate hides a missing case"),
    ({"compared": 5, "mismatches": 0, "known-mismatches": "case-17 case-42"}, "known list given as text"),
    ({"compared": 5, "mismatches": 0, "known-mismatches": None}, "known list given as null"),
    ({"compared": 5, "mismatches": 0, "known-mismatches": [17, 42]}, "known list of numbers"),
    ({"compared": 5, "mismatches": 3, "known-mismatches": None}, "null range before expect"),
    ({"compared": 5, "mismatches": "0", "known-mismatches": K42}, "mismatches given as text is not measured"),
    # a literal too large for a double (sent as 1e400) parses to infinity: not a number, not measured
    ({"compared": INF, "mismatches": 0, "known-mismatches": K42}, "overflowing count is not evidence"),
    ({"compared": 5, "mismatches": INF, "known-mismatches": K42}, "overflowing mismatches is not measured"),
    ({"compared": 5, "mismatches": -INF, "known-mismatches": K42}, "negative overflow is not measured"),
    # a request without the inputs key is the same as inputs {}: nothing measured
    (None, "inputs key absent"),
]]
B = lambda feats, i: {"features": feats, "id": i} if i is not None else {"features": feats}
DESIGN["identifier_feature_independent"]["cases"] = [({} if b is None else {"builds": b}, n) for b, n in [
    ([B([], "ab"), B(["std"], "ab"), B(["std", "parallel"], "ab")], "3 feature sets agree"),
    ([B([], "ab"), B(["std"], "cd")], "2 feature sets disagree"),
    ([], "no builds"), (None, "builds not reported"),
    ([B(["std"], "ab")], "one build"),
    ([B(["std"], "ab"), B(["std"], "ab")], "same feature set twice"),
    ([B(["std", "parallel"], "ab"), B(["parallel", "std"], "cd")], "same set in another order"),
    ([B(["std"], "ab"), B(["simd"], "ab"), B(["std"], "ab")], "duplicate set collapses, 2 remain"),
    ([B(["std"], "ab"), B(["simd"], None)], "a build without identifier"),
    ([B(["std"], "ab"), B(["std"], None)], "evidence before expect"),
    ([B([], "x"), B(["a"], "y"), B(["b"], "z")], "all differ"),
    ([B([], "x"), B(["a"], "x"), B(["b"], "x"), B(["a", "b"], "y")], "one differs"),
    # builds that does not match its type is not measured as a whole: one malformed
    # build (or id) is enough, and the verdict is no_evidence for builds
    ([{"id": "ab"}, B(["std"], "ab"), B(["simd"], "ab")], "a build without features"),
    ([{"features": "std", "id": "ab"}, B(["simd"], "ab"), B([], "ab")], "features given as text"),
    ([7, B(["std"], "ab"), B(["simd"], "ab")], "a build that is not an object"),
    ([B(["std", 3], "ab"), B(["simd"], "ab")], "a feature that is not text"),
    ([B(["std"], 5), B(["simd"], 5)], "id given as a number"),
    ([B(["std"], "ab"), {"features": ["simd"], "id": None}], "id given as null"),
    ("std", "builds given as text"),
    # derivations: values compare with their type and as text, sets ignore order and duplicates
    ([B(["std"], "1"), B(["simd"], "1.0")], "ids 1 and 1.0 are different text"),
    ([B(["1"], "x"), B(["01"], "x")], "feature names 1 and 01 are different text"),
    ([B([], "x"), B([], "x")], "every feature list empty: one set"),
    ([B([], "x"), B(["a", "a"], "x"), B(["a"], "x")], "duplicates collapse inside a set"),
]]
DESIGN["identifier_feature_independent"]["cases"] += [
    # a name defined by x-metric is never read from the request
    ({"builds": [B(["std"], "ab")], "feature_sets": 5}, "feature_sets in the request is ignored"),
    ({"builds": [B(["std"], "ab"), B(["simd"], "ab")], "distinct_identifiers": 2},
     "distinct_identifiers in the request is ignored"),
    ({"builds": [B(["std"], None), B(["simd"], "ab")], "distinct_identifiers": 1},
     "distinct_identifiers in the request does not stand in for an unmeasured one"),
]

KEPLER_BASE = {"e": 0.5, "n": 200, "periods": 4}
KEPLER_CASES = [({"e": 0.5, "n": 500, "periods": 20}, "source configuration"),
                ({"e": 0.35, "n": 400, "periods": 8}, ""), ({"e": 0.65, "n": 151, "periods": 6}, "odd n")]


def design_for(law):
    if law["name"].startswith("kepler_energy_bounded"):
        return {"base": KEPLER_BASE, "cases": KEPLER_CASES, "edges": {}}
    return DESIGN[law["name"]]


def realise(law, c, d):
    """A full request for corner c (or None when the corner is not for one request)"""
    base = dict(d.get("base", {}))
    rebuild = d.get("rebuild", lambda i: i)
    t = c["type"]
    if t in ("bound", "piece"):
        # law_corners found a point that satisfies every scalar x-range: use it
        return rebuild({**base, **c.get("witness", {}), c["input"]: c["value"]})
    if t == "outside":
        lo_hi = next(v["range"] for v in law["inputs"] if v["name"] == c["input"])
        span = lo_hi[1] - lo_hi[0]
        x = (c["below"] - max(1, span * 1e-3) if c["input"] in law["integers"] else c["below"] - span * 1e-3) \
            if "below" in c else \
            (c["above"] + 1 if c["input"] in law["integers"] else c["above"] + span * 1e-3)
        return {**rebuild(base), c["input"]: x}
    if t == "non-integer":
        return {**base, c["input"]: base[c["input"]] + 0.5}
    if t in ("edge", "beyond"):
        return d["edges"][c["id"]]
    if t == "vertex":
        return rebuild({**base, **c["point"]})
    raise ValueError(t)


# ------------------------------------------------------------------ vectors

def vector(law, inputs, note):
    """Expected outcome of one request, from the law file"""
    v = {"law": law["name"], "inputs": inputs, "note": note}
    if law["kind"] == "audit":
        # inputs None: the request has no inputs key, read as an empty object
        nums, ranges = x_metrics(law, {} if inputs is None else inputs)
        verdict, subject = audit_verdict(law, nums, ranges)
        v.update(kind="verdict", expected={"verdict": verdict, "subject": subject}, tolerance=None)
        return v
    # in range, but an intermediate value is not finite in double: rejected as well
    # (conformance/TASK.md: every intermediate value of an expression is finite)
    if not valid(law, inputs) or not lc.finite_in_double(law, inputs):
        v.update(kind="reject", expected=None, tolerance=None)
        return v
    if law["name"].startswith("kepler_energy_bounded"):
        v.update(kind="invariant", expected={"growth_max": 1.2, "order_ratio": [3, 5],
                                             "max_n": next(x["range"][1] for x in law["inputs"] if x["name"] == "n"),
                                             "state_tol": 1e-7}, tolerance=None)
        return v
    periodic = law.get("_periodic")
    exp, tol = closed_form(law, inputs, periodic)
    v.update(kind="trajectory" if law["lists"] else "value", expected=exp, tolerance=tol)
    if periodic:
        v["periodic"] = periodic
    return v


def kepler_fill(args):
    law, v = args
    n_total = int(v["inputs"]["n"]) * int(v["inputs"]["periods"])
    st = kepler_states(law, v["inputs"], {1, n_total})
    v["expected"]["first_state"] = st[1]
    v["expected"]["final_state"] = st[n_total]
    return v


def build(law):
    d = design_for(law)
    vecs = [vector(law, i, n) for i, n in d["cases"]]
    for c in lc.corners(law):
        if any(lc.matches(c, law, v["inputs"], v["kind"] == "reject") for v in vecs):
            continue
        inputs = realise(law, c, d)
        v = vector(law, inputs, f"corner {c['id']}")
        if v["kind"] != "verdict" and (v["kind"] == "reject") == c["accept"]:
            raise SystemExit(f"{law['name']}: corner {c['id']} realised as {inputs}, "
                             f"which the law {'rejects' if c['accept'] else 'accepts'}")
        if not lc.matches(c, law, inputs, v["kind"] == "reject"):
            raise SystemExit(f"{law['name']}: corner {c['id']} not hit by {inputs}")
        vecs.append(v)
    return vecs + request_vectors(law)


def request_vectors(law):
    """The request contract: inputs null is the same as no inputs key; inputs that is
    not an object is an error of the request (exit 2); a quantitative law with no
    inputs misses its inputs (exit 2)"""
    out = []
    for lit, note in [([], "inputs given as an array"), ("hh", "inputs given as text")]:
        out.append({"law": law["name"], "inputs": None, "request_inputs": lit, "note": note,
                    "kind": "request_error", "expected": None, "tolerance": None})
    if law["kind"] == "audit":
        v = vector(law, None, "inputs given as null")
        v["request_inputs"] = None
        out.append(v)
    else:
        for lit, note in [(None, "inputs given as null"), ({}, "inputs empty")]:
            out.append({"law": law["name"], "inputs": None, "request_inputs": lit, "note": note,
                        "kind": "request_error", "expected": None, "tolerance": None})
    return out


def load_tools(tools):
    global lc, ls
    sys.path.insert(0, tools)
    import law_corners
    import law_schema
    lc, ls = law_corners, law_schema


def main():
    ap = argparse.ArgumentParser()
    root = Path(__file__).resolve().parent.parent
    ap.add_argument("--laws", default=str(root / "laws" / "spike"))
    ap.add_argument("--tools", default=str(root / "scripts"), help="directory with law_corners.py")
    ap.add_argument("--out", required=True)
    ap.add_argument("--jobs", type=int, default=8)
    a = ap.parse_args()
    load_tools(a.tools)
    files = sorted(Path(a.laws).glob("*.law"))
    if not files:
        raise SystemExit("no law files")
    V = []
    for f in files:
        law = lc.load(f)
        law["_file"] = str(f)
        for line in f.read_text().splitlines():
            w = line.split("#", 1)[0].split()
            if w[:1] == ["x-periodic"]:
                law["_periodic"] = float(w[2])
        V += build(law)
    kep = [(lc.load(next(f for f in files if f.stem == v["law"])), v) for v in V if v["kind"] == "invariant"]
    for law, _ in kep:
        law["_file"] = ""
    with ProcessPoolExecutor(a.jobs, initializer=load_tools, initargs=(a.tools,)) as ex:
        done = list(ex.map(kepler_fill, kep))
    it = iter(done)
    V = [next(it) if v["kind"] == "invariant" else v for v in V]
    Path(a.out).write_text(json.dumps(V, indent=1))
    from collections import Counter
    c = Counter(v["law"] for v in V)
    r = Counter(v["law"] for v in V if v["kind"] == "reject")
    for k in c:
        print(f"{k}: {c[k]} vectors ({r[k]} rejections)", file=sys.stderr)
    print("total", len(V), file=sys.stderr)


if __name__ == "__main__":
    main()
