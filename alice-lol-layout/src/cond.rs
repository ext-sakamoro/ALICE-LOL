//! Conditions of `refuse` and `status` rules
//!
//! ```text
//! cond  := conj ("or" conj)*
//! conj  := unary ("and" unary)*
//! unary := "not" unary | atom
//! atom  := ("any" | "none") "(" <table> ":" cond ")"
//!        | term ("==" | "!=" | "<" | "<=" | ">" | ">=") term
//!        | term "bit" <n>
//!        | term ("in" | "not-in") <tags constant>
//! term  := <integer> | "<text>" | <constant> | <derive>
//!        | <segment>.<field> | <table>[<i>].<field> | <data>[<i>] | <bytes segment>
//!        | "count" "(" <table> ":" cond ")" | "len" <bytes term>
//! ```
//!
//! Inside `any` / `none` / `count`, a bare field name is the field of the row being tested.
//! A term that names something absent (a row past the end) makes its atom false: `not` of
//! such an atom is true, so `x != v` and `not x == v` differ when `x` is absent.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::decl::{
    err, is_ident, parse_int, Const, FieldType, Layout, LayoutError, SegmentKind, Tok,
};

/// Deepest nesting of a condition
pub const MAX_DEPTH: usize = 16;
/// Most atoms in one condition
pub const MAX_ATOMS: usize = 64;

/// A comparison operator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
}

/// A term of a condition
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// an integer literal
    Int(u64),
    /// a text literal or a bytes / text constant
    Bytes(Vec<u8>),
    /// a derived value
    Derive(String),
    /// a field of a block segment
    Field {
        /// the block segment
        segment: String,
        /// the field
        field: String,
    },
    /// a field of a table row (`row = None`: the row being tested)
    RowField {
        /// the table
        table: String,
        /// the row
        row: Option<u64>,
        /// the field
        field: String,
    },
    /// the run of one row of a data segment
    Data {
        /// the data segment
        segment: String,
        /// the row
        row: u64,
    },
    /// the bytes of a `bytes` segment
    Segment(String),
    /// the rows of a table for which a condition holds
    Count {
        /// the table
        table: String,
        /// tested per row
        cond: Box<Cond>,
    },
    /// the length of a bytes term
    Len(Box<Self>),
}

/// A condition
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cond {
    /// all hold
    And(Vec<Self>),
    /// one holds
    Or(Vec<Self>),
    /// does not hold
    Not(Box<Self>),
    /// a comparison
    Cmp(Term, Op, Term),
    /// bit `n` of an integer is set
    Bit(Term, u32),
    /// a 4-byte term is (or is not) one of the tags of a constant
    In {
        /// the term
        term: Term,
        /// the tags
        tags: Vec<[u8; 4]>,
        /// `not-in`
        negated: bool,
    },
    /// some row (or no row, `negated`) of a table satisfies a condition
    Any {
        /// the table
        table: String,
        /// tested per row
        cond: Box<Self>,
        /// `none`
        negated: bool,
    },
}

/// A value while a condition is evaluated
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Val<'a> {
    /// an integer
    Int(u64),
    /// bytes
    Bytes(Cow<'a, [u8]>),
}

/// What a condition reads (implemented by the decoder over decoded input)
pub trait Scope<'a> {
    /// a field of a block segment
    fn field(&self, segment: &str, field: &str) -> Option<Val<'a>>;
    /// a field of a table row
    fn row_field(&self, table: &str, row: usize, field: &str) -> Option<Val<'a>>;
    /// the number of rows of a table
    fn rows(&self, table: &str) -> usize;
    /// the run of a row of a data segment
    fn data(&self, segment: &str, row: usize) -> Option<&'a [u8]>;
    /// the bytes of a bytes segment
    fn segment(&self, segment: &str) -> Option<&'a [u8]>;
    /// a derived value
    fn derive(&self, name: &str) -> Option<Vec<u8>>;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ty {
    Int,
    Bytes,
}

fn split_tokens(toks: &[Tok]) -> Vec<Tok> {
    let mut out = Vec::new();
    for t in toks {
        match t {
            Tok::Text(_) => out.push(t.clone()),
            Tok::Word(w) => {
                let mut cur = String::new();
                for c in w.chars() {
                    if c == '(' || c == ')' || c == ':' {
                        if !cur.is_empty() {
                            out.push(Tok::Word(core::mem::take(&mut cur)));
                        }
                        out.push(Tok::Word(c.to_string()));
                    } else {
                        cur.push(c);
                    }
                }
                if !cur.is_empty() {
                    out.push(Tok::Word(cur));
                }
            }
        }
    }
    out
}

struct P<'l> {
    toks: Vec<Tok>,
    i: usize,
    layout: &'l Layout,
    line: usize,
    atoms: usize,
    /// the table whose row a bare field name refers to
    row_table: Option<String>,
}

/// Reads a condition (the tokens after `when`)
pub(crate) fn parse_cond(toks: &[Tok], layout: &Layout, line: usize) -> Result<Cond, LayoutError> {
    let mut p = P {
        toks: split_tokens(toks),
        i: 0,
        layout,
        line,
        atoms: 0,
        row_table: None,
    };
    let c = p.or(0)?;
    if p.i != p.toks.len() {
        return Err(err(line, "unexpected words after the condition"));
    }
    Ok(c)
}

impl P<'_> {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.i).and_then(Tok::word)
    }

    fn next_word(&mut self) -> Result<String, LayoutError> {
        let w = self
            .toks
            .get(self.i)
            .and_then(Tok::word)
            .ok_or_else(|| err(self.line, "the condition ends early"))?
            .to_string();
        self.i += 1;
        Ok(w)
    }

    fn expect(&mut self, w: &str) -> Result<(), LayoutError> {
        if self.peek() == Some(w) {
            self.i += 1;
            Ok(())
        } else {
            Err(err(self.line, format!("expected `{w}`")))
        }
    }

    fn or(&mut self, depth: usize) -> Result<Cond, LayoutError> {
        if depth > MAX_DEPTH {
            return Err(err(self.line, "a condition is nested too deeply"));
        }
        let mut parts = Vec::from([self.and(depth)?]);
        while self.peek() == Some("or") {
            self.i += 1;
            parts.push(self.and(depth)?);
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Cond::Or(parts)
        })
    }

    fn and(&mut self, depth: usize) -> Result<Cond, LayoutError> {
        let mut parts = Vec::from([self.unary(depth)?]);
        while self.peek() == Some("and") {
            self.i += 1;
            parts.push(self.unary(depth)?);
        }
        Ok(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Cond::And(parts)
        })
    }

    fn unary(&mut self, depth: usize) -> Result<Cond, LayoutError> {
        if self.peek() == Some("not") {
            self.i += 1;
            if depth + 1 > MAX_DEPTH {
                return Err(err(self.line, "a condition is nested too deeply"));
            }
            return Ok(Cond::Not(Box::new(self.unary(depth + 1)?)));
        }
        self.atoms += 1;
        if self.atoms > MAX_ATOMS {
            return Err(err(self.line, "a condition has too many parts"));
        }
        if let Some(q @ ("any" | "none")) = self.peek() {
            let negated = q == "none";
            self.i += 1;
            let (table, cond) = self.quantified(depth)?;
            return Ok(Cond::Any {
                table,
                cond: Box::new(cond),
                negated,
            });
        }
        let (left, lt) = self.term(depth)?;
        let op = self.next_word()?;
        match op.as_str() {
            "bit" => {
                if lt != Ty::Int {
                    return Err(err(self.line, "`bit` tests an integer"));
                }
                let n = parse_int(&self.next_word()?)
                    .filter(|&n| n < 64)
                    .ok_or_else(|| err(self.line, "`bit <0..63>`"))?;
                Ok(Cond::Bit(left, u32::try_from(n).unwrap_or(0)))
            }
            "in" | "not-in" => {
                let c = self.next_word()?;
                let Some(Const::Tags(tags)) = self.layout.constant(&c) else {
                    return Err(err(self.line, format!("`{c}` is not a tags constant")));
                };
                if lt != Ty::Bytes {
                    return Err(err(self.line, "`in` tests a 4-byte value"));
                }
                Ok(Cond::In {
                    term: left,
                    tags: tags.clone(),
                    negated: op == "not-in",
                })
            }
            _ => {
                let op = match op.as_str() {
                    "==" => Op::Eq,
                    "!=" => Op::Ne,
                    "<" => Op::Lt,
                    "<=" => Op::Le,
                    ">" => Op::Gt,
                    ">=" => Op::Ge,
                    other => return Err(err(self.line, format!("unknown comparison `{other}`"))),
                };
                let (right, rt) = self.term(depth)?;
                if lt != rt {
                    return Err(err(
                        self.line,
                        "the two sides of a comparison have different types",
                    ));
                }
                if lt == Ty::Bytes && !matches!(op, Op::Eq | Op::Ne) {
                    return Err(err(self.line, "bytes are compared with `==` or `!=` only"));
                }
                Ok(Cond::Cmp(left, op, right))
            }
        }
    }

    fn quantified(&mut self, depth: usize) -> Result<(String, Cond), LayoutError> {
        self.expect("(")?;
        let table = self.next_word()?;
        if !matches!(
            self.layout.segment(&table).map(|s| &s.kind),
            Some(SegmentKind::Table { .. })
        ) {
            return Err(err(self.line, format!("`{table}` is not a table segment")));
        }
        if self.row_table.is_some() {
            return Err(err(self.line, "a row condition does not nest another"));
        }
        self.expect(":")?;
        self.row_table = Some(table.clone());
        let c = self.or(depth + 1);
        self.row_table = None;
        let c = c?;
        self.expect(")")?;
        Ok((table, c))
    }

    fn term(&mut self, depth: usize) -> Result<(Term, Ty), LayoutError> {
        if let Some(Tok::Text(t)) = self.toks.get(self.i) {
            let b = t.as_bytes().to_vec();
            self.i += 1;
            return Ok((Term::Bytes(b), Ty::Bytes));
        }
        let w = self.next_word()?;
        // inside a row condition a field of the row wins over the keywords (`len` is a likely
        // field name)
        if self.row_table.is_some() {
            if let Ok(found) = self.row_field(&w) {
                return Ok(found);
            }
        }
        if w == "count" {
            let (table, cond) = self.quantified(depth)?;
            return Ok((
                Term::Count {
                    table,
                    cond: Box::new(cond),
                },
                Ty::Int,
            ));
        }
        if w == "len" {
            let (t, ty) = self.term(depth)?;
            if ty != Ty::Bytes {
                return Err(err(self.line, "`len` takes bytes"));
            }
            return Ok((Term::Len(Box::new(t)), Ty::Int));
        }
        if let Some(v) = parse_int(&w) {
            return Ok((Term::Int(v), Ty::Int));
        }
        if let Some(c) = self.layout.constant(&w) {
            return match c {
                Const::Integer(v) => Ok((Term::Int(*v), Ty::Int)),
                Const::Bytes(b) => Ok((Term::Bytes(b.clone()), Ty::Bytes)),
                Const::Text(t) => Ok((Term::Bytes(t.as_bytes().to_vec()), Ty::Bytes)),
                Const::Tags(_) => Err(err(self.line, "a tags constant is used with `in`")),
            };
        }
        if self.layout.derives.iter().any(|d| d.name == w) {
            return Ok((Term::Derive(w), Ty::Bytes));
        }
        self.reference(&w)
    }

    fn field_ty(&self, block: &str, field: &str) -> Result<Ty, LayoutError> {
        let b = self
            .layout
            .block(block)
            .ok_or_else(|| err(self.line, "internal: a segment without its block"))?;
        let f = b
            .fields
            .iter()
            .find(|f| f.name == field)
            .ok_or_else(|| err(self.line, format!("`{block}` has no field `{field}`")))?;
        Ok(match f.ty {
            FieldType::Uint { .. } => Ty::Int,
            FieldType::Bytes(_) => Ty::Bytes,
        })
    }

    /// A bare field of the row being tested
    fn row_field(&self, w: &str) -> Result<(Term, Ty), LayoutError> {
        let t = self.row_table.as_ref().filter(|_| is_ident(w));
        if let Some(t) = t {
            if let Some(SegmentKind::Table { block, .. }) = self.layout.segment(t).map(|s| &s.kind)
            {
                let ty = self.field_ty(block, w)?;
                return Ok((
                    Term::RowField {
                        table: t.clone(),
                        row: None,
                        field: w.to_string(),
                    },
                    ty,
                ));
            }
        }
        Err(err(self.line, format!("`{w}` is not a field of the row")))
    }

    fn reference(&self, w: &str) -> Result<(Term, Ty), LayoutError> {
        let (head, field) = match w.split_once('.') {
            Some((h, f)) => (h, Some(f)),
            None => (w, None),
        };
        let (name, row) = match head.split_once('[') {
            Some((n, r)) => {
                let r = r
                    .strip_suffix(']')
                    .and_then(parse_int)
                    .ok_or_else(|| err(self.line, format!("`{w}`: a row is `[<n>]`")))?;
                (n, Some(r))
            }
            None => (head, None),
        };
        let kind = self
            .layout
            .segment(name)
            .map(|s| &s.kind)
            .ok_or_else(|| err(self.line, format!("unknown name `{name}`")))?;
        match (kind, row, field) {
            (SegmentKind::Block(b), None, Some(f)) => Ok((
                Term::Field {
                    segment: name.to_string(),
                    field: f.to_string(),
                },
                self.field_ty(b, f)?,
            )),
            (SegmentKind::Table { block, .. }, Some(r), Some(f)) => Ok((
                Term::RowField {
                    table: name.to_string(),
                    row: Some(r),
                    field: f.to_string(),
                },
                self.field_ty(block, f)?,
            )),
            (SegmentKind::Data { .. }, Some(r), None) => Ok((
                Term::Data {
                    segment: name.to_string(),
                    row: r,
                },
                Ty::Bytes,
            )),
            (SegmentKind::Bytes(_), None, None) => Ok((Term::Segment(name.to_string()), Ty::Bytes)),
            _ => Err(err(self.line, format!("`{w}` does not name a value"))),
        }
    }
}

// ─── evaluation ────────────────────────────────────────────────────────────

fn idx(r: u64) -> usize {
    usize::try_from(r).unwrap_or(usize::MAX)
}

fn value<'a>(t: &Term, s: &dyn Scope<'a>, row: Option<(&str, usize)>) -> Option<Val<'a>> {
    match t {
        Term::Int(v) => Some(Val::Int(*v)),
        Term::Bytes(b) => Some(Val::Bytes(Cow::Owned(b.clone()))),
        Term::Derive(d) => s.derive(d).map(|b| Val::Bytes(Cow::Owned(b))),
        Term::Field { segment, field } => s.field(segment, field),
        Term::RowField {
            table,
            row: r,
            field,
        } => {
            let i = match r {
                Some(r) => idx(*r),
                None => row.filter(|(t, _)| t == table).map(|(_, i)| i)?,
            };
            s.row_field(table, i, field)
        }
        Term::Data { segment, row: r } => s
            .data(segment, idx(*r))
            .map(|b| Val::Bytes(Cow::Borrowed(b))),
        Term::Segment(n) => s.segment(n).map(|b| Val::Bytes(Cow::Borrowed(b))),
        Term::Count { table, cond } => {
            let n = (0..s.rows(table))
                .filter(|&i| holds(cond, s, Some((table, i))))
                .count();
            Some(Val::Int(n as u64))
        }
        Term::Len(t) => match value(t, s, row)? {
            Val::Bytes(b) => Some(Val::Int(b.len() as u64)),
            Val::Int(_) => None,
        },
    }
}

/// Whether a condition holds over decoded input (`row`: the row a bare field refers to)
#[allow(clippy::many_single_char_names)] // the comparison sides x / y, as in the grammar
pub fn holds(c: &Cond, s: &dyn Scope<'_>, row: Option<(&str, usize)>) -> bool {
    match c {
        Cond::And(cs) => cs.iter().all(|c| holds(c, s, row)),
        Cond::Or(cs) => cs.iter().any(|c| holds(c, s, row)),
        Cond::Not(c) => !holds(c, s, row),
        Cond::Cmp(a, op, b) => match (value(a, s, row), value(b, s, row)) {
            (Some(Val::Int(x)), Some(Val::Int(y))) => match op {
                Op::Eq => x == y,
                Op::Ne => x != y,
                Op::Lt => x < y,
                Op::Le => x <= y,
                Op::Gt => x > y,
                Op::Ge => x >= y,
            },
            (Some(Val::Bytes(x)), Some(Val::Bytes(y))) => match op {
                Op::Eq => x == y,
                Op::Ne => x != y,
                _ => false,
            },
            _ => false,
        },
        Cond::Bit(t, n) => matches!(value(t, s, row), Some(Val::Int(v)) if v >> n & 1 == 1),
        Cond::In {
            term,
            tags,
            negated,
        } => match value(term, s, row) {
            Some(Val::Bytes(b)) => tags.iter().any(|t| t[..] == b[..]) != *negated,
            _ => false,
        },
        Cond::Any {
            table,
            cond,
            negated,
        } => (0..s.rows(table)).any(|i| holds(cond, s, Some((table, i)))) != *negated,
    }
}
