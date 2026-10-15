//! 単位の宣言: `units(<unit>, <purpose>, <program>)`
//!
//! LOL の数値が何の長さかを program 自身が言う宣言で、root にだけ置ける (入れ子は不可)
//!
//! ```text
//! units(mm, print, cylinder(50, 5))        1 単位 = 1 mm の印刷用の形
//! units(world, render, sphere(1))          物理的な大きさを持たない描画用の場面
//! ```
//!
//! 単位と用途の一覧は `alice-lol/language/constructs.toml` の `unit` 表と `purpose` が正典で、
//! 文法 (`lol.gbnf`) はそこから生成し、[`Unit`] / [`Purpose`] がそれと一致することは
//! `tests/units_table.rs` が確かめる 長さを持たない単位 (`world`) は `render` にだけ使える
//!
//! 宣言の無い program は今までどおり [`crate::print_export::PrintConfig`] の倍率と範囲で
//! 書き出し、[`no_units_warning`] の警告 (`LOL-U001`) を出す

use crate::intent::Program;
use crate::print_export::PrintConfig;

/// 1 program 単位が何か
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Unit {
    /// millimetre
    Mm,
    /// centimetre (10 mm)
    Cm,
    /// metre (1000 mm)
    M,
    /// inch (25.4 mm)
    In,
    /// 物理的な大きさを持たない (描画用の場面、`render` にだけ使える)
    World,
}

impl Unit {
    /// 宣言できる単位すべて (`constructs.toml` の `unit` 表の順)
    pub const ALL: [Self; 5] = [Self::Mm, Self::Cm, Self::M, Self::In, Self::World];

    /// program に書く名前
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mm => "mm",
            Self::Cm => "cm",
            Self::M => "m",
            Self::In => "in",
            Self::World => "world",
        }
    }

    /// 名前から (知らない名前は `None`)
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.name() == name)
    }

    /// 1 program 単位が何 mm か (長さを持たない `world` は `None`)
    #[must_use]
    pub const fn mm_per_unit(self) -> Option<f32> {
        match self {
            Self::Mm => Some(1.0),
            Self::Cm => Some(10.0),
            Self::M => Some(1000.0),
            Self::In => Some(25.4),
            Self::World => None,
        }
    }
}

/// program が何のためのものか
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Purpose {
    /// 印刷する形 (長さの単位が要る)
    Print,
    /// 描画する場面
    Render,
}

impl Purpose {
    /// 宣言できる用途すべて (`constructs.toml` の `purpose` の順)
    pub const ALL: [Self; 2] = [Self::Print, Self::Render];

    /// program に書く名前
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Print => "print",
            Self::Render => "render",
        }
    }

    /// 名前から (知らない名前は `None`)
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// 単位の宣言 (`units(<unit>, <purpose>, ...)` の 2 つの引数)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct UnitsDecl {
    /// 1 program 単位が何か
    pub unit: Unit,
    /// 何のための program か
    pub purpose: Purpose,
}

impl UnitsDecl {
    /// 宣言を作る 長さを持たない単位を `print` に使うと `None`
    #[must_use]
    pub const fn new(unit: Unit, purpose: Purpose) -> Option<Self> {
        if unit.mm_per_unit().is_none() && matches!(purpose, Purpose::Print) {
            return None;
        }
        Some(Self { unit, purpose })
    }
}

/// 読んだ program と、その単位の宣言 ([`crate::runtime_parser::parse_declared`] が返す)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Declared {
    /// 単位の宣言 (無ければ `None`、書き出しは [`PrintConfig`] の値に従い `LOL-U001` を出す)
    pub units: Option<UnitsDecl>,
    /// 宣言の中身 (裸の式は SDF だけの program)
    pub program: Program,
}

impl Declared {
    /// 宣言と program から
    #[must_use]
    pub const fn new(units: Option<UnitsDecl>, program: Program) -> Self {
        Self { units, program }
    }
}

/// 診断 (検査の結果として人と LLM に見せる 1 件)
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// 種類 (`LOL-U001` など)
    pub code: &'static str,
    /// 本文 (LLM への修正指示にもそのまま渡す)
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "warning[{}]: {}", self.code, self.message)
    }
}

/// 単位の宣言が無い program を `config` で書き出す時の警告
#[must_use]
pub fn no_units_warning(config: &PrintConfig) -> Diagnostic {
    let s = config.scale_mm;
    let (lo, hi) = (config.bounds_min, config.bounds_max);
    Diagnostic {
        code: "LOL-U001",
        message: format!(
            "no units declaration; exported with PrintConfig.scale_mm={s} (1 unit = {s} mm) \
             inside bounds ({}, {}, {})..({}, {}, {}). If the numbers are millimetres, write \
             units(mm, print, <program>); for a render-only scene write units(world, render, <program>).",
            lo.x, lo.y, lo.z, hi.x, hi.y, hi.z
        ),
    }
}
