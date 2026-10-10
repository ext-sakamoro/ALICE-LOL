//! A quantitative law is evaluated strictly: every intermediate value is finite
//!
//! `conformance/TASK.md`: when an intermediate value of a quantitative law's expression is
//! not finite (an overflow, a domain error, a division by zero), the request is rejected,
//! even when IEEE arithmetic would carry the value on to a finite result (`1/exp(1000)`
//! would be 0). `research_law` checks every operation ([`ResearchLawError::NonFinite`]).
//!
//! The law `laws/spike/finite_evaluation_probe.law` (`y = 1/exp(1000 sin(x))`) is read from
//! its file and checked against the probes of `conformance/probes.json` for it (each a
//! rejection) and against the values the reference implementation gives in its range.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use alice_lol::law_input::{parse_json, Json};
use alice_lol::research_law::{LawExpr, Param, ResearchLaw, ResearchLawError, Var};
use alice_zip::law::{Provenance, ValidRange};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// the probe law from its file: `input`, `let` and `output` lines (the `let` written into
/// the output, as `ResearchLaw` takes one expression)
fn probe_law() -> ResearchLaw {
    let text =
        std::fs::read_to_string(root().join("laws/spike/finite_evaluation_probe.law")).unwrap();
    let mut input: Option<(String, f64, f64)> = None;
    let mut lets: BTreeMap<String, String> = BTreeMap::new();
    let mut output: Option<String> = None;
    for line in text.lines().map(|l| l.split('#').next().unwrap().trim()) {
        let mut w = line.split_whitespace();
        match w.next() {
            Some("input") => {
                let v: Vec<&str> = w.collect();
                input = Some((
                    v[0].to_owned(),
                    v[3].parse::<f64>().unwrap(),
                    v[4].parse::<f64>().unwrap(),
                ));
            }
            Some(kw @ ("let" | "output")) => {
                let (lhs, expr) = line.split_once('=').unwrap();
                let name = lhs.split_whitespace().nth(1).unwrap().to_owned();
                let mut e = expr.trim().to_owned();
                for (n, def) in &lets {
                    e = e.replace(n.as_str(), &format!("({def})"));
                }
                if kw == "let" {
                    lets.insert(name, e);
                } else {
                    output = Some(e);
                }
            }
            _ => {}
        }
    }
    let (name, lo, hi) = input.unwrap();
    let no_params: [Param; 0] = [];
    ResearchLaw::new(
        "finite_evaluation_probe",
        &output.unwrap(),
        Var::new("y", "1"),
        &[Var::new(&name, "1")],
        &no_params,
        &[(name.as_str(), ValidRange { lo, hi })],
        Provenance::new("laws/spike/finite_evaluation_probe.law", "spike law file"),
    )
    .unwrap()
}

fn field<'a>(o: &'a Json, key: &str) -> Option<&'a Json> {
    match o {
        Json::Object(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

#[test]
fn the_probes_of_the_law_are_rejections() {
    let law = probe_law();
    let probes =
        parse_json(&std::fs::read_to_string(root().join("conformance/probes.json")).unwrap())
            .unwrap();
    let Json::Array(probes) = probes else {
        panic!("probes.json is not an array")
    };
    let mut checked = 0;
    for p in &probes {
        if !matches!(field(p, "law"), Some(Json::Text(n)) if n == "finite_evaluation_probe") {
            continue;
        }
        assert!(matches!(field(p, "expect"), Some(Json::Text(e)) if e == "rejected"));
        let Some(Json::Number(x)) = field(field(p, "inputs").unwrap(), "x") else {
            panic!("x")
        };
        assert_eq!(
            law.evaluate(&[("x", *x)]),
            Err(ResearchLawError::NonFinite),
            "x = {x}"
        );
        checked += 1;
    }
    assert!(checked >= 2, "only {checked} probes of the law");
}

#[test]
fn inside_the_finite_part_of_the_range_the_value_agrees_with_the_reference() {
    let law = probe_law();
    // the reference implementation's answers (`conformance/ref_impl.py`, Python's math)
    for (x, want) in [
        (0.0, 1.0),
        (0.5, 6.139_515_507_992_199e-209),
        (2.6, 1.320_075_602_001_890_7e-224),
    ] {
        let got = law.evaluate(&[("x", x)]).unwrap();
        // the law's tolerance: 1e-9 relative
        assert!(
            (got - want).abs() <= 1e-9 * want,
            "x = {x}: {got:e} vs {want:e}"
        );
    }
    // outside the range: out of range, not non-finite
    assert!(matches!(
        law.evaluate(&[("x", 3.2)]),
        Err(ResearchLawError::OutOfRange { .. })
    ));
}

#[test]
fn each_kind_of_non_finite_intermediate_is_refused() {
    for expr in [
        "exp(1000)",
        "10^400",
        "1e200*1e200",
        "1.7e308+1.7e308",
        "ln(-1)",
        "sqrt(-1)",
        "1/0",
        "0^(-1)",
        "1/exp(1000)",
        "exp(1000) - exp(1000)",
    ] {
        let e = LawExpr::parse(expr).unwrap();
        assert_eq!(
            e.evaluate_constant(),
            Err(ResearchLawError::NonFinite),
            "{expr}"
        );
    }
    assert!(
        LawExpr::parse("1/exp(700)")
            .unwrap()
            .evaluate_constant()
            .unwrap()
            > 0.0
    );
}
