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

/// an `input` line: name, unit and range (if any)
type Input = (String, String, Option<(f64, f64)>);

/// `expr` with each identifier that names a `let` replaced by its parenthesised definition
/// (whole identifiers only: `d` is replaced in `d^2`, not in `dx`)
fn expand(expr: &str, lets: &BTreeMap<String, String>) -> String {
    let bytes = expr.as_bytes();
    let (mut out, mut at) = (String::new(), 0);
    while at < bytes.len() {
        if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            let start = at;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
            let id = &expr[start..at];
            match lets.get(id) {
                Some(def) => {
                    out.push('(');
                    out.push_str(def);
                    out.push(')');
                }
                None => out.push_str(id),
            }
        } else {
            out.push(char::from(bytes[at]));
            at += 1;
        }
    }
    out
}

/// a quantitative law from its file under `laws/spike/`: `input` lines (name, unit, range),
/// `let` and `output` lines (each `let` written into the expressions after it, as
/// `ResearchLaw` takes one expression); other lines are not needed here
fn law_from_file(name: &str) -> ResearchLaw {
    let path = format!("laws/spike/{name}.law");
    let text = std::fs::read_to_string(root().join(&path)).unwrap();
    let mut inputs: Vec<Input> = Vec::new();
    let mut lets: BTreeMap<String, String> = BTreeMap::new();
    let mut output: Option<(String, String)> = None;
    for line in text.lines().map(|l| l.split('#').next().unwrap().trim()) {
        let mut w = line.split_whitespace();
        match w.next() {
            Some("input") => {
                let v: Vec<&str> = w.collect();
                let range = (v.get(2) == Some(&"range"))
                    .then(|| (v[3].parse::<f64>().unwrap(), v[4].parse::<f64>().unwrap()));
                inputs.push((v[0].to_owned(), v[1].to_owned(), range));
            }
            Some(kw @ ("let" | "output")) => {
                let (lhs, expr) = line.split_once('=').unwrap();
                let head: Vec<&str> = lhs.split_whitespace().collect();
                let e = expand(expr.trim(), &lets);
                if kw == "let" {
                    lets.insert(head[1].to_owned(), e);
                } else {
                    output = Some((head[2].to_owned(), e));
                }
            }
            _ => {}
        }
    }
    let (unit, expr) = output.unwrap();
    let vars: Vec<Var> = inputs.iter().map(|(n, u, _)| Var::new(n, u)).collect();
    let ranges: Vec<(&str, ValidRange)> = inputs
        .iter()
        .filter_map(|(n, _, r)| r.map(|(lo, hi)| (n.as_str(), ValidRange { lo, hi })))
        .collect();
    let no_params: [Param; 0] = [];
    ResearchLaw::new(
        name,
        &expr,
        Var::new("out", &unit),
        &vars,
        &no_params,
        &ranges,
        Provenance::new(&path, "spike law file"),
    )
    .unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn probe_law() -> ResearchLaw {
    law_from_file("finite_evaluation_probe")
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

/// `four_bar_rocker_angle` at a Grashof margin of about 1e-13: the radicand of `across` is -1.8e-12 in
/// double and +1.2e-12 exactly; there is no clamping, so the evaluation is not finite
/// (`conformance/TASK.md`; the same vector is in `conformance/probes.json`)
#[test]
fn a_negative_radicand_by_rounding_is_not_clamped() {
    let law = law_from_file("four_bar_rocker_angle");
    let at = [
        ("lc", 1.793_999_999_999_898_8),
        ("lco", 94.027),
        ("lr", 5.407),
        ("lg", 90.414),
        ("theta2", 0.0),
    ];
    assert_eq!(law.evaluate(&at), Err(ResearchLawError::NonFinite));
    // the source linkage evaluates (the reference implementation gives the same angle)
    let ok = [
        ("lc", 1.0),
        ("lco", 2.0),
        ("lr", 1.5),
        ("lg", 2.3),
        ("theta2", 1.0),
    ];
    // the reference implementation's angle (`conformance/ref_impl.py`); the law's tolerance
    // is 0.01, and the two compute the same expression, so they agree far closer
    let got = law.evaluate(&ok).unwrap();
    assert!((got - 1.483_512_684_293_831_3).abs() <= 1e-9, "{got}");
}

/// within 41 ulp of both crossings of `1000 sin(x)` over the overflow of `exp`, the verdict is
/// the correctly rounded one (`conformance/finite_probe_crossings.json`, written by the
/// reference implementation with correctly rounded `sin` / `exp`; alice-det-math is correctly
/// rounded, so the two agree at every point)
#[test]
fn the_verdict_at_the_crossings_agrees_with_the_reference() {
    let law = probe_law();
    let doc = parse_json(
        &std::fs::read_to_string(root().join("conformance/finite_probe_crossings.json")).unwrap(),
    )
    .unwrap();
    let Some(Json::Array(points)) = field(&doc, "points") else {
        panic!("points")
    };
    assert_eq!(points.len(), 166);
    let mut rejected = 0;
    for row in points {
        let Some(Json::Number(x)) = field(row, "x") else {
            panic!("x")
        };
        let Some(Json::Bool(want)) = field(row, "rejected") else {
            panic!("rejected")
        };
        let got = law.evaluate(&[("x", *x)]);
        assert_eq!(
            got == Err(ResearchLawError::NonFinite),
            *want,
            "x = {x:e}: {got:?}"
        );
        rejected += usize::from(*want);
    }
    // both sides of both crossings are in the grid
    assert!(
        rejected > 0 && rejected < points.len(),
        "{rejected} of {}",
        points.len()
    );
}
