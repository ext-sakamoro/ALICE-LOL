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


SEM = {"rule": bytes(32), "table": bytes(32), "money_rule": bytes([3]) * 32, "money_table": bytes([4]) * 32, "statute": bytes([5]) * 32, "procedure": bytes([6]) * 32, "contract": bytes([7]) * 32}
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


CURRENCIES = ("kind table\nx-input code text\nvalue e0 e2 unknown\n"
              "row jpy: e0 when code == \"JPY\"\nrow usd: e2 when code == \"USD\"\notherwise unknown\n")


def money_law(body, kind="predicate"):
    table = R.parse(CURRENCIES, SEM)
    cid = R.law_id(table, SEM).hex()
    return R.parse(f"kind {kind}\ncurrencies {cid}\n" + body, SEM, currencies=table), table


BIG = 2 ** 63 + 1
TOP = 2 ** 127 - 1


class Money(unittest.TestCase):
    """conformance/TASK.md: money (exact minor units, a currency from the currency table)"""

    def law(self):
        return money_law("x-input a money\nx-input b money\nconst cap money = 170141183460469231731687303715884105727 JPY\n"
                         "require sum_fits: a + b <= cap\nrequire ordered: a < b\n"
                         "example {\"a\": \"1 JPY\", \"b\": \"2 JPY\"}\n")[0]

    def test_amounts_beyond_2_63_compare_and_add_exactly(self):
        law = self.law()
        ok = R.respond(law, {"a": f"{BIG} JPY", "b": f"{BIG + 1} JPY"})
        self.assertEqual(ok, ("outputs", {"valid": True, "failed": None}))
        self.assertEqual(R.respond(law, {"a": f"{BIG + 1} JPY", "b": f"{BIG} JPY"}),
                         ("outputs", {"valid": False, "failed": "ordered"}))
        # 2^127 - 1 is the largest amount; a sum beyond it rejects, never wraps or clamps
        self.assertEqual(R.respond(law, {"a": f"{TOP - 1} JPY", "b": "1 JPY"})[0], "outputs")
        self.assertEqual(R.respond(law, {"a": f"{TOP} JPY", "b": "1 JPY"})[0], "rejected")
        self.assertEqual(R.respond(law, {"a": f"-{TOP + 1} JPY", "b": "-1 JPY"})[0], "rejected")

    def test_what_is_not_money(self):
        law = self.law()
        for v in (f"{TOP + 1} JPY", f"-{TOP + 2} JPY", "1 jpy", "1 JP", "1  JPY", "+1 JPY", "01 JPY",
                  "1 EUR", "1JPY", 5, 5.0, None, True):
            self.assertEqual(R.respond(law, {"a": v, "b": "1 JPY"})[0], "rejected", repr(v))
        self.assertEqual(R.respond(law, {"a": f"-{TOP + 1} JPY", "b": "1 JPY"})[0], "outputs")  # -2^127 is money

    def test_a_code_is_three_upper_case_letters_whatever_the_table_lists(self):
        # a table that lists "jpy" and "JP" does not make them money: the form is checked first
        odd = R.parse(CURRENCIES.replace("row usd", "row low: e0 when code == \"jpy\"\n"
                                         "row two: e0 when code == \"JP\"\nrow usd"), SEM)
        text = (f"kind predicate\ncurrencies {R.law_id(odd, SEM).hex()}\nx-input a money\n"
                "require t: a == a\nexample {\"a\": \"1 JPY\"}\n")
        law = R.parse(text, SEM, currencies=odd)
        self.assertEqual(R.respond(law, {"a": "1 JPY"})[0], "outputs")
        for v in ("1 jpy", "1 JP"):
            self.assertEqual(R.respond(law, {"a": v})[0], "rejected", v)
        for c in ("jpy", "JP"):
            with self.assertRaises(R.RuleError):
                R.parse(text.replace("require t: a == a", f"const k money = 1 {c}\nrequire t: a == a"),
                        SEM, currencies=odd)

    def test_a_law_file_is_read_with_the_table_of_its_directory_it_names(self):
        import tempfile
        table = R.parse(CURRENCIES, SEM)
        other = CURRENCIES.replace("row usd", "row eur: e2 when code == \"EUR\"\nrow usd")
        law = (f"kind predicate\ncurrencies {R.law_id(table, SEM).hex()}\nx-input a money\n"
               "require t: a == a\nexample {\"a\": \"1 JPY\"}\n")
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "a_other.law").write_text(other, encoding="utf-8")  # first by name, not the one named
            (d / "fee.law").write_text(law, encoding="utf-8")
            with self.assertRaises(R.RuleError):  # the named table is not in the directory
                R.parse_file(d / "fee.law", SEM)
            (d / "units.law").write_text(CURRENCIES, encoding="utf-8")
            got = R.parse_file(d / "fee.law", SEM)
            self.assertEqual(R.respond(got, {"a": "1 USD"})[0], "outputs")
            self.assertEqual(R.respond(got, {"a": "1 EUR"})[0], "rejected")
            with self.assertRaises(R.RuleError):  # a law with money needs the semantics identifiers
                R.parse_file(d / "fee.law")
            self.assertEqual(R.parse_file(d / "units.law")["kind"], "table")  # a law without money

    def test_an_edition_with_money_names_the_money_semantics(self):
        table = R.parse(CURRENCIES, SEM)
        body = (f"kind predicate\ncurrencies {R.law_id(table, SEM).hex()}\nx-input fee money\n"
                "const cap money = 1000 JPY\nrequire within: fee <= cap\nexample {\"fee\": \"1 JPY\"}\n")

        def head(sem):
            return ("domain demo-fee\nedition y2025\nsupersedes first\neffective 1735689600 until 1767225600\n"
                    f"semantics {sem.hex()}\n")
        first = head(SEM["money_rule"]) + body
        law = R.parse(first, SEM, currencies=table)
        with self.assertRaises(R.RuleError):  # a law with money is not evaluated under the rule semantics
            R.parse(head(SEM["rule"]) + body, SEM, currencies=table)
        delta = (f"domain demo-fee\nedition y2026\nsupersedes {R.law_id(law, SEM).hex()}\neffective 1767225600\n"
                 f"semantics {SEM['money_rule'].hex()}\nreplace const cap: const cap money = 2000 JPY\nchanges raised\n")
        resolved = R.resolved_text(first, SEM, table)
        with self.assertRaises(R.RuleError):  # read without its table
            R.resolve(resolved, delta, SEM)
        text, changes = R.resolve(resolved, delta, SEM, table)
        self.assertEqual(changes, ["raised"])
        later = R.parse(text, SEM, currencies=table)
        self.assertEqual(R.respond(later, {"fee": "1500 JPY"})[1]["valid"], True)
        self.assertEqual(R.respond(law, {"fee": "1500 JPY"})[1]["valid"], False)
        with self.assertRaises(R.RuleError):  # the delta names the rule semantics
            R.resolve(resolved, delta.replace(SEM["money_rule"].hex(), SEM["rule"].hex()), SEM, table)

    def test_currencies_do_not_mix(self):
        law = self.law()
        self.assertEqual(R.respond(law, {"a": "1 USD", "b": "2 JPY"})[0], "rejected")
        with self.assertRaises(R.RuleError):  # two constants of different currencies
            money_law("x-input a money\nconst x money = 1 JPY\nconst y money = 1 USD\nrequire r: x < y\n"
                      "example {\"a\": \"1 JPY\"}\n")
        # minor units only: 100 JPY and 100 USD are not compared, and nothing is converted
        t, _ = money_law("x-input a money\nconst limit money = 100 JPY\nvalue small big\n"
                         "row s: small when a < limit\notherwise big\n", kind="table")
        self.assertEqual(R.respond(t, {"a": "99 JPY"})[1]["value"], "small")
        self.assertEqual(R.respond(t, {"a": "99 USD"})[0], "rejected")
        # a law without examples: the reading itself refuses, not an example
        with self.assertRaises(R.RuleError):  # two constants of different currencies
            money_law("x-input a money\nconst x money = 1 JPY\nconst y money = 1 USD\nvalue small big\n"
                      "row s: small when a < x or x < y\notherwise big\n", kind="table")
        with self.assertRaises(R.RuleError):  # a constant in a currency the table does not know
            money_law("x-input a money\nconst limit money = 100 EUR\nvalue small big\n"
                      "row s: small when a < limit\notherwise big\n", kind="table")
        # an input in a currency the table does not know is not money, even when nothing mixes
        only, _ = money_law("x-input a money\nx-input b money\nrequire o: a < b\n"
                            "example {\"a\": \"1 JPY\", \"b\": \"2 JPY\"}\n")
        self.assertEqual(R.respond(only, {"a": "1 USD", "b": "2 USD"})[0], "outputs")
        self.assertEqual(R.respond(only, {"a": "1 EUR", "b": "2 EUR"})[0], "rejected")

    def test_the_currency_table_is_part_of_the_law(self):
        law, table = money_law("x-input a money\nconst zero money = 0 JPY\nrequire pos: a > zero\n"
                               "example {\"a\": \"1 JPY\"}\n")
        other = R.parse(CURRENCIES.replace("row usd", "row eur: e2 when code == \"EUR\"\nrow usd"), SEM)
        text = f"kind predicate\ncurrencies {R.law_id(table, SEM).hex()}\nx-input a money\nconst zero money = 0 JPY\n" \
               "require pos: a > zero\nexample {\"a\": \"1 JPY\"}\n"
        with self.assertRaises(R.RuleError):  # the table given is not the one the law names
            R.parse(text, SEM, currencies=other)
        with self.assertRaises(R.RuleError):  # a money law read without its table
            R.parse(text, SEM)
        with self.assertRaises(R.RuleError):  # money without a currencies line
            R.parse("kind predicate\nx-input a money\nrequire t: a == a\nexample {\"a\": \"1 JPY\"}\n", SEM)
        with self.assertRaises(R.RuleError):  # a currencies line without money
            R.parse(f"kind predicate\ncurrencies {R.law_id(table, SEM).hex()}\nx-input n integer\n"
                    "require t: n == n\nexample {\"n\": 1}\n", SEM, currencies=table)
        with self.assertRaises(R.RuleError):  # a constant in a currency the table does not know
            R.parse(text.replace("0 JPY", "0 EUR"), SEM, currencies=table)
        # the table's identity is in the law's identity; money values are hashed exactly
        base = R.law_id(R.parse(text, SEM, currencies=table), SEM)
        t2 = R.parse(CURRENCIES.replace("otherwise unknown", "otherwise none").replace("unknown", "none"), SEM)
        moved = text.replace(R.law_id(table, SEM).hex(), R.law_id(t2, SEM).hex())
        self.assertNotEqual(base, R.law_id(R.parse(moved, SEM, currencies=t2), SEM))
        self.assertNotEqual(base, R.law_id(R.parse(text.replace("0 JPY", "-1 JPY"), SEM, currencies=table), SEM))
        # + and - are in the identity, and are not each other
        add = money_law("x-input a money\nx-input b money\nrequire r: a + b > a\nexample {\"a\": \"1 JPY\", \"b\": \"1 JPY\"}\n")[0]
        sub = money_law("x-input a money\nx-input b money\nrequire r: a - b < a\nexample {\"a\": \"1 JPY\", \"b\": \"1 JPY\"}\n")[0]
        add2 = money_law("x-input a money\nx-input b money\nrequire r: a - b > a\nexample {\"a\": \"1 JPY\", \"b\": \"-1 JPY\"}\n")[0]
        self.assertEqual(len({R.law_id(x, SEM) for x in (add, sub, add2)}), 3)

    def test_an_operation_not_evaluated_does_not_reject(self):
        law, _ = money_law("x-input a money\nx-input b money\nx-input go bool\n"
                           "require r: not go or a + b > a\nexample {\"a\": \"1 JPY\", \"b\": \"1 JPY\", \"go\": true}\n")
        self.assertEqual(R.respond(law, {"a": "1 JPY", "b": "1 USD", "go": False})[0], "outputs")
        self.assertEqual(R.respond(law, {"a": "1 JPY", "b": "1 USD", "go": True})[0], "rejected")


ACT = ("kind statute\ntitle Demo Act\n"
       "clause pay obligation from 100 until 200: the buyer pays within 30 days\n"
       "clause waive exception of pay from 150: unless the seller waives payment\n"
       "clause late penalty of pay from 100: a late payment bears interest\n"
       "clause term definition: buyer means the person who orders\n")


class Statute(unittest.TestCase):
    """conformance/TASK.md "Statute laws": the clauses in force at a time, in file order"""

    def at(self, law, t):
        return R.respond(law, {"at": t})

    def test_the_clauses_in_force_at_a_time(self):
        law = R.parse(ACT)
        for t, want in ((0, ["term"]), (99, ["term"]), (100, ["pay", "late", "term"]),
                        (149, ["pay", "late", "term"]), (150, ["pay", "waive", "late", "term"]),
                        (199, ["pay", "waive", "late", "term"]), (200, ["waive", "late", "term"]),
                        (-150, []), (100.0, ["pay", "late", "term"])):
            self.assertEqual(self.at(law, t), ("outputs", {"in_force": want}), t)
        for v in (1.5, "100", None, True, 2 ** 53):
            self.assertEqual(self.at(law, v)[0], "rejected", v)
        self.assertEqual(R.respond(law, {})[0], "request_error")

    def test_what_does_not_read(self):
        R.parse("kind statute\ntitle T\nclause a obligation: x\n")
        R.parse("kind statute\ntitle T\nclause initial obligation: x\nclause and_2 exception of initial: y\n")
        R.parse("kind statute\ntitle T\nclause a obligation from 18446744073 : x\n")
        for bad in ("kind statute\nclause a obligation: x\n",
                    "kind statute\ntitle T\ntitle U\nclause a obligation: x\n",
                    "kind statute\ntitle\nclause a obligation: x\n",
                    "kind statute\ntitle T\n",
                    "kind statute\ntitle T\nclause a duty: x\n",
                    "kind statute\ntitle T\nclause a obligation:\n",
                    "kind statute\ntitle T\nclause a obligation x\n",
                    "kind statute\ntitle T\nclause A obligation: x\n",
                    "kind statute\ntitle T\nclause a obligation: x\nclause a permission: y\n",
                    "kind statute\ntitle T\nclause e exception: x\n",
                    "kind statute\ntitle T\nclause e exception of a: x\nclause a obligation: y\n",
                    "kind statute\ntitle T\nclause a obligation from 5 until 5: x\n",
                    "kind statute\ntitle T\nclause a obligation until 0: x\n",
                    "kind statute\ntitle T\nclause a obligation from 18446744074: x\n",
                    "kind statute\ntitle T\nclause a obligation until 18446744074: x\n",
                    "kind statute\ntitle T\nclause a obligation from 01: x\n",
                    "kind statute\ntitle T\nclause a obligation until 9 from 1: x\n",
                    "kind statute\ntitle T\nclause a obligation from 1 extra: x\n",
                    "kind statute\ntitle T\nclause a obligation: x\nrequire r: true\n",
                    f"kind statute\ncurrencies {'0' * 64}\ntitle T\nclause a obligation: x\n"):
            with self.assertRaises(R.RuleError, msg=bad):
                R.parse(bad)

    def test_the_identifier_names_every_clause_field(self):
        base = R.law_id(R.parse(ACT), SEM)
        for a, b in (("title Demo Act", "title Demo Act 2"), ("clause term definition", "clause terms definition"),
                     ("clause term definition", "clause term permission"), ("from 100 until 200", "from 100 until 201"),
                     ("from 100 until 200", "from 101 until 200"), ("from 100 until 200", "from 100"),
                     ("within 30 days", "within 31 days"), ("clause late penalty of pay", "clause late penalty")):
            self.assertNotEqual(R.law_id(R.parse(ACT.replace(a, b)), SEM), base, f"{a} -> {b}")
        moved = ACT.replace("clause waive exception of pay from 150: unless the seller waives payment\n"
                            "clause late penalty of pay from 100: a late payment bears interest\n",
                            "clause late penalty of pay from 100: a late payment bears interest\n"
                            "clause waive exception of late from 150: unless the seller waives payment\n")
        self.assertNotEqual(R.law_id(R.parse(moved), SEM), base)
        spaced = ACT.replace("clause term definition:", "clause\tterm  definition :") + "# a comment\n"
        self.assertEqual(R.law_id(R.parse(spaced), SEM), base)
        self.assertEqual(R.law_id(R.parse("kind statute\ntitle T\nclause a obligation: x\n"), SEM),
                         R.law_id(R.parse("kind statute\ntitle T\nclause a obligation from 0: x\n"), SEM))

    def test_an_edition_and_a_delta(self):
        head = lambda ed, sup, eff: (f"domain demo-act\nedition {ed}\nsupersedes {sup}\neffective {eff}\n"
                                     f"semantics {SEM['statute'].hex()}\n")
        first = head("y2025", "first", "100 until 300") + ACT
        law = R.parse(first, SEM)
        self.assertEqual(law["clauses"][3]["from"], 100)  # a clause without `from` takes the edition's start
        with self.assertRaises(R.RuleError):
            R.parse(first.replace(SEM["statute"].hex(), SEM["rule"].hex()), SEM)
        resolved = R.resolved_text(first, SEM)
        delta = (head("y2026", R.law_id(law, SEM).hex(), "300")
                 + "replace clause pay: clause pay obligation from 300: the buyer pays within 14 days\n"
                   "remove clause late\nchanges shorter\n")
        text, changes = R.resolve(resolved, delta, SEM)
        self.assertEqual(changes, ["shorter"])
        self.assertEqual(R.respond(R.parse(text, SEM), {"at": 300}), ("outputs", {"in_force": ["pay", "waive", "term"]}))
        with self.assertRaises(R.RuleError):  # the exception's parent removed
            R.resolve(resolved, head("y2026", R.law_id(law, SEM).hex(), "300") + "remove clause pay\nchanges gone\n", SEM)

    def test_the_corpus_covers_every_clause_in_and_out_of_force(self):
        law = R.parse(ACT)
        vecs = R.vectors(law, "act")
        self.assertEqual(R.coverage(law, vecs), [])
        # without the times at which `waive` is in force, the corpus misses it
        early = [v for v in vecs if not (v["kind"] == "exact" and "waive" in v["expected"]["in_force"])]
        self.assertIn("a vector with waive in force", R.coverage(law, early))
        # without the times before 1970, `term` (in force from 0, no end) is never out of force
        later = [v for v in vecs if not (v["kind"] == "exact" and isinstance(v["inputs"].get("at"), int)
                                         and v["inputs"]["at"] < 0)]
        self.assertIn("a vector with term not in force", R.coverage(law, later))
        # each bound and the time before it
        no99 = [v for v in vecs if v["inputs"].get("at") != 99]
        self.assertIn("a vector at 99 (a bound of pay)", R.coverage(law, no99))


PERMIT = ("kind procedure\nstate pending initial\nstate in_progress\nstate approved\nstate rejected\nstate completed\n"
          "event application\nevent review guard complete\nevent approval\nevent rejection\nevent notification\n"
          "event completion\non application from pending to in_progress\n"
          "on review from in_progress to in_progress when complete\non approval from in_progress to approved\n"
          "on rejection from in_progress to rejected\non notification from approved to approved\n"
          "on notification from rejected to rejected\non completion from approved rejected to completed\n")


class Procedure(unittest.TestCase):
    """conformance/TASK.md "Procedure": a machine over a procedure's statuses and step kinds"""

    def test_the_status_table_is_alice_legal_s(self):
        # ALICE-Legal 9490cf2 src/procedure.rs add_step, written out
        for st in R.PROCEDURE_STATUSES:
            for k in R.STEP_KINDS:
                want = {"approval": "approved", "rejection": "rejected", "completion": "completed"}.get(
                    k, "in_progress" if st == "pending" else st)
                self.assertEqual(R.next_status(st, k), want, (st, k))

    def test_a_procedure_answers_as_a_machine(self):
        law = R.parse(PERMIT)
        self.assertEqual(R.respond(law, {"state": "pending", "event": "application"})[1]["state"], "in_progress")
        refused = R.respond(law, {"state": "pending", "event": "approval"})
        self.assertEqual((refused[1]["state"], refused[1]["transition"]), ("pending", "refused"))
        same = R.parse(PERMIT.replace("kind procedure", "kind machine"))
        for inp in ({"state": "in_progress", "event": "rejection"}, {"state": "approved", "event": "notification"}):
            self.assertEqual(R.respond(law, inp), R.respond(same, inp))
        self.assertNotEqual(R.law_id(law, SEM), R.law_id(same, SEM))

    def test_what_does_not_read(self):
        for a, b in (("on approval from in_progress to approved\n", "on approval from in_progress to completed\n"),
                     ("on application from pending to in_progress\n", "on application from pending to pending\n"),
                     ("on notification from approved to approved\n", "on notification from approved to in_progress\n"),
                     ("on completion from approved rejected to completed\n", "on completion from approved rejected to rejected\n"),
                     ("state pending initial\nstate in_progress\n", "state pending\nstate in_progress initial\n"),
                     ("state completed\n", "state completed\nstate archived\n"),
                     ("event completion\n", "event completion\nevent audit\n"),
                     ("on notification from rejected to rejected\n", "on notification from rejected approved to rejected\n")):
            with self.assertRaises(R.RuleError, msg=b):
                R.parse(PERMIT.replace(a, b))
        R.parse(PERMIT.replace("kind procedure\n", "kind procedure\nconst k integer = 1\n"))

    def test_an_edition_names_the_procedure_semantics(self):
        head = lambda sem: ("domain demo-permit\nedition y2025\nsupersedes first\neffective 0\n"
                            f"semantics {sem.hex()}\n")
        R.parse(head(SEM["procedure"]) + PERMIT, SEM)
        with self.assertRaises(R.RuleError):
            R.parse(head(SEM["rule"]) + PERMIT, SEM)



def sale_law():
    table = R.parse(CURRENCIES, SEM)
    text = (f"kind contract\ncurrencies {R.law_id(table, SEM).hex()}\nparty buyer\nparty seller\ncreated 100\n"
            f"condition inspected decided by {'cd' * 32}: the goods were inspected\n"
            "obligation deposit buyer to seller 1000 JPY due 200\n"
            "obligation price buyer to seller 9223372036854775900 JPY due 300\n"
            "obligation delivery_fee seller to buyer 50 JPY due 300\n")
    return text, table


class Contract(unittest.TestCase):
    """conformance/TASK.md "Contract": the status ALICE-Legal's check_status gives"""

    def ask(self, inp):
        text, table = sale_law()
        return R.respond(R.parse(text, SEM, currencies=table), inp)

    def test_the_status(self):
        allo = ["deposit", "price", "delivery_fee"]
        self.assertEqual(self.ask({"now": 150}), ("outputs", {"status": "active", "unfulfilled": allo,
                                                              "owed": "9223372036854776950 JPY", "conditions_met": False}))
        self.assertEqual(self.ask({"now": 200})[1]["status"], "active")
        self.assertEqual(self.ask({"now": 201})[1]["status"], "breached")
        self.assertEqual(self.ask({"now": 201, "fulfilled": ["deposit"]})[1]["owed"], "9223372036854775950 JPY")
        self.assertEqual(self.ask({"now": 301, "fulfilled": ["deposit", "price"]})[1]["status"], "breached")
        self.assertEqual(self.ask({"now": 999, "fulfilled": ["delivery_fee", "deposit", "price"], "met": ["inspected"]}),
                         ("outputs", {"status": "fulfilled", "unfulfilled": [], "owed": "0 JPY", "conditions_met": True}))
        self.assertEqual(self.ask({"now": -5})[1]["status"], "active")
        text, table = sale_law()
        two = R.parse(text.replace("obligation deposit", f"condition paid decided by {'ef' * 32}: the bank confirmed\n"
                                   "obligation deposit"), SEM, currencies=table)
        self.assertFalse(R.respond(two, {"now": 150, "met": ["inspected"]})[1]["conditions_met"])
        self.assertTrue(R.respond(two, {"now": 150, "met": ["paid", "inspected"]})[1]["conditions_met"])
        big = R.parse(text.replace("1000 JPY due 200", f"{2 ** 126} JPY due 200").replace("9223372036854775900 JPY", f"{2 ** 126} JPY"),
                      SEM, currencies=table)
        self.assertEqual(R.respond(big, {"now": 150})[0], "rejected")
        self.assertEqual(R.respond(big, {"now": 150, "fulfilled": ["deposit"]})[0], "outputs")
        self.assertEqual(self.ask({"fulfilled": []})[0], "request_error")
        for bad in ({"now": "150"}, {"now": 1.5}, {"now": 150, "fulfilled": "deposit"},
                    {"now": 150, "fulfilled": ["deposit", "deposit"]}, {"now": 150, "fulfilled": ["buyer"]},
                    {"now": 150, "fulfilled": ["inspected"]}, {"now": 150, "fulfilled": [1]},
                    {"now": 150, "met": ["deposit"]}, {"now": 150, "met": None}, {"now": 150, "fulfilled": None}):
            self.assertEqual(self.ask(bad)[0], "rejected", bad)

    def test_what_does_not_read(self):
        base, table = sale_law()
        R.parse(base, SEM, currencies=table)
        for a, b in (("party seller\n", ""), ("party seller\n", "party seller\nparty seller\n"), ("created 100\n", ""),
                     ("created 100\n", "created 100\ncreated 101\n"), ("created 100\n", "created 301\n"),
                     ("created 100\n", "created 18446744074\n"), ("buyer to seller 1000 JPY", "buyer to buyer 1000 JPY"),
                     ("buyer to seller 1000 JPY", "buyer to carrier 1000 JPY"), ("1000 JPY due 200", "1000 USD due 200"),
                     ("1000 JPY due 200", "1000 EUR due 200"), ("1000 JPY due 200", "0 JPY due 200"),
                     ("1000 JPY due 200", "-1 JPY due 200"), ("1000 JPY due 200", "1000 jpy due 200"),
                     ("1000 JPY due 200", "1000 JPY by 200"), ("due 200\n", "due 18446744074\n"),
                     ("decided by cdcd", "decided bycdcd"), (": the goods were inspected", ":"),
                     ("condition inspected", "condition deposit"), ("obligation price", "obligation buyer"),
                     ("created 100\n", "created 100\nrequire r: true\n")):
            with self.assertRaises(R.RuleError, msg=b):
                R.parse(base.replace(a, b), SEM, currencies=table)
        with self.assertRaises(R.RuleError):
            R.parse(base, SEM)
        with self.assertRaises(R.RuleError):
            R.parse("".join(l + "\n" for l in base.splitlines() if not l.startswith("currencies")), SEM, currencies=table)

    def test_the_identifier_names_every_field(self):
        base, table = sale_law()
        idf = lambda t: R.law_id(R.parse(t, SEM, currencies=table), SEM)
        b = idf(base)
        for x, y in (("party buyer\nparty seller\n", "party seller\nparty buyer\n"), ("created 100", "created 101"),
                     ("condition inspected", "condition checked"), ("decided by cdcd", "decided by cdcc"),
                     ("the goods were inspected", "the goods were seen"), ("obligation deposit", "obligation down_payment"),
                     ("deposit buyer to seller", "deposit seller to buyer"), ("1000 JPY due 200", "1001 JPY due 200"),
                     ("1000 JPY due 200", "1000 JPY due 201")):
            self.assertNotEqual(idf(base.replace(x, y)), b, f"{x} -> {y}")

    def test_the_corpus_covers_every_status_and_obligation(self):
        base, table = sale_law()
        law = R.parse(base, SEM, currencies=table)
        vecs = R.vectors(law, "sale")
        self.assertEqual(R.coverage(law, vecs), [])
        no_breach = [v for v in vecs if not (v["kind"] == "exact" and v["expected"]["status"] == "breached")]
        self.assertIn("a vector with status breached", R.coverage(law, no_breach))
        no_201 = [v for v in vecs if v["inputs"].get("now") != 201]
        self.assertIn("a vector at 201 with nothing fulfilled (the due time of deposit)", R.coverage(law, no_201))


if __name__ == "__main__":
    unittest.main()
