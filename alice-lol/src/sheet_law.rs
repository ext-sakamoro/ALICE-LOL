//! Sheet laws (`kind sheet`): the information a character sheet set must carry, declared as
//! slots, and the check of a filled sheet against them
//!
//! A sheet law lists pages; each page has slots (one value) and groups (repeated items made of
//! fields). A slot or field has a type, a unit, a range (fixed, or taken from a proportion law
//! the sheet `uses`), a vocabulary, whether it is required, a condition under which it is
//! required (`when`), where its value comes from (`source`) and, when no part law owns it yet
//! as a knob, `pending <part>`. Rules (`unique`) relate values across a group;
//! plausibility rules (`along`, `when`, `nesting`, `used`) relate a value to its neighbours.
//!
//! ```text
//! law avatar_sheet
//! kind sheet
//! uses avatar_body body.law
//! page turnaround: the body from the front, the side and the back
//! slot turnaround.stature number unit m range 0.5 2.5 required source authored : height
//! slot base_body.knee number unit eye_height range law avatar_body.knee required source law:avatar_body.knee : knee
//! group clothing.layers min 0 max 8 : one layer
//! field clothing.layers.hem number unit fraction range 0 1 required source authored : hem
//! rule unique clothing.layers.name : layer names differ
//! ```
//!
//! A filled sheet is JSON (`sheet_fill 1`); [`SheetLaw::check`] lists every missing, mistyped,
//! out-of-range, unreferenced, unprovenanced or rule-breaking value with its place and reason,
//! and a sheet is complete only when none of those is left. A fill from which no value was
//! read at all is a failure, not an empty success. [`SheetLaw::spec`] and
//! [`SheetLaw::template`] are generated from the law, so what an author is told to fill and
//! what the check reads are the same text.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};

use crate::json_lite::{get, parse as parse_json, J};
use crate::proportion_law::ProportionLaw;

/// The law kind's tag in [`SheetLaw::law_id`]
pub const SHEET_LAW_KIND: &[u8] = b"lol.law.sheet/v1";
/// The fill format [`SheetLaw::check`] reads (`"sheet_fill": 1`)
pub const SHEET_FILL_FORMAT: u32 = 1;
/// The longest reason an `out_of_range_intent` note may give
const INTENT_WHY_MAX: usize = 120;
/// The version of [`SheetCheck::to_json`]'s shape
pub const SHEET_CHECK_JSON_VERSION: u32 = 2;

/// A slot's type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlotType {
    Number,
    Int,
    Bool,
    /// `#rrggbb` in sRGB
    Colour,
    /// Text of at most this many characters
    Text(usize),
    /// One word of the vocabulary
    Enum(Vec<String>),
    /// The value of another group's field (`page.group.field`) in some item of that group
    Ref(String),
    /// A file: format, pixel size, colour space and provenance
    Asset,
}

impl SlotType {
    fn word(&self) -> String {
        match self {
            Self::Number => "number".into(),
            Self::Int => "integer".into(),
            Self::Bool => "true / false".into(),
            Self::Colour => "colour #rrggbb (sRGB)".into(),
            Self::Text(n) => format!("text (≤ {n} characters)"),
            Self::Enum(v) => format!("one of {}", v.join(" / ")),
            Self::Ref(t) => format!("a name from {t}"),
            Self::Asset => "file (asset object)".into(),
        }
    }
}

/// The condition under which a slot is required: another slot or field of the same page
/// (or item) equals a word, or is a number above a bound
#[derive(Debug, Clone, PartialEq)]
pub enum When {
    Eq(String, String),
    Above(String, f64),
}

/// A slot (`group` = None) or a group's field
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub page: String,
    pub group: Option<String>,
    pub name: String,
    pub ty: SlotType,
    pub unit: Option<String>,
    /// The range, once resolved
    pub range: Option<(f64, f64)>,
    /// `range law <alias>.<param>`: where the range comes from
    pub range_law: Option<(String, String)>,
    /// For a `range law` through a family alias: the range of each family (value of the family
    /// slot, range), once resolved; the check takes the one the fill names
    pub family_ranges: Vec<(String, (f64, f64))>,
    pub required: bool,
    pub when: Option<When>,
    pub source: String,
    pub pending: Option<String>,
    pub description: String,
}

impl Slot {
    /// `page.name` or `page.group.name`
    #[must_use]
    pub fn path(&self) -> String {
        self.group.as_ref().map_or_else(
            || format!("{}.{}", self.page, self.name),
            |g| format!("{}.{g}.{}", self.page, self.name),
        )
    }
}

/// A repeated group of fields
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub page: String,
    pub name: String,
    pub min: usize,
    pub max: usize,
    pub description: String,
}

/// A relation across a group's items
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// The field's values differ between the items (`page.group.field`)
    Unique(String, String, String, String),
}

/// A `family` line: when the fill's family slot has `value`, each alias names that used law
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    /// the enum slot that names the family (`page.slot`)
    pub slot: String,
    pub value: String,
    /// (alias, used law)
    pub laws: Vec<(String, String)>,
}

/// A page of the set
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub name: String,
    pub purpose: String,
}

/// A sheet law
#[derive(Debug, Clone, PartialEq)]
pub struct SheetLaw {
    pub name: String,
    pub claims: Vec<String>,
    pub uses: Vec<(String, String)>,
    /// `family <page.slot> <value>: <alias>=<law> …`: which used law each alias names when
    /// the fill's family slot has that value
    pub families: Vec<Family>,
    pub pages: Vec<Page>,
    pub slots: Vec<Slot>,
    pub groups: Vec<Group>,
    pub rules: Vec<Rule>,
    /// What may be written on a page: (allow, name, description)
    pub texts: Vec<(bool, String, String)>,
    /// Named landmark chains, top to bottom (`chain body head_top chin …`)
    pub chains: Vec<(String, Vec<String>)>,
    /// Plausibility rules over a group's items
    pub item_rules: Vec<ItemRule>,
    /// Each plausibility rule as written, without its reason (for the specification)
    pub item_rule_texts: Vec<String>,
    canonical: String,
}

/// Why a sheet law was refused
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetLawError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for SheetLawError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for SheetLawError {}

fn lerr(line: usize, message: impl Into<String>) -> SheetLawError {
    SheetLawError {
        line,
        message: message.into(),
    }
}

fn ident(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// The words of a slot / field line (before ` : `) after its path
#[allow(clippy::too_many_lines)] // one arm per option word
#[allow(clippy::many_single_char_names)] // n line, w words, s slot, v vocab, m max, as in the grammar
fn options(
    n: usize,
    words: &[&str],
    page: &str,
    group: Option<&str>,
    name: &str,
    description: String,
) -> Result<Slot, SheetLawError> {
    let mut it = words.iter().copied().peekable();
    let ty_word = it.next().ok_or_else(|| lerr(n, "a type is required"))?;
    let mut ty = match ty_word {
        "number" => SlotType::Number,
        "int" => SlotType::Int,
        "bool" => SlotType::Bool,
        "colour" => SlotType::Colour,
        "text" => SlotType::Text(0),
        "enum" => SlotType::Enum(Vec::new()),
        "asset" => SlotType::Asset,
        "ref" => {
            let t = it
                .next()
                .ok_or_else(|| lerr(n, "ref needs page.group.field"))?;
            if t.split('.').count() != 3 || !t.split('.').all(ident) {
                return Err(lerr(n, format!("ref `{t}`: page.group.field")));
            }
            SlotType::Ref(t.to_owned())
        }
        w => return Err(lerr(n, format!("unknown type `{w}`"))),
    };
    let mut s = Slot {
        page: page.to_owned(),
        group: group.map(str::to_owned),
        name: name.to_owned(),
        ty: SlotType::Number,
        unit: None,
        range: None,
        range_law: None,
        family_ranges: Vec::new(),
        required: false,
        when: None,
        source: String::new(),
        pending: None,
        description,
    };
    let mut required_seen = false;
    let num = |w: Option<&str>, what: &str| -> Result<f64, SheetLawError> {
        let w = w.ok_or_else(|| lerr(n, format!("{what}: a number")))?;
        w.parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| lerr(n, format!("{what}: `{w}` is not a finite number")))
    };
    while let Some(w) = it.next() {
        match w {
            "unit" => s.unit = Some(it.next().ok_or_else(|| lerr(n, "unit: a word"))?.to_owned()),
            "range" if it.peek() == Some(&"law") => {
                it.next();
                let t = it.next().ok_or_else(|| lerr(n, "range law: alias.param"))?;
                let (a, p) = t
                    .split_once('.')
                    .ok_or_else(|| lerr(n, "range law: alias.param"))?;
                s.range_law = Some((a.to_owned(), p.to_owned()));
            }
            "range" => {
                let lo = num(it.next(), "range")?;
                let hi = num(it.next(), "range")?;
                if lo >= hi {
                    return Err(lerr(n, "range: the low end must be below the high end"));
                }
                s.range = Some((lo, hi));
            }
            "vocab" => {
                let v: Vec<String> = it
                    .next()
                    .ok_or_else(|| lerr(n, "vocab: a|b|c"))?
                    .split('|')
                    .map(str::to_owned)
                    .collect();
                if v.iter().any(std::string::String::is_empty) {
                    return Err(lerr(n, "vocab: empty word"));
                }
                match &mut ty {
                    SlotType::Enum(e) => *e = v,
                    _ => return Err(lerr(n, "vocab belongs to an enum")),
                }
            }
            "max" => {
                let m = num(it.next(), "max")?;
                match &mut ty {
                    // a whole count of characters
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    SlotType::Text(x) if m >= 1.0 && m.fract() == 0.0 => *x = m as usize,
                    _ => return Err(lerr(n, "max belongs to a text, as a whole number ≥ 1")),
                }
            }
            "required" | "optional" if !required_seen => {
                s.required = w == "required";
                required_seen = true;
            }
            "when" => {
                let c = it
                    .next()
                    .ok_or_else(|| lerr(n, "when: field=word or field>number"))?;
                s.when = Some(if let Some((f, v)) = c.split_once('=') {
                    When::Eq(f.to_owned(), v.to_owned())
                } else if let Some((f, v)) = c.split_once('>') {
                    When::Above(f.to_owned(), num(Some(v), "when")?)
                } else {
                    return Err(lerr(n, "when: field=word or field>number"));
                });
            }
            "source" => {
                it.next()
                    .ok_or_else(|| lerr(n, "source: a word"))?
                    .clone_into(&mut s.source);
            }
            "pending" => {
                s.pending = Some(
                    it.next()
                        .ok_or_else(|| lerr(n, "pending: a part"))?
                        .to_owned(),
                );
            }
            w => return Err(lerr(n, format!("unknown word `{w}`"))),
        }
    }
    if !required_seen {
        return Err(lerr(n, "required or optional must be stated"));
    }
    if s.source.is_empty() {
        return Err(lerr(n, "source must be stated"));
    }
    match &ty {
        SlotType::Enum(v) if v.is_empty() => return Err(lerr(n, "an enum needs a vocab")),
        SlotType::Text(0) => return Err(lerr(n, "a text needs max")),
        _ => {}
    }
    if (s.range.is_some() || s.range_law.is_some())
        && !matches!(ty, SlotType::Number | SlotType::Int)
    {
        return Err(lerr(n, "range belongs to a number or an integer"));
    }
    s.ty = ty;
    Ok(s)
}

impl SheetLaw {
    /// Read a sheet law (ranges taken from proportion laws are resolved by [`SheetLaw::resolve`])
    ///
    /// # Errors
    ///
    /// [`SheetLawError`] for a missing `law` / `kind sheet` header, an unknown line or word, a
    /// slot on an unknown page or group, a slot given twice, a rule on unknown fields.
    #[allow(clippy::too_many_lines)] // one arm per kind of line, in file order
    #[allow(clippy::many_single_char_names)] // n line, l line text, w words, s slot, a / p / t parts
    pub fn from_file(text: &str) -> Result<Self, SheetLawError> {
        let lines: Vec<(usize, &str)> = text
            .lines()
            .enumerate()
            .map(|(i, l)| (i + 1, l.trim()))
            .filter(|(_, l)| !l.is_empty() && !l.starts_with('#'))
            .collect();
        let canonical = lines
            .iter()
            .map(|(_, l)| l.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect::<Vec<_>>()
            .join("\n");
        let mut it = lines.into_iter();
        let name = match it.next() {
            Some((_, l)) if l.starts_with("law ") && ident(l[4..].trim()) => {
                l[4..].trim().to_owned()
            }
            Some((n, _)) => return Err(lerr(n, "the first line must be `law <name>`")),
            None => return Err(lerr(0, "empty law")),
        };
        match it.next() {
            Some((_, "kind sheet")) => {}
            Some((n, _)) => return Err(lerr(n, "the second line must be `kind sheet`")),
            None => return Err(lerr(0, "the second line must be `kind sheet`")),
        }
        let mut law = Self {
            name,
            claims: Vec::new(),
            uses: Vec::new(),
            families: Vec::new(),
            pages: Vec::new(),
            slots: Vec::new(),
            groups: Vec::new(),
            rules: Vec::new(),
            texts: Vec::new(),
            chains: Vec::new(),
            item_rules: Vec::new(),
            item_rule_texts: Vec::new(),
            canonical,
        };
        for (n, l) in it {
            let (head, desc) = l.split_once(" : ").map_or((l, ""), |(h, d)| (h, d));
            let w: Vec<&str> = head.split_whitespace().collect();
            match w[0] {
                "claim" => law.claims.push(l[6..].trim().to_owned()),
                "text"
                    if w.len() == 3
                        && (w[1] == "allow" || w[1] == "forbid")
                        && ident(w[2])
                        && !desc.trim().is_empty() =>
                {
                    if law.texts.iter().any(|(_, n, _)| n == w[2]) {
                        return Err(lerr(n, format!("text `{}` given twice", w[2])));
                    }
                    law.texts
                        .push((w[1] == "allow", w[2].to_owned(), desc.trim().to_owned()));
                }
                "uses" if w.len() == 3 && ident(w[1]) => {
                    law.uses.push((w[1].to_owned(), w[2].to_owned()));
                }
                "family" => {
                    let (head, map) = l[7..]
                        .split_once(':')
                        .ok_or_else(|| lerr(n, "family <page.slot> <value>: <alias>=<law> …"))?;
                    let head: Vec<&str> = head.split_whitespace().collect();
                    let [slot, value] = head[..] else {
                        return Err(lerr(n, "family <page.slot> <value>: <alias>=<law> …"));
                    };
                    let mut laws = Vec::new();
                    for pair in map.split_whitespace() {
                        let Some((a, l)) =
                            pair.split_once('=').filter(|(a, l)| ident(a) && ident(l))
                        else {
                            return Err(lerr(n, format!("family: `{pair}` is not <alias>=<law>")));
                        };
                        laws.push((a.to_owned(), l.to_owned()));
                    }
                    if laws.is_empty() {
                        return Err(lerr(n, "family: at least one <alias>=<law>"));
                    }
                    if law
                        .families
                        .iter()
                        .any(|f| f.slot == slot && f.value == value)
                    {
                        return Err(lerr(n, format!("family {slot} {value} given twice")));
                    }
                    law.families.push(Family {
                        slot: slot.to_owned(),
                        value: value.to_owned(),
                        laws,
                    });
                }
                "page" => {
                    let (pname, purpose) = l[5..]
                        .split_once(':')
                        .ok_or_else(|| lerr(n, "page <name>: <purpose>"))?;
                    let pname = pname.trim();
                    if !ident(pname) || law.pages.iter().any(|p| p.name == pname) {
                        return Err(lerr(n, format!("page `{pname}`: a new lower-case name")));
                    }
                    law.pages.push(Page {
                        name: pname.to_owned(),
                        purpose: purpose.trim().to_owned(),
                    });
                }
                "slot" | "field" if w.len() >= 2 => {
                    let parts: Vec<&str> = w[1].split('.').collect();
                    let (page, group, sname) = match (w[0], parts.as_slice()) {
                        ("slot", [p, s]) => (*p, None, *s),
                        ("field", [p, g, s]) => (*p, Some(*g), *s),
                        _ => return Err(lerr(n, "slot page.name / field page.group.name")),
                    };
                    if !law.pages.iter().any(|p| p.name == page) {
                        return Err(lerr(n, format!("no page `{page}` above this line")));
                    }
                    if let Some(g) = group {
                        if !law.groups.iter().any(|x| x.page == page && x.name == g) {
                            return Err(lerr(n, format!("no group `{page}.{g}` above this line")));
                        }
                    }
                    if !ident(sname) {
                        return Err(lerr(n, format!("`{sname}`: a lower-case name")));
                    }
                    let s = options(n, &w[2..], page, group, sname, desc.trim().to_owned())?;
                    if law.slots.iter().any(|x| x.path() == s.path())
                        || (group.is_none()
                            && law.groups.iter().any(|g| g.page == page && g.name == sname))
                    {
                        return Err(lerr(n, format!("{} given twice", s.path())));
                    }
                    law.slots.push(s);
                }
                "group" if w.len() == 6 && w[2] == "min" && w[4] == "max" => {
                    let (page, gname) = w[1]
                        .split_once('.')
                        .ok_or_else(|| lerr(n, "group page.name"))?;
                    if !law.pages.iter().any(|p| p.name == page) || !ident(gname) {
                        return Err(lerr(
                            n,
                            format!("group `{}`: an existing page and a lower-case name", w[1]),
                        ));
                    }
                    let (min, max) = (
                        w[3].parse::<usize>().map_err(|_| lerr(n, "min: a count"))?,
                        w[5].parse::<usize>().map_err(|_| lerr(n, "max: a count"))?,
                    );
                    if min > max || max == 0 {
                        return Err(lerr(n, "min ≤ max, max ≥ 1"));
                    }
                    law.groups.push(Group {
                        page: page.to_owned(),
                        name: gname.to_owned(),
                        min,
                        max,
                        description: desc.trim().to_owned(),
                    });
                }
                "chain" if w.len() >= 4 && ident(w[1]) && w[2..].iter().all(|x| ident(x)) => {
                    if law.chains.iter().any(|(c, _)| c == w[1]) {
                        return Err(lerr(n, format!("chain `{}` given twice", w[1])));
                    }
                    law.chains.push((
                        w[1].to_owned(),
                        w[2..].iter().map(|x| (*x).to_owned()).collect(),
                    ));
                }
                "rule"
                    if w.len() >= 3
                        && matches!(w[1], "along" | "when" | "warn" | "nesting" | "used") =>
                {
                    let r = parse_item_rule(n, &w[1..], desc.trim())?;
                    for f in r.fields() {
                        if !law.slots.iter().any(|s| s.path() == f) {
                            return Err(lerr(
                                n,
                                format!("rule on `{f}`, which is not a field above this line"),
                            ));
                        }
                    }
                    law.item_rules.push(r);
                    law.item_rule_texts.push(w[1..].join(" "));
                }
                "rule" if w.len() >= 3 => {
                    let split = |t: &str| -> Result<(String, String, String), SheetLawError> {
                        let p: Vec<&str> = t.split('.').collect();
                        match p.as_slice() {
                            [a, b, c] => Ok(((*a).to_owned(), (*b).to_owned(), (*c).to_owned())),
                            _ => Err(lerr(n, format!("`{t}`: page.group.field"))),
                        }
                    };
                    let rule = match (w[1], w.len()) {
                        ("unique", 3) => {
                            let (p, g, f) = split(w[2])?;
                            Rule::Unique(p, g, f, desc.trim().to_owned())
                        }
                        _ => return Err(lerr(n, "rule unique page.group.field")),
                    };
                    let fields: Vec<String> = match &rule {
                        Rule::Unique(p, g, f, _) => vec![format!("{p}.{g}.{f}")],
                    };
                    for f in fields {
                        if !law.slots.iter().any(|s| s.path() == f) {
                            return Err(lerr(
                                n,
                                format!("rule on `{f}`, which is not a field above this line"),
                            ));
                        }
                    }
                    law.rules.push(rule);
                }
                _ => return Err(lerr(n, format!("unknown or malformed line `{l}`"))),
            }
        }
        if law.pages.is_empty() {
            return Err(lerr(0, "a sheet law has at least one page"));
        }
        for s in &law.slots {
            if let SlotType::Ref(t) = &s.ty {
                if !law.slots.iter().any(|x| x.path() == *t) {
                    return Err(lerr(
                        0,
                        format!("{} refers to `{t}`, which is not a field", s.path()),
                    ));
                }
            }
            if let Some((a, _)) = &s.range_law {
                if !law.uses.iter().any(|(u, _)| u == a) && !law.family_aliases().any(|x| x == a) {
                    return Err(lerr(
                        0,
                        format!(
                            "{}: range law `{a}` is neither in uses nor a family alias",
                            s.path()
                        ),
                    ));
                }
            }
        }
        law.check_families()?;
        law.check_rule_names()?;
        Ok(law)
    }

    fn family_aliases(&self) -> impl Iterator<Item = &str> {
        self.families
            .first()
            .into_iter()
            .flat_map(|f| f.laws.iter().map(|(a, _)| a.as_str()))
    }

    /// The families name one enum slot, cover its vocabulary once, map the same aliases, and
    /// name used laws; an alias is not also a used law's name
    fn check_families(&self) -> Result<(), SheetLawError> {
        let Some(first) = self.families.first() else {
            return Ok(());
        };
        let bad = |why: String| Err(lerr(0, format!("family: {why}")));
        let Some(slot) = self
            .slots
            .iter()
            .find(|s| s.path() == first.slot && s.group.is_none())
        else {
            return bad(format!("`{}` is not a slot of a page", first.slot));
        };
        let SlotType::Enum(vocab) = &slot.ty else {
            return bad(format!("`{}` is not an enum", first.slot));
        };
        let aliases: Vec<&str> = first.laws.iter().map(|(a, _)| a.as_str()).collect();
        for f in &self.families {
            if f.slot != first.slot {
                return bad(format!(
                    "every family line names the same slot (`{}`, `{}`)",
                    first.slot, f.slot
                ));
            }
            if !vocab.contains(&f.value) {
                return bad(format!(
                    "`{}` is not in the vocabulary of `{}`",
                    f.value, f.slot
                ));
            }
            if f.laws.iter().map(|(a, _)| a.as_str()).collect::<Vec<_>>() != aliases {
                return bad(format!(
                    "`{}` maps other aliases than `{}`",
                    f.value, first.value
                ));
            }
            for (a, l) in &f.laws {
                if !self.uses.iter().any(|(u, _)| u == l) {
                    return bad(format!("`{a}={l}`: `{l}` is not in uses"));
                }
                if self.uses.iter().any(|(u, _)| u == a) {
                    return bad(format!("`{a}` is both a family alias and a used law"));
                }
            }
        }
        if let Some(v) = vocab
            .iter()
            .find(|v| !self.families.iter().any(|f| f.value == **v))
        {
            return bad(format!("`{v}` of `{}` has no family line", first.slot));
        }
        Ok(())
    }

    /// The law as the check reads it for one family (`None`: the fill names none, and no
    /// family range is applied)
    fn for_family(&self, value: Option<&str>) -> Self {
        let mut l = self.clone();
        for s in &mut l.slots {
            if !s.family_ranges.is_empty() {
                s.range = value
                    .and_then(|v| s.family_ranges.iter().find(|(x, _)| x == v))
                    .map(|(_, r)| *r);
            }
        }
        l
    }

    /// Every name a plausibility rule reads exists, with the type the rule reads it as: a
    /// misspelt name would leave the rule silently never applying
    fn check_rule_names(&self) -> Result<(), SheetLawError> {
        let field = |path: &str| self.slots.iter().find(|s| s.path() == path);
        for (r, text) in self.item_rules.iter().zip(&self.item_rule_texts) {
            let bad = |why: String| Err(lerr(0, format!("rule `{text}`: {why}")));
            let ItemRule::When {
                group,
                cond,
                require,
                ..
            } = r
            else {
                continue;
            };
            for c in std::iter::once(cond).chain(require) {
                let slot = match &c.side {
                    Side::Field(f) => field(&format!("{group}.{f}")),
                    Side::Sum(_) => None,
                    Side::Deref(r, f) => {
                        let Some(SlotType::Ref(target)) =
                            field(&format!("{group}.{r}")).map(|s| &s.ty)
                        else {
                            return bad(format!("`{r}` is not a reference"));
                        };
                        let target_group = target.rsplit_once('.').map_or("", |(g, _)| g);
                        let Some(s) = field(&format!("{target_group}.{f}")) else {
                            return bad(format!("`{target_group}` has no field `{f}`"));
                        };
                        Some(s)
                    }
                };
                if let (Some(s), Test::In(words) | Test::NotIn(words)) = (slot, &c.test) {
                    if let SlotType::Enum(vocab) = &s.ty {
                        if let Some(w) = words.iter().find(|w| !vocab.contains(w)) {
                            return bad(format!(
                                "`{w}` is not in the vocabulary of `{}`",
                                s.path()
                            ));
                        }
                    }
                }
                if let Test::AtMost(Bound::Scaled(p, _)) | Test::AtLeast(Bound::Scaled(p, _)) =
                    &c.test
                {
                    if !matches!(field(p).map(|s| &s.ty), Some(SlotType::Number))
                        || field(p).is_some_and(|s| s.group.is_some())
                    {
                        return bad(format!("`{p}` is not a number slot of a page"));
                    }
                }
            }
        }
        Ok(())
    }

    /// Take the `range law` ranges from the proportion laws the sheet uses (alias, law)
    ///
    /// # Errors
    ///
    /// When an alias has no law, or the law has no such parameter.
    pub fn resolve(&mut self, laws: &[(&str, &ProportionLaw)]) -> Result<(), String> {
        let families = self.families.clone();
        for s in &mut self.slots {
            if let Some((a, p)) = s
                .range_law
                .clone()
                .filter(|(a, _)| families.iter().any(|f| f.laws.iter().any(|(x, _)| x == a)))
            {
                s.family_ranges.clear();
                for f in &families {
                    let l = f.laws.iter().find(|(x, _)| *x == a).map_or(&a, |(_, l)| l);
                    let law = laws
                        .iter()
                        .find(|(x, _)| x == l)
                        .ok_or_else(|| format!("{}: no law given for `{l}`", s.path()))?
                        .1;
                    if let Some(q) = law.params().iter().find(|q| q.name == p) {
                        s.family_ranges.push((f.value.clone(), (q.lo, q.hi)));
                    } else {
                        // a parameter only some families have: the slot is asked only for them
                        {
                            let field = f.slot.rsplit_once('.').map_or("", |(_, x)| x);
                            let only_others = s.page
                                == f.slot.split('.').next().unwrap_or_default()
                                && matches!(&s.when, Some(When::Eq(w, v)) if w == field && *v != f.value);
                            if !only_others {
                                return Err(format!("{}: {l} has no parameter `{p}` (ask it only when {} is not {})", s.path(), f.slot, f.value));
                            }
                        }
                    }
                }
                continue;
            }
            if let Some((a, p)) = &s.range_law {
                let law = laws
                    .iter()
                    .find(|(x, _)| x == a)
                    .ok_or_else(|| format!("{}: no law given for `{a}`", s.path()))?
                    .1;
                let param = law
                    .params()
                    .iter()
                    .find(|q| q.name == *p)
                    .ok_or_else(|| format!("{}: {a} has no parameter `{p}`", s.path()))?;
                s.range = Some((param.lo, param.hi));
            }
        }
        Ok(())
    }

    /// The law's identifier: SHA-256 of the kind tag and its lines (comments and spacing
    /// do not count)
    #[must_use]
    pub fn law_id(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(SHEET_LAW_KIND);
        h.update([0]);
        h.update(self.canonical.as_bytes());
        h.finalize().into()
    }

    fn page_slots<'a>(&'a self, page: &'a str) -> impl Iterator<Item = &'a Slot> + 'a {
        self.slots
            .iter()
            .filter(move |s| s.page == page && s.group.is_none())
    }

    fn group_fields<'a>(
        &'a self,
        page: &'a str,
        group: &'a str,
    ) -> impl Iterator<Item = &'a Slot> + 'a {
        self.slots
            .iter()
            .filter(move |s| s.page == page && s.group.as_deref() == Some(group))
    }
}

// ── the check ───────────────────────────────────────────────────────────────────────────────

/// What is wrong with a filled value
#[derive(Debug, Clone, PartialEq)]
pub enum Problem {
    Missing,
    WrongType(String),
    OutOfRange {
        value: f64,
        lo: f64,
        hi: f64,
    },
    NotInVocabulary(String),
    TooLong(usize),
    NoSuchName(String),
    AssetIncomplete(String),
    TooFew {
        have: usize,
        min: usize,
    },
    TooMany {
        have: usize,
        max: usize,
    },
    NotUnique(String),
    NotOrdered(String, String),
    /// A plausibility rule does not hold (its description)
    Implausible(String),
    /// A name no reference uses
    NotUsed(String),
    /// The design wants a value outside the slot's range (`out_of_range_intent`)
    DesignWants {
        slot: String,
        value: f64,
        lo: f64,
        hi: f64,
    },
    UnknownKey,
    NothingRead,
}

impl Problem {
    /// A stable code for the JSON
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::WrongType(_) => "wrong_type",
            Self::OutOfRange { .. } => "out_of_range",
            Self::NotInVocabulary(_) => "not_in_vocabulary",
            Self::TooLong(_) => "too_long",
            Self::NoSuchName(_) => "no_such_name",
            Self::AssetIncomplete(_) => "asset_incomplete",
            Self::TooFew { .. } => "too_few",
            Self::TooMany { .. } => "too_many",
            Self::NotUnique(_) => "not_unique",
            Self::NotOrdered(..) => "not_ordered",
            Self::Implausible(_) => "implausible",
            Self::NotUsed(_) => "not_used",
            Self::DesignWants { .. } => "design_wants_out_of_range",
            Self::UnknownKey => "unknown_key",
            Self::NothingRead => "nothing_read",
        }
    }

    /// The reason in Japanese
    #[must_use]
    pub fn japanese(&self, slot: Option<&Slot>) -> String {
        match self {
            Self::Missing => "必須なのに空".to_owned(),
            Self::WrongType(want) => format!("型が違う ({want} が必要)"),
            Self::OutOfRange { value, lo, hi } => {
                let unit = slot.and_then(|s| s.unit.clone()).unwrap_or_default();
                format!("範囲外: {value} (範囲 {lo} 〜 {hi} {unit})")
            }
            Self::NotInVocabulary(v) => format!(
                "`{v}` は語彙に無い ({})",
                slot.map(|s| s.ty.word()).unwrap_or_default()
            ),
            Self::TooLong(max) => format!("長すぎる ({max} 文字まで)"),
            Self::NoSuchName(t) => format!("参照先 {t} にその名前が無い"),
            Self::AssetIncomplete(what) => format!("ファイルの情報が足りない: {what}"),
            Self::TooFew { have, min } => format!("数が足りない ({have} 個、{min} 個以上)"),
            Self::TooMany { have, max } => format!("多すぎる ({have} 個、{max} 個まで)"),
            Self::NotUnique(v) => format!("`{v}` が重複している"),
            Self::NotOrdered(a, b) => format!("順序が逆: {a} が {b} より上にあるべき"),
            Self::Implausible(why) => format!("あり得ない組み合わせ: {why}"),
            Self::NotUsed(name) => format!("`{name}` はどこからも使われていない"),
            Self::DesignWants { slot, value, lo, hi } => format!("デザインは {slot} = {value} を求めている (範囲 {lo}〜{hi} の外、slot には範囲内の値)"),
            Self::UnknownKey => "この law に無い項目".to_owned(),
            Self::NothingRead => "値が 1 つも読めなかった (空の記入は完成ではない)".to_owned(),
        }
    }
}

/// One finding of the check
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub page: String,
    /// The place in the fill, e.g. `clothing.layers[2].hem`
    pub path: String,
    pub problem: Problem,
    /// The slot's index in the law, when the finding is about a declared slot
    slot: Option<usize>,
}

/// A filled sheet checked against its law
#[derive(Debug, Clone, PartialEq)]
pub struct SheetCheck {
    pub law: String,
    pub law_id: [u8; 32],
    pub findings: Vec<Finding>,
    /// Plausibility warnings (listed, not blocking)
    pub warnings: Vec<Finding>,
    /// Values read (present and not null)
    pub read: usize,
    /// Slots and fields that ask for a value, counted over the items present
    pub asked: usize,
    pub pages: Vec<(String, bool)>,
    /// Slots that exist here before their part law owns them (path, part)
    pub pending: Vec<(String, String)>,
    slot_info: Vec<Slot>,
}

impl SheetCheck {
    /// Every page complete, and something was read
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.findings.is_empty() && self.read > 0
    }
}

fn is_hex_colour(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

const ASSET_FORMATS: [&str; 6] = ["png", "jpg", "webp", "svg", "exr", "tga"];

#[allow(clippy::many_single_char_names)] // v value, o / p objects, s / k / x strings
fn asset_problem(v: &J) -> Option<String> {
    let J::Obj(o) = v else {
        return Some("an object {file, format, width, height, colour_space, provenance}".into());
    };
    let allowed = [
        "file",
        "format",
        "width",
        "height",
        "colour_space",
        "provenance",
    ];
    if let Some((k, _)) = o.iter().find(|(k, _)| !allowed.contains(&k.as_str())) {
        return Some(format!("unknown key `{k}`"));
    }
    let s = |k: &str| match get(o, k) {
        Some(J::Str(s)) if !s.trim().is_empty() => Some(s.clone()),
        _ => None,
    };
    if s("file").is_none() {
        return Some("file".into());
    }
    if !s("format").is_some_and(|f| ASSET_FORMATS.contains(&f.as_str())) {
        return Some(format!("format ({})", ASSET_FORMATS.join(" | ")));
    }
    for k in ["width", "height"] {
        if !matches!(get(o, k), Some(J::Num(x)) if *x >= 1.0 && x.fract() == 0.0) {
            return Some(format!("{k} (pixels)"));
        }
    }
    if !s("colour_space").is_some_and(|c| c == "srgb" || c == "linear") {
        return Some("colour_space (srgb | linear)".into());
    }
    let Some(J::Obj(p)) = get(o, "provenance") else {
        return Some("provenance {source, licence, author, terms_sha256}".into());
    };
    for k in ["source", "licence", "author"] {
        if !matches!(get(p, k), Some(J::Str(x)) if !x.trim().is_empty()) {
            return Some(format!("provenance.{k}"));
        }
    }
    if !matches!(get(p, "terms_sha256"), Some(J::Str(x)) if x.len() == 64 && x.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Some("provenance.terms_sha256 (64 hex digits of the licence terms)".into());
    }
    None
}

impl SheetLaw {
    fn value_problem(slot: &Slot, v: &J) -> Option<Problem> {
        match (&slot.ty, v) {
            (SlotType::Number | SlotType::Int, J::Num(x)) => {
                if matches!(slot.ty, SlotType::Int) && x.fract() != 0.0 {
                    return Some(Problem::WrongType(slot.ty.word()));
                }
                match slot.range {
                    Some((lo, hi)) if *x < lo || *x > hi => {
                        Some(Problem::OutOfRange { value: *x, lo, hi })
                    }
                    _ => None,
                }
            }
            (SlotType::Bool, J::Bool(_)) | (SlotType::Ref(_), J::Str(_)) => None, // a reference is checked once every name is known
            (SlotType::Colour, J::Str(s)) if is_hex_colour(s) => None,
            (SlotType::Text(max), J::Str(s)) => {
                (s.chars().count() > *max).then_some(Problem::TooLong(*max))
            }
            (SlotType::Enum(vocab), J::Str(s)) => {
                (!vocab.contains(s)).then(|| Problem::NotInVocabulary(s.clone()))
            }
            (SlotType::Asset, v) => asset_problem(v).map(Problem::AssetIncomplete),
            _ => Some(Problem::WrongType(slot.ty.word())),
        }
    }

    fn applies(when: Option<&When>, siblings: &[(String, J)]) -> bool {
        match when {
            None => true,
            Some(When::Eq(f, w)) => matches!(get(siblings, f), Some(J::Str(s)) if s == w),
            Some(When::Above(f, b)) => matches!(get(siblings, f), Some(J::Num(x)) if x > b),
        }
    }

    /// Check a filled sheet (JSON, `sheet_fill 1`)
    ///
    /// # Errors
    ///
    /// When the text is not JSON in the accepted subset, or not a `sheet_fill 1` for this law.
    pub fn check(&self, fill: &str) -> Result<SheetCheck, String> {
        let Some(family) = self.families.first() else {
            return self.check_for_family(fill);
        };
        // the ranges of the family the fill names (none when it names none: the family slot
        // is then listed as missing or wrong, and no family range is judged)
        let J::Obj(root) = parse_json(fill)? else {
            return Err("the fill must be a JSON object".into());
        };
        let (page, field) = family.slot.split_once('.').unwrap_or_default();
        let value = match get(&root, "pages") {
            Some(J::Obj(p)) => match get(p, page) {
                Some(J::Obj(o)) => match get(o, field) {
                    Some(J::Str(v)) => Some(v.clone()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
        let mut c = self.for_family(value.as_deref()).check_for_family(fill)?;
        // a value for a slot this family's laws have no parameter for is not used: listed
        if let Some(v) = &value {
            for slot in self
                .slots
                .iter()
                .filter(|s| s.group.is_none() && !s.family_ranges.is_empty())
            {
                let given = match get(&root, "pages") {
                    Some(J::Obj(p)) => {
                        matches!(get(p, &slot.page), Some(J::Obj(o)) if get(o, &slot.name).is_some_and(|x| !matches!(x, J::Null)))
                    }
                    _ => false,
                };
                if given && !slot.family_ranges.iter().any(|(f, _)| f == v) {
                    c.warnings.push(Finding {
                        page: slot.page.clone(),
                        path: slot.path(),
                        problem: Problem::NotUsed(format!(
                            "{} ({}: {v})",
                            slot.path(),
                            family.slot
                        )),
                        slot: None,
                    });
                }
            }
        }
        Ok(c)
    }

    #[allow(clippy::too_many_lines)] // pages, then groups, then references, then rules
    #[allow(clippy::many_single_char_names)] // f findings, p page, g group, o item, s slot, v value, r rule
    fn check_for_family(&self, fill: &str) -> Result<SheetCheck, String> {
        let J::Obj(root) = parse_json(fill)? else {
            return Err("the fill must be a JSON object".into());
        };
        if let Some((k, _)) = root.iter().find(|(k, _)| {
            !["sheet_fill", "law", "pages", "out_of_range_intent"].contains(&k.as_str())
        }) {
            return Err(format!(
                "unknown top-level key `{k}` (sheet_fill, law, pages, out_of_range_intent)"
            ));
        }
        if !matches!(get(&root, "sheet_fill"), Some(J::Num(v)) if v.to_bits() == f64::from(SHEET_FILL_FORMAT).to_bits())
        {
            return Err(format!("\"sheet_fill\": {SHEET_FILL_FORMAT} is required"));
        }
        if !matches!(get(&root, "law"), Some(J::Str(s)) if *s == self.name) {
            return Err(format!("\"law\": \"{}\" is required", self.name));
        }
        let empty = Vec::new();
        let pages = match get(&root, "pages") {
            Some(J::Obj(p)) => p,
            None => &empty,
            Some(_) => return Err("\"pages\" must be an object".into()),
        };
        let mut f: Vec<Finding> = Vec::new();
        let (mut read, mut asked) = (0usize, 0usize);
        let idx = |s: &Slot| self.slots.iter().position(|x| x.path() == s.path());
        for (k, _) in pages {
            if !self.pages.iter().any(|p| p.name == *k) {
                f.push(Finding {
                    page: k.clone(),
                    path: k.clone(),
                    problem: Problem::UnknownKey,
                    slot: None,
                });
            }
        }
        // every name a group defines, for references: (page.group.field) -> values
        let mut names: Vec<(String, Vec<String>)> = Vec::new();
        for page in &self.pages {
            let p = &page.name;
            let obj: &[(String, J)] = match get(pages, p) {
                Some(J::Obj(o)) => o,
                _ => &[],
            };
            for (k, _) in obj {
                let known = self.page_slots(p).any(|s| s.name == *k)
                    || self.groups.iter().any(|g| g.page == *p && g.name == *k);
                if !known {
                    f.push(Finding {
                        page: p.clone(),
                        path: format!("{p}.{k}"),
                        problem: Problem::UnknownKey,
                        slot: None,
                    });
                }
            }
            for s in self.page_slots(p) {
                let applies = Self::applies(s.when.as_ref(), obj);
                match get(obj, &s.name) {
                    None | Some(J::Null) => {
                        if s.required && applies {
                            asked += 1;
                            f.push(Finding {
                                page: p.clone(),
                                path: s.path(),
                                problem: Problem::Missing,
                                slot: idx(s),
                            });
                        }
                    }
                    Some(v) => {
                        read += 1;
                        asked += 1;
                        if let Some(pr) = Self::value_problem(s, v) {
                            f.push(Finding {
                                page: p.clone(),
                                path: s.path(),
                                problem: pr,
                                slot: idx(s),
                            });
                        }
                    }
                }
            }
            for g in self.groups.iter().filter(|g| g.page == *p) {
                let items: &[J] = match get(obj, &g.name) {
                    Some(J::Arr(a)) => a,
                    None | Some(J::Null) => &[],
                    Some(_) => {
                        f.push(Finding {
                            page: p.clone(),
                            path: format!("{p}.{}", g.name),
                            problem: Problem::WrongType("a list of items".into()),
                            slot: None,
                        });
                        &[]
                    }
                };
                if items.len() < g.min {
                    f.push(Finding {
                        page: p.clone(),
                        path: format!("{p}.{}", g.name),
                        problem: Problem::TooFew {
                            have: items.len(),
                            min: g.min,
                        },
                        slot: None,
                    });
                }
                if items.len() > g.max {
                    f.push(Finding {
                        page: p.clone(),
                        path: format!("{p}.{}", g.name),
                        problem: Problem::TooMany {
                            have: items.len(),
                            max: g.max,
                        },
                        slot: None,
                    });
                }
                for (i, item) in items.iter().enumerate() {
                    let place = format!("{p}.{}[{i}]", g.name);
                    let J::Obj(o) = item else {
                        f.push(Finding {
                            page: p.clone(),
                            path: place,
                            problem: Problem::WrongType("an object".into()),
                            slot: None,
                        });
                        continue;
                    };
                    for (k, _) in o {
                        if !self.group_fields(p, &g.name).any(|s| s.name == *k) {
                            f.push(Finding {
                                page: p.clone(),
                                path: format!("{place}.{k}"),
                                problem: Problem::UnknownKey,
                                slot: None,
                            });
                        }
                    }
                    for s in self.group_fields(p, &g.name) {
                        let applies = Self::applies(s.when.as_ref(), o);
                        match get(o, &s.name) {
                            None | Some(J::Null) => {
                                if s.required && applies {
                                    asked += 1;
                                    f.push(Finding {
                                        page: p.clone(),
                                        path: format!("{place}.{}", s.name),
                                        problem: Problem::Missing,
                                        slot: idx(s),
                                    });
                                }
                            }
                            Some(v) => {
                                read += 1;
                                asked += 1;
                                if let Some(pr) = Self::value_problem(s, v) {
                                    f.push(Finding {
                                        page: p.clone(),
                                        path: format!("{place}.{}", s.name),
                                        problem: pr,
                                        slot: idx(s),
                                    });
                                }
                                if let J::Str(x) = v {
                                    let key = s.path();
                                    match names.iter_mut().find(|(k, _)| *k == key) {
                                        Some((_, vs)) => vs.push(x.clone()),
                                        None => names.push((key, vec![x.clone()])),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // references: the named item must exist
        let values_of = |key: &str| {
            names
                .iter()
                .find(|(k, _)| k == key)
                .map_or(&[][..], |(_, v)| v.as_slice())
        };
        let mut refs = Vec::new();
        for page in &self.pages {
            let p = &page.name;
            let Some(J::Obj(obj)) = get(pages, p) else {
                continue;
            };
            for s in self.page_slots(p) {
                if let (SlotType::Ref(t), Some(J::Str(v))) = (&s.ty, get(obj, &s.name)) {
                    refs.push((p.clone(), s.path(), t.clone(), v.clone(), idx(s)));
                }
            }
            for g in self.groups.iter().filter(|g| g.page == *p) {
                let Some(J::Arr(items)) = get(obj, &g.name) else {
                    continue;
                };
                for (i, item) in items.iter().enumerate() {
                    let J::Obj(o) = item else { continue };
                    for s in self.group_fields(p, &g.name) {
                        if let (SlotType::Ref(t), Some(J::Str(v))) = (&s.ty, get(o, &s.name)) {
                            refs.push((
                                p.clone(),
                                format!("{p}.{}[{i}].{}", g.name, s.name),
                                t.clone(),
                                v.clone(),
                                idx(s),
                            ));
                        }
                    }
                }
            }
        }
        for (page, path, target, v, slot) in refs {
            if !values_of(&target).contains(&v) {
                f.push(Finding {
                    page,
                    path,
                    problem: Problem::NoSuchName(target),
                    slot,
                });
            }
        }
        // rules
        for r in &self.rules {
            let (p, g) = match r {
                Rule::Unique(p, g, ..) => (p, g),
            };
            let Some(J::Obj(obj)) = get(pages, p) else {
                continue;
            };
            let Some(J::Arr(items)) = get(obj, g) else {
                continue;
            };
            match r {
                Rule::Unique(_, _, field, _) => {
                    let mut seen: Vec<String> = Vec::new();
                    for (i, item) in items.iter().enumerate() {
                        let J::Obj(o) = item else { continue };
                        let v = match get(o, field) {
                            Some(J::Str(s)) => s.clone(),
                            Some(J::Num(x)) => x.to_string(),
                            _ => continue,
                        };
                        if seen.contains(&v) {
                            f.push(Finding {
                                page: p.clone(),
                                path: format!("{p}.{g}[{i}].{field}"),
                                problem: Problem::NotUnique(v),
                                slot: None,
                            });
                        } else {
                            seen.push(v);
                        }
                    }
                }
            }
        }
        let mut warnings = Vec::new();
        self.check_item_rules(pages, &mut f, &mut warnings);
        match get(&root, "out_of_range_intent") {
            None | Some(J::Null) => {}
            Some(J::Arr(notes)) => self.check_intent(notes, &mut f, &mut warnings),
            Some(_) => f.push(Finding {
                page: String::new(),
                path: "out_of_range_intent".into(),
                problem: Problem::WrongType("list".into()),
                slot: None,
            }),
        }
        if read == 0 {
            f.push(Finding {
                page: String::new(),
                path: String::new(),
                problem: Problem::NothingRead,
                slot: None,
            });
        }
        let pages_done = self
            .pages
            .iter()
            .map(|p| (p.name.clone(), !f.iter().any(|x| x.page == p.name)))
            .collect();
        let pending = self
            .slots
            .iter()
            .filter_map(|s| s.pending.clone().map(|part| (s.path(), part)))
            .collect();
        Ok(SheetCheck {
            law: self.name.clone(),
            law_id: self.law_id(),
            findings: f,
            warnings,
            read,
            asked,
            pages: pages_done,
            pending,
            slot_info: self.slots.clone(),
        })
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

impl SheetCheck {
    /// The report in Japanese, one line per finding, grouped by page
    #[must_use]
    pub fn report_japanese(&self) -> String {
        let mut s = format!("sheet law {} ({})\n", self.law, &hex(&self.law_id)[..16]);
        let _ = writeln!(
            s,
            "読めた値 {} / 求められた値 {}、見つかった問題 {} 件",
            self.read,
            self.asked,
            self.findings.len()
        );
        for (page, ok) in &self.pages {
            let n = self.findings.iter().filter(|f| f.page == *page).count();
            let _ = writeln!(
                s,
                "[{}] {page}{}",
                if *ok { "完成" } else { "未完成" },
                if *ok {
                    String::new()
                } else {
                    format!(" ({n} 件)")
                }
            );
            for x in self.findings.iter().filter(|f| f.page == *page) {
                let slot = x.slot.and_then(|i| self.slot_info.get(i));
                let _ = writeln!(s, "  - {}: {}", x.path, x.problem.japanese(slot));
            }
        }
        for x in self
            .findings
            .iter()
            .filter(|f| !self.pages.iter().any(|(p, _)| *p == f.page))
        {
            let _ = writeln!(
                s,
                "  - {}: {}",
                if x.path.is_empty() {
                    "(全体)"
                } else {
                    &x.path
                },
                x.problem.japanese(None)
            );
        }
        if !self.warnings.is_empty() {
            let _ = writeln!(
                s,
                "注意 {} 件 (描画は止めない、意図どおりか確かめる)",
                self.warnings.len()
            );
            for x in &self.warnings {
                let _ = writeln!(s, "  - {}: {}", x.path, x.problem.japanese(None));
            }
        }
        let _ = writeln!(
            s,
            "{}",
            if self.complete() {
                "→ 組全体が完成 (描画できる)"
            } else {
                "→ 未完成 (上の問題を直すまで描画しない)"
            }
        );
        s
    }

    /// The check as JSON (shape version [`SHEET_CHECK_JSON_VERSION`]), keys in a fixed order
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut s = format!(
            "{{\"sheet_check\":{SHEET_CHECK_JSON_VERSION},\"law\":{},\"law_id\":\"{}\",\"complete\":{},\"read\":{},\"asked\":{},\"pages\":[",
            json_str(&self.law),
            hex(&self.law_id),
            self.complete(),
            self.read,
            self.asked
        );
        for (i, (p, ok)) in self.pages.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(s, "{{\"page\":{},\"complete\":{ok}}}", json_str(p));
        }
        for (key, list) in [("findings", &self.findings), ("warnings", &self.warnings)] {
            let _ = write!(s, "],\"{key}\":[");
            for (i, x) in list.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                let slot = x.slot.and_then(|i| self.slot_info.get(i));
                let _ = write!(
                    s,
                    "{{\"page\":{},\"path\":{},\"code\":\"{}\",\"reason\":{}}}",
                    json_str(&x.page),
                    json_str(&x.path),
                    x.problem.code(),
                    json_str(&x.problem.japanese(slot))
                );
            }
        }
        s.push_str("],\"pending\":[");
        for (i, (p, part)) in self.pending.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(
                s,
                "{{\"slot\":{},\"part\":{}}}",
                json_str(p),
                json_str(part)
            );
        }
        s.push_str("]}");
        s
    }
}

// ── plausibility rules ──────────────────────────────────────────────────────────────────────

/// The left side of a condition: a field of the item, a field of the item another field names
/// (`material->elasticity`), or the sum of fields (`sole+heel`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
    Field(String),
    Deref(String, String),
    Sum(Vec<String>),
}

/// How a condition compares
#[derive(Debug, Clone, PartialEq)]
pub enum Test {
    /// The value is present
    Present,
    /// One of the words
    In(Vec<String>),
    /// None of the words
    NotIn(Vec<String>),
    /// The text mentions one of the words (any case, as a part of a word too)
    Mentions(Vec<String>),
    /// At most the bound
    AtMost(Bound),
    /// At least the bound
    AtLeast(Bound),
}

/// A number, or a page slot's value times a factor (`turnaround.stature*80`)
#[derive(Debug, Clone, PartialEq)]
pub enum Bound {
    Num(f64),
    Scaled(String, f64),
}

/// One condition on an item
#[derive(Debug, Clone, PartialEq)]
pub struct Cond {
    pub side: Side,
    pub test: Test,
}

/// A rule over the items of one group (`page.group`)
#[derive(Debug, Clone, PartialEq)]
pub enum ItemRule {
    /// `from` and `to` lie on one chain, `from` above `to`
    Along {
        group: String,
        from: String,
        to: String,
        description: String,
    },
    /// When `cond` holds for an item, one of `require` holds too
    When {
        warn: bool,
        group: String,
        cond: Cond,
        require: Vec<Cond>,
        description: String,
    },
    /// Where two items' spans (`from`..`to` on one chain) overlap, the one with the higher
    /// `order` has at least the other's `ease`
    Nesting {
        group: String,
        order: String,
        ease: String,
        from: String,
        to: String,
        description: String,
    },
    /// Every value of the field is named by some reference to it
    Used {
        warn: bool,
        field: String,
        description: String,
    },
}

fn parse_cond(n: usize, t: &str) -> Result<Cond, SheetLawError> {
    let (lhs, test) = if let Some((a, b)) = t.split_once('~') {
        (
            a,
            Test::Mentions(b.split('|').map(str::to_lowercase).collect()),
        )
    } else if let Some((a, b)) = t.split_once("!=") {
        (a, Test::NotIn(b.split('|').map(str::to_owned).collect()))
    } else if let Some((a, b)) = t.split_once("<=") {
        (a, Test::AtMost(parse_bound(n, b)?))
    } else if let Some((a, b)) = t.split_once(">=") {
        (a, Test::AtLeast(parse_bound(n, b)?))
    } else if let Some((a, b)) = t.split_once('=') {
        (a, Test::In(b.split('|').map(str::to_owned).collect()))
    } else {
        (t, Test::Present)
    };
    let side = if let Some((r, f)) = lhs.split_once("->") {
        Side::Deref(r.to_owned(), f.to_owned())
    } else if lhs.contains('+') {
        Side::Sum(lhs.split('+').map(str::to_owned).collect())
    } else {
        Side::Field(lhs.to_owned())
    };
    let names: Vec<&str> = match &side {
        Side::Field(f) => vec![f],
        Side::Deref(r, f) => vec![r, f],
        Side::Sum(v) => v.iter().map(String::as_str).collect(),
    };
    if !names.iter().all(|x| ident(x)) {
        return Err(lerr(n, format!("condition `{t}`: field names")));
    }
    Ok(Cond { side, test })
}

fn parse_bound(n: usize, b: &str) -> Result<Bound, SheetLawError> {
    if let Some((slot, k)) = b.split_once('*') {
        let k = k
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| lerr(n, format!("bound `{b}`: a factor")))?;
        return Ok(Bound::Scaled(slot.to_owned(), k));
    }
    b.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(Bound::Num)
        .ok_or_else(|| lerr(n, format!("bound `{b}`: a number or page.slot*factor")))
}

/// The words of a `rule` line after `rule`
#[allow(clippy::many_single_char_names)] // n line number, w words, g / p page.group, as in `from_file`
fn parse_item_rule(n: usize, w: &[&str], description: &str) -> Result<ItemRule, SheetLawError> {
    let description = description.to_owned();
    if description.is_empty() {
        return Err(lerr(n, "a plausibility rule needs a reason after ` : `"));
    }
    let group_of = |t: &str| -> Result<(String, String), SheetLawError> {
        let p: Vec<&str> = t.split('.').collect();
        match p.as_slice() {
            [a, b, c] => Ok((format!("{a}.{b}"), (*c).to_owned())),
            _ => Err(lerr(n, format!("`{t}`: page.group.field"))),
        }
    };
    let (warn, w) = if w.first() == Some(&"warn") {
        (true, &w[1..])
    } else {
        (false, w)
    };
    match w {
        ["along", from_field, to_field] if !warn => {
            let (g, from) = group_of(from_field)?;
            let (g2, to) = group_of(to_field)?;
            if g != g2 {
                return Err(lerr(n, "along: two fields of one group"));
            }
            Ok(ItemRule::Along {
                group: g,
                from,
                to,
                description,
            })
        }
        ["when", group, cond, "require", rest @ ..] if !rest.is_empty() => {
            if group.split('.').count() != 2 {
                return Err(lerr(n, "when: page.group"));
            }
            let mut require = Vec::new();
            for (i, word) in rest.iter().enumerate() {
                if i % 2 == 1 {
                    if *word != "or" {
                        return Err(lerr(n, "require: conditions joined by `or`"));
                    }
                } else {
                    require.push(parse_cond(n, word)?);
                }
            }
            if rest.len() % 2 == 0 {
                return Err(lerr(n, "require: a condition after `or`"));
            }
            Ok(ItemRule::When {
                warn,
                group: (*group).to_owned(),
                cond: parse_cond(n, cond)?,
                require,
                description,
            })
        }
        ["nesting", group, "order", order, "ease", ease, "along", from, to] if !warn => {
            if group.split('.').count() != 2 {
                return Err(lerr(n, "nesting: page.group"));
            }
            Ok(ItemRule::Nesting {
                group: (*group).to_owned(),
                order: (*order).to_owned(),
                ease: (*ease).to_owned(),
                from: (*from).to_owned(),
                to: (*to).to_owned(),
                description,
            })
        }
        ["used", f] => {
            group_of(f)?;
            Ok(ItemRule::Used {
                warn,
                field: (*f).to_owned(),
                description,
            })
        }
        _ => Err(lerr(
            n,
            "rule along / [warn] when … require … / nesting … / [warn] used page.group.field",
        )),
    }
}

impl Cond {
    fn names(&self) -> Vec<String> {
        match &self.side {
            Side::Field(f) => vec![f.clone()],
            Side::Deref(r, _) => vec![r.clone()],
            Side::Sum(v) => v.clone(),
        }
    }
}

impl ItemRule {
    /// The fields (`page.group.field`) the rule reads in its own group
    fn fields(&self) -> Vec<String> {
        match self {
            Self::Along {
                group, from, to, ..
            } => vec![format!("{group}.{from}"), format!("{group}.{to}")],
            Self::When {
                group,
                cond,
                require,
                ..
            } => cond
                .names()
                .into_iter()
                .chain(require.iter().flat_map(Cond::names))
                .map(|f| format!("{group}.{f}"))
                .collect(),
            Self::Nesting {
                group,
                order,
                ease,
                from,
                to,
                ..
            } => [order, ease, from, to]
                .iter()
                .map(|f| format!("{group}.{f}"))
                .collect(),
            Self::Used { field, .. } => vec![field.clone()],
        }
    }
}

const fn num(v: Option<&J>) -> Option<f64> {
    match v {
        Some(J::Num(x)) => Some(*x),
        _ => None,
    }
}

impl SheetLaw {
    fn chain_span(&self, a: &str, b: &str) -> Option<(usize, usize, usize)> {
        self.chains.iter().enumerate().find_map(|(c, (_, marks))| {
            let ia = marks.iter().position(|m| m == a)?;
            let ib = marks.iter().position(|m| m == b)?;
            Some((c, ia, ib))
        })
    }

    /// The value a condition's side reads in an item (`None`: not there, the rule does not apply)
    fn side_value<'a>(
        &self,
        pages: &'a [(String, J)],
        group: &str,
        item: &'a [(String, J)],
        side: &Side,
    ) -> Option<J> {
        match side {
            Side::Field(f) => get(item, f).filter(|v| !matches!(v, J::Null)).cloned(),
            Side::Sum(fs) => {
                let mut t = 0.0;
                for f in fs {
                    t += num(get(item, f))?;
                }
                Some(J::Num(t))
            }
            Side::Deref(r, f) => {
                let J::Str(name) = get(item, r)? else {
                    return None;
                };
                let target = self
                    .slots
                    .iter()
                    .find(|s| s.path() == format!("{group}.{r}"))
                    .and_then(|s| match &s.ty {
                        SlotType::Ref(t) => Some(t.clone()),
                        _ => None,
                    })?;
                let mut parts = target.split('.');
                let (tp, tg, tf) = (parts.next()?, parts.next()?, parts.next()?);
                let Some(J::Obj(page)) = get(pages, tp) else {
                    return None;
                };
                let Some(J::Arr(items)) = get(page, tg) else {
                    return None;
                };
                items.iter().find_map(|it| match it {
                    J::Obj(o) if matches!(get(o, tf), Some(J::Str(x)) if x == name) => {
                        get(o, f).cloned()
                    }
                    _ => None,
                })
            }
        }
    }

    fn bound_value(pages: &[(String, J)], b: &Bound) -> Option<f64> {
        match b {
            Bound::Num(x) => Some(*x),
            Bound::Scaled(slot, k) => {
                let (p, name) = slot.split_once('.')?;
                let Some(J::Obj(o)) = get(pages, p) else {
                    return None;
                };
                num(get(o, name)).map(|v| v * k)
            }
        }
    }

    /// Whether a condition holds (`None`: its value is not there)
    fn holds(
        &self,
        pages: &[(String, J)],
        group: &str,
        item: &[(String, J)],
        c: &Cond,
    ) -> Option<bool> {
        let v = self.side_value(pages, group, item, &c.side);
        Some(match &c.test {
            Test::Present => v.is_some(),
            Test::In(words) => matches!(v?, J::Str(x) if words.contains(&x)),
            Test::NotIn(words) => !matches!(v?, J::Str(x) if words.contains(&x)),
            Test::Mentions(words) => {
                matches!(v?, J::Str(x) if { let x = x.to_lowercase(); words.iter().any(|w| x.contains(w.as_str())) })
            }
            Test::AtMost(b) => num(v.as_ref())? <= Self::bound_value(pages, b)?,
            Test::AtLeast(b) => num(v.as_ref())? >= Self::bound_value(pages, b)?,
        })
    }

    fn items<'a>(pages: &'a [(String, J)], group: &str) -> &'a [J] {
        let Some((p, g)) = group.split_once('.') else {
            return &[];
        };
        match get(pages, p) {
            Some(J::Obj(o)) => match get(o, g) {
                Some(J::Arr(a)) => a,
                _ => &[],
            },
            _ => &[],
        }
    }

    /// The design's values the ranges do not hold: each is listed as a warning (the slot keeps
    /// a value inside its range), a malformed note is an error
    fn check_intent(&self, notes: &[J], errors: &mut Vec<Finding>, warnings: &mut Vec<Finding>) {
        for (i, note) in notes.iter().enumerate() {
            let path = format!("out_of_range_intent[{i}]");
            let mut err = |page: &str, problem| {
                errors.push(Finding {
                    page: page.to_owned(),
                    path: path.clone(),
                    problem,
                    slot: None,
                });
            };
            let J::Obj(o) = note else {
                err("", Problem::WrongType("object {slot, wants, why}".into()));
                continue;
            };
            if o.iter()
                .any(|(k, _)| !["slot", "wants", "why"].contains(&k.as_str()))
            {
                err("", Problem::UnknownKey);
                continue;
            }
            let Some(J::Str(name)) = get(o, "slot") else {
                err("", Problem::WrongType("slot: page.slot".into()));
                continue;
            };
            let page = name.split('.').next().unwrap_or_default();
            let Some((lo, hi)) = self
                .slots
                .iter()
                .find(|s| s.path() == *name)
                .and_then(|s| s.range)
            else {
                err(page, Problem::NoSuchName("a slot with a range".into()));
                continue;
            };
            let Some(J::Num(value)) = get(o, "wants") else {
                err(page, Problem::WrongType("wants: number".into()));
                continue;
            };
            match get(o, "why") {
                Some(J::Str(w)) if w.chars().count() <= INTENT_WHY_MAX => {}
                Some(J::Str(_)) => {
                    err(page, Problem::TooLong(INTENT_WHY_MAX));
                    continue;
                }
                _ => {
                    err(page, Problem::WrongType("why: text".into()));
                    continue;
                }
            }
            let value = *value;
            if (lo..=hi).contains(&value) {
                err(
                    page,
                    Problem::Implausible(format!(
                        "{name} = {value} is inside its range: write it in the slot"
                    )),
                );
                continue;
            }
            warnings.push(Finding {
                page: page.to_owned(),
                path,
                problem: Problem::DesignWants {
                    slot: name.clone(),
                    value,
                    lo,
                    hi,
                },
                slot: None,
            });
        }
    }

    #[allow(clippy::too_many_lines)] // one block per rule kind
    fn check_item_rules(
        &self,
        pages: &[(String, J)],
        errors: &mut Vec<Finding>,
        warnings: &mut Vec<Finding>,
    ) {
        for rule in &self.item_rules {
            match rule {
                ItemRule::Along {
                    group,
                    from,
                    to,
                    description,
                } => {
                    for (i, it) in Self::items(pages, group).iter().enumerate() {
                        let J::Obj(o) = it else { continue };
                        if let (Some(J::Str(a)), Some(J::Str(b))) = (get(o, from), get(o, to)) {
                            let problem = match self.chain_span(a, b) {
                                Some((_, ia, ib)) if ia < ib => continue,
                                Some(_) => Problem::NotOrdered(a.clone(), b.clone()),
                                None => Problem::Implausible(format!("{description} ({a} → {b})")),
                            };
                            errors.push(Finding {
                                page: page_of(group),
                                path: format!("{group}[{i}]"),
                                problem,
                                slot: None,
                            });
                        }
                    }
                }
                ItemRule::When {
                    warn,
                    group,
                    cond,
                    require,
                    description,
                } => {
                    for (i, it) in Self::items(pages, group).iter().enumerate() {
                        let J::Obj(o) = it else { continue };
                        if self.holds(pages, group, o, cond) != Some(true) {
                            continue;
                        }
                        let results: Vec<Option<bool>> = require
                            .iter()
                            .map(|c| self.holds(pages, group, o, c))
                            .collect();
                        // an absent value is the missing-slot check's business, unless presence is what is asked
                        let broken = results.iter().all(|r| *r == Some(false))
                            || (results.iter().all(Option::is_none)
                                && require.iter().all(|c| matches!(c.test, Test::Present)));
                        if broken {
                            let x = Finding {
                                page: page_of(group),
                                path: format!("{group}[{i}]"),
                                problem: Problem::Implausible(description.clone()),
                                slot: None,
                            };
                            if *warn {
                                warnings.push(x);
                            } else {
                                errors.push(x);
                            }
                        }
                    }
                }
                ItemRule::Nesting {
                    group,
                    order,
                    ease,
                    from,
                    to,
                    description,
                } => {
                    let items = Self::items(pages, group);
                    let span = |o: &[(String, J)]| -> Option<(usize, usize, usize, f64, f64)> {
                        let (J::Str(a), J::Str(b)) = (get(o, from)?, get(o, to)?) else {
                            return None;
                        };
                        let (c, ia, ib) = self.chain_span(a, b)?;
                        Some((
                            c,
                            ia.min(ib),
                            ia.max(ib),
                            num(get(o, order))?,
                            num(get(o, ease))?,
                        ))
                    };
                    for (i, a) in items.iter().enumerate() {
                        let J::Obj(oa) = a else { continue };
                        let Some((ca, la, ha, ord_a, ease_a)) = span(oa) else {
                            continue;
                        };
                        for b in items {
                            let J::Obj(ob) = b else { continue };
                            let Some((cb, lb, hb, ord_b, ease_b)) = span(ob) else {
                                continue;
                            };
                            if ca == cb && la < hb && lb < ha && ord_a > ord_b && ease_a < ease_b {
                                errors.push(Finding {
                                    page: page_of(group),
                                    path: format!("{group}[{i}]"),
                                    problem: Problem::Implausible(description.clone()),
                                    slot: None,
                                });
                                break;
                            }
                        }
                    }
                }
                ItemRule::Used {
                    warn,
                    field,
                    description: _,
                } => {
                    let (group, f) = field.rsplit_once('.').unwrap_or((field, ""));
                    let referrers: Vec<&Slot> = self
                        .slots
                        .iter()
                        .filter(|s| matches!(&s.ty, SlotType::Ref(t) if t == field))
                        .collect();
                    let mut used: Vec<String> = Vec::new();
                    for s in &referrers {
                        let Some(J::Obj(page)) = get(pages, &s.page) else {
                            continue;
                        };
                        match &s.group {
                            None => {
                                if let Some(J::Str(x)) = get(page, &s.name) {
                                    used.push(x.clone());
                                }
                            }
                            Some(g) => {
                                if let Some(J::Arr(items)) = get(page, g) {
                                    for it in items {
                                        if let J::Obj(o) = it {
                                            if let Some(J::Str(x)) = get(o, &s.name) {
                                                used.push(x.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    for (i, it) in Self::items(pages, group).iter().enumerate() {
                        if let J::Obj(o) = it {
                            if let Some(J::Str(name)) = get(o, f) {
                                if !used.contains(name) {
                                    let x = Finding {
                                        page: page_of(group),
                                        path: format!("{group}[{i}].{f}"),
                                        problem: Problem::NotUsed(name.clone()),
                                        slot: None,
                                    };
                                    if *warn {
                                        warnings.push(x);
                                    } else {
                                        errors.push(x);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn page_of(group: &str) -> String {
    group.split('.').next().unwrap_or_default().to_owned()
}

// ── generated documents ─────────────────────────────────────────────────────────────────────

fn range_text(s: &Slot) -> String {
    if let (false, Some((a, p))) = (s.family_ranges.is_empty(), &s.range_law) {
        return s
            .family_ranges
            .iter()
            .map(|(v, (lo, hi))| format!("{v}: {lo} – {hi}"))
            .collect::<Vec<_>>()
            .join("; ")
            + &format!(" (from {a}.{p})");
    }
    match (s.range, &s.range_law) {
        (Some((lo, hi)), Some((a, p))) => format!("{lo} – {hi} (from {a}.{p})"),
        (Some((lo, hi)), None) => format!("{lo} – {hi}"),
        (None, Some((a, p))) => format!("from {a}.{p}"),
        (None, None) => String::new(),
    }
}

fn need_text(s: &Slot) -> String {
    let base = if s.required { "required" } else { "optional" };
    match &s.when {
        None => base.to_owned(),
        Some(When::Eq(f, w)) => format!("{base} when {f} = {w}"),
        Some(When::Above(f, b)) => format!("{base} when {f} > {b}"),
    }
}

impl SheetLaw {
    /// The specification an author (a person or a model with no other context) follows to
    /// fill the sheet, generated from the law: pages, slot tables, groups, rules, the fill
    /// format and what complete means (Markdown)
    #[must_use]
    #[allow(clippy::many_single_char_names)] // s text, g group, x slot, d description, p / a / b names
    #[allow(clippy::too_many_lines)] // one block per section of the document, in its order
    pub fn spec(&self) -> String {
        let mut s = format!(
            "# Sheet law `{}`: how to fill a character sheet set\n\n",
            self.name
        );
        let _ = writeln!(s, "law id `{}`\n", hex(&self.law_id()));
        s.push_str("## What this is\n\n");
        for c in &self.claims {
            let _ = writeln!(s, "- {c}");
        }
        s.push_str("\n## How to answer\n\n");
        let _ = writeln!(
            s,
            "Answer with one JSON object and nothing else: `{{\"sheet_fill\": {SHEET_FILL_FORMAT}, \"law\": \"{}\", \"pages\": {{ … }}}}`. \
             Under `pages`, each page is an object; a slot is a key with its value; a group is a key with a list of items, each item an object of its fields. \
             Use only the keys listed below. Numbers are plain JSON numbers in the slot's unit. Text has no escapes (no `\\\\` and no `\\\"`). \
             A value you cannot decide is `null`: it will be listed as missing, which is better than a guess.\n",
            self.name
        );
        if let Some(first) = self.families.first() {
            let _ = writeln!(
                s,
                "Families: `{}` chooses the laws the ranges come from, and a range column lists each family's range. Choose the family from the idea, then use that family's whole range.",
                first.slot
            );
            for f in &self.families {
                let laws: Vec<String> = f
                    .laws
                    .iter()
                    .map(|(a, l)| format!("`{a}` = `{l}`"))
                    .collect();
                let _ = writeln!(s, "- `{}`: {}", f.value, laws.join(", "));
            }
            s.push('\n');
        }
        s.push_str("Units: `eye_height` = the height of the eyes above the sole is 1; `chin_eye` = the distance from the chin to the eye line is 1; `fraction` = 0 to 1; `m`, `mm`, `deg`, `ratio` as usual.\n\n");
        s.push_str("A file (`asset`) is an object: `{\"file\": \"name.png\", \"format\": \"png\", \"width\": 1024, \"height\": 1024, \"colour_space\": \"srgb\", \"provenance\": {\"source\": \"…\", \"licence\": \"…\", \"author\": \"…\", \"terms_sha256\": \"<64 hex digits of the licence text>\"}}`; formats: ");
        let _ = writeln!(s, "{}.\n", ASSET_FORMATS.join(", "));
        let _ = writeln!(
            s,
            "Derive every value from the idea and use the whole range: the middle of a range is not a target. \
             When the design wants a number outside a slot's range, put the nearest value inside the range in the slot and add a note at the top level: \
             `\"out_of_range_intent\": [{{\"slot\": \"base_body.knee\", \"wants\": 0.27, \"why\": \"…\"}}]` (the reason at most {INTENT_WHY_MAX} characters). \
             The note is listed as a warning, not an error, and tells where the ranges are too narrow.\n"
        );
        s.push_str("Complete means: every required slot (and every field of every item, when its condition holds) has a value of its type inside its range or vocabulary, every name you refer to exists, every rule holds, and every file has its provenance.\n");
        for page in &self.pages {
            let _ = writeln!(s, "\n## Page `{}`\n\n{}\n", page.name, page.purpose);
            let slots: Vec<&Slot> = self.page_slots(&page.name).collect();
            if !slots.is_empty() {
                s.push_str(
                    "| slot | type | unit | range | need | meaning |\n|---|---|---|---|---|---|\n",
                );
                for x in slots {
                    let _ = writeln!(
                        s,
                        "| `{}` | {} | {} | {} | {} | {} |",
                        x.name,
                        x.ty.word(),
                        x.unit.clone().unwrap_or_default(),
                        range_text(x),
                        need_text(x),
                        x.description
                    );
                }
            }
            for g in self.groups.iter().filter(|g| g.page == page.name) {
                let _ = writeln!(
                    s,
                    "\nGroup `{}` ({} to {} items): {}\n",
                    g.name, g.min, g.max, g.description
                );
                s.push_str(
                    "| field | type | unit | range | need | meaning |\n|---|---|---|---|---|---|\n",
                );
                for x in self.group_fields(&page.name, &g.name) {
                    let _ = writeln!(
                        s,
                        "| `{}` | {} | {} | {} | {} | {} |",
                        x.name,
                        x.ty.word(),
                        x.unit.clone().unwrap_or_default(),
                        range_text(x),
                        need_text(x),
                        x.description
                    );
                }
            }
        }
        if !self.texts.is_empty() {
            s.push_str("\n## What may be written on a page\n\n");
            for (allow, name, d) in &self.texts {
                let _ = writeln!(
                    s,
                    "- {} `{name}`: {d}",
                    if *allow { "allowed" } else { "forbidden" }
                );
            }
        }
        if !self.rules.is_empty() {
            s.push_str("\n## Rules\n\n");
            for r in &self.rules {
                match r {
                    Rule::Unique(p, g, f, d) => {
                        let _ = writeln!(s, "- `{p}.{g}.{f}` is unique: {d}");
                    }
                }
            }
        }
        if !self.chains.is_empty() {
            s.push_str("\n## Chains\n\nLandmarks from top to bottom; a span (`from` → `to`) lies on one chain, `from` above `to`.\n\n");
            for (name, marks) in &self.chains {
                let _ = writeln!(s, "- `{name}`: {}", marks.join(" → "));
            }
        }
        if !self.item_rules.is_empty() {
            s.push_str("\n## Plausibility\n\nAn error stops the drawing; a warning is listed and the drawing goes on (check it is meant). `a->b` is field `b` of the item that field `a` names; `~` means the text mentions one of the words.\n\n");
            for (r, text) in self.item_rules.iter().zip(&self.item_rule_texts) {
                let (warn, d) = match r {
                    ItemRule::When {
                        warn, description, ..
                    }
                    | ItemRule::Used {
                        warn, description, ..
                    } => (*warn, description),
                    ItemRule::Along { description, .. } | ItemRule::Nesting { description, .. } => {
                        (false, description)
                    }
                };
                let _ = writeln!(
                    s,
                    "- {} `{text}`: {d}",
                    if warn { "warning" } else { "error" }
                );
            }
        }
        s
    }

    /// A blank fill: every page, every slot `null`, every group with one item of `null` fields
    /// (JSON, two-space indented)
    #[must_use]
    pub fn template(&self) -> String {
        let mut s = format!(
            "{{\n  \"sheet_fill\": {SHEET_FILL_FORMAT},\n  \"law\": {},\n  \"pages\": {{\n",
            json_str(&self.name)
        );
        for (pi, page) in self.pages.iter().enumerate() {
            let _ = writeln!(s, "    {}: {{", json_str(&page.name));
            let mut entries: Vec<String> = self
                .page_slots(&page.name)
                .map(|x| format!("      {}: null", json_str(&x.name)))
                .collect();
            for g in self.groups.iter().filter(|g| g.page == page.name) {
                let fields: Vec<String> = self
                    .group_fields(&page.name, &g.name)
                    .map(|x| format!("{}: null", json_str(&x.name)))
                    .collect();
                entries.push(format!(
                    "      {}: [{{{}}}]",
                    json_str(&g.name),
                    fields.join(", ")
                ));
            }
            s.push_str(&entries.join(",\n"));
            let _ = writeln!(
                s,
                "\n    }}{}",
                if pi + 1 < self.pages.len() { "," } else { "" }
            );
        }
        s.push_str("  }\n}\n");
        s
    }
}
