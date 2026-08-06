//! # alice-lol-humanoid
//!
//! Humanoid template for `alice_lol` DSL parametric character generation.
//!
//! `~/ALICE-LOL/docs/HUMANOID_TEMPLATE_ROADMAP.md` の Phase H.0-H.6 に沿って段階実装
//! 現在 Phase = **H.0 Scaffolding** (crate 骨格のみ、実装は H.1 以降)
//!
//! # Roadmap
//!
//! - H.0 Scaffolding (本 Phase)
//! - H.1 Static template (`HumanoidTemplate::default()` + `to_sdf(k)`)
//! - H.2 Parametric morphology (`MorphologyParams` + builder)
//! - H.3 VRM import (feature `vrm`、gltf optional dep)
//! - H.4 BVH import + pose (`apply_pose`)
//! - H.5 ALICE-Manga との duplication 整理
//! - H.6 Intent 結線 (`apply_intent(&IntentNode)`)
//!
//! # 三相原理での位置付け
//!
//! Phase 2 Law (parametric morphology) → Phase 3 Intent (verb) の橋渡し
//! 詳細: `docs/HUMANOID_TEMPLATE_DESIGN.md` §1

#![forbid(unsafe_code)]

/// Humanoid template scaffolding placeholder (H.0)
///
/// 実際の joint / bone / morphology / `to_sdf` API は H.1 以降で追加予定
/// 現在は crate 骨格の smoke test target のみ
#[derive(Debug, Clone, Copy, Default)]
pub struct HumanoidTemplate;

impl HumanoidTemplate {
    /// H.0 scaffolding constructor 実質 [`Default::default()`] と等価
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffolding_smoke_test() {
        let _t = HumanoidTemplate::new();
    }

    #[test]
    fn zero_sized_placeholder() {
        assert_eq!(std::mem::size_of::<HumanoidTemplate>(), 0);
    }
}
