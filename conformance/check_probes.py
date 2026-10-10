#!/usr/bin/env python3
"""Check an implementation against the decided answers for inputs the laws used to leave open.

Each probe in conformance/probes.json is an input on which independent implementations
of the same law files gave different answers before TASK.md decided it. The expected
outcome is written by hand from TASK.md (not produced by the reference implementation):
`{"verdict", "subject"}` for an audit, "rejected", or "exit 2" (an error of the request).
The string "__INF__" in inputs is sent as the overflowing literal 1e400. A probe with "raw"
sends that text as the whole request (for JSON edge cases).

usage: check_probes.py [--probes conformance/probes.json] [--laws a,b,...] -- <command...>
--laws limits the check to the probes of those laws (for an implementation of a subset).
Exit 1 when any probe disagrees, 2 when no probe was compared.
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path


def request_text(p):
    if "raw" in p:
        # the request text exactly as written (JSON edge cases that json.dumps cannot spell)
        return p["raw"]
    req = {"law": p["law"]} if p.get("omit_inputs") else {"law": p["law"], "inputs": p["inputs"]}
    return json.dumps(req).replace('"__INF__"', "1e400")


def outcome(cmd, p, timeout):
    r = subprocess.run(cmd, input=request_text(p), capture_output=True, text=True, timeout=timeout)
    if r.returncode == 2:
        return "exit 2" if not r.stdout.strip() else f"exit 2 with stdout {r.stdout[:60]!r}"
    if r.returncode != 0:
        return f"exit {r.returncode}"
    try:
        out = json.loads(r.stdout)
    except ValueError:
        return f"stdout is not JSON: {r.stdout[:60]!r}"
    if "rejected" in out:
        return "rejected"
    o = out.get("outputs", {})
    return {"verdict": o.get("verdict"), "subject": o.get("subject")}


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--probes", default=str(Path(__file__).with_name("probes.json")))
    ap.add_argument("--timeout", type=float, default=60)
    ap.add_argument("--laws", help="comma separated law names; only their probes are checked")
    ap.add_argument("cmd", nargs=argparse.REMAINDER)
    a = ap.parse_args(argv)
    cmd = a.cmd[1:] if a.cmd[:1] == ["--"] else a.cmd
    if not cmd:
        ap.error("no implementation command given")
    probes = json.loads(Path(a.probes).read_text(encoding="utf-8"))
    if a.laws:
        keep = set(a.laws.split(","))
        probes = [p for p in probes if p["law"] in keep]
    bad = 0
    for p in probes:
        got = outcome(cmd, p, a.timeout)
        if got != p["expect"]:
            bad += 1
            print(f"DIFF {p['law']}: {p['note']}: expected {p['expect']}, got {got}")
    print(f"compared {len(probes)} probes, disagreements {bad}")
    if not probes:
        print("error: no probe compared", file=sys.stderr)
        return 2
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
