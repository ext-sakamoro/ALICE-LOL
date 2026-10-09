#!/usr/bin/env python3
"""Measures the valid range of the two Kepler laws and the stability of their verdict.

usage: kepler_sweep.py [--grid coarse|fine|both] [--rk4] [--jobs N] [--check]

Builds conformance/kepler_sweep.c with `cc` into a temporary directory and runs it.

A cell is (method, e, n), measured with 40 periods and with each of the four
evaluation orders of 1/r^3 listed in kepler_sweep.c:
- coarse grid: e = 0.200, 0.205, .., 0.800 (121 values) and
  n in {100, 101, .., 300} + {301, 338, .., 1997} + {2000} (248 values)
- fine grid:   e = 0.700, 0.701, .., 0.800 (101 values) and n = 100 .. 200 (101 values)

Criteria (per cell and order):
- new (the law files): growth G = max error over periods 2..40 / max error in period 1 <= 1.2,
  and order O = mean error in period 1 with n steps / with 2n steps in [3, 5]
- old: the same growth, and the max-based order ratio max(n) / max(2n) in [3, 5]

A cell passes when it passes for all four orders. Reported per method:
first failing e (new and old), the range e <= first failure - 0.02 rounded down to
0.005, the largest G and smallest O inside that range with their cells, the largest
spread of G and O over the four orders, the number of cells inside the range and of
those that do not pass, and the cells whose verdict depends on the order.
--check compares the range with the `input e` line of laws/spike/kepler_energy_bounded_<method>.law
and exits 1 when they differ. Standard library only (needs `cc`).
"""
import argparse
import math
import os
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent

COARSE_E = [round(0.2 + 0.005 * i, 3) for i in range(121)]
COARSE_N = list(range(100, 301)) + list(range(301, 2001, 37)) + [2000]
FINE_E = [round(0.7 + 0.001 * i, 3) for i in range(101)]
FINE_N = list(range(100, 201))
PERIODS = 40


def build(tmp):
    exe = Path(tmp) / "kepler_sweep"
    subprocess.run(["cc", "-O2", "-o", str(exe), str(HERE / "kepler_sweep.c"), "-lm"], check=True)
    return exe


def measure(exe, method, e, ns):
    """{(order, n): (G, O_mean, O_max)}"""
    out = subprocess.run([str(exe), method, f"{e:.3f}", str(PERIODS), *map(str, ns)],
                         check=True, capture_output=True, text=True).stdout
    res = {}
    for line in out.splitlines():
        w = line.split()
        o, n = int(w[1]), int(w[3])
        f2, g2 = float(w[4]), float(w[5])
        v = list(map(float, w[6:]))
        pm, pa = v[0::2], v[1::2]
        res[(o, n)] = (max(pm[1:]) / pm[0], pa[0] / g2, pm[0] / f2)
    return res


def passes(stat, crit):
    g, om, ox = stat
    return g <= 1.2 and 3 <= (om if crit == "new" else ox) <= 5


def sweep(exe, method, es, ns, jobs):
    with ThreadPoolExecutor(jobs) as ex:
        results = list(ex.map(lambda e: (e, measure(exe, method, e, ns)), es))
    return {(e, n): [r[(o, n)] for o in range(4)] for e, r in results for n in ns}


def first_failures(cells):
    rep = {}
    for crit in ("new", "old"):
        fails = [(e, n) for (e, n), st in cells.items() if not all(passes(s, crit) for s in st)]
        rep[crit] = min(fails) if fails else None
    return rep


def range_hi(first_fail_e):
    return round(math.floor(round((first_fail_e - 0.02) / 0.005, 6)) * 0.005, 3)


def analyse(cells, hi):
    rep = {}
    inside = {k: v for k, v in cells.items() if k[0] <= hi + 1e-9}
    rep["cells_in_range"] = len(inside)
    rep["cells_in_range_not_passing"] = sum(not all(passes(s, "new") for s in st) for st in inside.values())
    rep["max_growth"] = max((max(s[0] for s in st), k) for k, st in inside.items())
    rep["min_order"] = min((min(s[1] for s in st), k) for k, st in inside.items())
    rep["spread_growth"] = max(max(s[0] for s in st) - min(s[0] for s in st) for st in inside.values())
    rep["spread_order"] = max(max(s[1] for s in st) - min(s[1] for s in st) for st in inside.values())
    dep = sorted(k for k, st in cells.items() if len({passes(s, "new") for s in st}) > 1)
    rep["order_dependent_cells"] = dep
    rep["order_dependent_in_range"] = [k for k in dep if k[0] <= hi + 1e-9]
    return rep


def law_e_hi(method):
    text = (ROOT / "laws" / "spike" / f"kepler_energy_bounded_{method}.law").read_text(encoding="utf-8")
    for line in text.splitlines():
        w = line.split("#", 1)[0].split()
        if w[:2] == ["input", "e"]:
            return float(w[5])
    raise SystemExit(f"no `input e` line for {method}")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--grid", choices=["coarse", "fine", "both"], default="both")
    ap.add_argument("--rk4", action="store_true", help="also report RK4 on the coarse grid")
    ap.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args()
    if not shutil.which("cc"):
        raise SystemExit("needs a C compiler on PATH as `cc`")
    bad = 0
    with tempfile.TemporaryDirectory() as tmp:
        exe = build(tmp)
        grids = {"coarse": (COARSE_E, COARSE_N), "fine": (FINE_E, FINE_N)}
        names = ["coarse", "fine"] if a.grid == "both" else [a.grid]
        for method in ("kdk", "dkd"):
            cells = {g: sweep(exe, method, *grids[g], a.jobs) for g in names}
            ff = {g: first_failures(cells[g]) for g in names}
            for g in names:
                print(f"{method} {g}: {len(cells[g])} cells x 4 orders; first failure (e, n): "
                      f"new criteria {ff[g]['new']}, old criteria {ff[g]['old']}")
            first = min(ff[g]["new"] for g in names if ff[g]["new"])
            hi = range_hi(first[0])
            law = law_e_hi(method)
            same = abs(hi - law) < 1e-9
            print(f"{method}: first failure e = {first[0]} (n = {first[1]}) -> range e <= {hi}, "
                  f"margin {first[0] - hi:.3f}; law file: e <= {law} {'(same)' if same else '(DIFFERENT)'}")
            bad += a.check and not same
            for g in names:
                r = analyse(cells[g], hi)
                print(f"  {g}, e <= {hi}: {r['cells_in_range']} cells, "
                      f"{r['cells_in_range_not_passing']} not passing under all 4 orders")
                print(f"    max growth {r['max_growth'][0]:.6f} at (e, n) = {r['max_growth'][1]}; "
                      f"min order {r['min_order'][0]:.6f} at (e, n) = {r['min_order'][1]}")
                print(f"    largest spread over the 4 orders: growth {r['spread_growth']:.2e}, "
                      f"order {r['spread_order']:.2e}")
                print(f"    order-dependent verdicts: {len(r['order_dependent_cells'])} cells "
                      f"{r['order_dependent_cells'][:8]}; inside the range {len(r['order_dependent_in_range'])}")
                if r["cells_in_range"] == 0:
                    print("    error: 0 cells inside the range", file=sys.stderr)
                    bad += 1
        if a.rk4:
            cells = sweep(exe, "rk4", [e for e in COARSE_E if e <= 0.77 + 1e-9], COARSE_N, a.jobs)
            om = [s[1] for st in cells.values() for s in st]
            print(f"rk4 coarse, e <= 0.77: order ratio of means in [{min(om):.2f}, {max(om):.2f}] "
                  f"over {len(cells)} cells x 4 orders")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
