//! Oracles for layout laws: what a declaration refuses before any bytes are read, how bytes
//! are bounded before they are used, the identifier, and the behaviour pin
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alice_lol_layout::{
    decode, fold_pins, parse, semantics_probe, semantics_probe_inputs, Budget, DecodeError,
    FieldValue, Reason, LAYOUT_SEMANTICS_ID, LAYOUT_SEMANTICS_PINS, PROBE_LAW,
};
use sha2::{Digest, Sha256};

const RECORD: &str = "law record
kind layout
endian le
const VERSION integer = 1
block head size 7
field version u8 = VERSION refuse version
field count u16 refuse count
field reserved u32 = 0 refuse reserved
end block
block row size 3
field kind u8 in 1 3 refuse kind
field len u16
end block
segment head block head
segment rows table row count head.count refuse layout
segment runs data rows length len refuse layout
";

fn record(rows: &[(u8, &[u8])]) -> Vec<u8> {
    let mut v = vec![1];
    v.extend_from_slice(&u16::try_from(rows.len()).unwrap().to_le_bytes());
    v.extend_from_slice(&[0; 4]);
    for (k, r) in rows {
        v.push(*k);
        v.extend_from_slice(&u16::try_from(r.len()).unwrap().to_le_bytes());
    }
    for (_, r) in rows {
        v.extend_from_slice(r);
    }
    v
}

fn refusal(e: &DecodeError) -> (&str, Reason) {
    match e {
        DecodeError::Refused { label, reason, .. } => (label.as_str(), *reason),
        DecodeError::BudgetExceeded { .. } => panic!("expected a refusal, got {e}"),
    }
}

#[test]
fn a_record_reads_field_by_field() {
    let law = parse(RECORD).unwrap();
    let bytes = record(&[(1, b"ab"), (3, b""), (2, b"xyz")]);
    let d = decode(&law, &bytes, Budget::for_input(bytes.len())).unwrap();
    assert_eq!(d.value("head", "count"), Some(FieldValue::Int(3)));
    assert_eq!(d.rows("rows"), 3);
    assert_eq!(d.row_value("rows", 2, "kind"), Some(FieldValue::Int(2)));
    assert_eq!(d.data_run("runs", 0), Some(&b"ab"[..]));
    assert_eq!(d.data_run("runs", 1), Some(&b""[..]));
    assert_eq!(d.data_run("runs", 2), Some(&b"xyz"[..]));
    assert_eq!(d.runs("runs"), 3);
}

#[test]
fn each_check_refuses_with_its_label() {
    let law = parse(RECORD).unwrap();
    let good = record(&[(1, b"ab")]);
    let cases: [(usize, u8, &str, Reason); 4] = [
        (0, 2, "version", Reason::Value),
        (3, 1, "reserved", Reason::Value),
        (7, 4, "kind", Reason::Range),
        (7, 0, "kind", Reason::Range),
    ];
    let mut compared = 0;
    for (at, v, label, reason) in cases {
        let mut b = good.clone();
        b[at] = v;
        let e = decode(&law, &b, Budget::for_input(b.len())).unwrap_err();
        assert_eq!(refusal(&e), (label, reason), "byte {at} = {v}");
        compared += 1;
    }
    // a trailing byte, a missing byte
    let mut b = good.clone();
    b.push(0);
    assert_eq!(
        refusal(&decode(&law, &b, Budget::for_input(b.len())).unwrap_err()),
        ("layout", Reason::Trailing)
    );
    let b = &good[..good.len() - 1];
    assert!(matches!(
        refusal(&decode(&law, b, Budget::for_input(b.len())).unwrap_err()),
        ("layout", Reason::Truncated { .. })
    ));
    assert_eq!(compared, 4);
}

#[test]
fn a_count_is_bounded_by_the_input_before_anything_is_allocated() {
    let law = parse(RECORD).unwrap();
    // 65535 rows claimed, 7 bytes of input: refused from the arithmetic, not by reading
    let b = [1, 0xFF, 0xFF, 0, 0, 0, 0];
    let e = decode(&law, &b, Budget::for_input(b.len())).unwrap_err();
    assert_eq!(
        refusal(&e),
        ("layout", Reason::Truncated { need: 65535 * 3 })
    );
}

#[test]
fn the_budget_is_checked_before_rows_are_allocated() {
    let law = parse(RECORD).unwrap();
    let rows: Vec<(u8, &[u8])> = (0..10).map(|_| (1u8, &b""[..])).collect();
    let b = record(&rows);
    let tight = Budget {
        max_rows: 9,
        max_bytes: 1 << 20,
    };
    assert_eq!(
        decode(&law, &b, tight).unwrap_err(),
        DecodeError::BudgetExceeded {
            what: "rows",
            need: 10,
            limit: 9
        }
    );
    let tiny_bytes = Budget {
        max_rows: 100,
        max_bytes: 8,
    };
    assert!(matches!(
        decode(&law, &b, tiny_bytes).unwrap_err(),
        DecodeError::BudgetExceeded { what: "bytes", .. }
    ));
    assert!(decode(&law, &b, Budget::for_input(b.len())).is_ok());
}

/// The default budget caps decoded values at 64 MiB: a table whose values would take more is
/// refused before its rows are allocated, though every row is in the input
#[test]
fn the_default_budget_caps_decoded_values_at_64_mib() {
    let law = parse(
        "law many
kind layout
endian le
block head size 4
field count u32
end block
block row size 1
field k u8
end block
segment head block head
segment rows table row count head.count refuse layout
",
    )
    .unwrap();
    // one value per row and at least 8 bytes per decoded value: this many rows need more
    // than 64 MiB
    let rows: u32 = (64 << 20) / 8 + 1;
    let mut b = rows.to_le_bytes().to_vec();
    b.resize(4 + rows as usize, 7);
    let budget = Budget::for_input(b.len());
    assert_eq!(budget.max_bytes, 64 << 20);
    assert!(budget.max_rows >= u64::from(rows));
    match decode(&law, &b, budget).unwrap_err() {
        DecodeError::BudgetExceeded { what, need, limit } => {
            assert_eq!((what, limit), ("bytes", 64 << 20));
            assert!(need > limit);
        }
        e => panic!("{e:?}"),
    }
    // the same law reads a table that fits
    let mut small = 3u32.to_le_bytes().to_vec();
    small.extend([1, 2, 3]);
    assert!(decode(&law, &small, Budget::for_input(small.len())).is_ok());
}

#[test]
fn lengths_and_offsets_near_the_integer_limit_are_refused_not_wrapped() {
    let law = parse(
        "law big
kind layout
endian le
block head size 16
field off u64
field len u64
end block
block row size 16
field off u64
field len u64
end block
segment head block head
segment body bytes head.len refuse body
",
    )
    .unwrap_err();
    // `head.off` and the `row` block are unused: the declaration itself is refused
    assert!(law.message.contains("not used"), "{law}");
    let law = parse(
        "law big
kind layout
endian le
block head size 16
field skip u64
field len u64
end block
block row size 16
field off u64
field len u64
end block
segment head block head
segment rows table row count head.skip refuse rows
segment runs data rows length len offset off refuse runs
",
    )
    .unwrap();
    for (count, off, len) in [
        (1u64, u64::MAX, 0u64),
        (1, 32, u64::MAX),
        (1, 32, u64::MAX - 31),
    ] {
        let mut b = Vec::new();
        b.extend_from_slice(&count.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&off.to_le_bytes());
        b.extend_from_slice(&len.to_le_bytes());
        let e = decode(&law, &b, Budget::for_input(b.len())).unwrap_err();
        let (label, reason) = refusal(&e);
        assert_eq!(label, "runs");
        assert!(
            matches!(reason, Reason::Placement | Reason::Truncated { .. }),
            "{reason:?}"
        );
    }
}

#[test]
fn a_stream_ending_right_after_a_full_chunk_is_refused() {
    let law = parse(
        "law chunks
kind layout
const TAG integer = 2
block head size 1
field size u8 in 1 255
end block
segment head block head
segment chunks stream size head.size plus TAG max 3 refuse chunks
",
    )
    .unwrap();
    // size 2: full chunks of 4 bytes, the last 2..=3 bytes
    let ok = [2u8, 1, 1, 1, 1, 9, 9];
    let d = decode(&law, &ok, Budget::for_input(ok.len())).unwrap();
    assert_eq!(d.runs("chunks"), 2);
    assert_eq!(d.data_run("chunks", 1), Some(&[9u8, 9][..]));
    for bad in [
        &[2u8, 1, 1, 1, 1][..],
        &[2, 1, 1, 1, 1, 9][..],
        &[2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 9, 9][..],
    ] {
        let e = decode(&law, bad, Budget::for_input(bad.len())).unwrap_err();
        assert_eq!(refusal(&e).1, Reason::Chunks, "{bad:?}");
    }
    // only a tag: an empty last chunk
    assert_eq!(
        decode(&law, &[2, 9, 9], Budget::for_input(3))
            .unwrap()
            .runs("chunks"),
        1
    );
}

#[test]
fn the_declaration_is_checked_before_any_bytes() {
    let bad: [(&str, &str); 12] = [
        ("law x\nkind audit\n", "kind layout"),
        ("kind layout\n", "law <name>"),
        ("law x\nkind layout\nendian le\nblock b size 3\nfield a u16\nend block\nsegment s block b\n", "size 3"),
        ("law x\nkind layout\nblock b\nfield a u16\nend block\nsegment s block b\n", "endian"),
        ("law x\nkind layout\nconst A integer = 1\nblock b\nfield a u8\nend block\nsegment s block b\n", "not used"),
        ("law x\nkind layout\nblock b\nfield a u8\nend block\nsegment t bytes 1 at end\nsegment s block b\n", "last"),
        ("law x\nkind layout\nblock b\nfield a u8 = 256\nend block\nsegment s block b\n", "fit"),
        ("law x\nkind layout\nblock b\nfield a u8\n", "end block"),
        ("law x\nkind layout\nblock b\nfield a u8\nend block\nsegment s block b\nrefuse r when s.a == \"x\"\n", "types"),
        ("law x\nkind layout\nblock b\nfield a u8\nend block\nsegment s block c\n", "unknown block"),
        ("law x\nkind layout\nconst T text = \"open\n", "not closed"),
        ("law x\nkind layout\nwhatever\n", "unknown keyword"),
    ];
    for (text, needle) in bad {
        let e = parse(text).expect_err(text);
        assert!(e.message.contains(needle), "{text:?}: {e}");
    }
    let long = "#\n".repeat(alice_lol_layout::decl::MAX_LINES + 1);
    assert!(parse(&long).is_err());
}

#[test]
fn a_condition_is_bounded() {
    let deep = format!(
        "law x\nkind layout\nblock b\nfield a u8\nend block\nsegment s block b\nrefuse r when {}s.a == 1\n",
        "not ".repeat(alice_lol_layout::cond::MAX_DEPTH + 2)
    );
    assert!(parse(&deep).unwrap_err().message.contains("deeply"));
    let wide = format!(
        "law x\nkind layout\nblock b\nfield a u8\nend block\nsegment s block b\nrefuse r when s.a == 1{}\n",
        " or s.a == 1".repeat(alice_lol_layout::cond::MAX_ATOMS)
    );
    assert!(parse(&wide).unwrap_err().message.contains("too many"));
}

#[test]
fn the_identifier_names_the_declaration_not_its_spelling() {
    let a = parse(RECORD).unwrap().law_id();
    let spaced = RECORD.replace(" refuse ", "   refuse\t").replace(
        "kind layout\n",
        "kind layout  # a comment\nclaim for people\n\n",
    );
    assert_eq!(parse(&spaced).unwrap().law_id(), a);
    let changed = RECORD.replace("field count u16", "field count u16be");
    assert_ne!(parse(&changed).unwrap().law_id(), a);
    let relabelled = RECORD.replace("refuse kind", "refuse kinds");
    assert_ne!(parse(&relabelled).unwrap().law_id(), a);
}

/// The identifier of a fixed declaration, as a literal: dropping the layout semantics, the kind
/// tag or a line from the hash, or changing how lines are normalised, changes it
#[test]
fn the_identifier_of_a_fixed_declaration_is_pinned() {
    assert_eq!(
        hex(&parse(RECORD).unwrap().law_id()),
        "87074f60f1e416d92aa24471956c539de8dc070191d159cf47848c205b5fbaa3"
    );
}

#[test]
fn a_static_derive_is_the_declared_recipe() {
    let law = parse(
        "law fid
kind layout
endian le
const DOMAIN text = \"d\"
const MAGIC bytes = 0x0102
const N integer = 56
const TAGS tags = AAAA BBBB
block b
field x u8
end block
segment s block b
derive fid sha256 lenprefix u64be DOMAIN MAGIC N:u64le TAGS
derive raw sha256 DOMAIN N:u16be
derive over sha256 s
",
    )
    .unwrap();
    let items: [&[u8]; 4] = [b"d", &[1, 2], &56u64.to_le_bytes(), b"AAAABBBB"];
    let mut h = Sha256::new();
    for i in items {
        h.update((i.len() as u64).to_be_bytes());
        h.update(i);
    }
    let want: [u8; 32] = h.finalize().into();
    assert_eq!(law.derive_static("fid"), Some(want));
    let want_raw: [u8; 32] = Sha256::digest([b'd', 0, 56]).into();
    assert_eq!(law.derive_static("raw"), Some(want_raw));
    // a derive over a segment needs bytes
    assert_eq!(law.derive_static("over"), None);
    let d = decode(&law, &[7], Budget::for_input(1)).unwrap();
    let want_over: [u8; 32] = Sha256::digest([7]).into();
    assert_eq!(d.derived("over"), Some(want_over));
}

#[test]
fn the_behaviour_pin_is_measured() {
    let probe = semantics_probe();
    assert_ne!(
        probe,
        [0; 32],
        "PROBE_LAW does not parse: {:?}",
        parse(PROBE_LAW).err()
    );
    assert_eq!(LAYOUT_SEMANTICS_PINS.len(), 1);
    assert_eq!(
        LAYOUT_SEMANTICS_PINS[0].1,
        probe,
        "decode-probe/v1 moved: {}",
        hex(&probe)
    );
    let fold = fold_pins(LAYOUT_SEMANTICS_PINS);
    assert_eq!(
        LAYOUT_SEMANTICS_ID,
        fold,
        "LAYOUT_SEMANTICS_ID: {}",
        hex(&fold)
    );
}

#[test]
fn the_probe_covers_acceptance_and_every_refusal_it_declares() {
    // the pin has teeth only if the probe inputs reach the checks
    let law = parse(PROBE_LAW).unwrap();
    let mut labels = std::collections::BTreeSet::new();
    let mut accepted = 0;
    for input in semantics_probe_inputs() {
        match decode(&law, &input, Budget::for_input(input.len())) {
            Ok(_) => accepted += 1,
            Err(e) => {
                labels.insert(e.label().unwrap_or("budget").to_string());
            }
        }
    }
    assert!(accepted >= 1);
    for l in [
        "magic", "version", "flags", "layout", "run", "trailer", "unknown", "short",
    ] {
        assert!(
            labels.contains(l),
            "the probe never refuses `{l}`: {labels:?}"
        );
    }
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

#[test]
fn the_markdown_is_the_declaration() {
    let law = parse(RECORD).unwrap();
    let md = law.markdown();
    for want in [
        "| `[0..1)` | `version` | u8 | = 1 (refuse `version`) |",
        "| `[1..3)` | `count` | u16 |  |",
        "| `[3..7)` | `reserved` | u32 | = 0 (refuse `reserved`) |",
        "| `[0..1)` | `kind` | u8 | in 1..=3 (refuse `kind`) |",
        "| `rows` | `head.count` rows of block `row` (refuse `layout`) |",
        "| `runs` | one run per row of `rows`, back to back, `len` bytes each (refuse `layout`) |",
    ] {
        assert!(md.contains(want), "missing {want:?} in\n{md}");
    }
    // the same declaration renders the same, a changed one differently
    assert_eq!(parse(RECORD).unwrap().markdown(), md);
    assert_ne!(
        parse(&RECORD.replace("in 1 3", "in 1 4"))
            .unwrap()
            .markdown(),
        md
    );
}

fn stamp(id: &[u8; 32], text: &str) -> String {
    format!("semantics {}\n{text}", hex(id))
}

#[test]
fn a_stamped_law_is_read_only_under_the_same_layout_semantics() {
    use alice_lol_layout::{parse_stamped, ErrorKind};
    let plain = parse(RECORD).unwrap();
    // the same semantics: read, and the stamp is not part of the identifier
    let same = stamp(&LAYOUT_SEMANTICS_ID, RECORD);
    assert_eq!(parse_stamped(&same).unwrap().law_id(), plain.law_id());
    assert_eq!(parse(&same).unwrap().law_id(), plain.law_id());
    // another semantics: refused by both, naming both identifiers
    let mut other = LAYOUT_SEMANTICS_ID;
    other[0] ^= 1;
    for e in [
        parse_stamped(&stamp(&other, RECORD)).unwrap_err(),
        parse(&stamp(&other, RECORD)).unwrap_err(),
    ] {
        assert_eq!(
            e.kind,
            ErrorKind::SemanticsMismatch {
                ours: LAYOUT_SEMANTICS_ID,
                theirs: other
            }
        );
        assert!(
            e.message.contains(&hex(&other)) && e.message.contains(&hex(&LAYOUT_SEMANTICS_ID)),
            "{e}"
        );
    }
    // the stamped reader requires the line; the plain one does not
    assert_eq!(
        parse_stamped(RECORD).unwrap_err().kind,
        ErrorKind::SemanticsMissing
    );
    // a malformed stamp is a syntax error, not a mismatch
    let bad = parse_stamped(&format!("semantics {}\n{RECORD}", "AB".repeat(32))).unwrap_err();
    assert_eq!(bad.kind, ErrorKind::Syntax);
}
