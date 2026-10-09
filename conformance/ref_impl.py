#!/usr/bin/env python3
"""Reference implementation of the 9 laws in laws/spike, used to test the runner and the corpus.

Reads {"law": ..., "inputs": {...}} on stdin, prints {"outputs": {...}} or {"rejected": "..."}.
REF_BUG=1: terminal velocity drag force without the factor 1/2
REF_BUG=2: ISA altitude range not enforced
REF_BUG=3: identifier audit ignores the "at least 2 feature sets" rule
REF_BUG=4: Kepler integrated with RK4 (not the stated method)
REF_BUG=5: Kepler drops the last step (n * periods - 1 states)
REF_BUG=6: Kepler evaluates r^3 as r2 ** 1.5 (another rounding; must still conform)
REF_BUG=7: Kepler reports the state before each step (initial state first, last step missing)
REF_BUG=8: Kepler evaluates r^3 as sqrt(r2) ** 3 (another rounding; must still conform)
"""
import json, math, os, sys

BUG = os.environ.get("REF_BUG", "")


class Reject(Exception):
    pass


def rng(name, x, lo, hi):
    if not isinstance(x, (int, float)) or isinstance(x, bool) or not math.isfinite(x) or x < lo or x > hi:
        raise Reject(f"{name}={x} outside [{lo}, {hi}]")


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
    ts = i["t"]
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
    ts = i["t"]
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
    return {"theta4": math.atan2(by, bx - lg)}


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


def audit(clauses, nums, ranges):
    """clauses: list of (kind, ...) in file order; returns (verdict, subject)"""
    for c in clauses:
        if c[0] == "evidence" and nums.get(c[1], 0) == 0:
            return "no_evidence", c[1]
    for c in clauses:
        if c[0] == "range":
            got = ranges.get(c[1])
            if got is None:
                return "out_of_range", c[1]
            if sorted(got, key=vkey) != sorted(c[2], key=vkey):
                return "parameter_update", c[1]
    for c in clauses:
        if c[0] == "expect":
            got = nums.get(c[1])
            if got is None:
                return "undecided", c[1]
            if abs(got - c[2]) > c[3]:
                return "breaks", c[1]
    return "supports", None


def gate(i):
    nums = {k: v for k, v in i.items() if not isinstance(v, list)}
    ranges = {k: v for k, v in i.items() if isinstance(v, list)}
    v, s = audit([("evidence", "compared"), ("expect", "mismatches", 0.0, 0.0),
                  ("range", "known-mismatches", ["case-17", "case-42"])], nums, ranges)
    return {"verdict": v, "subject": s}


def ident(i):
    builds = i.get("builds") or []
    nums = {"builds": len(builds)}
    sets = {frozenset(b.get("features", [])) for b in builds}
    fs = len(sets)
    nums["feature_sets"] = fs if (fs >= 2 or BUG == "3") else 0
    if builds and all(b.get("id") is not None for b in builds):
        nums["distinct_identifiers"] = len({b["id"] for b in builds})
    v, s = audit([("evidence", "builds"), ("evidence", "feature_sets"),
                  ("expect", "distinct_identifiers", 1.0, 0.0)], nums, {})
    return {"verdict": v, "subject": s}


LAWS = {
    "terminal_velocity_quadratic_drag": terminal,
    "first_order_decay": decay,
    "pd_step_overshoot": pd,
    "kepler_energy_bounded_kdk": kepler("kdk", 0.77),
    "kepler_energy_bounded_dkd": kepler("dkd", 0.765),
    "four_bar_rocker_angle": four_bar,
    "isa1976_lower_atmosphere": isa,
    "gate_compares_nonzero": gate,
    "identifier_feature_independent": ident,
}


def main():
    req = json.load(sys.stdin)
    try:
        out = {"outputs": LAWS[req["law"]](req["inputs"])}
    except Reject as e:
        out = {"rejected": str(e)}
    json.dump(out, sys.stdout)


if __name__ == "__main__":
    main()
