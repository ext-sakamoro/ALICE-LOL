//! Contract laws (`kind contract`, conformance/TASK.md "Contract (`kind contract`)"): parties, a time
//! of creation, conditions each decided by a rule law named by its identifier, and obligations of
//! money from one party to another by a time.
//!
//! The answer to `{"now": <time>, "fulfilled": [...], "met": [...]}` is the status ALICE-Legal's
//! `Contract::check_status` gives a contract with those obligations fulfilled at that time
//! (`breached` when an unfulfilled obligation is past its time, otherwise `fulfilled` when every
//! obligation is, otherwise `active`), the unfulfilled obligations, what they still owe, and
//! whether every condition is met. The obligations of one contract are in one currency: the
//! obligation ALICE-Legal records holds an amount and no currency.

use alice_zip::law_id::LawIdHasher;

use super::{
    err, fields, law_id_start, money_text, read_value, statute::statute_time, Answer, Currencies,
    Edition, Json, RuleLawError, Ty, Value, CONTRACT_SEMANTICS_ID, PEOPLE,
};

/// The kind tag of a contract law
pub const CONTRACT_LAW_KIND: &[u8] = b"lol.law.contract/v1";

/// A condition of a contract: a label, the identifier of the rule law that decides it and a
/// description for people
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCondition {
    label: String,
    decided_by: [u8; 32],
    description: String,
}

impl ContractCondition {
    /// Its label
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The identifier of the rule law that decides it
    #[must_use]
    pub const fn decided_by(&self) -> &[u8; 32] {
        &self.decided_by
    }

    /// Its description, as written after the `:`
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }
}

/// An obligation of a contract: one party owes another an amount by a time
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractObligation {
    label: String,
    debtor: usize,
    creditor: usize,
    amount: i128,
    due: u64,
}

impl ContractObligation {
    /// Its label
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The index (in [`ContractLaw::parties`]) of the party that owes
    #[must_use]
    pub const fn debtor(&self) -> usize {
        self.debtor
    }

    /// The index of the party that is owed
    #[must_use]
    pub const fn creditor(&self) -> usize {
        self.creditor
    }

    /// The amount in minor units of the contract's currency ([`ContractLaw::currency`])
    #[must_use]
    pub const fn amount(&self) -> i128 {
        self.amount
    }

    /// The time it is due (seconds); it is breached when unfulfilled after it
    #[must_use]
    pub const fn due(&self) -> u64 {
        self.due
    }
}

/// A contract law (`kind contract`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractLaw {
    pub(super) edition: Option<Edition>,
    currencies: Currencies,
    parties: Vec<String>,
    created: u64,
    currency: String,
    conditions: Vec<ContractCondition>,
    obligations: Vec<ContractObligation>,
}

impl ContractLaw {
    /// The parties, in file order
    #[must_use]
    pub fn parties(&self) -> &[String] {
        &self.parties
    }

    /// The time the contract was made (seconds)
    #[must_use]
    pub const fn created(&self) -> u64 {
        self.created
    }

    /// The currency of every obligation (three upper-case letters the currency table knows)
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }

    /// The conditions, in file order
    #[must_use]
    pub fn conditions(&self) -> &[ContractCondition] {
        &self.conditions
    }

    /// The obligations, in file order
    #[must_use]
    pub fn obligations(&self) -> &[ContractObligation] {
        &self.obligations
    }

    /// The labels of a request's list, each known and given once; `None` (the key absent) is `[]`
    fn labels(v: Option<&Json>, known: &[&str], what: &str) -> Result<Vec<bool>, Answer> {
        let mut given = vec![false; known.len()];
        let Some(v) = v else {
            return Ok(given);
        };
        let Json::Array(items) = v else {
            return Err(Answer::Rejected(format!("{what} is not a list")));
        };
        for item in items {
            let i = match item {
                Json::Text(t) => known.iter().position(|k| k == t),
                _ => None,
            };
            match i {
                Some(i) if !given[i] => given[i] = true,
                Some(_) => return Err(Answer::Rejected(format!("{what}: a label given twice"))),
                None => return Err(Answer::Rejected(format!("{what}: not a label of the law"))),
            }
        }
        Ok(given)
    }

    pub(super) fn respond(&self, inp: &Json) -> Answer {
        let Some(now) = inp.get("now") else {
            return Answer::RequestError("now missing".to_owned());
        };
        let Some(Value::Int(now)) = read_value(now, &Ty::Time, None) else {
            return Answer::Rejected("now does not have its type".to_owned());
        };
        let obl: Vec<&str> = self.obligations.iter().map(|o| o.label.as_str()).collect();
        let cond: Vec<&str> = self.conditions.iter().map(|c| c.label.as_str()).collect();
        let fulfilled = match Self::labels(inp.get("fulfilled"), &obl, "fulfilled") {
            Ok(f) => f,
            Err(a) => return a,
        };
        let met = match Self::labels(inp.get("met"), &cond, "met") {
            Ok(m) => m,
            Err(a) => return a,
        };
        let now = i128::from(now);
        let open: Vec<&ContractObligation> = self
            .obligations
            .iter()
            .zip(&fulfilled)
            .filter(|(_, f)| !**f)
            .map(|(o, _)| o)
            .collect();
        let status = if open.iter().any(|o| now > i128::from(o.due)) {
            "breached"
        } else if open.is_empty() {
            "fulfilled"
        } else {
            "active"
        };
        let Some(owed) = open
            .iter()
            .try_fold(0_i128, |sum, o| sum.checked_add(o.amount))
        else {
            return Answer::Rejected("an amount outside -2^127 to 2^127 - 1".to_owned());
        };
        Answer::Outputs(Json::Object(vec![
            ("status".to_owned(), Json::Text(status.to_owned())),
            (
                "unfulfilled".to_owned(),
                Json::Array(open.iter().map(|o| Json::Text(o.label.clone())).collect()),
            ),
            (
                "owed".to_owned(),
                Json::Text(format!("{owed} {}", self.currency)),
            ),
            (
                "conditions_met".to_owned(),
                Json::Bool(met.iter().all(|m| *m)),
            ),
        ]))
    }

    pub(super) fn law_id(&self) -> [u8; 32] {
        let mut h: LawIdHasher = law_id_start(
            CONTRACT_LAW_KIND,
            "contract",
            &CONTRACT_SEMANTICS_ID,
            self.edition.as_ref(),
        )
        .str("currencies")
        .id(&self.currencies.id)
        .str("parties")
        .u64(self.parties.len() as u64);
        for p in &self.parties {
            h = h.str(p);
        }
        h = h
            .str("created")
            .u64(self.created)
            .str("conditions")
            .u64(self.conditions.len() as u64);
        for c in &self.conditions {
            h = h.str(&c.label).id(&c.decided_by).str(&c.description);
        }
        h = h.str("obligations").u64(self.obligations.len() as u64);
        for o in &self.obligations {
            h = Value::Money(o.amount, self.currency.clone()).hash(
                h.str(&o.label)
                    .str(&self.parties[o.debtor])
                    .str(&self.parties[o.creditor]),
            );
            h = h.u64(o.due);
        }
        h.finish()
    }
}

fn is_label(s: &str) -> bool {
    let b = s.as_bytes();
    b.first().is_some_and(u8::is_ascii_lowercase)
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

/// Reads the body of a contract law (the lines after its header and `currencies` line)
#[allow(clippy::too_many_lines)]
pub(super) fn read_contract(
    ls: &[(usize, &str)],
    edition: Option<Edition>,
    cur: Option<&Currencies>,
) -> Result<ContractLaw, RuleLawError> {
    let cur = cur.ok_or_else(|| {
        err(
            0,
            "a contract law has one `currencies <id>` line: its obligations are money",
        )
    })?;
    let mut parties: Vec<String> = Vec::new();
    let mut created: Option<u64> = None;
    let mut conditions: Vec<ContractCondition> = Vec::new();
    let mut obligations: Vec<ContractObligation> = Vec::new();
    let mut currency: Option<String> = None;
    // every label of the law, and each party's index: one lookup per line, so reading is linear
    let mut labels: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut party_index: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for &(n, l) in ls {
        let w = fields(l);
        match w[0] {
            h if PEOPLE.contains(&h) || h == "kind" => {}
            "party" => {
                let [_, label] = w.as_slice() else {
                    return Err(err(n, "`party <label>`"));
                };
                if !is_label(label) {
                    return Err(err(n, format!("`{label}` is not a label")));
                }
                if !labels.insert((*label).to_owned()) {
                    return Err(err(n, format!("a second `{label}`")));
                }
                party_index.insert((*label).to_owned(), parties.len());
                parties.push((*label).to_owned());
            }
            "created" => {
                let [_, t] = w.as_slice() else {
                    return Err(err(n, "`created <time>`"));
                };
                if created.is_some() {
                    return Err(err(n, "a second `created` line"));
                }
                created = Some(statute_time(n, t)?);
            }
            "condition" => {
                const SHAPE: &str =
                    "`condition <label> decided by <law identifier>: <description>`";
                let (head, text) = l.split_once(':').ok_or_else(|| err(n, SHAPE))?;
                let description = text.trim_matches([' ', '\t']);
                let h = fields(head);
                let [_, label, "decided", "by", id] = h.as_slice() else {
                    return Err(err(n, SHAPE));
                };
                if description.is_empty() {
                    return Err(err(n, "a condition has a description after the `:`"));
                }
                if !is_label(label) {
                    return Err(err(n, format!("`{label}` is not a label")));
                }
                if !labels.insert((*label).to_owned()) {
                    return Err(err(n, format!("a second `{label}`")));
                }
                let decided_by =
                    super::id_hex(id).ok_or_else(|| err(n, "`decided by <64 lower-case hex>`"))?;
                conditions.push(ContractCondition {
                    label: (*label).to_owned(),
                    decided_by,
                    description: description.to_owned(),
                });
            }
            "obligation" => {
                const SHAPE: &str =
                    "`obligation <label> <party> to <party> <amount> <currency> due <time>`";
                let [_, label, debtor, "to", creditor, amount, code, "due", due] = w.as_slice()
                else {
                    return Err(err(n, SHAPE));
                };
                if !is_label(label) {
                    return Err(err(n, format!("`{label}` is not a label")));
                }
                if !labels.insert((*label).to_owned()) {
                    return Err(err(n, format!("a second `{label}`")));
                }
                let party = |p: &str| {
                    party_index
                        .get(p)
                        .copied()
                        .ok_or_else(|| err(n, format!("`{p}` is not a party declared before")))
                };
                let (debtor, creditor) = (party(debtor)?, party(creditor)?);
                if debtor == creditor {
                    return Err(err(n, "a party does not owe itself"));
                }
                let (amount, code) = money_text(&format!("{amount} {code}"))
                    .ok_or_else(|| err(n, "the amount is `<minor units> <currency>`"))?;
                if amount <= 0 {
                    return Err(err(n, "an obligation owes more than 0"));
                }
                if !cur.knows(&code) {
                    return Err(err(n, format!("the currency table does not know {code}")));
                }
                match &currency {
                    Some(c) if *c != code => {
                        return Err(err(
                            n,
                            format!("the obligations of a contract are in one currency ({c})"),
                        ))
                    }
                    Some(_) => {}
                    None => currency = Some(code),
                }
                obligations.push(ContractObligation {
                    label: (*label).to_owned(),
                    debtor,
                    creditor,
                    amount,
                    due: statute_time(n, due)?,
                });
            }
            h => return Err(err(n, format!("`{h}` is not a line of a contract law"))),
        }
    }
    let created = created.ok_or_else(|| err(0, "a contract has one `created <time>` line"))?;
    let currency = currency.ok_or_else(|| err(0, "a contract has at least one obligation"))?;
    if let Some(o) = obligations.iter().find(|o| o.due < created) {
        return Err(err(
            0,
            format!(
                "obligation `{}` is due before the contract is made",
                o.label
            ),
        ));
    }
    Ok(ContractLaw {
        edition,
        currencies: cur.clone(),
        parties,
        created,
        currency,
        conditions,
        obligations,
    })
}

/// The measured value of every pin of [`super::CONTRACT_SEMANTICS_PINS`]
///
/// The first is the digest of the answers of a fixed contract law, read with its currency table,
/// to fixed requests (before, at and after a due time, fulfilled obligations, conditions met,
/// labels that are not the law's or given twice, a mistyped and a missing `now`).
#[must_use]
pub fn contract_semantics_fingerprints() -> Vec<(&'static str, [u8; 32])> {
    vec![
        (super::CONTRACT_SEMANTICS_PINS[0].0, contract_fingerprint()),
        (
            super::CONTRACT_SEMANTICS_PINS[1].0,
            super::MONEY_SEMANTICS_ID,
        ),
    ]
}

fn contract_fingerprint() -> [u8; 32] {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let table = super::rule_law_from_file(
        "kind table\nx-input code text\nvalue e0 e2 unknown\nrow jpy: e0 when code == \"JPY\"\n\
         row usd: e2 when code == \"USD\"\notherwise unknown\n",
    )
    .expect("the fingerprint currency table reads");
    let id = table.law_id().iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    });
    let law = super::rule_law_from_file_with(
        &format!(
            "kind contract\ncurrencies {id}\nparty buyer\nparty seller\ncreated 10\n\
             condition inspected decided by {}: the goods were inspected\n\
             obligation deposit buyer to seller 100 JPY due 20\n\
             obligation price buyer to seller 9223372036854775900 JPY due 30\n",
            "ab".repeat(32)
        ),
        Some(&table),
    )
    .expect("the fingerprint contract reads");
    let mut h = Sha256::new();
    for r in [
        r#"{"now": 15}"#,
        r#"{"now": 20}"#,
        r#"{"now": 21}"#,
        r#"{"now": 21, "fulfilled": ["deposit"]}"#,
        r#"{"now": 31, "fulfilled": ["deposit"]}"#,
        r#"{"now": 99, "fulfilled": ["price", "deposit"], "met": ["inspected"]}"#,
        r#"{"now": -5}"#,
        r#"{"now": 15, "fulfilled": ["deposit", "deposit"]}"#,
        r#"{"now": 15, "fulfilled": ["buyer"]}"#,
        r#"{"now": 15, "met": "inspected"}"#,
        r#"{"now": "15"}"#,
        r#"{"fulfilled": []}"#,
    ] {
        let a = law.respond(&crate::law_input::parse_json(r).expect("fingerprint JSON"));
        h.update(format!("{r} -> {a:?}\n").as_bytes());
    }
    h.finalize().into()
}
