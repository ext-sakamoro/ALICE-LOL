#!/usr/bin/env python3
"""Re-integrates spot points of the Kepler sweep at 50 digits and compares with double.

usage: kepler_mp_check.py        (needs mpmath, see conformance/requirements.txt)

For each spot point (method, e, n) the same stepped orbit is integrated twice,
in double and with mpmath at 50 digits, over 40 periods plus one period with
2n steps. Both report the growth ratio G (max error over periods 2..40 / max in
period 1), the order ratio of means and the max-based order ratio; the energy
error is evaluated against the exact E0 = -1/2. The largest difference over all
points is printed; it is the rounding effect on the statistics, to be compared
with the margins in conformance/kepler_range.md.
"""
import math
import sys

import mpmath as mp

POINTS = [("kdk", "0.770", 100), ("kdk", "0.770", 101), ("kdk", "0.735", 103), ("kdk", "0.764", 101),
          ("kdk", "0.794", 101), ("dkd", "0.765", 100), ("dkd", "0.765", 101), ("dkd", "0.789", 103)]


def stats(method, e, n, P, num, sqrt, pi):
    def run(nn, periods):
        h = 2 * pi / nn
        r0 = 1 + e
        x, y, vx, vy = r0, num(0), num(0), sqrt(2 / r0 - 1)
        e0 = num(-1) / 2

        def acc(x, y):
            r2 = x * x + y * y
            k = 1 / (r2 * sqrt(r2))
            return -x * k, -y * k

        ax, ay = acc(x, y)
        pm, pa = [], []
        for _ in range(periods):
            m = s = num(0)
            for _ in range(nn):
                if method == "kdk":
                    vx += h / 2 * ax; vy += h / 2 * ay; x += h * vx; y += h * vy
                    ax, ay = acc(x, y); vx += h / 2 * ax; vy += h / 2 * ay
                else:
                    x += h / 2 * vx; y += h / 2 * vy; bx, by = acc(x, y)
                    vx += h * bx; vy += h * by; x += h / 2 * vx; y += h / 2 * vy
                en = (vx * vx + vy * vy) / 2 - 1 / sqrt(x * x + y * y)
                r = abs((en - e0) / e0)
                s += r
                m = max(m, r)
            pm.append(m)
            pa.append(s / nn)
        return pm, pa

    pm, pa = run(n, P)
    pm2, pa2 = run(2 * n, 1)
    return max(pm[1:]) / pm[0], pa[0] / pa2[0], pm[0] / pm2[0]


def main():
    mp.mp.dps = 50
    worst = 0.0
    for method, e, n in POINTS:
        d = stats(method, float(e), n, 40, float, math.sqrt, math.pi)
        q = stats(method, mp.mpf(e), n, 40, mp.mpf, mp.sqrt, mp.pi)
        diff = max(abs(a - float(b)) for a, b in zip(d, q))
        worst = max(worst, diff)
        print(f"{method} e = {e} n = {n}: G {float(q[0]):.10f} O_mean {float(q[1]):.10f} "
              f"O_max {float(q[2]):.10f} (50 digits); largest difference to double {diff:.1e}")
    print(f"largest difference over {len(POINTS)} points: {worst:.1e}")
    return 0 if POINTS else 1


if __name__ == "__main__":
    sys.exit(main())
