//! The declaration: what a layout law says, as read by [`parse`]

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::cond::Cond;
pub use crate::read::parse;
pub(crate) use crate::token::{is_ident, parse_int, Tok};

/// Most lines a declaration may have (the text is untrusted input too)
pub const MAX_LINES: usize = 4096;
/// Longest line, in bytes
pub const MAX_LINE_LEN: usize = 1024;
/// Most blocks, segments, digests, derives or rules of each sort
pub const MAX_ITEMS: usize = 256;
/// Most fields in one block
pub const MAX_FIELDS: usize = 256;

/// Why a declaration does not read
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutError {
    /// 1-based line number (0 when the problem is the whole text)
    pub line: usize,
    /// What is wrong
    pub message: String,
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

pub fn err(line: usize, message: impl Into<String>) -> LayoutError {
    LayoutError {
        line,
        message: message.into(),
    }
}

/// Byte order of a multi-byte integer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    /// least significant byte first
    Little,
    /// most significant byte first
    Big,
}

/// A named constant (`const <name> <type> = <value>`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Const {
    /// `integer`: a non-negative integer below 2^64
    Integer(u64),
    /// `bytes`: `0x` followed by an even number of hex digits
    Bytes(Vec<u8>),
    /// `text`: a double-quoted string (no escapes, no double quote inside)
    Text(String),
    /// `tags`: 4-byte tags, in order
    Tags(Vec<[u8; 4]>),
}

/// The length of a `bytes` field or segment
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Len {
    /// a fixed number of bytes
    Fixed(u64),
    /// the value of an integer field read earlier (`<block field>` or `<segment>.<field>`)
    Field(FieldRef),
    /// everything up to the segments kept at the end (segments only)
    Rest,
}

/// A reference to a field: `<field>` inside the same block, `<segment>.<field>` elsewhere
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRef {
    /// the segment, `None` for a field of the same block
    pub segment: Option<String>,
    /// the field
    pub field: String,
}

/// The type of a field
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    /// an unsigned integer of 1, 2, 4 or 8 bytes; `None` = the declared `endian`
    Uint {
        /// width in bytes
        width: u8,
        /// a per-field override of the byte order
        endian: Option<Endian>,
    },
    /// a run of bytes
    Bytes(Len),
}

/// A value a field is compared with: a literal or a constant, typed when the declaration is
/// checked
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// an integer
    Integer(u64),
    /// bytes (a `bytes` or `text` constant, a `0x` literal on a bytes field, or a text literal)
    Bytes(Vec<u8>),
}

/// One field of a block
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// its name (unique in the block)
    pub name: String,
    /// its type
    pub ty: FieldType,
    /// `= <value>`: the only value it may hold
    pub equals: Option<Value>,
    /// `bits <mask>`: the bits it may have set
    pub bits: Option<u64>,
    /// `in <lo> <hi>`: the inclusive range it must lie in
    pub range: Option<(u64, u64)>,
    /// the label a refusal of this field carries
    pub label: String,
    pub(crate) line: usize,
}

/// A block: fields read one after the other
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// its name
    pub name: String,
    /// `size <n>`: the size every instance has (checked against the fields)
    pub size: Option<u64>,
    /// the fields, in order
    pub fields: Vec<Field>,
}

impl Block {
    /// The bytes the fixed-width fields take (the least an instance can be)
    #[must_use]
    pub fn min_size(&self) -> u64 {
        self.fields
            .iter()
            .map(|f| match &f.ty {
                FieldType::Uint { width, .. } => u64::from(*width),
                FieldType::Bytes(Len::Fixed(n)) => *n,
                FieldType::Bytes(_) => 0,
            })
            .fold(0u64, u64::saturating_add)
    }
}

/// What a segment holds
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentKind {
    /// one instance of a block
    Block(String),
    /// `count` instances of a block, the count read from an earlier integer field
    Table {
        /// the block of each row
        block: String,
        /// where the row count is
        count: FieldRef,
    },
    /// one run of bytes per row of a table, back to back
    Data {
        /// the table segment
        table: String,
        /// the row field that holds the run's length
        length: String,
        /// the row field that must hold the run's offset from the start of the input
        offset: Option<String>,
    },
    /// a run of bytes
    Bytes(Len),
    /// the rest as chunks of `size + plus` bytes, the last shorter: between `plus` and
    /// `size + plus - 1` bytes (a stream that ends right after a full chunk was cut short)
    Stream {
        /// the length of a full chunk before `plus`
        size: Len,
        /// the bytes every chunk adds (a tag)
        plus: u64,
        /// the most chunks a stream may have
        max: Option<u64>,
    },
}

/// One segment of the input, in order
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// its name (unique)
    pub name: String,
    /// what it holds
    pub kind: SegmentKind,
    /// `at end`: placed at the end of the input, before the others are read
    pub at_end: bool,
    /// the label a refusal of its size or placement carries
    pub label: String,
}

/// What a digest is compared with
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestOver {
    /// every byte before the segment the digest is (`digest <segment> sha256 before`)
    Before,
    /// the row's run of a data segment (`digest <table>.<field> sha256 <data segment>`)
    Data(String),
}

/// A digest that must hold (SHA-256)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digest {
    /// the segment holding it, or the table whose rows hold it
    pub segment: String,
    /// the row field holding it (tables only)
    pub field: Option<String>,
    /// what it is the digest of
    pub over: DigestOver,
    /// the label of its refusal
    pub label: String,
}

/// An encoding of an integer constant inside a derive
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntEncoding {
    /// 2 bytes little endian
    U16Le,
    /// 2 bytes big endian
    U16Be,
    /// 4 bytes little endian
    U32Le,
    /// 4 bytes big endian
    U32Be,
    /// 8 bytes little endian
    U64Le,
    /// 8 bytes big endian
    U64Be,
}

impl IntEncoding {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "u16le" => Self::U16Le,
            "u16be" => Self::U16Be,
            "u32le" => Self::U32Le,
            "u32be" => Self::U32Be,
            "u64le" => Self::U64Le,
            "u64be" => Self::U64Be,
            _ => return None,
        })
    }

    /// The spelling of the encoding in a law (`u16le` ...)
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::U16Le => "u16le",
            Self::U16Be => "u16be",
            Self::U32Le => "u32le",
            Self::U32Be => "u32be",
            Self::U64Le => "u64le",
            Self::U64Be => "u64be",
        }
    }

    /// The bytes of `v`, or `None` when it does not fit
    #[must_use]
    pub fn encode(self, v: u64) -> Option<Vec<u8>> {
        Some(match self {
            Self::U16Le => u16::try_from(v).ok()?.to_le_bytes().to_vec(),
            Self::U16Be => u16::try_from(v).ok()?.to_be_bytes().to_vec(),
            Self::U32Le => u32::try_from(v).ok()?.to_le_bytes().to_vec(),
            Self::U32Be => u32::try_from(v).ok()?.to_be_bytes().to_vec(),
            Self::U64Le => v.to_le_bytes().to_vec(),
            Self::U64Be => v.to_be_bytes().to_vec(),
        })
    }
}

/// One item hashed by a derive
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeriveItem {
    /// a constant; an integer one names its encoding (`MAJOR:u16le`)
    Const(String, Option<IntEncoding>),
    /// the bytes of a segment as read
    Segment(String),
}

/// A value computed from constants and segments
/// (`derive <name> sha256 [lenprefix u64be] <item> ...`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derive {
    /// its name
    pub name: String,
    /// every item is prefixed with its length as a big-endian `u64`
    pub length_prefixed: bool,
    /// the items, in order
    pub items: Vec<DeriveItem>,
}

/// A rule evaluated after the segments and digests, in order
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// `refuse <label> when <cond>`
    Refuse {
        /// the label of the refusal
        label: String,
        /// when to refuse
        cond: Cond,
        /// the condition as written (normalized spacing)
        text: String,
    },
    /// `status <name> when <cond>` / `status <name> otherwise`: the first that holds names the
    /// status of the input
    Status {
        /// the status
        name: String,
        /// `None` = otherwise
        cond: Option<Cond>,
        /// the condition as written (normalized spacing; empty for otherwise)
        text: String,
    },
}

/// A byte layout law
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// the name on the `law` line
    pub name: String,
    /// the byte order of integers without their own
    pub endian: Endian,
    /// the constants, in order
    pub consts: Vec<(String, Const)>,
    /// `minimum <n> refuse <label>`: the shortest input, checked first
    pub minimum: Option<(u64, String)>,
    /// the blocks
    pub blocks: Vec<Block>,
    /// the segments, in order
    pub segments: Vec<Segment>,
    /// the digests, in order
    pub digests: Vec<Digest>,
    /// the derived values
    pub derives: Vec<Derive>,
    /// the rules, in order
    pub rules: Vec<Rule>,
    /// the lines that make the identifier (normalized)
    pub(crate) id_lines: Vec<String>,
}

impl Layout {
    /// A constant by name
    #[must_use]
    pub fn constant(&self, name: &str) -> Option<&Const> {
        self.consts.iter().find(|(n, _)| n == name).map(|(_, c)| c)
    }

    /// A block by name
    #[must_use]
    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.name == name)
    }

    /// A segment by name
    #[must_use]
    pub fn segment(&self, name: &str) -> Option<&Segment> {
        self.segments.iter().find(|s| s.name == name)
    }
}

impl fmt::Display for Endian {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Little => "little endian",
            Self::Big => "big endian",
        })
    }
}

/// The text of a tag (its 4 bytes as ASCII)
#[must_use]
pub fn tag_text(t: &[u8; 4]) -> String {
    t.iter().map(|&b| char::from(b)).collect::<String>()
}
