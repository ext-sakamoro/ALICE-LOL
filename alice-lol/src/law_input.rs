//! 入力の読み方 — 型を持つ `x-input` と、request の JSON の読み.
//!
//! law file の `x-input <名前> <型>` が書く型と、その型に値が合うかを LOL 自身が決める
//! 実装する側 (どの言語でも) はこの規則に揃える
//!
//! ```text
//! type  := number | integer | text | list of <type> | record(<field>, <field>, ...)
//! field := <name>: <type>  |  <name>: optional <type>
//! ```
//!
//! - `number` は解析後に有限な数、`integer` はその内で整数値のもの (`3` と `3.0`)
//! - `optional` の field は欠けてよい 有る field は型に合わなければならない (`null` は欠けていない)
//! - ⚠️ **型に合わない値は入力全体が合わない** list の要素 1 つ、record の field 1 つで足りる
//!   監査ではその入力は測られていない、量の法則では拒否 ([`read`])
//!
//! JSON の数は `f64` として読み、double に収まらない literal (`1e400`) は無限大になる
//! (多くの言語の parser と同じ) 無限大は `number` に合わないので、上の規則で扱われる

use std::fmt;

use sha2::{Digest, Sha256};

/// JSON の値 object の key は書かれた順に持ち、同じ key が 2 度あれば後の方を読む
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    /// `null`
    Null,
    /// `true` / `false`
    Bool(bool),
    /// 数 (`f64`、桁あふれは ±∞)
    Number(f64),
    /// 文字列
    Text(String),
    /// 配列
    Array(Vec<Self>),
    /// object (key と値の組、書かれた順)
    Object(Vec<(String, Self)>),
}

impl Json {
    /// object の key の値 (同じ key が複数あれば最後のもの) object でなければ `None`
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(kv) => kv.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// JSON text を読めなかった理由
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonError {
    /// 読めなかった位置 (byte)
    pub at: usize,
    /// 理由
    pub reason: &'static str,
}

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JSON: {} at byte {}", self.reason, self.at)
    }
}

impl std::error::Error for JsonError {}

/// JSON text を読む (RFC 8259、前後の空白以外の余りは誤り)
///
/// # Errors
///
/// 文法に合わない時
pub fn parse_json(text: &str) -> Result<Json, JsonError> {
    let mut p = JsonParser {
        s: text.as_bytes(),
        i: 0,
        depth: 0,
    };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i == p.s.len() {
        Ok(v)
    } else {
        Err(p.err("text after the value"))
    }
}

/// 入れ子の上限 (深い入力で stack を使い切らないため)
const MAX_DEPTH: usize = 512;

struct JsonParser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
}

impl JsonParser<'_> {
    const fn err(&self, reason: &'static str) -> JsonError {
        JsonError { at: self.i, reason }
    }

    fn ws(&mut self) {
        while matches!(self.s.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn lit(&mut self, word: &[u8], v: Json) -> Result<Json, JsonError> {
        if self.s[self.i..].starts_with(word) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(self.err("unknown literal"))
        }
    }

    fn value(&mut self) -> Result<Json, JsonError> {
        match self.s.get(self.i) {
            None => Err(self.err("unexpected end")),
            Some(b'n') => self.lit(b"null", Json::Null),
            Some(b't') => self.lit(b"true", Json::Bool(true)),
            Some(b'f') => self.lit(b"false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::Text),
            Some(b'[') => self.nested(Self::array),
            Some(b'{') => self.nested(Self::object),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("unexpected character")),
        }
    }

    fn nested(&mut self, f: fn(&mut Self) -> Result<Json, JsonError>) -> Result<Json, JsonError> {
        if self.depth >= MAX_DEPTH {
            return Err(self.err("nested too deeply"));
        }
        self.depth += 1;
        let v = f(self);
        self.depth -= 1;
        v
    }

    fn array(&mut self) -> Result<Json, JsonError> {
        self.i += 1;
        let mut out = Vec::new();
        self.ws();
        if self.s.get(self.i) == Some(&b']') {
            self.i += 1;
            return Ok(Json::Array(out));
        }
        loop {
            self.ws();
            out.push(self.value()?);
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Array(out));
                }
                _ => return Err(self.err("`,` or `]` expected")),
            }
        }
    }

    fn object(&mut self) -> Result<Json, JsonError> {
        self.i += 1;
        let mut out = Vec::new();
        self.ws();
        if self.s.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Ok(Json::Object(out));
        }
        loop {
            self.ws();
            if self.s.get(self.i) != Some(&b'"') {
                return Err(self.err("key expected"));
            }
            let k = self.string()?;
            self.ws();
            if self.s.get(self.i) != Some(&b':') {
                return Err(self.err("`:` expected"));
            }
            self.i += 1;
            self.ws();
            out.push((k, self.value()?));
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Object(out));
                }
                _ => return Err(self.err("`,` or `}` expected")),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, JsonError> {
        let h = self
            .s
            .get(self.i..self.i + 4)
            .ok_or_else(|| self.err("short \\u escape"))?;
        let text = std::str::from_utf8(h).map_err(|_| self.err("bad \\u escape"))?;
        let v = u32::from_str_radix(text, 16).map_err(|_| self.err("bad \\u escape"))?;
        self.i += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, JsonError> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let start = self.i;
            while let Some(&c) = self.s.get(self.i) {
                if c == b'"' || c == b'\\' || c < 0x20 {
                    break;
                }
                self.i += 1;
            }
            out.push_str(
                std::str::from_utf8(&self.s[start..self.i]).map_err(|_| JsonError {
                    at: start,
                    reason: "not UTF-8",
                })?,
            );
            match self.s.get(self.i) {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let e = *self
                        .s
                        .get(self.i)
                        .ok_or_else(|| self.err("unterminated string"))?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                if !self.s[self.i..].starts_with(b"\\u") {
                                    return Err(self.err("lone surrogate"));
                                }
                                self.i += 2;
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(self.err("lone surrogate"));
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                hi
                            };
                            out.push(char::from_u32(cp).ok_or_else(|| self.err("lone surrogate"))?);
                        }
                        _ => return Err(self.err("unknown escape")),
                    }
                }
                Some(_) => return Err(self.err("control character in string")),
            }
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.i;
        while matches!(self.s.get(self.i), Some(b'0'..=b'9')) {
            self.i += 1;
        }
        self.i - start
    }

    fn number(&mut self) -> Result<Json, JsonError> {
        let start = self.i;
        if self.s.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        match self.s.get(self.i) {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(self.err("digit expected")),
        }
        if self.s.get(self.i) == Some(&b'.') {
            self.i += 1;
            if self.digits() == 0 {
                return Err(self.err("digit expected after `.`"));
            }
        }
        if matches!(self.s.get(self.i), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.s.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if self.digits() == 0 {
                return Err(self.err("digit expected in exponent"));
            }
        }
        // ⚠️ `f64::from_str` は桁あふれを ±∞ にする (JSON の数は double として読む)
        let text =
            std::str::from_utf8(&self.s[start..self.i]).map_err(|_| self.err("bad number"))?;
        text.parse::<f64>()
            .map(Json::Number)
            .map_err(|_| self.err("bad number"))
    }
}

/// `x-input` の型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputType {
    /// 有限な数
    Number,
    /// 有限で整数値の数
    Integer,
    /// 文字列
    Text,
    /// 要素がすべてその型の配列
    List(Box<Self>),
    /// field の組 (欠けてよい field は `optional`)
    Record(Vec<Field>),
}

/// record の field
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    /// 名前
    pub name: String,
    /// 欠けてよいか
    pub optional: bool,
    /// 型
    pub ty: InputType,
}

/// 型の text を読めなかった理由
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeError(pub String);

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "x-input type: {}", self.0)
    }
}

impl std::error::Error for TypeError {}

impl InputType {
    /// 型の text を読む (module doc の文法)
    ///
    /// # Errors
    ///
    /// 文法に合わない時、record の field 名が重なる時
    pub fn parse(text: &str) -> Result<Self, TypeError> {
        let (t, rest) = Self::parse_prefix(text.trim())?;
        if rest.trim().is_empty() {
            Ok(t)
        } else {
            Err(TypeError(format!("text after the type: `{}`", rest.trim())))
        }
    }

    fn parse_prefix(s: &str) -> Result<(Self, &str), TypeError> {
        let s = s.trim_start();
        for (word, t) in [
            ("number", Self::Number),
            ("integer", Self::Integer),
            ("text", Self::Text),
        ] {
            if let Some(rest) = s.strip_prefix(word) {
                if !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                    return Ok((t, rest));
                }
            }
        }
        if let Some(rest) = s.strip_prefix("list of ") {
            let (inner, rest) = Self::parse_prefix(rest)?;
            return Ok((Self::List(Box::new(inner)), rest));
        }
        if let Some(mut rest) = s.strip_prefix("record(") {
            let mut fields: Vec<Field> = Vec::new();
            loop {
                let (name, after) = rest
                    .split_once(':')
                    .ok_or_else(|| TypeError(format!("field name expected at `{rest}`")))?;
                let name = name.trim();
                let ok_name = !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                    && !name.starts_with(|c: char| c.is_ascii_digit());
                if !ok_name {
                    return Err(TypeError(format!("bad field name `{name}`")));
                }
                if fields.iter().any(|f| f.name == name) {
                    return Err(TypeError(format!("field `{name}` appears twice")));
                }
                let after = after.trim_start();
                let (optional, after) = after
                    .strip_prefix("optional ")
                    .map_or((false, after), |r| (true, r));
                let (ty, after) = Self::parse_prefix(after)?;
                fields.push(Field {
                    name: name.to_owned(),
                    optional,
                    ty,
                });
                let after = after.trim_start();
                if let Some(r) = after.strip_prefix(',') {
                    rest = r;
                } else if let Some(r) = after.strip_prefix(')') {
                    return Ok((Self::Record(fields), r));
                } else {
                    return Err(TypeError(format!("`,` or `)` expected at `{after}`")));
                }
            }
        }
        Err(TypeError(format!("type expected at `{s}`")))
    }

    /// 正規形の text (空白を詰めた書き方) law の識別子はこの text を hash する
    #[must_use]
    pub fn canonical(&self) -> String {
        match self {
            Self::Number => "number".to_owned(),
            Self::Integer => "integer".to_owned(),
            Self::Text => "text".to_owned(),
            Self::List(t) => format!("list of {}", t.canonical()),
            Self::Record(fs) => {
                let parts: Vec<String> = fs
                    .iter()
                    .map(|f| {
                        format!(
                            "{}: {}{}",
                            f.name,
                            if f.optional { "optional " } else { "" },
                            f.ty.canonical()
                        )
                    })
                    .collect();
                format!("record({})", parts.join(", "))
            }
        }
    }

    /// 値が型に合うか ⚠️ 合わない部分が 1 つでもあれば全体が合わない
    #[must_use]
    pub fn matches(&self, v: &Json) -> bool {
        match (self, v) {
            (Self::Number, Json::Number(x)) => x.is_finite(),
            (Self::Integer, Json::Number(x)) => x.is_finite() && x.fract() == 0.0,
            (Self::Text, Json::Text(_)) => true,
            (Self::List(t), Json::Array(xs)) => xs.iter().all(|x| t.matches(x)),
            (Self::Record(fs), Json::Object(_)) => fs
                .iter()
                .all(|f| v.get(&f.name).map_or(f.optional, |x| f.ty.matches(x))),
            _ => false,
        }
    }
}

/// law の種類 (型に合わない入力の扱いが違う)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LawKind {
    /// 監査: 合わない入力は測られていない
    Audit,
    /// 量の法則: 合わない入力は拒否
    Quantitative,
}

/// 1 つの `x-input` を読んだ結果
#[derive(Debug, Clone, PartialEq)]
pub enum Read<'a> {
    /// 型に合った値
    Value(&'a Json),
    /// 測られていない (欠けている、または監査で型に合わない)
    NotMeasured,
    /// 拒否 (量の法則で型に合わない)
    Rejected(String),
}

/// `x-input` を 1 つ読む 欠けている入力は [`Read::NotMeasured`] (量の法則で欠けた入力を
/// request の誤りとするのは呼び出し側)
#[must_use]
pub fn read<'a>(name: &str, ty: &InputType, value: Option<&'a Json>, kind: LawKind) -> Read<'a> {
    match value {
        None => Read::NotMeasured,
        Some(v) if ty.matches(v) => Read::Value(v),
        Some(_) => match kind {
            LawKind::Audit => Read::NotMeasured,
            LawKind::Quantitative => {
                Read::Rejected(format!("`{name}` does not match `{}`", ty.canonical()))
            }
        },
    }
}

/// law file を監査の Law として読めなかった理由
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LawFileError {
    /// `begin audit` ... `end audit` が無い、または閉じていない
    NoAuditBlock,
    /// audit block が読めない
    Audit(String),
    /// `x-input` の型が読めない (行番号は 1 から)
    InputType {
        /// 行
        line: usize,
        /// 理由
        error: TypeError,
    },
}

impl fmt::Display for LawFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAuditBlock => {
                write!(f, "law file: no closed `begin audit` ... `end audit` block")
            }
            Self::Audit(e) => write!(f, "law file: audit block: {e}"),
            Self::InputType { line, error } => write!(f, "law file: line {line}: {error}"),
        }
    }
}

impl std::error::Error for LawFileError {}

/// law file (`kind audit`) を監査の Law として読む: audit block の項と、`x-input` の行の型
///
/// 型は [`AuditLaw::law_id`](crate::law_id) に入る `#` 以降は注記として読まない
///
/// # Errors
///
/// audit block が無い・読めない時、`x-input` の型が読めない時
pub fn audit_law_from_file(text: &str) -> Result<crate::audit_law::AuditLaw, LawFileError> {
    let mut block: Option<String> = None;
    let mut done: Option<String> = None;
    let mut inputs = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if let Some(b) = block.as_mut() {
            if line == "end audit" {
                done = block.take();
            } else if !line.is_empty() {
                b.push_str(line);
                b.push('\n');
            }
            continue;
        }
        if line == "begin audit" {
            block = Some(String::new());
        } else if let Some(rest) = line.strip_prefix("x-input ") {
            let rest = rest.trim();
            let (name, ty) = rest.split_once(' ').unwrap_or((rest, ""));
            let ty = InputType::parse(ty)
                .map_err(|error| LawFileError::InputType { line: n + 1, error })?;
            inputs.push((name.to_owned(), ty));
        }
    }
    let body = done.ok_or(LawFileError::NoAuditBlock)?;
    let mut law = crate::runtime_parser::parse_law(&body)
        .map_err(|e| LawFileError::Audit(format!("{e:?}")))?;
    for (name, ty) in inputs {
        law = law.with_input(&name, ty);
    }
    Ok(law)
}

/// 値を hash に書く (型の tag と中身、数は bit、文字列と列は長さ付き)
fn encode_json(h: &mut Sha256, v: &Json) {
    match v {
        Json::Null => h.update([0]),
        Json::Bool(b) => h.update([1, u8::from(*b)]),
        Json::Number(x) => {
            h.update([2]);
            h.update(x.to_bits().to_le_bytes());
        }
        Json::Text(t) => {
            h.update([3]);
            h.update((t.len() as u64).to_le_bytes());
            h.update(t.as_bytes());
        }
        Json::Array(xs) => {
            h.update([4]);
            h.update((xs.len() as u64).to_le_bytes());
            for x in xs {
                encode_json(h, x);
            }
        }
        Json::Object(kv) => {
            h.update([5]);
            h.update((kv.len() as u64).to_le_bytes());
            for (k, x) in kv {
                h.update((k.len() as u64).to_le_bytes());
                h.update(k.as_bytes());
                encode_json(h, x);
            }
        }
    }
}

/// 入力の読み方の指紋の case: `(型, request の inputs の JSON text, law の種類)`
///
/// inputs の key `v` を読む 型の照合だけでなく、JSON の読み (同じ key は後の方 / 桁あふれは
/// ±∞ / `NaN` `Infinity` は誤り / 入れ子は 512 段まで / escape / 孤立した surrogate は誤り)
/// と、監査と量の法則の扱いの違いを含む
fn input_reading_cases() -> Vec<(&'static str, String, LawKind)> {
    use LawKind::{Audit, Quantitative};
    const BUILDS: &str = "list of record(features: list of text, id: optional text)";
    let v = |value: &str| format!("{{\"v\":{value}}}");
    let mut cases: Vec<(&'static str, String, LawKind)> = [
        // identifier の判定が変わった 5 件と、その周り
        (
            BUILDS,
            r#"[{"features":["a"],"id":"x"},{"features":["b"]}]"#,
        ),
        (BUILDS, r#"[{"id":"x"},{"features":["b"],"id":"x"}]"#),
        (
            BUILDS,
            r#"[{"features":"a","id":"x"},{"features":["b"],"id":"x"}]"#,
        ),
        (BUILDS, r#"[7,{"features":["b"],"id":"x"}]"#),
        (
            BUILDS,
            r#"[{"features":["a",3],"id":"x"},{"features":["b"],"id":"x"}]"#,
        ),
        (
            BUILDS,
            r#"[{"features":["a"],"id":5},{"features":["b"],"id":5}]"#,
        ),
        (
            BUILDS,
            r#"[{"features":["a"],"id":null},{"features":["b"],"id":"x"}]"#,
        ),
        (BUILDS, "[]"),
        (BUILDS, r#""std""#),
        ("list of text", r#"["case-17","case-42"]"#),
        ("list of text", r#"["case-17",42]"#),
        ("list of text", "null"),
        ("number", "1e400"),
        ("number", "-1e400"),
        ("number", "true"),
        ("integer", "3.0"),
        ("integer", "2.5"),
        // escape と surrogate
        ("text", r#""\u0063ase-17\/\n""#),
        ("text", r#""\ud83d\ude00""#),
        ("text", r#""\ud800""#),
        // JSON に無い literal
        ("number", "Infinity"),
        ("number", "NaN"),
    ]
    .iter()
    .map(|(t, value)| (*t, v(value), Audit))
    .collect();
    // 400 桁の整数は ±∞ (数として読み、double に収まらない)
    cases.push(("number", v(&format!("1{}", "0".repeat(399))), Audit));
    // 同じ key は後の方を読む
    cases.push(("text", r#"{"v":1,"v":"x"}"#.to_owned(), Audit));
    cases.push(("text", r#"{"v":"x","v":1}"#.to_owned(), Audit));
    // 型に合わない値: 監査は測られていない、量の法則は拒否
    for kind in [Audit, Quantitative] {
        cases.push(("number", v(r#""2.5""#), kind));
        cases.push(("number", v("2.5"), kind));
    }
    // 入れ子: request の object を 1 段目として 512 段までは読む、513 段は誤り
    for depth in [MAX_DEPTH, MAX_DEPTH + 1] {
        let inner = depth - 1;
        cases.push(("text", v(&("[".repeat(inner) + &"]".repeat(inner))), Audit));
    }
    cases
}

/// 入力の読み方の指紋 — **振る舞いから計算する**
///
/// 固定の case ([`input_reading_cases`]) を request と同じ経路 ([`parse_json`] → key の
/// 値 → [`read`]) で読み、読めたか・どう読んだか (読んだ値そのもの) を hash する
/// ⚠️ 読み方 (入力全体で判定する / `null` は欠けていない / ∞ は数でない / 同じ key は
/// 後の方 / 監査は測られていない・量の法則は拒否 / 入れ子の上限 / escape) のどれを
/// 変えても結果の並びが変わり、この値も変わる
///
/// # Panics
///
/// 固定の型が読めない時 (型は定数なので起きない、試験が確かめる)
#[must_use]
pub fn input_reading_fingerprint() -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"lol.input-reading.2");
    for (ty, text, kind) in input_reading_cases() {
        let t = InputType::parse(ty).expect("fingerprint type parses");
        let canon = t.canonical();
        h.update((canon.len() as u64).to_le_bytes());
        h.update(canon.as_bytes());
        h.update((text.len() as u64).to_le_bytes());
        h.update(text.as_bytes());
        h.update([u8::from(kind == LawKind::Quantitative)]);
        match parse_json(&text) {
            Err(_) => h.update([0]),
            Ok(req) => match read("v", &t, req.get("v"), kind) {
                Read::Value(x) => {
                    h.update([1]);
                    encode_json(&mut h, x);
                }
                Read::NotMeasured => h.update([2]),
                Read::Rejected(_) => h.update([3]),
            },
        }
    }
    h.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILDS: &str = "list of record(features: list of text, id: optional text)";

    fn ty(s: &str) -> InputType {
        InputType::parse(s).expect("type")
    }

    fn js(s: &str) -> Json {
        parse_json(s).expect("json")
    }

    #[test]
    fn one_bad_part_makes_the_whole_input_not_match() {
        let b = ty(BUILDS);
        assert!(b.matches(&js(r#"[{"features":["a"],"id":"x"},{"features":[]}]"#)));
        for bad in [
            r#"[{"features":["a"],"id":"x"},7]"#,
            r#"[{"features":["a"],"id":"x"},{"id":"x"}]"#,
            r#"[{"features":["a"],"id":"x"},{"features":"b"}]"#,
            r#"[{"features":["a"],"id":"x"},{"features":["b",3]}]"#,
            r#"[{"features":["a"],"id":"x"},{"features":["b"],"id":5}]"#,
            r#""std""#,
        ] {
            assert!(!b.matches(&js(bad)), "{bad}");
        }
    }

    #[test]
    fn null_is_not_absent() {
        let b = ty(BUILDS);
        assert!(b.matches(&js(r#"[{"features":["a"]}]"#)));
        assert!(!b.matches(&js(r#"[{"features":["a"],"id":null}]"#)));
    }

    #[test]
    fn audit_and_quantitative_reads_differ_only_for_a_bad_value() {
        let t = ty("number");
        let good = js("2.5");
        let bad = js("\"2.5\"");
        assert_eq!(
            read("x", &t, Some(&good), LawKind::Audit),
            Read::Value(&good)
        );
        assert_eq!(
            read("x", &t, Some(&good), LawKind::Quantitative),
            Read::Value(&good)
        );
        assert_eq!(read("x", &t, Some(&bad), LawKind::Audit), Read::NotMeasured);
        assert!(matches!(
            read("x", &t, Some(&bad), LawKind::Quantitative),
            Read::Rejected(_)
        ));
        assert_eq!(
            read("x", &t, None, LawKind::Quantitative),
            Read::NotMeasured
        );
    }

    #[test]
    fn the_canonical_text_reads_back_to_the_same_type() {
        for s in [
            BUILDS,
            "list of list of integer",
            "record( a:text ,b:  optional number )",
        ] {
            let t = ty(s);
            assert_eq!(ty(&t.canonical()), t, "{s}");
        }
        assert_eq!(
            ty("record( a:text ,b:  optional number )").canonical(),
            "record(a: text, b: optional number)"
        );
    }

    #[test]
    fn malformed_types_are_rejected() {
        for bad in [
            "",
            "texts",
            "list text",
            "record(a text)",
            "record(a: text",
            "record(a: text, a: text)",
            "record()",
            "text text",
            "list of",
            "record(: text)",
        ] {
            assert!(InputType::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn json_edge_cases() {
        assert_eq!(js(r#"{"a":1,"a":2}"#).get("a"), Some(&Json::Number(2.0)));
        assert_eq!(js(r#""\ud83d\ude00""#), Json::Text("\u{1f600}".to_owned()));
        assert_eq!(js(" [ ] "), Json::Array(Vec::new()));
        for bad in [
            "",
            "01",
            "1.",
            "-",
            "[1,]",
            "{\"a\"}",
            "\"\\ud800\"",
            "nul",
            "[1] 2",
            "\"a\u{1}\"",
        ] {
            assert!(parse_json(bad).is_err(), "{bad:?}");
        }
        let deep = "[".repeat(MAX_DEPTH + 1) + &"]".repeat(MAX_DEPTH + 1);
        assert!(parse_json(&deep).is_err());
    }

    #[test]
    fn each_fingerprint_case_reads_as_decided() {
        // 期待は TASK.md の規則から手で書いた (読み方を変えるとここと pin の両方が red)
        let expected = [
            "value", "not", "not", "not", "not", "not", "not", "value", "not", // builds
            "value", "not", "not", // list of text
            "not", "not", "not", "value", "not", // number / integer
            "value", "value", "error", // escape / surrogate pair / lone surrogate
            "error", "error", // Infinity / NaN literal
            "not",   // 400-digit integer is infinite
            "value", "not", // duplicate key: the last one
            "not", "value", "rejected",
            "value", // audit / quantitative on a bad and a good value
            "not", "error", // depth 512 is read (and a list is not text), 513 is an error
        ];
        let got: Vec<&str> = input_reading_cases()
            .into_iter()
            .map(|(ty, text, kind)| {
                let t = InputType::parse(ty).expect("type");
                parse_json(&text).map_or("error", |req| match read("v", &t, req.get("v"), kind) {
                    Read::Value(_) => "value",
                    Read::NotMeasured => "not",
                    Read::Rejected(_) => "rejected",
                })
            })
            .collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn the_fingerprint_cases_all_parse() {
        // panics inside input_reading_fingerprint otherwise
        let _ = input_reading_fingerprint();
    }

    #[test]
    fn overflowing_literals_are_infinite_and_not_numbers() {
        assert_eq!(parse_json("1e400"), Ok(Json::Number(f64::INFINITY)));
        assert_eq!(parse_json("-1e400"), Ok(Json::Number(f64::NEG_INFINITY)));
        assert!(!InputType::Number.matches(&Json::Number(f64::INFINITY)));
    }
}
