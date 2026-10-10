//! Probe for the degenerate-parameter grid test (`tests/degenerate_grid.rs`).
//!
//! `argv[1]` is one `.lol` source string. This binary calls `parse_lol` on it
//! and exits 0 (`Ok`) or 1 (`Err`) -- a clean result either way, since a
//! malformed-but-detected input is the parser doing its job. There is no
//! orchestration logic here at all: the grid test spawns one of these per
//! case and classifies the *process*, not this binary's own return value
//! alone, because the hazards being probed for (a hang, an unbounded
//! allocation, a panic) do not necessarily let this binary's own `main`
//! return in the first place.
//!
//! A custom global allocator tracks live bytes and exits with a distinct
//! code (42) once they exceed a cap, instead of letting the process actually
//! grow unbounded: `ulimit`/`RLIMIT_AS` is not reliably enforced on every
//! platform this runs on, so the cap has to live inside the process that is
//! doing the allocating.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Live-byte cap before the probe exits 42 instead of continuing to grow.
/// 256 MiB: generously above what any legitimate stdlib construction needs
/// (the largest known eager site, `MAX_NODE_EXPANSION` = 10,000 nodes, is
/// ~6 MB, see `alice_lol::limits`), far below what would actually exhaust a
/// CI runner.
const LIVE_BYTES_CAP: usize = 256 * 1024 * 1024;
const ALLOC_CAP_EXIT_CODE: i32 = 42;

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

// SAFETY: delegates every operation to `System`, the only addition is an
// atomic counter and an early `process::exit` -- no unsafe memory access of
// our own.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let live = LIVE_BYTES.fetch_add(layout.size(), Ordering::SeqCst) + layout.size();
        if live > LIVE_BYTES_CAP {
            // flush first: stdout/stderr buffering can otherwise lose the
            // reason for the exit when the process is torn down immediately
            eprintln!("parse_one_lol: live bytes {live} > cap {LIVE_BYTES_CAP}, exiting");
            std::process::exit(ALLOC_CAP_EXIT_CODE);
        }
        // SAFETY: forwarding to the system allocator with the same layout
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::SeqCst);
        // SAFETY: forwarding to the system allocator with the same
        // pointer/layout this allocator itself returned from `alloc`
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn main() {
    let Some(src) = std::env::args().nth(1) else {
        eprintln!("usage: parse_one_lol <source>");
        std::process::exit(2);
    };
    match alice_lol::runtime_parser::parse_lol(&src) {
        Ok(_) => std::process::exit(0),
        Err(_) => std::process::exit(1),
    }
}
