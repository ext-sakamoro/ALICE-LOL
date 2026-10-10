//! Stdlib shapes built from untrusted parameters (e.g. from an API request,
//! not a file you wrote yourself), using the fallible `try_*` entry points so
//! a degenerate value never panics: it is refused as an `Err` and handled,
//! not an uncontrolled allocation or an infinite loop.

use alice_lol::stdlib::hardsurface::pattern_sdf::{try_gridfinity_bin, GridfinitySpec};
use alice_lol::stdlib::hardsurface::skadis_sdf::try_skadis_panel_sdf;

fn build_gridfinity(cols: u32, rows: u32) {
    let spec = GridfinitySpec {
        dividers: Some((cols, rows)),
        ..GridfinitySpec::default_2x2()
    };
    match try_gridfinity_bin(&spec) {
        Ok(_bin) => println!("gridfinity {cols}x{rows} dividers: built"),
        Err(e) => println!("gridfinity {cols}x{rows} dividers: refused ({e})"),
    }
}

fn build_skadis_panel(size: f32) {
    match try_skadis_panel_sdf(size, 5.0, 5.0) {
        Ok(_panel) => println!("skadis panel size={size}: built"),
        Err(e) => println!("skadis panel size={size}: refused ({e})"),
    }
}

fn main() {
    build_gridfinity(2, 2); // a normal request
    build_gridfinity(1024, 1024); // a degenerate one: refused, not a crash

    build_skadis_panel(300.0); // a normal request
    build_skadis_panel(f32::NAN); // a degenerate one: refused, not a hang
    build_skadis_panel(f32::INFINITY); // same, refused instead of looping forever
}
