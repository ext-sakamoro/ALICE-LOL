//! 監査の Law — 検査を「何が成立すべきか」として書く.
//!
//! 上の [`law`](crate::law) (幾何の制約) と [`research_law`](crate::research_law)
//! (データに対する式) と並ぶ 3 つ目の Law 一致を検査する対象が **測定そのもの**で、
//! 「検査器が何件比べたか」「既知の違反がどこまでか」を主張する
//!
//! 3 つを分ける:
//!
//! 1. **Law** ([`AuditLaw`]) — text で書かれた主張 ⚠️ **測り方を一切書かない**
//! 2. **実測** ([`Measurements`]) — 測った側が渡す値 どう測ったかは測る側の自由で、
//!    Rust でも別の言語でも同じ Law に食わせられる
//! 3. **判定** ([`Verdict`]) — 6 値
//!
//! 真偽の 2 値では「測れていない」と「違反した」が同じ失敗になり、「判定不能」を
//! 表現できない 6 値は [`research_law::ResearchVerdict`](crate::research_law::ResearchVerdict)
//! と同じ考え方で、証拠に対する判定を分ける
//!
//! # 書式
//!
//! ```
//! use alice_lol::audit_law::{Measurements, Verdict};
//! use alice_lol::runtime_parser::parse_law;
//!
//! let law = parse_law(
//!     "audit lock-single-version\n\
//!      evidence packages\n\
//!      expect unbaselined_duplicates == 0\n\
//!      range alice-det-math 0.3.2 0.4.0\n",
//! )?;
//!
//! let measured = Measurements::new()
//!     .with_number("packages", 175.0)
//!     .with_number("unbaselined_duplicates", 0.0)
//!     .with_range("alice-det-math", &["0.3.2", "0.4.0"]);
//!
//! assert_eq!(law.evaluate(&measured), Verdict::Supports);
//! # Ok::<(), alice_lol::runtime_parser::ParseError>(())
//! ```
//!
//! # 判定の型を閉じている理由
//!
//! [`Verdict`] と [`Clause`] に `#[non_exhaustive]` を **付けていない** 判定の値が
//! 増えるのは監査の意味が増えることなので、呼び出し側の `match` が `_ =>` で黙って
//! 吸収してはいけない compile error になって対応を強制するのが正しい

use std::collections::BTreeMap;
use std::fmt;

/// 監査の判定 真偽 2 値では潰れる 4 つを別の値として持つ
///
/// 値を増やすのは破壊的変更として扱う (module doc 参照)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 主張がすべて満たされた
    Supports,
    /// その量が 1 件も測られていない (検査器の空振り)
    NoEvidence {
        /// 測られていなかった量の名前
        metric: String,
    },
    /// 期待値から外れた
    Breaks {
        /// 外れた量の名前
        metric: String,
        /// Law が期待していた値 (許容差つきなら `値 ± 許容`)
        expected: String,
        /// 測られた値
        measured: String,
    },
    /// 成立範囲の行が実測に無い (解消済なのに残っている)
    OutOfRange {
        /// 残っていた行の key
        key: String,
    },
    /// 成立範囲の行の値の組が変わった
    ParameterUpdate {
        /// 変わった行の key
        key: String,
        /// Law に書かれていた組
        was: Vec<String>,
        /// 測られた組
        now: Vec<String>,
    },
    /// 判定に必要な前提が欠けていて判定できない
    Undecided {
        /// なぜ判定できないか
        reason: String,
    },
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Supports => write!(f, "supports"),
            Self::NoEvidence { metric } => {
                write!(f, "no evidence: nothing measured for `{metric}`")
            }
            Self::Breaks {
                metric,
                expected,
                measured,
            } => write!(
                f,
                "breaks: `{metric}` expected {expected}, measured {measured}"
            ),
            Self::OutOfRange { key } => {
                write!(
                    f,
                    "out of range: `{key}` is not in the measurement (drop the row)"
                )
            }
            Self::ParameterUpdate { key, was, now } => write!(
                f,
                "parameter update: `{key}` moved from {} to {}",
                was.join(" "),
                now.join(" ")
            ),
            Self::Undecided { reason } => write!(f, "undecided: {reason}"),
        }
    }
}

impl Verdict {
    /// 監査として通ったか (`Supports` だけが通る)
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self, Self::Supports)
    }
}

/// Law の 1 項
///
/// 値を増やすのは破壊的変更として扱う ([`Verdict`] と同じ理由)
#[derive(Debug, Clone, PartialEq)]
pub enum Clause {
    /// その量が 1 件以上測られていること
    Evidence {
        /// 証拠として要求する量の名前
        metric: String,
    },
    /// 期待値 (許容差つき)
    Expect {
        /// 期待する量の名前
        metric: String,
        /// 期待値
        value: f64,
        /// 許容差 (0 なら厳密一致)
        tolerance: f64,
    },
    /// 既知の違反 (ラチェット)
    Range {
        /// 行の key
        key: String,
        /// 許容する値の組 (昇順に正規化される)
        values: Vec<String>,
    },
}

/// text で書かれた監査の Law
///
/// 構築は [`parse_law`](crate::runtime_parser::parse_law) 経由
#[derive(Debug, Clone, PartialEq)]
pub struct AuditLaw {
    name: String,
    clauses: Vec<Clause>,
}

impl AuditLaw {
    /// 名前と項から組み立てる (parser 用)
    #[must_use]
    pub(crate) const fn new(name: String, clauses: Vec<Clause>) -> Self {
        Self { name, clauses }
    }

    /// Law の名前
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Law の項
    #[must_use]
    pub fn clauses(&self) -> &[Clause] {
        &self.clauses
    }

    /// 実測と突き合わせて判定する 最初に見つかった不成立を返し、すべて満たせば
    /// [`Verdict::Supports`]
    ///
    /// ⚠️ 項の順序には依らず **証拠 → 成立範囲 → 期待値** の順に見る
    /// (測れていない状態で期待値を判定すると `0 == 0` が偶然通るため)
    #[must_use]
    pub fn evaluate(&self, measured: &Measurements) -> Verdict {
        self.check_evidence(measured)
            .or_else(|| self.check_ranges(measured))
            .or_else(|| self.check_expectations(measured))
            .unwrap_or(Verdict::Supports)
    }

    fn check_evidence(&self, m: &Measurements) -> Option<Verdict> {
        self.clauses.iter().find_map(|c| match c {
            Clause::Evidence { metric } if m.number(metric).unwrap_or(0.0) == 0.0 => {
                Some(Verdict::NoEvidence {
                    metric: metric.clone(),
                })
            }
            _ => None,
        })
    }

    fn check_ranges(&self, m: &Measurements) -> Option<Verdict> {
        self.clauses.iter().find_map(|c| {
            let Clause::Range { key, values } = c else {
                return None;
            };
            match m.range(key) {
                None => Some(Verdict::OutOfRange { key: key.clone() }),
                Some(now) if now != values.as_slice() => Some(Verdict::ParameterUpdate {
                    key: key.clone(),
                    was: values.clone(),
                    now: now.to_vec(),
                }),
                Some(_) => None,
            }
        })
    }

    fn check_expectations(&self, m: &Measurements) -> Option<Verdict> {
        self.clauses.iter().find_map(|c| {
            let Clause::Expect {
                metric,
                value,
                tolerance,
            } = c
            else {
                return None;
            };
            let Some(got) = m.number(metric) else {
                return Some(Verdict::Undecided {
                    reason: format!("`{metric}` is not in the measurement"),
                });
            };
            if (got - value).abs() > *tolerance {
                return Some(Verdict::Breaks {
                    metric: metric.clone(),
                    expected: if *tolerance == 0.0 {
                        format!("{value}")
                    } else {
                        format!("{value} ± {tolerance}")
                    },
                    measured: format!("{got}"),
                });
            }
            None
        })
    }
}

/// 値を昇順に正規化する (`0.10` > `0.9`)
#[must_use]
pub(crate) fn sorted_values(values: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = values.iter().map(|s| (*s).to_owned()).collect();
    v.sort_by_key(|s| version_key(s));
    v
}

fn version_key(v: &str) -> Vec<(u8, u64, String)> {
    v.split(['.', '-', '+'])
        .map(|c| {
            c.parse::<u64>()
                .map_or_else(|_| (1, 0, c.to_owned()), |n| (0, n, String::new()))
        })
        .collect()
}

/// 測った側が渡す値 ⚠️ どう測ったかは測る側の自由で、Law はそれを知らない
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Measurements {
    numbers: BTreeMap<String, f64>,
    ranges: BTreeMap<String, Vec<String>>,
}

impl Measurements {
    /// 空の実測
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 数えた量を足す
    #[must_use]
    pub fn with_number(mut self, metric: &str, value: f64) -> Self {
        self.numbers.insert(metric.to_owned(), value);
        self
    }

    /// 測られた組を足す (版の組など) 値は昇順に正規化する
    #[must_use]
    pub fn with_range(mut self, key: &str, values: &[&str]) -> Self {
        self.ranges.insert(key.to_owned(), sorted_values(values));
        self
    }

    /// 数えた量
    #[must_use]
    pub fn number(&self, metric: &str) -> Option<f64> {
        self.numbers.get(metric).copied()
    }

    /// 測られた組
    #[must_use]
    pub fn range(&self, key: &str) -> Option<&[String]> {
        self.ranges.get(key).map(Vec::as_slice)
    }
}
