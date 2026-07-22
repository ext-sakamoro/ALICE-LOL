//! LLM Guided Generation bridge (Phase X.8 B-6).
//!
//! Enabled by the `llm-bridge` feature. Bundles the LOL DSL grammar
//! (`lol.gbnf`) with a one-shot cached parse and re-exports the
//! primitives from `alice-llm` needed to drive grammar-constrained
//! decoding, so downstream doesn't have to pull in `alice-llm` on its
//! own or hand-load the grammar file.
//!
//! # Typical flow
//!
//! ```ignore
//! use alice_lol::bridge::{lol_grammar, Fsm, mask_logits_by_grammar,
//!                         advance_fsm_on_emit};
//!
//! let grammar = lol_grammar();
//! let mut fsm = Fsm::start(grammar)?.with_max_depth(4096);
//! loop {
//!     let mut logits = model.forward(last_token);
//!     mask_logits_by_grammar(&fsm, tokenizer, &mut logits);
//!     let tok = argmax(&logits);
//!     if tok == tokenizer.eos_id() { break; }
//!     advance_fsm_on_emit(&mut fsm, tokenizer, tok)?;
//!     last_token = tok;
//! }
//! ```
//!
//! # What is out of scope here
//!
//! - Model wiring and sampling loops themselves. See Phase X.8 B-7 for
//!   `generate_sdf_from_prompt`, which glues this bridge to `alice-llm`'s
//!   `Llama3Model::generate_grammar`.
//! - Grammar authoring: the grammar file `lol.gbnf` lives at the ALICE-LOL
//!   workspace root and is loaded via `include_str!` here.

use std::sync::OnceLock;

// Re-exports so downstream crates can `use alice_lol::bridge::*` and
// avoid a direct dependency on alice-llm. Any breaking change in the
// alice-llm surface will surface here as a compile error rather than
// being deferred to a downstream user.
pub use alice_llm::grammar::{parse_gbnf, CharSet, Fsm, FsmError, Grammar};
pub use alice_llm::sampling::{advance_fsm_on_emit, mask_logits_by_grammar, GrammarTokenizer};

/// Recommended `Fsm::with_max_depth` for the LOL grammar.
///
/// The 19-way `expr` dispatch plus deeply nested `translate` +
/// `smooth_union` chains blow past the alice-llm default of 256 on
/// realistic scenes; 4096 gives comfortable headroom for the shapes
/// the golden tests exercise.
pub const LOL_FSM_MAX_DEPTH: usize = 4096;

/// Raw grammar text (compiled into the binary; no filesystem I/O).
const LOL_GBNF: &str = include_str!("../../../lol.gbnf");

static LOL_GRAMMAR: OnceLock<Grammar> = OnceLock::new();

/// Return a shared reference to the parsed LOL DSL grammar.
///
/// The grammar is parsed on the first call and cached for the process
/// lifetime. Subsequent callers get the same `&Grammar` for zero cost.
///
/// # Panics
///
/// Panics if `lol.gbnf` (compiled into the binary at build time) fails
/// to parse. This is a build-time invariant — a green CI means it will
/// never fire at runtime. The panic message includes the parser error
/// for diagnostics.
#[must_use]
pub fn lol_grammar() -> &'static Grammar {
    LOL_GRAMMAR.get_or_init(|| {
        parse_gbnf(LOL_GBNF).unwrap_or_else(|e| panic!("bundled lol.gbnf failed to parse: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_parses_and_is_cached() {
        let g1 = lol_grammar();
        let g2 = lol_grammar();
        // Same reference across calls — the OnceLock cached it.
        assert!(std::ptr::eq(g1, g2));
        assert!(g1.rule("expr").is_some(), "expected root `expr` rule");
    }

    #[test]
    fn grammar_accepts_a_trivial_snippet() {
        let g = lol_grammar();
        let fsm = Fsm::start(g).unwrap().with_max_depth(LOL_FSM_MAX_DEPTH);
        assert!(fsm.accepts_str("sphere(1.0)"));
    }
}
