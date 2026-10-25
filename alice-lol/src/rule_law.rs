//! Rule laws — a rule of a product written as a law file.
//!
//! A rule law is a state machine (`kind machine`), a predicate over typed inputs
//! (`kind predicate`) or a table whose first holding row gives the answer (`kind table`).
//!
//! The layout and the meaning are those of `conformance/TASK.md` § "Rule laws"; this module
//! is a reader of it ([`rule_law_from_file`]) and answers a request the way the contract says
//! ([`RuleLaw::respond`]). The corpus generator (`scripts/law_rules.py`) is a separate
//! implementation of the same contract, and `conformance/run_conformance.py --only-rule-laws`
//! scores `examples/rule_conformance.rs` (built on this module) on the corpus it generates,
//! so a defect on either side shows as a failing vector.
//!
//! # Identifier
//!
//! [`RuleLaw::law_id`] names the rule, not its layout: comments, the lines for people, the
//! spacing and the `example` of a predicate are not part of it; the declarations, the order
//! of the `on` lines and of the `require` lines, the labels and every expression are. It is
//! hashed with ALICE-Zip's law identifier v2 (`alice_zip::law_id::LawIdHasher`, the layout every
//! ALICE crate uses; `scripts/law_rules.py` computes it independently and
//! `alice-lol/tests/data/rule_law_ids_v2.txt` holds its values for the laws under `laws/`), with its
//! own kind tag ([`MACHINE_LAW_KIND`] / [`PREDICATE_LAW_KIND`]) under
//! [`RULE_SEMANTICS_ID`], the fold of [`RULE_SEMANTICS_PINS`], which are measured from the
//! behaviour of this module (`tests/rule_law_oracle.rs` recomputes them). A table law
//! ([`TABLE_LAW_KIND`]) is hashed under [`TABLE_SEMANTICS_ID`], the fold of
//! [`TABLE_SEMANTICS_PINS`] (the order of its rows, and [`RULE_SEMANTICS_ID`] for the
//! expressions and inputs it shares with a predicate), so adding it left the identifier of
//! every machine and predicate law as it was. ⚠️ The rule kinds do
//! not fold into [`crate::law_id::LOL_SEMANTICS_ID`]: adding them leaves the identifier of
//! every audit and research law, and every stamped text, as it was.
//!
//! # The call
//!
//! `hmac_sha256` is alice-sign's, behind the `crypto-calls` feature (off by default). A law
//! that calls it reads, and has its identifier, in every build; answering it without the
//! feature gives [`Answer::Unsupported`], and its `example` is checked only where the call is
//! built.

use std::fmt;

use alice_zip::law_id::LawIdHasher;

use crate::law_id::hex32;
use crate::law_input::Json;

/// The kind tag of a machine law
pub const MACHINE_LAW_KIND: &[u8] = b"lol.law.machine/v1";

/// The kind tag of a predicate law
pub const PREDICATE_LAW_KIND: &[u8] = b"lol.law.predicate/v1";

/// The kind tag of a table law
pub const TABLE_LAW_KIND: &[u8] = b"lol.law.table/v1";

/// The largest integer magnitude a rule law reads (2^53 - 1: every JSON reader holds it exactly)
pub const MAX_INTEGER: i64 = (1 << 53) - 1;

/// The semantics of the rule kinds `(name, fingerprint of that behaviour)`
///
/// ⚠️ Each value is measured ([`rule_semantics_fingerprints`]); the oracle recomputes it. A
/// change of behaviour moves a pin, then [`RULE_SEMANTICS_ID`] (record the pin first, then
/// the fold, as for [`crate::law_id::LOL_SEMANTICS_PINS`]).
pub const RULE_SEMANTICS_PINS: &[(&str, [u8; 32])] = &[
    (
        "rule machine: first matching transition, refused keeps the state, allowances after the step",
        hex32("26ce440741455802f781e81ecc98130971b84dc974d341a22b4a5090f63029a7"),
    ),
    (
        "rule predicate: shape first, requires in file order, first failure reported",
        hex32("a8985981d768cf2513566cb3b29638d47aa64fc73275ac33419911d5e8162c39"),
    ),
    (
        "rule inputs: integers up to 2^53-1, lower-case even hex, whole records",
        hex32("2a4c68b5b7badf105ca00988dbf8cb89d121fa5c4fcf37b40cb1ef878444ddd8"),
    ),
    (
        "call: alice-sign hmac_sha256 (RFC 4231 cases 1-4)",
        hex32("f4b40043bca243afd16904ad3e673dee350e1dd864f53d3c6b36b22487a7c937"),
    ),
];

/// The fold of [`RULE_SEMANTICS_PINS`] (the oracle checks the two agree)
pub const RULE_SEMANTICS_ID: [u8; 32] =
    hex32("a1008789ce5000d7c259234df49065b9201427155956eb8de2e7b771730d5f2c");

/// The semantics of the table kind `(name, fingerprint of that behaviour)`
///
/// ⚠️ The first value is measured ([`table_semantics_fingerprints`]); the second is
/// [`RULE_SEMANTICS_ID`], the expressions and inputs a table shares with a predicate. The
/// oracle recomputes both and the fold.
pub const TABLE_SEMANTICS_PINS: &[(&str, [u8; 32])] = &[
    (
        "rule table: rows in file order, the first that holds gives its value and label, otherwise with row null",
        hex32("d1f2aedf5cfccda3f67a8b538ecaae2020618f3eccb9186cd000819ffe5f2bab"),
    ),
    (
        "rule expressions and inputs (RULE_SEMANTICS_ID)",
        RULE_SEMANTICS_ID,
    ),
];

/// The fold of [`TABLE_SEMANTICS_PINS`] (the oracle checks the two agree)
pub const TABLE_SEMANTICS_ID: [u8; 32] =
    hex32("ec2652c7436c40f93a923d23eaed410d45636e68a184cb3b1f9ebc9e6e16d3e2");

/// Which rule kind a law file is
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    /// `kind machine`
    Machine,
    /// `kind predicate`
    Predicate,
    /// `kind table`
    Table,
}

/// The kind of a rule law file: `None` when it has the line `kind audit` (an audit law,
/// whatever its other lines) or none of `kind machine`, `kind predicate` and `kind table`
#[must_use]
pub fn rule_kind(text: &str) -> Option<RuleKind> {
    let lines: Vec<&str> = text
        .split('\n')
        .map(|l| {
            l.split('#')
                .next()
                .unwrap_or("")
                .trim_matches([' ', '\t', '\r'])
        })
        .collect();
    if lines.contains(&"kind audit") {
        None
    } else if lines.contains(&"kind machine") {
        Some(RuleKind::Machine)
    } else if lines.contains(&"kind predicate") {
        Some(RuleKind::Predicate)
    } else if lines.contains(&"kind table") {
        Some(RuleKind::Table)
    } else {
        None
    }
}

/// Why a rule law file does not read
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleLawError {
    /// The line (1-based; 0 for the file as a whole)
    pub line: usize,
    /// What is wrong
    pub reason: String,
}

impl fmt::Display for RuleLawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "rule law: {}", self.reason)
        } else {
            write!(f, "rule law, line {}: {}", self.line, self.reason)
        }
    }
}

impl std::error::Error for RuleLawError {}

fn err(line: usize, reason: impl Into<String>) -> RuleLawError {
    RuleLawError {
        line,
        reason: reason.into(),
    }
}

/// The answer to one request (conformance/TASK.md "Program contract")
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// `{"outputs": ...}`
    Outputs(Json),
    /// `{"rejected": ...}`: an input that is not of its type or not declared
    Rejected(String),
    /// An error of the request (exit status 2): a missing input
    RequestError(String),
    /// The law calls a function this build does not have (`crypto-calls` off)
    Unsupported(String),
}

// ------------------------------------------------------------------ values and types

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ty {
    Integer,
    Time,
    Text,
    Bytes,
    Bool,
    Record(Vec<(String, Self)>),
}

impl Ty {
    fn scalar(word: &str) -> Option<Self> {
        Some(match word {
            "integer" => Self::Integer,
            "time" => Self::Time,
            "text" => Self::Text,
            "bytes" => Self::Bytes,
            "bool" => Self::Bool,
            _ => return None,
        })
    }

    const fn word(&self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Time => "time",
            Self::Text => "text",
            Self::Bytes => "bytes",
            Self::Bool => "bool",
            Self::Record(_) => "record",
        }
    }

    fn hash(&self, h: LawIdHasher) -> LawIdHasher {
        match self {
            Self::Record(fields) => {
                let mut h = h.str("record").u64(fields.len() as u64);
                for (name, ty) in fields {
                    h = ty.hash(h.str(name));
                }
                h
            }
            other => h.str(other.word()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Int(i64),
    Text(String),
    Bytes(Vec<u8>),
    Bool(bool),
    Record(Vec<(String, Self)>),
}

impl Value {
    fn hash(&self, h: LawIdHasher) -> LawIdHasher {
        match self {
            Self::Int(v) => h.str("int").u64(v.cast_unsigned()),
            Self::Text(t) => h.str("text").str(t),
            Self::Bytes(b) => h.str("bytes").bytes(b),
            Self::Bool(b) => h.str("bool").u32(u32::from(*b)),
            Self::Record(_) => h.str("record"),
        }
    }
}

/// [`MAX_INTEGER`] as a double (2^53 - 1 is exact in a double)
const MAX_INTEGER_F64: f64 = 9_007_199_254_740_991.0;

/// An integer of a JSON number: finite, integral, within [`MAX_INTEGER`]
#[allow(clippy::cast_possible_truncation)]
fn json_integer(v: &Json) -> Option<i64> {
    match v {
        Json::Number(x) if x.is_finite() && x.fract() == 0.0 && x.abs() <= MAX_INTEGER_F64 => {
            Some(*x as i64)
        }
        _ => None,
    }
}

fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(2) || !b.iter().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    let nib = |c: u8| {
        if c.is_ascii_digit() {
            c - b'0'
        } else {
            c - b'a' + 10
        }
    };
    Some(b.chunks(2).map(|p| nib(p[0]) << 4 | nib(p[1])).collect())
}

/// A JSON value read as a type (TASK.md "Types of values"); `None` when it does not have it
fn read_value(v: &Json, ty: &Ty) -> Option<Value> {
    match (ty, v) {
        (Ty::Integer | Ty::Time, _) => json_integer(v).map(Value::Int),
        (Ty::Text, Json::Text(t)) => Some(Value::Text(t.clone())),
        (Ty::Bytes, Json::Text(t)) => hex_bytes(t).map(Value::Bytes),
        (Ty::Bool, Json::Bool(b)) => Some(Value::Bool(*b)),
        (Ty::Record(fields), Json::Object(_)) => fields
            .iter()
            .map(|(f, t)| Some((f.clone(), read_value(v.get(f)?, t)?)))
            .collect::<Option<Vec<_>>>()
            .map(Value::Record),
        _ => None,
    }
}

/// `0` or `-`? `[1-9][0-9]*`, ASCII, within [`MAX_INTEGER`]
fn integer_text(s: &str) -> Option<i64> {
    let digits = s.strip_prefix('-').unwrap_or(s);
    let b = digits.as_bytes();
    let well_formed = !b.is_empty()
        && b.iter().all(u8::is_ascii_digit)
        && (b[0] != b'0' || (b.len() == 1 && digits.len() == s.len()));
    if !well_formed || b.len() > 16 {
        return None;
    }
    let v: i64 = s.parse().ok()?;
    (v.abs() <= MAX_INTEGER).then_some(v)
}

// ------------------------------------------------------------------ expressions

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Cmp {
    const fn word(self) -> &'static str {
        match self {
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Func {
    Len,
    Prefix,
    Utf8,
    HmacSha256,
}

const FUNCTIONS: &[(&str, Func)] = &[
    ("len", Func::Len),
    ("prefix", Func::Prefix),
    ("utf8", Func::Utf8),
    ("hmac_sha256", Func::HmacSha256),
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    Lit(Value),
    Ref(String, Option<String>),
    State(String),
    Mode(String),
    Guard(String),
    Not(Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
    Cmp(Cmp, Box<Self>, Box<Self>),
    Cat(Box<Self>, Box<Self>),
    Call(Func, Vec<Self>),
}

impl Expr {
    fn hash(&self, h: LawIdHasher) -> LawIdHasher {
        match self {
            Self::Lit(v) => v.hash(h.str("lit")),
            Self::Ref(n, f) => h
                .str("ref")
                .str(n)
                .str(f.as_deref().unwrap_or(""))
                .u32(u32::from(f.is_some())),
            Self::State(s) => h.str("state").str(s),
            Self::Mode(m) => h.str("mode").str(m),
            Self::Guard(g) => h.str("guard").str(g),
            Self::Not(e) => e.hash(h.str("not")),
            Self::And(a, b) => b.hash(a.hash(h.str("and"))),
            Self::Or(a, b) => b.hash(a.hash(h.str("or"))),
            Self::Cmp(op, a, b) => b.hash(a.hash(h.str("cmp").str(op.word()))),
            Self::Cat(a, b) => b.hash(a.hash(h.str("++"))),
            Self::Call(f, args) => {
                let name = FUNCTIONS
                    .iter()
                    .find(|(_, g)| g == f)
                    .map_or("", |(n, _)| n);
                let mut h = h.str("call").str(name).u64(args.len() as u64);
                for a in args {
                    h = a.hash(h);
                }
                h
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Name(String),
    Int(i64),
    Text(String),
    Bytes(Vec<u8>),
    Op(&'static str),
}

const OPS: &[&str] = &["==", "!=", "<=", ">=", "++", "<", ">", "(", ")", ",", "."];

/// The bytes from `at` on that `keep` accepts, as a count
fn run_len(b: &[u8], at: usize, keep: impl Fn(u8) -> bool) -> usize {
    b[at..].iter().take_while(|&&c| keep(c)).count()
}

fn tokens(src: &str) -> Result<Vec<Tok>, String> {
    let sb = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    // every step reads at least one byte, so `sb.len()` steps reach the end; a step that read
    // none would stop here instead of looping
    for _ in 0..sb.len() {
        if i >= sb.len() {
            break;
        }
        let ch = sb[i];
        if ch == b' ' || ch == b'\t' {
            i += 1;
        } else if sb[i..].starts_with(b"0x") {
            let start = i + 2;
            let j = start + run_len(sb, start, |c| matches!(c, b'0'..=b'9' | b'a'..=b'f'));
            let bytes = hex_bytes(&src[start..j]).ok_or_else(|| {
                format!(
                    "byte literal `{}` has an odd number of hex digits",
                    &src[i..j]
                )
            })?;
            out.push(Tok::Bytes(bytes));
            i = j;
        } else if ch.is_ascii_digit() {
            let j = i + run_len(sb, i, |c| c.is_ascii_digit());
            let digits = &src[i..j];
            let value = if digits.len() > 1 && digits.starts_with('0') {
                None
            } else {
                integer_text(digits)
            };
            out.push(Tok::Int(
                value.ok_or_else(|| format!("integer literal `{digits}`"))?,
            ));
            i = j;
        } else if ch == b'"' {
            let end = src[i + 1..]
                .find(['"', '\\'])
                .map(|at| i + 1 + at)
                .filter(|&at| sb[at] == b'"')
                .ok_or_else(|| format!("`{}`: a text literal is not closed", &src[i..]))?;
            out.push(Tok::Text(src[i + 1..end].to_owned()));
            i = end + 1;
        } else if let Some(op) = OPS.iter().find(|op| sb[i..].starts_with(op.as_bytes())) {
            out.push(Tok::Op(op));
            i += op.len();
        } else if ch.is_ascii_lowercase() {
            let j = i + run_len(sb, i, |c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_'
            });
            out.push(Tok::Name(src[i..j].to_owned()));
            i = j;
        } else {
            return Err(format!("`{src}`: unexpected `{}`", &src[i..]));
        }
    }
    if i < sb.len() {
        return Err(format!(
            "`{src}`: the reading stopped at byte {i} of {}",
            sb.len()
        ));
    }
    Ok(out)
}

/// What a name means where an expression is read
enum Scope<'a> {
    Machine {
        states: &'a [String],
        modes: Option<&'a [String]>,
        guards: &'a [String],
        consts: &'a [(String, Ty, Value)],
    },
    Predicate {
        types: &'a [(String, Ty)],
        consts: &'a [(String, Ty, Value)],
        uses_hmac: bool,
    },
}

struct Parser<'a> {
    src: &'a str,
    t: Vec<Tok>,
    i: usize,
    scope: Scope<'a>,
}

type Typed = (Expr, Ty);

impl Parser<'_> {
    fn fail<T>(&self, why: impl fmt::Display) -> Result<T, String> {
        Err(format!("`{}`: {why}", self.src))
    }

    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }

    fn peek_at(&self, k: usize) -> Option<&Tok> {
        self.t.get(self.i + k)
    }

    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Some(Tok::Op(o)) if *o == op)
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Name(n)) if n == w)
    }

    fn expect_op(&mut self, op: &str) -> Result<(), String> {
        if self.is_op(op) {
            self.i += 1;
            Ok(())
        } else {
            self.fail(format!("expected `{op}`"))
        }
    }

    const fn machine(&self) -> bool {
        matches!(self.scope, Scope::Machine { .. })
    }

    fn parse(mut self) -> Result<Typed, String> {
        let e = self.or()?;
        if self.i != self.t.len() {
            return self.fail("unexpected text after the expression");
        }
        Ok(e)
    }

    fn boolean(&self, e: &Typed, what: &str) -> Result<(), String> {
        if e.1 == Ty::Bool {
            Ok(())
        } else {
            self.fail(format!("`{what}` needs booleans"))
        }
    }

    fn or(&mut self) -> Result<Typed, String> {
        let mut e = self.and()?;
        while self.is_word("or") {
            self.i += 1;
            let r = self.and()?;
            self.boolean(&e, "or")?;
            self.boolean(&r, "or")?;
            e = (Expr::Or(Box::new(e.0), Box::new(r.0)), Ty::Bool);
        }
        Ok(e)
    }

    fn and(&mut self) -> Result<Typed, String> {
        let mut e = self.not()?;
        while self.is_word("and") {
            self.i += 1;
            let r = self.not()?;
            self.boolean(&e, "and")?;
            self.boolean(&r, "and")?;
            e = (Expr::And(Box::new(e.0), Box::new(r.0)), Ty::Bool);
        }
        Ok(e)
    }

    fn not(&mut self) -> Result<Typed, String> {
        if self.is_word("not") {
            self.i += 1;
            let e = self.not()?;
            self.boolean(&e, "not")?;
            return Ok((Expr::Not(Box::new(e.0)), Ty::Bool));
        }
        self.cmp()
    }

    fn cmp_op(&self) -> Option<Cmp> {
        Some(match self.peek()? {
            Tok::Op("==") => Cmp::Eq,
            Tok::Op("!=") => Cmp::Ne,
            Tok::Op("<") => Cmp::Lt,
            Tok::Op("<=") => Cmp::Le,
            Tok::Op(">") => Cmp::Gt,
            Tok::Op(">=") => Cmp::Ge,
            _ => return None,
        })
    }

    fn cmp(&mut self) -> Result<Typed, String> {
        let a = self.cat()?;
        let Some(op) = self.cmp_op() else {
            return Ok(a);
        };
        if self.machine() {
            return self.fail("a machine expression compares only `state` and `mode`");
        }
        self.i += 1;
        let b = self.cat()?;
        if self.cmp_op().is_some() {
            return self.fail("comparisons do not chain");
        }
        if a.1 != b.1 || matches!(a.1, Ty::Record(_)) {
            return self.fail(format!(
                "`{}` compares {} with {}",
                op.word(),
                a.1.word(),
                b.1.word()
            ));
        }
        if !matches!(op, Cmp::Eq | Cmp::Ne) && !matches!(a.1, Ty::Integer | Ty::Time) {
            return self.fail(format!(
                "`{}` orders integers or times, not {}",
                op.word(),
                a.1.word()
            ));
        }
        Ok((Expr::Cmp(op, Box::new(a.0), Box::new(b.0)), Ty::Bool))
    }

    fn cat(&mut self) -> Result<Typed, String> {
        let mut e = self.primary()?;
        while self.is_op("++") {
            if self.machine() {
                return self.fail("a machine expression has no `++`");
            }
            self.i += 1;
            let r = self.primary()?;
            if e.1 != Ty::Bytes || r.1 != Ty::Bytes {
                return self.fail("`++` joins bytes");
            }
            e = (Expr::Cat(Box::new(e.0), Box::new(r.0)), Ty::Bytes);
        }
        Ok(e)
    }

    fn primary(&mut self) -> Result<Typed, String> {
        let Some(tok) = self.peek().cloned() else {
            return self.fail("an expression ends early");
        };
        self.i += 1;
        let lit = |v: Value, ty: Ty| Ok((Expr::Lit(v), ty));
        match tok {
            Tok::Op("(") => {
                let e = self.or()?;
                self.expect_op(")")?;
                Ok(e)
            }
            Tok::Name(n) if n == "true" || n == "false" => lit(Value::Bool(n == "true"), Ty::Bool),
            _ if self.machine() => self.machine_atom(tok),
            Tok::Int(v) => lit(Value::Int(v), Ty::Integer),
            Tok::Text(t) => lit(Value::Text(t), Ty::Text),
            Tok::Bytes(b) => lit(Value::Bytes(b), Ty::Bytes),
            Tok::Name(n) => {
                if let Some(&(_, f)) = FUNCTIONS.iter().find(|(name, _)| *name == n) {
                    if self.is_op("(") {
                        return self.call(f);
                    }
                }
                let field = if self.is_op(".") {
                    self.i += 1;
                    match self.peek().cloned() {
                        Some(Tok::Name(f)) => {
                            self.i += 1;
                            Some(f)
                        }
                        _ => return self.fail("a field name is expected after `.`"),
                    }
                } else {
                    None
                };
                self.predicate_atom(&n, field)
            }
            Tok::Op(o) => self.fail(format!("unexpected `{o}`")),
        }
    }

    fn machine_atom(&mut self, tok: Tok) -> Result<Typed, String> {
        let Scope::Machine {
            states,
            modes,
            guards,
            consts,
        } = &self.scope
        else {
            unreachable!("machine scope");
        };
        let Tok::Name(n) = tok else {
            return self.fail("a machine expression has no literals other than true / false");
        };
        if (n == "state" || n == "mode") && matches!(self.peek(), Some(Tok::Op("==" | "!="))) {
            let negate = self.is_op("!=");
            let Some(Tok::Name(v)) = self.peek_at(1).cloned() else {
                return self.fail(format!("`{n}` is compared with a name"));
            };
            self.i += 2;
            let node = if n == "state" {
                if !states.contains(&v) {
                    return self.fail(format!("`{v}` is not a state"));
                }
                Expr::State(v)
            } else {
                if !modes.is_some_and(|m| m.contains(&v)) {
                    return self.fail(format!("`{v}` is not a mode value"));
                }
                Expr::Mode(v)
            };
            let node = if negate {
                Expr::Not(Box::new(node))
            } else {
                node
            };
            return Ok((node, Ty::Bool));
        }
        if guards.contains(&n) {
            return Ok((Expr::Guard(n), Ty::Bool));
        }
        if let Some((_, Ty::Bool, v)) = consts.iter().find(|(c, _, _)| *c == n) {
            return Ok((Expr::Lit(v.clone()), Ty::Bool));
        }
        self.fail(format!(
            "`{n}` is not a guard of the event or a boolean constant here"
        ))
    }

    fn predicate_atom(&self, n: &str, field: Option<String>) -> Result<Typed, String> {
        let Scope::Predicate { types, consts, .. } = &self.scope else {
            unreachable!("predicate scope");
        };
        if field.is_none() {
            if let Some((_, ty, v)) = consts.iter().find(|(c, _, _)| c == n) {
                return Ok((Expr::Lit(v.clone()), ty.clone()));
            }
        }
        let Some((_, ty)) = types.iter().find(|(t, _)| t == n) else {
            return self.fail(format!("`{n}` is not declared on an earlier line"));
        };
        match (field, ty) {
            (None, ty) => Ok((Expr::Ref(n.to_owned(), None), ty.clone())),
            (Some(f), Ty::Record(fields)) => match fields.iter().find(|(name, _)| *name == f) {
                Some((_, fty)) => Ok((Expr::Ref(n.to_owned(), Some(f)), fty.clone())),
                None => self.fail(format!("`{n}.{f}`: no such field")),
            },
            (Some(f), _) => self.fail(format!("`{n}.{f}`: `{n}` is not a record")),
        }
    }

    fn call(&mut self, f: Func) -> Result<Typed, String> {
        self.expect_op("(")?;
        let mut args = vec![self.or()?];
        while self.is_op(",") {
            self.i += 1;
            args.push(self.or()?);
        }
        self.expect_op(")")?;
        let tys: Vec<&Ty> = args.iter().map(|a| &a.1).collect();
        let ty = match (f, tys.as_slice()) {
            (Func::Len, [Ty::Text | Ty::Bytes]) => Ty::Integer,
            (Func::Prefix, [Ty::Bytes, Ty::Integer]) | (Func::Utf8, [Ty::Text]) => Ty::Bytes,
            (Func::HmacSha256, [Ty::Bytes, Ty::Bytes]) => {
                if !matches!(
                    self.scope,
                    Scope::Predicate {
                        uses_hmac: true,
                        ..
                    }
                ) {
                    return self
                        .fail("`hmac_sha256` is called without `use alice-sign hmac_sha256`");
                }
                Ty::Bytes
            }
            _ => return self.fail("a function is called with arguments of other types"),
        };
        Ok((Expr::Call(f, args.into_iter().map(|a| a.0).collect()), ty))
    }
}

// ------------------------------------------------------------------ reading

const PEOPLE: &[&str] = &["law", "claim", "source", "verdict", "oracle"];
const RESERVED: &[&str] = &[
    "true",
    "false",
    "and",
    "or",
    "not",
    "state",
    "mode",
    "shape",
    "from",
    "to",
    "when",
    "guard",
    "initial",
    "len",
    "prefix",
    "utf8",
    "hmac_sha256",
];

fn is_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
        && !RESERVED.contains(&s)
}

/// The words of a line (split on spaces and tabs) with the byte offset after each
fn words(line: &str) -> Vec<(&str, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in line
        .char_indices()
        .chain(std::iter::once((line.len(), ' ')))
    {
        if c == ' ' || c == '\t' {
            if let Some(s) = start.take() {
                out.push((&line[s..i], i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    out
}

fn rest_after(line: &str, w: &[(&str, usize)], k: usize) -> String {
    line[w[k].1..].trim_matches([' ', '\t']).to_owned()
}

/// The lines of a law file without comments: `(line number, text)`, empty lines left out
fn lines(text: &str) -> Result<Vec<(usize, &str)>, RuleLawError> {
    let segments: Vec<&str> = text.split('\n').collect();
    let last = segments.len() - 1;
    let mut out = Vec::new();
    for (n, seg) in segments.iter().enumerate() {
        let body = if n < last {
            seg.strip_suffix('\r').unwrap_or(seg)
        } else {
            seg
        };
        if let Some(c) = body.chars().find(|&c| !crate::law_input::allowed_char(c)) {
            return Err(err(
                n + 1,
                format!("U+{:04X} is not allowed in a law file", u32::from(c)),
            ));
        }
        let l = body.split('#').next().unwrap_or("");
        if !l.trim_matches([' ', '\t']).is_empty() {
            out.push((n + 1, l));
        }
    }
    Ok(out)
}

/// A rule law, read from its file
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleLaw {
    /// `kind machine`
    Machine(MachineLaw),
    /// `kind predicate`
    Predicate(PredicateLaw),
    /// `kind table`
    Table(TableLaw),
}

/// A state machine law
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineLaw {
    consts: Vec<(String, Ty, Value)>,
    modes: Option<Vec<String>>,
    states: Vec<String>,
    initial: String,
    events: Vec<(String, Vec<String>)>,
    on: Vec<(String, Vec<String>, String, Option<Expr>)>,
    allow: Vec<(String, Expr)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
    Split(String, String, Vec<String>),
    Bind(String, String, BindAs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BindAs {
    Integer,
    Time,
    Hex,
}

/// A predicate law
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredicateLaw {
    consts: Vec<(String, Ty, Value)>,
    uses_hmac: bool,
    inputs: Vec<(String, Ty)>,
    steps: Vec<Step>,
    requires: Vec<(String, Expr)>,
}

/// A table law: rows tried in file order
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableLaw {
    consts: Vec<(String, Ty, Value)>,
    inputs: Vec<(String, Ty)>,
    values: Vec<String>,
    rows: Vec<(String, String, Expr)>,
    otherwise: String,
}

struct Names(Vec<String>);

impl Names {
    fn declare(&mut self, line: usize, name: &str, what: &str) -> Result<(), RuleLawError> {
        if !is_name(name) {
            return Err(err(line, format!("`{name}` is not a name ({what})")));
        }
        if self.0.iter().any(|n| n == name) {
            return Err(err(line, format!("`{name}` is declared twice")));
        }
        self.0.push(name.to_owned());
        Ok(())
    }
}

fn const_line(
    line: usize,
    l: &str,
    names: &mut Names,
    consts: &mut Vec<(String, Ty, Value)>,
) -> Result<(), RuleLawError> {
    let w = words(l);
    let bad = || err(line, "`const <name> <type> = <literal>` expected");
    if w.len() < 5 || w[3].0 != "=" {
        return Err(bad());
    }
    let ty = Ty::scalar(w[2].0).ok_or_else(bad)?;
    names.declare(line, w[1].0, "constant")?;
    let lit = rest_after(l, &w, 3);
    let toks = tokens(&lit).map_err(|e| err(line, e))?;
    let v = match (&ty, toks.as_slice()) {
        (Ty::Integer | Ty::Time, [Tok::Int(v)]) => Value::Int(*v),
        (Ty::Text, [Tok::Text(t)]) => Value::Text(t.clone()),
        (Ty::Bytes, [Tok::Bytes(b)]) => Value::Bytes(b.clone()),
        (Ty::Bool, [Tok::Name(n)]) if n == "true" || n == "false" => Value::Bool(n == "true"),
        _ => {
            return Err(err(
                line,
                format!("`{lit}` is not a literal of type {}", ty.word()),
            ))
        }
    };
    consts.push((w[1].0.to_owned(), ty, v));
    Ok(())
}

/// Reads a rule law file (conformance/TASK.md "Rule laws")
///
/// # Errors
///
/// When the file is not a rule law or does not read by the rules of its kind
pub fn rule_law_from_file(text: &str) -> Result<RuleLaw, RuleLawError> {
    let kind = rule_kind(text).ok_or_else(|| {
        err(
            0,
            "not a rule law (no `kind machine` / `kind predicate` / `kind table`)",
        )
    })?;
    let ls = lines(text)?;
    let kinds = ls.iter().filter(|(_, l)| words(l)[0].0 == "kind").count();
    if kinds != 1 {
        return Err(err(0, "a rule law file has exactly one `kind` line"));
    }
    match kind {
        RuleKind::Machine => read_machine(&ls).map(RuleLaw::Machine),
        RuleKind::Predicate => read_predicate(&ls).map(RuleLaw::Predicate),
        RuleKind::Table => read_table(&ls).map(RuleLaw::Table),
    }
}

#[allow(clippy::too_many_lines)]
fn read_machine(ls: &[(usize, &str)]) -> Result<MachineLaw, RuleLawError> {
    let mut names = Names(Vec::new());
    let mut consts = Vec::new();
    let mut modes: Option<Vec<String>> = None;
    let mut states = Vec::new();
    let mut initial: Option<String> = None;
    let mut events: Vec<(String, Vec<String>)> = Vec::new();
    // `on` / `allow` lines are read after every declaration (a name may come later)
    let mut pending: Vec<(usize, &str)> = Vec::new();
    for &(n, l) in ls {
        let w = words(l);
        let head = w[0].0;
        match head {
            h if PEOPLE.contains(&h) || h == "kind" => {}
            "const" => const_line(n, l, &mut names, &mut consts)?,
            "mode" => {
                if modes.is_some() || w.len() < 2 {
                    return Err(err(n, "at most one `mode <value>...` line"));
                }
                for (v, _) in &w[1..] {
                    names.declare(n, v, "mode value")?;
                }
                modes = Some(w[1..].iter().map(|(v, _)| (*v).to_owned()).collect());
            }
            "state" => {
                match w.as_slice() {
                    [_, (s, _), (i, _)] if *i == "initial" => {
                        if initial.is_some() {
                            return Err(err(n, "a second initial state"));
                        }
                        initial = Some((*s).to_owned());
                    }
                    [_, _] => {}
                    _ => return Err(err(n, "`state <name> [initial]`")),
                }
                names.declare(n, w[1].0, "state")?;
                states.push(w[1].0.to_owned());
            }
            "event" => {
                if w.len() < 2 || (w.len() > 2 && (w[2].0 != "guard" || w.len() < 4)) {
                    return Err(err(n, "`event <name> [guard <guard>...]`"));
                }
                names.declare(n, w[1].0, "event")?;
                for (g, _) in w.iter().skip(3) {
                    names.declare(n, g, "guard")?;
                }
                events.push((
                    w[1].0.to_owned(),
                    w.iter().skip(3).map(|(g, _)| (*g).to_owned()).collect(),
                ));
            }
            "on" => pending.push((n, l)),
            "allow" => {
                if w.len() < 4 || w[2].0 != "when" {
                    return Err(err(n, "`allow <name> when <expression>`"));
                }
                names.declare(n, w[1].0, "allowance")?;
                pending.push((n, l));
            }
            other => return Err(err(n, format!("`{other}` is not a line of a machine law"))),
        }
    }
    if states.is_empty() || events.is_empty() {
        return Err(err(0, "a machine law has at least one state and one event"));
    }
    let initial = initial.ok_or_else(|| err(0, "a machine law has exactly one initial state"))?;
    let mut on = Vec::new();
    let mut allow = Vec::new();
    let expr = |n: usize, src: &str, guards: &[String]| -> Result<Expr, RuleLawError> {
        let p = Parser {
            src,
            t: tokens(src).map_err(|e| err(n, e))?,
            i: 0,
            scope: Scope::Machine {
                states: &states,
                modes: modes.as_deref(),
                guards,
                consts: &consts,
            },
        };
        let (e, ty) = p.parse().map_err(|e| err(n, e))?;
        if ty != Ty::Bool {
            return Err(err(n, format!("`{src}` is not a boolean")));
        }
        Ok(e)
    };
    for (n, l) in pending {
        let w = words(l);
        if w[0].0 == "allow" {
            allow.push((w[1].0.to_owned(), expr(n, &rest_after(l, &w, 2), &[])?));
            continue;
        }
        let bad = || {
            err(
                n,
                "`on <event> from <state>... to <state> [when <expression>]`",
            )
        };
        if w.len() < 6 || w[2].0 != "from" {
            return Err(bad());
        }
        let to_at = w.iter().position(|(x, _)| *x == "to").ok_or_else(bad)?;
        if to_at < 4
            || to_at + 1 >= w.len()
            || (to_at + 2 < w.len() && (w[to_at + 2].0 != "when" || to_at + 3 >= w.len()))
        {
            return Err(bad());
        }
        let event = w[1].0;
        let from: Vec<String> = w[3..to_at].iter().map(|(s, _)| (*s).to_owned()).collect();
        let to = w[to_at + 1].0;
        let Some((_, guards)) = events.iter().find(|(e, _)| e == event) else {
            return Err(err(n, format!("`{event}` is not a declared event")));
        };
        if !states.iter().any(|s| s == to) || from.iter().any(|f| !states.contains(f)) {
            return Err(err(n, "an `on` line names an undeclared state"));
        }
        let when = if to_at + 2 < w.len() {
            Some(expr(n, &rest_after(l, &w, to_at + 2), guards)?)
        } else {
            None
        };
        on.push((event.to_owned(), from, to.to_owned(), when));
    }
    if on.is_empty() {
        return Err(err(0, "a machine law has at least one `on` line"));
    }
    Ok(MachineLaw {
        consts,
        modes,
        states,
        initial,
        events,
        on,
        allow,
    })
}

fn parse_type(line: usize, src: &str) -> Result<Ty, RuleLawError> {
    let src = src.trim_matches([' ', '\t']);
    if let Some(t) = Ty::scalar(src) {
        return Ok(t);
    }
    let inner = src
        .strip_prefix("record(")
        .and_then(|r| r.strip_suffix(')'))
        .ok_or_else(|| err(line, format!("type `{src}`")))?;
    let mut fields: Vec<(String, Ty)> = Vec::new();
    for part in inner.split(',') {
        let (name, ty) = part
            .split_once(':')
            .ok_or_else(|| err(line, format!("record field `{}`", part.trim())))?;
        let (name, ty) = (name.trim_matches([' ', '\t']), ty.trim_matches([' ', '\t']));
        let ty =
            Ty::scalar(ty).ok_or_else(|| err(line, format!("record field `{}`", part.trim())))?;
        let b = name.as_bytes();
        let ok = !b.is_empty()
            && b[0].is_ascii_lowercase()
            && b.iter()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_');
        if !ok || fields.iter().any(|(f, _)| f == name) {
            return Err(err(line, format!("record field `{}`", part.trim())));
        }
        fields.push((name.to_owned(), ty));
    }
    Ok(Ty::Record(fields))
}

#[allow(clippy::too_many_lines)]
fn read_predicate(ls: &[(usize, &str)]) -> Result<PredicateLaw, RuleLawError> {
    let mut names = Names(Vec::new());
    let mut consts: Vec<(String, Ty, Value)> = Vec::new();
    let mut uses_hmac = false;
    let mut inputs: Vec<(String, Ty)> = Vec::new();
    // every name an expression may use, with its type (inputs, parts, bound parts)
    let mut types: Vec<(String, Ty)> = Vec::new();
    let mut steps = Vec::new();
    let mut requires: Vec<(String, Expr)> = Vec::new();
    let mut example: Option<(usize, Json)> = None;
    for &(n, l) in ls {
        let w = words(l);
        let head = w[0].0;
        let trimmed = l.trim_start_matches([' ', '\t']);
        if trimmed.to_ascii_lowercase().starts_with("x-") {
            let (kw, rest) = l.split_once(' ').unwrap_or((l, ""));
            if !matches!(kw, "x-input" | "x-split" | "x-bind") || rest.starts_with([' ', '\t']) {
                return Err(err(
                    n,
                    format!("`{}` is not an x- line of a predicate law", l.trim()),
                ));
            }
            let rw = words(rest);
            match kw {
                "x-input" => {
                    let Some(&(name, at)) = rw.first() else {
                        return Err(err(n, "`x-input <name> <type>`"));
                    };
                    names.declare(n, name, "input")?;
                    let ty = parse_type(n, &rest[at..])?;
                    if let Ty::Record(_) = ty {}
                    types.push((name.to_owned(), ty.clone()));
                    inputs.push((name.to_owned(), ty));
                }
                "x-split" => {
                    let bad = || err(n, "`x-split <source> \"<separator>\" <name> <name>...`");
                    let &(src, at) = rw.first().ok_or_else(bad)?;
                    let after = rest[at..].trim_start_matches([' ', '\t']);
                    let body = after.strip_prefix('"').ok_or_else(bad)?;
                    let close = body
                        .find(['"', '\\'])
                        .filter(|&k| body.as_bytes()[k] == b'"')
                        .ok_or_else(bad)?;
                    let sep = &body[..close];
                    let tail = &body[close + 1..];
                    if !tail.starts_with([' ', '\t']) {
                        return Err(bad());
                    }
                    let parts: Vec<&str> = words(tail).into_iter().map(|(x, _)| x).collect();
                    let src_is_text = types.iter().any(|(t, ty)| t == src && *ty == Ty::Text);
                    if !src_is_text || sep.is_empty() || parts.len() < 2 {
                        return Err(err(n, "a split takes a text, a separator that is not empty and two names or more"));
                    }
                    for p in &parts {
                        names.declare(n, p, "part")?;
                        types.push(((*p).to_owned(), Ty::Text));
                    }
                    steps.push(Step::Split(
                        src.to_owned(),
                        sep.to_owned(),
                        parts.iter().map(|p| (*p).to_owned()).collect(),
                    ));
                }
                _ => {
                    let as_ = rw.get(2).and_then(|(t, _)| match *t {
                        "integer" => Some(BindAs::Integer),
                        "time" => Some(BindAs::Time),
                        "hex" => Some(BindAs::Hex),
                        _ => None,
                    });
                    let part_is_text = rw.get(1).is_some_and(|(p, _)| {
                        types.iter().any(|(t, ty)| t == p && *ty == Ty::Text)
                    });
                    let (Some(as_), true, 3) = (as_, part_is_text, rw.len()) else {
                        return Err(err(n, "`x-bind <name> <part> integer|time|hex`"));
                    };
                    names.declare(n, rw[0].0, "bound part")?;
                    let ty = match as_ {
                        BindAs::Integer => Ty::Integer,
                        BindAs::Time => Ty::Time,
                        BindAs::Hex => Ty::Bytes,
                    };
                    types.push((rw[0].0.to_owned(), ty));
                    steps.push(Step::Bind(rw[0].0.to_owned(), rw[1].0.to_owned(), as_));
                }
            }
            continue;
        }
        match head {
            h if PEOPLE.contains(&h) || h == "kind" => {}
            "const" => const_line(n, l, &mut names, &mut consts)?,
            "use" => {
                let ws: Vec<&str> = w.iter().map(|(x, _)| *x).collect();
                if ws != ["use", "alice-sign", "hmac_sha256"] {
                    return Err(err(n, "the only call is `use alice-sign hmac_sha256`"));
                }
                uses_hmac = true;
            }
            "require" => {
                let label = w.get(1).and_then(|(x, _)| x.strip_suffix(':'));
                let Some(label) = label.filter(|lb| {
                    let b = lb.as_bytes();
                    !b.is_empty()
                        && b[0].is_ascii_lowercase()
                        && b.iter()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
                }) else {
                    return Err(err(n, "`require <label>: <expression>`"));
                };
                if w.len() < 3 {
                    return Err(err(n, "`require <label>: <expression>`"));
                }
                // labels have a namespace of their own
                if RESERVED.contains(&label) || requires.iter().any(|(lb, _)| lb == label) {
                    return Err(err(n, format!("label `{label}` is reserved or used twice")));
                }
                let src = rest_after(l, &w, 1);
                let p = Parser {
                    src: &src,
                    t: tokens(&src).map_err(|e| err(n, e))?,
                    i: 0,
                    scope: Scope::Predicate {
                        types: &types,
                        consts: &consts,
                        uses_hmac,
                    },
                };
                let (e, ty) = p.parse().map_err(|e| err(n, e))?;
                if ty != Ty::Bool {
                    return Err(err(n, format!("`{src}` is not a boolean")));
                }
                requires.push((label.to_owned(), e));
            }
            "example" => {
                let j = (w.len() > 1)
                    .then(|| crate::law_input::parse_json(&rest_after(l, &w, 0)).ok())
                    .flatten()
                    .filter(|j| matches!(j, Json::Object(_)));
                let (Some(j), None) = (j, &example) else {
                    return Err(err(n, "one `example <JSON object>` line"));
                };
                example = Some((n, j));
            }
            other => {
                return Err(err(
                    n,
                    format!("`{other}` is not a line of a predicate law"),
                ))
            }
        }
    }
    let Some((ex_line, ex)) = example else {
        return Err(err(
            0,
            "a predicate law has at least one `require` line and one `example` line",
        ));
    };
    if requires.is_empty() {
        return Err(err(
            0,
            "a predicate law has at least one `require` line and one `example` line",
        ));
    }
    let law = PredicateLaw {
        consts,
        uses_hmac,
        inputs,
        steps,
        requires,
    };
    match law.respond(&ex) {
        Answer::Unsupported(_) => {}
        Answer::Outputs(o) if o.get("valid") == Some(&Json::Bool(true)) => {}
        _ => return Err(err(ex_line, "the example does not hold")),
    }
    Ok(law)
}

/// A label of a `require` / `row` line (labels have a namespace of their own)
fn is_label(lb: &str) -> bool {
    let b = lb.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
        && !RESERVED.contains(&lb)
}

#[allow(clippy::too_many_lines)]
fn read_table(ls: &[(usize, &str)]) -> Result<TableLaw, RuleLawError> {
    let mut names = Names(Vec::new());
    let mut consts: Vec<(String, Ty, Value)> = Vec::new();
    let mut inputs: Vec<(String, Ty)> = Vec::new();
    let mut values: Option<Vec<String>> = None;
    let mut rows: Vec<(String, String, Expr)> = Vec::new();
    let mut otherwise: Option<String> = None;
    for &(n, l) in ls {
        let w = words(l);
        let head = w[0].0;
        let trimmed = l.trim_start_matches([' ', '\t']);
        if trimmed.to_ascii_lowercase().starts_with("x-") {
            let (kw, rest) = l.split_once(' ').unwrap_or((l, ""));
            if kw != "x-input" || rest.starts_with([' ', '\t']) {
                return Err(err(
                    n,
                    format!("`{}` is not an x- line of a table law", l.trim()),
                ));
            }
            let rw = words(rest);
            let Some(&(name, at)) = rw.first() else {
                return Err(err(n, "`x-input <name> <type>`"));
            };
            names.declare(n, name, "input")?;
            inputs.push((name.to_owned(), parse_type(n, &rest[at..])?));
            continue;
        }
        match head {
            h if PEOPLE.contains(&h) || h == "kind" => {}
            "const" => const_line(n, l, &mut names, &mut consts)?,
            "value" => {
                if values.is_some() || w.len() < 3 {
                    return Err(err(
                        n,
                        "one `value <name> <name>...` line, two values or more",
                    ));
                }
                for (v, _) in &w[1..] {
                    names.declare(n, v, "value")?;
                }
                values = Some(w[1..].iter().map(|(v, _)| (*v).to_owned()).collect());
            }
            "row" => {
                let bad = || err(n, "`row <label>: <value> when <expression>`");
                if otherwise.is_some() {
                    return Err(err(n, "a `row` line after the `otherwise` line"));
                }
                let label = w
                    .get(1)
                    .and_then(|(x, _)| x.strip_suffix(':'))
                    .ok_or_else(bad)?;
                if w.len() < 5 || w[3].0 != "when" || !is_name_like(label) {
                    return Err(bad());
                }
                if !is_label(label) || rows.iter().any(|(lb, _, _)| lb == label) {
                    return Err(err(n, format!("label `{label}` is reserved or used twice")));
                }
                let src = rest_after(l, &w, 3);
                let p = Parser {
                    src: &src,
                    t: tokens(&src).map_err(|e| err(n, e))?,
                    i: 0,
                    scope: Scope::Predicate {
                        types: &inputs,
                        consts: &consts,
                        uses_hmac: false,
                    },
                };
                let (e, ty) = p.parse().map_err(|e| err(n, e))?;
                if ty != Ty::Bool {
                    return Err(err(n, format!("`{src}` is not a boolean")));
                }
                rows.push((label.to_owned(), w[2].0.to_owned(), e));
            }
            "otherwise" => {
                if w.len() != 2 || otherwise.is_some() {
                    return Err(err(n, "one `otherwise <value>` line"));
                }
                otherwise = Some(w[1].0.to_owned());
            }
            other => return Err(err(n, format!("`{other}` is not a line of a table law"))),
        }
    }
    let (Some(values), Some(otherwise), false) = (values, otherwise, rows.is_empty()) else {
        return Err(err(
            0,
            "a table law has one `value` line, at least one `row` line and one `otherwise` line",
        ));
    };
    let given: Vec<&String> = rows
        .iter()
        .map(|(_, v, _)| v)
        .chain(std::iter::once(&otherwise))
        .collect();
    if let Some(stray) = given.iter().find(|v| !values.contains(v)) {
        return Err(err(
            0,
            format!("`{stray}` is not a value of the `value` line"),
        ));
    }
    if let Some(unused) = values.iter().find(|v| !given.contains(v)) {
        return Err(err(0, format!("value `{unused}` is given by no row")));
    }
    Ok(TableLaw {
        consts,
        inputs,
        values,
        rows,
        otherwise,
    })
}

/// The shape of a label before its namespace is checked (letters, digits and `_`)
fn is_name_like(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

// ------------------------------------------------------------------ answering

impl RuleLaw {
    /// The answer to a request whose `inputs` is `inputs` (`{}` when the request has none)
    #[must_use]
    pub fn respond(&self, inputs: &Json) -> Answer {
        match self {
            Self::Machine(m) => m.respond(inputs),
            Self::Predicate(p) => p.respond(inputs),
            Self::Table(t) => t.respond(inputs),
        }
    }

    /// The kind of the law
    #[must_use]
    pub const fn kind(&self) -> RuleKind {
        match self {
            Self::Machine(_) => RuleKind::Machine,
            Self::Predicate(_) => RuleKind::Predicate,
            Self::Table(_) => RuleKind::Table,
        }
    }

    /// The identifier of the rule (module doc)
    #[must_use]
    pub fn law_id(&self) -> [u8; 32] {
        match self {
            Self::Machine(m) => m.law_id(),
            Self::Predicate(p) => p.law_id(),
            Self::Table(t) => t.law_id(),
        }
    }
}

fn hash_consts(mut h: LawIdHasher, consts: &[(String, Ty, Value)]) -> LawIdHasher {
    h = h.str("consts").u64(consts.len() as u64);
    for (n, ty, v) in consts {
        h = v.hash(ty.hash(h.str(n)));
    }
    h
}

impl MachineLaw {
    fn respond(&self, inp: &Json) -> Answer {
        let mut keys = vec!["state", "event"];
        if self.modes.is_some() {
            keys.push("mode");
        }
        if let Some(k) = keys.iter().find(|k| inp.get(k).is_none()) {
            return Answer::RequestError(format!("{k} missing"));
        }
        let text = |k: &str| match inp.get(k) {
            Some(Json::Text(t)) => Some(t.as_str()),
            _ => None,
        };
        let Some(state) = text("state").filter(|s| self.states.iter().any(|x| x == s)) else {
            return Answer::Rejected("unknown state".to_owned());
        };
        let Some((event, guard_names)) =
            text("event").and_then(|e| self.events.iter().find(|(x, _)| x == e))
        else {
            return Answer::Rejected("unknown event".to_owned());
        };
        let mode = text("mode");
        if let Some(ms) = &self.modes {
            if !mode.is_some_and(|m| ms.iter().any(|x| x == m)) {
                return Answer::Rejected("unknown mode".to_owned());
            }
        }
        let empty = Json::Object(Vec::new());
        let guards = inp.get("guards").unwrap_or(&empty);
        if !matches!(guards, Json::Object(_)) {
            return Answer::Rejected("guards is not an object".to_owned());
        }
        let mut g = Vec::new();
        for name in guard_names {
            match guards.get(name) {
                Some(Json::Bool(b)) => g.push((name.as_str(), *b)),
                _ => return Answer::Rejected(format!("guard {name} missing or not a boolean")),
            }
        }
        let holds = |e: &Expr, st: &str| machine_eval(e, st, mode, &g);
        let mut new = state;
        let mut taken = false;
        for (ev, from, to, when) in &self.on {
            if ev == event
                && from.iter().any(|f| f == state)
                && when.as_ref().is_none_or(|w| holds(w, state))
            {
                new = to;
                taken = true;
                break;
            }
        }
        let allow = self
            .allow
            .iter()
            .map(|(name, e)| (name.clone(), Json::Bool(holds(e, new))))
            .collect();
        Answer::Outputs(Json::Object(vec![
            ("state".to_owned(), Json::Text(new.to_owned())),
            (
                "transition".to_owned(),
                Json::Text(if taken { "taken" } else { "refused" }.to_owned()),
            ),
            ("allow".to_owned(), Json::Object(allow)),
        ]))
    }

    fn law_id(&self) -> [u8; 32] {
        let mut h = hash_consts(
            LawIdHasher::new(MACHINE_LAW_KIND, &RULE_SEMANTICS_ID),
            &self.consts,
        );
        h = h.str("modes").u32(u32::from(self.modes.is_some()));
        for m in self.modes.iter().flatten() {
            h = h.str(m);
        }
        h = h.str("states").u64(self.states.len() as u64);
        for s in &self.states {
            h = h.str(s).u32(u32::from(*s == self.initial));
        }
        h = h.str("events").u64(self.events.len() as u64);
        for (e, guards) in &self.events {
            h = h.str(e).u64(guards.len() as u64);
            for g in guards {
                h = h.str(g);
            }
        }
        h = h.str("on").u64(self.on.len() as u64);
        for (e, from, to, when) in &self.on {
            h = h.str(e).u64(from.len() as u64);
            for f in from {
                h = h.str(f);
            }
            h = h.str(to).u32(u32::from(when.is_some()));
            if let Some(w) = when {
                h = w.hash(h);
            }
        }
        h = h.str("allow").u64(self.allow.len() as u64);
        for (name, e) in &self.allow {
            h = e.hash(h.str(name));
        }
        h.finish()
    }
}

fn machine_eval(e: &Expr, state: &str, mode: Option<&str>, guards: &[(&str, bool)]) -> bool {
    let r = |x: &Expr| machine_eval(x, state, mode, guards);
    match e {
        Expr::Lit(Value::Bool(b)) => *b,
        Expr::Guard(g) => guards.iter().any(|(n, v)| n == g && *v),
        Expr::State(s) => s == state,
        Expr::Mode(m) => mode == Some(m.as_str()),
        Expr::Not(x) => !r(x),
        Expr::And(a, b) => r(a) && r(b),
        Expr::Or(a, b) => r(a) || r(b),
        // the reader admits no other node in a machine expression
        other => unreachable!("not a machine expression: {other:?}"),
    }
}

/// Byte strings compared in time that does not depend on where they differ
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    #[cfg(feature = "crypto-calls")]
    {
        alice_sign::mac::ct_eq(a, b)
    }
    #[cfg(not(feature = "crypto-calls"))]
    {
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }
}

struct NotBuilt;

/// HMAC-SHA256 of alice-sign (the call is built only with `crypto-calls`)
#[cfg_attr(
    feature = "crypto-calls",
    allow(
        clippy::unnecessary_wraps,
        reason = "the build without the call returns NotBuilt"
    )
)]
fn hmac(key: &[u8], message: &[u8]) -> Result<Vec<u8>, NotBuilt> {
    #[cfg(feature = "crypto-calls")]
    {
        Ok(alice_sign::mac::hmac_sha256(key, message).to_vec())
    }
    #[cfg(not(feature = "crypto-calls"))]
    {
        let _ = (key, message);
        Err(NotBuilt)
    }
}

type Env = Vec<(String, Value)>;

fn lookup<'a>(env: &'a Env, name: &str) -> &'a Value {
    env.iter()
        .rev()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v)
        .expect("the reader declared every name")
}

fn predicate_eval(expr: &Expr, env: &Env) -> Result<Value, NotBuilt> {
    let sub = |x: &Expr| predicate_eval(x, env);
    let truth = |x: &Expr| -> Result<bool, NotBuilt> { Ok(matches!(sub(x)?, Value::Bool(true))) };
    Ok(match expr {
        Expr::Lit(v) => v.clone(),
        Expr::Ref(name, None) => lookup(env, name).clone(),
        Expr::Ref(name, Some(field)) => match lookup(env, name) {
            Value::Record(fields) => fields
                .iter()
                .find(|(f, _)| f == field)
                .map(|(_, v)| v.clone())
                .expect("typed"),
            _ => unreachable!("typed: a record"),
        },
        Expr::Not(x) => Value::Bool(!truth(x)?),
        Expr::And(lhs, rhs) => Value::Bool(truth(lhs)? && truth(rhs)?),
        Expr::Or(lhs, rhs) => Value::Bool(truth(lhs)? || truth(rhs)?),
        Expr::Cat(lhs, rhs) => match (sub(lhs)?, sub(rhs)?) {
            (Value::Bytes(mut head), Value::Bytes(tail)) => {
                head.extend_from_slice(&tail);
                Value::Bytes(head)
            }
            _ => unreachable!("typed: bytes"),
        },
        Expr::Cmp(op, lhs, rhs) => {
            let (left, right) = (sub(lhs)?, sub(rhs)?);
            let ord = match (&left, &right) {
                (Value::Int(l), Value::Int(r)) => Some(l.cmp(r)),
                _ => None,
            };
            let eq = match (&left, &right) {
                (Value::Bytes(l), Value::Bytes(r)) => ct_eq(l, r),
                _ => left == right,
            };
            Value::Bool(match op {
                Cmp::Eq => eq,
                Cmp::Ne => !eq,
                Cmp::Lt => ord.is_some_and(std::cmp::Ordering::is_lt),
                Cmp::Le => ord.is_some_and(std::cmp::Ordering::is_le),
                Cmp::Gt => ord.is_some_and(std::cmp::Ordering::is_gt),
                Cmp::Ge => ord.is_some_and(std::cmp::Ordering::is_ge),
            })
        }
        Expr::Call(func, args) => {
            let vals: Vec<Value> = args.iter().map(sub).collect::<Result<_, _>>()?;
            let len = |n: usize| Value::Int(i64::try_from(n).unwrap_or(i64::MAX));
            match (func, vals.as_slice()) {
                (Func::Len, [Value::Text(text)]) => len(text.len()),
                (Func::Len, [Value::Bytes(bytes)]) => len(bytes.len()),
                (Func::Prefix, [Value::Bytes(bytes), Value::Int(n)]) => {
                    let keep = usize::try_from((*n).max(0))
                        .unwrap_or(usize::MAX)
                        .min(bytes.len());
                    Value::Bytes(bytes[..keep].to_vec())
                }
                (Func::Utf8, [Value::Text(text)]) => Value::Bytes(text.as_bytes().to_vec()),
                (Func::HmacSha256, [Value::Bytes(key), Value::Bytes(msg)]) => {
                    Value::Bytes(hmac(key, msg)?)
                }
                _ => unreachable!("typed call"),
            }
        }
        Expr::State(_) | Expr::Mode(_) | Expr::Guard(_) => {
            unreachable!("not a predicate expression")
        }
    })
}

/// The `x-input` values of a request (a missing input first, then one of another type)
fn read_inputs(inputs: &[(String, Ty)], inp: &Json) -> Result<Env, Answer> {
    if let Some((name, _)) = inputs.iter().find(|(n, _)| inp.get(n).is_none()) {
        return Err(Answer::RequestError(format!("{name} missing")));
    }
    let mut env: Env = Vec::new();
    for (name, ty) in inputs {
        match inp.get(name).and_then(|v| read_value(v, ty)) {
            Some(v) => env.push((name.clone(), v)),
            None => return Err(Answer::Rejected(format!("{name} does not have its type"))),
        }
    }
    Ok(env)
}

fn hash_inputs(mut h: LawIdHasher, inputs: &[(String, Ty)]) -> LawIdHasher {
    h = h.str("inputs").u64(inputs.len() as u64);
    for (n, ty) in inputs {
        h = ty.hash(h.str(n));
    }
    h
}

impl PredicateLaw {
    fn respond(&self, inp: &Json) -> Answer {
        let mut env = match read_inputs(&self.inputs, inp) {
            Ok(env) => env,
            Err(a) => return a,
        };
        let out = |failed: Option<&str>| {
            Answer::Outputs(Json::Object(vec![
                ("valid".to_owned(), Json::Bool(failed.is_none())),
                (
                    "failed".to_owned(),
                    failed.map_or(Json::Null, |f| Json::Text(f.to_owned())),
                ),
            ]))
        };
        for step in &self.steps {
            match step {
                Step::Split(src, sep, names) => {
                    let Value::Text(t) = lookup(&env, src) else {
                        unreachable!("typed: a text");
                    };
                    let parts: Vec<String> = t.split(sep.as_str()).map(str::to_owned).collect();
                    if parts.len() != names.len() {
                        return out(Some("shape"));
                    }
                    env.extend(
                        names
                            .iter()
                            .cloned()
                            .zip(parts.into_iter().map(Value::Text)),
                    );
                }
                Step::Bind(name, part, as_) => {
                    let Value::Text(t) = lookup(&env, part) else {
                        unreachable!("typed: a text");
                    };
                    let v = match as_ {
                        BindAs::Hex => hex_bytes(t).map(Value::Bytes),
                        BindAs::Integer | BindAs::Time => integer_text(t).map(Value::Int),
                    };
                    let Some(v) = v else {
                        return out(Some("shape"));
                    };
                    env.push((name.clone(), v));
                }
            }
        }
        for (label, e) in &self.requires {
            match predicate_eval(e, &env) {
                Ok(Value::Bool(true)) => {}
                Ok(_) => return out(Some(label)),
                Err(NotBuilt) => return Answer::Unsupported(
                    "the law calls hmac_sha256; build alice-lol with the `crypto-calls` feature"
                        .to_owned(),
                ),
            }
        }
        out(None)
    }

    fn law_id(&self) -> [u8; 32] {
        let mut h = hash_consts(
            LawIdHasher::new(PREDICATE_LAW_KIND, &RULE_SEMANTICS_ID),
            &self.consts,
        );
        h = h.str("use hmac_sha256").u32(u32::from(self.uses_hmac));
        h = hash_inputs(h, &self.inputs);
        h = h.str("steps").u64(self.steps.len() as u64);
        for s in &self.steps {
            h = match s {
                Step::Split(src, sep, names) => {
                    let mut h = h.str("split").str(src).str(sep).u64(names.len() as u64);
                    for n in names {
                        h = h.str(n);
                    }
                    h
                }
                Step::Bind(n, p, as_) => h.str("bind").str(n).str(p).str(match as_ {
                    BindAs::Integer => "integer",
                    BindAs::Time => "time",
                    BindAs::Hex => "hex",
                }),
            };
        }
        h = h.str("requires").u64(self.requires.len() as u64);
        for (label, e) in &self.requires {
            h = e.hash(h.str(label));
        }
        h.finish()
    }
}

impl TableLaw {
    fn respond(&self, inp: &Json) -> Answer {
        let env = match read_inputs(&self.inputs, inp) {
            Ok(env) => env,
            Err(a) => return a,
        };
        let mut answer = (&self.otherwise, None);
        for (label, value, e) in &self.rows {
            // a table has no call, so every expression evaluates
            if matches!(predicate_eval(e, &env), Ok(Value::Bool(true))) {
                answer = (value, Some(label));
                break;
            }
        }
        Answer::Outputs(Json::Object(vec![
            ("value".to_owned(), Json::Text(answer.0.clone())),
            (
                "row".to_owned(),
                answer.1.map_or(Json::Null, |r| Json::Text(r.clone())),
            ),
        ]))
    }

    fn law_id(&self) -> [u8; 32] {
        let mut h = hash_consts(
            LawIdHasher::new(TABLE_LAW_KIND, &TABLE_SEMANTICS_ID),
            &self.consts,
        );
        h = hash_inputs(h, &self.inputs);
        h = h.str("values").u64(self.values.len() as u64);
        for v in &self.values {
            h = h.str(v);
        }
        h = h.str("rows").u64(self.rows.len() as u64);
        for (label, value, e) in &self.rows {
            h = e.hash(h.str(label).str(value));
        }
        h.str("otherwise").str(&self.otherwise).finish()
    }
}

// ------------------------------------------------------------------ semantics pins

fn digest_answers(law: &str, requests: &[&str]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let law = rule_law_from_file(law).expect("the fingerprint law reads");
    let mut h = Sha256::new();
    for r in requests {
        let a = law.respond(&crate::law_input::parse_json(r).expect("fingerprint JSON"));
        h.update(format!("{r} -> {a:?}\n").as_bytes());
    }
    h.finalize().into()
}

/// The measured value of every pin of [`RULE_SEMANTICS_PINS`] this build can measure (the call
/// pin needs `crypto-calls`)
#[must_use]
pub fn rule_semantics_fingerprints() -> Vec<(&'static str, [u8; 32])> {
    let machine = digest_answers(
        "kind machine\nmode m n\nstate a initial\nstate b\nevent go guard ok\nevent back\n\
         on go from a to b when ok\non go from a b to a\non back from b to a\n\
         allow here when state == b or mode == n\n",
        &[
            r#"{"mode": "m", "state": "a", "event": "go", "guards": {"ok": true}}"#,
            r#"{"mode": "m", "state": "a", "event": "go", "guards": {"ok": false}}"#,
            r#"{"mode": "m", "state": "a", "event": "back"}"#,
            r#"{"mode": "n", "state": "b", "event": "back"}"#,
            r#"{"mode": "m", "state": "b", "event": "go", "guards": {"ok": true}}"#,
            r#"{"mode": "m", "state": "a", "event": "go"}"#,
            r#"{"mode": "x", "state": "a", "event": "back"}"#,
            r#"{"state": "a", "event": "back"}"#,
        ],
    );
    let predicate = digest_answers(
        "kind predicate\nx-input s text\nx-input now time\nx-split s \".\" a b\nx-bind t b time\n\
         require named: len(a) > 0\nrequire before: now < t\nrequire short: prefix(utf8(a), 1) == 0x78\n\
         example {\"s\": \"x.10\", \"now\": 9}\n",
        &[
            r#"{"s": "x.10", "now": 9}"#,
            r#"{"s": "x.10", "now": 10}"#,
            r#"{"s": ".10", "now": 10}"#,
            r#"{"s": "y.10", "now": 10}"#,
            r#"{"s": "x.010", "now": 9}"#,
            r#"{"s": "x.1.0", "now": 9}"#,
            r#"{"s": 1, "now": 9}"#,
            r#"{"s": "x.10"}"#,
        ],
    );
    let inputs = digest_answers(
        "kind predicate\nx-input i integer\nx-input b bytes\nx-input r record(f: bool)\n\
         require ok: true\nexample {\"i\": 1, \"b\": \"\", \"r\": {\"f\": true}}\n",
        &[
            r#"{"i": 9007199254740991, "b": "00ff", "r": {"f": false, "g": 1}}"#,
            r#"{"i": 9007199254740992, "b": "", "r": {"f": true}}"#,
            r#"{"i": 3.0, "b": "", "r": {"f": true}}"#,
            r#"{"i": 1, "b": "0F", "r": {"f": true}}"#,
            r#"{"i": 1, "b": "0", "r": {"f": true}}"#,
            r#"{"i": 1, "b": "", "r": {"f": 1}}"#,
            r#"{"i": 1, "b": "", "r": {}}"#,
            r#"{"i": true, "b": "", "r": {"f": true}}"#,
        ],
    );
    #[allow(unused_mut)]
    let mut out = vec![
        (RULE_SEMANTICS_PINS[0].0, machine),
        (RULE_SEMANTICS_PINS[1].0, predicate),
        (RULE_SEMANTICS_PINS[2].0, inputs),
    ];
    #[cfg(feature = "crypto-calls")]
    {
        use sha2::{Digest, Sha256};
        // RFC 4231 test cases 1-4 (key, data)
        let cases: [(Vec<u8>, Vec<u8>); 4] = [
            (vec![0x0b; 20], b"Hi There".to_vec()),
            (b"Jefe".to_vec(), b"what do ya want for nothing?".to_vec()),
            (vec![0xaa; 20], vec![0xdd; 50]),
            ((1..=25).collect(), vec![0xcd; 50]),
        ];
        let mut h = Sha256::new();
        for (k, d) in &cases {
            h.update(alice_sign::mac::hmac_sha256(k, d));
        }
        out.push((RULE_SEMANTICS_PINS[3].0, h.finalize().into()));
    }
    out
}

/// The measured value of every pin of [`TABLE_SEMANTICS_PINS`]
#[must_use]
pub fn table_semantics_fingerprints() -> Vec<(&'static str, [u8; 32])> {
    let rows = digest_answers(
        "kind table\nx-input n integer\nx-input t text\nconst k integer = 3\nvalue a b c\n\
         row big: a when n > k\nrow named: b when t == \"x\" or n == k\nrow also: a when n > 0\n\
         otherwise c\n",
        &[
            r#"{"n": 4, "t": "x"}"#,
            r#"{"n": 3, "t": "y"}"#,
            r#"{"n": 1, "t": "x"}"#,
            r#"{"n": 1, "t": "y"}"#,
            r#"{"n": 0, "t": "y"}"#,
            r#"{"n": 2.0, "t": "y"}"#,
            r#"{"n": 1.5, "t": "y"}"#,
            r#"{"n": "1", "t": 1}"#,
            r#"{"t": "x"}"#,
        ],
    );
    vec![
        (TABLE_SEMANTICS_PINS[0].0, rows),
        (TABLE_SEMANTICS_PINS[1].0, RULE_SEMANTICS_ID),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every token reads at least one byte, so a text of n bytes gives at most n tokens; a
    /// reading that stops advancing ends as an error after n steps instead of looping or
    /// growing its output
    #[test]
    fn tokens_are_bounded_by_the_input_length() {
        let cases = [
            ("a", 1),
            ("  a  ", 1),
            ("(a or b) and c", 7),
            ("0x0102 ++ k == k", 5),
            ("n >= 10", 3),
            ("\"t\" == x", 3),
            ("f(a, b).c", 8),
            ("((()))", 6),
            ("   \t ", 0),
        ];
        for (src, count) in cases {
            let t = tokens(src).unwrap_or_else(|e| panic!("{src}: {e}"));
            assert_eq!(t.len(), count, "{src}: {t:?}");
            assert!(t.len() <= src.len(), "{src}");
        }
        let long = "(a or b) ".repeat(4096);
        assert_eq!(tokens(&long).map(|t| t.len()), Ok(5 * 4096));
        for src in ["0x0", "01", "\"open", "a $ b", "A", "a\\b"] {
            assert!(tokens(src).is_err(), "{src}");
        }
    }
}
