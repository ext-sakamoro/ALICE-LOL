//! Fuzz target: a rule law reader (`rule_law_from_file`) on any text either reads the law or
//! returns a `RuleLawError`; a panic, an abort (a stack overflow) or a hang is a failure
//!
//! The seeds are the boundary files of the reader's limits (conformance/TASK.md "Limits of a
//! rule law file"): each limit, and one past it

#![no_main]

use alice_lol::rule_law::rule_law_from_file;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = rule_law_from_file(text);
});
