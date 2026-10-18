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

const LOL_GBNF: &str = include_str!("../lol.gbnf");
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
fn accepts_single_whitespace_rejects_runs() {
    // The LLM grammar allows at most one whitespace char between tokens
    // (the runtime lexer allows any run). Unbounded `ws` was a rambling
    // channel: a 41-tab run was observed once comments were removed.
    let g = lol_grammar();
    assert!(accepts(g, " sphere( 1.0 ) "));
    assert!(accepts(g, "sphere(\n1.0\n)"));
    assert!(accepts(g, "sphere(\t1.0\t)"));
    assert!(rejects(g, "  sphere(1.0)"));
    assert!(rejects(g, "sphere(1.0)\t\t"));
    assert!(rejects(g, "sphere(\n\t1.0\n)"));
    assert!(rejects(g, "union(sphere(1.0),  box3d(1.0, 1.0, 1.0))"));
}

/// Collapse whitespace runs to a single space so human-formatted `.lol`
/// files (4-space indentation) can be checked against the LLM grammar.
fn collapse_ws(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_ws = false;
    for ch in src.chars() {
        if ch.is_ascii_whitespace() {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    out
}

#[test]
fn rejects_line_comments() {
    // Comments are lexer-only (human files). The LLM grammar excludes
    // them on purpose: a comment state admits almost every token, which
    // both defeats the token-trie mask and lets the model ramble instead
    // of emitting the program (B-10, 2026-09-14). `parse_lol` still
    // accepts them, so this is grammar ⊂ parser, never the reverse.
    let g = lol_grammar();
    assert!(rejects(g, "// leading comment\nsphere(1.0)\n"));
    assert!(rejects(
        g,
        "smooth_union(0.3, // between args\n  sphere(1.0), box3d(1.0, 1.0, 1.0))"
    ));
    // Plain whitespace / newlines between tokens remain fine.
    assert!(accepts(
        g,
        "smooth_union(0.3,\nsphere(1.0),\nbox3d(1.0, 1.0, 1.0))\n"
    ));
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
    assert!(accepts(
        g,
        "rotate_q(0.0, 0.0, 0.0, 1.0, box3d(0.5, 0.5, 0.5))"
    ));
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
        accepts(g, &collapse_ws(SWORD_EXAMPLE.trim_end())),
        "examples/sword.lol (whitespace-collapsed) was not accepted by lol.gbnf"
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
        "program(\nsphere(1.0),\nentities(sphere(0.2)),\nseq(grasp(0, left, 3.0), rest(500))\n)"
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

#[test]
fn accepts_llm_reference_coaster_example_verbatim() {
    // `LLM_REFERENCE.md` "3D Printable: ALICE Coaster" block, copied as-is
    // (no comments, no indentation) — the reference must show the model
    // exactly what the LLM grammar accepts.
    let g = lol_grammar();
    let snippet = r"subtract(
subtract(
subtract(
subtract(
cylinder(2.5, 0.125),
polar_repeat(12, translate(1.8, 0.0, 0.0, cylinder(0.3, 0.2)))
),
rotate(0.0, 15.0, 0.0,
polar_repeat(12, translate(1.2, 0.0, 0.0, cylinder(0.2, 0.2)))
)
),
polar_repeat(6, translate(0.6, 0.0, 0.0, hex_prism(0.15, 0.2)))
),
translate(0.0, 0.04, 0.0, hex_prism(0.25, 0.1))
)";
    assert!(accepts(g, snippet));
    assert!(alice_lol::runtime_parser::parse_lol(snippet).is_ok());
}

#[test]
fn grammar_covers_every_runtime_parser_construct() {
    // Drift guard (2026-09-14): every `"name" => { … }` arm of the runtime
    // parser must appear as a quoted name in `lol.gbnf`. Until today 103
    // product / mechanical / fastener constructs were parseable but not
    // emittable under the grammar — they had been added to the parser and
    // to a downstream copy of the grammar, never here.
    const PARSER_SRC: &str = include_str!("../src/runtime_parser.rs");
    // Not grammar constructs: `semantics` is the identifier wrapper (`semantics(<hex>, ...)`, and the
    // first line `semantics <hex>` of an audit law, ALICE-Law G7). The program that wrote a value adds
    // it (`emit::to_lol`); a model has no identifier to state, and a grammar that let it write one
    // would let it claim an identifier it does not have. The authoring readers take text without it.
    const NOT_IN_GRAMMAR: &[&str] = &["semantics"];
    let grammar_names: std::collections::HashSet<&str> = LOL_GBNF
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| {
            s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
        .collect();
    let mut missing = Vec::new();
    for line in PARSER_SRC.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with('"') || !trimmed.contains("=>") {
            continue;
        }
        // `"a" | "b" => {` — collect every quoted identifier on the line.
        for name in trimmed.split('"').skip(1).step_by(2) {
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                && !grammar_names.contains(name)
                && !NOT_IN_GRAMMAR.contains(&name)
            {
                missing.push(name.to_string());
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "runtime_parser constructs missing from lol.gbnf: {missing:?}"
    );
    for name in NOT_IN_GRAMMAR {
        assert!(
            PARSER_SRC.contains(&format!("\"{name}\" =>")),
            "{name} is exempt but the parser no longer reads it"
        );
        assert!(
            !grammar_names.contains(name),
            "{name} is exempt but lol.gbnf has it"
        );
    }
}

#[test]
fn accepts_product_and_mechanical_shortcuts() {
    let g = lol_grammar();
    assert!(accepts(g, "pen_cup(50,100)"));
    assert!(accepts(
        g,
        "union(pen_cup(50,100), translate(33,0,50, rotate(0,90,0, torus(15,5))))"
    ));
    assert!(accepts(g, "gridfinity_bin(2, 3, 6)"));
    assert!(accepts(g, "gridfinity_bin_ex(2, 3, 6, 1, 2, 1.2, 1.0)"));
    assert!(accepts(g, "wall_hook()"));
    assert!(accepts(g, "vesa_mount(100, 4, 4)"));
    assert!(accepts(
        g,
        "subtract(rounded_box(30,2.5,30,3), screw_hole(4,15))"
    ));
    assert!(accepts(g, "jst_ph_slot(4)"));
}

// ─── audit law values: every value `parse_law` reads as one token, in printable ASCII ──
//
// `parse_law` splits a clause on whitespace and reads `#` as the start of a comment, so a
// value is any run without them; the grammar takes the printable ASCII part of that (it is
// stricter on purpose: no comment, no non-ASCII), which includes method paths and versions

#[test]
fn accepts_law_values_with_paths_underscores_and_versions() {
    let g = lol_grammar();
    for law in [
        "audit lint\nrange banned f32::sin f64::sin_cos\n",
        "audit lint\nrange known case_17 a-b 0.4.0 +x ~y\n",
    ] {
        assert!(accepts(g, law), "{law:?}");
        assert!(alice_lol::runtime_parser::parse_law(law).is_ok(), "{law:?}");
    }
}

#[test]
fn rejects_a_law_value_with_a_comment_character() {
    let g = lol_grammar();
    assert!(rejects(g, "audit lint\nrange known a#b\n"));
}

// ─── audit laws (the `law` rule): the grammar and `parse_law` agree ──────────

const AUDIT_LAW_EVERY_CLAUSE: &str =
    "audit lint-bans\nevidence compared\nexpect mismatches == 0 within 0\nrange known a b\nincludes banned f32::sin f64::sin_cos\n";

#[test]
fn accepts_an_audit_law_with_every_clause() {
    let g = lol_grammar();
    assert!(accepts(g, AUDIT_LAW_EVERY_CLAUSE));
    assert!(alice_lol::runtime_parser::parse_law(AUDIT_LAW_EVERY_CLAUSE).is_ok());
}

#[test]
fn rejects_an_includes_clause_without_a_value() {
    let g = lol_grammar();
    let law = "audit lint-bans\nincludes banned\n";
    assert!(rejects(g, law));
    assert!(alice_lol::runtime_parser::parse_law(law).is_err());
}

#[test]
fn rejects_an_unknown_audit_clause() {
    let g = lol_grammar();
    let law = "audit lint-bans\ncontains banned f32::sin\n";
    assert!(rejects(g, law));
    assert!(alice_lol::runtime_parser::parse_law(law).is_err());
}

// ─── units declarations (the `units` rule): the grammar and `parse_declared` agree ──────

#[test]
fn accepts_every_declared_unit_and_purpose_the_parser_accepts() {
    use alice_lol::units::{Purpose, Unit, UnitsDecl};
    let g = lol_grammar();
    let mut compared = 0;
    for unit in Unit::ALL {
        for purpose in Purpose::ALL {
            for body in [
                "sphere(1.0)",
                "program(sphere(1.0), entities(box3d(0.1, 0.1, 0.1)), rest(500))",
            ] {
                let text = format!("units({}, {}, {body})", unit.name(), purpose.name());
                let parsed = alice_lol::runtime_parser::parse_declared(&text);
                let valid = UnitsDecl::new(unit, purpose).is_some();
                assert_eq!(parsed.is_ok(), valid, "{text}: {parsed:?}");
                assert_eq!(accepts(g, &text), valid, "{text}");
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 20);
}

#[test]
fn rejects_unknown_nested_and_misplaced_units_declarations() {
    let g = lol_grammar();
    for text in [
        "units(km, print, sphere(1.0))",
        "units(mm, show, sphere(1.0))",
        "units(mm, print, units(mm, print, sphere(1.0)))",
        "union(units(mm, print, sphere(1.0)), sphere(1.0))",
        "units(mm, print)",
        "units(mm, print, sphere(1.0)) sphere(1.0)",
    ] {
        assert!(rejects(g, text), "grammar accepted {text:?}");
        assert!(
            alice_lol::runtime_parser::parse_declared(text).is_err(),
            "parser accepted {text:?}"
        );
    }
}
