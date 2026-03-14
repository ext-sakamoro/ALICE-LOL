//! v0.5 変数キャプチャ構文のテスト

use alice_lol::{eval, lol, Vec3};

/// {expr} で変数をキャプチャ
#[test]
fn capture_variable_sphere() {
    let r = 1.0_f32;
    let node = lol! { sphere({r}) };
    let d = eval(&node, Vec3::ZERO);
    assert!((d - (-1.0)).abs() < 1e-4, "SDF at origin = -{r}, got {d}");
}

/// {式} で算術式をキャプチャ
#[test]
fn capture_expression() {
    let base = 2.0_f32;
    let node = lol! { sphere({base * 0.5}) };
    let d = eval(&node, Vec3::ZERO);
    assert!(
        (d - (-1.0)).abs() < 1e-4,
        "radius = base*0.5 = 1.0, got {d}"
    );
}

/// translate の引数に変数を混在
#[test]
fn capture_mixed_translate() {
    let offset = 3.0_f32;
    let node = lol! { translate({offset}, 0.0, 0.0, sphere(1.0)) };
    // 原点では距離 = 3.0 - 1.0 = 2.0
    let d = eval(&node, Vec3::ZERO);
    assert!((d - 2.0).abs() < 1e-4, "expected 2.0, got {d}");
}

/// smooth_union の k に変数
#[test]
fn capture_k_smooth_union() {
    let k = 0.5_f32;
    let node = lol! {
        smooth_union({k},
            sphere(1.0),
            translate(2.0, 0.0, 0.0, sphere(1.0))
        )
    };
    let d = eval(&node, Vec3::ZERO);
    assert!(d < 0.0, "内部なので負になるはず, got {d}");
}

/// 裸の変数名（{} なし）
#[test]
fn capture_bare_variable() {
    let r = 1.5_f32;
    let node = lol! { sphere(r) };
    let d = eval(&node, Vec3::ZERO);
    assert!((d - (-1.5)).abs() < 1e-4, "radius=1.5, got {d}");
}

/// 複数の {expr} を組み合わせ
#[test]
fn capture_multiple_expressions() {
    let hx = 1.0_f32;
    let hy = 2.0_f32;
    let hz = 0.5_f32;
    let node = lol! { box3d({hx}, {hy}, {hz}) };
    let d = eval(&node, Vec3::ZERO);
    // Box3d at origin: max(-1.0, -2.0, -0.5) = -0.5
    assert!((d - (-0.5)).abs() < 1e-4, "expected -0.5, got {d}");
}

/// scale の factor に変数
#[test]
fn capture_scale_factor() {
    let factor = 2.0_f32;
    let node = lol! { scale({factor}, sphere(1.0)) };
    let d = eval(&node, Vec3::ZERO);
    assert!((d - (-2.0)).abs() < 1e-4, "scaled sphere, got {d}");
}

/// 関数呼び出しを {expr} で埋め込み
#[test]
fn capture_function_call() {
    fn compute_radius() -> f32 {
        3.0
    }
    let node = lol! { sphere({compute_radius()}) };
    let d = eval(&node, Vec3::ZERO);
    assert!((d - (-3.0)).abs() < 1e-4, "expected -3.0, got {d}");
}
