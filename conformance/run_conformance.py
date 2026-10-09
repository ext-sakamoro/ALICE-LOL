#!/usr/bin/env python3
"""Runs an implementation against a conformance corpus made by gen_corpus.py.

usage: run_conformance.py --corpus FILE [--law NAME] [--timeout S] [--show N] -- <command...>

The command is spawned once per request with {"law", "inputs"} on stdin and must
print {"outputs": {...}} or {"rejected": "..."} on stdout.

Failure types: wrong value (> 10x tolerance), out-of-tolerance (1x..10x),
crash (exit code, timeout, unreadable or incomplete output), range not enforced
(expected a rejection, got outputs), spurious rejection (rejected an in-range
input), wrong verdict.  Exit 0 when every vector passed, 1 when any failed,
2 when no vector ran.
"""
import argparse, json, math, os, subprocess, sys
from collections import OrderedDict, defaultdict
from pathlib import Path

# diagnostic only: skip the first / last state comparison to measure what the two
# energy invariants catch on their own (never used for a score)
INVARIANTS_ONLY = os.environ.get("KEPLER_INVARIANTS_ONLY") == "1"

FAIL_TYPES = ["wrong value", "out-of-tolerance", "crash", "range not enforced",
              "spurious rejection", "wrong verdict"]


class Fail(Exception):
    def __init__(self, kind, detail):
        super().__init__(detail)
        self.kind, self.detail = kind, detail


def call(cmd, law, inputs, timeout):
    req = json.dumps({"law": law, "inputs": inputs})
    try:
        p = subprocess.run(cmd, input=req, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        raise Fail("crash", f"timeout after {timeout}s")
    except OSError as e:
        raise Fail("crash", f"cannot run: {e}")
    if p.returncode != 0:
        raise Fail("crash", f"exit {p.returncode}: {p.stderr.strip()[-300:]}")
    try:
        out = json.loads(p.stdout)
    except ValueError:
        raise Fail("crash", f"stdout is not JSON: {p.stdout[:200]!r}")
    if not isinstance(out, dict) or ("outputs" in out) == ("rejected" in out):
        raise Fail("crash", f"expected exactly one of outputs / rejected: {str(out)[:200]}")
    return out


def outputs_of(out):
    if "rejected" in out:
        raise Fail("spurious rejection", f"rejected: {out['rejected']}")
    if not isinstance(out["outputs"], dict):
        raise Fail("crash", "outputs is not an object")
    return out["outputs"]


def num(x, what):
    if isinstance(x, bool) or not isinstance(x, (int, float)) or not math.isfinite(x):
        raise Fail("crash", f"{what} is not a finite number: {x!r}")
    return float(x)


def compare(name, got, want, tol, periodic=None):
    got = num(got, name)
    err = abs(got - want)
    if periodic:
        err = abs((got - want + periodic / 2) % periodic - periodic / 2)
    if err <= tol:
        return
    kind = "out-of-tolerance" if err <= 10 * tol else "wrong value"
    raise Fail(kind, f"{name}: got {got!r}, expected {want!r} +- {tol!r} (|err| {err:.3g})")


def check_values(v, outs):
    periodic = v.get("periodic")
    for name, want in v["expected"].items():
        if name not in outs:
            raise Fail("crash", f"output `{name}` missing")
        tol = v["tolerance"][name]
        if isinstance(want, list):
            got = outs[name]
            if not isinstance(got, list) or len(got) != len(want):
                raise Fail("crash", f"{name}: expected a list of {len(want)}")
            for k, (g, w) in enumerate(zip(got, want)):
                compare(f"{name}[{k}] (t={v['inputs']['t'][k]})", g, w, tol)
        else:
            compare(name, outs[name], want, tol, periodic)


def kepler_rel(states, expected_len):
    """Relative energy error of every reported state (and the states as floats)"""
    if not isinstance(states, list) or len(states) != expected_len:
        raise Fail("wrong value" if isinstance(states, list) else "crash",
                   f"states: expected {expected_len} entries (n * periods), got "
                   f"{len(states) if isinstance(states, list) else type(states).__name__}")
    rel, out = [], []
    for s in states:
        if not isinstance(s, list) or len(s) != 4:
            raise Fail("crash", "a state is not [x, y, vx, vy]")
        x, y, vx, vy = (num(c, "state") for c in s)
        r = math.hypot(x, y)
        if r == 0:
            raise Fail("wrong value", "state at the origin")
        rel.append(abs(((vx * vx + vy * vy) / 2 - 1 / r + 0.5) / 0.5))
        out.append((x, y, vx, vy))
    return rel, out


def check_state(what, got, want, tol):
    err = max(abs(a - b) for a, b in zip(got, want))
    if err > tol:
        raise Fail("wrong value", f"{what}: got {[round(c, 9) for c in got]}, expected "
                   f"{[round(c, 9) for c in want]} (max |err| {err:.3g} > {tol:g}; another method, "
                   f"a missing or extra step, or states reported before the step)")


def kepler_mean_first(cmd, law, e, k, timeout):
    out = call(cmd, law, {"e": e, "n": k, "periods": 2}, timeout)
    rel, _ = kepler_rel(outputs_of(out).get("states"), 2 * k)
    return sum(rel[:k]) / k


def check_kepler(cmd, v, outs, timeout):
    """Length and step of the reported states, then both invariants of the law.

    first / last state: equal to the stated method's trajectory (mpmath, 30 digits)
    within state_tol; the spread over evaluation orders of r^3 is <= 2.2e-11 inside
    the valid range, one step moves the state by >= 1e-3.
    growth: max over steps n+1 .. periods n <= growth_max * max over steps 1 .. n
    order: k = n if 2n <= max_n else n // 2; mean over the first period with k and
    with 2k steps per period (two extra runs of 2 periods); ratio in order_ratio.
    """
    i, x = v["inputs"], v["expected"]
    n, p = int(i["n"]), int(i["periods"])
    rel, states = kepler_rel(outs.get("states"), n * p)
    if not INVARIANTS_ONLY:
        check_state("state after step 1", states[0], x["first_state"], x["state_tol"])
        check_state(f"state after step {n * p}", states[-1], x["final_state"], x["state_tol"])
    first, later = max(rel[:n]), max(rel[n:])
    if not first > 0:
        raise Fail("wrong value", "zero energy error in the first period (states not integrated)")
    if later > x["growth_max"] * first:
        raise Fail("wrong value", f"energy error grew: max after period 1 / max in period 1 = "
                   f"{later / first:.4f} > {x['growth_max']}")
    k = n if 2 * n <= x["max_n"] else n // 2
    m1 = sum(rel[:n]) / n if k == n else kepler_mean_first(cmd, v["law"], i["e"], k, timeout)
    m2 = kepler_mean_first(cmd, v["law"], i["e"], 2 * k, timeout)
    ratio = m1 / m2 if m2 > 0 else math.inf
    lo, hi = x["order_ratio"]
    if not lo <= ratio <= hi:
        raise Fail("wrong value", f"mean(k)/mean(2k) = {ratio:.4f} not in [{lo}, {hi}] (k = {k})")


def run_vector(cmd, v, timeout):
    out = call(cmd, v["law"], v["inputs"], timeout)
    if v["kind"] == "reject":
        if "rejected" not in out:
            raise Fail("range not enforced", f"returned outputs for {v['note']}")
        return
    outs = outputs_of(out)
    if v["kind"] in ("value", "trajectory"):
        check_values(v, outs)
    elif v["kind"] == "invariant":
        check_kepler(cmd, v, outs, timeout)
    elif v["kind"] == "verdict":
        got = (outs.get("verdict"), outs.get("subject"))
        want = (v["expected"]["verdict"], v["expected"]["subject"])
        if got != want:
            raise Fail("wrong verdict", f"got {got}, expected {want}")
    else:
        raise ValueError(v["kind"])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--corpus", required=True)
    ap.add_argument("--law")
    ap.add_argument("--timeout", type=float, default=120)
    ap.add_argument("--show", type=int, default=3, help="failure details shown per law")
    ap.add_argument("cmd", nargs=argparse.REMAINDER)
    a = ap.parse_args()
    cmd = a.cmd[1:] if a.cmd[:1] == ["--"] else a.cmd
    if not cmd:
        ap.error("no implementation command given")
    corpus = json.loads(Path(a.corpus).read_text())
    if a.law:
        corpus = [v for v in corpus if v["law"] == a.law]
    stats = OrderedDict()
    details = defaultdict(list)
    for v in corpus:
        s = stats.setdefault(v["law"], {"passed": 0, "failed": 0, "error": 0, **{k: 0 for k in FAIL_TYPES}})
        try:
            run_vector(cmd, v, a.timeout)
            s["passed"] += 1
        except Fail as f:
            s["error" if f.kind == "crash" else "failed"] += 1
            s[f.kind] += 1
            details[v["law"]].append(f"[{f.kind}] {v['note'] or v['inputs']}: {f.detail}")
    ran = sum(s["passed"] + s["failed"] + s["error"] for s in stats.values())
    w = max([len(k) for k in stats] + [3])
    short = ["wrong val", "out-of-tol", "crash", "range", "spurious", "verdict"]
    print(f"{'law':<{w}}  pass  fail   err  " + "  ".join(f"{h:>10}" for h in short))
    for law, s in stats.items():
        print(f"{law:<{w}}  {s['passed']:4d}  {s['failed']:4d}  {s['error']:4d}  "
              + "  ".join(f"{s[k]:10d}" for k in FAIL_TYPES))
    tot = {k: sum(s[k] for s in stats.values()) for k in ["passed", "failed", "error", *FAIL_TYPES]}
    print(f"{'TOTAL':<{w}}  {tot['passed']:4d}  {tot['failed']:4d}  {tot['error']:4d}  "
          + "  ".join(f"{tot[k]:10d}" for k in FAIL_TYPES))
    for law, d in details.items():
        print(f"\n{law}:")
        for line in d[:a.show]:
            print("  " + line)
        if len(d) > a.show:
            print(f"  ... {len(d) - a.show} more")
    if ran == 0:
        print("error: 0 vectors ran (compared nothing)", file=sys.stderr)
        sys.exit(2)
    sys.exit(0 if tot["failed"] + tot["error"] == 0 else 1)


if __name__ == "__main__":
    main()
