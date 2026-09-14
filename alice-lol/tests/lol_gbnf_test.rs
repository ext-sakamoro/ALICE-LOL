//! Golden parse tests for `lol.gbnf` (Phase X.8 B-5, feature-gated in
//! Phase X.8 B-6).
//!
//! Loads the grammar file bundled at the workspace root via the
//! [`alice_lol::bridge`] module (B-6) and verifies:
//!
//! 1. `parse_gbnf` accepts it (the grammar file itself is well-formed).
//! 2. `Fsm` accepts a spread of hand-written LOL DSL snippets.
//! 3. `Fsm` also accepts the shipped `examples/sword.lol` demo verbatim.
//! 4. `Fsm` rejects a spread of malformed / unknown-name snippets.
//!
//! The FSM is exercised at a *character* level via `accepts_str`.
//! We do not depend on `parse_lol` itself — that would test the runtime
//! parser, not the grammar. If a snippet parses at runtime but the FSM
//! rejects it, `lol.gbnf` is missing a rule.

#![cfg(feature = "llm-bridge")]

use alice_lol::bridge::{lol_grammar, parse_gbnf, Fsm, Grammar, LOL_FSM_MAX_DEPTH};

const LOL_GBNF: &str = include_str!("../../lol.gbnf");
const SWORD_EXAMPLE: &str = include_str!("../../examples/sword.lol");

fn accepts(g: &Grammar, snippet: &str) -> bool {
    let fsm = Fsm::start(g).unwrap().with_max_depth(LOL_FSM_MAX_DEPTH);
    if !fsm.accepts_str(snippet) {
        return false;
    }
    // Full-input acceptance = "does at least one cursor end in a final
    // state after consuming every char?" `accepts_str` only checks the
    // char-by-char step; is_final at end confirms nothing remains.
    let mut driven = fsm;
    for ch in snippet.chars() {
        driven.advance(ch).expect("accepts_str claimed OK");
    }
    driven.is_final()
}

fn rejects(g: &Grammar, snippet: &str) -> bool {
    let fsm = Fsm::start(g).unwrap().with_max_depth(LOL_FSM_MAX_DEPTH);
    let mut driven = fsm;
    for ch in snippet.chars() {
        if driven.advance(ch).is_err() {
            return true;
        }
    }
    !driven.is_final()
}

#[test]
fn gbnf_is_well_formed() {
    let g = parse_gbnf(LOL_GBNF).expect("lol.gbnf failed to parse");
    // Sanity: at least the categories we care about are present.
    assert!(g.rule("expr").is_some(), "root rule `expr` missing");
    assert!(g.rule("number").is_some(), "number rule missing");
    assert!(g.rule("ws").is_some(), "whitespace rule missing");
}

#[test]
fn accepts_bare_primitives() {
    let g = lol_grammar();
    assert!(accepts(g, "sphere(1.0)"));
    assert!(accepts(g, "box3d(0.5, 0.5, 0.5)"));
    assert!(accepts(g, "cylinder(0.1, 1.0)"));
    assert!(accepts(g, "octahedron(1.0)"));
    assert!(accepts(g, "torus(1.0, 0.25)"));
}

#[test]
fn accepts_negative_and_scientific_numbers() {
    let g = lol_grammar();
    assert!(accepts(g, "translate(0.0, -1.0, 0.0, sphere(1.0))"));
    assert!(accepts(g, "sphere(1e-3)"));
    assert!(accepts(g, "sphere(1.5E+2)"));
    assert!(accepts(g, "sphere(.5)"));
}

#[test]
fn accepts_whitespace_and_newlines() {
    let g = lol_grammar();
    assert!(accepts(g, "  sphere( 1.0 )  "));
    assert!(accepts(g, "sphere(\n\t1.0\n)"));
}

#[test]
fn accepts_line_comments() {
    let g = lol_grammar();
    let snippet = "// leading comment\nsphere(1.0)\n";
    assert!(accepts(g, snippet));
    let inner = "smooth_union(0.3, // between args\n  sphere(1.0), box3d(1.0, 1.0, 1.0))";
    assert!(accepts(g, inner));
}

#[test]
fn accepts_boolean_ops() {
    let g = lol_grammar();
    assert!(accepts(g, "union(sphere(1.0), box3d(1.0, 1.0, 1.0))"));
    assert!(accepts(
        g,
        "smooth_union(0.3, sphere(1.0), box3d(0.5, 0.5, 0.5))"
    ));
    assert!(accepts(g, "subtract(sphere(1.0), box3d(0.5, 0.5, 0.5))"));
    assert!(accepts(
        g,
        "smooth_subtract(0.1, sphere(1.0), box3d(0.5, 0.5, 0.5))"
    ));
    assert!(accepts(
        g,
        "stairs_union(0.2, 4.0, sphere(1.0), box3d(1.0, 1.0, 1.0))"
    ));
    assert!(accepts(
        g,
        "groove(0.2, 0.05, sphere(1.0), box3d(0.5, 0.5, 0.5))"
    ));
}

#[test]
fn accepts_transforms_and_modifiers() {
    let g = lol_grammar();
    assert!(accepts(g, "translate(0.0, 1.0, 0.0, sphere(1.0))"));
    assert!(accepts(g, "rotate(0.0, 0.5, 0.0, box3d(0.5, 0.5, 0.5))"));
    assert!(accepts(g, "scale(2.0, sphere(1.0))"));
    assert!(accepts(g, "scale_non_uniform(1.0, 2.0, 0.5, sphere(1.0))"));
    assert!(accepts(g, "twist(0.5, box3d(0.5, 0.5, 0.5))"));
    assert!(accepts(g, "octant_mirror(sphere(1.0))"));
    assert!(accepts(g, "animate(1.0, 0.2, sphere(1.0))"));
    assert!(accepts(g, "morph(0.5, sphere(1.0), box3d(1.0, 1.0, 1.0))"));
}

#[test]
fn accepts_deeply_nested_snippet() {
    let g = lol_grammar();
    let snippet = "translate(0.0, 1.2, 0.0, smooth_union(0.2, \
                   scale(0.3, sphere(1.0)), \
                   rotate(0.0, 0.5, 0.0, box3d(0.5, 0.5, 0.5))))";
    assert!(accepts(g, snippet));
}

#[test]
fn accepts_shipped_sword_example() {
    let g = lol_grammar();
    assert!(
        accepts(g, SWORD_EXAMPLE.trim_end()),
        "examples/sword.lol was not accepted by lol.gbnf"
    );
}

#[test]
fn rejects_unknown_name() {
    let g = lol_grammar();
    assert!(rejects(g, "spheer(1.0)"));
    assert!(rejects(g, "unknown_fn(1.0)"));
    assert!(rejects(g, "cube(1.0)")); // real geometry, wrong name
}

#[test]
fn rejects_empty_input() {
    let g = lol_grammar();
    assert!(rejects(g, ""));
    assert!(rejects(g, "   "));
    assert!(rejects(g, "// just a comment\n"));
}

#[test]
fn rejects_bad_syntax() {
    let g = lol_grammar();
    // Missing close paren.
    assert!(rejects(g, "sphere(1.0"));
    // Empty argument list where a number is expected.
    assert!(rejects(g, "sphere()"));
    // Trailing junk after a complete expression.
    assert!(rejects(g, "sphere(1.0) trailing"));
    // Runtime-parser-specific syntax that the LLM path never emits.
    assert!(rejects(g, "sphere({r})"));
}

#[test]
fn rejects_variadic_with_single_child() {
    // `union` is a *variadic* op — the grammar (like the runtime parser)
    // requires at least two children so `fold_left` has something to
    // combine. A single child is a category-level mistake.
    let g = lol_grammar();
    assert!(rejects(g, "union(sphere(1.0))"));
    assert!(rejects(g, "smooth_union(0.3, sphere(1.0))"));
}

// ---------------------------------------------------------------------------
// Phase 3 Intent / program(...) (A0)
// ---------------------------------------------------------------------------

#[test]
fn accepts_program_wrapper_forms() {
    let g = lol_grammar();
    assert!(accepts(g, "program(sphere(1.0))"));
    assert!(accepts(g, "program(sphere(1.0), entities())"));
    assert!(accepts(
        g,
        "program(sphere(1.0), entities(box3d(0.1, 0.1, 0.1)))"
    ));
    assert!(accepts(
        g,
        "program(sphere(1.0), entities(box3d(0.1, 0.1, 0.1), sphere(0.2)), grasp(1, right, 5.0))"
    ));
    assert!(accepts(
        g,
        "program(\n  sphere(1.0), // scene\n  entities(sphere(0.2)),\n  seq(grasp(0, left, 3.0), rest(500))\n)"
    ));
}

#[test]
fn accepts_every_intent_verb() {
    let g = lol_grammar();
    let verbs = [
        "grasp(0, right, 5.0)",
        "release(0)",
        "catch(0)",
        "walk(1.0, 0.0, 2.0, 1.2)",
        "gaze(0.0, 1.5, 0.0, 800)",
        "point(1.0, 1.0, 1.0, both)",
        "throw(3.0, 1.0, 0.0, 9.5, right)",
        "push(0, 1.0, 0.0, 0.0, 2.0)",
        "pull(0, -1.0, 0.0, 0.0, 2.0)",
        "turn(0, 0.0, 1.0, 0.0, 1.25)",
        "align(0, 0.0, 0.0, 1.0)",
        "follow(0, 0.5)",
        "avoid(0, 0.3)",
        "rest(250)",
        "latent(0.1, 0.2, 0.3, 0.4)",
        "latent(0.1, 0.2, 0.3, 0.4, 0.5, 0.6)",
        "seq(rest(1), rest(2))",
        "par(walk(0.0, 0.0, 1.0, 1.0), gaze(0.0, 1.0, 0.0, 100))",
        "seq(par(rest(1), rest(2)), grasp(0, both, 1.0))",
        "music(0, 1, 2, 3, 4, 5, 6, 255)",
    ];
    for v in verbs {
        let snippet = format!("program(sphere(1.0), entities(sphere(0.1)), {v})");
        assert!(accepts(g, &snippet), "grammar rejected intent verb: {v}");
    }
}

#[test]
fn rejects_intent_outside_program_and_bad_intent_shapes() {
    let g = lol_grammar();
    // Intent verbs are only valid in the third slot of program(...).
    assert!(rejects(g, "grasp(0, right, 5.0)"));
    assert!(rejects(g, "program(grasp(0, right, 5.0))"));
    // Intent slot needs entities(...) first.
    assert!(rejects(g, "program(sphere(1.0), rest(1))"));
    // SDF `rotate` is not an intent verb (the intent spelling is `turn`).
    assert!(rejects(
        g,
        "program(sphere(1.0), entities(), rotate(0, 0.0, 1.0, 0.0, 1.0))"
    ));
    // Hand must be a bare identifier from the enum.
    assert!(rejects(
        g,
        "program(sphere(1.0), entities(), grasp(0, up, 5.0))"
    ));
    // NodeId must be an unsigned integer literal.
    assert!(rejects(g, "program(sphere(1.0), entities(), release(-1))"));
    assert!(rejects(g, "program(sphere(1.0), entities(), release(0.5))"));
    // latent needs at least 4 values.
    assert!(rejects(
        g,
        "program(sphere(1.0), entities(), latent(0.1, 0.2, 0.3))"
    ));
    // music needs exactly 8 bytes.
    assert!(rejects(
        g,
        "program(sphere(1.0), entities(), music(0, 1, 2, 3, 4, 5, 6))"
    ));
    // seq / par need at least one child.
    assert!(rejects(g, "program(sphere(1.0), entities(), seq())"));
}
