#!/usr/bin/env python3
"""Separation of two Kepler trajectories that start 1 ulp apart.

usage: kepler_divergence.py <kdk|dkd> <e> <n> <periods> [--every K]
       kepler_divergence.py --report      (the table quoted in the Kepler law files)

Both runs use the law's method with the step h = 2 pi / n from apoapsis
(x = 1 + e, vy = sqrt((1 - e) / (1 + e))); the second run starts with x moved
to the next larger double. For each period the script prints the largest
component difference max(|dx|, |dy|, |dvx|, |dvy|) over the steps of that
period. A regular (non-chaotic) stepped orbit keeps the separation near the
rounding level; a chaotic one grows it to O(1). Standard library only.
"""
import argparse
import math


def run(method, e, n, periods, x0):
    h = 2 * math.pi / n
    x, y, vx, vy = x0, 0.0, 0.0, math.sqrt((1 - e) / (1 + e))

    def acc(x, y):
        r2 = x * x + y * y
        k = 1 / (r2 * math.sqrt(r2))
        return -x * k, -y * k

    out = []
    ax, ay = acc(x, y)
    for _ in range(periods):
        per = []
        for _ in range(n):
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
            per.append((x, y, vx, vy))
        out.append(per)
    return out


def separation(method, e, n, periods):
    a = run(method, e, n, periods, 1 + e)
    b = run(method, e, n, periods, math.nextafter(1 + e, math.inf))
    return [max(max(abs(p - q) for p, q in zip(s, t)) for s, t in zip(pa, pb)) for pa, pb in zip(a, b)]


REPORT_INSIDE = {"kdk": 0.77, "dkd": 0.765}           # the upper bound of e in the law files
REPORT_N_INSIDE = [100, 101, 102, 103, 104, 105, 106, 107, 131]
REPORT_N_OUTSIDE = [100, 101, 103, 105, 107]         # at e = 0.8, outside both ranges


def report():
    for m, e in REPORT_INSIDE.items():
        seps = {n: max(separation(m, e, n, 80)) for n in REPORT_N_INSIDE}
        for n, d in seps.items():
            print(f"{m} e = {e} n = {n:4d}: max separation over 80 periods {d:.2e}")
        print(f"{m} e = {e}: range {min(seps.values()):.1e} .. {max(seps.values()):.1e}")
    for m in REPORT_INSIDE:
        firsts = []
        for n in REPORT_N_OUTSIDE:
            sep = separation(m, 0.8, n, 80)
            p = next((i for i, d in enumerate(sep, 1) if d > 0.1), None)
            firsts.append(p)
            print(f"{m} e = 0.8 n = {n:4d}: first period with separation > 0.1: {p}")
        if None in firsts:
            print(f"{m} e = 0.8: some n never exceed 0.1 within 80 periods")
        else:
            print(f"{m} e = 0.8: first period > 0.1 in {min(firsts)} .. {max(firsts)}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--report", action="store_true")
    ap.add_argument("method", nargs="?", choices=["kdk", "dkd"])
    ap.add_argument("e", nargs="?", type=float)
    ap.add_argument("n", nargs="?", type=int)
    ap.add_argument("periods", nargs="?", type=int)
    ap.add_argument("--every", type=int, default=1, help="print every K-th period")
    a = ap.parse_args()
    if a.report:
        report()
        return
    if a.periods is None:
        ap.error("give <kdk|dkd> <e> <n> <periods>, or --report")
    sep = separation(a.method, a.e, a.n, a.periods)
    for p, d in enumerate(sep, 1):
        if p % a.every == 0 or p == 1:
            print(f"period {p:3d}  max separation {d:.3e}")
    print(f"max over all periods {max(sep):.3e}")


if __name__ == "__main__":
    main()
