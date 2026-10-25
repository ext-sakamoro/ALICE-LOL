//! Statute laws (`kind statute`, conformance/TASK.md "Statute laws"): a title and clauses, each of a
//! legal nature, optionally under an earlier clause, in force from a time and optionally until one.
//!
//! The answer to `{"at": <time>}` is the labels of the clauses in force at that time, in file order.
//! The clauses are the shape ALICE-Legal's `StatuteTree` holds (`ClauseKind`, a parent, an
//! effective time and an expiry), so a statute law can be compiled to one: every time is at most
//! [`MAX_LEGAL_TIME`], whose nanoseconds fit 64 bits.

use std::collections::BTreeMap;

use alice_zip::law_id::LawIdHasher;

use super::{
    err, fields, law_id_start, read_inputs, time_literal, words, Answer, Edition, Json,
    RuleLawError, Ty, Value, PEOPLE, STATUTE_SEMANTICS_ID, STATUTE_SEMANTICS_PINS,
};

/// The kind tag of a statute law
pub const STATUTE_LAW_KIND: &[u8] = b"lol.law.statute/v1";

/// The latest time a statute or a contract names (seconds): its nanoseconds are at most `u64::MAX`
pub const MAX_LEGAL_TIME: u64 = u64::MAX / 1_000_000_000;

/// The legal nature of a clause (the words of a `clause` line, ALICE-Legal's `ClauseKind`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClauseKind {
    /// `obligation`: a duty imposed on a subject
    Obligation,
    /// `prohibition`: an act that is forbidden
    Prohibition,
    /// `permission`: an act that is allowed
    Permission,
    /// `definition`: a term defined for use elsewhere
    Definition,
    /// `penalty`: a sanction for non-compliance
    Penalty,
    /// `exception`: a carve-out from the clause it is `of`
    Exception,
}

impl ClauseKind {
    const ALL: [(&'static str, Self); 6] = [
        ("obligation", Self::Obligation),
        ("prohibition", Self::Prohibition),
        ("permission", Self::Permission),
        ("definition", Self::Definition),
        ("penalty", Self::Penalty),
        ("exception", Self::Exception),
    ];

    /// The word of this kind in a `clause` line
    #[must_use]
    pub fn word(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, k)| *k == self)
            .map_or("", |(w, _)| w)
    }

    fn of_word(w: &str) -> Option<Self> {
        Self::ALL.iter().find(|(x, _)| *x == w).map(|(_, k)| *k)
    }
}

/// One clause of a statute law
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatuteClause {
    label: String,
    kind: ClauseKind,
    parent: Option<usize>,
    from: u64,
    until: Option<u64>,
    text: String,
}

impl StatuteClause {
    /// The clause's label (unique in the law)
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Its legal nature
    #[must_use]
    pub const fn kind(&self) -> ClauseKind {
        self.kind
    }

    /// The index (in [`StatuteLaw::clauses`]) of the clause it is under, always an earlier one
    #[must_use]
    pub const fn parent(&self) -> Option<usize> {
        self.parent
    }

    /// In force from this time (seconds, inclusive)
    #[must_use]
    pub const fn from(&self) -> u64 {
        self.from
    }

    /// In force until this time (exclusive); `None` when it does not expire
    #[must_use]
    pub const fn until(&self) -> Option<u64> {
        self.until
    }

    /// Its text, as written after the `:`
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// `at` may be before 1970 (a time is read like an integer): no clause is in force then
    fn in_force(&self, at: i64) -> bool {
        let at = i128::from(at);
        at >= i128::from(self.from) && self.until.is_none_or(|u| at < i128::from(u))
    }
}

/// A statute law (`kind statute`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatuteLaw {
    pub(super) edition: Option<Edition>,
    title: String,
    clauses: Vec<StatuteClause>,
}

impl StatuteLaw {
    /// The statute's title (`title <text>`)
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Its clauses in file order
    #[must_use]
    pub fn clauses(&self) -> &[StatuteClause] {
        &self.clauses
    }

    pub(super) fn respond(&self, inp: &Json) -> Answer {
        let env = match read_inputs(&[("at".to_owned(), Ty::Time)], inp, None) {
            Ok(env) => env,
            Err(a) => return a,
        };
        let at = match env.first() {
            Some((_, Value::Int(t))) => *t,
            _ => return Answer::Rejected("at does not have its type".to_owned()),
        };
        let labels = self
            .clauses
            .iter()
            .filter(|c| c.in_force(at))
            .map(|c| Json::Text(c.label.clone()))
            .collect();
        Answer::Outputs(Json::Object(vec![(
            "in_force".to_owned(),
            Json::Array(labels),
        )]))
    }

    pub(super) fn law_id(&self) -> [u8; 32] {
        let mut h: LawIdHasher = law_id_start(
            STATUTE_LAW_KIND,
            "statute",
            &STATUTE_SEMANTICS_ID,
            self.edition.as_ref(),
        )
        .str("title")
        .str(&self.title)
        .str("clauses")
        .u64(self.clauses.len() as u64);
        for c in &self.clauses {
            h = h
                .str(&c.label)
                .str(c.kind.word())
                .u32(u32::from(c.parent.is_some()));
            if let Some(p) = c.parent {
                h = h.str(&self.clauses[p].label);
            }
            h = h.u64(c.from).u32(u32::from(c.until.is_some()));
            if let Some(u) = c.until {
                h = h.u64(u);
            }
            h = h.str(&c.text);
        }
        h.finish()
    }
}

/// A clause label: a lower-case ASCII letter, then lower-case letters, digits and `_` (no word is
/// reserved: a label is only ever read after `clause` or `of`)
fn is_label(s: &str) -> bool {
    let b = s.as_bytes();
    b.first().is_some_and(u8::is_ascii_lowercase)
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

pub(super) fn statute_time(n: usize, s: &str) -> Result<u64, RuleLawError> {
    let t = time_literal(n, s)?;
    if t > MAX_LEGAL_TIME {
        return Err(err(
            n,
            format!("`{s}` is after the latest time a statute names ({MAX_LEGAL_TIME})"),
        ));
    }
    Ok(t)
}

/// Reads the body of a statute law (the lines after its header); `from` is the start a clause
/// without `from` takes (the edition's, or 0)
pub(super) fn read_statute(
    ls: &[(usize, &str)],
    edition: Option<Edition>,
) -> Result<StatuteLaw, RuleLawError> {
    let default_from = edition.as_ref().map_or(0, Edition::from);
    let mut title: Option<String> = None;
    let mut clauses: Vec<StatuteClause> = Vec::new();
    // label -> index: each label is looked up once, so reading is linear in the clauses
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for &(n, l) in ls {
        let w = words(l);
        match w[0].0 {
            h if PEOPLE.contains(&h) || h == "kind" => {}
            "title" => {
                if title.is_some() {
                    return Err(err(n, "a second `title` line"));
                }
                let t = l[w[0].1..].trim_matches([' ', '\t']);
                if t.is_empty() {
                    return Err(err(n, "`title <text>`"));
                }
                title = Some(t.to_owned());
            }
            "clause" => {
                let c = read_clause(n, l, &index, default_from)?;
                index.insert(c.label.clone(), clauses.len());
                clauses.push(c);
            }
            h => return Err(err(n, format!("`{h}` is not a line of a statute law"))),
        }
    }
    let title = title.ok_or_else(|| err(0, "a statute law has one `title <text>` line"))?;
    if clauses.is_empty() {
        return Err(err(0, "a statute law has at least one `clause` line"));
    }
    Ok(StatuteLaw {
        edition,
        title,
        clauses,
    })
}

fn read_clause(
    n: usize,
    l: &str,
    earlier: &BTreeMap<String, usize>,
    default_from: u64,
) -> Result<StatuteClause, RuleLawError> {
    const SHAPE: &str =
        "`clause <label> <kind> [of <clause>] [from <time>] [until <time>]: <text>`";
    let (head, text) = l.split_once(':').ok_or_else(|| err(n, SHAPE))?;
    let text = text.trim_matches([' ', '\t']);
    if text.is_empty() {
        return Err(err(n, "a clause has a text after the `:`"));
    }
    let w = fields(head);
    let (label, kind) = match w.as_slice() {
        [_, label, kind, ..] => (*label, *kind),
        _ => return Err(err(n, SHAPE)),
    };
    if !is_label(label) {
        return Err(err(n, format!("`{label}` is not a clause label")));
    }
    if earlier.contains_key(label) {
        return Err(err(n, format!("a second clause `{label}`")));
    }
    let kind = ClauseKind::of_word(kind)
        .ok_or_else(|| err(n, format!("`{kind}` is not a clause kind")))?;
    let mut rest = &w[3..];
    let mut parent = None;
    if let ["of", p, tail @ ..] = rest {
        parent = Some(
            earlier
                .get(*p)
                .copied()
                .ok_or_else(|| err(n, format!("`of {p}`: no earlier clause `{p}`")))?,
        );
        rest = tail;
    }
    let mut from = default_from;
    if let ["from", t, tail @ ..] = rest {
        from = statute_time(n, t)?;
        rest = tail;
    }
    let mut until = None;
    if let ["until", t, tail @ ..] = rest {
        let u = statute_time(n, t)?;
        if u <= from {
            return Err(err(n, "`until` is not after the clause's start"));
        }
        until = Some(u);
        rest = tail;
    }
    if !rest.is_empty() {
        return Err(err(n, SHAPE));
    }
    if kind == ClauseKind::Exception && parent.is_none() {
        return Err(err(n, "an exception is `of` the clause it carves out of"));
    }
    Ok(StatuteClause {
        label: label.to_owned(),
        kind,
        parent,
        from,
        until,
        text: text.to_owned(),
    })
}

/// The measured value of every pin of [`STATUTE_SEMANTICS_PINS`]: the answers of a fixed statute
/// law to fixed requests (the two ends of a period, a clause without an end, a missing and a
/// mistyped `at`)
#[must_use]
pub fn statute_semantics_fingerprints() -> Vec<(&'static str, [u8; 32])> {
    vec![(STATUTE_SEMANTICS_PINS[0].0, statute_fingerprint())]
}

fn statute_fingerprint() -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let law = read_statute(
        &super::lines(
            "kind statute\ntitle fixture\nclause a obligation from 10 until 20: pay\n\
             clause b exception of a from 15: unless waived\nclause c definition: a term\n",
        )
        .expect("the fingerprint statute reads"),
        None,
    )
    .expect("the fingerprint statute reads");
    let mut h = Sha256::new();
    for r in [
        r#"{"at": -15}"#,
        r#"{"at": 0}"#,
        r#"{"at": 9}"#,
        r#"{"at": 10}"#,
        r#"{"at": 15}"#,
        r#"{"at": 19}"#,
        r#"{"at": 20}"#,
        r#"{"at": 18446744073}"#,
        r#"{"at": 1.5}"#,
        r#"{"at": "10"}"#,
        "{}",
    ] {
        let a = law.respond(&crate::law_input::parse_json(r).expect("fingerprint JSON"));
        h.update(format!("{r} -> {a:?}\n").as_bytes());
    }
    h.finalize().into()
}
