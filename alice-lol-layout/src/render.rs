//! The declaration as Markdown, for documentation generated from the law instead of written
//! beside it

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;

use crate::decl::{
    tag_text, Block, Const, DeriveItem, DigestOver, FieldType, Layout, Len, Rule, SegmentKind,
    Value,
};

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::from("0x"), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn len_text(l: &Len) -> String {
    match l {
        Len::Fixed(n) => format!("{n}"),
        Len::Field(r) => r.segment.as_ref().map_or_else(
            || format!("`{}`", r.field),
            |s| format!("`{s}.{}`", r.field),
        ),
        Len::Rest => String::from("the rest"),
    }
}

fn field_type(t: &FieldType) -> String {
    match t {
        FieldType::Uint { width, endian } => {
            let e = match endian {
                Some(crate::decl::Endian::Little) => " le",
                Some(crate::decl::Endian::Big) => " be",
                None => "",
            };
            format!("u{}{e}", u32::from(*width) * 8)
        }
        FieldType::Bytes(l) => format!("bytes {}", len_text(l)),
    }
}

fn block_table(out: &mut String, layout: &Layout, b: &Block) {
    let _ = writeln!(
        out,
        "\n**block `{}`**{} ({})\n",
        b.name,
        b.size.map_or_else(String::new, |s| format!(", {s} bytes")),
        layout.endian
    );
    out.push_str("| bytes | field | type | check |\n|---|---|---|---|\n");
    let mut at = Some(0u64);
    for f in &b.fields {
        let width = match &f.ty {
            FieldType::Uint { width, .. } => Some(u64::from(*width)),
            FieldType::Bytes(Len::Fixed(n)) => Some(*n),
            FieldType::Bytes(_) => None,
        };
        let range = match (at, width) {
            (Some(a), Some(w)) => format!("`[{a}..{})`", a + w),
            (Some(a), None) => format!("`[{a}..)`"),
            (None, _) => String::from("after the previous"),
        };
        at = at.zip(width).map(|(a, w)| a + w);
        let mut check = Vec::new();
        match &f.equals {
            Some(Value::Integer(v)) => check.push(format!("= {v}")),
            Some(Value::Bytes(v)) => check.push(format!("= `{}`", hex(v))),
            None => {}
        }
        if let Some(m) = f.bits {
            check.push(format!("bits `{m:#x}`"));
        }
        if let Some((lo, hi)) = f.range {
            check.push(format!("in {lo}..={hi}"));
        }
        let check = if check.is_empty() {
            String::new()
        } else {
            format!("{} (refuse `{}`)", check.join(", "), f.label)
        };
        let _ = writeln!(
            out,
            "| {range} | `{}` | {} | {check} |",
            f.name,
            field_type(&f.ty)
        );
    }
}

impl Layout {
    /// The declaration as Markdown: the segments in order, a table per block with the byte
    /// ranges of its fixed fields, the digests, the derived values and the rules
    ///
    /// The text depends only on the declaration, so a document that embeds it can be checked
    /// for being up to date by rendering again and comparing
    #[must_use]
    #[allow(clippy::too_many_lines)] // one section per part of the declaration, in its order
    pub fn markdown(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Layout law `{}`, integers {}.\n",
            self.name, self.endian
        );
        if let Some((m, l)) = &self.minimum {
            let _ = writeln!(out, "An input shorter than {m} bytes is refused (`{l}`).\n");
        }
        out.push_str("| segment | content |\n|---|---|\n");
        for s in &self.segments {
            let content = match &s.kind {
                SegmentKind::Block(b) => format!("block `{b}`"),
                SegmentKind::Table { block, count } => format!(
                    "`{}.{}` rows of block `{block}`",
                    count.segment.as_deref().unwrap_or(""),
                    count.field
                ),
                SegmentKind::Data {
                    table,
                    length,
                    offset,
                } => format!(
                    "one run per row of `{table}`, back to back, `{length}` bytes each{}",
                    offset
                        .as_ref()
                        .map_or_else(String::new, |o| format!(", starting where `{o}` says"))
                ),
                SegmentKind::Bytes(l) => format!("{} bytes", len_text(l)),
                SegmentKind::Stream { size, plus, max } => format!(
                    "chunks of {} + {plus} bytes, the last shorter (at least {plus}){}",
                    len_text(size),
                    max.map_or_else(String::new, |m| format!(", at most {m}"))
                ),
            };
            let end = if s.at_end { ", at the end" } else { "" };
            let _ = writeln!(
                out,
                "| `{}` | {content}{end} (refuse `{}`) |",
                s.name, s.label
            );
        }
        for b in &self.blocks {
            block_table(&mut out, self, b);
        }
        if !self.digests.is_empty() {
            out.push_str("\nDigests (SHA-256):\n\n");
            for g in &self.digests {
                let what = match (&g.over, &g.field) {
                    (DigestOver::Before, _) => {
                        format!("`{}` is the digest of every byte before it", g.segment)
                    }
                    (DigestOver::Data(d), Some(f)) => {
                        format!(
                            "`{}.{f}` of each row is the digest of its run in `{d}`",
                            g.segment
                        )
                    }
                    (DigestOver::Data(d), None) => format!("`{}` over `{d}`", g.segment),
                };
                let _ = writeln!(out, "- {what} (refuse `{}`)", g.label);
            }
        }
        if !self.derives.is_empty() {
            out.push_str("\nDerived values:\n\n");
            for d in &self.derives {
                let items: Vec<String> = d
                    .items
                    .iter()
                    .map(|it| match it {
                        DeriveItem::Const(c, Some(e)) => format!("`{c}` as {}", e.name()),
                        DeriveItem::Const(c, None) => format!("`{c}`"),
                        DeriveItem::Segment(s) => format!("segment `{s}`"),
                    })
                    .collect();
                let prefix = if d.length_prefixed {
                    ", each prefixed with its length as a big-endian u64"
                } else {
                    ""
                };
                let value = self
                    .derive_static(&d.name)
                    .map_or_else(String::new, |v| format!(" = `{}`", &hex(&v)[2..]));
                let _ = writeln!(
                    out,
                    "- `{}` = SHA-256 of {}{prefix}{value}",
                    d.name,
                    items.join(", ")
                );
            }
        }
        if !self.consts.is_empty() {
            out.push_str("\nConstants:\n\n");
            for (n, c) in &self.consts {
                let v = match c {
                    Const::Integer(v) => format!("{v}"),
                    Const::Bytes(b) => format!("`{}`", hex(b)),
                    Const::Text(t) => format!("`\"{t}\"`"),
                    Const::Tags(ts) => ts
                        .iter()
                        .map(|t| format!("`{}`", tag_text(t)))
                        .collect::<Vec<_>>()
                        .join(" "),
                };
                let _ = writeln!(out, "- `{n}` = {v}");
            }
        }
        if !self.rules.is_empty() {
            out.push_str("\nRules, in order:\n\n");
            for r in &self.rules {
                match r {
                    Rule::Refuse { label, text, .. } => {
                        let _ = writeln!(out, "- refuse `{label}` when `{text}`");
                    }
                    Rule::Status {
                        name,
                        cond: Some(_),
                        text,
                    } => {
                        let _ = writeln!(out, "- status `{name}` when `{text}`");
                    }
                    Rule::Status {
                        name, cond: None, ..
                    } => {
                        let _ = writeln!(out, "- otherwise status `{name}`");
                    }
                }
            }
        }
        out
    }
}
