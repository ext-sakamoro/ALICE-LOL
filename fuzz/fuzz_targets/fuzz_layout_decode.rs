//! Fuzz target: a layout law reads untrusted bytes and untrusted declarations without panic
//!
//! The first byte picks the mode: the probe declaration over arbitrary bytes (every segment
//! kind, digest and rule form), or arbitrary text as a declaration and, when it reads, the
//! rest of the input decoded by it. A decoded input is covered by its segments exactly.

#![no_main]

use alice_lol::layout::{decode, parse, Budget, PROBE_LAW};
use libfuzzer_sys::fuzz_target;

fn covered(law: &alice_lol::layout::Layout, d: &alice_lol::layout::Decoded<'_, '_>, len: usize) {
    let total: usize = law
        .segments
        .iter()
        .map(|s| d.segment_bytes(&s.name).map_or(0, <[u8]>::len))
        .sum();
    assert_eq!(total, len, "the segments do not cover the input");
}

fuzz_target!(|data: &[u8]| {
    let Some((&mode, rest)) = data.split_first() else {
        return;
    };
    if mode % 2 == 0 {
        let law = parse(PROBE_LAW).expect("the probe declaration reads");
        let budget = Budget::for_input(rest.len());
        if let Ok(d) = decode(&law, rest, budget) {
            covered(&law, &d, rest.len());
        }
        return;
    }
    // declaration text, then a zero byte, then the bytes to decode
    let (text, bytes) = match rest.iter().position(|&b| b == 0) {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, &[][..]),
    };
    let Ok(text) = std::str::from_utf8(text) else {
        return;
    };
    let Ok(law) = parse(text) else {
        return;
    };
    let _ = law.law_id();
    for d in &law.derives {
        let _ = law.derive_static(&d.name);
    }
    if let Ok(d) = decode(&law, bytes, Budget::for_input(bytes.len())) {
        covered(&law, &d, bytes.len());
    }
});
