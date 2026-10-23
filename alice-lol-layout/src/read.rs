//! Reading a law file into a [`Layout`], with the checks that need no bytes

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::decl::{
    err, Block, Const, Endian, ErrorKind, Field, FieldType, Layout, LayoutError, Len, Rule,
    SegmentKind, Value, MAX_FIELDS, MAX_ITEMS, MAX_LINES, MAX_LINE_LEN,
};
use crate::token::{
    is_ident, normalize, parse_field_ref, parse_hex_bytes, parse_int, parse_tag, tokenize, Tok,
};

/// An untyped field value as written
#[derive(Debug, Clone)]
pub enum RawValue {
    Word(String),
    Text(String),
}

pub struct Reader {
    pub layout: Layout,
    pub raw_equals: Vec<(String, usize, RawValue)>,
    /// unresolved integer expressions of sizes: (where, line, terms)
    pub open_block: Option<Block>,
    pub labels: Vec<String>,
    pub saw_kind: bool,
    pub saw_endian: bool,
}

pub fn words(toks: &[Tok], n: usize) -> Result<Vec<&str>, LayoutError> {
    toks.iter()
        .map(|t| t.word().ok_or_else(|| err(n, "a text is not allowed here")))
        .collect()
}

/// Splits a trailing `refuse <label>`
pub fn take_refuse<'a>(
    w: &[&'a str],
    n: usize,
) -> Result<(Vec<&'a str>, Option<String>), LayoutError> {
    if let Some(i) = w.iter().position(|&x| x == "refuse") {
        if i + 2 != w.len() || !is_ident(w[i + 1]) {
            return Err(err(n, "`refuse` takes one label and ends the line"));
        }
        Ok((w[..i].to_vec(), Some(w[i + 1].to_owned())))
    } else {
        Ok((w.to_vec(), None))
    }
}

impl Reader {
    pub fn int_expr(&self, terms: &[&str], n: usize) -> Result<u64, LayoutError> {
        if terms.is_empty() || terms.len().is_multiple_of(2) {
            return Err(err(n, "an integer expression is `<a> [+ <b> ...]`"));
        }
        let mut sum = 0u64;
        for (i, t) in terms.iter().enumerate() {
            if i % 2 == 1 {
                if *t != "+" {
                    return Err(err(n, "only `+` joins the terms of an integer expression"));
                }
                continue;
            }
            let v = if let Some(v) = parse_int(t) {
                v
            } else {
                match self.layout.constant(t) {
                    Some(Const::Integer(v)) => *v,
                    Some(_) => return Err(err(n, format!("`{t}` is not an integer constant"))),
                    None => return Err(err(n, format!("unknown constant `{t}`"))),
                }
            };
            sum = sum
                .checked_add(v)
                .ok_or_else(|| err(n, "an integer expression overflows 64 bits"))?;
        }
        Ok(sum)
    }

    pub fn label(&mut self, l: String, n: usize) -> Result<String, LayoutError> {
        // a label may repeat (two checks may refuse with the same class); only its form is checked
        if !is_ident(&l) && !l.contains('.') {
            return Err(err(n, "a label is an identifier"));
        }
        self.labels.push(l.clone());
        Ok(l)
    }

    /// Constants, segments and derives share one namespace (a condition or a derive names any
    /// of them); blocks have their own (only a `segment` line names a block)
    pub fn name_free(&self, name: &str, n: usize) -> Result<(), LayoutError> {
        let l = &self.layout;
        if l.consts.iter().any(|(c, _)| c == name)
            || l.segments.iter().any(|s| s.name == name)
            || l.derives.iter().any(|d| d.name == name)
        {
            return Err(err(n, format!("`{name}` is declared twice")));
        }
        Ok(())
    }

    fn line(&mut self, toks: &[Tok], n: usize) -> Result<(), LayoutError> {
        let head = toks[0]
            .word()
            .ok_or_else(|| err(n, "a line starts with a keyword"))?;
        if self.open_block.is_some() && head != "field" && head != "end" {
            return Err(err(
                n,
                "only `field` lines and `end block` are allowed inside a block",
            ));
        }
        match head {
            // `law` / `kind` are read by `parse`; the lines for people declare nothing
            "law" | "kind" | "claim" | "source" | "verdict" => Ok(()),
            "endian" => self.endian(toks, n),
            "const" => self.constant(toks, n),
            "minimum" => self.minimum(toks, n),
            "block" => self.block(toks, n),
            "field" => self.field(toks, n),
            "end" => self.end(toks, n),
            "segment" => self.segment(toks, n),
            "digest" => self.digest(toks, n),
            "derive" => self.derive(toks, n),
            "refuse" | "status" => self.rule(toks, n),
            other => Err(err(n, format!("unknown keyword `{other}`"))),
        }
    }

    fn endian(&mut self, toks: &[Tok], n: usize) -> Result<(), LayoutError> {
        let w = words(toks, n)?;
        if self.saw_endian {
            return Err(err(n, "`endian` is declared twice"));
        }
        self.layout.endian = match w.as_slice() {
            ["endian", "le"] => Endian::Little,
            ["endian", "be"] => Endian::Big,
            _ => return Err(err(n, "`endian le` or `endian be`")),
        };
        self.saw_endian = true;
        Ok(())
    }

    fn constant(&mut self, toks: &[Tok], n: usize) -> Result<(), LayoutError> {
        let name = match toks.get(1).and_then(Tok::word) {
            Some(x) if is_ident(x) => x,
            _ => return Err(err(n, "`const <name> <type> = <value>`")),
        };
        self.name_free(name, n)?;
        let ty = toks.get(2).and_then(Tok::word);
        if toks.get(3).and_then(Tok::word) != Some("=") {
            return Err(err(n, "`const <name> <type> = <value>`"));
        }
        let rest = &toks[4..];
        let c =
            match (ty, rest) {
                (Some("integer"), [Tok::Word(v)]) => {
                    Const::Integer(parse_int(v).ok_or_else(|| err(n, "not an integer"))?)
                }
                (Some("bytes"), [Tok::Word(v)]) => Const::Bytes(
                    parse_hex_bytes(v).ok_or_else(|| err(n, "bytes are `0x` + hex pairs"))?,
                ),
                (Some("text"), [Tok::Text(v)]) => Const::Text(v.clone()),
                (Some("tags"), tags) if !tags.is_empty() => {
                    let mut out: Vec<[u8; 4]> = Vec::new();
                    for t in tags {
                        let tag = t
                            .word()
                            .and_then(parse_tag)
                            .ok_or_else(|| err(n, "a tag is 4 printable ASCII bytes"))?;
                        if out.contains(&tag) {
                            return Err(err(n, "a tag is listed twice"));
                        }
                        out.push(tag);
                    }
                    Const::Tags(out)
                }
                _ => return Err(err(
                    n,
                    "a constant is `integer`, `bytes`, `text` or `tags` with a value of that type",
                )),
            };
        if self.layout.consts.len() >= MAX_ITEMS {
            return Err(err(n, "too many constants"));
        }
        self.layout.consts.push((name.to_owned(), c));
        Ok(())
    }

    fn minimum(&mut self, toks: &[Tok], n: usize) -> Result<(), LayoutError> {
        let w = words(toks, n)?;
        let (w, label) = take_refuse(&w, n)?;
        if self.layout.minimum.is_some() {
            return Err(err(n, "`minimum` is declared twice"));
        }
        let v = self.int_expr(&w[1..], n)?;
        let label = self.label(label.unwrap_or_else(|| "minimum".to_owned()), n)?;
        self.layout.minimum = Some((v, label));
        Ok(())
    }

    fn block(&mut self, toks: &[Tok], n: usize) -> Result<(), LayoutError> {
        let w = words(toks, n)?;
        let name = match w.get(1) {
            Some(x) if is_ident(x) => *x,
            _ => return Err(err(n, "`block <name> [size <n>]`")),
        };
        if self.layout.block(name).is_some() {
            return Err(err(n, format!("block `{name}` is declared twice")));
        }
        let size = match &w[2..] {
            [] => None,
            ["size", rest @ ..] => Some(self.int_expr(rest, n)?),
            _ => return Err(err(n, "`block <name> [size <n>]`")),
        };
        if self.layout.blocks.len() >= MAX_ITEMS {
            return Err(err(n, "too many blocks"));
        }
        self.open_block = Some(Block {
            name: name.to_owned(),
            size,
            fields: Vec::new(),
        });
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // one line form, its options read in order
    fn field(&mut self, toks: &[Tok], n: usize) -> Result<(), LayoutError> {
        let Some(block_name) = self.open_block.as_ref().map(|b| b.name.clone()) else {
            return Err(err(n, "a `field` line is inside a block"));
        };
        let name = match toks.get(1).and_then(Tok::word) {
            Some(x) if is_ident(x) => x.to_owned(),
            _ => return Err(err(n, "`field <name> <type> ...`")),
        };
        let mut i = 2;
        let tyw = toks
            .get(i)
            .and_then(Tok::word)
            .ok_or_else(|| err(n, "a field has a type"))?;
        i += 1;
        let uint = |w: u8, e: Option<Endian>| FieldType::Uint {
            width: w,
            endian: e,
        };
        let ty = match tyw {
            "u8" => uint(1, None),
            "u16" => uint(2, None),
            "u32" => uint(4, None),
            "u64" => uint(8, None),
            "u16le" => uint(2, Some(Endian::Little)),
            "u32le" => uint(4, Some(Endian::Little)),
            "u64le" => uint(8, Some(Endian::Little)),
            "u16be" => uint(2, Some(Endian::Big)),
            "u32be" => uint(4, Some(Endian::Big)),
            "u64be" => uint(8, Some(Endian::Big)),
            "bytes" => {
                let lw = toks
                    .get(i)
                    .and_then(Tok::word)
                    .ok_or_else(|| err(n, "`bytes <length>`"))?;
                i += 1;
                if let Some(v) = parse_int(lw) {
                    FieldType::Bytes(Len::Fixed(v))
                } else if let Some(Const::Integer(v)) = self.layout.constant(lw) {
                    FieldType::Bytes(Len::Fixed(*v))
                } else if let Some(r) = parse_field_ref(lw) {
                    FieldType::Bytes(Len::Field(r))
                } else {
                    return Err(err(
                        n,
                        "a bytes length is an integer, a constant or a field",
                    ));
                }
            }
            other => return Err(err(n, format!("unknown field type `{other}`"))),
        };
        let mut field = Field {
            name: name.clone(),
            ty,
            equals: None,
            bits: None,
            range: None,
            label: format!("{block_name}.{name}"),
            line: n,
        };
        while i < toks.len() {
            match toks[i].word() {
                Some("=") => {
                    let v = toks.get(i + 1).ok_or_else(|| err(n, "`=` takes a value"))?;
                    let raw = match v {
                        Tok::Word(w) => RawValue::Word(w.clone()),
                        Tok::Text(t) => RawValue::Text(t.clone()),
                    };
                    if field.equals.is_some()
                        || self
                            .raw_equals
                            .iter()
                            .any(|(k, l, _)| *k == name && *l == n)
                    {
                        return Err(err(n, "`=` is given twice"));
                    }
                    self.raw_equals.push((name.clone(), n, raw));
                    // resolved in `end`, once the type is known
                    field.equals = Some(Value::Integer(0));
                    i += 2;
                }
                Some("bits") => {
                    let m = toks
                        .get(i + 1)
                        .and_then(Tok::word)
                        .and_then(|w| self.int_or_const(w));
                    field.bits = Some(m.ok_or_else(|| err(n, "`bits <mask>`"))?);
                    i += 2;
                }
                Some("in") => {
                    let lo = toks
                        .get(i + 1)
                        .and_then(Tok::word)
                        .and_then(|w| self.int_or_const(w));
                    let hi = toks
                        .get(i + 2)
                        .and_then(Tok::word)
                        .and_then(|w| self.int_or_const(w));
                    match (lo, hi) {
                        (Some(lo), Some(hi)) if lo <= hi => field.range = Some((lo, hi)),
                        _ => return Err(err(n, "`in <lo> <hi>` with lo <= hi")),
                    }
                    i += 3;
                }
                Some("refuse") => {
                    let l = toks
                        .get(i + 1)
                        .and_then(Tok::word)
                        .filter(|w| is_ident(w))
                        .ok_or_else(|| err(n, "`refuse <label>`"))?;
                    if i + 2 != toks.len() {
                        return Err(err(n, "`refuse <label>` ends the line"));
                    }
                    field.label = self.label(l.to_owned(), n)?;
                    i += 2;
                }
                _ => return Err(err(
                    n,
                    "after the type: `= <value>`, `bits <mask>`, `in <lo> <hi>`, `refuse <label>`",
                )),
            }
        }
        let b = self
            .open_block
            .as_mut()
            .ok_or_else(|| err(n, "no open block"))?;
        if b.fields.iter().any(|f| f.name == name) {
            return Err(err(n, format!("field `{name}` is declared twice")));
        }
        if b.fields.len() >= MAX_FIELDS {
            return Err(err(n, "too many fields"));
        }
        if matches!(field.ty, FieldType::Bytes(_))
            && (field.bits.is_some() || field.range.is_some())
        {
            return Err(err(n, "`bits` and `in` are for integer fields"));
        }
        b.fields.push(field);
        Ok(())
    }

    fn int_or_const(&self, w: &str) -> Option<u64> {
        parse_int(w).or_else(|| match self.layout.constant(w) {
            Some(Const::Integer(v)) => Some(*v),
            _ => None,
        })
    }

    fn end(&mut self, toks: &[Tok], n: usize) -> Result<(), LayoutError> {
        let w = words(toks, n)?;
        if w.as_slice() != ["end", "block"] {
            return Err(err(n, "`end block`"));
        }
        let mut b = self
            .open_block
            .take()
            .ok_or_else(|| err(n, "`end block` without a block"))?;
        if b.fields.is_empty() {
            return Err(err(n, "a block has at least one field"));
        }
        // resolve `= <value>` against each field's type
        let raws: Vec<(String, usize, RawValue)> = core::mem::take(&mut self.raw_equals);
        for (fname, line, raw) in raws {
            let f = b
                .fields
                .iter_mut()
                .find(|f| f.name == fname && f.line == line)
                .ok_or_else(|| err(line, "internal: a value without its field"))?;
            f.equals = Some(resolve_value(&self.layout, &f.ty, &raw, line)?);
        }
        // a field length refers to an earlier integer field of this block
        for (i, f) in b.fields.iter().enumerate() {
            if let FieldType::Bytes(Len::Field(r)) = &f.ty {
                if r.segment.is_some() {
                    return Err(err(
                        f.line,
                        "a field length names a field of the same block",
                    ));
                }
                let ok = b.fields[..i]
                    .iter()
                    .any(|g| g.name == r.field && matches!(g.ty, FieldType::Uint { .. }));
                if !ok {
                    return Err(err(
                        f.line,
                        format!("`{}` is not an earlier integer field of the block", r.field),
                    ));
                }
            }
        }
        if let Some(size) = b.size {
            let variable = b
                .fields
                .iter()
                .any(|f| matches!(f.ty, FieldType::Bytes(Len::Field(_))));
            if variable {
                return Err(err(
                    n,
                    format!(
                        "block `{}` declares a size but has a variable-length field",
                        b.name
                    ),
                ));
            }
            if b.min_size() != size {
                return Err(err(
                    n,
                    format!(
                        "block `{}` declares size {size} but its fields take {}",
                        b.name,
                        b.min_size()
                    ),
                ));
            }
        }
        if b.min_size() == 0 {
            return Err(err(n, "a block takes at least one byte"));
        }
        b.fields.shrink_to_fit();
        self.layout.blocks.push(b);
        Ok(())
    }
}

fn resolve_value(
    layout: &Layout,
    ty: &FieldType,
    raw: &RawValue,
    n: usize,
) -> Result<Value, LayoutError> {
    match (ty, raw) {
        (FieldType::Uint { width, .. }, RawValue::Word(w)) => {
            let v = parse_int(w)
                .or_else(|| match layout.constant(w) {
                    Some(Const::Integer(v)) => Some(*v),
                    _ => None,
                })
                .ok_or_else(|| err(n, format!("`{w}` is not an integer or an integer constant")))?;
            if *width < 8 && v >> (u32::from(*width) * 8) != 0 {
                return Err(err(n, format!("{v} does not fit {width} bytes")));
            }
            Ok(Value::Integer(v))
        }
        (FieldType::Bytes(len), raw) => {
            let b = match raw {
                RawValue::Text(t) => t.as_bytes().to_vec(),
                RawValue::Word(w) => match parse_hex_bytes(w) {
                    Some(b) => b,
                    None => match layout.constant(w) {
                        Some(Const::Bytes(b)) => b.clone(),
                        Some(Const::Text(t)) => t.as_bytes().to_vec(),
                        _ => {
                            return Err(err(
                                n,
                                format!("`{w}` is not bytes or a bytes / text constant"),
                            ))
                        }
                    },
                },
            };
            if let Len::Fixed(l) = len {
                if b.len() as u64 != *l {
                    return Err(err(
                        n,
                        format!("the value has {} bytes, the field {l}", b.len()),
                    ));
                }
            }
            Ok(Value::Bytes(b))
        }
        (FieldType::Uint { .. }, RawValue::Text(_)) => {
            Err(err(n, "an integer field is not compared with a text"))
        }
    }
}

/// Reads a layout law file (`law <name>` / `kind layout` / ...)
///
/// The text may start with `semantics <64 lower-case hex digits>`, the reading of layouts it was
/// written for; when present it is compared with [`crate::LAYOUT_SEMANTICS_ID`]. It is not part of
/// the law's identifier.
///
/// # Errors
///
/// [`LayoutError`] with the line of the first problem; [`ErrorKind::SemanticsMismatch`] naming
/// both identifiers when the `semantics` line names another
pub fn parse(text: &str) -> Result<Layout, LayoutError> {
    read(text, false)
}

/// [`parse`] for a law another program or release wrote: the `semantics` line is required
///
/// # Errors
///
/// As [`parse`], and [`ErrorKind::SemanticsMissing`] without the `semantics` line
pub fn parse_stamped(text: &str) -> Result<Layout, LayoutError> {
    read(text, true)
}

fn semantics_line(toks: &[Tok], n: usize) -> Result<(), LayoutError> {
    let hex = match words(toks, n)?.as_slice() {
        ["semantics", h] => *h,
        _ => return Err(err(n, "`semantics <64 lower-case hex digits>`")),
    };
    let ok = hex.len() == 64
        && hex
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c));
    let theirs = if ok {
        parse_hex_bytes(&format!("0x{hex}"))
    } else {
        None
    };
    let Some(theirs) = theirs.and_then(|t| <[u8; 32]>::try_from(t).ok()) else {
        return Err(err(n, "`semantics <64 lower-case hex digits>`"));
    };
    let ours = crate::LAYOUT_SEMANTICS_ID;
    if theirs == ours {
        return Ok(());
    }
    Err(LayoutError {
        line: n,
        message: format!(
            "the law names the layout semantics {hex}, this reader is {}",
            ours.iter().fold(String::new(), |mut s, b| {
                let _ = core::fmt::Write::write_fmt(&mut s, format_args!("{b:02x}"));
                s
            })
        ),
        kind: ErrorKind::SemanticsMismatch { ours, theirs },
    })
}

#[allow(clippy::too_many_lines)] // the header lines, then the checks of the whole text, in order
fn read(text: &str, stamped: bool) -> Result<Layout, LayoutError> {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.len() > MAX_LINES {
        return Err(err(0, format!("more than {MAX_LINES} lines")));
    }
    let mut r = Reader {
        layout: Layout {
            name: String::new(),
            endian: Endian::Little,
            consts: Vec::new(),
            minimum: None,
            blocks: Vec::new(),
            segments: Vec::new(),
            digests: Vec::new(),
            derives: Vec::new(),
            rules: Vec::new(),
            id_lines: Vec::new(),
        },
        raw_equals: Vec::new(),
        open_block: None,
        labels: Vec::new(),
        saw_kind: false,
        saw_endian: false,
    };
    let mut saw_law = false;
    let mut saw_semantics = false;
    for (i, line) in lines.iter().enumerate() {
        let n = i + 1;
        if line.len() > MAX_LINE_LEN {
            return Err(err(
                n,
                format!("a line is longer than {MAX_LINE_LEN} bytes"),
            ));
        }
        let toks = tokenize(line, n)?;
        let Some(head) = toks.first() else { continue };
        let head = head.word().unwrap_or("");
        if !saw_law && !saw_semantics && head == "semantics" {
            semantics_line(&toks, n)?;
            saw_semantics = true;
            continue;
        }
        if !saw_law {
            if stamped && !saw_semantics {
                return Err(LayoutError {
                    line: n,
                    message: "a stamped law starts with `semantics <hex>`".to_owned(),
                    kind: ErrorKind::SemanticsMissing,
                });
            }
            match words(&toks, n)?.as_slice() {
                ["law", name] if is_ident(name) => {
                    (*name).clone_into(&mut r.layout.name);
                    saw_law = true;
                    r.layout.id_lines.push(normalize(&toks));
                    continue;
                }
                _ => return Err(err(n, "the first line is `law <name>`")),
            }
        }
        if !r.saw_kind {
            if words(&toks, n)?.as_slice() == ["kind", "layout"] {
                r.saw_kind = true;
                r.layout.id_lines.push(normalize(&toks));
                continue;
            }
            return Err(err(n, "the second line is `kind layout`"));
        }
        if matches!(head, "law" | "kind") {
            return Err(err(n, format!("`{head}` is declared twice")));
        }
        r.line(&toks, n)?;
        if !matches!(head, "claim" | "source" | "verdict") {
            r.layout.id_lines.push(normalize(&toks));
        }
    }
    if !saw_law || !r.saw_kind {
        return Err(err(
            0,
            "a layout law starts with `law <name>` and `kind layout`",
        ));
    }
    if r.open_block.is_some() {
        return Err(err(lines.len(), "a block is not closed with `end block`"));
    }
    if r.layout.segments.iter().all(|s| s.at_end) {
        return Err(err(
            0,
            "a layout has at least one segment that is not `at end`",
        ));
    }
    if !r.saw_endian {
        let multi = r
            .layout
            .blocks
            .iter()
            .flat_map(|b| &b.fields)
            .any(|f| matches!(f.ty, FieldType::Uint { width, endian: None } if width > 1));
        if multi {
            return Err(err(
                0,
                "multi-byte integers without their own order need an `endian` line",
            ));
        }
    }
    let statuses = r
        .layout
        .rules
        .iter()
        .filter(|x| matches!(x, Rule::Status { cond: None, .. }))
        .count();
    if statuses > 1 {
        return Err(err(0, "`status <name> otherwise` is given more than once"));
    }
    // every block is used by a segment, every constant by something (a typo leaves one unused)
    for b in &r.layout.blocks {
        let used = r.layout.segments.iter().any(|s| match &s.kind {
            SegmentKind::Block(x) | SegmentKind::Table { block: x, .. } => *x == b.name,
            _ => false,
        });
        if !used {
            return Err(err(
                0,
                format!("block `{}` is not used by a segment", b.name),
            ));
        }
    }
    for (c, _) in &r.layout.consts {
        let used = r.layout.id_lines.iter().skip(2).any(|l| {
            !l.starts_with("const ") && l.split([' ', ':', '(', ')', '[', ']', ',']).any(|t| t == c)
        });
        if !used {
            return Err(err(0, format!("constant `{c}` is not used")));
        }
    }
    let _ = r.labels;
    Ok(r.layout)
}
