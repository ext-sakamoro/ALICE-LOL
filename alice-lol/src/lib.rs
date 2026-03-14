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
pub mod runtime_parser;

// ── 空間枝刈りコンパイラ ──
pub mod pruned_compile;

// ── 法則（Law）制約チェッカー ──
// LawSet ビルダー、静的矛盾検出、残差フィルタリング
pub mod law;

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
#[cfg(feature = "physics")]
pub use alice_sdf::physics_bridge::{sdf_to_physics_field, CompiledSdfField};

#[cfg(feature = "physics")]
pub use alice_sdf::sim_bridge::{attach_physics, simulate_sdf, SimulatedSdf};
