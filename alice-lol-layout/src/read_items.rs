//! The `segment`, `digest`, `derive`, `refuse` and `status` lines

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::cond::parse_cond;
use crate::decl::{
    err, Block, Const, Derive, DeriveItem, Digest, DigestOver, FieldRef, FieldType, IntEncoding,
    LayoutError, Len, Rule, Segment, SegmentKind, MAX_ITEMS,
};
use crate::read::{take_refuse, words, Reader};
use crate::token::{is_ident, normalize, parse_field_ref, Tok};

impl Reader {
    #[allow(clippy::too_many_lines)] // the five segment kinds and where `at end` may stand
    pub fn segment(&mut self, toks: &[Tok], n: usize) -> Result<(), LayoutError> {
        let w = words(toks, n)?;
        let (w, label) = take_refuse(&w, n)?;
        let name = match w.get(1) {
            Some(x) if is_ident(x) => *x,
            _ => return Err(err(n, "`segment <name> <kind> ...`")),
        };
        self.name_free(name, n)?;
        let (body, at_end) = if w.len() >= 2 && w[w.len() - 2..] == ["at", "end"] {
            (&w[2..w.len() - 2], true)
        } else {
            (&w[2..], false)
        };
        let kind = match body {
            ["block", b] => {
                self.need_block(b, n)?;
                SegmentKind::Block((*b).to_owned())
            }
            ["table", b, "count", r] => {
                self.need_block(b, n)?;
                let count = parse_field_ref(r)
                    .filter(|r| r.segment.is_some())
                    .ok_or_else(|| err(n, "`count <segment>.<field>`"))?;
                self.need_int_field(&count, n)?;
                SegmentKind::Table {
                    block: (*b).to_owned(),
                    count,
                }
            }
            ["data", t, "length", len, rest @ ..] => {
                let block = self.table_block(t, n)?;
                let offset = match rest {
                    [] => None,
                    ["offset", o] => Some((*o).to_owned()),
                    _ => return Err(err(n, "`data <table> length <field> [offset <field>]`")),
                };
                for f in core::iter::once(*len).chain(offset.as_deref()) {
                    let ok = block
                        .fields
                        .iter()
                        .any(|g| g.name == f && matches!(g.ty, FieldType::Uint { .. }));
                    if !ok {
                        return Err(err(
                            n,
                            format!("`{f}` is not an integer field of the table's block"),
                        ));
                    }
                }
                SegmentKind::Data {
                    table: (*t).to_owned(),
                    length: (*len).to_owned(),
                    offset,
                }
            }
            ["bytes", "rest"] => SegmentKind::Bytes(Len::Rest),
            ["bytes", rest @ ..] => SegmentKind::Bytes(self.len_of(rest, n)?),
            ["stream", "size", rest @ ..] => {
                let p = rest
                    .iter()
                    .position(|&x| x == "plus")
                    .ok_or_else(|| err(n, "`stream size <n> plus <n> [max <n>]`"))?;
                let size = self.len_of(&rest[..p], n)?;
                let after = &rest[p + 1..];
                let (plus_terms, max) = match after.iter().position(|&x| x == "max") {
                    Some(m) => (&after[..m], Some(self.int_expr(&after[m + 1..], n)?)),
                    None => (after, None),
                };
                let plus = self.int_expr(plus_terms, n)?;
                SegmentKind::Stream { size, plus, max }
            }
            _ => {
                return Err(err(
                    n,
                    "a segment is `block`, `table`, `data`, `bytes` or `stream`",
                ))
            }
        };
        if at_end {
            if !matches!(kind, SegmentKind::Bytes(Len::Fixed(_))) {
                return Err(err(n, "only a fixed `bytes` segment can be `at end`"));
            }
        } else if self.layout.segments.iter().any(|s| s.at_end) {
            return Err(err(n, "the `at end` segments come last"));
        }
        if self.layout.segments.iter().any(|s| {
            !s.at_end
                && matches!(
                    s.kind,
                    SegmentKind::Stream { .. } | SegmentKind::Bytes(Len::Rest)
                )
        }) && !at_end
        {
            return Err(err(
                n,
                "nothing but `at end` segments follows a `stream` or `bytes rest`",
            ));
        }
        if self.layout.segments.len() >= MAX_ITEMS {
            return Err(err(n, "too many segments"));
        }
        let label = self.label(label.unwrap_or_else(|| name.to_owned()), n)?;
        self.layout.segments.push(Segment {
            name: name.to_owned(),
            kind,
            at_end,
            label,
        });
        Ok(())
    }

    fn len_of(&self, terms: &[&str], n: usize) -> Result<Len, LayoutError> {
        if let [one] = terms {
            if let Some(r) = parse_field_ref(one).filter(|r| r.segment.is_some()) {
                self.need_int_field(&r, n)?;
                return Ok(Len::Field(r));
            }
        }
        Ok(Len::Fixed(self.int_expr(terms, n)?))
    }

    fn need_block(&self, b: &str, n: usize) -> Result<(), LayoutError> {
        if self.layout.block(b).is_none() {
            return Err(err(n, format!("unknown block `{b}`")));
        }
        Ok(())
    }

    fn table_block(&self, t: &str, n: usize) -> Result<&Block, LayoutError> {
        match self.layout.segment(t).map(|s| &s.kind) {
            Some(SegmentKind::Table { block, .. }) => self
                .layout
                .block(block)
                .ok_or_else(|| err(n, "internal: a table without its block")),
            _ => Err(err(n, format!("`{t}` is not an earlier table segment"))),
        }
    }

    /// `<segment>.<field>` names an integer field of an earlier `block` segment
    fn need_int_field(&self, r: &FieldRef, n: usize) -> Result<(), LayoutError> {
        let seg = r.segment.as_deref().unwrap_or("");
        let block = match self.layout.segment(seg).map(|s| &s.kind) {
            Some(SegmentKind::Block(b)) => self.layout.block(b),
            _ => None,
        }
        .ok_or_else(|| err(n, format!("`{seg}` is not an earlier block segment")))?;
        if !block
            .fields
            .iter()
            .any(|f| f.name == r.field && matches!(f.ty, FieldType::Uint { .. }))
        {
            return Err(err(
                n,
                format!("`{seg}.{}` is not an integer field", r.field),
            ));
        }
        Ok(())
    }

    #[allow(clippy::many_single_char_names)] // t / f / d / s: table, field, data, segment
    pub fn digest(&mut self, toks: &[Tok], n: usize) -> Result<(), LayoutError> {
        let w = words(toks, n)?;
        let (w, label) = take_refuse(&w, n)?;
        let (target, over) =
            match w.as_slice() {
                ["digest", t, "sha256", "before"] => (*t, DigestOver::Before),
                ["digest", t, "sha256", d] => (*t, DigestOver::Data((*d).to_owned())),
                _ => return Err(err(
                    n,
                    "`digest <segment> sha256 before` or `digest <table>.<field> sha256 <data>`",
                )),
            };
        let (segment, field) = match (&over, t_split(target)) {
            (DigestOver::Before, (s, None)) => {
                match self.layout.segment(s).map(|x| &x.kind) {
                    Some(SegmentKind::Bytes(Len::Fixed(32))) => {}
                    _ => return Err(err(n, format!("`{s}` is not a 32-byte `bytes` segment"))),
                }
                (s.to_owned(), None)
            }
            (DigestOver::Data(d), (t, Some(f))) => {
                let block = self.table_block(t, n)?;
                if !block
                    .fields
                    .iter()
                    .any(|g| g.name == f && g.ty == FieldType::Bytes(Len::Fixed(32)))
                {
                    return Err(err(n, format!("`{t}.{f}` is not a 32-byte field")));
                }
                match self.layout.segment(d).map(|x| &x.kind) {
                    Some(SegmentKind::Data { table, .. }) if table == t => {}
                    _ => return Err(err(n, format!("`{d}` is not the data segment of `{t}`"))),
                }
                (t.to_owned(), Some(f.to_owned()))
            }
            _ => {
                return Err(err(
                    n,
                    "a `before` digest names a segment, a data digest names `<table>.<field>`",
                ))
            }
        };
        if self.layout.digests.len() >= MAX_ITEMS {
            return Err(err(n, "too many digests"));
        }
        let label = self.label(label.unwrap_or_else(|| format!("digest.{target}")), n)?;
        self.layout.digests.push(Digest {
            segment,
            field,
            over,
            label,
        });
        Ok(())
    }

    pub fn derive(&mut self, toks: &[Tok], n: usize) -> Result<(), LayoutError> {
        let w = words(toks, n)?;
        let (name, rest) = match w.as_slice() {
            ["derive", name, "sha256", rest @ ..] if is_ident(name) => (*name, rest),
            _ => {
                return Err(err(
                    n,
                    "`derive <name> sha256 [lenprefix u64be] <item> ...`",
                ))
            }
        };
        self.name_free(name, n)?;
        let (length_prefixed, items_w) = match rest {
            ["lenprefix", "u64be", items @ ..] => (true, items),
            items => (false, items),
        };
        if items_w.is_empty() {
            return Err(err(n, "a derive hashes at least one item"));
        }
        let mut items = Vec::with_capacity(items_w.len());
        for it in items_w {
            let (base, enc) = match it.split_once(':') {
                Some((b, e)) => (
                    b,
                    Some(
                        IntEncoding::parse(e)
                            .ok_or_else(|| err(n, format!("unknown encoding `{e}`")))?,
                    ),
                ),
                None => (*it, None),
            };
            if let Some(c) = self.layout.constant(base) {
                match (c, enc) {
                    (Const::Integer(v), Some(e)) => {
                        e.encode(*v)
                            .ok_or_else(|| err(n, format!("`{base}` does not fit its encoding")))?;
                    }
                    (Const::Integer(_), None) => {
                        return Err(err(
                            n,
                            format!("integer `{base}` needs an encoding (`{base}:u64le`)"),
                        ))
                    }
                    (_, Some(_)) => {
                        return Err(err(n, "only an integer constant takes an encoding"))
                    }
                    (_, None) => {}
                }
                items.push(DeriveItem::Const(base.to_owned(), enc));
            } else if self.layout.segment(base).is_some() && enc.is_none() {
                items.push(DeriveItem::Segment(base.to_owned()));
            } else {
                return Err(err(
                    n,
                    format!("`{it}` is neither a constant nor a segment"),
                ));
            }
        }
        if self.layout.derives.len() >= MAX_ITEMS {
            return Err(err(n, "too many derives"));
        }
        self.layout.derives.push(Derive {
            name: name.to_owned(),
            length_prefixed,
            items,
        });
        Ok(())
    }

    pub fn rule(&mut self, toks: &[Tok], n: usize) -> Result<(), LayoutError> {
        let head = toks[0].word().unwrap_or("");
        let name = toks
            .get(1)
            .and_then(Tok::word)
            .filter(|w| is_ident(w))
            .ok_or_else(|| err(n, format!("`{head} <name> when <condition>`")))?
            .to_owned();
        let rule = match (head, toks.get(2).and_then(Tok::word)) {
            ("status", Some("otherwise")) if toks.len() == 3 => Rule::Status {
                name,
                cond: None,
                text: String::new(),
            },
            (_, Some("when")) if toks.len() > 3 => {
                let cond = parse_cond(&toks[3..], &self.layout, n)?;
                let text = normalize(&toks[3..]);
                if head == "refuse" {
                    let label = self.label(name, n)?;
                    Rule::Refuse { label, cond, text }
                } else {
                    Rule::Status {
                        name,
                        cond: Some(cond),
                        text,
                    }
                }
            }
            _ => return Err(err(n, format!("`{head} <name> when <condition>`"))),
        };
        if self.layout.rules.len() >= MAX_ITEMS {
            return Err(err(n, "too many rules"));
        }
        self.layout.rules.push(rule);
        Ok(())
    }
}

fn t_split(s: &str) -> (&str, Option<&str>) {
    match s.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (s, None),
    }
}
