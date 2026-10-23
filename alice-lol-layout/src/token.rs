//! The tokens of a line and the literals of a layout law

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

use crate::decl::{err, FieldRef, LayoutError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    Word(String),
    Text(String),
}

impl Tok {
    #[must_use]
    pub fn word(&self) -> Option<&str> {
        match self {
            Self::Word(w) => Some(w),
            Self::Text(_) => None,
        }
    }
}

/// The tokens of one line, without its comment: words split at spaces and tabs, a
/// double-quoted text is one token
pub fn tokenize(line: &str, n: usize) -> Result<Vec<Tok>, LayoutError> {
    let mut out = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some(&(i, c)) = chars.peek() {
        if c == ' ' || c == '\t' || c == '\r' {
            chars.next();
        } else if c == '#' {
            break;
        } else if c == '"' {
            chars.next();
            let start = i + 1;
            let mut end = None;
            for (j, d) in chars.by_ref() {
                if d == '"' {
                    end = Some(j);
                    break;
                }
            }
            let end = end.ok_or_else(|| err(n, "a text is not closed"))?;
            out.push(Tok::Text(line[start..end].to_owned()));
        } else {
            let start = i;
            let mut end = line.len();
            while let Some(&(j, d)) = chars.peek() {
                if d == ' ' || d == '\t' || d == '\r' || d == '#' || d == '"' {
                    end = j;
                    break;
                }
                chars.next();
            }
            out.push(Tok::Word(line[start..end].to_owned()));
        }
    }
    Ok(out)
}

pub fn normalize(toks: &[Tok]) -> String {
    let mut s = String::new();
    for (i, t) in toks.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        match t {
            Tok::Word(w) => s.push_str(w),
            Tok::Text(x) => {
                s.push('"');
                s.push_str(x);
                s.push('"');
            }
        }
    }
    s
}

#[must_use]
pub fn is_ident(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A non-negative integer literal: decimal or `0x` hex (at most 16 digits)
#[must_use]
pub fn parse_int(s: &str) -> Option<u64> {
    if let Some(h) = s.strip_prefix("0x") {
        if h.is_empty() || h.len() > 16 {
            return None;
        }
        u64::from_str_radix(h, 16).ok()
    } else if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        if s.len() > 1 && s.starts_with('0') {
            return None;
        }
        s.parse().ok()
    } else {
        None
    }
}

/// `0x` + an even number of hex digits
#[must_use]
pub fn parse_hex_bytes(s: &str) -> Option<Vec<u8>> {
    let h = s.strip_prefix("0x")?;
    if h.is_empty() || h.len() % 2 != 0 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).ok())
        .collect()
}

pub fn parse_tag(s: &str) -> Option<[u8; 4]> {
    let b = s.as_bytes();
    if b.len() == 4 && b.iter().all(u8::is_ascii_graphic) {
        Some([b[0], b[1], b[2], b[3]])
    } else {
        None
    }
}

pub fn parse_field_ref(s: &str) -> Option<FieldRef> {
    match s.split_once('.') {
        Some((seg, f)) if is_ident(seg) && is_ident(f) => Some(FieldRef {
            segment: Some(seg.to_owned()),
            field: f.to_owned(),
        }),
        None if is_ident(s) => Some(FieldRef {
            segment: None,
            field: s.to_owned(),
        }),
        _ => None,
    }
}
