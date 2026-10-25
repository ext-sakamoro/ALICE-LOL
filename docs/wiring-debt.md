# Wiring debt

What is written but not yet wired, each named by a key. A test that pins it is ignored with
`#[ignore = "src gap: <key>: <reason>"]`; `scripts/src_gap_check.py` checks that every such ignore
has a row here, that every row is used, and (with `--run`, in CI) that each of those tests still
fails when run alone, so an ignore cannot outlive its gap. When a row closes, remove the ignore and
the row in the same commit.

| key | what is not wired | closes when |
|---|---|---|
| `legal-records` | the statute, contract and procedure laws are not compiled into ALICE-Legal's `StatuteTree`, `Contract` and `Procedure`, so the records ALICE-Legal holds for them are not built and re-hashed by its own constructors | alice-lol links alice-legal with a single alice-zip and a single alice-det-math in the build (alice-legal pins the alice-zip revision whose alice-det-math differs from the one alice-lol's other dependencies bring today) |
