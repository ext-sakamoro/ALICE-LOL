//! Skeleton laws (`kind skeleton`): named joints in a hierarchy, placed by expressions of named
//! parameters
//!
//! A skeleton law declares parameters (each with a default and a range), intermediate values and
//! joints. A joint is a root at a point, or an offset from its parent; a chain is a run of joints,
//! each an offset from the one before, whose expressions read the joint's place `i` in the run
//! (1, 2, ...), so a chain's shape is a law of its own (a length profile, a bend). A `mirror` line
//! gives every joint named with one suffix a twin named with the other, its offset's x negated,
//! under its parent's twin. The law knows nothing of what the joints are; a consumer reads them
//! by name. Expressions are those of research laws (`f64` without fused multiply-add,
//! `alice-det-math` for every function) and the joints' places stay `f64`.
//!
//! ```text
//! law arm
//! kind skeleton
//! claim an arm hanging from a shoulder, two segments and a three-joint finger
//! param upper number = 0.3 range 0.2 0.4
//! param curl number = 0.1 range 0 0.5
//! joint shoulder_l = (0.2, 1.4, 0)
//! joint elbow_l from shoulder_l = (upper, 0, 0)
//! chain finger_l from elbow_l count 3 = (0.05 / i, 0 - curl * i * 0.01, 0)
//! mirror _l _r
//! ```
//!
//! # Lines
//!
//! - `law <name>` first, then `kind skeleton`; `claim` / `source` lines are notes (not in the
//!   identifier); `#` starts a comment line
//! - `param <name> number = <default> range <lo> <hi>`: a named number, `lo <= default <= hi`
//! - `let <name> = <expr>`: a value for the later lines, before any joint
//! - `joint <name> = (<expr>, <expr>, <expr>)`: a root joint at that point
//! - `joint <name> from <parent> = (<expr>, <expr>, <expr>)`: a joint at its parent plus that
//!   offset; the parent is a joint declared before it
//! - `chain <name> from <parent> count <n> = (<expr>, <expr>, <expr>)`: joints `<name>1` ..
//!   `<name><n>` (1 <= n <= 64), each at the one before (the first at the parent) plus the
//!   offset, whose expressions may read `i`, the joint's place in the chain
//! - `mirror <a> <b>`: at most one, anywhere after the kind; after the declared joints, every
//!   joint whose name (for a chain's joint, its chain's name) ends with suffix `a` and whose twin
//!   (the name with `b` in its place; `finger_l` 2 gives `finger_r2`) is not declared gets that
//!   twin: its parent is the parent's twin when there is one (else the same
//!   parent), its offset (a root's point) has x negated. Suffixes are `_` and lowercase letters
//!   or digits, and differ
//!
//! Expressions read `pi`, the parameters and the earlier `let` values (and `i` on a chain line).
//! Every name is declared once across parameters, lets and joints (a chain's joints and the
//! twins count); `i` is reserved. A law has at least one joint, and its defaults must place
//! every joint at finite numbers.

use std::fmt;

use crate::law_id::{fold_semantics_pins, hex32, LawIdHasher};
use crate::motion_law::SampleError;
use crate::research_law::{CompiledExpr, LawExpr};

/// The kind tag of a skeleton law, hashed into its identifier
pub const SKELETON_LAW_KIND: &[u8] = b"lol.law.skeleton/v1";

/// The most parameters
pub const MAX_PARAMS: usize = 1024;
/// The most `let` lines
pub const MAX_LETS: usize = 1024;
/// The most joints, the chains' and the twins counted
pub const MAX_JOINTS: usize = 2048;
/// The most joints of one chain
pub const MAX_CHAIN: u32 = 64;
/// The most lines
pub const MAX_LINES: usize = 4096;

/// What the reading of a skeleton law means, as pins measured from the behaviour of this module
/// ([`skeleton_semantics_fingerprints`]; the oracle recomputes every one)
pub const SKELETON_SEMANTICS_PINS: &[(&str, [u8; 32])] = &[
    (
        "skeleton: joints as f64 sums of research-law offsets down the hierarchy, chains by place, mirrored twins",
        hex32("080b547ae824d26de22fc0faabdf77d1566f398f0a66d8cbe37573e6c6f89f28"),
    ),
    (
        "skeleton: the expression functions of research laws",
        hex32("ee1b39dde2b1e577892f19daee2fb514a7b43f56be7d2211061de1ddac28fad3"),
    ),
];

/// The fold of [`SKELETON_SEMANTICS_PINS`] (the oracle checks the two agree)
pub const SKELETON_SEMANTICS_ID: [u8; 32] =
    hex32("ebff472c0cf042ab31ef4797d26f8b5cc6641e2147d580e4a7b03813c92b6bb4");

/// A skeleton law that does not read, with the line it was on (0 for the whole file)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonLawError {
    /// The 1-based line, 0 when the error is not on one line
    pub line: usize,
    /// What is wrong
    pub message: String,
}

impl fmt::Display for SkeletonLawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "skeleton law: {}", self.message)
        } else {
            write!(f, "skeleton law, line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for SkeletonLawError {}

fn err(line: usize, message: impl Into<String>) -> SkeletonLawError {
    SkeletonLawError {
        line,
        message: message.into(),
    }
}

/// A declared parameter
#[derive(Debug, Clone, PartialEq)]
pub struct SkeletonParam {
    /// Its name
    pub name: String,
    /// The value it takes when not given
    pub default: f64,
    /// The least value it takes
    pub lo: f64,
    /// The greatest value it takes
    pub hi: f64,
}

/// How a joint's place is written
#[derive(Debug, Clone)]
enum Place {
    /// A root at the point
    Root([CompiledExpr; 3]),
    /// The parent's place plus the offset
    From(usize, [CompiledExpr; 3]),
    /// The `k`-th joint (1-based) of a chain: the joint before (the parent for the first) plus
    /// the offset, `i` = k
    Chain(usize, u32, [CompiledExpr; 3]),
}

#[derive(Debug, Clone)]
struct Joint {
    name: String,
    /// For a chain's joint, the chain's name (the mirror reads its suffix there)
    chain: Option<String>,
    parent: Option<usize>,
    place: Place,
    /// x negated (a mirrored twin)
    mirrored: bool,
}

/// A declared line in the identifier's terms: the expressions as trees
#[derive(Debug, Clone)]
enum Decl {
    Joint {
        name: String,
        parent: Option<String>,
        exprs: [LawExpr; 3],
    },
    Chain {
        name: String,
        parent: String,
        count: u32,
        exprs: [LawExpr; 3],
    },
}

/// A skeleton law that read
#[derive(Debug, Clone)]
pub struct SkeletonLaw {
    name: String,
    params: Vec<SkeletonParam>,
    lets: Vec<(String, LawExpr, CompiledExpr)>,
    decls: Vec<Decl>,
    mirror: Option<(String, String)>,
    joints: Vec<Joint>,
}

/// A skeleton law with parameter values, and its joints' places
#[derive(Debug, Clone)]
pub struct BoundSkeleton<'a> {
    law: &'a SkeletonLaw,
    values: Vec<f64>,
    places: Vec<[f64; 3]>,
}

const RESERVED: [&str; 2] = ["pi", "i"];

fn is_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

fn is_suffix(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}

fn number(s: &str) -> Option<f64> {
    let v: f64 = s.parse().ok()?;
    let plain = s
        .bytes()
        .all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'-'));
    (plain && v.is_finite()).then_some(v)
}

fn expr(line: usize, text: &str) -> Result<LawExpr, SkeletonLawError> {
    LawExpr::parse(text).map_err(|e| err(line, format!("`{}`: {e}", text.trim())))
}

/// The three expressions of `(a, b, c)`, split at the commas outside any parentheses
fn triple(line: usize, text: &str) -> Result<[&str; 3], SkeletonLawError> {
    let t = text.trim();
    let inner = t
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .ok_or_else(|| err(line, "`= (<expr>, <expr>, <expr>)` expected"))?;
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    for (k, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&inner[start..k]);
                start = k + 1;
            }
            _ => {}
        }
        if depth < 0 {
            return Err(err(line, "unbalanced parentheses"));
        }
    }
    parts.push(&inner[start..]);
    <[&str; 3]>::try_from(parts)
        .map_err(|p| err(line, format!("a place has 3 coordinates, not {}", p.len())))
}

/// The three expressions, parsed and checked against the names they may read
fn place(
    line: usize,
    text: &str,
    names: &[&str],
) -> Result<([LawExpr; 3], [CompiledExpr; 3]), SkeletonLawError> {
    let [a, b, c] = triple(line, text)?;
    let mut exprs = Vec::new();
    let mut codes = Vec::new();
    for t in [a, b, c] {
        let e = expr(line, t)?;
        codes.push(
            e.compile_named(names)
                .map_err(|e| err(line, e.to_string()))?,
        );
        exprs.push(e);
    }
    let exprs: [LawExpr; 3] = exprs.try_into().unwrap_or_else(|_| unreachable!());
    let codes: [CompiledExpr; 3] = codes.try_into().unwrap_or_else(|_| unreachable!());
    Ok((exprs, codes))
}

impl SkeletonLaw {
    /// Reads a skeleton law file
    ///
    /// # Errors
    ///
    /// [`SkeletonLawError`] naming the line: not `kind skeleton`, an unknown line, a name declared
    /// twice or reserved, a parameter whose default is outside its range, a `let` after a joint,
    /// a parent not declared before, a chain count outside 1..=64, a second `mirror` or bad
    /// suffixes, an expression that does not parse or names something not declared before it, a
    /// place without three coordinates, no joint, more lines, parameters, lets or joints than
    /// the limits, or a joint that is not finite with the defaults
    #[allow(clippy::too_many_lines)] // one arm per kind of line, in file order
    pub fn from_file(text: &str) -> Result<Self, SkeletonLawError> {
        let mut name = None;
        let mut kind = false;
        let mut params: Vec<SkeletonParam> = Vec::new();
        let mut lets: Vec<(String, LawExpr, CompiledExpr)> = Vec::new();
        let mut decls: Vec<Decl> = Vec::new();
        let mut joints: Vec<Joint> = Vec::new();
        let mut mirror: Option<(String, String)> = None;
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() > MAX_LINES {
            return Err(err(0, format!("more than {MAX_LINES} lines")));
        }
        for (index, raw) in lines.iter().enumerate() {
            let line = index + 1;
            if raw.chars().any(|c| c.is_control() && c != '\t') {
                return Err(err(line, "a control character"));
            }
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let (word, rest) = trimmed
                .split_once(' ')
                .map_or((trimmed, ""), |(head, tail)| (head, tail.trim()));
            if name.is_none() {
                if word != "law" || !is_name(rest) {
                    return Err(err(line, "the first line is `law <name>`"));
                }
                name = Some(rest.to_owned());
                continue;
            }
            if !kind {
                if trimmed != "kind skeleton" {
                    return Err(err(line, "the second line is `kind skeleton`"));
                }
                kind = true;
                continue;
            }
            let taken = |n: &str,
                         params: &[SkeletonParam],
                         lets: &[(String, LawExpr, CompiledExpr)],
                         joints: &[Joint]| {
                RESERVED.contains(&n)
                    || params.iter().any(|p| p.name == n)
                    || lets.iter().any(|(l, _, _)| l == n)
                    || joints.iter().any(|j| j.name == n)
            };
            let names = |params: &[SkeletonParam],
                         lets: &[(String, LawExpr, CompiledExpr)]|
             -> Vec<String> {
                let mut v: Vec<String> = vec!["pi".to_owned()];
                v.extend(params.iter().map(|p| p.name.clone()));
                v.extend(lets.iter().map(|(l, _, _)| l.clone()));
                v
            };
            match word {
                "claim" | "source" => {}
                "param" => {
                    let w: Vec<&str> = rest.split_whitespace().collect();
                    let [n, "number", "=", d, "range", lo, hi] = w[..] else {
                        return Err(err(
                            line,
                            "`param <name> number = <default> range <lo> <hi>` expected",
                        ));
                    };
                    if !is_name(n) || taken(n, &params, &lets, &joints) {
                        return Err(err(line, format!("`{n}` is reserved or declared twice")));
                    }
                    if !lets.is_empty() || !joints.is_empty() {
                        return Err(err(line, "`param` comes before `let` and the joints"));
                    }
                    let (Some(d), Some(lo), Some(hi)) = (number(d), number(lo), number(hi)) else {
                        return Err(err(line, "a parameter's numbers are plain decimals"));
                    };
                    if !(lo <= d && d <= hi) {
                        return Err(err(line, "the default is outside the range"));
                    }
                    if params.len() == MAX_PARAMS {
                        return Err(err(line, format!("more than {MAX_PARAMS} parameters")));
                    }
                    params.push(SkeletonParam {
                        name: n.to_owned(),
                        default: d,
                        lo,
                        hi,
                    });
                }
                "let" => {
                    let Some((n, e)) = rest.split_once('=') else {
                        return Err(err(line, "`let <name> = <expr>` expected"));
                    };
                    let n = n.trim();
                    if !is_name(n) || taken(n, &params, &lets, &joints) {
                        return Err(err(line, format!("`{n}` is reserved or declared twice")));
                    }
                    if !joints.is_empty() {
                        return Err(err(line, "`let` comes before the joints"));
                    }
                    if lets.len() == MAX_LETS {
                        return Err(err(line, format!("more than {MAX_LETS} `let` lines")));
                    }
                    let e = expr(line, e)?;
                    let ns = names(&params, &lets);
                    let ns: Vec<&str> = ns.iter().map(String::as_str).collect();
                    let code = e.compile_named(&ns).map_err(|e| err(line, e.to_string()))?;
                    lets.push((n.to_owned(), e, code));
                }
                "joint" | "chain" => {
                    let Some((head, rhs)) = rest.split_once('=') else {
                        return Err(err(line, format!("`{word} <name> ... = (...)` expected")));
                    };
                    let words: Vec<&str> = head.split_whitespace().collect();
                    let ns = names(&params, &lets);
                    let mut ns: Vec<&str> = ns.iter().map(String::as_str).collect();
                    let parent_of = |p: &str, joints: &[Joint]| {
                        joints.iter().position(|j| j.name == p).ok_or_else(|| {
                            err(line, format!("parent `{p}` is not a joint declared before"))
                        })
                    };
                    if word == "joint" {
                        let (n, parent) = match words[..] {
                            [n] => (n, None),
                            [n, "from", p] => (n, Some(p)),
                            _ => {
                                return Err(err(
                                    line,
                                    "`joint <name> [from <parent>] = (<expr>, <expr>, <expr>)` expected",
                                ))
                            }
                        };
                        if !is_name(n) || taken(n, &params, &lets, &joints) {
                            return Err(err(line, format!("`{n}` is reserved or declared twice")));
                        }
                        let pi = parent.map(|p| parent_of(p, &joints)).transpose()?;
                        let (exprs, codes) = place(line, rhs, &ns)?;
                        if joints.len() == MAX_JOINTS {
                            return Err(err(line, format!("more than {MAX_JOINTS} joints")));
                        }
                        joints.push(Joint {
                            name: n.to_owned(),
                            chain: None,
                            parent: pi,
                            place: match pi {
                                Some(p) => Place::From(p, codes),
                                None => Place::Root(codes),
                            },
                            mirrored: false,
                        });
                        decls.push(Decl::Joint {
                            name: n.to_owned(),
                            parent: parent.map(str::to_owned),
                            exprs,
                        });
                    } else {
                        let [n, "from", p, "count", c] = words[..] else {
                            return Err(err(
                                line,
                                "`chain <name> from <parent> count <n> = (<expr>, <expr>, <expr>)` expected",
                            ));
                        };
                        let Ok(count) = c.parse::<u32>() else {
                            return Err(err(line, "a chain's count is a whole number"));
                        };
                        if !(1..=MAX_CHAIN).contains(&count) || c.starts_with('0') {
                            return Err(err(line, format!("a chain has 1 to {MAX_CHAIN} joints")));
                        }
                        if !is_name(n) || n.ends_with(|ch: char| ch.is_ascii_digit()) {
                            return Err(err(
                                line,
                                format!("`{n}` is not a chain name (a name not ending in a digit)"),
                            ));
                        }
                        for k in 1..=count {
                            let jn = format!("{n}{k}");
                            if taken(&jn, &params, &lets, &joints) {
                                return Err(err(
                                    line,
                                    format!("`{jn}` is reserved or declared twice"),
                                ));
                            }
                        }
                        let pi = parent_of(p, &joints)?;
                        ns.push("i");
                        let (exprs, codes) = place(line, rhs, &ns)?;
                        if joints.len() + count as usize > MAX_JOINTS {
                            return Err(err(line, format!("more than {MAX_JOINTS} joints")));
                        }
                        for k in 1..=count {
                            let parent = if k == 1 { pi } else { joints.len() - 1 };
                            joints.push(Joint {
                                name: format!("{n}{k}"),
                                chain: Some(n.to_owned()),
                                parent: Some(parent),
                                place: Place::Chain(parent, k, codes.clone()),
                                mirrored: false,
                            });
                        }
                        decls.push(Decl::Chain {
                            name: n.to_owned(),
                            parent: p.to_owned(),
                            count,
                            exprs,
                        });
                    }
                }
                "mirror" => {
                    let w: Vec<&str> = rest.split_whitespace().collect();
                    let [a, b] = w[..] else {
                        return Err(err(line, "`mirror <suffix> <suffix>` expected"));
                    };
                    if mirror.is_some() {
                        return Err(err(line, "a second `mirror` line"));
                    }
                    if !is_suffix(a) || !is_suffix(b) || a == b {
                        return Err(err(
                            line,
                            "mirror suffixes are `_`, lowercase letters or digits, and differ",
                        ));
                    }
                    mirror = Some((a.to_owned(), b.to_owned()));
                }
                _ => return Err(err(line, format!("unknown line `{word}`"))),
            }
        }
        let name = name.ok_or_else(|| err(0, "empty (no `law <name>` line)"))?;
        if !kind {
            return Err(err(0, "not a skeleton law (no `kind skeleton`)"));
        }
        if joints.is_empty() {
            return Err(err(0, "no `joint` or `chain` line"));
        }
        // the twins, after the declared joints, in their order
        if let Some((a, b)) = &mirror {
            let declared = joints.len();
            let mut twin_of: Vec<Option<usize>> = vec![None; declared];
            for j in 0..declared {
                // a chain's joint is named by its chain: `finger_l` 2 has the twin `finger_r2`
                let (base, place_no) = match (&joints[j].chain, &joints[j].place) {
                    (Some(c), Place::Chain(_, k, _)) => (c.clone(), k.to_string()),
                    _ => (joints[j].name.clone(), String::new()),
                };
                let Some(stem) = base.strip_suffix(a.as_str()) else {
                    continue;
                };
                let tn = format!("{stem}{b}{place_no}");
                if let Some(k) = joints.iter().position(|x| x.name == tn) {
                    twin_of[j] = Some(k);
                    continue;
                }
                if RESERVED.contains(&tn.as_str())
                    || params.iter().any(|p| p.name == tn)
                    || lets.iter().any(|(l, _, _)| l == &tn)
                {
                    return Err(err(
                        0,
                        format!("the twin `{tn}` is declared as a parameter or let"),
                    ));
                }
                if joints.len() == MAX_JOINTS {
                    return Err(err(0, format!("more than {MAX_JOINTS} joints")));
                }
                let parent = joints[j]
                    .parent
                    .map(|p| twin_of.get(p).copied().flatten().unwrap_or(p));
                let place = match &joints[j].place {
                    Place::Root(c) => Place::Root(c.clone()),
                    Place::From(_, c) => Place::From(parent.unwrap_or(0), c.clone()),
                    Place::Chain(_, k, c) => Place::Chain(parent.unwrap_or(0), *k, c.clone()),
                };
                twin_of[j] = Some(joints.len());
                let chain = joints[j].chain.as_ref().map(|_| format!("{stem}{b}"));
                joints.push(Joint {
                    name: tn,
                    chain,
                    parent,
                    place,
                    mirrored: true,
                });
            }
        }
        let law = Self {
            name,
            params,
            lets,
            decls,
            mirror,
            joints,
        };
        // the defaults must place every joint at finite numbers, so a law that reads always has
        // one instance
        law.bind(&[])
            .map_err(|e| err(0, format!("with the default parameters: {e}")))?;
        Ok(law)
    }

    /// The law's name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The declared parameters, in file order
    #[must_use]
    pub fn params(&self) -> &[SkeletonParam] {
        &self.params
    }

    /// Every joint's name and its parent's index, in order: the declared joints (a chain's in
    /// its order) and then the mirrored twins; a parent comes before its children
    pub fn joints(&self) -> impl Iterator<Item = (&str, Option<usize>)> {
        self.joints.iter().map(|j| (j.name.as_str(), j.parent))
    }

    /// The identifier of the law: its kind, [`SKELETON_SEMANTICS_ID`], its name, every parameter
    /// (name, default, range), every `let`, every joint and chain line (name, parent, count and
    /// the expressions as trees) and the mirror suffixes (spelling and spacing do not count;
    /// notes and comments are not in it)
    #[must_use]
    pub fn law_id(&self) -> [u8; 32] {
        self.law_id_under(&SKELETON_SEMANTICS_ID)
    }

    fn law_id_under(&self, semantics: &[u8; 32]) -> [u8; 32] {
        let mut h = LawIdHasher::new(SKELETON_LAW_KIND, semantics)
            .str(&self.name)
            .u64(self.params.len() as u64);
        for p in &self.params {
            h = h.str(&p.name).f64(p.default).f64(p.lo).f64(p.hi);
        }
        h = h.u64(self.lets.len() as u64);
        for (n, e, _) in &self.lets {
            h = e.hash_into(h.str(n));
        }
        h = h.u64(self.decls.len() as u64);
        for d in &self.decls {
            let exprs = match d {
                Decl::Joint {
                    name,
                    parent,
                    exprs,
                } => {
                    h = h.u64(0).str(name);
                    h = match parent {
                        Some(p) => h.u64(1).str(p),
                        None => h.u64(0),
                    };
                    exprs
                }
                Decl::Chain {
                    name,
                    parent,
                    count,
                    exprs,
                } => {
                    h = h.u64(1).str(name).str(parent).u64(u64::from(*count));
                    exprs
                }
            };
            for e in exprs {
                h = e.hash_into(h);
            }
        }
        match &self.mirror {
            Some((a, b)) => h.u64(1).str(a).str(b).finish(),
            None => h.u64(0).finish(),
        }
    }

    /// The law with parameter values and its joints' places: each named value must be declared,
    /// given once and in its range; the others take their defaults
    ///
    /// # Errors
    ///
    /// [`SampleError::UnknownParam`], [`SampleError::RepeatedParam`],
    /// [`SampleError::ParamOutOfRange`], or [`SampleError::NonFinite`] naming the `let` or joint
    /// whose value is not finite
    pub fn bind(&self, values: &[(&str, f64)]) -> Result<BoundSkeleton<'_>, SampleError> {
        let mut vals: Vec<f64> = self.params.iter().map(|p| p.default).collect();
        let mut seen = vec![false; self.params.len()];
        for (n, v) in values {
            let Some(j) = self.params.iter().position(|p| p.name == *n) else {
                return Err(SampleError::UnknownParam((*n).to_owned()));
            };
            if seen[j] {
                return Err(SampleError::RepeatedParam((*n).to_owned()));
            }
            seen[j] = true;
            let p = &self.params[j];
            if !(v.is_finite() && p.lo <= *v && *v <= p.hi) {
                return Err(SampleError::ParamOutOfRange {
                    name: p.name.clone(),
                });
            }
            vals[j] = *v;
        }
        let mut slots = vec![std::f64::consts::PI];
        slots.extend_from_slice(&vals);
        // the evaluator refuses a value that is not finite (it never returns one)
        let finite = |name: &str, v: Result<f64, _>| -> Result<f64, SampleError> {
            v.map_err(|_| SampleError::NonFinite {
                name: name.to_owned(),
            })
        };
        for (n, _, c) in &self.lets {
            let v = finite(n, c.eval(&slots))?;
            slots.push(v);
        }
        let mut places: Vec<[f64; 3]> = Vec::with_capacity(self.joints.len());
        for j in &self.joints {
            let (base, codes, i) = match &j.place {
                Place::Root(c) => ([0.0; 3], c, None),
                Place::From(p, c) => (places[*p], c, None),
                Place::Chain(p, k, c) => (places[*p], c, Some(f64::from(*k))),
            };
            let mut off = [0.0; 3];
            for (x, c) in off.iter_mut().zip(codes) {
                *x = match i {
                    Some(i) => {
                        slots.push(i);
                        let v = c.eval(&slots);
                        slots.pop();
                        finite(&j.name, v)?
                    }
                    None => finite(&j.name, c.eval(&slots))?,
                };
            }
            if j.mirrored {
                off[0] = -off[0];
            }
            places.push([base[0] + off[0], base[1] + off[1], base[2] + off[2]]);
        }
        Ok(BoundSkeleton {
            law: self,
            values: vals,
            places,
        })
    }
}

impl BoundSkeleton<'_> {
    /// The value of every parameter, in the law's declared order
    #[must_use]
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// Every joint's name, its parent's index and its place, in the order of
    /// [`SkeletonLaw::joints`]
    pub fn joints(&self) -> impl Iterator<Item = (&str, Option<usize>, [f64; 3])> {
        self.law
            .joints
            .iter()
            .zip(&self.places)
            .map(|(j, p)| (j.name.as_str(), j.parent, *p))
    }

    /// The place of the joint of this name
    #[must_use]
    pub fn place(&self, name: &str) -> Option<[f64; 3]> {
        self.law
            .joints
            .iter()
            .position(|j| j.name == name)
            .map(|i| self.places[i])
    }
}

/// The law the first skeleton pin evaluates: parameters, a let, a root, joints from parents, a
/// chain reading `i` and the expression functions, and mirrored twins
pub const SKELETON_PROBE_LAW: &str = "law skeleton_probe
kind skeleton
param a number = 0.3 range 0.1 0.9
param b number = 2 range 1 4
let c = a * b - 0.125
joint root = (0, c, a / 3)
joint hip_l from root = (a, 0 - c, 0.01)
chain toe_l from hip_l count 3 = (sin(i * a) / b, 0 - sqrt(i) * 0.1, exp(-i) * c)
joint ear_r from root = (0.0 - a, b / 10, atan2(a, b))
chain lock from root count 2 = (max(i - 1.5, 0.125), min(c, 0.5) ^ 2, 0 - pi / 9)
mirror _l _r
";

/// The measured value of every pin of [`SKELETON_SEMANTICS_PINS`]
///
/// # Panics
///
/// If [`SKELETON_PROBE_LAW`] does not read (the oracle reads it first)
#[must_use]
pub fn skeleton_semantics_fingerprints() -> Vec<(&'static str, [u8; 32])> {
    use sha2::{Digest as _, Sha256};
    let law = SkeletonLaw::from_file(SKELETON_PROBE_LAW).expect("the probe law reads");
    let mut h = Sha256::new();
    for a in [0.1, 0.3, 0.5, 0.9] {
        for b in [1.0, 2.0, 3.5] {
            let bound = law.bind(&[("a", a), ("b", b)]).expect("the probe binds");
            for (n, parent, p) in bound.joints() {
                h.update(n.as_bytes());
                h.update(parent.map_or(u64::MAX, |x| x as u64).to_le_bytes());
                for v in p {
                    h.update(v.to_bits().to_le_bytes());
                }
            }
        }
    }
    vec![
        (SKELETON_SEMANTICS_PINS[0].0, h.finalize().into()),
        (
            SKELETON_SEMANTICS_PINS[1].0,
            crate::research_law::expression_functions_fingerprint(),
        ),
    ]
}

/// The fold of the measured pins (what [`SKELETON_SEMANTICS_ID`] must equal)
#[must_use]
pub fn measured_skeleton_semantics_id() -> [u8; 32] {
    fold_semantics_pins(&skeleton_semantics_fingerprints())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The identifier moves with the skeleton semantics
    #[test]
    fn the_identifier_moves_with_the_skeleton_semantics() {
        let law = SkeletonLaw::from_file(SKELETON_PROBE_LAW).unwrap();
        assert_eq!(law.law_id(), law.law_id_under(&SKELETON_SEMANTICS_ID));
        let mut other = SKELETON_SEMANTICS_ID;
        other[0] ^= 1;
        assert_ne!(law.law_id_under(&other), law.law_id());
    }
}
