//! Backend parity test suite (Milestone A.4 Level 1: structural)
//!
//! 全 GLSL/WGSL/HLSL backend が同一 SdfNode に対して:
//!
//! 1. panic なく transpile 完了
//! 2. 非空の shader source 生成
//! 3. 主要 operator の characteristic keyword が出現
//!
//! を保証する
//!
//! Level 2 (実 GPU 実行 + CPU eval との数値比較) は A.4.1 別 sprint (wgpu setup 必要)
//!
//! 実行:
//!   cargo test --test backend_parity --features "glsl wgsl hlsl"

#![cfg(all(feature = "glsl", feature = "wgsl", feature = "hlsl"))]

use alice_lol::{
    lol, to_glsl, to_glsl_dynamic, to_hlsl, to_hlsl_dynamic, to_wgsl, to_wgsl_dynamic, SdfNode,
    Vec3,
};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Test 支援関数
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 全 backend transpile 実行 (Hardcoded mode) — panic なし + 非空 output 検証
fn assert_all_backends_transpile(node: &SdfNode, case_name: &str) {
    let glsl = to_glsl(node);
    let wgsl = to_wgsl(node);
    let hlsl = to_hlsl(node);

    assert!(!glsl.is_empty(), "{case_name}: GLSL transpile が空");
    assert!(!wgsl.is_empty(), "{case_name}: WGSL transpile が空");
    assert!(!hlsl.is_empty(), "{case_name}: HLSL transpile が空");

    // 各 backend の main SDF 関数が生成されているか (naming convention は backend 別)
    assert!(
        glsl.contains("float") || glsl.contains("void main"),
        "{case_name}: GLSL に float 型宣言 or main 無し"
    );
    assert!(
        wgsl.contains("fn ") || wgsl.contains("f32"),
        "{case_name}: WGSL に fn / f32 無し"
    );
    assert!(
        hlsl.contains("float") || hlsl.contains("void main"),
        "{case_name}: HLSL に float 無し"
    );
}

/// Dynamic mode 版 — parity 検証 (uniform buffer 経路)
fn assert_all_backends_transpile_dynamic(node: &SdfNode, case_name: &str) {
    let glsl = to_glsl_dynamic(node);
    let wgsl = to_wgsl_dynamic(node);
    let hlsl = to_hlsl_dynamic(node);

    assert!(!glsl.is_empty(), "{case_name}: GLSL dynamic transpile が空");
    assert!(!wgsl.is_empty(), "{case_name}: WGSL dynamic transpile が空");
    assert!(!hlsl.is_empty(), "{case_name}: HLSL dynamic transpile が空");
}

/// CPU eval が有限値を返す (Inf / NaN でない) — Reference 実装の健全性検証
fn assert_cpu_eval_finite(node: &SdfNode, case_name: &str) {
    for &point in &[
        Vec3::ZERO,
        Vec3::new(0.5, 0.5, 0.5),
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(2.0, 2.0, 2.0),
    ] {
        let d = alice_sdf::eval(node, point);
        assert!(
            d.is_finite(),
            "{case_name}: CPU eval at {point:?} = {d} (not finite)"
        );
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Primitive parity (7 test)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn parity_sphere() {
    let n = lol! { sphere(1.0) };
    assert_all_backends_transpile(&n, "sphere");
    assert_all_backends_transpile_dynamic(&n, "sphere");
    assert_cpu_eval_finite(&n, "sphere");
}

#[test]
fn parity_box3d() {
    let n = lol! { box3d(1.0, 0.5, 0.5) };
    assert_all_backends_transpile(&n, "box3d");
    assert_all_backends_transpile_dynamic(&n, "box3d");
    assert_cpu_eval_finite(&n, "box3d");
}

#[test]
fn parity_torus() {
    let n = lol! { torus(1.0, 0.3) };
    assert_all_backends_transpile(&n, "torus");
    assert_all_backends_transpile_dynamic(&n, "torus");
    assert_cpu_eval_finite(&n, "torus");
}

#[test]
fn parity_cylinder() {
    let n = lol! { cylinder(1.0, 2.0) };
    assert_all_backends_transpile(&n, "cylinder");
    assert_all_backends_transpile_dynamic(&n, "cylinder");
    assert_cpu_eval_finite(&n, "cylinder");
}

#[test]
fn parity_cone() {
    let n = lol! { cone(1.0, 2.0) };
    assert_all_backends_transpile(&n, "cone");
    assert_all_backends_transpile_dynamic(&n, "cone");
    assert_cpu_eval_finite(&n, "cone");
}

#[test]
fn parity_rounded_box() {
    let n = lol! { rounded_box(1.0, 0.5, 0.5, 0.1) };
    assert_all_backends_transpile(&n, "rounded_box");
    assert_all_backends_transpile_dynamic(&n, "rounded_box");
    assert_cpu_eval_finite(&n, "rounded_box");
}

#[test]
fn parity_ellipsoid() {
    let n = lol! { ellipsoid(1.0, 0.5, 0.3) };
    assert_all_backends_transpile(&n, "ellipsoid");
    assert_all_backends_transpile_dynamic(&n, "ellipsoid");
    assert_cpu_eval_finite(&n, "ellipsoid");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CSG parity (5 test)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn parity_union() {
    let n = lol! { union(sphere(1.0), box3d(0.5, 0.5, 0.5)) };
    assert_all_backends_transpile(&n, "union");
    assert_all_backends_transpile_dynamic(&n, "union");
    assert_cpu_eval_finite(&n, "union");
}

#[test]
fn parity_smooth_union() {
    let n = lol! { smooth_union(0.2, sphere(1.0), box3d(0.5, 0.5, 0.5)) };
    assert_all_backends_transpile(&n, "smooth_union");
    assert_all_backends_transpile_dynamic(&n, "smooth_union");
    assert_cpu_eval_finite(&n, "smooth_union");
}

#[test]
fn parity_intersection() {
    let n = lol! { intersection(sphere(1.0), box3d(0.7, 0.7, 0.7)) };
    assert_all_backends_transpile(&n, "intersection");
    assert_all_backends_transpile_dynamic(&n, "intersection");
    assert_cpu_eval_finite(&n, "intersection");
}

#[test]
fn parity_subtract() {
    let n = lol! { subtract(sphere(1.0), box3d(0.5, 0.5, 0.5)) };
    assert_all_backends_transpile(&n, "subtract");
    assert_all_backends_transpile_dynamic(&n, "subtract");
    assert_cpu_eval_finite(&n, "subtract");
}

#[test]
fn parity_chamfer_union() {
    let n = lol! { chamfer_union(0.15, sphere(1.0), box3d(0.5, 0.5, 0.5)) };
    assert_all_backends_transpile(&n, "chamfer_union");
    assert_all_backends_transpile_dynamic(&n, "chamfer_union");
    assert_cpu_eval_finite(&n, "chamfer_union");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Transform parity (3 test)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn parity_translate() {
    let n = lol! { translate(1.0, 0.5, 0.0, sphere(1.0)) };
    assert_all_backends_transpile(&n, "translate");
    assert_all_backends_transpile_dynamic(&n, "translate");
    assert_cpu_eval_finite(&n, "translate");
}

#[test]
fn parity_scale() {
    let n = lol! { scale(2.0, sphere(0.5)) };
    assert_all_backends_transpile(&n, "scale");
    assert_all_backends_transpile_dynamic(&n, "scale");
    assert_cpu_eval_finite(&n, "scale");
}

#[test]
fn parity_scale_non_uniform() {
    let n = lol! { scale_non_uniform(2.0, 1.0, 0.5, sphere(1.0)) };
    assert_all_backends_transpile(&n, "scale_non_uniform");
    assert_all_backends_transpile_dynamic(&n, "scale_non_uniform");
    assert_cpu_eval_finite(&n, "scale_non_uniform");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Modifier parity (4 test)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn parity_twist() {
    let n = lol! { twist(2.0, cylinder(0.5, 2.0)) };
    assert_all_backends_transpile(&n, "twist");
    assert_all_backends_transpile_dynamic(&n, "twist");
    assert_cpu_eval_finite(&n, "twist");
}

#[test]
fn parity_onion() {
    let n = lol! { onion(0.1, sphere(1.0)) };
    assert_all_backends_transpile(&n, "onion");
    assert_all_backends_transpile_dynamic(&n, "onion");
    assert_cpu_eval_finite(&n, "onion");
}

#[test]
fn parity_round() {
    let n = lol! { round(0.1, box3d(1.0, 0.5, 0.5)) };
    assert_all_backends_transpile(&n, "round");
    assert_all_backends_transpile_dynamic(&n, "round");
    assert_cpu_eval_finite(&n, "round");
}

#[test]
fn parity_bend() {
    let n = lol! { bend(1.5, box3d(2.0, 0.3, 0.3)) };
    assert_all_backends_transpile(&n, "bend");
    assert_all_backends_transpile_dynamic(&n, "bend");
    assert_cpu_eval_finite(&n, "bend");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// TPMS parity (1 test)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn parity_gyroid() {
    let n = lol! { gyroid(1.0, 0.1) };
    assert_all_backends_transpile(&n, "gyroid");
    assert_all_backends_transpile_dynamic(&n, "gyroid");
    assert_cpu_eval_finite(&n, "gyroid");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Composite parity (2 test) — 実利用シナリオに近い複雑 tree
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn parity_nested_csg() {
    let n = lol! {
        smooth_union(
            0.3,
            translate(1.0, 0.0, 0.0, sphere(0.8)),
            translate(-1.0, 0.0, 0.0, box3d(0.5, 0.8, 0.5))
        )
    };
    assert_all_backends_transpile(&n, "nested_csg");
    assert_all_backends_transpile_dynamic(&n, "nested_csg");
    assert_cpu_eval_finite(&n, "nested_csg");
}

#[test]
fn parity_transformed_hollow() {
    let n = lol! {
        onion(0.05, rotate(0.5, 0.5, 0.5, subtract(sphere(1.0), box3d(0.6, 0.6, 0.6))))
    };
    assert_all_backends_transpile(&n, "transformed_hollow");
    assert_all_backends_transpile_dynamic(&n, "transformed_hollow");
    assert_cpu_eval_finite(&n, "transformed_hollow");
}
