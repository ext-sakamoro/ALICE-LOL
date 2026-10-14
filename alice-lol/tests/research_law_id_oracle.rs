//! `ResearchLaw::law_id`: the identifier of a research law (oracle written before the
//! implementation)
//!
//! A research law's identity is its claim: two laws that predict the same output for every
//! input under the same arithmetic are the same law, and what only records support for the
//! claim (observations, the residual, provenance, oracle cases, a parameter's uncertainty)
//! does not change which law it is.
//!
//! oracle: the serialization written out in this file from the specification below, with
//! `LawIdHasher`'s typed fields (independent of the implementation's own traversal), plus
//! one hex value recorded for a fixed law so the serialization cannot drift silently.
//!
//! specification (`RESEARCH_LAW_KIND` = `lol.law.research/v1`):
//!   name;
//!   the expression tree in preorder, each node a u32 tag: 0 number (its f64 bits),
//!   1 identifier (its name), 2 negation, 3 binary (+ u32 operator 0 add / 1 sub / 2 mul /
//!   3 div, then left, right), 4 power (base, then the exponent's f64 bits), 5 call
//!   (+ the function's name, + u64 argument count, then the arguments);
//!   the output's name and unit text;
//!   u64 input count, then each input's name and unit text;
//!   u64 parameter count, then each parameter's name, value (f64 bits) and unit text;
//!   per input, in input order: u32 0 (no range) or u32 1 + the bounds' f64 bits

#![allow(clippy::unwrap_used)]

use alice_lol::law_id::{LawIdHasher, LOL_SEMANTICS_ID, RESEARCH_LAW_KIND};
use alice_lol::research_law::{Observation, Param, ResearchLaw, ResearchOracle, Var};
use alice_zip::law::{Provenance, ValidRange};

const R: f64 = 8.314_462_618;

fn law(
    name: &str,
    expr: &str,
    r: f64,
    r_unc: f64,
    v_unit: &str,
    t_range: (f64, f64),
) -> ResearchLaw {
    ResearchLaw::new(
        name,
        expr,
        Var::new("P", "Pa"),
        &[
            Var::new("n", "mol"),
            Var::new("T", "K"),
            Var::new("V", v_unit),
        ],
        &[Param::new("R", r, r_unc, "J/(mol*K)")],
        &[(
            "T",
            ValidRange {
                lo: t_range.0,
                hi: t_range.1,
            },
        )],
        Provenance::new("closed form", "stated"),
    )
    .unwrap()
}

fn gas() -> ResearchLaw {
    law("ideal gas", "n*R*T/V", R, 0.0, "m^3", (1.0, 2000.0))
}

fn id(l: &ResearchLaw) -> [u8; 32] {
    l.law_id(&LOL_SEMANTICS_ID)
}

/// the semantics identifier the recorded values below were computed under (LOL under
/// alice-det-math 0.5.0): a recorded value pins the serialization, so it is compared under the
/// generation it was recorded in, whatever alice-det-math this build links
const fn recorded_semantics() -> [u8; 32] {
    alice_lol::law_id::semantics_id_for(&alice_lol::law_id::hex32(
        "135c407d4249c1c200e4d1be79a12e848ce5c355fc3a918da965849e697ef000",
    ))
}

/// `n*R*T/V` parses left-associatively: ((n * R) * T) / V
fn expected_gas_id(semantics: &[u8; 32]) -> [u8; 32] {
    let ident = |h: LawIdHasher, s: &str| h.u32(1).str(s);
    let mut h = LawIdHasher::new(RESEARCH_LAW_KIND, semantics).str("ideal gas");
    h = h.u32(3).u32(3); // div
    h = h.u32(3).u32(2); //   mul
    h = h.u32(3).u32(2); //     mul
    h = ident(h, "n");
    h = ident(h, "R");
    h = ident(h, "T");
    h = ident(h, "V");
    h = h.str("P").str("Pa");
    h = h
        .u64(3)
        .str("n")
        .str("mol")
        .str("T")
        .str("K")
        .str("V")
        .str("m^3");
    h = h.u64(1).str("R").f64(R).str("J/(mol*K)");
    h = h.u32(0); // n: no range
    h = h.u32(1).f64(1.0).f64(2000.0); // T
    h = h.u32(0); // V: no range
    h.finish()
}

/// A law whose tree uses every node kind: number, identifier, negation, the four binary
/// operators, power, and calls with 1, 2 and 3 arguments (the parser: unary minus above
/// power, `*` `/` above `+` `-`, all binary operators left-associative)
const ALL_NODES: &str = "-a + b*k - max(a, b, c)/sqrt(c)^2 + atan2(b, a) + 0.5";

fn all_nodes(expr: &str) -> ResearchLaw {
    ResearchLaw::new(
        "all nodes",
        expr,
        Var::new("y", "1"),
        &[Var::new("a", "1"), Var::new("b", "1"), Var::new("c", "1")],
        &[Param::new("k", 2.0, 0.0, "1")],
        &[("c", ValidRange { lo: 0.0, hi: 10.0 })],
        Provenance::new("closed form", "stated"),
    )
    .unwrap()
}

/// ((((-a + b*k) - max(a, b, c) / sqrt(c)^2) + atan2(b, a)) + 0.5)
fn expected_all_nodes_id(semantics: &[u8; 32]) -> [u8; 32] {
    let ident = |h: LawIdHasher, s: &str| h.u32(1).str(s);
    let mut h = LawIdHasher::new(RESEARCH_LAW_KIND, semantics).str("all nodes");
    h = h.u32(3).u32(0); // + 0.5
    h = h.u32(3).u32(0); //   + atan2(b, a)
    h = h.u32(3).u32(1); //     - (max / sqrt^2)
    h = h.u32(3).u32(0); //       -a + b*k
    h = ident(h.u32(2), "a"); //         -a
    h = h.u32(3).u32(2); //         b * k
    h = ident(h, "b");
    h = ident(h, "k");
    h = h.u32(3).u32(3); //       max(a, b, c) / sqrt(c)^2
    h = h.u32(5).str("max").u64(3);
    h = ident(h, "a");
    h = ident(h, "b");
    h = ident(h, "c");
    h = h.u32(4); //         power: base, then exponent bits
    h = h.u32(5).str("sqrt").u64(1);
    h = ident(h, "c");
    h = h.f64(2.0);
    h = h.u32(5).str("atan2").u64(2); //     atan2(b, a)
    h = ident(h, "b");
    h = ident(h, "a");
    h = h.u32(0).f64(0.5); //   0.5
    h = h.str("y").str("1");
    h = h
        .u64(3)
        .str("a")
        .str("1")
        .str("b")
        .str("1")
        .str("c")
        .str("1");
    h = h.u64(1).str("k").f64(2.0).str("1");
    h = h.u32(0).u32(0).u32(1).f64(0.0).f64(10.0);
    h.finish()
}

#[test]
fn every_node_kind_is_the_specified_serialization() {
    assert_eq!(
        all_nodes(ALL_NODES).law_id(&LOL_SEMANTICS_ID),
        expected_all_nodes_id(&LOL_SEMANTICS_ID)
    );
}

#[test]
fn the_recorded_value_for_the_all_nodes_law_does_not_drift() {
    let hex = all_nodes(ALL_NODES)
        .law_id(&recorded_semantics())
        .iter()
        .fold(String::new(), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        });
    assert_eq!(hex, RECORDED_ALL_NODES_ID);
}

const RECORDED_ALL_NODES_ID: &str =
    "14295d24370f2f135af1452a809def929f8577841cd90e316d05e6beaff7114c";

#[test]
fn a_call_is_named_by_its_function_its_arity_and_its_argument_order() {
    let base = all_nodes(ALL_NODES).law_id(&LOL_SEMANTICS_ID);
    for (what, expr) in [
        (
            "arity: max of 2 instead of 3",
            "-a + b*k - max(a, b)/sqrt(c)^2 + atan2(b, a) + 0.5",
        ),
        (
            "argument order",
            "-a + b*k - max(a, b, c)/sqrt(c)^2 + atan2(a, b) + 0.5",
        ),
        (
            "function name",
            "-a + b*k - min(a, b, c)/sqrt(c)^2 + atan2(b, a) + 0.5",
        ),
        (
            "negation dropped",
            "a + b*k - max(a, b, c)/sqrt(c)^2 + atan2(b, a) + 0.5",
        ),
        (
            "exponent",
            "-a + b*k - max(a, b, c)/sqrt(c)^3 + atan2(b, a) + 0.5",
        ),
    ] {
        assert_ne!(
            all_nodes(expr).law_id(&LOL_SEMANTICS_ID),
            base,
            "{what} did not change the identifier"
        );
    }
}

#[test]
fn the_identifier_is_the_specified_serialization() {
    assert_eq!(id(&gas()), expected_gas_id(&LOL_SEMANTICS_ID));
}

#[test]
fn the_recorded_value_for_the_example_law_does_not_drift() {
    // recorded once from the specification above (expected_gas_id), never edited to make a
    // test pass: a change here is a change of every research law's identifier
    let hex = gas()
        .law_id(&recorded_semantics())
        .iter()
        .fold(String::new(), |mut s, b| {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
            s
        });
    assert_eq!(hex, RECORDED_GAS_ID);
}

const RECORDED_GAS_ID: &str = "3eb9778f5f2a76e333aba1b18534b455a4ad6d506e0bb1e95fccadd39aac21fa";

#[test]
fn spelling_that_does_not_change_the_tree_does_not_change_the_identifier() {
    let spaced = law("ideal gas", " n * R*T /  V ", R, 0.0, "m^3", (1.0, 2000.0));
    let parens = law("ideal gas", "((n*R)*T)/V", R, 0.0, "m^3", (1.0, 2000.0));
    assert_eq!(id(&spaced), id(&gas()));
    assert_eq!(id(&parens), id(&gas()));
}

#[test]
fn each_part_of_the_claim_changes_the_identifier() {
    let base = id(&gas());
    let ulp_up = f64::from_bits(R.to_bits() + 1);
    let cases = [
        (
            "name",
            law("ideal gas 2", "n*R*T/V", R, 0.0, "m^3", (1.0, 2000.0)),
        ),
        (
            "operand order",
            law("ideal gas", "R*n*T/V", R, 0.0, "m^3", (1.0, 2000.0)),
        ),
        (
            "grouping",
            law("ideal gas", "n*(R*T)/V", R, 0.0, "m^3", (1.0, 2000.0)),
        ),
        (
            "parameter value, one ulp",
            law("ideal gas", "n*R*T/V", ulp_up, 0.0, "m^3", (1.0, 2000.0)),
        ),
        (
            "unit text",
            law("ideal gas", "n*R*T/V", R, 0.0, "L", (1.0, 2000.0)),
        ),
        (
            "range bound",
            law("ideal gas", "n*R*T/V", R, 0.0, "m^3", (1.0, 2001.0)),
        ),
    ];
    for (what, l) in cases {
        assert_ne!(id(&l), base, "{what} did not change the identifier");
    }
}

#[test]
fn literals_one_ulp_apart_are_different_laws() {
    // the same tree shape, so only the literal's bits can tell them apart
    let one = law("k", "n*R*T/V*1.0", R, 0.0, "m^3", (1.0, 2000.0));
    let next = law(
        "k",
        "n*R*T/V*1.0000000000000002",
        R,
        0.0,
        "m^3",
        (1.0, 2000.0),
    );
    assert_ne!(id(&one), id(&next));
}

#[test]
fn a_negative_zero_literal_is_a_different_law_from_a_positive_zero() {
    // the zero sits in a dimensionless factor, so both laws are valid and evaluate alike
    let plus = law("z", "n*R*T/V*(1.0 + 0.0)", R, 0.0, "m^3", (1.0, 2000.0));
    let minus = law("z", "n*R*T/V*(1.0 + -0.0)", R, 0.0, "m^3", (1.0, 2000.0));
    assert_ne!(id(&plus), id(&minus));
}

#[test]
fn input_order_is_part_of_the_claim() {
    let swapped = ResearchLaw::new(
        "ideal gas",
        "n*R*T/V",
        Var::new("P", "Pa"),
        &[
            Var::new("T", "K"),
            Var::new("n", "mol"),
            Var::new("V", "m^3"),
        ],
        &[Param::new("R", R, 0.0, "J/(mol*K)")],
        &[(
            "T",
            ValidRange {
                lo: 1.0,
                hi: 2000.0,
            },
        )],
        Provenance::new("closed form", "stated"),
    )
    .unwrap();
    assert_ne!(id(&swapped), id(&gas()));
}

#[test]
fn support_for_the_claim_does_not_change_the_identifier() {
    let base = id(&gas());
    let with_evidence = gas()
        .with_evidence(&[Observation::new(
            &[("n", 1.0), ("T", 300.0), ("V", 0.0224)],
            111_000.0,
        )])
        .unwrap();
    let with_oracle = gas().with_oracle(ResearchOracle::new(
        &[("n", 1.0), ("T", 273.15), ("V", 0.022_414)],
        101_325.0,
        50.0,
        "STP",
    ));
    let other_provenance = ResearchLaw::new(
        "ideal gas",
        "n*R*T/V",
        Var::new("P", "Pa"),
        &[
            Var::new("n", "mol"),
            Var::new("T", "K"),
            Var::new("V", "m^3"),
        ],
        &[Param::new("R", R, 0.0, "J/(mol*K)")],
        &[(
            "T",
            ValidRange {
                lo: 1.0,
                hi: 2000.0,
            },
        )],
        Provenance::new("a textbook", "measured"),
    )
    .unwrap();
    assert_eq!(
        id(&with_evidence),
        base,
        "evidence (and the residual it sets)"
    );
    assert_eq!(id(&with_oracle), base, "an oracle case");
    assert_eq!(id(&other_provenance), base, "provenance");
}

/// A parameter's uncertainty is not part of the identifier because evaluation does not
/// read it: two laws that differ only in it evaluate identically. This test is the
/// tripwire: if a future code path makes evaluation read the uncertainty, the first
/// assertion fails, and the uncertainty must then move into the identifier.
#[test]
fn a_parameter_uncertainty_neither_changes_evaluation_nor_the_identifier() {
    let certain = gas();
    let uncertain = law("ideal gas", "n*R*T/V", R, 0.5, "m^3", (1.0, 2000.0));
    let at = [("n", 2.0), ("T", 300.0), ("V", 0.05)];
    assert_eq!(
        certain.evaluate(&at).unwrap().to_bits(),
        uncertain.evaluate(&at).unwrap().to_bits(),
        "evaluation now depends on the uncertainty: move it into law_id"
    );
    assert_eq!(id(&certain), id(&uncertain));
}

#[test]
fn units_are_compared_as_written() {
    // "m^3" and "m*m*m" denote the same unit, but the identifier reads the unit text as
    // written until a unit-parsing pin exists in LOL_SEMANTICS_PINS
    let cubed = law("ideal gas", "n*R*T/V", R, 0.0, "m*m*m", (1.0, 2000.0));
    let at = [("n", 2.0), ("T", 300.0), ("V", 0.05)];
    assert_eq!(
        cubed.evaluate(&at).unwrap().to_bits(),
        gas().evaluate(&at).unwrap().to_bits()
    );
    assert_ne!(id(&cubed), id(&gas()));
}

#[test]
fn the_arithmetic_generation_is_part_of_the_name() {
    let other = [0x5a_u8; 32];
    assert_ne!(gas().law_id(&other), id(&gas()));
    assert_eq!(gas().law_id(&other), expected_gas_id(&other));
}

#[test]
fn a_research_law_and_an_audit_law_never_share_an_identifier_kind() {
    assert_ne!(RESEARCH_LAW_KIND, alice_lol::law_id::AUDIT_LAW_KIND);
    // the same leading fields under the two kinds hash differently
    let research = LawIdHasher::new(RESEARCH_LAW_KIND, &LOL_SEMANTICS_ID)
        .str("x")
        .finish();
    let audit = LawIdHasher::new(alice_lol::law_id::AUDIT_LAW_KIND, &LOL_SEMANTICS_ID)
        .str("x")
        .finish();
    assert_ne!(research, audit);
}
