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


# (battery_pct, thermal, frame_drops, value, row), from the rule of power_mode in words
POWER = [
    (50, "nominal", 0, "full", None),
    (50, "serious", 0, "minimal", "hot"),
    (50, "critical", 0, "minimal", "hot"),
    # hot and empty both hold: the row listed first decides
    (10, "critical", 20, "minimal", "hot"),
    (14, "nominal", 0, "minimal", "empty"),
    (15, "nominal", 0, "standard", "low"),
    # low and warm both hold
    (29, "fair", 0, "standard", "low"),
    (30, "fair", 0, "standard", "warm"),
    (30, "nominal", 10, "standard", "dropping"),
    (30, "nominal", 9, "full", None),
    (100, "Fair", 0, "full", None),
    (-5, "", 0, "minimal", "empty"),
]


class Table(unittest.TestCase):
    def test_the_power_rows_in_order(self):
        lw = law("power_mode")
        for b, t, d, value, row in POWER:
            self.assertEqual(R.respond(lw, {"battery_pct": b, "thermal": t, "frame_drops": d}),
                             ("outputs", {"value": value, "row": row}), (b, t, d))

    def test_rejections_and_errors(self):
        lw = law("power_mode")
        ok = {"battery_pct": 50, "thermal": "nominal", "frame_drops": 0}
        self.assertEqual(R.respond(lw, {**ok, "battery_pct": 1.5})[0], "rejected")
        self.assertEqual(R.respond(lw, {**ok, "thermal": 3})[0], "rejected")
        self.assertEqual(R.respond(lw, {**ok, "frame_drops": None})[0], "rejected")
        # a missing input is decided before a value of another type
        self.assertEqual(R.respond(lw, {"battery_pct": "x", "thermal": "nominal"})[0], "request_error")
        # 3.0 is the integer 3
        self.assertEqual(R.respond(lw, {**ok, "battery_pct": 3.0}), ("outputs", {"value": "minimal", "row": "empty"}))


M = "kind machine\nstate a initial\nevent go\non go from a to a\n"
P = 'kind predicate\nx-input n integer\nrequire pos: n > 0\nexample {"n": 1}\n'
TB = "kind table\nx-input n integer\nvalue a b\nrow pos: a when n > 0\notherwise b\n"

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
    # table laws
    TB.replace("value a b\n", ""), TB + "value c d\n", TB.replace("value a b", "value a"),
    TB.replace("otherwise b\n", ""), TB + "otherwise a\n",
    "kind table\nx-input n integer\nvalue a b\notherwise b\nrow pos: a when n > 0\n",
    "kind table\nx-input n integer\nvalue a b\notherwise b\n",
    TB.replace("row pos: a", "row pos: c"), TB.replace("otherwise b", "otherwise c"),
    TB.replace("value a b", "value a b c"), TB.replace("otherwise b", "row pos: b when n < 0\notherwise b"),
    TB.replace("n > 0", "n"), TB.replace("n > 0", "m > 0"), TB + "use alice-sign hmac_sha256\n",
    "kind table\nx-input k bytes\nvalue a b\nrow mac: a when hmac_sha256(k, k) == k\notherwise b\n",
    TB.replace("x-input n integer", "x-input n integer\nx-input s text\nx-split s \".\" p q"),
    TB.replace("row pos:", "row:"), TB.replace("row pos:", "row true:"), TB.replace("value a b", "value a n"),
    TB.replace("row pos: a when", "row pos: a if"), TB + 'example {"n": 1}\n',
    TB.replace("x-input n", "x-input  n"), TB.replace("x-input n integer", "x-input n integer\nconst n integer = 1"),
]


class Reading(unittest.TestCase):
    def test_files_that_do_not_read(self):
        for i, text in enumerate(BAD):
            with self.assertRaises(R.RuleError, msg=f"case {i}:\n{text}"):
                R.parse(text)
        R.parse(M)
        R.parse(P)
        R.parse(TB)

    def test_kind(self):
        self.assertEqual(R.rule_kind("kind audit\nkind machine\n"), None)
        self.assertEqual(R.rule_kind((SPIKE / "gate_compares_nonzero.law").read_text()), None)
        self.assertEqual(R.rule_kind((SPIKE / "consent_choice.law").read_text()), "machine")
        self.assertEqual(R.rule_kind((SPIKE / "power_mode.law").read_text()), "table")
        self.assertEqual(R.rule_kind("kind audit\nkind table\n"), None)


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

    def test_table_vectors_reach_every_row_and_coverage_has_teeth(self):
        lw = law("power_mode")
        v = R.vectors(lw, "power_mode")
        self.assertEqual(R.coverage(lw, v), [])
        rows = {x["expected"]["row"] for x in v if x["kind"] == "exact"}
        self.assertEqual(rows, {"hot", "empty", "low", "warm", "dropping", None})
        # every answer is the hand-written rule: the first holding row
        def first(i):
            b, t, d = i["battery_pct"], i["thermal"], i["frame_drops"]
            for label, val, holds in (("hot", "minimal", t in ("serious", "critical")),
                                      ("empty", "minimal", b < 15), ("low", "standard", b < 30),
                                      ("warm", "standard", t == "fair"), ("dropping", "standard", d >= 10)):
                if holds:
                    return {"value": val, "row": label}
            return {"value": "full", "row": None}
        exact = [x for x in v if x["kind"] == "exact"]
        self.assertGreater(len(exact), 20)
        for x in exact:
            self.assertEqual(x["expected"], first(x["inputs"]), x["inputs"])
        # the corpus has a vector where two rows hold and the order decides
        self.assertTrue(any(x["inputs"]["battery_pct"] < 15 and x["inputs"]["thermal"] == "serious" for x in exact))
        # the boundaries of every comparison are in it
        for b in (14, 15, 16, 29, 30, 31):
            self.assertTrue(any(x["inputs"]["battery_pct"] == b for x in exact), b)
        for d in (9, 10, 11):
            self.assertTrue(any(x["inputs"]["frame_drops"] == d for x in exact), d)
        # coverage catches a corpus without a row, without a rejection or without a request error
        self.assertNotEqual(R.coverage(lw, [x for x in v if x["kind"] != "exact" or x["expected"]["row"] != "warm"]), [])
        self.assertNotEqual(R.coverage(lw, [x for x in v if x["kind"] != "exact" or x["expected"]["row"] is not None]), [])
        self.assertNotEqual(R.coverage(lw, [x for x in v if x["kind"] != "reject"]), [])
        self.assertNotEqual(R.coverage(lw, [x for x in v if x["kind"] != "request_error"]), [])
        json.dumps(v)

    def test_a_row_that_no_vector_reaches_does_not_make_a_corpus(self):
        # `late` never holds before `early` decides every n > 5, so no input reaches it
        lw = R.parse("kind table\nx-input n integer\nvalue a b\nrow early: a when n > 5\n"
                     "row late: b when n > 7\notherwise b\n")
        with self.assertRaises(R.RuleError):
            R.vectors(lw, "x")

    def test_the_last_match_bug_is_seen(self):
        lw = law("power_mode")
        v = [x for x in R.vectors(lw, "p") if x["kind"] == "exact"]
        self.assertTrue(any(R.respond(lw, x["inputs"], bug="table_last_match") != ("outputs", x["expected"]) for x in v))
        self.assertTrue(any(R.respond(lw, x["inputs"], bug="predicate_lt_inclusive") != ("outputs", x["expected"]) for x in v))

    def test_every_require_fails_alone_somewhere(self):
        lw = law("session_cookie_valid")
        failed = {x["expected"]["failed"] for x in R.vectors(lw, "s") if x["kind"] == "exact"}
        self.assertEqual(failed, {None, "shape", "tag", "fresh", "current"})


SEM = {"rule": bytes(32), "table": bytes(32)}
RATE = ("kind table\nx-input n integer\nvalue low high\nrow big: high when n > 10\notherwise low\n")
HEADER = ("domain demo-tax\nedition y2025\nsupersedes first\neffective 1735689600 until 1767225600\n"
          "semantics " + "00" * 32 + "\n")


class Editions(unittest.TestCase):
    """conformance/TASK.md "Editions of a rule set": the five header lines"""

    def test_an_edition_reads_and_answers_as_the_file_without_its_header(self):
        e = R.parse(HEADER + RATE)
        self.assertEqual(e["edition"], {"domain": "demo-tax", "name": "y2025", "supersedes": None,
                                        "from": 1735689600, "until": 1767225600, "semantics": bytes(32)})
        plain = R.parse(RATE)
        self.assertIsNone(plain["edition"])
        for n in (3, 11):
            self.assertEqual(R.respond(e, {"n": n}), R.respond(plain, {"n": n}))

    def test_the_header_is_part_of_the_identity(self):
        base = R.law_id(R.parse(HEADER + RATE, SEM), SEM)
        self.assertNotEqual(base, R.law_id(R.parse(RATE, SEM), SEM))
        for old, new in (("y2025", "y2026"), ("demo-tax", "demo-vat"), ("until 1767225600", "until 1767225601"),
                         ("until 1767225600", ""), ("supersedes first", "supersedes " + "ab" * 32)):
            self.assertNotEqual(base, R.law_id(R.parse((HEADER + RATE).replace(old, new), SEM), SEM), new)
        # layout and position of the header lines are not
        moved = RATE.replace("kind table\n", "kind table\n" + HEADER.replace("edition", "edition ", 1))
        self.assertEqual(base, R.law_id(R.parse(moved, SEM), SEM))

    def test_header_files_that_do_not_read(self):
        bad = [
            HEADER.replace("domain demo-tax\n", ""),                       # four of the five
            HEADER + "edition again\n",                                       # twice
            HEADER.replace("demo-tax", "Demo"),                               # not the alphabet
            HEADER.replace("supersedes first", "supersedes " + "AB" * 32),   # upper-case hex
            HEADER.replace("supersedes first", "supersedes 1234"),            # not 64 digits
            HEADER.replace("until 1767225600", "until 1735689600"),           # not after the start
            HEADER.replace("effective 1735689600", "effective 01735689600"),  # not an integer literal
            HEADER.replace("1767225600", str(2 ** 53)),                        # above 2^53 - 1
            HEADER.replace("semantics " + "00" * 32, "semantics " + "11" * 32),  # another generation
        ]
        for text in bad:
            with self.assertRaises(R.RuleError, msg=text):
                R.parse(text + RATE, SEM)


E1 = ("law the first edition, written in full\n"
      "domain demo-tax\nedition e1\nsupersedes first\neffective 100 until 200\nsemantics " + "00" * 32 + "\n"
      "kind table\nx-input amount integer\nvalue reduced standard\n"
      "row small: reduced when amount <= 1000   # the bound\notherwise standard\n")


def _delta(prev_text, edition, ops, effective="200"):
    prev_id = R.law_id(R.parse(prev_text, SEM), SEM).hex()
    return (f"domain demo-tax\nedition {edition}\nsupersedes {prev_id}\neffective {effective}\n"
            f"semantics {'00' * 32}\n" + ops)


class Deltas(unittest.TestCase):
    """conformance/TASK.md: A later edition written as a delta"""

    def test_the_resolved_text_of_a_full_edition(self):
        self.assertEqual(R.resolved_text(E1, SEM),
                         "domain demo-tax\nedition e1\nsupersedes first\neffective 100 until 200\n"
                         "semantics " + "00" * 32 + "\nkind table\n"
                         "x-input amount integer\nvalue reduced standard\nrow small: reduced when amount <= 1000\n"
                         "otherwise standard\n")

    def test_a_chain_of_deltas_equals_the_edition_written_in_full(self):
        r1 = R.resolved_text(E1, SEM)
        d2 = _delta(r1, "e2", "replace row small: row small: reduced when amount <= 1200\nchanges mid\n",
                    "200 until 300")
        r2, ch2 = R.resolve(r1, d2, SEM)
        self.assertEqual(ch2, ["mid"])
        d3 = _delta(r2, "e3", "add after row small: row tiny: reduced when amount <= 10\n"
                              "remove row small\n", "300")
        r3, _ = R.resolve(r2, d3, SEM)
        e2_id = R.law_id(R.parse(r2, SEM), SEM).hex()
        by_hand = ("domain demo-tax\nedition e3\nsupersedes " + e2_id + "\neffective 300\nsemantics " + "00" * 32 + "\n"
                   "kind table\nx-input amount integer\n"
                   "value reduced standard\nrow tiny: reduced when amount <= 10\notherwise standard\n")
        self.assertEqual(r3, R.resolved_text(by_hand, SEM))
        self.assertEqual(R.law_id(R.parse(r3, SEM), SEM), R.law_id(R.parse(by_hand, SEM), SEM))
        # the resolved edition answers as the text it resolves to
        self.assertEqual(R.respond(R.parse(r2, SEM), {"amount": 1100})[1]["value"], "reduced")
        self.assertEqual(R.respond(R.parse(r1, SEM), {"amount": 1100})[1]["value"], "standard")

    def test_add_after_and_before_place_the_line(self):
        r1 = R.resolved_text(E1, SEM)
        body = lambda r: [ln for ln in r.splitlines() if ln.startswith("row ")]
        after, _ = R.resolve(r1, _delta(r1, "e2", "add after row small: row tiny: reduced when amount <= 10\n"), SEM)
        before, _ = R.resolve(r1, _delta(r1, "e2", "add before row small: row tiny: reduced when amount <= 10\n"), SEM)
        self.assertEqual([ln.split()[1] for ln in body(after)], ["small:", "tiny:"])
        self.assertEqual([ln.split()[1] for ln in body(before)], ["tiny:", "small:"])
        # first match: the order is part of the rule and of its identifier
        self.assertNotEqual(R.law_id(R.parse(after, SEM), SEM), R.law_id(R.parse(before, SEM), SEM))

    def test_two_lines_with_one_key_do_not_resolve(self):
        m = ("domain demo-tax\nedition e1\nsupersedes first\neffective 100\nsemantics " + "00" * 32 + "\n"
             "kind machine\nstate a initial\nstate b\nevent go\non go from a to b\non go from a to a\n")
        R.parse(m, SEM)  # the reader takes two transitions from one state (the first matching one is taken)
        with self.assertRaises(R.RuleError):
            R.resolved_text(m, SEM)
        # lines for people are not in the body, so two `source` lines are fine
        R.resolved_text(E1.replace("kind table", "source a\nsource b\nkind table"), SEM)

    def test_independent_operations_commute(self):
        r1 = R.resolved_text(E1, SEM)
        a = "replace row small: row small: reduced when amount <= 999\n"
        b = "add row big: standard when amount > 5000\n"
        ra, _ = R.resolve(r1, _delta(r1, "e2", a + b), SEM)
        rb, _ = R.resolve(r1, _delta(r1, "e2", b + a), SEM)
        self.assertEqual(ra, rb)

    def test_deltas_that_do_not_resolve(self):
        r1 = R.resolved_text(E1, SEM)
        bad = [
            _delta(r1, "e2", "remove row nothing\n"),                                  # no such key
            _delta(r1, "e2", "add row small: reduced when amount <= 5\n"),             # key already there
            _delta(r1, "e2", "replace row small: row big: reduced when amount <= 5\n"),  # changes the key
            _delta(r1, "e2", "rename row small\n"),                                     # not an operation
            _delta(r1, "e2", "").replace(R.law_id(R.parse(r1, SEM), SEM).hex(), "ab" * 32),  # wrong predecessor
            _delta(r1, "e2", "").replace("domain demo-tax", "domain demo-vat"),         # another domain
            _delta(r1, "e2", "add row x: reduced when amount <=\n"),                   # does not read after
        ]
        for d in bad:
            with self.assertRaises(R.RuleError, msg=d):
                R.resolve(r1, d, SEM)
        with self.assertRaises(R.RuleError):
            R.resolved_text(E1.replace("otherwise standard", "otherwise standard\nrow small: x when amount > 1"), SEM)


if __name__ == "__main__":
    unittest.main()
