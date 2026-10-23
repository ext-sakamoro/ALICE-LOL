//! Reading bytes by a declaration
//!
//! Every length and count is checked against the bytes that remain before it is used, with
//! checked arithmetic, and nothing is allocated from a count before that check and the
//! [`Budget`] allow it. The input is borrowed: a decoded value is a range of it.

use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use sha2::{Digest as _, Sha256};

use crate::cond::{holds, Scope, Val};
use crate::decl::{Block, Const, DigestOver, Endian, FieldType, Layout, Len, Rule, SegmentKind};
use crate::id::derive_value;

/// What a decode may allocate
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// most table rows and stream chunks, all segments together
    pub max_rows: u64,
    /// most bytes of decoded values (rows × fields × the size of one value)
    pub max_bytes: u64,
}

impl Budget {
    /// The budget for an input: a row takes at least one byte, so `len` rows; at most 64 MiB
    /// of decoded values
    #[must_use]
    pub const fn for_input(len: usize) -> Self {
        Self {
            max_rows: len as u64,
            max_bytes: 64 << 20,
        }
    }
}

/// Why a check refused the input
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// shorter than `minimum`
    TooShort,
    /// a segment or field needs more bytes than remain
    Truncated {
        /// the bytes it needs (saturated at `u64::MAX`)
        need: u64,
    },
    /// a length or count does not fit the arithmetic
    Overflow,
    /// a field holds another value than `=` says
    Value,
    /// a field has a bit set that `bits` does not allow
    Bits,
    /// a field is outside its `in` range
    Range,
    /// a run of a data segment is not where its offset field says
    Placement,
    /// bytes remain after the last segment
    Trailing,
    /// a digest does not hold (`row` for a data digest)
    Digest {
        /// the row
        row: Option<u64>,
    },
    /// a stream ends right after a full chunk, or has more chunks than its `max`
    Chunks,
    /// a `refuse` rule holds
    Rule,
}

/// The input does not read
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// a check refused it: `label` is the declared class, `at` the byte offset
    Refused {
        /// the label of the check (the class a consumer maps to its own error)
        label: String,
        /// the byte offset of the check (0 for rules)
        at: u64,
        /// why
        reason: Reason,
    },
    /// reading it would allocate more than the [`Budget`] allows
    BudgetExceeded {
        /// what would be allocated (`rows` / `bytes`)
        what: &'static str,
        /// how much
        need: u64,
        /// the budget
        limit: u64,
    },
}

impl DecodeError {
    /// The label of a refusal
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Refused { label, .. } => Some(label),
            Self::BudgetExceeded { .. } => None,
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused { label, at, reason } => {
                write!(f, "refused `{label}` at byte {at}: {reason:?}")
            }
            Self::BudgetExceeded { what, need, limit } => {
                write!(f, "would allocate {need} {what}, the budget is {limit}")
            }
        }
    }
}

/// A decoded field value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldValue<'a> {
    /// an integer
    Int(u64),
    /// bytes of the input
    Bytes(&'a [u8]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FVal {
    Int(u64),
    Bytes(usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SegVal {
    Block(Vec<FVal>),
    Table(Vec<Vec<FVal>>),
    Data(Vec<(usize, usize)>),
    Bytes,
    Stream(Vec<(usize, usize)>),
}

/// An input read by a layout
#[derive(Debug, Clone)]
pub struct Decoded<'a, 'l> {
    layout: &'l Layout,
    input: &'a [u8],
    /// per segment of the layout: its range and its values
    segs: Vec<((usize, usize), SegVal)>,
    status: Option<String>,
}

fn refused(label: &str, at: usize, reason: Reason) -> DecodeError {
    DecodeError::Refused {
        label: label.to_string(),
        at: at as u64,
        reason,
    }
}

fn read_uint(b: &[u8], endian: Endian) -> u64 {
    let mut v = 0u64;
    match endian {
        Endian::Big => {
            for &x in b {
                v = v << 8 | u64::from(x);
            }
        }
        Endian::Little => {
            for &x in b.iter().rev() {
                v = v << 8 | u64::from(x);
            }
        }
    }
    v
}

fn to_usize(v: u64) -> Option<usize> {
    usize::try_from(v).ok()
}

struct Dec<'a, 'l> {
    layout: &'l Layout,
    input: &'a [u8],
    budget: Budget,
    rows_used: u64,
    bytes_used: u64,
    segs: Vec<((usize, usize), SegVal)>,
}

/// The runs of a data segment that directly follows a table of fixed-size rows: where they
/// start is known before the rows are read, so each row's placement is checked as soon as the
/// row is read (in the order a reader that walks the table checks it)
struct FollowingRuns<'l> {
    label: &'l str,
    length: usize,
    offset: Option<usize>,
    at: usize,
}

impl FollowingRuns<'_> {
    fn check(&mut self, row: &[FVal], limit: usize) -> Result<(), DecodeError> {
        if let Some(oi) = self.offset {
            if row.get(oi) != Some(&FVal::Int(self.at as u64)) {
                return Err(refused(self.label, self.at, Reason::Placement));
            }
        }
        let Some(FVal::Int(len)) = row.get(self.length) else {
            return Err(refused(self.label, self.at, Reason::Overflow));
        };
        self.at = to_usize(*len)
            .and_then(|l| self.at.checked_add(l))
            .filter(|&e| e <= limit)
            .ok_or_else(|| refused(self.label, self.at, Reason::Truncated { need: *len }))?;
        Ok(())
    }
}

/// [`FollowingRuns`] for table segment `i` of `n` rows of `blk` starting at `cursor`, when the
/// next segment is its data segment and the rows have a fixed size
fn following_runs<'l>(
    layout: &'l Layout,
    i: usize,
    blk: &Block,
    n: u64,
    cursor: usize,
) -> Option<FollowingRuns<'l>> {
    let next = layout.segments.get(i + 1)?;
    let SegmentKind::Data {
        table,
        length,
        offset,
    } = &next.kind
    else {
        return None;
    };
    if *table != layout.segments[i].name || next.at_end {
        return None;
    }
    let at = to_usize(n.checked_mul(blk.size?)?)?.checked_add(cursor)?;
    Some(FollowingRuns {
        label: &next.label,
        length: blk.fields.iter().position(|f| f.name == *length)?,
        offset: match offset {
            Some(o) => Some(blk.fields.iter().position(|f| f.name == *o)?),
            None => None,
        },
        at,
    })
}

impl Dec<'_, '_> {
    const fn spend(&mut self, rows: u64, values: u64) -> Result<(), DecodeError> {
        let r = self.rows_used.saturating_add(rows);
        if r > self.budget.max_rows {
            return Err(DecodeError::BudgetExceeded {
                what: "rows",
                need: r,
                limit: self.budget.max_rows,
            });
        }
        let b = self
            .bytes_used
            .saturating_add(values.saturating_mul(core::mem::size_of::<FVal>() as u64));
        if b > self.budget.max_bytes {
            return Err(DecodeError::BudgetExceeded {
                what: "bytes",
                need: b,
                limit: self.budget.max_bytes,
            });
        }
        self.rows_used = r;
        self.bytes_used = b;
        Ok(())
    }

    /// The integer value of `<segment>.<field>` (a block segment read earlier)
    fn ref_int(&self, segment: &str, field: &str) -> Option<u64> {
        let i = self
            .layout
            .segments
            .iter()
            .position(|s| s.name == segment)?;
        let SegmentKind::Block(b) = &self.layout.segments[i].kind else {
            return None;
        };
        let fi = self
            .layout
            .block(b)?
            .fields
            .iter()
            .position(|f| f.name == field)?;
        match self.segs.get(i)? {
            (_, SegVal::Block(v)) => match v.get(fi)? {
                FVal::Int(x) => Some(*x),
                FVal::Bytes(..) => None,
            },
            _ => None,
        }
    }

    fn len_value(&self, len: &Len, cursor: usize, limit: usize) -> Option<u64> {
        match len {
            Len::Fixed(n) => Some(*n),
            Len::Field(r) => self.ref_int(r.segment.as_deref()?, &r.field),
            Len::Rest => Some((limit - cursor) as u64),
        }
    }

    /// One instance of a block at `cursor`; returns its values and its end
    fn block(
        &self,
        block: &str,
        seg_label: &str,
        cursor: usize,
        limit: usize,
    ) -> Result<(Vec<FVal>, usize), DecodeError> {
        let blk = self
            .layout
            .block(block)
            .ok_or_else(|| refused(seg_label, cursor, Reason::Overflow))?;
        let mut vals = Vec::with_capacity(blk.fields.len());
        let mut at = cursor;
        for f in &blk.fields {
            let width = match &f.ty {
                FieldType::Uint { width, .. } => u64::from(*width),
                FieldType::Bytes(Len::Fixed(n)) => *n,
                FieldType::Bytes(Len::Field(r)) => {
                    let fi = blk
                        .fields
                        .iter()
                        .position(|g| g.name == r.field)
                        .unwrap_or(0);
                    match vals.get(fi) {
                        Some(FVal::Int(v)) => *v,
                        _ => return Err(refused(seg_label, at, Reason::Overflow)),
                    }
                }
                FieldType::Bytes(Len::Rest) => (limit - at) as u64,
            };
            let end = to_usize(width)
                .and_then(|w| at.checked_add(w))
                .filter(|&e| e <= limit)
                .ok_or_else(|| refused(seg_label, at, Reason::Truncated { need: width }))?;
            let v = match &f.ty {
                FieldType::Uint { endian, .. } => FVal::Int(read_uint(
                    &self.input[at..end],
                    endian.unwrap_or(self.layout.endian),
                )),
                FieldType::Bytes(_) => FVal::Bytes(at, end),
            };
            if let Some(eq) = &f.equals {
                let ok = match (eq, v) {
                    (crate::decl::Value::Integer(x), FVal::Int(y)) => *x == y,
                    (crate::decl::Value::Bytes(x), FVal::Bytes(s, e)) => x[..] == self.input[s..e],
                    _ => false,
                };
                if !ok {
                    return Err(refused(&f.label, at, Reason::Value));
                }
            }
            if let (Some(mask), FVal::Int(y)) = (f.bits, v) {
                if y & !mask != 0 {
                    return Err(refused(&f.label, at, Reason::Bits));
                }
            }
            if let (Some((lo, hi)), FVal::Int(y)) = (f.range, v) {
                if y < lo || y > hi {
                    return Err(refused(&f.label, at, Reason::Range));
                }
            }
            vals.push(v);
            at = end;
        }
        Ok((vals, at))
    }

    #[allow(clippy::too_many_lines)] // one arm per segment kind, each bounded before it reads
    fn segment(&mut self, i: usize, cursor: usize, limit: usize) -> Result<usize, DecodeError> {
        let s = &self.layout.segments[i];
        let label = s.label.as_str();
        let (val, end) = match &s.kind {
            SegmentKind::Block(b) => {
                let fields = self.layout.block(b).map_or(0, |b| b.fields.len()) as u64;
                self.spend(0, fields)?;
                let (v, end) = self.block(b, label, cursor, limit)?;
                (SegVal::Block(v), end)
            }
            SegmentKind::Table { block, count } => {
                let n = self
                    .ref_int(count.segment.as_deref().unwrap_or(""), &count.field)
                    .ok_or_else(|| refused(label, cursor, Reason::Overflow))?;
                let blk = self
                    .layout
                    .block(block)
                    .ok_or_else(|| refused(label, cursor, Reason::Overflow))?;
                let min = blk.min_size();
                let fields = blk.fields.len() as u64;
                // bounded by the bytes that remain before anything is allocated
                let room = (limit - cursor) as u64;
                if n.checked_mul(min).is_none_or(|need| need > room) {
                    return Err(refused(
                        label,
                        cursor,
                        Reason::Truncated {
                            need: n.saturating_mul(min),
                        },
                    ));
                }
                self.spend(n, n.saturating_mul(fields))?;
                let mut follow = following_runs(self.layout, i, blk, n, cursor);
                let mut rows = Vec::with_capacity(to_usize(n).unwrap_or(0));
                let mut at = cursor;
                for _ in 0..n {
                    let (v, end) = self.block(block, label, at, limit)?;
                    if let Some(f) = follow.as_mut() {
                        f.check(&v, limit)?;
                    }
                    rows.push(v);
                    at = end;
                }
                (SegVal::Table(rows), at)
            }
            SegmentKind::Data {
                table,
                length,
                offset,
            } => {
                let ti = self
                    .layout
                    .segments
                    .iter()
                    .position(|x| x.name == *table)
                    .ok_or_else(|| refused(label, cursor, Reason::Overflow))?;
                let SegmentKind::Table { block, .. } = &self.layout.segments[ti].kind else {
                    return Err(refused(label, cursor, Reason::Overflow));
                };
                let blk = self
                    .layout
                    .block(block)
                    .ok_or_else(|| refused(label, cursor, Reason::Overflow))?;
                let li = blk.fields.iter().position(|f| f.name == *length);
                let oi = offset
                    .as_ref()
                    .and_then(|o| blk.fields.iter().position(|f| f.name == *o));
                let SegVal::Table(rows) = &self.segs[ti].1 else {
                    return Err(refused(label, cursor, Reason::Overflow));
                };
                let mut runs = Vec::with_capacity(rows.len());
                let mut at = cursor;
                for row in rows {
                    if let Some(oi) = oi {
                        if row.get(oi) != Some(&FVal::Int(at as u64)) {
                            return Err(refused(label, at, Reason::Placement));
                        }
                    }
                    let Some(FVal::Int(len)) = li.and_then(|li| row.get(li)) else {
                        return Err(refused(label, at, Reason::Overflow));
                    };
                    let end = to_usize(*len)
                        .and_then(|l| at.checked_add(l))
                        .filter(|&e| e <= limit)
                        .ok_or_else(|| refused(label, at, Reason::Truncated { need: *len }))?;
                    runs.push((at, end));
                    at = end;
                }
                (SegVal::Data(runs), at)
            }
            SegmentKind::Bytes(len) => {
                let n = self
                    .len_value(len, cursor, limit)
                    .ok_or_else(|| refused(label, cursor, Reason::Overflow))?;
                let end = to_usize(n)
                    .and_then(|l| cursor.checked_add(l))
                    .filter(|&e| e <= limit)
                    .ok_or_else(|| refused(label, cursor, Reason::Truncated { need: n }))?;
                (SegVal::Bytes, end)
            }
            SegmentKind::Stream { size, plus, max } => {
                let size = self
                    .len_value(size, cursor, limit)
                    .ok_or_else(|| refused(label, cursor, Reason::Overflow))?;
                let full = size
                    .checked_add(*plus)
                    .filter(|&f| f > 0)
                    .ok_or_else(|| refused(label, cursor, Reason::Overflow))?;
                let room = (limit - cursor) as u64;
                let rem = room % full;
                if rem < *plus || (*plus == 0 && rem == 0) {
                    // the last chunk is shorter than a full one and holds at least its tag
                    return Err(refused(label, limit, Reason::Chunks));
                }
                let chunks = room / full + 1;
                if max.is_some_and(|m| chunks > m) {
                    return Err(refused(label, cursor, Reason::Chunks));
                }
                self.spend(chunks, 0)?;
                let full =
                    to_usize(full).ok_or_else(|| refused(label, cursor, Reason::Overflow))?;
                let mut v = Vec::with_capacity(to_usize(chunks).unwrap_or(0));
                let mut at = cursor;
                while limit - at >= full {
                    v.push((at, at + full));
                    at += full;
                }
                v.push((at, limit));
                (SegVal::Stream(v), limit)
            }
        };
        self.segs.push(((cursor, end), val));
        Ok(end)
    }
}

/// Reads `input` by `layout` within `budget`
///
/// # Errors
///
/// [`DecodeError::Refused`] with the label of the first check that fails, in this order:
/// `minimum`; the segments in order (each field's `=` / `bits` / `in` as it is read; a data
/// segment's placement per row); bytes left after the last segment before the `at end` ones
/// (the label of that segment); the digests in order; the rules in order.
/// [`DecodeError::BudgetExceeded`] before an allocation the budget does not allow.
pub fn decode<'a, 'l>(
    layout: &'l Layout,
    input: &'a [u8],
    budget: Budget,
) -> Result<Decoded<'a, 'l>, DecodeError> {
    if let Some((min, label)) = &layout.minimum {
        if (input.len() as u64) < *min {
            return Err(refused(label, 0, Reason::TooShort));
        }
    }
    // the `at end` segments are placed first, from the end
    let mut tail = 0usize;
    for s in layout.segments.iter().filter(|s| s.at_end) {
        let SegmentKind::Bytes(Len::Fixed(n)) = s.kind else {
            return Err(refused(&s.label, 0, Reason::Overflow));
        };
        tail = to_usize(n)
            .and_then(|n| tail.checked_add(n))
            .filter(|&t| t <= input.len())
            .ok_or_else(|| refused(&s.label, 0, Reason::Truncated { need: n }))?;
    }
    let limit = input.len() - tail;
    let mut d = Dec {
        layout,
        input,
        budget,
        rows_used: 0,
        bytes_used: 0,
        segs: Vec::with_capacity(layout.segments.len()),
    };
    let mut cursor = 0usize;
    let mut last_label = "";
    for (i, s) in layout.segments.iter().enumerate() {
        if s.at_end {
            continue;
        }
        cursor = d.segment(i, cursor, limit)?;
        last_label = &s.label;
    }
    if cursor != limit {
        return Err(refused(last_label, cursor, Reason::Trailing));
    }
    let mut at = limit;
    for (i, s) in layout.segments.iter().enumerate() {
        if s.at_end {
            at = d.segment(i, at, input.len())?;
        }
    }
    let mut out = Decoded {
        layout,
        input,
        segs: d.segs,
        status: None,
    };
    for g in &layout.digests {
        match &g.over {
            DigestOver::Before => {
                let (s, e) = out
                    .range(&g.segment)
                    .ok_or_else(|| refused(&g.label, 0, Reason::Overflow))?;
                if Sha256::digest(&input[..s])[..] != input[s..e] {
                    return Err(refused(&g.label, s, Reason::Digest { row: None }));
                }
            }
            DigestOver::Data(data) => {
                let field = g.field.as_deref().unwrap_or("");
                for r in 0..out.rows(&g.segment) {
                    let held = out.row_value(&g.segment, r, field);
                    let run = out.data_run(data, r);
                    match (held, run) {
                        (Some(FieldValue::Bytes(h)), Some(run))
                            if Sha256::digest(run)[..] == *h => {}
                        _ => {
                            let at = out.data_range(data, r).map_or(0, |x| x.0);
                            return Err(refused(
                                &g.label,
                                at,
                                Reason::Digest {
                                    row: Some(r as u64),
                                },
                            ));
                        }
                    }
                }
            }
        }
    }
    for rule in &layout.rules {
        match rule {
            Rule::Refuse { label, cond, .. } => {
                if holds(cond, &out, None) {
                    return Err(refused(label, 0, Reason::Rule));
                }
            }
            Rule::Status { name, cond, .. } => {
                if out.status.is_none() && cond.as_ref().is_none_or(|c| holds(c, &out, None)) {
                    out.status = Some(name.clone());
                }
            }
        }
    }
    Ok(out)
}

impl<'a> Decoded<'a, '_> {
    fn seg_index(&self, name: &str) -> Option<usize> {
        self.layout.segments.iter().position(|s| s.name == name)
    }

    fn range(&self, name: &str) -> Option<(usize, usize)> {
        Some(self.segs.get(self.seg_index_decoded(name)?)?.0)
    }

    /// the `at end` segments are declared last, so a segment's place in `segs` is its
    /// declaration index
    fn seg_index_decoded(&self, name: &str) -> Option<usize> {
        self.seg_index(name)
    }

    fn fval(&self, v: FVal) -> FieldValue<'a> {
        match v {
            FVal::Int(x) => FieldValue::Int(x),
            FVal::Bytes(s, e) => FieldValue::Bytes(&self.input[s..e]),
        }
    }

    fn field_index(&self, segment: &str, field: &str) -> Option<usize> {
        let (SegmentKind::Block(block) | SegmentKind::Table { block, .. }) =
            &self.layout.segments[self.seg_index(segment)?].kind
        else {
            return None;
        };
        self.layout
            .block(block)?
            .fields
            .iter()
            .position(|f| f.name == field)
    }

    /// The status the first holding `status` rule named
    #[must_use]
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// The bytes a segment spans
    #[must_use]
    pub fn segment_bytes(&self, name: &str) -> Option<&'a [u8]> {
        let (s, e) = self.range(name)?;
        Some(&self.input[s..e])
    }

    /// A field of a block segment
    #[must_use]
    pub fn value(&self, segment: &str, field: &str) -> Option<FieldValue<'a>> {
        let fi = self.field_index(segment, field)?;
        match &self.segs.get(self.seg_index_decoded(segment)?)?.1 {
            SegVal::Block(v) => Some(self.fval(*v.get(fi)?)),
            _ => None,
        }
    }

    /// The rows of a table segment (0 for any other name)
    #[must_use]
    pub fn rows(&self, table: &str) -> usize {
        match self.seg_index_decoded(table).and_then(|i| self.segs.get(i)) {
            Some((_, SegVal::Table(r))) => r.len(),
            _ => 0,
        }
    }

    /// A field of a table row
    #[must_use]
    pub fn row_value(&self, table: &str, row: usize, field: &str) -> Option<FieldValue<'a>> {
        let fi = self.field_index(table, field)?;
        match &self.segs.get(self.seg_index_decoded(table)?)?.1 {
            SegVal::Table(r) => Some(self.fval(*r.get(row)?.get(fi)?)),
            _ => None,
        }
    }

    fn data_range(&self, segment: &str, row: usize) -> Option<(usize, usize)> {
        match &self.segs.get(self.seg_index_decoded(segment)?)?.1 {
            SegVal::Data(r) | SegVal::Stream(r) => r.get(row).copied(),
            _ => None,
        }
    }

    /// The run of row `row` of a data segment, or chunk `row` of a stream
    #[must_use]
    pub fn data_run(&self, segment: &str, row: usize) -> Option<&'a [u8]> {
        let (s, e) = self.data_range(segment, row)?;
        Some(&self.input[s..e])
    }

    /// The runs of a data segment or the chunks of a stream
    #[must_use]
    pub fn runs(&self, segment: &str) -> usize {
        match self
            .seg_index_decoded(segment)
            .and_then(|i| self.segs.get(i))
        {
            Some((_, SegVal::Data(r) | SegVal::Stream(r))) => r.len(),
            _ => 0,
        }
    }

    /// A derived value over this input
    #[must_use]
    pub fn derived(&self, name: &str) -> Option<[u8; 32]> {
        derive_value(self.layout, name, &|seg| {
            self.segment_bytes(seg).map(Cow::Borrowed)
        })
    }

    /// The constant of that name
    #[must_use]
    pub fn constant(&self, name: &str) -> Option<&Const> {
        self.layout.constant(name)
    }
}

const fn val(v: FieldValue<'_>) -> Val<'_> {
    match v {
        FieldValue::Int(x) => Val::Int(x),
        FieldValue::Bytes(b) => Val::Bytes(Cow::Borrowed(b)),
    }
}

impl<'a> Scope<'a> for Decoded<'a, '_> {
    fn field(&self, segment: &str, field: &str) -> Option<Val<'a>> {
        self.value(segment, field).map(val)
    }

    fn row_field(&self, table: &str, row: usize, field: &str) -> Option<Val<'a>> {
        self.row_value(table, row, field).map(val)
    }

    fn rows(&self, table: &str) -> usize {
        Decoded::rows(self, table)
    }

    fn data(&self, segment: &str, row: usize) -> Option<&'a [u8]> {
        self.data_run(segment, row)
    }

    fn segment(&self, segment: &str) -> Option<&'a [u8]> {
        self.segment_bytes(segment)
    }

    fn derive(&self, name: &str) -> Option<Vec<u8>> {
        self.derived(name).map(|d| d.to_vec())
    }
}
