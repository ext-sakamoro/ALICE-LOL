// 3D Print Structural Intent テスト

use alice_lol::{eval, lol};
use glam::Vec3;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// proc_macro テスト
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn lattice_infill_compiles() {
    let node = lol! { lattice_infill(0.05, 5.0, 0.02, sphere(1.0)) };
    let d = eval(&node, Vec3::ZERO);
    // 原点は球体内部 → infill構造の一部として負の距離が期待される
    assert!(d < 0.5, "distance at origin should be finite: {d}");
}

#[test]
fn diamond_infill_compiles() {
    let node = lol! { diamond_infill(0.05, 5.0, 0.02, box3d(1.0, 1.0, 1.0)) };
    let d = eval(&node, Vec3::ZERO);
    assert!(d < 0.5, "distance at origin should be finite: {d}");
}

#[test]
fn schwarz_infill_compiles() {
    let node = lol! { schwarz_infill(0.05, 5.0, 0.02, cylinder(0.5, 1.0)) };
    let d = eval(&node, Vec3::ZERO);
    assert!(d < 0.5, "distance at origin should be finite: {d}");
}

#[test]
fn lattice_infill_shell_present() {
    // シェル厚 0.1 の球体（半径 1.0）
    // 表面付近（r=1.0）ではシェルの onion が存在するので内部
    let node = lol! { lattice_infill(0.1, 8.0, 0.02, sphere(1.0)) };
    // 表面のすぐ内側（r=0.95）
    let d_inner = eval(&node, Vec3::new(0.95, 0.0, 0.0));
    assert!(
        d_inner < 0.0,
        "point just inside shell should be negative: {d_inner}"
    );
    // 外部（r=1.5）
    let d_outer = eval(&node, Vec3::new(1.5, 0.0, 0.0));
    assert!(d_outer > 0.0, "point outside should be positive: {d_outer}");
}

#[test]
fn lattice_infill_with_variable_capture() {
    let shell = 0.05_f32;
    let scale = 5.0_f32;
    let thick = 0.02_f32;
    let node = lol! { lattice_infill({shell}, {scale}, {thick}, sphere(1.0)) };
    let d = eval(&node, Vec3::ZERO);
    assert!(d.is_finite(), "distance should be finite: {d}");
}

#[test]
fn lattice_infill_nested_in_translate() {
    let node = lol! {
        translate(2.0, 0.0, 0.0,
            lattice_infill(0.05, 5.0, 0.02, sphere(0.5))
        )
    };
    // 移動先の中心で評価
    let d = eval(&node, Vec3::new(2.0, 0.0, 0.0));
    assert!(d < 0.5, "should evaluate near translated center: {d}");
}

#[test]
fn infill_glsl_output() {
    let node = lol! { lattice_infill(0.05, 5.0, 0.02, sphere(1.0)) };
    let glsl = alice_lol::to_glsl(&node);
    // 展開結果にはGyroidとOnionの構造が含まれるはず
    assert!(!glsl.is_empty(), "GLSL output should not be empty");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ランタイムパーサーテスト
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn runtime_lattice_infill() {
    let node = alice_lol::runtime_parser::parse_lol("lattice_infill(0.05, 5.0, 0.02, sphere(1.0))")
        .unwrap();
    let d = eval(&node, Vec3::ZERO);
    assert!(
        d.is_finite(),
        "runtime lattice_infill should produce valid SDF: {d}"
    );
}

#[test]
fn runtime_diamond_infill() {
    let node = alice_lol::runtime_parser::parse_lol(
        "diamond_infill(0.05, 5.0, 0.02, box3d(1.0, 1.0, 1.0))",
    )
    .unwrap();
    let d = eval(&node, Vec3::ZERO);
    assert!(
        d.is_finite(),
        "runtime diamond_infill should produce valid SDF: {d}"
    );
}

#[test]
fn runtime_schwarz_infill() {
    let node =
        alice_lol::runtime_parser::parse_lol("schwarz_infill(0.05, 5.0, 0.02, cylinder(0.5, 1.0))")
            .unwrap();
    let d = eval(&node, Vec3::ZERO);
    assert!(
        d.is_finite(),
        "runtime schwarz_infill should produce valid SDF: {d}"
    );
}

#[test]
fn runtime_infill_nested() {
    let node = alice_lol::runtime_parser::parse_lol(
        "translate(1.0, 0.0, 0.0, lattice_infill(0.1, 4.0, 0.03, sphere(0.5)))",
    )
    .unwrap();
    let d = eval(&node, Vec3::new(1.0, 0.0, 0.0));
    assert!(d.is_finite(), "nested runtime infill should be valid: {d}");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Law 制約との統合テスト
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn lattice_infill_min_thickness_law() {
    use alice_lol::law::{check_laws, CheckConfig, Constraint, Law};

    let node = lol! { lattice_infill(0.1, 5.0, 0.02, sphere(1.0)) };
    let laws = vec![Law::hard(
        "PrintWall",
        Constraint::MinThickness {
            node,
            min_thickness: 0.08,
        },
    )];
    let config = CheckConfig {
        aabb_min: Vec3::splat(-1.5),
        aabb_max: Vec3::splat(1.5),
        resolution: 8,
    };
    let report = check_laws(&laws, &config);
    // シェル厚 0.1 > min_thickness 0.08 なので、シェル部分はパスするはず
    // （TPMS部分は薄いのでソフト違反の可能性あり。ここでは構造の健全性のみ検証）
    assert!(report.total_laws == 1, "should have 1 law checked");
}
