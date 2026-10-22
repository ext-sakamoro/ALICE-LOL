#!/usr/bin/env python3
"""Oracle for scripts/law_rules.py (rule laws: reading, answering, corpus, coverage).

The machine table is written out by hand from the rule in words; alice-lol's
tests/rule_law_oracle.rs holds the same cases for the Rust reader, so the two readers
are pinned to the same expectations independently of the generated corpus.
Standard library only.
"""
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import law_rules as R  # noqa: E402

SPIKE = Path(__file__).resolve().parent.parent / "laws" / "spike"


def law(name):
    return R.parse((SPIKE / f"{name}.law").read_text(encoding="utf-8"))


# (state, event, device_proof, new state, taken, analytics in the strict mode)
STRICT = [
    ("undecided", "accept", None, "accepted", True, True),
    ("undecided", "decline", None, "declined", True, False),
    ("undecided", "withdraw", False, "undecided", False, False),
    ("undecided", "withdraw", True, "undecided", False, False),
    ("accepted", "accept", None, "accepted", False, True),
    ("accepted", "decline", None, "accepted", False, True),
    ("accepted", "withdraw", False, "accepted", False, True),
    ("accepted", "withdraw", True, "declined", True, False),
    ("declined", "accept", None, "accepted", True, True),
    ("declined", "decline", None, "declined", False, False),
    ("declined", "withdraw", False, "declined", False, False),
    ("declined", "withdraw", True, "declined", False, False),
]

KEY = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
COOKIE = "42.7.1893456000:b13155066f4d53246b4b26c896d79b7f"


def cookie(c=COOKIE, now=0, key=KEY, version=7):
    return {"cookie": c, "now": now, "key": key, "user": {"session_version": version}}


class Machine(unittest.TestCase):
    def test_every_step_in_both_modes(self):
        m = law("consent_choice")
        n = 0
        for mode in ("strict", "relaxed"):
            for state, event, proof, new, taken, strict_analytics in STRICT:
                inp = {"mode": mode, "state": state, "event": event,
                       "guards": {} if proof is None else {"device_proof": proof}}
                kind, out = R.respond(m, inp)
                self.assertEqual(kind, "outputs")
                self.assertEqual(out, {"state": new, "transition": "taken" if taken else "refused",
                                       "allow": {"analytics": mode == "relaxed" or strict_analytics}}, inp)
                n += 1
        self.assertEqual(n, 24)

    def test_rejections_and_errors(self):
        m = law("consent_choice")
        base = {"mode": "strict", "state": "accepted", "event": "accept"}
        for change in [{"state": "nowhere"}, {"state": 1}, {"event": "jump"}, {"mode": "lenient"},
                       {"event": "withdraw"}, {"event": "withdraw", "guards": {"device_proof": 1}},
                       {"guards": []}]:
            self.assertEqual(R.respond(m, {**base, **change})[0], "rejected", change)
        for k in ("mode", "state", "event"):
            self.assertEqual(R.respond(m, {x: v for x, v in base.items() if x != k})[0], "request_error", k)

    def test_first_match(self):
        m = R.parse("kind machine\nstate a initial\nstate b\nstate c\nevent go\non go from a to b\non go from a to c\n")
        self.assertEqual(R.respond(m, {"state": "a", "event": "go"}),
                         ("outputs", {"state": "b", "transition": "taken", "allow": {}}))


class Predicate(unittest.TestCase):
    def failed(self, inp):
        kind, out = R.respond(law("session_cookie_valid"), inp)
        self.assertEqual(kind, "outputs", inp)
        return out["failed"]

    def test_example_and_boundaries(self):
        self.assertIsNone(self.failed(cookie(now=1893455999)))
        self.assertEqual(self.failed(cookie(now=1893456000)), "fresh")
        self.assertEqual(self.failed(cookie(now=1893456001)), "fresh")
        for v in (6, 8):
            self.assertEqual(self.failed(cookie(version=v)), "current")

    def test_tag(self):
        for c in ["42.7.1893456000:313155066f4d53246b4b26c896d79b7f",
                  "42.7.1893456000:b13155066f4d53246b4b26c896d79b7e",
                  "42.7.1893456000:b13155066f4d53246b4b26c896d79b",
                  "42.7.1893456000:b13155066f4d53246b4b26c896d79b7f00",
                  "43.7.1893456000:b13155066f4d53246b4b26c896d79b7f",
                  "42.8.1893456000:b13155066f4d53246b4b26c896d79b7f",
                  "42.7.1893456001:b13155066f4d53246b4b26c896d79b7f"]:
            self.assertEqual(self.failed(cookie(c)), "tag", c)
        self.assertEqual(self.failed(cookie(key="ff" + KEY[2:])), "tag")

    def test_shape(self):
        for c in ["42.1893456000:b13155066f4d53246b4b26c896d79b7f", "42.7.1893456000",
                  "42.7.1893456000:b1:31", "42.07.1893456000:b13155066f4d53246b4b26c896d79b7f",
                  "42.+7.1893456000:b13155066f4d53246b4b26c896d79b7f",
                  "42.x.1893456000:b13155066f4d53246b4b26c896d79b7f",
                  "42.7.1893456000:B13155066F4D53246B4B26C896D79B7F", "42.7.1893456000:b13",
                  "42.7.9007199254740993:b13155066f4d53246b4b26c896d79b7f"]:
            self.assertEqual(self.failed(cookie(c)), "shape", c)

    def test_types_and_missing(self):
        p = law("session_cookie_valid")
        for now in (1.5, "1", True, float("inf"), 9007199254740993, 9007199254740992):
            self.assertEqual(R.respond(p, cookie(now=now))[0], "rejected", now)
        self.assertEqual(R.respond(p, cookie(key="zz"))[0], "rejected")
        self.assertEqual(R.respond(p, {**cookie(), "user": {}})[0], "rejected")
        self.assertEqual(R.respond(p, {k: v for k, v in cookie().items() if k != "now"})[0], "request_error")

    def test_rfc_4231_case_2(self):
        p = R.parse("kind predicate\nuse alice-sign hmac_sha256\nx-input k bytes\nx-input m text\n"
                    "require mac: hmac_sha256(k, utf8(m)) == 0x5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843\n"
                    'example {"k": "4a656665", "m": "what do ya want for nothing?"}\n')
        self.assertEqual(R.respond(p, {"k": "4a656665", "m": "what do ya want for nothing?"}),
                         ("outputs", {"valid": True, "failed": None}))


M = "kind machine\nstate a initial\nevent go\non go from a to a\n"
P = 'kind predicate\nx-input n integer\nrequire pos: n > 0\nexample {"n": 1}\n'

BAD = [
    M + "colour red\n", M + "kind predicate\n", M + "state a\n", M + "state mode\n", M + "state b initial\n",
    "kind machine\nstate a\nevent go\non go from a to a\n",
    "kind machine\nstate a initial\nevent go\non go from b to a\n",
    "kind machine\nstate a initial\nevent go\non stop from a to a\n",
    "kind machine\nstate a initial\nevent go\nevent stop guard ok\non go from a to a when ok\n",
    "kind machine\nstate a initial\nevent go\non go from a to a\nallow x when mode == m\n",
    P.replace("n > 0", "m > 0"), P.replace("n > 0", "0 < n < 2"),
    'kind predicate\nx-input t text\nrequire o: t < t\nexample {"t": "a"}\n',
    'kind predicate\nx-input t time\nrequire o: t < 5\nexample {"t": 1}\n',
    'kind predicate\nx-input k bytes\nrequire o: hmac_sha256(k, k) == k\nexample {"k": ""}\n',
    P + "use alice-sign ed25519_verify\n", P.replace("n > 0", "n"),
    'kind predicate\nx-input n integer\nexample {"n": 1}\n', P.replace('example {"n": 1}\n', ""),
    P.replace('{"n": 1}', '{"n": 0}'), P.replace("x-input n", "x-input  n"), P + "x-metric c = count(n)\n",
    P.replace("x-input n", " x-input n"), P + 'const c integer = "1"\n', P + "const c integer = 9007199254740993\n",
    P + "const c bytes = 0x0\n", P + "claim a\u0007b\n",
]


class Reading(unittest.TestCase):
    def test_files_that_do_not_read(self):
        for i, text in enumerate(BAD):
            with self.assertRaises(R.RuleError, msg=f"case {i}:\n{text}"):
                R.parse(text)
        R.parse(M)
        R.parse(P)

    def test_kind(self):
        self.assertEqual(R.rule_kind("kind audit\nkind machine\n"), None)
        self.assertEqual(R.rule_kind((SPIKE / "gate_compares_nonzero.law").read_text()), None)
        self.assertEqual(R.rule_kind((SPIKE / "consent_choice.law").read_text()), "machine")


class Corpus(unittest.TestCase):
    def test_vectors_cover_and_coverage_has_teeth(self):
        for name in ("consent_choice", "session_cookie_valid"):
            lw = law(name)
            v = R.vectors(lw, name)
            self.assertEqual(R.coverage(lw, v), [], name)
            # a corpus without one kind of vector is caught
            self.assertNotEqual(R.coverage(lw, [x for x in v if x["kind"] != "reject"]), [], name)
            if name.startswith("session"):
                # no vector failing `fresh`
                cut = [x for x in v if x["kind"] != "exact" or x["expected"]["failed"] != "fresh"]
            else:
                # one step of the machine left out
                cut = [x for x in v if x["inputs"] != v[0]["inputs"]]
            self.assertNotEqual(R.coverage(lw, cut), [], name)
            self.assertNotEqual(R.coverage(lw, []), [], name)
            json.dumps(v)  # the corpus is JSON

    def test_every_require_fails_alone_somewhere(self):
        lw = law("session_cookie_valid")
        failed = {x["expected"]["failed"] for x in R.vectors(lw, "s") if x["kind"] == "exact"}
        self.assertEqual(failed, {None, "shape", "tag", "fresh", "current"})


if __name__ == "__main__":
    unittest.main()
