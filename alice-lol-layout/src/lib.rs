//! Byte layout laws: a binary format declared once as a law file, and bytes read or checked
//! against that declaration.
//!
//! A format is usually written down several times: a table in the documentation, the
//! constants and parser of the implementation, a reference writer, an independent reader in
//! the tests. A layout law is the one source: the documentation can be generated from it and
//! every implementation is compared with it field by field on the same inputs, so the
//! declaration and the code cannot drift apart unnoticed.
//!
//! # A law file
//!
//! ```
//! let law = alice_lol_layout::parse(
//!     "law tiny_record\n\
//!      kind layout\n\
//!      claim a version byte, a big-endian length and that many bytes\n\
//!      endian be\n\
//!      const VERSION integer = 1\n\
//!      block head\n\
//!      field version u8 = VERSION refuse unknown_version\n\
//!      field len u16\n\
//!      end block\n\
//!      segment head block head\n\
//!      segment body bytes head.len refuse length\n",
//! )?;
//! let d = alice_lol_layout::decode(&law, &[1, 0, 2, 0xAA, 0xBB], alice_lol_layout::Budget::for_input(5))
//!     .expect("reads");
//! assert_eq!(d.segment_bytes("body"), Some(&[0xAA, 0xBB][..]));
//! let e = alice_lol_layout::decode(&law, &[2, 0, 0], alice_lol_layout::Budget::for_input(3))
//!     .expect_err("version 2 is refused");
//! assert_eq!(e.label(), Some("unknown_version"));
//! # Ok::<(), alice_lol_layout::LayoutError>(())
//! ```
//!
//! # Lines
//!
//! One item per line; `#` starts a comment. After `law <name>` and `kind layout`:
//!
//! | line | meaning |
//! |---|---|
//! | `claim` / `source` / `verdict` … | for people; not part of the identifier |
//! | `endian le` / `endian be` | the byte order of integers without their own |
//! | `const <name> integer\|bytes\|text\|tags = <value>` | a named constant (`0x…` bytes, `"…"` text, 4-byte tags separated by spaces) |
//! | `minimum <n> [refuse <label>]` | the shortest input, checked first |
//! | `block <name> [size <n>]` … `end block` | fields read in order; `size` is checked against them |
//! | `field <name> <type> [= <v>] [bits <mask>] [in <lo> <hi>] [refuse <label>]` | `u8`, `u16`, `u32`, `u64` (optionally `le` / `be`), `bytes <n\|const\|earlier field>` |
//! | `segment <name> block <block>` | one instance |
//! | `segment <name> table <block> count <segment>.<field>` | rows, the count read earlier |
//! | `segment <name> data <table> length <field> [offset <field>]` | one run per row, back to back |
//! | `segment <name> bytes <n\|segment.field\|rest> [at end]` | a run of bytes; `at end` places it at the end |
//! | `segment <name> stream size <n\|segment.field> plus <n> [max <n>]` | the rest as chunks of `size + plus` bytes, the last shorter |
//! | `digest <segment> sha256 before` | a 32-byte segment is the SHA-256 of every byte before it |
//! | `digest <table>.<field> sha256 <data>` | each row's field is the SHA-256 of its run |
//! | `derive <name> sha256 [lenprefix u64be] <item> …` | a value hashed from constants (`NAME:u16le` for an integer) and segments |
//! | `refuse <label> when <condition>` | refuse when the condition holds (see [`cond`]) |
//! | `status <name> when <condition>` / `status <name> otherwise` | the first that holds names the input's status |
//!
//! Segments, `at end` ones last, must cover the input exactly.
//!
//! # Reading untrusted bytes
//!
//! [`decode`] checks every count and length against the bytes that remain, with checked
//! arithmetic, before it is used or anything is allocated from it, and stays within a
//! [`Budget`]. A refusal carries the label of the check, so a consumer maps it to its own
//! error class. The declaration is untrusted text too: its size and the nesting of its
//! conditions are bounded ([`decl::MAX_LINES`], [`cond::MAX_DEPTH`]).
//!
//! # Identifier
//!
//! [`Layout::law_id`] hashes the declaring lines under [`LAYOUT_LAW_KIND`] and
//! [`LAYOUT_SEMANTICS_ID`]; the latter is the fold of pins measured from this crate's
//! behaviour ([`semantics_probe`]). A law another program or release wrote can name the
//! semantics it was written for in a first line `semantics <64 hex digits>`: [`parse`] compares it
//! when present, [`parse_stamped`] requires it, and a different one is refused naming both
//! ([`ErrorKind::SemanticsMismatch`]).

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod cond;
pub mod decl;
mod decode;
mod id;
mod read;
mod read_items;
mod render;
mod token;

pub use decl::{parse, Const, Endian, ErrorKind, Layout, LayoutError};
pub use decode::{decode, Budget, DecodeError, Decoded, FieldValue, Reason};
pub use id::{fold_pins, LAYOUT_LAW_KIND, LAYOUT_SEMANTICS_ID, LAYOUT_SEMANTICS_PINS};
pub use read::parse_stamped;

use alloc::format;
use alloc::string::String;
use core::fmt::Write as _;
use sha2::{Digest as _, Sha256};

/// 64 hex digits as 32 bytes, at compile time
///
/// # Panics
///
/// When `s` is not 64 lower-case hex digits (at compile time for a constant)
#[must_use]
pub const fn hex32(s: &str) -> [u8; 32] {
    const fn nib(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => panic!("hex32: not a lower-case hex digit"),
        }
    }
    let b = s.as_bytes();
    assert!(b.len() == 64, "hex32: 64 hex digits");
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = nib(b[2 * i]) << 4 | nib(b[2 * i + 1]);
        i += 1;
    }
    out
}

/// The declaration the behaviour pin reads (every segment kind, constraint and rule form)
pub const PROBE_LAW: &str = "law probe
kind layout
endian le
const MAGIC bytes = 0x5052
const KNOWN tags = AAAA BBBB
const DOMAIN text = \"probe\"
minimum 2 + 3 refuse short
block head size 9
field magic bytes 2 = MAGIC refuse magic
field version u8 in 1 2 refuse version
field flags u16be bits 0x0003 refuse flags
field count u16
field chunk u16
end block
block row size 41
field tag bytes 4
field len u8 bits 0x7f refuse row_len
field off u32
field sum bytes 32
end block
segment head block head
segment rows table row count head.count refuse layout
segment runs data rows length len offset off refuse layout
segment chunks stream size head.chunk plus 1 max 4 refuse chunks
segment tail bytes 32 at end refuse short
digest tail sha256 before refuse trailer
digest rows.sum sha256 runs refuse run
derive fid sha256 lenprefix u64be DOMAIN MAGIC KNOWN
derive id sha256 head rows
refuse unknown when any(rows: tag not-in KNOWN and not len == 0)
refuse many when count(rows: tag == \"AAAA\") > 1
status first when rows[0].tag == \"AAAA\" and len runs[0] >= 1
status other otherwise
";

/// The behaviour pin: [`PROBE_LAW`] read over a fixed set of inputs, each outcome (the
/// status and every segment's bytes, or the refusal's label, offset and reason) hashed in
/// order
#[must_use]
pub fn semantics_probe() -> [u8; 32] {
    let Ok(law) = parse(PROBE_LAW) else {
        return [0; 32];
    };
    let mut h = Sha256::new();
    for input in semantics_probe_inputs() {
        let line: String = match decode(&law, &input, Budget::for_input(input.len())) {
            Ok(d) => {
                let mut s = format!("ok {:?}", d.status());
                for seg in &law.segments {
                    let _ = write!(s, " {}={:02x?}", seg.name, d.segment_bytes(&seg.name));
                }
                let _ = write!(
                    s,
                    " fid={:02x?} id={:02x?}",
                    d.derived("fid"),
                    d.derived("id")
                );
                s
            }
            Err(e) => format!("err {e}"),
        };
        h.update((line.len() as u64).to_be_bytes());
        h.update(line.as_bytes());
    }
    h.finalize().into()
}

/// The inputs of [`semantics_probe`]: one well-formed input for [`PROBE_LAW`], each single-bit
/// change of it, a shortened one, one with a wrong trailer, one without its trailer and one
/// far too short
#[must_use]
pub fn semantics_probe_inputs() -> alloc::vec::Vec<alloc::vec::Vec<u8>> {
    use alloc::vec::Vec;
    let run: &[u8] = b"xyz";
    let mut good = Vec::new();
    good.extend_from_slice(b"PR");
    good.push(1);
    good.extend_from_slice(&[0, 1]); // flags (big endian)
    good.extend_from_slice(&1u16.to_le_bytes()); // one row
    good.extend_from_slice(&2u16.to_le_bytes()); // chunk size 2
    let rows_end: u32 = 9 + 41;
    good.extend_from_slice(b"AAAA");
    good.push(3);
    good.extend_from_slice(&rows_end.to_le_bytes());
    good.extend_from_slice(&Sha256::digest(run));
    good.extend_from_slice(run);
    good.extend_from_slice(&[7, 7, 7, 8, 8]); // a chunk of 3 and a last one of 2
    let seal = |mut v: Vec<u8>| {
        let d = Sha256::digest(&v);
        v.extend_from_slice(&d);
        v
    };
    let mut out = Vec::new();
    out.push(seal(good.clone()));
    for i in 0..good.len() {
        let mut v = good.clone();
        v[i] ^= 0x01;
        out.push(seal(v));
    }
    let mut cut = good.clone();
    cut.pop();
    cut.pop();
    out.push(seal(cut));
    let mut bad_trailer = seal(good.clone());
    if let Some(last) = bad_trailer.last_mut() {
        *last ^= 0x01;
    }
    out.push(bad_trailer);
    // two faults: row 0 is not where its offset says (checked as row 0 is read) and row 1 has
    // a bit `bits` does not allow; the first is reported
    let mut two = good.clone();
    two[5] = 2;
    let mut row1 = alloc::vec![0u8; 41];
    row1[4] = 0x80;
    two.splice(50..50, row1);
    out.push(seal(two));
    out.push(good.clone()); // no trailer
    out.push(alloc::vec![0; 4]);
    out
}
