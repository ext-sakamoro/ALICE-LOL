//! LLM Guided Generation bridge (Phase X.8 B-6 / B-7).
//!
//! Enabled by the `llm-bridge` feature. Bundles the LOL DSL grammar
//! (`lol.gbnf`) with a one-shot cached parse, re-exports the primitives
//! from `alice-llm` needed to drive grammar-constrained decoding, and
//! provides [`generate_sdf_from_prompt`] — a one-call wrapper that runs
//! an LLM against the grammar and hands back a parsed [`SdfNode`].
//!
//! # One-call flow (B-7, recommended)
//!
//! ```ignore
//! use alice_lol::bridge::{generate_sdf_from_prompt, Llama3Model, GgufTokenizer};
//!
//! let sdf = generate_sdf_from_prompt(&mut model, &tokenizer,
//!                                    "a chess knight", 256)?;
//! // `sdf: SdfNode` is ready to hand off to alice-sdf / print / render.
//! ```
//!
//! # Low-level flow (B-6, if you need custom sampling)
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
//! - Real-model end-to-end verification (Phase X.8 B-9 — Mac Metal +
//!   Jetson Vulkan smoke run against a shipped GGUF).
//! - Sampling knobs beyond greedy: [`generate_sdf_from_prompt`] hard-codes
//!   `temperature = 1.0` (no scaling) and `top_k = 1` (strict argmax)
//!   because DSL generation wants determinism, not diversity. A
//!   `_with_config` variant can be layered on later if needed.
//! - Grammar authoring: the grammar file `lol.gbnf` lives at the ALICE-LOL
//!   workspace root and is loaded via `include_str!` here.

use std::sync::OnceLock;

// Re-exports so downstream crates can `use alice_lol::bridge::*` and
// avoid a direct dependency on alice-llm. Any breaking change in the
// alice-llm surface will surface here as a compile error rather than
// being deferred to a downstream user.
pub use alice_llm::gguf::GgufTokenizer;
pub use alice_llm::grammar::{parse_gbnf, CharSet, Fsm, FsmError, Grammar};
pub use alice_llm::llama3::{GenerateResult, GrammarGenError, Llama3Model};
pub use alice_llm::sampling::{advance_fsm_on_emit, mask_logits_by_grammar, GrammarTokenizer};

use crate::runtime_parser::{parse_lol, ParseError};
use crate::SdfNode;

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

// ---------------------------------------------------------------------------
// Prompt-to-SDF wrapper (Phase X.8 B-7)
// ---------------------------------------------------------------------------

/// Errors from [`generate_sdf_from_prompt`].
///
/// Both variants are recoverable in principle — the caller can retry
/// with a different prompt / more tokens — but neither indicates a bug
/// in this crate. `Parse` in particular *should* never fire against a
/// grammar-obeying LLM output; if it does, `lol.gbnf` and the runtime
/// parser have diverged and the grammar needs updating.
#[derive(Debug)]
pub enum BridgeError {
    /// Grammar-constrained decoding failed inside `alice-llm`.
    ///
    /// Usually [`GrammarGenError::NoValidToken`] — every token was
    /// masked at some step, meaning the grammar cannot be satisfied
    /// from the current context (typically a broken prompt).
    GrammarGen(GrammarGenError),
    /// The generated text passed the grammar mask but the runtime
    /// parser rejected it. Indicates a `lol.gbnf` vs `runtime_parser`
    /// divergence — should not happen with a green golden test suite.
    Parse(ParseError),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GrammarGen(e) => write!(f, "grammar-constrained generation failed: {e}"),
            Self::Parse(e) => write!(f, "generated text failed to parse as LOL: {e}"),
        }
    }
}

impl std::error::Error for BridgeError {}

impl From<GrammarGenError> for BridgeError {
    fn from(e: GrammarGenError) -> Self {
        Self::GrammarGen(e)
    }
}

impl From<ParseError> for BridgeError {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

/// Run the LLM against the LOL grammar and return a parsed [`SdfNode`].
///
/// This is the one-call happy path for grammar-constrained SDF
/// generation. It:
///
/// 1. Invokes [`Llama3Model::generate_grammar`] with `temperature = 1.0`
///    (no scaling) and `top_k = 1` (strict argmax) so output is
///    deterministic given `prompt`.
/// 2. Trims the generated text (LLMs commonly emit trailing newlines).
/// 3. Hands the text to [`parse_lol`] to build the [`SdfNode`] tree.
///
/// # Errors
///
/// - [`BridgeError::GrammarGen`] if the grammar mask starves the sampler
///   (unsatisfiable prompt) or an FSM transition fails.
/// - [`BridgeError::Parse`] if the runtime parser rejects the generated
///   text. This should not happen with a green golden test suite;
///   surfacing it flags a `lol.gbnf` ↔ [`runtime_parser`](crate::runtime_parser) divergence.
///
/// # Determinism
///
/// The API is intentionally minimal — no `temperature` / `top_k` knobs.
/// DSL generation wants a repeatable answer, not diversity. If you need
/// custom sampling, drop down to the [`lol_grammar`] +
/// [`mask_logits_by_grammar`] + [`advance_fsm_on_emit`] primitives and
/// run your own loop.
pub fn generate_sdf_from_prompt(
    model: &mut Llama3Model<'_>,
    tokenizer: &GgufTokenizer,
    prompt: &str,
    max_new_tokens: usize,
) -> Result<SdfNode, BridgeError> {
    let result = model.generate_grammar(
        tokenizer,
        prompt,
        max_new_tokens,
        lol_grammar(),
        1.0, // temperature — no scaling
        1,   // top_k — strict argmax
    )?;
    let node = parse_lol(result.text.trim())?;
    Ok(node)
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

    // ---- BridgeError surface tests (B-7) ----
    // Method-level end-to-end tests require a real GGUF model and are
    // handled in Phase X.8 B-9. Here we cover only the error-type
    // contract that downstream code depends on: Display, From, and
    // std::error::Error blanket impl.

    #[test]
    fn bridge_error_display_wraps_grammar_gen() {
        let e = BridgeError::GrammarGen(GrammarGenError::NoValidToken { step: 7 });
        let msg = format!("{e}");
        assert!(msg.contains("grammar-constrained"));
        assert!(msg.contains("step 7"));
    }

    #[test]
    fn bridge_error_display_wraps_parse_error() {
        let inner = ParseError {
            message: "unexpected token".to_string(),
            position: 42,
        };
        let e = BridgeError::Parse(inner);
        let msg = format!("{e}");
        assert!(msg.contains("failed to parse as LOL"));
        assert!(msg.contains("pos 42"));
    }

    #[test]
    fn bridge_error_from_grammar_gen() {
        // `?` operator relies on the From impl; confirm it round-trips.
        let src = GrammarGenError::NoValidToken { step: 0 };
        let converted: BridgeError = src.clone().into();
        match converted {
            BridgeError::GrammarGen(inner) => assert_eq!(inner, src),
            BridgeError::Parse(_) => panic!("wrong variant"),
        }
    }

    #[test]
    fn bridge_error_from_parse_error() {
        let src = ParseError {
            message: "boom".to_string(),
            position: 3,
        };
        let converted: BridgeError = src.clone().into();
        match converted {
            BridgeError::Parse(inner) => {
                assert_eq!(inner.message, src.message);
                assert_eq!(inner.position, src.position);
            }
            BridgeError::GrammarGen(_) => panic!("wrong variant"),
        }
    }
}
