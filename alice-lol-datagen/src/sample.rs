//! Sample 型 + JSONL 出力 + 自己検証

use alice_lol::intent::{IntentNode, Program};
use alice_lol::runtime_parser::parse_program;
use alice_lol::{eval, Vec3};
use std::fmt::Write as _;

/// 内外
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// SDF < 0
    In,
    /// SDF > 0
    Out,
}

/// oracle 点 (benchmark と同じ判定を data 側にも持たせる)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OraclePoint {
    /// 座標 (mm)
    pub p: Vec3,
    /// 期待
    pub side: Side,
}

impl OraclePoint {
    /// 内側
    #[must_use]
    pub const fn inside(x: f32, y: f32, z: f32) -> Self {
        Self {
            p: Vec3::new(x, y, z),
            side: Side::In,
        }
    }
    /// 外側
    #[must_use]
    pub const fn outside(x: f32, y: f32, z: f32) -> Self {
        Self {
            p: Vec3::new(x, y, z),
            side: Side::Out,
        }
    }
}

/// 1 sample = (caption, LOL) pair + 検証材料
#[derive(Debug, Clone)]
pub struct Sample {
    /// `<family>-<seed hex>` ([`crate::generate`] が付与)
    pub id: String,
    /// family 名
    pub family: &'static str,
    /// この sample の seed ([`crate::generate`] が付与)
    pub seed: u64,
    /// 英語 caption (LLM への user prompt 相当)
    pub caption_en: String,
    /// 日本語 caption
    pub caption_ja: String,
    /// LOL text (family が書いた形 = 学習 target、product shortcut はそのまま)
    pub lol: String,
    /// 正規形 (`parse → emit`、shortcut は展開される) 検証 / dedup 用
    pub lol_canonical: String,
    /// oracle 点 (空可: `random_tree` 等)
    pub oracle: Vec<OraclePoint>,
    /// Intent verb 名列 (`program(...)` の時のみ、`seq`/`par` は先頭に)
    pub intent_verbs: Vec<String>,
}

/// [`Sample::verify`] の失敗理由
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// `parse_program` 失敗
    Parse(String),
    /// `Program::to_lol` が失敗 / 冪等でない
    Emit(String),
    /// oracle 点の eval が期待と逆
    Oracle {
        /// 点
        point: String,
        /// eval 値
        distance: String,
    },
    /// LLM grammar が受理しない (`grammar-check` feature)
    Grammar,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "parse: {e}"),
            Self::Emit(e) => write!(f, "emit: {e}"),
            Self::Oracle { point, distance } => write!(f, "oracle {point} d={distance}"),
            Self::Grammar => write!(f, "rejected by LLM grammar"),
        }
    }
}

impl Sample {
    /// caption + LOL text から sample を作る (`id` / `seed` は [`crate::generate`] が埋める)
    ///
    /// `lol` は family が書いた text をそのまま保持 (model に学ばせたい形、
    /// `pen_cup(50,100)` 等の shortcut を含む) `lol_canonical` に `parse → emit`
    /// の正規形を持つ
    ///
    /// # Errors
    ///
    /// parse / emit 失敗
    pub fn new(
        family: &'static str,
        caption_en: String,
        caption_ja: String,
        lol: &str,
        oracle: Vec<OraclePoint>,
    ) -> Result<Self, VerifyError> {
        let program = parse_program(lol).map_err(|e| VerifyError::Parse(e.to_string()))?;
        let canonical = program
            .to_lol()
            .map_err(|e| VerifyError::Emit(e.to_string()))?;
        let intent_verbs = program.intent.as_ref().map(verb_names).unwrap_or_default();
        Ok(Self {
            id: String::new(),
            family,
            seed: 0,
            caption_en,
            caption_ja,
            lol: lol.trim().to_string(),
            lol_canonical: canonical,
            oracle,
            intent_verbs,
        })
    }

    /// parse 可能 / emit 冪等 / oracle 一致 (/ grammar 受理) を確認
    ///
    /// # Errors
    ///
    /// [`VerifyError`]
    pub fn verify(&self) -> Result<(), VerifyError> {
        let program = parse_program(&self.lol).map_err(|e| VerifyError::Parse(e.to_string()))?;
        let canonical = program
            .to_lol()
            .map_err(|e| VerifyError::Emit(e.to_string()))?;
        if canonical != self.lol_canonical {
            return Err(VerifyError::Emit("canonical form drifted".to_string()));
        }
        // 正規形の冪等性 (emit ∘ parse ∘ emit = emit)
        let again = parse_program(&canonical)
            .map_err(|e| VerifyError::Parse(e.to_string()))?
            .to_lol()
            .map_err(|e| VerifyError::Emit(e.to_string()))?;
        if again != canonical {
            return Err(VerifyError::Emit(format!("not idempotent: `{again}`")));
        }
        for o in &self.oracle {
            let d = eval(&program.sdf, o.p);
            let ok = match o.side {
                Side::In => d < 0.0,
                Side::Out => d > 0.0,
            };
            if !ok {
                return Err(VerifyError::Oracle {
                    point: format!("({}, {}, {}) expected {:?}", o.p.x, o.p.y, o.p.z, o.side),
                    distance: format!("{d:.3}"),
                });
            }
        }
        #[cfg(feature = "grammar-check")]
        {
            use alice_lol::bridge::{lol_grammar, Fsm, LOL_FSM_MAX_DEPTH};
            let fsm = Fsm::start(lol_grammar())
                .map_err(|e| VerifyError::Emit(e.to_string()))?
                .with_max_depth(LOL_FSM_MAX_DEPTH);
            if !fsm.accepts_str(&self.lol) || !fsm.accepts_str(&self.lol_canonical) {
                return Err(VerifyError::Grammar);
            }
        }
        Ok(())
    }

    /// 1 行 JSON (serde 不使用、手書き escape)
    #[must_use]
    pub fn to_jsonl(&self) -> String {
        let mut s = String::with_capacity(self.lol.len() + self.caption_en.len() + 256);
        let _ = write!(
            s,
            "{{\"id\":\"{}\",\"family\":\"{}\",\"seed\":{},\"caption_en\":\"{}\",\"caption_ja\":\"{}\",\"lol\":\"{}\",\"lol_canonical\":\"{}\",\"oracle\":[",
            esc(&self.id),
            self.family,
            self.seed,
            esc(&self.caption_en),
            esc(&self.caption_ja),
            esc(&self.lol),
            esc(&self.lol_canonical)
        );
        for (i, o) in self.oracle.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(
                s,
                "[{},{},{},\"{}\"]",
                o.p.x,
                o.p.y,
                o.p.z,
                match o.side {
                    Side::In => "in",
                    Side::Out => "out",
                }
            );
        }
        s.push_str("],\"intent_verbs\":[");
        for (i, v) in self.intent_verbs.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(s, "\"{}\"", esc(v));
        }
        s.push_str("]}");
        s
    }
}

fn verb_names(node: &IntentNode) -> Vec<String> {
    const fn name(n: &IntentNode) -> &'static str {
        match n {
            IntentNode::Grasp { .. } => "grasp",
            IntentNode::Release { .. } => "release",
            IntentNode::Walk { .. } => "walk",
            IntentNode::Gaze { .. } => "gaze",
            IntentNode::Point { .. } => "point",
            IntentNode::Throw { .. } => "throw",
            IntentNode::Catch { .. } => "catch",
            IntentNode::Push { .. } => "push",
            IntentNode::Pull { .. } => "pull",
            IntentNode::Rotate { .. } => "turn",
            IntentNode::Align { .. } => "align",
            IntentNode::Follow { .. } => "follow",
            IntentNode::Avoid { .. } => "avoid",
            IntentNode::Rest { .. } => "rest",
            IntentNode::LatentIntent { .. } => "latent",
            IntentNode::Sequence(_) => "seq",
            IntentNode::Parallel(_) => "par",
            IntentNode::Music { .. } => "music",
        }
    }
    match node {
        IntentNode::Sequence(v) | IntentNode::Parallel(v) => {
            let mut out = vec![name(node).to_string()];
            out.extend(v.iter().map(|c| name(c).to_string()));
            out
        }
        single => vec![name(single).to_string()],
    }
}

/// JSON 文字列 escape
#[must_use]
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Program の SDF を取り出す (test / 観測用)
#[must_use]
pub fn parse_sdf(lol: &str) -> Option<Program> {
    // (const 化不可: parse は非 const)
    parse_program(lol).ok()
}
