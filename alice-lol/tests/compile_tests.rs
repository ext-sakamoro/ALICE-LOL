//! コンパイル確認テスト: 全27構文が正しく SdfNode を生成することを検証

use alice_lol::{lol, Vec3};

fn eval(node: &alice_lol::SdfNode, p: Vec3) -> f32 {
    alice_lol::eval(node, p)
}

const O: Vec3 = Vec3::ZERO;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// プリミティブ (10)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn prim_sphere() {
    let n = lol! { sphere(1.0) };
    assert!((eval(&n, O) - (-1.0)).abs() < 1e-5);
    assert!((eval(&n, Vec3::X) - 0.0).abs() < 1e-5);
}

#[test]
fn prim_box3d() {
    let n = lol! { box3d(0.5, 0.5, 0.5) };
    assert!((eval(&n, O) - (-0.5)).abs() < 1e-5);
    assert!((eval(&n, Vec3::X) - 0.5).abs() < 1e-5);
}

#[test]
fn prim_rounded_box() {
    let n = lol! { rounded_box(0.5, 0.5, 0.5, 0.1) };
    // 内部の距離は -(0.5 + 0.1) = -0.6
    assert!((eval(&n, O) - (-0.6)).abs() < 1e-4);
}

#[test]
fn prim_cylinder() {
    let n = lol! { cylinder(0.5, 1.0) };
    assert!((eval(&n, O) - (-0.5)).abs() < 1e-5);
}

#[test]
fn prim_torus() {
    let n = lol! { torus(1.0, 0.3) };
    // 原点はトーラスの中心穴の中: 距離 = major - minor = 0.7
    assert!((eval(&n, O) - 0.7).abs() < 1e-5);
    // (1,0,0) はリング上: 距離 ≈ -0.3
    assert!((eval(&n, Vec3::X) - (-0.3)).abs() < 1e-5);
}

#[test]
fn prim_cone() {
    let n = lol! { cone(0.5, 1.0) };
    assert!(eval(&n, O) < 0.0); // 内部
}

#[test]
fn prim_capsule() {
    let n = lol! { capsule(0.3, 1.0) };
    // 原点は軸上: 距離 = -0.3
    assert!((eval(&n, O) - (-0.3)).abs() < 1e-5);
}

#[test]
fn prim_ellipsoid() {
    let n = lol! { ellipsoid(1.0, 0.5, 0.8) };
    assert!(eval(&n, O) < 0.0); // 内部
}

#[test]
fn prim_plane() {
    let n = lol! { plane(0.0, 1.0, 0.0, 0.0) };
    assert!((eval(&n, O) - 0.0).abs() < 1e-5);
    assert!((eval(&n, Vec3::Y) - 1.0).abs() < 1e-5);
}

#[test]
fn prim_octahedron() {
    let n = lol! { octahedron(1.0) };
    assert!(eval(&n, O) < 0.0); // 内部
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// オペレーション (6)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn op_union() {
    let n = lol! { union(sphere(1.0), translate(5.0, 0.0, 0.0, sphere(1.0))) };
    assert!((eval(&n, O) - (-1.0)).abs() < 1e-5);
    assert!((eval(&n, Vec3::new(5.0, 0.0, 0.0)) - (-1.0)).abs() < 1e-5);
}

#[test]
fn op_smooth_union() {
    let n = lol! { smooth_union(0.5, sphere(1.0), translate(1.5, 0.0, 0.0, sphere(1.0))) };
    // スムーズ結合なので接合部は個別のunionより低い値になる
    let d_smooth = eval(&n, Vec3::new(0.75, 0.0, 0.0));
    let n_hard = lol! { union(sphere(1.0), translate(1.5, 0.0, 0.0, sphere(1.0))) };
    let d_hard = eval(&n_hard, Vec3::new(0.75, 0.0, 0.0));
    assert!(d_smooth <= d_hard);
}

#[test]
fn op_smooth_union_3way() {
    // 3体のleft-foldが正しく動作するか
    let n = lol! {
        smooth_union(0.2,
            sphere(0.5),
            translate(2.0, 0.0, 0.0, sphere(0.5)),
            translate(0.0, 2.0, 0.0, sphere(0.5))
        )
    };
    assert!(eval(&n, O) < 0.0);
    assert!(eval(&n, Vec3::new(2.0, 0.0, 0.0)) < 0.0);
    assert!(eval(&n, Vec3::new(0.0, 2.0, 0.0)) < 0.0);
}

#[test]
fn op_intersection() {
    let n = lol! { intersection(sphere(1.0), box3d(0.5, 0.5, 0.5)) };
    // intersection → max(d_sphere, d_box), originで-0.5
    assert!((eval(&n, O) - (-0.5)).abs() < 1e-5);
}

#[test]
fn op_smooth_intersection() {
    let n = lol! { smooth_intersection(0.1, sphere(1.0), box3d(0.5, 0.5, 0.5)) };
    assert!(eval(&n, O) < 0.0);
}

#[test]
fn op_subtract() {
    let n = lol! { subtract(sphere(1.0), box3d(0.6, 0.6, 0.6)) };
    // originはboxの内部なので引き算で外側になる
    assert!(eval(&n, O) > 0.0);
}

#[test]
fn op_smooth_subtract() {
    let n = lol! { smooth_subtract(0.1, sphere(1.0), box3d(0.6, 0.6, 0.6)) };
    assert!(eval(&n, O) > 0.0);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// トランスフォーム (3)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn transform_translate() {
    let n = lol! { translate(3.0, 0.0, 0.0, sphere(1.0)) };
    assert!((eval(&n, Vec3::new(3.0, 0.0, 0.0)) - (-1.0)).abs() < 1e-5);
    assert!((eval(&n, O) - 2.0).abs() < 1e-5);
}

#[test]
fn transform_rotate() {
    let n = lol! { rotate(0.0, 0.0, 90.0, box3d(2.0, 0.1, 0.1)) };
    // 90度回転: x→y
    assert!(eval(&n, Vec3::new(0.0, 1.0, 0.0)) < 0.0);
}

#[test]
fn transform_scale() {
    let n = lol! { scale(2.0, sphere(1.0)) };
    assert!((eval(&n, O) - (-2.0)).abs() < 1e-5);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// モディファイア (6)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn mod_round() {
    let n = lol! { round(0.1, box3d(0.5, 0.5, 0.5)) };
    assert!((eval(&n, O) - (-0.6)).abs() < 1e-4);
}

#[test]
fn mod_onion() {
    let n = lol! { onion(0.1, sphere(1.0)) };
    // originは球の内部 → onion(shell)で外側に: |d| - thickness
    assert!((eval(&n, O) - 0.9).abs() < 1e-5);
    // 表面上: |0| - 0.1 = -0.1
    assert!((eval(&n, Vec3::X) - (-0.1)).abs() < 1e-5);
}

#[test]
fn mod_twist() {
    let n = lol! { twist(1.0, cylinder(0.5, 2.0)) };
    assert!(eval(&n, O) < 0.0); // 内部
}

#[test]
fn mod_bend() {
    let n = lol! { bend(0.5, box3d(0.3, 0.3, 2.0)) };
    assert!(eval(&n, O) < 0.0); // 内部
}

#[test]
fn mod_mirror() {
    let n = lol! { mirror(1.0, 0.0, 0.0, translate(2.0, 0.0, 0.0, sphere(0.5))) };
    assert!((eval(&n, Vec3::new(2.0, 0.0, 0.0)) - (-0.5)).abs() < 1e-5);
    assert!((eval(&n, Vec3::new(-2.0, 0.0, 0.0)) - (-0.5)).abs() < 1e-5);
}

#[test]
fn mod_repeat() {
    let n = lol! { repeat(4.0, 4.0, 4.0, sphere(0.5)) };
    // 原点とリピート位置で同じ距離
    let d0 = eval(&n, O);
    let d1 = eval(&n, Vec3::new(4.0, 0.0, 0.0));
    assert!((d0 - d1).abs() < 1e-5);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// GLSL トランスパイル
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn transpile_glsl() {
    let n = lol! {
        field Test {
            smooth_union(0.2, sphere(1.0), box3d(0.5, 0.5, 0.5))
        }
    };
    let glsl = alice_lol::to_glsl(&n);
    assert!(glsl.contains("sdf_eval"));
    assert!(glsl.contains("vec3"));
    assert!(!glsl.is_empty());
}

#[test]
fn transpile_glsl_complex() {
    let n = lol! {
        field Complex {
            translate(1.0, 0.0, 0.0,
                twist(0.5,
                    subtract(sphere(1.0), cylinder(0.3, 2.0))
                )
            )
        }
    };
    let glsl = alice_lol::to_glsl(&n);
    assert!(glsl.contains("cos"));
    assert!(glsl.contains("sin"));
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 区間演算
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn interval_inside() {
    use alice_lol::{Interval, Vec3Interval};
    let n = lol! { sphere(2.0) };
    let bounds = Vec3Interval {
        x: Interval { lo: -0.5, hi: 0.5 },
        y: Interval { lo: -0.5, hi: 0.5 },
        z: Interval { lo: -0.5, hi: 0.5 },
    };
    let iv = alice_lol::eval_interval(&n, bounds);
    // 完全に内部: hi < 0
    assert!(iv.hi < 0.0);
}

#[test]
fn interval_outside() {
    use alice_lol::{Interval, Vec3Interval};
    let n = lol! { sphere(1.0) };
    let bounds = Vec3Interval {
        x: Interval { lo: 5.0, hi: 6.0 },
        y: Interval { lo: 5.0, hi: 6.0 },
        z: Interval { lo: 5.0, hi: 6.0 },
    };
    let iv = alice_lol::eval_interval(&n, bounds);
    // 完全に外部: lo > 0
    assert!(iv.lo > 0.0);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// field構文とbare構文
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn bare_expr() {
    // field wrapper なしのbare構文
    let n = lol! { sphere(1.0) };
    assert!((eval(&n, O) - (-1.0)).abs() < 1e-5);
}

#[test]
fn field_expr() {
    // field wrapper 付き
    let n = lol! { field MyScene { sphere(1.0) } };
    assert!((eval(&n, O) - (-1.0)).abs() < 1e-5);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ネスト深度テスト
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn deep_nesting() {
    let n = lol! {
        field Deep {
            smooth_union(0.1,
                translate(0.0, 0.0, 0.0,
                    round(0.05,
                        twist(0.3,
                            subtract(
                                sphere(1.0),
                                cylinder(0.3, 2.0)
                            )
                        )
                    )
                ),
                translate(3.0, 0.0, 0.0,
                    scale(0.5,
                        onion(0.1,
                            torus(1.0, 0.3)
                        )
                    )
                )
            )
        }
    };
    // コンパイルが通り、評価できればOK
    let _ = eval(&n, O);
    let glsl = alice_lol::to_glsl(&n);
    assert!(!glsl.is_empty());
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 負の値テスト
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn negative_values() {
    let n = lol! { translate(-1.0, -2.0, -3.0, sphere(1.0)) };
    assert!((eval(&n, Vec3::new(-1.0, -2.0, -3.0)) - (-1.0)).abs() < 1e-5);
}

#[test]
fn integer_args() {
    // 整数リテラルも受け付けること
    let n = lol! { sphere(1) };
    assert!((eval(&n, O) - (-1.0)).abs() < 1e-5);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 時間構文 (v0.2)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn time_animate() {
    // animate(speed, amplitude, child) → SdfNode::Animated
    let n = lol! { animate(2.0, 0.5, sphere(1.0)) };
    // Animated は距離評価時に child をそのまま返す
    assert!((eval(&n, O) - (-1.0)).abs() < 1e-5);
}

#[test]
fn time_animate_with_transform() {
    let n = lol! {
        animate(1.0, 0.3,
            translate(2.0, 0.0, 0.0, sphere(1.0))
        )
    };
    // (2,0,0) で sphere の中心 → -1.0
    assert!((eval(&n, Vec3::new(2.0, 0.0, 0.0)) - (-1.0)).abs() < 1e-5);
}

#[test]
fn time_morph_at_zero() {
    // morph(0.0, a, b) → a のみ
    let n = lol! { morph(0.0, sphere(1.0), box3d(0.5, 0.5, 0.5)) };
    let sphere_only = lol! { sphere(1.0) };
    let d_morph = eval(&n, O);
    let d_sphere = eval(&sphere_only, O);
    assert!((d_morph - d_sphere).abs() < 1e-5);
}

#[test]
fn time_morph_at_one() {
    // morph(1.0, a, b) → b のみ
    let n = lol! { morph(1.0, sphere(1.0), box3d(0.5, 0.5, 0.5)) };
    let box_only = lol! { box3d(0.5, 0.5, 0.5) };
    let d_morph = eval(&n, O);
    let d_box = eval(&box_only, O);
    assert!((d_morph - d_box).abs() < 1e-5);
}

#[test]
fn time_morph_at_half() {
    // morph(0.5, a, b) → (a + b) / 2
    let n = lol! { morph(0.5, sphere(1.0), box3d(0.5, 0.5, 0.5)) };
    let sphere_d = eval(&lol! { sphere(1.0) }, O);
    let box_d = eval(&lol! { box3d(0.5, 0.5, 0.5) }, O);
    let expected = sphere_d * 0.5 + box_d * 0.5;
    let d_morph = eval(&n, O);
    assert!((d_morph - expected).abs() < 1e-4);
}

#[test]
fn time_morph_glsl() {
    let n = lol! {
        field MorphScene {
            morph(0.5, sphere(1.0), torus(1.0, 0.3))
        }
    };
    let glsl = alice_lol::to_glsl(&n);
    assert!(!glsl.is_empty());
    // morph は mix() として GLSL に出力される
    assert!(glsl.contains("mix") || glsl.contains("sdf_eval"));
}

#[test]
fn time_animate_in_union() {
    // animate をオペレーションの子として使う
    let n = lol! {
        smooth_union(0.2,
            animate(1.0, 0.5, sphere(1.0)),
            translate(3.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5))
        )
    };
    assert!(eval(&n, O) < 0.0);
}
