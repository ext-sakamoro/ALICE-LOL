//! # ALICE-LOL: Law-Oriented Language v0.1
//!
//! `proc_macro` DSL that compiles LOL syntax → `SdfNode` → GLSL/WGSL/HLSL.
//!
//! ## Quick Start
//!
//! ```ignore
//! use alice_lol::lol;
//!
//! let node = lol! {
//!     field MyScene {
//!         smooth_union(0.2,
//!             sphere(1.0),
//!             translate(2.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5))
//!         )
//!     }
//! };
//!
//! let glsl = alice_lol::to_glsl(&node);
//! println!("{glsl}");
//! ```

// ── ランタイム LOL パーサー（LLM Text-to-3D 用） ──
// `parse_lol("sphere(1.0)")` → `SdfNode` に変換
/// The LOL DSL grammar (`lol.gbnf`, GBNF) as shipped with this crate version.
///
/// Feature-free on purpose: downstream crates (alice-bamboo → text-to-print,
/// or any llama.cpp-compatible runtime) embed the *same* bytes instead of
/// keeping a copy that drifts. With `llm-bridge` enabled,
/// [`bridge::lol_grammar`] parses exactly this text.
///
/// The grammar is deliberately stricter than [`runtime_parser`] (no `//`
/// comments, at most one whitespace char between tokens); see the file
/// header for why.
pub const LOL_GBNF: &str = include_str!("../../lol.gbnf");

pub mod runtime_parser;

/// `SdfNode` → LOL text (runtime parser の逆変換、Track C0)
pub mod emit;

// ── 3Dプリント向けエクスポート ──
// LOL → SdfNode → Mesh → STL/3MF のワンストップパイプライン
pub mod print_export;

// ── Roblox 向けエクスポート ──
// LOL → SdfNode → Mesh → OBJ/FBX (MeshPart / アクセサリー用)
#[cfg(feature = "roblox")]
pub mod roblox_export;

// ── LLM Guided Generation ブリッジ (Phase X.8 B-6) ──
// alice-llm の GBNF parser + FSM + logits mask を薄く再輸出し、
// `lol.gbnf` の LazyLock cached parse を提供する
#[cfg(feature = "llm-bridge")]
pub mod bridge;

// ── レーザー彫刻向け2Dパターン生成 ──
// hatch, crosshatch, halftone, dither, guilloche, lissajous, rose, phyllotaxis, turing
pub mod laser_pattern;

// ── 空間枝刈りコンパイラ ──
pub mod pruned_compile;

// ── 法則（Law）制約チェッカー ──
// LawSet ビルダー、静的矛盾検出、残差フィルタリング
pub mod law;

// ── Intent Layer (Milestone B.1、Phase 3 Intent 相 IR skeleton) ──
// IntentNode + Program 独立型、GPU backend との型分離、L1 Physical Intent 14 verb + Sequence/Parallel
pub mod intent;

// ── Standard Library (Phase A.1、hardsurface primitive collection) ──
// 実プリント検証済みの機械要素 primitive (fastener/joint/reinforcement/mount) を SdfNode helper で提供
pub mod stdlib;

// ── Re-export the proc_macro ──
pub use alice_lol_macro::lol;

// ── Re-exports used by macro-generated code ──
pub use alice_sdf::SdfNode;
pub use glam::{EulerRot, Quat, Vec3};

// ── Transpile functions ──

/// Transpile an `SdfNode` tree to GLSL (hardcoded constants).
#[must_use]
#[cfg(feature = "glsl")]
pub fn to_glsl(node: &SdfNode) -> String {
    use alice_sdf::compiled::glsl::{GlslShader, GlslTranspileMode};
    GlslShader::transpile(node, GlslTranspileMode::Hardcoded).source
}

/// Transpile an `SdfNode` tree to GLSL with dynamic parameters (uniform block).
#[must_use]
#[cfg(feature = "glsl")]
pub fn to_glsl_dynamic(node: &SdfNode) -> String {
    use alice_sdf::compiled::glsl::{GlslShader, GlslTranspileMode};
    GlslShader::transpile(node, GlslTranspileMode::Dynamic).source
}

/// Transpile an `SdfNode` tree to WGSL (hardcoded constants).
#[must_use]
#[cfg(feature = "wgsl")]
pub fn to_wgsl(node: &SdfNode) -> String {
    use alice_sdf::compiled::wgsl::{TranspileMode, WgslShader};
    WgslShader::transpile(node, TranspileMode::Hardcoded).source
}

/// Transpile an `SdfNode` tree to WGSL with dynamic parameters.
#[must_use]
#[cfg(feature = "wgsl")]
pub fn to_wgsl_dynamic(node: &SdfNode) -> String {
    use alice_sdf::compiled::wgsl::{TranspileMode, WgslShader};
    WgslShader::transpile(node, TranspileMode::Dynamic).source
}

/// Transpile an `SdfNode` tree to HLSL (hardcoded constants).
#[must_use]
#[cfg(feature = "hlsl")]
pub fn to_hlsl(node: &SdfNode) -> String {
    use alice_sdf::compiled::hlsl::{HlslShader, HlslTranspileMode};
    HlslShader::transpile(node, HlslTranspileMode::Hardcoded).source
}

/// Transpile an `SdfNode` tree to HLSL with dynamic parameters.
#[must_use]
#[cfg(feature = "hlsl")]
pub fn to_hlsl_dynamic(node: &SdfNode) -> String {
    use alice_sdf::compiled::hlsl::{HlslShader, HlslTranspileMode};
    HlslShader::transpile(node, HlslTranspileMode::Dynamic).source
}

/// フルレンダリングパイプライン付きGLSLを生成
///
/// PBR (Cook-Torrance), 大気散乱, 昼夜サイクル, 天候, ポストプロセスを含む
/// 完全なフラグメントシェーダーを出力する。
#[must_use]
#[cfg(feature = "glsl")]
pub fn to_glsl_full(node: &SdfNode, config: &RenderConfig) -> String {
    use alice_sdf::compiled::glsl::{GlslShader, GlslTranspileMode};
    GlslShader::transpile(node, GlslTranspileMode::Hardcoded).to_fragment_shader_full(config)
}

// ── RenderConfig re-export ──
#[cfg(feature = "glsl")]
pub use alice_sdf::compiled::glsl::RenderConfig;

/// Evaluate the SDF distance at a single point (CPU).
#[must_use]
pub fn eval(node: &SdfNode, point: Vec3) -> f32 {
    alice_sdf::eval(node, point)
}

// ── Interval arithmetic re-exports (for spatial pruning) ──
pub use alice_sdf::interval::{eval_interval, Interval, Vec3Interval};

// ── Autodiff re-exports (勾配・曲率解析) ──
pub use alice_sdf::autodiff::{
    eval_hessian, eval_with_gradient, gaussian_curvature, mean_curvature, principal_curvatures,
    Dual, Dual3,
};

// ── CompiledSdf re-exports (高速評価) ──
pub use alice_sdf::compiled::{
    eval_compiled, eval_compiled_batch_simd, eval_compiled_batch_simd_parallel,
    eval_compiled_batch_soa, eval_compiled_batch_soa_parallel, eval_compiled_bvh,
    eval_compiled_distance_and_normal, eval_compiled_normal, eval_compiled_simd, get_scene_aabb,
    AabbPacked, CompileError, CompiledSdf, CompiledSdfBvh, Vec3x8,
};

// ── Physics bridge re-exports (物理連携) ──
// alice-sdf 1.7.7 以降 physics_bridge / sim_bridge module は crates.io publish のため
// alice-sdf 側 physics feature ごと削除済 (source 上は cfg-gate 保持) alice-sdf が
// physics feature を復活させたら以下 pub use を再度有効化する
// #[cfg(feature = "physics")]
// pub use alice_sdf::physics_bridge::{sdf_to_physics_field, CompiledSdfField};
// #[cfg(feature = "physics")]
// pub use alice_sdf::sim_bridge::{attach_physics, simulate_sdf, SimulatedSdf};
