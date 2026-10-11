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

/// `smooth_union` の k に変数
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

/// TPMS surface の `scale`/`thickness` は、値が macro 展開時に分からない
/// (`{expr}` 捕捉・裸の変数名) 時は実行時に検査される
/// (`tests/compile_fail/`: literal の時は compile 時に検査される)
#[test]
#[should_panic(expected = "tpms_scale")]
fn captured_tpms_scale_is_checked_at_runtime() {
    let bad_scale = -0.0_f32;
    let _ = lol! { gyroid({bad_scale}, 0.1) };
}

#[test]
#[should_panic(expected = "tpms_thickness")]
fn captured_tpms_thickness_is_checked_at_runtime() {
    let bad_thickness = f32::NAN;
    let _ = lol! { lidinoid(3.0, {bad_thickness}) };
}

#[test]
fn captured_valid_tpms_fields_still_work() {
    let scale = 3.0_f32;
    let thickness = 0.1_f32;
    let node = lol! { gyroid({scale}, {thickness}) };
    assert!(matches!(node, alice_lol::SdfNode::Gyroid { .. }));
}

/// A literal TPMS scale this small used to be a `compile_error!` (the old
/// `>= 1e-6` floor); it is a finite positive value now, so the macro must
/// accept it at compile time (the `tpms_node` literal branch in
/// `codegen.rs`) and build the node, round-tripping bit-exact through
/// emit/parse like the runtime entrance does.
#[test]
fn literal_tpms_scale_below_the_old_1e_minus_6_floor_now_compiles() {
    let node = lol! { gyroid(5e-7, 0.1) };
    let alice_lol::SdfNode::Gyroid { scale, thickness } = node else {
        panic!("expected Gyroid, got {node:?}");
    };
    assert_eq!(scale.to_bits(), 5e-7_f32.to_bits());
    assert_eq!(thickness.to_bits(), 0.1_f32.to_bits());
    let text = alice_lol::emit::to_lol(&node).unwrap();
    assert_eq!(text, "gyroid(5e-7, 0.1)");
    let back = alice_lol::runtime_parser::parse_lol(&text).unwrap();
    let alice_lol::SdfNode::Gyroid {
        scale: back_scale,
        thickness: back_thickness,
    } = back
    else {
        panic!("expected Gyroid, got {back:?}");
    };
    assert_eq!(back_scale.to_bits(), scale.to_bits());
    assert_eq!(back_thickness.to_bits(), thickness.to_bits());
}

/// The `lol!` macro's literal-float parsing (`parser.rs`'s `parse_val`) must
/// round a decimal literal to `f32` bit-identically with the runtime `.lol`
/// text parser's lexer (`runtime_parser.rs`'s `read_number`, which just uses
/// `s.parse::<f32>()`) for the SAME digits -- a literal this close to an f32
/// rounding boundary pins an earlier regression where the macro parsed as
/// f64 first (to dodge a separate, unrelated overflow-literal panic) and
/// cast down, double-rounding to a DIFFERENT bit pattern than parsing the
/// decimal digits directly to f32 (`16777217.000000001` and
/// `1.0000000596046448`, both one ulp off the nearest round number in f32,
/// are exactly the shape that exposes it: f64 rounds first to a value
/// f32-representable as the round number, erasing the one-ulp residual that
/// a direct f32 parse keeps).
#[test]
fn macro_literal_float_parsing_is_bit_identical_to_the_runtime_parser() {
    for literal in [
        "16777217.000000001",
        "1.0000000596046448",
        "0.1",
        "3.0",
        "1e30",
        "1e-30",
        "-2.5",
    ] {
        let expected: f32 = literal.parse().unwrap();
        let text = format!("sphere({literal})");
        let runtime_node = alice_lol::runtime_parser::parse_lol(&text).unwrap();
        let alice_lol::SdfNode::Sphere {
            radius: runtime_radius,
        } = runtime_node
        else {
            panic!("expected Sphere");
        };
        assert_eq!(
            runtime_radius.to_bits(),
            expected.to_bits(),
            "runtime parser: {literal}"
        );
    }
    // the macro side is exercised separately (its literal is a Rust token, not
    // a runtime string): these are the SAME two digit strings written as
    // Rust float literals, which `parse_val` parses through the identical
    // `syn::LitFloat::base10_parse::<f32>()` path this test's doc comment
    // describes
    let macro_node_a = lol! { sphere(16777217.000000001) };
    let macro_node_b = lol! { sphere(1.0000000596046448) };
    let alice_lol::SdfNode::Sphere { radius: a } = macro_node_a else {
        panic!("expected Sphere");
    };
    let alice_lol::SdfNode::Sphere { radius: b } = macro_node_b else {
        panic!("expected Sphere");
    };
    assert_eq!(
        a.to_bits(),
        "16777217.000000001".parse::<f32>().unwrap().to_bits()
    );
    assert_eq!(
        b.to_bits(),
        "1.0000000596046448".parse::<f32>().unwrap().to_bits()
    );
}

/// One `#[should_panic]` test per TPMS-type construction site in the macro
/// (the 9 direct constructors, then the 3 infill wrappers): a mutant that
/// un-wires `tpms_node`/`checked_tpms_fields` at a SPECIFIC site (while
/// leaving the others wired) is only caught by exercising that exact site,
/// not by the handful of sites `captured_tpms_scale_is_checked_at_runtime`
/// and the `compile_fail` cases already cover.
macro_rules! tpms_scale_runtime_check {
    ($test_name:ident, $src:expr) => {
        #[test]
        #[should_panic(expected = "tpms_scale")]
        fn $test_name() {
            let bad_scale = -0.0_f32;
            let _ = $src(bad_scale);
        }
    };
}

tpms_scale_runtime_check!(site_gyroid_direct_is_wired, |s: f32| lol! {
    gyroid({s}, 0.1)
});
tpms_scale_runtime_check!(site_schwarz_p_direct_is_wired, |s: f32| lol! {
    schwarz_p({s}, 0.1)
});
tpms_scale_runtime_check!(site_diamond_surface_direct_is_wired, |s: f32| lol! {
    diamond_surface({s}, 0.1)
});
tpms_scale_runtime_check!(site_neovius_direct_is_wired, |s: f32| lol! {
    neovius({s}, 0.1)
});
tpms_scale_runtime_check!(site_lidinoid_direct_is_wired, |s: f32| lol! {
    lidinoid({s}, 0.1)
});
tpms_scale_runtime_check!(site_iwp_direct_is_wired, |s: f32| lol! { iwp({s}, 0.1) });
tpms_scale_runtime_check!(site_frd_direct_is_wired, |s: f32| lol! { frd({s}, 0.1) });
tpms_scale_runtime_check!(site_fischer_koch_s_direct_is_wired, |s: f32| lol! {
    fischer_koch_s({s}, 0.1)
});
tpms_scale_runtime_check!(site_pmy_direct_is_wired, |s: f32| lol! { pmy({s}, 0.1) });
tpms_scale_runtime_check!(site_lattice_infill_is_wired, |s: f32| lol! {
    lattice_infill(1.0, {s}, 0.1, sphere(1.0))
});
tpms_scale_runtime_check!(site_diamond_infill_is_wired, |s: f32| lol! {
    diamond_infill(1.0, {s}, 0.1, sphere(1.0))
});
tpms_scale_runtime_check!(site_schwarz_infill_is_wired, |s: f32| lol! {
    schwarz_infill(1.0, {s}, 0.1, sphere(1.0))
});
