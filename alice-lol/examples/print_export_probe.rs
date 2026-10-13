//! G13 determinism probe: export one scene's STL + 3MF to a directory named on
//! the command line, nothing else.
//!
//! `tests/print_export_determinism.rs` spawns this binary as two separate
//! processes, under different `RAYON_NUM_THREADS`, and diffs their output
//! byte for byte. A separate process (not just a separate thread) also gives
//! each run its own `HashMap` hash-randomization seed, so this additionally
//! stands in for a mutant that has `weld_coincident_vertices` iterate its
//! `HashMap<[u32; 3], usize>` instead of only looking entries up and reading
//! `raw.vertices` in order — see `src/print_export.rs`'s own doc comment on
//! why that function never iterates it.
//!
//! The scene is the lattice-infilled bracket from `examples/print_export.rs`'s
//! case B: a repeated unit cell produces many vertices that land on exactly
//! the same position, so `weld_coincident_vertices` has real work to do
//! (nothing to probe if the raw mesh has no coincident vertices to merge).
//!
//! ```bash
//! cargo run --example print_export_probe -- <out_dir>
//! ```

use alice_lol::lol;
use alice_lol::print_export::{node_to_3mf, node_to_stl, PrintConfig};
use glam::Vec3;

fn main() {
    let out_dir = std::env::args()
        .nth(1)
        .expect("usage: print_export_probe <out_dir>");
    let out_dir = std::path::Path::new(&out_dir);
    std::fs::create_dir_all(out_dir).expect("create_dir_all");

    let config = PrintConfig::high_quality()
        .with_bounds(Vec3::splat(-1.5), Vec3::splat(1.5))
        .with_scale_mm(20.0);

    let bracket = lol! {
        lattice_infill(0.05, 6.0, 0.02,
            subtract(
                rounded_box(1.0, 0.6, 0.4, 0.05),
                translate(0.5, 0.0, 0.0, cylinder(0.2, 0.5))
            )
        )
    };

    node_to_stl(&bracket, out_dir.join("probe.stl"), &config).expect("stl export");
    node_to_3mf(&bracket, out_dir.join("probe.3mf"), &config).expect("3mf export");
}
