//! template family 群 各 family は `fn generate(&mut Rng) -> Sample`
//!
//! sample の LOL は `Sample::new` で正規形化される caption は parameter から
//! 決定論的に組み立て、oracle 点も同じ parameter から作る (self-check で
//! 矛盾があれば捨てられるので、family の bug は `generate` の rejected 件数に出る)

use crate::rng::Rng;
use crate::sample::Sample;

pub mod attachment;
pub mod intent_program;
pub mod plate_holes;
pub mod primitive_placed;
pub mod product_shortcut;
pub mod random_tree;
pub mod stacked;
pub mod transformed;

/// family 識別子
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// 単体 primitive + 配置 (translate / half 引数)
    PrimitivePlaced,
    /// Y 軸に積む (translate + 合成)
    Stacked,
    /// 本体 + 付属 (mug / table / arch)
    Attachment,
    /// 板 + 穴 (`subtract` / `polar_repeat` / `repeat_finite`)
    PlateHoles,
    /// rotate / scale
    Transformed,
    /// Phase 3 Intent program
    IntentProgram,
    /// grammar bucket からの random tree (構造列挙 caption)
    RandomTree,
    /// product shortcut (`pen_cup(50,100)` 等) をそのまま学習 target に
    ProductShortcut,
}

/// 全 family (CLI `--families all`)
pub const ALL_FAMILIES: &[Family] = &[
    Family::PrimitivePlaced,
    Family::Stacked,
    Family::Attachment,
    Family::PlateHoles,
    Family::Transformed,
    Family::IntentProgram,
    Family::RandomTree,
    Family::ProductShortcut,
];

impl Family {
    /// JSONL / CLI 用の名前
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::PrimitivePlaced => "primitive_placed",
            Self::Stacked => "stacked",
            Self::Attachment => "attachment",
            Self::PlateHoles => "plate_holes",
            Self::Transformed => "transformed",
            Self::IntentProgram => "intent_program",
            Self::RandomTree => "random_tree",
            Self::ProductShortcut => "product_shortcut",
        }
    }

    /// 名前から (CLI 用)
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        ALL_FAMILIES.iter().copied().find(|f| f.name() == s)
    }

    /// 1 sample 生成 (self-check 前)
    ///
    /// # Panics
    ///
    /// family の template が自分の生成した LOL を parse / emit できない時
    /// (= family 実装の bug、data 生成時に即露見させる)
    #[must_use]
    pub fn generate(self, rng: &mut Rng) -> Sample {
        let r = match self {
            Self::PrimitivePlaced => primitive_placed::generate(rng),
            Self::Stacked => stacked::generate(rng),
            Self::Attachment => attachment::generate(rng),
            Self::PlateHoles => plate_holes::generate(rng),
            Self::Transformed => transformed::generate(rng),
            Self::IntentProgram => intent_program::generate(rng),
            Self::RandomTree => random_tree::generate(rng),
            Self::ProductShortcut => product_shortcut::generate(rng),
        };
        r.unwrap_or_else(|e| panic!("family {} produced unparsable LOL: {e}", self.name()))
    }
}

/// mm 寸法 (5..=100、5 刻み) — 「切りのいい」寸法
pub(crate) fn dim(rng: &mut Rng) -> f32 {
    rng.stepped(5.0, 100.0, 5.0)
}

/// 小寸法 (1..=20、1 刻み)
pub(crate) fn small(rng: &mut Rng) -> f32 {
    rng.stepped(1.0, 20.0, 1.0)
}

/// 配置座標 (-50..=50、5 刻み)
pub(crate) fn coord(rng: &mut Rng) -> f32 {
    rng.stepped(-50.0, 50.0, 5.0)
}
