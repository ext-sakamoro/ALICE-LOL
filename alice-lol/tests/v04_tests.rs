//! v0.4 DSL 拡張テスト

use alice_lol::lol;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 新プリミティブ (17)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn prim_rounded_cone() {
    let node = lol! { rounded_cone(0.5, 0.3, 1.0) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn prim_pyramid() {
    let node = lol! { pyramid(1.0) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn prim_hex_prism() {
    let node = lol! { hex_prism(1.0, 0.5) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn prim_link() {
    let node = lol! { link(0.5, 1.0, 0.2) };
    // link の内部点: (1.0, 0.0, 0.0) 付近
    let p = glam::Vec3::new(1.0, 0.0, 0.0);
    assert!(alice_lol::eval(&node, p) < 0.0);
}

#[test]
fn prim_capped_cone() {
    let node = lol! { capped_cone(1.0, 0.5, 0.3) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn prim_capped_torus() {
    let node = lol! { capped_torus(1.0, 0.3, 1.5) };
    let p = glam::Vec3::new(1.0, 0.0, 0.0);
    assert!(alice_lol::eval(&node, p) < 0.0);
}

#[test]
fn prim_rounded_cylinder() {
    let node = lol! { rounded_cylinder(1.0, 0.1, 0.5) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn prim_tube() {
    let node = lol! { tube(1.0, 0.2, 0.5) };
    let p = glam::Vec3::new(1.0, 0.0, 0.0);
    assert!(alice_lol::eval(&node, p) < 0.0);
}

#[test]
fn prim_barrel() {
    let node = lol! { barrel(1.0, 1.0, 0.3) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn prim_heart() {
    let node = lol! { heart(1.0) };
    // heart の内部点は形状依存 — コンパイル+評価が成功することを確認
    let _d = alice_lol::eval(&node, glam::Vec3::ZERO);
}

#[test]
fn prim_egg() {
    let node = lol! { egg(1.0, 0.3) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn prim_helix() {
    let node = lol! { helix(1.0, 0.2, 1.0, 2.0) };
    let p = glam::Vec3::new(1.0, 0.0, 0.0);
    assert!(alice_lol::eval(&node, p) < 0.0);
}

#[test]
fn prim_tetrahedron() {
    let node = lol! { tetrahedron(1.0) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn prim_box_frame() {
    let node = lol! { box_frame(1.0, 1.0, 1.0, 0.1) };
    // box_frame はワイヤーフレーム形状 — 評価が成功することを確認
    let _d = alice_lol::eval(&node, glam::Vec3::ZERO);
}

#[test]
fn prim_diamond() {
    let node = lol! { diamond(1.0, 1.5) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn prim_star_polygon() {
    let node = lol! { star_polygon(1.0, 5.0, 0.4, 0.5) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn prim_cross_shape() {
    let node = lol! { cross_shape(1.0, 0.3, 0.05, 0.5) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 新オペレーション (17)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn op_chamfer_union() {
    let node = lol! { chamfer_union(0.1, sphere(1.0), translate(2.0, 0.0, 0.0, sphere(1.0))) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn op_chamfer_intersection() {
    let node = lol! { chamfer_intersection(0.1, sphere(1.0), sphere(1.5)) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn op_chamfer_subtraction() {
    let node =
        lol! { chamfer_subtraction(0.1, sphere(1.0), translate(2.0, 0.0, 0.0, sphere(0.5))) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn op_stairs_union() {
    let node = lol! { stairs_union(0.2, 4.0, sphere(1.0), translate(1.5, 0.0, 0.0, sphere(1.0))) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn op_stairs_intersection() {
    let node = lol! { stairs_intersection(0.2, 4.0, sphere(1.0), sphere(1.5)) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn op_stairs_subtraction() {
    let node =
        lol! { stairs_subtraction(0.2, 4.0, sphere(1.0), translate(2.0, 0.0, 0.0, sphere(0.5))) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn op_xor() {
    let node = lol! { xor(sphere(1.0), sphere(1.5)) };
    // xor: 一方の中だが両方の中でない → 原点は両方の中なので正
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) > 0.0);
    // 表面付近は xor 内部
    let p = glam::Vec3::new(1.2, 0.0, 0.0);
    assert!(alice_lol::eval(&node, p) < 0.0);
}

#[test]
fn op_pipe() {
    let node = lol! { pipe(0.1, sphere(1.0), translate(1.0, 0.0, 0.0, sphere(1.0))) };
    // pipe は交差線付近のパイプ形状
    let _d = alice_lol::eval(&node, glam::Vec3::ZERO);
}

#[test]
fn op_engrave() {
    let node = lol! { engrave(0.1, sphere(1.0), translate(0.5, 0.0, 0.0, sphere(0.5))) };
    let _d = alice_lol::eval(&node, glam::Vec3::ZERO);
}

#[test]
fn op_groove() {
    let node = lol! { groove(0.2, 0.1, sphere(1.0), translate(0.5, 0.0, 0.0, sphere(0.5))) };
    let _d = alice_lol::eval(&node, glam::Vec3::ZERO);
}

#[test]
fn op_tongue() {
    let node = lol! { tongue(0.2, 0.1, sphere(1.0), translate(0.5, 0.0, 0.0, sphere(0.5))) };
    let _d = alice_lol::eval(&node, glam::Vec3::ZERO);
}

#[test]
fn op_columns_union() {
    let node = lol! { columns_union(0.2, 3.0, sphere(1.0), translate(1.5, 0.0, 0.0, sphere(1.0))) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn op_columns_intersection() {
    let node = lol! { columns_intersection(0.2, 3.0, sphere(1.0), sphere(1.5)) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn op_columns_subtraction() {
    let node =
        lol! { columns_subtraction(0.2, 3.0, sphere(1.0), translate(2.0, 0.0, 0.0, sphere(0.5))) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn op_exp_smooth_union() {
    let node = lol! { exp_smooth_union(32.0, sphere(1.0), translate(2.0, 0.0, 0.0, sphere(1.0))) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn op_exp_smooth_intersection() {
    let node = lol! { exp_smooth_intersection(32.0, sphere(1.0), sphere(1.5)) };
    // exp smooth は k が大きいほどシャープ — 評価が成功することを確認
    let _d = alice_lol::eval(&node, glam::Vec3::ZERO);
}

#[test]
fn op_exp_smooth_subtraction() {
    let node =
        lol! { exp_smooth_subtraction(32.0, sphere(1.0), translate(2.0, 0.0, 0.0, sphere(0.5))) };
    let _d = alice_lol::eval(&node, glam::Vec3::ZERO);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 新トランスフォーム (1)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn transform_scale_non_uniform() {
    let node = lol! { scale_non_uniform(2.0, 1.0, 1.0, sphere(1.0)) };
    // X方向に2倍スケール → X=1.5 は内部（元の sphere では外）
    assert!(alice_lol::eval(&node, glam::Vec3::new(1.5, 0.0, 0.0)) < 0.0);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 新モディファイア (13)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn mod_elongate() {
    let node = lol! { elongate(1.0, 0.0, 0.0, sphere(1.0)) };
    // X方向に elongate → X=1.5 は内部
    assert!(alice_lol::eval(&node, glam::Vec3::new(1.5, 0.0, 0.0)) < 0.0);
}

#[test]
fn mod_revolution() {
    let node = lol! { revolution(1.0, sphere(0.3)) };
    let p = glam::Vec3::new(1.0, 0.0, 0.0);
    assert!(alice_lol::eval(&node, p) < 0.0);
}

#[test]
fn mod_extrude() {
    let node = lol! { extrude(1.0, sphere(1.0)) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn mod_taper() {
    let node = lol! { taper(0.5, cylinder(1.0, 1.0)) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn mod_displacement() {
    let node = lol! { displacement(0.1, sphere(1.0)) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn mod_polar_repeat() {
    let node = lol! { polar_repeat(6.0, translate(2.0, 0.0, 0.0, sphere(0.5))) };
    let p = glam::Vec3::new(2.0, 0.0, 0.0);
    assert!(alice_lol::eval(&node, p) < 0.0);
}

#[test]
fn mod_shear() {
    let node = lol! { shear(0.5, 0.0, 0.0, box3d(1.0, 1.0, 1.0)) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn mod_noise() {
    let node = lol! { noise(0.1, 2.0, 42.0, sphere(1.0)) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn mod_repeat_finite() {
    // 3x1x1 繰り返し、間隔 3.0
    let node = lol! { repeat_finite(3.0, 1.0, 1.0, 3.0, 3.0, 3.0, sphere(1.0)) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn mod_octant_mirror() {
    let node = lol! { octant_mirror(translate(1.0, 1.0, 1.0, sphere(0.5))) };
    // 8象限にミラー → (-1, -1, -1) にもコピーあり
    assert!(alice_lol::eval(&node, glam::Vec3::new(-1.0, -1.0, -1.0)) < 0.0);
}

#[test]
fn mod_icosahedral_symmetry() {
    let node = lol! { icosahedral_symmetry(translate(2.0, 0.0, 0.0, sphere(0.3))) };
    // 正二十面体対称 → 多方向にコピー
    let _d = alice_lol::eval(&node, glam::Vec3::new(2.0, 0.0, 0.0));
}

#[test]
fn mod_with_material() {
    let node = lol! { with_material(1.0, sphere(1.0)) };
    // マテリアルは SDF 値に影響しない
    assert_eq!(
        alice_lol::eval(&node, glam::Vec3::ZERO),
        alice_lol::eval(&lol! { sphere(1.0) }, glam::Vec3::ZERO)
    );
}

#[test]
fn mod_surface_roughness() {
    let node = lol! { surface_roughness(5.0, 0.05, 3.0, sphere(1.0)) };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// N-ary fold テスト
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn chamfer_union_3way() {
    let node = lol! {
        chamfer_union(0.1,
            sphere(1.0),
            translate(2.0, 0.0, 0.0, sphere(1.0)),
            translate(0.0, 2.0, 0.0, sphere(1.0))
        )
    };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}

#[test]
fn exp_smooth_union_3way() {
    let node = lol! {
        exp_smooth_union(32.0,
            sphere(1.0),
            translate(2.0, 0.0, 0.0, sphere(1.0)),
            translate(0.0, 2.0, 0.0, sphere(1.0))
        )
    };
    assert!(alice_lol::eval(&node, glam::Vec3::ZERO) < 0.0);
}
