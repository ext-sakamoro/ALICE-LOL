# alice-lol-layout

Byte layout laws for [ALICE-LOL](../README.md): a binary format declared once as a law file
(`kind layout`), and bytes read and checked against that declaration.

A format is often written down several times: a table in the documentation, the constants
and parser of the implementation, a reference writer and an independent reader in the tests.
A layout law is the one source. Each implementation is compared with it field by field on
the same inputs, and a refusal carries the label of the declared check, so an
implementation can map it to its own error and compare refusals too.

```text
law tiny_record
kind layout
endian be
const VERSION integer = 1
block head
field version u8 = VERSION refuse unknown_version
field len u16
end block
segment head block head
segment body bytes head.len refuse length
```

- Segments: `block`, `table` (a row count read earlier), `data` (one run per row, with an
  optional offset that must match), `bytes` (fixed, from a field, `rest`, or `at end`),
  `stream` (fixed-size chunks with a shorter last one).
- Checks: `=`, `bits`, `in` on fields; SHA-256 digests over the preceding bytes or each row's
  run; `refuse` / `status` rules over the decoded values.
- `derive`: SHA-256 over constants and segments (with optional length prefixes), computable
  from the declaration alone when it names only constants.
- Untrusted input: every count and length is checked against the remaining bytes with
  checked arithmetic before use, and allocation stays within a `Budget`.

`no_std` + `alloc`; the only dependency is `sha2`. Licensed under `LicenseRef-Proprietary` (see the
repository's [LICENSE-Proprietary](LICENSE-Proprietary)).
