//! Every law file under `laws/spike/` is read with the existing parsers.
//!
//! A law file is line based (`#` starts a comment). Each line starts with a
//! keyword; the keyword decides which existing parser checks it:
//!
//! | keyword | content | checked by |
//! |---------|---------|------------|
//! | `law <name>` | law name | — |
//! | `kind research\|audit` | — | — |
//! | `claim <text>` / `verdict <text>` / `source <text>` | prose | — |
//! | `input <name> <unit> [range <lo> <hi>]` | input with unit and valid range | `Unit::parse`, `ValidRange` |
//! | `param <name> <value> <uncertainty> <unit>` | constant | `Param`, `Unit::parse` |
//! | `let <name> <unit> = <expr>` | named sub-expression | `LawExpr::parse`, dimensions via `ResearchLaw::new` |
//! | `output <name> <unit> = <expr>` | the law | same |
//! | `tolerance <output> <unit> = <expr>` | accepted absolute deviation | same |
//! | `oracle <name> k=v ... -> <value> within <tol>` | reference value | `ResearchOracle` + `check_oracles` |
//! | `begin audit` ... `end audit` | audit clauses | `runtime_parser::parse_law` |
//! | `x-...` | constructs the parsers above do not have (extensions) | see below |
//!
//! Extensions are checked as far as the existing parsers allow: an `x-expr`
//! must be rejected by `LawExpr::parse` with an unknown function or at a `,`
//! (a function of several arguments; otherwise it would not be an extension), the expression of `x-ode`, `x-initial` and
//! `x-piece` goes through `LawExpr::parse` and the dimension check, and the
//! names used by `x-list`, `x-integer` and `x-range` must be declared.
//! Free-text extensions (`x-invariant`, `x-reduce`, `x-metric`, …) are only
//! counted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use alice_lol::research_law::{
    LawExpr, Param, Provenance, ResearchLaw, ResearchLawError, ResearchOracle, Unit, ValidRange,
    Var,
};
use alice_lol::runtime_parser::parse_law;

fn law_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../laws/spike")
}

#[derive(Default)]
struct Counts {
    exprs: usize,
    units: usize,
    oracles: usize,
    audits: usize,
    x_expr_rejected: usize,
    extensions: BTreeMap<String, usize>,
}

struct Input {
    name: String,
    unit: String,
    range: Option<ValidRange>,
}

#[derive(Default)]
struct Env {
    inputs: Vec<Input>,
    states: Vec<(String, String)>,
    params: Vec<Param>,
    /// let name -> (unit, expanded expression)
    lets: BTreeMap<String, (String, String)>,
    /// output / piece name -> unit
    outputs: BTreeMap<String, String>,
    /// output name -> expanded expression (for oracles)
    exprs: BTreeMap<String, (String, String)>,
}

/// Replaces identifiers that name a `let` with the parenthesised expansion
fn expand(expr: &str, lets: &BTreeMap<String, (String, String)>) -> String {
    let b = expr.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_digit() || (c == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                i += 1;
                if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
                    i += 1;
                }
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push_str(&expr[start..i]);
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let id = &expr[start..i];
            match lets.get(id) {
                Some((_, e)) => {
                    out.push('(');
                    out.push_str(e);
                    out.push(')');
                }
                None => out.push_str(id),
            }
        } else {
            out.push(c as char);
            i += 1;
        }
    }
    out
}

impl Env {
    /// Builds a research law `__out = expr` and so checks names, units and
    /// dimensions; `extra` are additional input variables, `range_override`
    /// replaces the range of one input
    fn law(
        &self,
        file: &str,
        expr: &str,
        unit: &str,
        extra: &[(String, String)],
        range_override: Option<(&str, ValidRange)>,
    ) -> ResearchLaw {
        let mut vars: Vec<Var> = self
            .inputs
            .iter()
            .map(|v| Var::new(&v.name, &v.unit))
            .collect();
        vars.extend(self.states.iter().map(|(n, u)| Var::new(n, u)));
        vars.extend(extra.iter().map(|(n, u)| Var::new(n, u)));
        let mut ranges: Vec<(&str, ValidRange)> = self
            .inputs
            .iter()
            .filter_map(|v| v.range.map(|r| (v.name.as_str(), r)))
            .filter(|(n, _)| range_override.is_none_or(|(o, _)| o != *n))
            .collect();
        if let Some(r) = range_override {
            ranges.push(r);
        }
        ResearchLaw::new(
            file,
            expr,
            Var::new("__out", unit),
            &vars,
            &self.params,
            &ranges,
            Provenance::new(file, "spike law file"),
        )
        .unwrap_or_else(|e| panic!("{file}: `{expr}` [{unit}] does not build: {e}"))
    }

    fn known(&self, name: &str) -> bool {
        self.lets.contains_key(name)
            || self.inputs.iter().any(|v| v.name == name)
            || self.outputs.contains_key(name)
    }
}

/// `<name> <unit> = <expr>`
fn split_def(rest: &str) -> (String, String, String) {
    let (lhs, expr) = rest.split_once('=').expect("`<name> <unit> = <expr>`");
    let mut w = lhs.split_whitespace();
    let name = w.next().expect("name").to_owned();
    let unit = w.collect::<Vec<_>>().join(" ");
    (name, unit, expr.trim().to_owned())
}

/// State of one file while its lines are checked
struct FileCheck<'a> {
    file: String,
    counts: &'a mut Counts,
    env: Env,
    law_name: Option<String>,
    audit: Option<String>,
    audits_here: usize,
    checked_here: usize,
    x_names: Vec<String>,
}

impl FileCheck<'_> {
    /// Collects the lines of an audit block; returns false outside a block
    fn audit_line(&mut self, line: &str) -> bool {
        let file = &self.file;
        let Some(block) = self.audit.as_mut() else {
            return false;
        };
        if line == "end audit" {
            let law = parse_law(block)
                .unwrap_or_else(|e| panic!("{file}: audit block does not parse: {e:?}"));
            let expected = self.law_name.as_deref().unwrap().replace('_', "-");
            assert_eq!(law.name(), expected, "{file}: audit name");
            assert!(!law.clauses().is_empty());
            self.counts.audits += 1;
            self.audits_here += 1;
            self.audit = None;
        } else {
            block.push_str(line);
            block.push('\n');
        }
        true
    }

    fn line(&mut self, line: &str) {
        if self.audit_line(line) {
            return;
        }
        let file = self.file.clone();
        let (head, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        if head.starts_with("x-") {
            *self.counts.extensions.entry(head.to_owned()).or_default() += 1;
        }
        match head {
            "law" => self.law_name = Some(rest.to_owned()),
            "kind" => assert!(rest == "research" || rest == "audit", "{file}: kind"),
            // prose lines and free-text extensions are only counted
            "claim" | "verdict" | "source" | "x-method" | "x-step" | "x-output" | "x-reduce"
            | "x-invariant" | "x-metric" | "x-at-least" | "x-input" => {}
            "input" => self.input(rest, line),
            "param" => self.param(rest),
            "let" | "output" | "x-piece" | "tolerance" | "x-ode" | "x-initial" => {
                self.definition(head, rest);
            }
            "oracle" => self.oracle(rest),
            "begin" => {
                assert_eq!(rest, "audit", "{file}: `begin audit`");
                self.audit = Some(String::new());
            }
            "x-state" => {
                let w: Vec<&str> = rest.split_whitespace().collect();
                Unit::parse(w[1]).unwrap_or_else(|e| panic!("{file}: unit `{}`: {e}", w[1]));
                self.counts.units += 1;
                self.env.states.push((w[0].to_owned(), w[1].to_owned()));
            }
            "x-expr" => self.x_expr(rest),
            "x-list" | "x-integer" => {
                assert!(
                    self.env.inputs.iter().any(|v| v.name == rest),
                    "{file}: {head} {rest}"
                );
            }
            "x-range" => {
                let name = rest.split_whitespace().next().unwrap();
                assert!(
                    self.env.lets.contains_key(name) || self.x_names.iter().any(|n| n == name),
                    "{file}: x-range of undeclared {name}"
                );
            }
            "x-periodic" => {
                let name = rest.split_whitespace().next().unwrap();
                assert!(self.x_names.iter().any(|n| n == name) || self.env.known(name));
            }
            other => panic!("{file}: unknown keyword `{other}` in `{line}`"),
        }
    }

    /// `input <name> <unit> [range <lo> <hi>]`
    fn input(&mut self, rest: &str, line: &str) {
        let file = &self.file;
        let w: Vec<&str> = rest.split_whitespace().collect();
        Unit::parse(w[1]).unwrap_or_else(|e| panic!("{file}: unit `{}`: {e}", w[1]));
        self.counts.units += 1;
        let range = match w.len() {
            2 => None,
            5 if w[2] == "range" => Some(ValidRange {
                lo: w[3].parse().unwrap(),
                hi: w[4].parse().unwrap(),
            }),
            _ => panic!("{file}: `input <name> <unit> [range <lo> <hi>]`: {line}"),
        };
        self.env.inputs.push(Input {
            name: w[0].to_owned(),
            unit: w[1].to_owned(),
            range,
        });
    }

    /// `param <name> <value> <uncertainty> <unit>`
    fn param(&mut self, rest: &str) {
        let file = &self.file;
        let w: Vec<&str> = rest.split_whitespace().collect();
        assert_eq!(w.len(), 4, "{file}: `param <name> <value> <unc> <unit>`");
        Unit::parse(w[3]).unwrap_or_else(|e| panic!("{file}: unit `{}`: {e}", w[3]));
        self.counts.units += 1;
        self.env.params.push(Param::new(
            w[0],
            w[1].parse().unwrap(),
            w[2].parse().unwrap(),
            w[3],
        ));
    }

    /// The unit of a definition and, for `x-piece`, the input range it covers
    fn def_unit(
        &self,
        head: &str,
        name: &str,
        unit: String,
    ) -> (String, Option<(String, ValidRange)>) {
        let file = &self.file;
        match head {
            "x-piece" => {
                // `<out> <unit> <input> <lo> <hi>`
                let w: Vec<&str> = unit.split_whitespace().collect();
                assert_eq!(w.len(), 4, "{file}: x-piece header");
                let r = ValidRange {
                    lo: w[2].parse().unwrap(),
                    hi: w[3].parse().unwrap(),
                };
                (w[0].to_owned(), Some((w[1].to_owned(), r)))
            }
            "x-initial" => (self.state_unit(name, head).to_owned(), None),
            _ => (unit, None),
        }
    }

    fn state_unit(&self, name: &str, head: &str) -> &str {
        let file = &self.file;
        &self
            .env
            .states
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("{file}: {head} of unknown state {name}"))
            .1
    }

    /// `let` / `output` / `x-piece` / `tolerance` / `x-ode` / `x-initial`
    fn definition(&mut self, head: &str, rest: &str) {
        let file = self.file.clone();
        let (name, unit, expr) = split_def(rest);
        LawExpr::parse(&expr).unwrap_or_else(|e| panic!("{file}: `{expr}`: {e}"));
        let full = expand(&expr, &self.env.lets);
        let (unit, range) = self.def_unit(head, &name, unit);
        Unit::parse(&unit).unwrap_or_else(|e| panic!("{file}: unit `{unit}`: {e}"));
        self.counts.units += 1;
        // a tolerance may refer to its output; an output that is also a
        // simulated state is already a variable
        let extra: Vec<(String, String)> =
            if head == "tolerance" && !self.env.states.iter().any(|(n, _)| *n == name) {
                vec![(name.clone(), self.env.outputs[&name].clone())]
            } else {
                Vec::new()
            };
        // number literals are dimensionless in `LawExpr`, so `x-initial v = 0`
        // cannot be checked against a dimensioned state; a bare `0` start
        // value is accepted for any state (part of the extension)
        if head == "x-initial" && expr == "0" {
            *self
                .counts
                .extensions
                .entry("x-initial-zero".to_owned())
                .or_default() += 1;
            return;
        }
        let _ = self.env.law(
            &file,
            &full,
            &unit,
            &extra,
            range.as_ref().map(|(n, r)| (n.as_str(), *r)),
        );
        if head == "x-ode" {
            // d(state)/dt has the state's dimension over time
            let su = self.state_unit(&name, head);
            let rate = Unit::parse(&format!("({su})/s")).unwrap();
            assert_eq!(
                rate.dimension(),
                Unit::parse(&unit).unwrap().dimension(),
                "{file}: d{name}/dt unit"
            );
        }
        self.counts.exprs += 1;
        self.checked_here += 1;
        match head {
            "let" => {
                self.env.lets.insert(name, (unit, full));
            }
            "output" | "x-piece" => {
                self.env.outputs.insert(name.clone(), unit.clone());
                self.env.exprs.insert(name, (unit, full));
            }
            _ => {}
        }
    }

    /// `oracle <name> k=v ... -> <value> within <tol>`
    fn oracle(&mut self, rest: &str) {
        let file = self.file.clone();
        let (lhs, rhs) = rest.split_once("->").expect("`->`");
        let mut w = lhs.split_whitespace();
        let name = w.next().unwrap();
        let conds: Vec<(String, f64)> = w
            .map(|kv| {
                let (k, v) = kv.split_once('=').unwrap();
                (k.to_owned(), v.parse().unwrap())
            })
            .collect();
        let r: Vec<&str> = rhs.split_whitespace().collect();
        assert_eq!(r[1], "within");
        let (unit, expr) = self
            .env
            .exprs
            .get(name)
            .or_else(|| self.env.lets.get(name))
            .unwrap_or_else(|| panic!("{file}: oracle of unknown {name}"))
            .clone();
        let borrowed: Vec<(&str, f64)> = conds.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        // the closed form does not depend on simulated states, and
        // evaluation needs a value for every declared variable
        let states = std::mem::take(&mut self.env.states);
        let law = self.env.law(&file, &expr, &unit, &[], None);
        self.env.states = states;
        let law = law.with_oracle(ResearchOracle::new(
            &borrowed,
            r[0].parse().unwrap(),
            r[2].parse().unwrap(),
            &file,
        ));
        let out = law.check_oracles();
        assert!(out[0].passed, "{file}: oracle {name}: {:?}", out[0]);
        self.counts.oracles += 1;
        self.checked_here += 1;
    }

    /// `x-expr <name> <unit> = <expr>`: must need a function `LawExpr` lacks
    fn x_expr(&mut self, rest: &str) {
        let file = &self.file;
        let (name, unit, expr) = split_def(rest);
        Unit::parse(&unit).unwrap();
        match LawExpr::parse(&expr) {
            Err(ResearchLawError::UnknownFunction(_)) => self.counts.x_expr_rejected += 1,
            // functions of several arguments: `,` is not in the expression language
            Err(ResearchLawError::UnexpectedCharacter { position })
                if expr.as_bytes().get(position) == Some(&b',') =>
            {
                self.counts.x_expr_rejected += 1;
            }
            other => panic!(
                "{file}: x-expr `{expr}` should need a function LawExpr lacks, got {other:?}"
            ),
        }
        self.env.outputs.insert(name.clone(), unit);
        self.x_names.push(name);
    }
}

fn check_file(path: &Path, counts: &mut Counts) {
    let file = path.file_name().unwrap().to_string_lossy().into_owned();
    let text = std::fs::read_to_string(path).unwrap();
    let mut fc = FileCheck {
        file: file.clone(),
        counts,
        env: Env::default(),
        law_name: None,
        audit: None,
        audits_here: 0,
        checked_here: 0,
        x_names: Vec::new(),
    };
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if !line.is_empty() {
            fc.line(line);
        }
    }
    assert!(fc.audit.is_none(), "{file}: unterminated audit block");
    let name = fc
        .law_name
        .unwrap_or_else(|| panic!("{file}: no `law` line"));
    assert_eq!(
        format!("{name}.law"),
        file,
        "{file}: law name and file name differ"
    );
    assert!(
        fc.checked_here + fc.audits_here > 0,
        "{file}: nothing in the file went through a parser"
    );
}

#[test]
fn every_spike_law_file_parses_with_the_existing_parsers() {
    let mut files: Vec<PathBuf> = std::fs::read_dir(law_dir())
        .expect("laws/spike exists")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "law"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .law files found (compared nothing)");
    let mut counts = Counts::default();
    for f in &files {
        check_file(f, &mut counts);
    }
    eprintln!(
        "{} files: {} expressions, {} units, {} oracles, {} audit blocks, {} x-expr rejected",
        files.len(),
        counts.exprs,
        counts.units,
        counts.oracles,
        counts.audits,
        counts.x_expr_rejected
    );
    eprintln!("extensions used: {:?}", counts.extensions);
    assert!(counts.exprs > 0 && counts.units > 0 && counts.audits > 0 && counts.oracles > 0);
}

/// The checker has teeth: a broken dimension, unit, audit or extension fails
#[test]
fn the_checker_rejects_broken_law_files() {
    let dir = std::env::temp_dir().join(format!("spike-law-neg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let base = "law neg\nkind research\ninput t s range 0 1\ninput k 1/s\n";
    let cases = [
        "output c 1 = k*t + t\n",                               // dimension mismatch
        "output c furlong = k*t\n",                             // unknown unit
        "output c 1 = k*t\nx-expr q 1 = k*t\n", // x-expr that parses is not an extension
        "output c 1 = k*t\noracle c t=1 k=2 -> 3 within 0.5\n", // oracle off by 1
        "begin audit\naudit neg\nexpect a = 0\nend audit\n", // audit syntax
        "output c 1 = k*t\nx-ode c 1/s = k\n",  // ode of undeclared state
        "output c 1 = k*t\nfrobnicate\n",       // unknown keyword
    ];
    for (i, case) in cases.iter().enumerate() {
        let p = dir.join("neg.law");
        std::fs::write(&p, format!("{base}{case}")).unwrap();
        let r = std::panic::catch_unwind(|| check_file(&p, &mut Counts::default()));
        assert!(r.is_err(), "case {i} was accepted: {case}");
    }
    let p = dir.join("neg.law");
    std::fs::write(
        &p,
        format!("{base}output c 1 = k*t\noracle c t=1 k=2 -> 2 within 1e-12\n"),
    )
    .unwrap();
    check_file(&p, &mut Counts::default());
    std::fs::remove_dir_all(&dir).unwrap();
}
