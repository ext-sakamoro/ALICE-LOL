//! The residual of a law: the exact difference between a law and the reference it was fitted to,
//! in an envelope that does not depend on the kind of law
//!
//! A fitted model is a law plus its residual: the law is the part that generalises, the residual
//! what the law does not explain, stored exactly so the pair reproduces the reference. The
//! envelope says which law (and which instance of it) the residual belongs to, over which index
//! domains it is laid out, in which frame its values are, and holds its blocks of numbers; what the
//! numbers mean (where they are added, how to go between index points) is the kind's.
//!
//! ```text
//! residual <name>
//! for <kind tag> <law_id, 64 hex>
//! bound <sha256, 64 hex>
//! frame world | sheet-local
//! [transport <frame> over <param> ...]
//! index <domain> <words ...>            one per domain
//! between <domain> <rule words ...>     one per domain
//! [quantum <x>]
//! block <name> f64 <domain> <dims ...> <sha256> [when <param>=<value>]
//! block <name> sparse <domain> <count> <components> <sha256> [when <param>=<value>]
//! payload
//! ```
//! then the blocks' bytes, little-endian, in block order (`f64` values; a sparse block's `count`
//! `u32` indices, then its `count × components` values). `#` starts a comment line.
//!
//! - `bound` is the sha256 of the kind's canonical serialisation of the whole bound law
//!   ([`ResidualOwner::canonical_bound`]) without the parameters named by `transport` (continuous
//!   shape parameters the residual stays valid over, carried by the frame) and those that only
//!   appear in a block's `when` (selectors). Every other value, structural keys included, is in it,
//!   so a residual is refused by another instance of its law.
//! - A domain has as many elements as the product of the numbers among its words (`grid 2 3`: 6,
//!   `mesh vertices 10`: 10). A dense block holds a whole number of values per element; a sparse
//!   block's indices are below the element count.
//! - A block without `when` always applies; a block with `when <p>=<v>` applies when the law's
//!   selector `p` is `v` ([`LawResidual::active_blocks`]).
//! - The size in bits (the MDL rule): with `quantum q` a value `r` costs
//!   `1 + max(0, floor(log2(|r| / q)))` bits (from the float's exponent, no libm), without a
//!   quantum 64; a sparse index costs 32.
//! - The identifier ([`LawResidual::residual_id`]) is the sha256 of the canonical file (the header
//!   in the order above, single spaces, no comments, and every payload byte); the fitted model is
//!   the pair (law identifier, residual identifier).
//!
//! The rules a kind's own tests hold its residuals to: a residual of zeros evaluates as the law
//! alone, bit for bit; at the index points law plus residual is the reference (bit for bit, or
//! within half a quantum).

use std::fmt;
use std::fmt::Write as _;

use sha2::{Digest as _, Sha256};

/// The most blocks, domains and header lines
const MAX_BLOCKS: usize = 256;
const MAX_DOMAINS: usize = 16;
const MAX_HEADER_LINES: usize = 1024;
/// The most values in one block (2^28, 2 GiB of f64)
const MAX_VALUES: usize = 1 << 28;

/// The law a residual is read against
pub trait ResidualOwner {
    /// the kind tag of the law (as in its identifier, e.g. `lol.law.material/v1`)
    fn kind_tag(&self) -> &str;
    /// the law's identifier
    fn law_id(&self) -> [u8; 32];
    /// the canonical serialisation of the whole bound law (parameter values, structural keys and
    /// all), leaving out the named parameters
    fn canonical_bound(&self, excluded: &[&str]) -> Vec<u8>;
    /// the words of an index domain as the law lays it out, `None` for a domain it does not have
    fn index(&self, domain: &str) -> Option<Vec<String>>;
    /// the value of a selector parameter, as text
    fn selector(&self, param: &str) -> Option<String>;
}

/// sha256 of a kind's canonical bound law (the `bound` line)
#[must_use]
pub fn bound_hash(canonical: &[u8]) -> [u8; 32] {
    Sha256::digest(canonical).into()
}

/// A residual that does not read, or does not belong to the law it meets
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResidualError {
    /// the header or payload is malformed, with the 1-based header line (0 for the whole file)
    Malformed {
        /// the line, 0 when not one line
        line: usize,
        /// what is wrong
        message: String,
    },
    /// a residual of no block
    NoBlock,
    /// the residual belongs to another law, instance or index
    NotItsLaw(String),
}

impl fmt::Display for ResidualError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed { line: 0, message } => write!(f, "residual: {message}"),
            Self::Malformed { line, message } => write!(f, "residual, line {line}: {message}"),
            Self::NoBlock => write!(f, "residual: no block"),
            Self::NotItsLaw(m) => write!(f, "residual: not this law's: {m}"),
        }
    }
}

impl std::error::Error for ResidualError {}

fn bad(line: usize, message: impl Into<String>) -> ResidualError {
    ResidualError::Malformed {
        line,
        message: message.into(),
    }
}

/// One block of a residual
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// a value for every element (times the values per element)
    Dense {
        /// the block's name
        name: String,
        /// the domain it is laid out over
        domain: String,
        /// its dimensions (their product is the number of values)
        shape: Vec<usize>,
        /// the values, in row-major order
        values: Vec<f64>,
        /// the selector and value it applies under, if any
        when: Option<(String, String)>,
    },
    /// values at some elements only
    Sparse {
        /// the block's name
        name: String,
        /// the domain it is laid out over
        domain: String,
        /// the elements, each below the domain's element count
        indices: Vec<u32>,
        /// the values per element
        components: usize,
        /// `indices.len() × components` values, element by element
        values: Vec<f64>,
        /// the selector and value it applies under, if any
        when: Option<(String, String)>,
    },
}

impl Block {
    /// The block's name
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Dense { name, .. } | Self::Sparse { name, .. } => name,
        }
    }
    /// The domain the block is laid out over
    #[must_use]
    pub fn domain(&self) -> &str {
        match self {
            Self::Dense { domain, .. } | Self::Sparse { domain, .. } => domain,
        }
    }
    /// The selector and value the block applies under (`None`: always)
    #[must_use]
    pub fn when(&self) -> Option<(&str, &str)> {
        match self {
            Self::Dense { when, .. } | Self::Sparse { when, .. } => {
                when.as_ref().map(|(p, v)| (p.as_str(), v.as_str()))
            }
        }
    }
    /// The values (a sparse block's, element by element)
    #[must_use]
    pub fn values(&self) -> &[f64] {
        match self {
            Self::Dense { values, .. } | Self::Sparse { values, .. } => values,
        }
    }
    fn bytes(&self) -> Vec<u8> {
        match self {
            Self::Dense { values, .. } => values.iter().flat_map(|v| v.to_le_bytes()).collect(),
            Self::Sparse {
                indices, values, ..
            } => indices
                .iter()
                .flat_map(|i| i.to_le_bytes())
                .chain(values.iter().flat_map(|v| v.to_le_bytes()))
                .collect(),
        }
    }
}

/// A residual: its header and its blocks
#[derive(Debug, Clone, PartialEq)]
pub struct LawResidual {
    name: String,
    kind: String,
    law_id: [u8; 32],
    bound: [u8; 32],
    frame: String,
    transport: Option<(String, Vec<String>)>,
    /// (domain, words) in file order
    domains: Vec<(String, Vec<String>)>,
    /// (domain, rule words), in the order of `domains`
    between: Vec<(String, Vec<String>)>,
    /// the quantum as written, and its value
    quantum: Option<(String, f64)>,
    blocks: Vec<Block>,
}

fn hex(b: &[u8]) -> String {
    b.iter()
        .fold(String::with_capacity(2 * b.len()), |mut s, x| {
            let _ = write!(s, "{x:02x}");
            s
        })
}

fn unhex(line: usize, s: &str) -> Result<[u8; 32], ResidualError> {
    let b = s.as_bytes();
    if b.len() != 64 || !b.iter().all(u8::is_ascii_hexdigit) {
        return Err(bad(line, format!("`{s}` is not 64 hex digits")));
    }
    let mut out = [0u8; 32];
    for (k, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[2 * k..2 * k + 2], 16)
            .map_err(|_| bad(line, format!("`{s}` is not hex")))?;
    }
    Ok(out)
}

fn is_word(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'/'))
}

/// The element count of a domain: the product of the numbers among its words
fn element_count(words: &[String]) -> Option<usize> {
    words
        .iter()
        .filter_map(|w| w.parse::<usize>().ok())
        .try_fold(1usize, usize::checked_mul)
}

/// The bits a value costs under the quantum (see the module doc)
fn value_bits(r: f64, quantum: Option<f64>) -> u64 {
    match quantum {
        None | Some(0.0) => 64,
        Some(q) => {
            let x = r.abs() / q;
            if x < 1.0 {
                1
            } else {
                // floor(log2(x)) for a finite x >= 1: the unbiased exponent (the biased one is >= 1023)
                let biased = (x.to_bits() >> 52) & 0x7ff;
                1 + biased.saturating_sub(1023)
            }
        }
    }
}

impl LawResidual {
    /// Reads a residual file
    ///
    /// # Errors
    ///
    /// [`ResidualError::Malformed`] for a header that does not read, a block whose bytes are not
    /// its sha256 or that does not fit its domain or shape, a value that is not finite, or a
    /// payload of another length; [`ResidualError::NoBlock`] for a residual of no block
    #[allow(clippy::too_many_lines)] // one check per line kind, in order
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ResidualError> {
        // the block lines until the payload is read: (line, name, dense?, domain, dims, sha, when)
        type Pending = (
            usize,
            String,
            bool,
            String,
            Vec<usize>,
            [u8; 32],
            Option<(String, String)>,
        );
        const MARK: &[u8] = b"payload\n";
        let split = bytes
            .windows(MARK.len())
            .position(|w| w == MARK)
            .filter(|&p| p == 0 || bytes[p - 1] == b'\n')
            .ok_or_else(|| bad(0, "no `payload` line"))?;
        let header =
            std::str::from_utf8(&bytes[..split]).map_err(|_| bad(0, "the header is not UTF-8"))?;
        let mut payload = &bytes[split + MARK.len()..];
        let (mut name, mut kind, mut bound, mut frame) = (None, None, None, None);
        let mut law_id = None;
        let mut transport = None;
        let mut domains: Vec<(String, Vec<String>)> = Vec::new();
        let mut between: Vec<(String, Vec<String>)> = Vec::new();
        let mut quantum = None;
        let mut pending: Vec<Pending> = Vec::new();
        let lines: Vec<&str> = header.lines().collect();
        if lines.len() > MAX_HEADER_LINES {
            return Err(bad(0, format!("more than {MAX_HEADER_LINES} header lines")));
        }
        for (k, raw) in lines.iter().enumerate() {
            let line = k + 1;
            let t = raw.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let w: Vec<&str> = t.split_whitespace().collect();
            let once = |slot: bool, what: &str| {
                if slot {
                    Err(bad(line, format!("`{what}` given twice")))
                } else {
                    Ok(())
                }
            };
            match w[0] {
                "residual" => {
                    once(name.is_some(), "residual")?;
                    let [_, n] = w[..] else {
                        return Err(bad(line, "`residual <name>` expected"));
                    };
                    if !is_word(n) {
                        return Err(bad(line, "a residual's name is one word"));
                    }
                    name = Some(n.to_owned());
                }
                "for" => {
                    once(kind.is_some(), "for")?;
                    let [_, tag, id] = w[..] else {
                        return Err(bad(line, "`for <kind tag> <law_id>` expected"));
                    };
                    if !is_word(tag) {
                        return Err(bad(line, "a kind tag is one word"));
                    }
                    kind = Some(tag.to_owned());
                    law_id = Some(unhex(line, id)?);
                }
                "bound" => {
                    once(bound.is_some(), "bound")?;
                    let [_, h] = w[..] else {
                        return Err(bad(line, "`bound <sha256>` expected"));
                    };
                    bound = Some(unhex(line, h)?);
                }
                "frame" => {
                    once(frame.is_some(), "frame")?;
                    let [_, f @ ("world" | "sheet-local")] = w[..] else {
                        return Err(bad(line, "`frame world` or `frame sheet-local` expected"));
                    };
                    frame = Some(f.to_owned());
                }
                "transport" => {
                    once(transport.is_some(), "transport")?;
                    if w.len() < 4 || w[2] != "over" || !matches!(w[1], "world" | "sheet-local") {
                        return Err(bad(line, "`transport <frame> over <param> ...` expected"));
                    }
                    let params: Vec<String> = w[3..].iter().map(|s| (*s).to_owned()).collect();
                    if !params.iter().all(|p| is_word(p)) {
                        return Err(bad(line, "a transported parameter is one word"));
                    }
                    transport = Some((w[1].to_owned(), params));
                }
                "index" => {
                    if w.len() < 3 || !w[1..].iter().all(|s| is_word(s)) {
                        return Err(bad(line, "`index <domain> <words ...>` expected"));
                    }
                    if domains.iter().any(|(d, _)| d == w[1]) {
                        return Err(bad(line, format!("domain `{}` declared twice", w[1])));
                    }
                    if domains.len() == MAX_DOMAINS {
                        return Err(bad(line, format!("more than {MAX_DOMAINS} domains")));
                    }
                    let words: Vec<String> = w[2..].iter().map(|s| (*s).to_owned()).collect();
                    if element_count(&words).is_none_or(|n| n == 0) {
                        return Err(bad(line, "a domain of no elements"));
                    }
                    domains.push((w[1].to_owned(), words));
                }
                "between" => {
                    if w.len() < 3 || !w[1..].iter().all(|s| is_word(s)) {
                        return Err(bad(line, "`between <domain> <rule ...>` expected"));
                    }
                    if between.iter().any(|(d, _)| d == w[1]) {
                        return Err(bad(line, format!("`between {}` given twice", w[1])));
                    }
                    between.push((
                        w[1].to_owned(),
                        w[2..].iter().map(|s| (*s).to_owned()).collect(),
                    ));
                }
                "quantum" => {
                    once(quantum.is_some(), "quantum")?;
                    let [_, q] = w[..] else {
                        return Err(bad(line, "`quantum <x>` expected"));
                    };
                    let plain = q.bytes().all(|c| c.is_ascii_digit() || c == b'.');
                    let v: f64 = q.parse().map_err(|_| bad(line, "a quantum is a number"))?;
                    if !plain || !v.is_finite() || v < 0.0 {
                        return Err(bad(line, "a quantum is a plain non-negative decimal"));
                    }
                    quantum = Some((q.to_owned(), v));
                }
                "block" => {
                    let (body, when) = match w.iter().position(|s| *s == "when") {
                        Some(p) => {
                            let [cond] = w[p + 1..] else {
                                return Err(bad(line, "`when <param>=<value>` expected"));
                            };
                            let Some((par, val)) = cond.split_once('=') else {
                                return Err(bad(line, "`when <param>=<value>` expected"));
                            };
                            if !is_word(par) || !is_word(val) {
                                return Err(bad(line, "`when <param>=<value>` expected"));
                            }
                            (&w[..p], Some((par.to_owned(), val.to_owned())))
                        }
                        None => (&w[..], None),
                    };
                    if body.len() < 6 || !is_word(body[1]) {
                        return Err(bad(
                            line,
                            "`block <name> <type> <domain> ... <sha256>` expected",
                        ));
                    }
                    if pending.iter().any(|b| b.1 == body[1]) {
                        return Err(bad(line, format!("block `{}` given twice", body[1])));
                    }
                    if pending.len() == MAX_BLOCKS {
                        return Err(bad(line, format!("more than {MAX_BLOCKS} blocks")));
                    }
                    let dims: Vec<usize> = body[4..body.len() - 1]
                        .iter()
                        .map(|s| s.parse::<usize>())
                        .collect::<Result<_, _>>()
                        .map_err(|_| bad(line, "a block's dimensions are whole numbers"))?;
                    let dense = match body[2] {
                        "f64" => true,
                        "sparse" if dims.len() == 2 => false,
                        "sparse" => {
                            return Err(bad(line, "a sparse block gives <count> <components>"))
                        }
                        other => return Err(bad(line, format!("unknown block type `{other}`"))),
                    };
                    if dims.contains(&0) {
                        return Err(bad(line, "a zero dimension"));
                    }
                    let sha = unhex(line, body[body.len() - 1])?;
                    pending.push((
                        line,
                        body[1].to_owned(),
                        dense,
                        body[3].to_owned(),
                        dims,
                        sha,
                        when,
                    ));
                }
                other => return Err(bad(line, format!("unknown line `{other}`"))),
            }
        }
        let need =
            |v: Option<String>, what: &str| v.ok_or_else(|| bad(0, format!("no `{what}` line")));
        let name = need(name, "residual")?;
        let kind = need(kind, "for")?;
        let frame = need(frame, "frame")?;
        let law_id = law_id.ok_or_else(|| bad(0, "no `for` line"))?;
        let bound = bound.ok_or_else(|| bad(0, "no `bound` line"))?;
        if pending.is_empty() {
            return Err(ResidualError::NoBlock);
        }
        // every domain has a rule between its points, every rule a domain; in domain order
        let mut ordered = Vec::with_capacity(domains.len());
        for (d, _) in &domains {
            let r = between
                .iter()
                .find(|(b, _)| b == d)
                .ok_or_else(|| bad(0, format!("domain `{d}` has no `between` line")))?;
            ordered.push(r.clone());
        }
        if between.len() != domains.len() {
            return Err(bad(0, "a `between` line for an undeclared domain"));
        }
        let mut blocks = Vec::with_capacity(pending.len());
        for (line, bname, dense, domain, dims, sha, when) in pending {
            let words = &domains
                .iter()
                .find(|(d, _)| *d == domain)
                .ok_or_else(|| bad(line, format!("domain `{domain}` is not declared")))?
                .1;
            let elements = element_count(words).unwrap_or(0);
            let (n_idx, n_val) = if dense {
                let n = dims
                    .iter()
                    .try_fold(1usize, |a, &d| a.checked_mul(d))
                    .filter(|&n| n <= MAX_VALUES)
                    .ok_or_else(|| bad(line, "a block too large"))?;
                if n % elements != 0 {
                    return Err(bad(
                        line,
                        format!("{n} values do not fit the {elements} elements of `{domain}`"),
                    ));
                }
                (0, n)
            } else {
                let (count, comps) = (dims[0], dims[1]);
                let n = count
                    .checked_mul(comps)
                    .filter(|&n| n <= MAX_VALUES)
                    .ok_or_else(|| bad(line, "a block too large"))?;
                (count, n)
            };
            let len = 4 * n_idx + 8 * n_val;
            if payload.len() < len {
                return Err(bad(
                    line,
                    format!("the payload ends inside block `{bname}`"),
                ));
            }
            let (mine, rest) = payload.split_at(len);
            payload = rest;
            if bound_hash(mine) != sha {
                return Err(bad(
                    line,
                    format!("block `{bname}`: its bytes are not its sha256"),
                ));
            }
            let (ib, vb) = mine.split_at(4 * n_idx);
            let indices: Vec<u32> = ib
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect();
            let values: Vec<f64> = vb
                .as_chunks::<8>()
                .0
                .iter()
                .map(|c| f64::from_le_bytes(*c))
                .collect();
            if !values.iter().all(|v| v.is_finite()) {
                return Err(bad(
                    line,
                    format!("block `{bname}`: a value that is not finite"),
                ));
            }
            blocks.push(if dense {
                Block::Dense {
                    name: bname,
                    domain,
                    shape: dims,
                    values,
                    when,
                }
            } else {
                Block::Sparse {
                    name: bname,
                    domain,
                    indices,
                    components: dims[1],
                    values,
                    when,
                }
            });
        }
        if !payload.is_empty() {
            return Err(bad(
                0,
                format!("{} payload bytes after the last block", payload.len()),
            ));
        }
        Ok(Self {
            name,
            kind,
            law_id,
            bound,
            frame,
            transport,
            domains,
            between: ordered,
            quantum,
            blocks,
        })
    }

    /// The file in its canonical form (see the module doc)
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut h = format!(
            "residual {}\nfor {} {}\nbound {}\nframe {}\n",
            self.name,
            self.kind,
            hex(&self.law_id),
            hex(&self.bound),
            self.frame
        );
        if let Some((f, ps)) = &self.transport {
            let _ = writeln!(h, "transport {f} over {}", ps.join(" "));
        }
        for (d, w) in &self.domains {
            let _ = writeln!(h, "index {d} {}", w.join(" "));
        }
        for (d, r) in &self.between {
            let _ = writeln!(h, "between {d} {}", r.join(" "));
        }
        if let Some((q, _)) = &self.quantum {
            let _ = writeln!(h, "quantum {q}");
        }
        let mut payload = Vec::new();
        for b in &self.blocks {
            let bytes = b.bytes();
            let shape = match b {
                Block::Dense { shape, .. } => format!(
                    "f64 {} {}",
                    b.domain(),
                    shape
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                Block::Sparse {
                    indices,
                    components,
                    ..
                } => {
                    format!("sparse {} {} {components}", b.domain(), indices.len())
                }
            };
            let _ = write!(h, "block {} {shape} {}", b.name(), hex(&bound_hash(&bytes)));
            if let Some((p, v)) = b.when() {
                let _ = write!(h, " when {p}={v}");
            }
            h.push('\n');
            payload.extend(bytes);
        }
        h += "payload\n";
        let mut out = h.into_bytes();
        out.extend(payload);
        out
    }

    /// The residual's identifier: the sha256 of its canonical file
    #[must_use]
    pub fn residual_id(&self) -> [u8; 32] {
        bound_hash(&self.to_bytes())
    }

    /// The residual's name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The kind tag of the law it belongs to
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }
    /// The identifier of the law it belongs to
    #[must_use]
    pub const fn law_id(&self) -> [u8; 32] {
        self.law_id
    }
    /// The `bound` hash
    #[must_use]
    pub const fn bound(&self) -> [u8; 32] {
        self.bound
    }
    /// `world` or `sheet-local`
    #[must_use]
    pub fn frame(&self) -> &str {
        &self.frame
    }
    /// The words of a domain, `None` for an undeclared one
    #[must_use]
    pub fn index(&self, domain: &str) -> Option<&[String]> {
        self.domains
            .iter()
            .find(|(d, _)| d == domain)
            .map(|(_, w)| w.as_slice())
    }
    /// The rule between a domain's points
    #[must_use]
    pub fn between(&self, domain: &str) -> Option<&[String]> {
        self.between
            .iter()
            .find(|(d, _)| d == domain)
            .map(|(_, w)| w.as_slice())
    }
    /// A block by name
    #[must_use]
    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.name() == name)
    }
    /// Every block, in file order
    #[must_use]
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The parameters left out of `bound`: the transported ones and the selectors of `when`,
    /// sorted, each once
    #[must_use]
    pub fn excluded_params(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .transport
            .iter()
            .flat_map(|(_, ps)| ps.iter().map(String::as_str))
            .chain(self.blocks.iter().filter_map(|b| b.when().map(|(p, _)| p)))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// That the residual belongs to `law`: its kind, identifier, bound instance and every
    /// domain's words, and every sparse index within its domain
    ///
    /// # Errors
    ///
    /// [`ResidualError::NotItsLaw`] naming the first that differs
    pub fn check_belongs(&self, law: &dyn ResidualOwner) -> Result<(), ResidualError> {
        let no = |m: String| Err(ResidualError::NotItsLaw(m));
        if law.kind_tag() != self.kind {
            return no(format!(
                "kind `{}`, the law is `{}`",
                self.kind,
                law.kind_tag()
            ));
        }
        if law.law_id() != self.law_id {
            return no("another law identifier".into());
        }
        if bound_hash(&law.canonical_bound(&self.excluded_params())) != self.bound {
            return no("another instance of the law (bound values or structure)".into());
        }
        for (d, w) in &self.domains {
            match law.index(d) {
                Some(lw) if lw == *w => {}
                Some(lw) => {
                    return no(format!(
                        "domain `{d}` is `{}`, the law's `{}`",
                        w.join(" "),
                        lw.join(" ")
                    ))
                }
                None => return no(format!("the law has no domain `{d}`")),
            }
        }
        for b in &self.blocks {
            if let Block::Sparse {
                indices, domain, ..
            } = b
            {
                let n = self.index(domain).and_then(element_count).unwrap_or(0);
                if indices.iter().any(|&i| i as usize >= n) {
                    return no(format!(
                        "block `{}` has an index past the {n} elements of `{domain}`",
                        b.name()
                    ));
                }
            }
        }
        Ok(())
    }

    /// The blocks that apply to `law`: every block without `when`, and those whose selector the
    /// law has at their value, in file order
    ///
    /// # Errors
    ///
    /// As [`Self::check_belongs`]
    pub fn active_blocks(&self, law: &dyn ResidualOwner) -> Result<Vec<&Block>, ResidualError> {
        self.check_belongs(law)?;
        Ok(self
            .blocks
            .iter()
            .filter(|b| match b.when() {
                None => true,
                Some((p, v)) => law.selector(p).as_deref() == Some(v),
            })
            .collect())
    }

    /// A block's size in bits (see the module doc), `None` for no such block
    #[must_use]
    pub fn block_bits(&self, name: &str) -> Option<u64> {
        let q = self.quantum.as_ref().map(|(_, v)| *v);
        self.block(name).map(|b| {
            let idx = match b {
                Block::Sparse { indices, .. } => 32 * indices.len() as u64,
                Block::Dense { .. } => 0,
            };
            idx + b.values().iter().map(|&r| value_bits(r, q)).sum::<u64>()
        })
    }

    /// Every block's size in bits, summed
    #[must_use]
    pub fn total_bits(&self) -> u64 {
        self.blocks
            .iter()
            .filter_map(|b| self.block_bits(b.name()))
            .sum()
    }
}
