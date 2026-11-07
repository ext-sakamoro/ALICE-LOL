//! Fuzz target: a world law reader on any text either reads the law or returns a `WorldLawError`;
//! a law that reads binds its defaults (the reader checks it), so it has an identifier and a plain
//! text; a panic, an abort or a hang is a failure

#![no_main]

use alice_lol::world_law::WorldLaw;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(law) = WorldLaw::from_file(text) {
        let _ = law.law_id();
        let bound = law.bind(&[]).expect("a law that reads binds its defaults");
        let _ = bound.describe();
    }
});
