//! A Gridfinity bin built from untrusted parameters (e.g. from an API
//! request, not a file you wrote yourself), using the fallible `try_*`
//! entry point so a degenerate value never panics: it is refused as an
//! `Err` and handled, not an uncontrolled `Vec::with_capacity` overflow.

use alice_lol::stdlib::hardsurface::pattern_sdf::{try_gridfinity_bin, GridfinitySpec};

fn build(cols: u32, rows: u32) {
    let spec = GridfinitySpec {
        dividers: Some((cols, rows)),
        ..GridfinitySpec::default_2x2()
    };
    match try_gridfinity_bin(&spec) {
        Ok(_bin) => println!("{cols}x{rows} dividers: built"),
        Err(e) => println!("{cols}x{rows} dividers: refused ({e})"),
    }
}

fn main() {
    build(2, 2); // a normal request
    build(1024, 1024); // a degenerate one: refused, not a crash
}
