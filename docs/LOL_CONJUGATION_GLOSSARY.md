# Conjugating `lol` as a verb

This crate's name reads as a word, and `lol!` is the macro you actually write.
Japanese grammar already has six conjugation forms for verbs of this shape
(a stem plus a godan "-ru" ending, the same class as real verbs like "noru" —
to ride), and reusing them gives this project a short, memorable vocabulary
for the lifecycle a conformance claim about this crate goes through: from
"not yet checked" through "being checked" to "checked, with evidence", and
separately, two voice forms for who is doing the checking.

This is a naming convention, not new tooling: every state named below is one
this crate's own `alice-law-conformance.toml` already records (see
[CONFORMANCE.md](https://github.com/ext-sakamoro/ALICE-Law/blob/main/docs/CONFORMANCE.md)
in ALICE-Law for what each gate G1–G14 actually requires). The glossary gives
a name to a stage of that one pipeline; it does not add a new one.

Contents: [The six core forms](#the-six-core-forms) ·
[Worked examples against this crate's own gates](#worked-examples-against-this-crates-own-gates) ·
[Voice: passive and potential](#voice-passive-and-potential) ·
[Derived verbs](#derived-verbs) ·
[English forms](#english-forms) ·
[Future work](#future-work)

## The six core forms

`lol` conjugates as a godan verb ending in "-ru" (stem `lol`, final kana in
the ra-row: ら/り/る/れ/ろ — the same pattern as "noru", not the "-ro"
imperative of an ichidan verb like "taberu"; see the warning after the table).

| Form (Japanese) | Written as | Names this state | Reading as a sentence |
|---|---|---|---|
| Irrealis (未然形) | `lolらない` (lolra-nai), or volitional `lolろう` (lolro-u) | Not yet verified | "is not [verified]" / "[I] will verify [it]" — said before any check has run |
| Continuative (連用形) | `lolります` (lolri-masu), or past `lolった` (lol-tta) | Being verified (CI running), or a single verification event | "[it] is being verified" / "[it] got verified [once]" |
| Conclusive (終止形) | `lolる` (lol-ru) — the literal `lol!` macro invocation | Verified, with evidence | the dictionary form: "[it] verifies" / "[it] is verified" |
| Attributive (連体形) | `lolる` + noun, e.g. `lolる crate` | Verified, naming what was verified | "the crate that verifies" / "the Law-verified crate" |
| Hypothetical (仮定形) | `lolれば` (lol-reba) | Determinism: the same input gives the same result | "if [it] verifies, then..." |
| Imperative (命令形) | `lolれ` (lol-re) | A command: run the check | "verify [it]!" |

The hypothetical and imperative forms share the same stem, `lolre` — this is
not a coincidence specific to this verb: a godan verb's e-row stem is both
"if X" (followed by ば) and a bare command (standing alone), the same way it
would be for "noru" (`noreba` / `nore`).

**On the imperative specifically**: it is `lolれ` (lolre), not `lolろ`
(lolro). `-ro` is the ichidan imperative ending (e.g. `tabero`, "eat!", from
`taberu`) and does not apply here, since `lol` conjugates as godan. `lolろ`
only appears as part of the volitional `lolろう` (lolrou, "let's verify" /
"I will verify") above, in the irrealis row — never alone.

## Worked examples against this crate's own gates

The table above names a stage; it does not pick which of the 14 gates is
being talked about; any of them can be in any of these stages for this
crate at a given time. Reading this crate's own
[`alice-law-conformance.toml`](../alice-law-conformance.toml) as of this
chapter's writing:

- **G1 (no-libm-direct) is in the `lolる` state for this crate** (conclusive
  form): the manifest's `[G1]` entry records `status = "pass"`.
- **G2 (ieee-max-min) has not reached `lolる` for this crate** (irrealis
  form): the manifest's `[G2]` entry records `status = "missing"`.
- **G13 (no-unspecified-order) has not reached `lolる` for this crate**:
  the manifest's `[G13]` entry records `status = "missing"`.

Each entry's own `evidence` / `note` fields carry the specifics (what was
checked, what is still outstanding) — read
[`alice-law-conformance.toml`](../alice-law-conformance.toml) itself for
those rather than this chapter's summary of them, since a gate's status can
change release to release and a copy here would drift out of sync with the
manifest it is quoting.

A gate recorded as `n/a` in the manifest is a separate case this vocabulary
does not name: it is not "not yet verified" (irrealis) but "does not apply
here", with its own stated reason — closer to asking whether a ra-row verb
even has an object to act on, than to any conjugated form of it.

## Voice: passive and potential

Two further forms describe the relationship between the thing being checked
and the check itself, rather than a stage of the pipeline:

- **Passive (受身)**: `lolられる` (lolra-reru) — the crate (or value) is the
  one being verified, not the one doing the verifying: "this crate gets
  verified by G1", with the gate as the implicit actor.
- **Potential (可能)**: `lolれる` (lol-reru) — the mechanism to verify exists
  and could be run, independent of whether it has actually been run yet.
  G10's own manifest entry is a concrete example of this distinction: a
  manual search found no unseeded randomness or wall-clock reads in this
  crate's `src/`, so the property Potential names (checkable) already holds
  in the sense that nothing was found — but there is no durable CI job
  making that search repeatable, so the gate itself still records `missing`,
  not `pass`.

## Derived verbs

`lol` also takes the derivational patterns regular Japanese applies to a
borrowed stem, each naming something slightly different from the core six
forms above:

| Derived form | Written as | Names |
|---|---|---|
| Suru-verb (サ変) | `lolする` / `lol化する` (lol-suru / lol-ka-suru) | The act of constructing something *as* LOL — compiling source text through the `lol!` macro or the runtime parser into an `SdfNode` tree — prior to any gate check running against the result |
| Causative (使役) | `lolさせる` (lol-saseru, built on the suru-verb's own irregular causative, not the godan verb's `lolraseru`) | Causing another component to go through verification — a producer requiring a consumer to check a semantics identifier (the relationship G7 names) |
| Continuative aspect (継続) | `lolってる` (lol-tteru, the casual contraction of `lolっている`, sokuon as in noru's own `notteru`) | An ongoing *state* of having passed, as opposed to the single past *event* named by the continuative form's `lolった` above — "is currently in the verified state", matching a manifest's `since` field: not just that a gate passed once, but that it has stayed passed |
| Emphatic completion (完了強調) | `lolっちゃう` (lol-cchau, sokuon as in noru's own `nocchau`) | A completion treated as final and not casually reversible — the closest match in this crate's own gates is G9: an identifier move is reserved for an actual change in meaning, and once it happens it is recorded as a semver-major change, not walked back |

## English forms

For prose and code comments written in English: `lol`, `lols`, `lolled`,
`lolling` are the canonical inflections (a regular verb, same pattern as
"call" / "calls" / "called" / "calling"). `lol'd` is not used — the
apostrophe form reads as a contraction and is ambiguous on the page in a way
the regular `-ed` form is not.

The language's own name is capitalized, `LOL` (as in "the LOL language", or
this repository's name, `ALICE-LOL`), distinct from the lowercase verb form
used in running prose.

English already has an unrelated, existing verb spelled almost the same way:
"to loll" (to lounge or recline lazily) is a different word with a different
meaning; the two are not related, and a reader encountering "lolling" in
this crate's documentation should read it as the inflected form of this
chapter's verb, not as that unrelated English word, unless context makes
clear otherwise.

## Future work

This chapter names the vocabulary; it does not yet enforce that the
vocabulary and this crate's actual implementation stay in agreement. A
follow-on check would scan the word forms named above against this crate's
source and test names, and fail if it compares zero instances of either side
(an empty comparison must not read as a pass) — and would tie each
conjugated form used in a comment or test name to the specific gate (G1–G14)
it is being said about, so that a claim like "G1 `lolる`" is itself checked
against that gate's manifest entry, rather than being prose a reader has to
trust unchecked. This is not built yet; the manifest cross-reference in
[Worked examples](#worked-examples-against-this-crates-own-gates) above is
done by hand as of this writing.
