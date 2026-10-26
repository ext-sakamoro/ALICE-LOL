//! Memory laws (`kind memory`, conformance/TASK.md "Memory"): one memory of a character — its
//! category, when it was formed, its weights on the character's traits, the memories it rests on
//! and when it comes to mind.
//!
//! A program computes the character from the set of memory laws it holds, so the identifier
//! covers every line that changes that computation and none of the lines the character says
//! (`line`): rewording a line keeps the memory the same law.

use std::collections::BTreeMap;

use alice_zip::law_id::LawIdHasher;

use super::{
    err, fields, hash_consts, hash_inputs, id_hex, is_label, is_name, law_id_start, predicate_eval,
    read_inputs, rest_after, tokens, words, Answer, Edition, Expr, Json, Names, Parser,
    RuleLawError, Scope, Tok, Ty, Value, MEMORY_SEMANTICS_ID, MEMORY_SEMANTICS_PINS, PEOPLE,
};

/// The kind tag of a memory law
pub const MEMORY_LAW_KIND: &[u8] = b"lol.law.memory/v1";

/// The largest magnitude of a weight (`i32::MAX`): a sum of the weights of every law a file of
/// the size limit can hold stays far inside `i64`
pub const MAX_WEIGHT: i64 = 2_147_483_647;

/// A memory law (`kind memory`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryLaw {
    pub(super) edition: Option<Edition>,
    category: String,
    formed: i64,
    weights: BTreeMap<String, i64>,
    links: Vec<[u8; 32]>,
    consts: Vec<(String, Ty, Value)>,
    inputs: Vec<(String, Ty)>,
    recall: Option<Expr>,
    said: Vec<(String, String)>,
}

impl MemoryLaw {
    /// Its category (`category <name>`)
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }

    /// When it was formed, as an ordinal (`formed <integer>`)
    #[must_use]
    pub const fn formed(&self) -> i64 {
        self.formed
    }

    /// Its weights, `(axis, weight)` in the order of the axis names
    #[must_use]
    pub fn weights(&self) -> Vec<(&str, i64)> {
        self.weights.iter().map(|(a, w)| (a.as_str(), *w)).collect()
    }

    /// Its weight on `axis` (0 when it has no `weight` line for it)
    #[must_use]
    pub fn weight(&self, axis: &str) -> i64 {
        self.weights.get(axis).copied().unwrap_or(0)
    }

    /// The identifiers of the memories it rests on, in ascending order
    #[must_use]
    pub fn links(&self) -> &[[u8; 32]] {
        &self.links
    }

    /// The lines the character may say when it comes to mind, `(label, text)` in file order
    #[must_use]
    pub fn lines(&self) -> &[(String, String)] {
        &self.said
    }

    pub(super) fn respond(&self, inp: &Json) -> Answer {
        let env = match read_inputs(&self.inputs, inp, None) {
            Ok(env) => env,
            Err(a) => return a,
        };
        // no money and no call: an evaluation gives a boolean
        let recalled = self
            .recall
            .as_ref()
            .is_some_and(|e| matches!(predicate_eval(e, &env), Ok(Value::Bool(true))));
        Answer::Outputs(Json::Object(vec![(
            "recalled".to_owned(),
            Json::Bool(recalled),
        )]))
    }

    pub(super) fn law_id(&self) -> [u8; 32] {
        let mut h: LawIdHasher = hash_consts(
            law_id_start(
                MEMORY_LAW_KIND,
                "memory",
                &MEMORY_SEMANTICS_ID,
                self.edition.as_ref(),
            ),
            &self.consts,
        );
        h = hash_inputs(h, &self.inputs)
            .str("category")
            .str(&self.category)
            .str("formed")
            .u64(self.formed.cast_unsigned())
            .str("weights")
            .u64(self.weights.len() as u64);
        for (axis, w) in &self.weights {
            h = h.str(axis).u64(w.cast_unsigned());
        }
        h = h.str("links").u64(self.links.len() as u64);
        for id in &self.links {
            h = h.id(id);
        }
        h = h.str("recall").u32(u32::from(self.recall.is_some()));
        if let Some(e) = &self.recall {
            h = e.hash(h);
        }
        h.finish()
    }
}

/// A weight: an integer literal with an optional `-`, at most [`MAX_WEIGHT`] in magnitude, not `-0`
fn weight_literal(n: usize, s: &str) -> Result<i64, RuleLawError> {
    let (neg, digits) = s.strip_prefix('-').map_or((false, s), |d| (true, d));
    let bad = || {
        err(
            n,
            format!("`{s}` is not a weight (an integer from -{MAX_WEIGHT} to {MAX_WEIGHT})"),
        )
    };
    let well_formed = !digits.is_empty()
        && digits.bytes().all(|c| c.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && digits.len() <= 10;
    if !well_formed || (neg && digits == "0") {
        return Err(bad());
    }
    let v: i64 = digits.parse().map_err(|_| bad())?;
    if v > MAX_WEIGHT {
        return Err(bad());
    }
    Ok(if neg { -v } else { v })
}

/// Reads the body of a memory law (the lines after its header)
#[allow(clippy::too_many_lines)]
pub(super) fn read_memory(
    ls: &[(usize, &str)],
    edition: Option<Edition>,
) -> Result<MemoryLaw, RuleLawError> {
    let mut names = Names(Vec::new());
    let mut consts: Vec<(String, Ty, Value)> = Vec::new();
    let mut inputs: Vec<(String, Ty)> = Vec::new();
    let mut category: Option<String> = None;
    let mut formed: Option<i64> = None;
    let mut weights: BTreeMap<String, i64> = BTreeMap::new();
    let mut links: Vec<[u8; 32]> = Vec::new();
    let mut recall: Option<Expr> = None;
    let mut said: Vec<(String, String)> = Vec::new();
    for &(n, l) in ls {
        let w = words(l);
        let head = w[0].0;
        let trimmed = l.trim_start_matches([' ', '\t']);
        if trimmed.to_ascii_lowercase().starts_with("x-") {
            let (kw, rest) = l.split_once(' ').unwrap_or((l, ""));
            if kw != "x-input" || rest.starts_with([' ', '\t']) {
                return Err(err(
                    n,
                    format!(
                        "`{}` is not an x- line of a memory law",
                        l.trim_matches([' ', '\t'])
                    ),
                ));
            }
            let rw = words(rest);
            let Some(&(name, at)) = rw.first() else {
                return Err(err(n, "`x-input <name> <type>`"));
            };
            names.declare(n, name, "input")?;
            let ty = super::parse_type(n, &rest[at..])?;
            if ty == Ty::Money {
                return Err(err(n, "a memory law has no money"));
            }
            inputs.push((name.to_owned(), ty));
            continue;
        }
        match head {
            h if PEOPLE.contains(&h) || h == "kind" => {}
            "const" => {
                super::const_line(n, l, &mut names, &mut consts)?;
                if consts.last().is_some_and(|(_, t, _)| *t == Ty::Money) {
                    return Err(err(n, "a memory law has no money"));
                }
            }
            "category" => {
                let f = fields(l);
                if category.is_some() || f.len() != 2 || !is_name(f[1]) {
                    return Err(err(n, "one `category <name>` line"));
                }
                category = Some(f[1].to_owned());
            }
            "formed" => {
                let f = fields(l);
                if formed.is_some() || f.len() != 2 {
                    return Err(err(n, "one `formed <integer>` line"));
                }
                let v = match tokens(f[1]).map_err(|e| err(n, e))?.as_slice() {
                    [Tok::Int(v)] => *v,
                    _ => return Err(err(n, format!("`{}` is not an integer literal", f[1]))),
                };
                formed = Some(v);
            }
            "weight" => {
                let f = fields(l);
                let [_, axis, value] = f.as_slice() else {
                    return Err(err(n, "`weight <axis> <weight>`"));
                };
                if !is_name(axis) {
                    return Err(err(n, format!("`{axis}` is not a name (axis)")));
                }
                if weights.contains_key(*axis) {
                    return Err(err(n, format!("a second `weight` line for `{axis}`")));
                }
                weights.insert((*axis).to_owned(), weight_literal(n, value)?);
            }
            "link" => {
                let f = fields(l);
                let id = match f.as_slice() {
                    [_, v] => id_hex(v),
                    _ => None,
                }
                .ok_or_else(|| err(n, "`link <64 lower-case hex>`"))?;
                if links.contains(&id) {
                    return Err(err(n, "a second `link` line for the same identifier"));
                }
                links.push(id);
            }
            "recall" => {
                if recall.is_some() || w.len() < 3 || w[1].0 != "when" {
                    return Err(err(n, "one `recall when <expression>` line"));
                }
                let src = rest_after(l, &w, 1);
                let p = Parser {
                    src: &src,
                    t: tokens(&src).map_err(|e| err(n, e))?,
                    i: 0,
                    depth: 0,
                    scope: Scope::Predicate {
                        types: &inputs,
                        consts: &consts,
                        uses_hmac: false,
                    },
                };
                let (e, ty) = p.parse().map_err(|e| err(n, e))?;
                if ty != Ty::Bool {
                    return Err(err(n, format!("`{src}` is not a boolean")));
                }
                recall = Some(e);
            }
            "line" => {
                let bad = || err(n, "`line <label>: <text>`");
                let (head, text) = l.split_once(':').ok_or_else(bad)?;
                let hf = fields(head);
                let [_, label] = hf.as_slice() else {
                    return Err(bad());
                };
                let text = text.trim_matches([' ', '\t']);
                if text.is_empty() {
                    return Err(err(n, "a `line` has a text after the `:`"));
                }
                if !is_label(label) || said.iter().any(|(lb, _)| lb == label) {
                    return Err(err(n, format!("label `{label}` is reserved or used twice")));
                }
                said.push(((*label).to_owned(), text.to_owned()));
            }
            other => return Err(err(n, format!("`{other}` is not a line of a memory law"))),
        }
    }
    let (Some(category), Some(formed)) = (category, formed) else {
        return Err(err(
            0,
            "a memory law has one `category` line and one `formed` line",
        ));
    };
    links.sort_unstable();
    Ok(MemoryLaw {
        edition,
        category,
        formed,
        weights,
        links,
        consts,
        inputs,
        recall,
        said,
    })
}

/// The measured value of every pin of [`MEMORY_SEMANTICS_PINS`]
///
/// The answers of a fixed memory law to fixed requests (the expression holding and not, a missing
/// and a mistyped input), and of a memory law without `recall`
#[must_use]
pub fn memory_semantics_fingerprints() -> Vec<(&'static str, [u8; 32])> {
    vec![
        (MEMORY_SEMANTICS_PINS[0].0, memory_fingerprint()),
        (MEMORY_SEMANTICS_PINS[1].0, MEMORY_SEMANTICS_PINS[1].1),
    ]
}

fn memory_fingerprint() -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let read = |text: &str| {
        read_memory(
            &super::lines(text).expect("the fingerprint memory reads"),
            None,
        )
        .expect("the fingerprint memory reads")
    };
    let law = read(
        "kind memory\ncategory emotion\nformed 3\nweight cheer 3\nweight calm -2\n\
         x-input topic text\nx-input day integer\n\
         recall when topic == \"festival\" and day >= 2\nline a: hands full of paint\n",
    );
    let silent = read("kind memory\ncategory emotion\nformed 0\n");
    let mut h = Sha256::new();
    for r in [
        r#"{"topic": "festival", "day": 2}"#,
        r#"{"topic": "festival", "day": 1}"#,
        r#"{"topic": "rain", "day": 5}"#,
        r#"{"topic": "festival"}"#,
        r#"{"topic": 1, "day": 2}"#,
        r#"{"topic": "festival", "day": 2.5}"#,
    ] {
        let a = law.respond(&crate::law_input::parse_json(r).expect("fingerprint JSON"));
        h.update(format!("{r} -> {a:?}\n").as_bytes());
    }
    let a = silent.respond(&crate::law_input::parse_json("{}").expect("fingerprint JSON"));
    h.update(format!("silent {{}} -> {a:?}\n").as_bytes());
    for (axis, w) in law.weights() {
        h.update(format!("weight {axis} {w}\n").as_bytes());
    }
    h.finalize().into()
}
