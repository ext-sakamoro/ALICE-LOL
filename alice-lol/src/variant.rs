//! Variants of a part law: a knob table read from a `kind proportion` law, a canonical integer
//! encoding of a knob vector, a variant identifier, and a seeded sampler over the whole declared box.
//!
//! A knob is a parameter of the knob law. Its value is `default + q · step` with an integer `q` and
//! `step = (hi − lo) / 2^16`, so `q = 0` is the default exactly and a variant with every `q = 0` is
//! its base bit for bit. The bounds of `q` are taken so that both extreme values, after `f64`
//! rounding, stay inside `[lo, hi]` (the binder never sees a value an ulp outside). The values use
//! one multiplication and one addition each (no transcendental function), so they are the same on
//! every machine.
//!
//! The members of an unordered group (`group` lines of the law) are interchangeable: a variant is
//! put in canonical form (the members sorted by their integers) before its identifier is taken and
//! before any gate sees it, so a permuted input is the same variant.
//!
//! A variant names its base as neutral or reference. The sampler draws only from a neutral base:
//! distributed variants carry nothing of a reference body. A reference base can still be named (a
//! local variant of it), and its identifiers differ from a neutral base's with the same bytes.
//!
//! The sampler draws each knob uniformly over its whole range ([`Rng::uniform`], exact), knobs in the
//! law's declared order, one stream per seed and knob law, and keeps a variant only when every gate
//! passes; no region is left out. Gates are the caller's (derived geometry, seams, poses); each must
//! report how much it compared, and a gate that compared nothing stops the sampling.
use crate::law_id::LawIdHasher;
use crate::proportion_law::{BoundProportion, ProportionLaw, PROPORTION_SEMANTICS_ID};
use crate::rng::Rng;

/// The kind of a variant identifier
pub const VARIANT_KIND: &[u8] = b"lol.variant/v1";
/// Steps of a knob across its range (`step = (hi − lo) / KNOB_STEPS`)
pub const KNOB_STEPS: f64 = 65_536.0;

/// A knob: a parameter of the knob law and the integer range of its steps
#[derive(Debug, Clone, PartialEq)]
pub struct Knob {
    pub name: String,
    pub default: f64,
    pub lo: f64,
    pub hi: f64,
    /// the value of one step
    pub step: f64,
    /// the smallest and the largest step count (both values inside `[lo, hi]`)
    pub q_min: i64,
    pub q_max: i64,
}

impl Knob {
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a step count is within ±2^16"
    )]
    fn of(name: &str, default: f64, lo: f64, hi: f64) -> Self {
        let step = (hi - lo) / KNOB_STEPS;
        let (mut q_min, mut q_max) = if step > 0.0 {
            (
                ((lo - default) / step).ceil() as i64,
                ((hi - default) / step).floor() as i64,
            )
        } else {
            (0, 0)
        };
        let k = |q: i64| Self::value_of(default, step, q);
        // f64 rounding can put an extreme an ulp outside the range: step back in
        while q_max > 0 && k(q_max) > hi {
            q_max -= 1;
        }
        while q_min < 0 && k(q_min) < lo {
            q_min += 1;
        }
        Self {
            name: name.to_owned(),
            default,
            lo,
            hi,
            step,
            q_min,
            q_max,
        }
    }

    #[allow(
        clippy::cast_precision_loss,
        reason = "a step count is within ±2^16, exact in f64"
    )]
    #[allow(
        clippy::suboptimal_flops,
        reason = "unfused: one rounding of the product, one of the sum"
    )]
    fn value_of(default: f64, step: f64, q: i64) -> f64 {
        default + q as f64 * step
    }

    /// The value at `q` steps from the default
    #[must_use]
    pub fn value(&self, q: i64) -> f64 {
        Self::value_of(self.default, self.step, q)
    }
}

/// The base a variant departs from
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    /// a base of generic proportions: the one distributed variants and the sampler use
    Neutral([u8; 32]),
    /// a base fitted to a reference body: local variants only, never sampled
    Reference([u8; 32]),
}

impl Base {
    const fn role(self) -> (u32, [u8; 32]) {
        match self {
            Self::Neutral(id) => (0, id),
            Self::Reference(id) => (1, id),
        }
    }
}

/// A variant: the knob law, the base, every knob's step count in canonical form, and the part laws
/// it is built with
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    /// the knob law's identifier
    pub knob_law: [u8; 32],
    pub base: Base,
    /// one step count per knob, in the law's declared order, groups canonical
    pub q: Vec<i64>,
    /// the identifiers of the part laws the variant is built with, in the caller's order
    pub parts: Vec<[u8; 32]>,
}

impl Variant {
    /// The variant identifier: the knob law, the base and its role, the step counts and the part laws
    #[must_use]
    pub fn id(&self) -> [u8; 32] {
        let (role, base) = self.base.role();
        let mut h = LawIdHasher::new(VARIANT_KIND, &PROPORTION_SEMANTICS_ID)
            .bytes(&self.knob_law)
            .bytes(&base)
            .u32(role)
            .u64(self.q.len() as u64);
        for q in &self.q {
            h = h.u64(u64::from_ne_bytes(q.to_ne_bytes()));
        }
        h = h.u64(self.parts.len() as u64);
        for p in &self.parts {
            h = h.bytes(p);
        }
        h.finish()
    }
}

/// The knobs of a knob law (its parameters, in declared order) and its unordered groups
#[derive(Debug, Clone)]
pub struct KnobTable<'l> {
    law: &'l ProportionLaw,
    knobs: Vec<Knob>,
    /// per group, per member, the knobs' indices
    groups: Vec<Vec<Vec<usize>>>,
}

impl<'l> KnobTable<'l> {
    /// The knob table of a `kind proportion` law: every parameter is a knob
    #[must_use]
    pub fn of(law: &'l ProportionLaw) -> Self {
        let knobs: Vec<Knob> = law
            .params()
            .iter()
            .map(|p| Knob::of(&p.name, p.default, p.lo, p.hi))
            .collect();
        // the law's parser has checked that every group names declared parameters
        let index = |n: &str| knobs.iter().position(|k| k.name == n);
        let groups = law
            .groups()
            .iter()
            .map(|g| {
                g.members
                    .iter()
                    .map(|m| m.iter().filter_map(|n| index(n)).collect())
                    .collect()
            })
            .collect();
        Self { law, knobs, groups }
    }

    /// The knobs, in declared order
    #[must_use]
    pub fn knobs(&self) -> &[Knob] {
        &self.knobs
    }

    /// The members of every group sorted by their step counts (lexicographically, member by member)
    fn canonical(&self, q: &mut [i64]) {
        for members in &self.groups {
            let mut tuples: Vec<Vec<i64>> = members
                .iter()
                .map(|m| m.iter().map(|&i| q[i]).collect())
                .collect();
            tuples.sort();
            for (m, t) in members.iter().zip(tuples) {
                for (&i, v) in m.iter().zip(t) {
                    q[i] = v;
                }
            }
        }
    }

    /// A variant from named step counts (any order; a knob not named stays at 0, its default)
    ///
    /// # Errors
    ///
    /// A name that is not a knob, a knob named twice, or a step count outside the knob's range.
    pub fn variant(
        &self,
        base: Base,
        knobs: &[(&str, i64)],
        parts: &[[u8; 32]],
    ) -> Result<Variant, String> {
        let mut q = vec![0i64; self.knobs.len()];
        let mut seen = vec![false; self.knobs.len()];
        for (name, v) in knobs {
            let i = self
                .knobs
                .iter()
                .position(|k| k.name == *name)
                .ok_or_else(|| format!("`{name}` is not a knob of {}", self.law.name()))?;
            if std::mem::replace(&mut seen[i], true) {
                return Err(format!("the knob `{name}` is given twice"));
            }
            let k = &self.knobs[i];
            if *v < k.q_min || *v > k.q_max {
                return Err(format!(
                    "`{name}` = {v} steps is outside [{}, {}]",
                    k.q_min, k.q_max
                ));
            }
            q[i] = *v;
        }
        Ok(self.of_steps(base, q, parts))
    }

    fn of_steps(&self, base: Base, mut q: Vec<i64>, parts: &[[u8; 32]]) -> Variant {
        self.canonical(&mut q);
        Variant {
            knob_law: self.law.law_id(),
            base,
            q,
            parts: parts.to_vec(),
        }
    }

    /// The knob law bound with a variant's values (every knob at `default + q · step`)
    ///
    /// # Errors
    ///
    /// A variant of another knob law or of another length, or the binder's refusal (a `require` line).
    pub fn bind(&self, v: &Variant) -> Result<BoundProportion<'l>, String> {
        if v.knob_law != self.law.law_id() || v.q.len() != self.knobs.len() {
            return Err("the variant is of another knob law".into());
        }
        let values: Vec<(&str, f64)> = self
            .knobs
            .iter()
            .zip(&v.q)
            .map(|(k, q)| (k.name.as_str(), k.value(*q)))
            .collect();
        self.law.bind(&values).map_err(|e| e.to_string())
    }
}

/// What one gate compared and how much of it failed
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateResult {
    pub gate: String,
    pub compared: u64,
    pub failed: u64,
}

/// The gates' results for one variant
pub type GateReport = Vec<GateResult>;

/// What a sampling run did: the trials drawn, the variants kept, and per gate the rejections
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleReport {
    pub trials: u64,
    pub accepted: u64,
    /// per gate (in the order the gates first reported): (name, compared over all trials, trials it rejected)
    pub gates: Vec<(String, u64, u64)>,
}

impl SampleReport {
    /// The accepted share of the trials
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "counts of trials")]
    pub fn acceptance(&self) -> f64 {
        if self.trials == 0 {
            0.0
        } else {
            self.accepted as f64 / self.trials as f64
        }
    }
}

/// Up to `wanted` variants of a neutral base, drawn from `seed` over every knob's whole range, kept
/// when `gates` reports no failure; at most `max_trials` draws
///
/// # Errors
///
/// A reference base (distributed variants come only from a neutral base), or a gate that reports
/// it compared nothing (a gate that compared nothing has not judged the variant).
pub fn sample(
    table: &KnobTable,
    base: Base,
    seed: u64,
    wanted: u64,
    max_trials: u64,
    parts: &[[u8; 32]],
    gates: &mut dyn FnMut(&Variant) -> GateReport,
) -> Result<(Vec<Variant>, SampleReport), String> {
    if matches!(base, Base::Reference(_)) {
        return Err(
            "a reference base is not sampled: distributed variants come only from a neutral base"
                .into(),
        );
    }
    let law = table.law.law_id();
    // the seed and the knob law mixed (SplitMix64's finaliser) before the stream: Rng::new alone maps
    // seeds 2k and 2k + 1 to the same stream
    let law_bits = u64::from_le_bytes([
        law[0], law[1], law[2], law[3], law[4], law[5], law[6], law[7],
    ]);
    let mut rng = Rng::new(mix(seed ^ law_bits));
    let mut report = SampleReport {
        trials: 0,
        accepted: 0,
        gates: Vec::new(),
    };
    let mut out = Vec::new();
    while report.accepted < wanted && report.trials < max_trials {
        report.trials += 1;
        let q: Vec<i64> = table
            .knobs
            .iter()
            .map(|k| rng.uniform(k.q_min, k.q_max))
            .collect();
        let v = table.of_steps(base, q, parts);
        let mut pass = true;
        for g in gates(&v) {
            if g.compared == 0 {
                return Err(format!(
                    "the gate `{}` compared nothing (trial {})",
                    g.gate, report.trials
                ));
            }
            let i = report
                .gates
                .iter()
                .position(|(n, _, _)| *n == g.gate)
                .unwrap_or_else(|| {
                    report.gates.push((g.gate.clone(), 0, 0));
                    report.gates.len() - 1
                });
            report.gates[i].1 += g.compared;
            if g.failed > 0 {
                report.gates[i].2 += 1;
                pass = false;
            }
        }
        if pass {
            report.accepted += 1;
            out.push(v);
        }
    }
    Ok((out, report))
}

/// `SplitMix64`'s finaliser: every bit of the input moves every bit of the output
const fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// How evenly a set of variants covers one knob's whole range
#[derive(Debug, Clone, PartialEq)]
pub struct Uniformity {
    /// the variants compared
    pub compared: u64,
    pub bins: u32,
    /// Pearson's χ² against the share of the range's steps in each bin
    pub chi_square: f64,
    /// degrees of freedom (`bins − 1`)
    pub dof: u32,
}

/// χ² of a knob's step counts over `bins` bins of its range
///
/// # Errors
///
/// No variant to compare, a knob that is not in the table, fewer than 2 bins, or more bins than steps.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "counts and step indices"
)]
#[allow(
    clippy::many_single_char_names,
    reason = "χ² terms: i, k, s, b, n, o, e, d"
)]
pub fn uniformity(
    table: &KnobTable,
    variants: &[Variant],
    knob: &str,
    bins: u32,
) -> Result<Uniformity, String> {
    if variants.is_empty() {
        return Err("no variant to compare (compared 0)".into());
    }
    let i = table
        .knobs
        .iter()
        .position(|k| k.name == knob)
        .ok_or_else(|| format!("`{knob}` is not a knob"))?;
    let k = &table.knobs[i];
    let span = (k.q_max - k.q_min + 1) as u64;
    if bins < 2 || u64::from(bins) > span {
        return Err(format!("{bins} bins for {span} steps"));
    }
    // bin b holds the steps [b·span/bins, (b+1)·span/bins): its expected share is its width / span
    let edge = |b: u64| b * span / u64::from(bins);
    let mut observed = vec![0u64; bins as usize];
    for v in variants {
        let s = (v.q[i] - k.q_min) as u64;
        let b = ((s + 1) * u64::from(bins) - 1) / span;
        // the integer division can land one bin high at an edge: step down to the bin holding s
        let b = if s < edge(b) { b - 1 } else { b };
        observed[b as usize] += 1;
    }
    let n = variants.len() as f64;
    let mut chi = 0.0;
    for (b, &o) in observed.iter().enumerate() {
        let width = (edge(b as u64 + 1) - edge(b as u64)) as f64;
        let e = n * width / span as f64;
        let d = o as f64 - e;
        chi += d * d / e;
    }
    Ok(Uniformity {
        compared: variants.len() as u64,
        bins,
        chi_square: chi,
        dof: bins - 1,
    })
}
