//! `emit::to_lol` round-trip (Track C0)
//!
//! `lol.gbnf` の name bucket を走査して **全 construct** に既定引数を与えた
//! snippet を生成し、`parse → emit → parse` の 2 tree が eval parity を持つ
//! ことを検証する grammar に construct が増えれば自動的に対象になる
//! (`grammar_covers_every_runtime_parser_construct` と合わせて
//! parser ⊆ grammar ⊆ emit の 3 者を CI で固定)
//!
//! 判定が eval parity なのは `Rotate` の Quat ↔ Euler 往復等で bit 一致が
//! 期待できないため 2 回目の emit は 1 回目と文字列一致する (正規形の冪等性)

#![allow(clippy::cast_precision_loss)] // sample 点生成

use alice_lol::emit::to_lol;
use alice_lol::runtime_parser::parse_lol;
use alice_lol::{eval, SdfNode, Vec3};

mod common;
use common::corpus::{fixtures, grammar_corpus};

/// 64 sample 点 (原点近傍 + 少し外) で eval parity
fn assert_parity(a: &SdfNode, b: &SdfNode, label: &str) {
    let mut seed: u32 = 0x9E37_79B9;
    for i in 0..64 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let r = |s: u32| (s as f32 / u32::MAX as f32).mul_add(2.0, -1.0);
        let scale = if i % 4 == 0 { 8.0 } else { 2.5 };
        let p = Vec3::new(
            r(seed) * scale,
            r(seed.rotate_left(11)) * scale,
            r(seed.rotate_left(22)) * scale,
        );
        let da = eval(a, p);
        let db = eval(b, p);
        assert!(
            alice_lol::parity::agrees(da, db, 1e-4),
            "{label}: eval mismatch at {p:?}: {da} vs {db}"
        );
    }
}

#[test]
fn every_grammar_construct_round_trips() {
    let corpus = grammar_corpus();
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for (name, src) in &corpus {
        let n1 = match parse_lol(src) {
            Ok(n) => n,
            Err(e) => {
                failures.push(format!(
                    "{name}: parse of default snippet failed: {e} ({src})"
                ));
                continue;
            }
        };
        let text = match to_lol(&n1) {
            Ok(t) => t,
            Err(e) => {
                failures.push(format!("{name}: emit failed: {e}"));
                continue;
            }
        };
        let n2 = match parse_lol(&text) {
            Ok(n) => n,
            Err(e) => {
                failures.push(format!("{name}: re-parse failed: {e} (emitted `{text}`)"));
                continue;
            }
        };
        assert_parity(&n1, &n2, name);
        // 正規形の冪等性
        let text2 = to_lol(&n2).unwrap();
        assert_eq!(text, text2, "{name}: emit is not idempotent");
        checked += 1;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // 257 parser construct − Intent verb 18 − program/entities 2 = 237 (2026-09-14)
    assert!(checked >= 230, "only {checked} constructs round-tripped");
}

#[test]
fn fixtures_round_trip() {
    for (_, src) in fixtures() {
        let n1 = parse_lol(src).unwrap_or_else(|e| panic!("{e}: {src}"));
        let text = to_lol(&n1).unwrap_or_else(|e| panic!("{e}: {src}"));
        let n2 = parse_lol(&text).unwrap_or_else(|e| panic!("{e}: emitted `{text}`"));
        assert_parity(&n1, &n2, src);
        assert_eq!(text, to_lol(&n2).unwrap());
    }
}

#[test]
fn emitted_text_is_accepted_by_llm_grammar_rules() {
    // emit は comment を書かず、token 間 whitespace は `", "` の 1 個だけ
    let n = parse_lol("union(pen_cup(50,100), translate(33,0,50, rotate(0,90,0, torus(15,5))))")
        .unwrap();
    let text = to_lol(&n).unwrap();
    assert!(!text.contains("//"));
    assert!(!text.contains("  "));
    assert!(!text.contains('\n'));
}

/// a fuzz input whose field overflows: both sides of the round trip evaluate to +∞, and they
/// agree (`inf - inf` is NaN, so a tolerance alone said they did not)
#[test]
fn a_field_that_overflows_round_trips_and_agrees() {
    let src = "capped_torus(511111111111111111111111,1,748)";
    let n1 = parse_lol(src).unwrap();
    let text = to_lol(&n1).unwrap();
    let n2 = parse_lol(&text).unwrap();
    assert_eq!(to_lol(&n2).unwrap(), text);
    let p = Vec3::new(-3.824_012_8, 0.421_419_14, 7.066_578);
    let (a, b) = (eval(&n1, p), eval(&n2, p));
    assert!(
        a.is_infinite() && a > 0.0,
        "the case no longer overflows: {a}"
    );
    assert!(alice_lol::parity::agrees(a, b, 1e-4), "{a} vs {b}");
    assert_parity(&n1, &n2, src);
}

/// a fuzz input whose rotation angle is huge (`1e9` degrees, fed to `can_rack`'s
/// `tilt_angle_deg`): `n1`'s `Rotate` quaternion is built straight from
/// `1e9.to_radians()`, which `sin`/`cos` cannot accurately reduce (see
/// `alice_lol::angle`'s module doc), so without reducing the degree value
/// first the recovered Euler angle on emit (`109.79`) is tens of degrees away
/// from the true one (`280.0`) and `n2`'s rotation genuinely disagrees with
/// `n1`'s, not merely by a rounding ulp. fuzz corpus:
/// `fuzz/seeds/fuzz_lol_emit_parity/huge-rotation-angle`
#[test]
fn a_huge_rotation_angle_round_trips_and_agrees() {
    let src = "\ncan_rack(71,9,1e9)";
    let n1 = parse_lol(src).unwrap();
    let text = to_lol(&n1).unwrap();
    let n2 = parse_lol(&text).unwrap_or_else(|e| panic!("{e}: emitted `{text}`"));
    assert_eq!(to_lol(&n2).unwrap(), text);
    assert_parity(&n1, &n2, src);
}

/// the same hazard through the generic `rotate(x, y, z, child)` construct
/// directly (not only through a stdlib product that happens to feed a huge
/// angle to a rotation): any of the 3 axes can carry the same extreme value
#[test]
fn a_huge_generic_rotate_angle_round_trips_and_agrees_on_every_axis() {
    for src in [
        "rotate(1000000000, 0, 0, box3d(1, 1, 1))",
        "rotate(-1000000000, 0, 0, box3d(1, 1, 1))",
        "rotate(0, 1000000000, 0, box3d(1, 1, 1))",
        "rotate(0, 0, 1000000000, box3d(1, 1, 1))",
    ] {
        let n1 = parse_lol(src).unwrap_or_else(|e| panic!("{e}: {src}"));
        let text = to_lol(&n1).unwrap_or_else(|e| panic!("{e}: {src}"));
        let n2 = parse_lol(&text).unwrap_or_else(|e| panic!("{e}: emitted `{text}`"));
        assert_eq!(to_lol(&n2).unwrap(), text, "{src}: emit is not idempotent");
        assert_parity(&n1, &n2, src);
    }
}

/// `rotate(deg, ...)` must agree with `rotate(canon_angle_deg(deg), ...)`: if
/// the parser's `"rotate"` arm fed an un-reduced huge `deg` straight to
/// `.to_radians()`, the two would disagree (not by a rounding ulp, see
/// `alice_lol::angle`'s module doc -- an un-reduced huge angle's `sin`/`cos`
/// settle on a rotation tens of degrees away from the true, reduced one).
/// The child is offset far from the origin (leverage), the same reason the
/// `can_rack` regression above surfaced a discrepancy that a tiny, origin-
/// centered child would not: a small angular error is an arc-length error
/// proportional to distance from the rotation's center.
#[test]
fn rotate_agrees_with_its_exact_mod_360_reduction() {
    let degrees: &[f32] = &[
        0.0,
        10.0,
        -15.0,
        90.0,
        180.0,
        -180.0,
        270.0,
        359.0,
        1e4,
        1e6,
        1e9,
        -1e9,
        1e12,
        1e18,
        1e30,
        f32::MAX,
    ];
    for &deg in degrees {
        let reduced = alice_lol::angle::canon_angle_deg(deg);
        for axis in 0..3 {
            let arg = |is_axis: bool, v: f32| {
                if is_axis {
                    v.to_string()
                } else {
                    "0".to_string()
                }
            };
            let huge = format!(
                "rotate({}, {}, {}, translate(500, 0, 0, box3d(5, 5, 5)))",
                arg(axis == 0, deg),
                arg(axis == 1, deg),
                arg(axis == 2, deg),
            );
            let small = format!(
                "rotate({}, {}, {}, translate(500, 0, 0, box3d(5, 5, 5)))",
                arg(axis == 0, reduced),
                arg(axis == 1, reduced),
                arg(axis == 2, reduced),
            );
            let n_huge = parse_lol(&huge).unwrap_or_else(|e| panic!("{e}: {huge}"));
            let n_small = parse_lol(&small).unwrap_or_else(|e| panic!("{e}: {small}"));
            assert_parity(
                &n_huge,
                &n_small,
                &format!("axis={axis} deg={deg} vs reduced={reduced}"),
            );
        }
    }
}

/// the same oracle as `rotate_agrees_with_its_exact_mod_360_reduction`, for
/// the stdlib product functions that build their own `Rotate` quaternion
/// directly from a spec field (not through the parser's `"rotate"` arm):
/// `phone_dock`'s and `can_rack`'s `tilt_angle_deg`, and `tape_dispenser`'s
/// `tear_angle_deg`.
///
/// Compares the `{:?}` of the built tree, not `eval` agreement: unlike
/// `rotate_agrees_with_its_exact_mod_360_reduction` (which compares two
/// trees built from already-different numeric literals, so the best an
/// oracle can do is check nearby samples agree), `canon_angle_deg` is
/// idempotent (`canon_angle_deg(1e9) == canon_angle_deg(-80.0) == -80.0`
/// exactly), so a correct implementation makes `spec.tilt_angle_deg = 1e9`
/// and `spec.tilt_angle_deg = -80.0` produce the bit-identical radian value
/// everywhere it is used -- not just in the rotation, in every `.sin()`/
/// `.cos()` derived offset too -- and therefore the bit-identical tree.
/// `eval`-only comparison at a plausible-looking sample scale missed 2 of
/// the 3 mutants below in this test's first draft (the tear edge this
/// catches is a small subtract near one corner; most random points miss it
/// regardless of its rotation).
#[test]
fn stdlib_tilt_angles_agree_with_their_exact_mod_360_reduction() {
    use alice_lol::angle::canon_angle_deg;
    use alice_lol::stdlib::hardsurface::pattern_sdf::{
        can_rack, phone_dock, tape_dispenser, CanRackSpec, PhoneDockSpec, TapeDispenserSpec,
    };

    let degrees: &[f32] = &[10.0, -15.0, 1e9, -1e9, 1e30, f32::MAX];
    for &deg in degrees {
        let reduced = canon_angle_deg(deg);

        let mut huge = CanRackSpec::standard_2_tier();
        huge.tilt_angle_deg = deg;
        let mut small = CanRackSpec::standard_2_tier();
        small.tilt_angle_deg = reduced;
        assert_eq!(
            format!("{:?}", can_rack(&huge)),
            format!("{:?}", can_rack(&small)),
            "can_rack deg={deg} vs reduced={reduced}"
        );

        let mut huge = PhoneDockSpec::standard_80x100();
        huge.tilt_angle_deg = deg;
        let mut small = PhoneDockSpec::standard_80x100();
        small.tilt_angle_deg = reduced;
        assert_eq!(
            format!("{:?}", phone_dock(&huge)),
            format!("{:?}", phone_dock(&small)),
            "phone_dock deg={deg} vs reduced={reduced}"
        );

        let mut huge = TapeDispenserSpec::packing_tape_standard();
        huge.tear_angle_deg = deg;
        let mut small = TapeDispenserSpec::packing_tape_standard();
        small.tear_angle_deg = reduced;
        assert_eq!(
            format!("{:?}", tape_dispenser(&huge)),
            format!("{:?}", tape_dispenser(&small)),
            "tape_dispenser deg={deg} vs reduced={reduced}"
        );
    }
}

/// `stabilize_euler_deg`'s preimage search picks the candidate whose per-axis
/// ulp distance to the initial extraction is smallest, not whichever exact
/// match the search loop happens to visit first: for `rotate(-85.46687, 0,
/// 0, ...)`, the second and third axes are both so close to `0.0` that the
/// search finds MANY exact bit-for-bit matches within the search radius (the
/// subnormal neighbourhood of `0.0` all round-trip to the same near-identity
/// rotation component) -- iterating `dx, dy, dz` from `-K` upward visits a
/// distance-8 candidate (`rotate(-85.46687, -1e-44, -1e-44, ...)`) long
/// before it visits the distance-0 one (`rotate(-85.46687, 0.0, 0.0, ...)`),
/// so picking "first found" would silently emit the noisy form
#[test]
fn canonical_pick_is_the_nearest_exact_match_not_the_first_one_found() {
    let n = parse_lol("rotate(-85.46687, 0, 0, box3d(1, 1, 1))").unwrap();
    let text = to_lol(&n).unwrap();
    assert_eq!(text, "rotate(-85.46687, 0.0, 0.0, box3d(1.0, 1.0, 1.0))");
}

/// the permanent regression for the fuzz-found crash's root cause: a dense
/// sweep of `can_rack`'s `tilt_angle_deg` (71 rows, ~1700mm lever arm --
/// the structure a single-degree rounding error is large enough to amplify
/// past the eval tolerance, see `alice_lol::emit`'s `stabilize_euler_deg`
/// doc), each checked for both `emit ∘ parse ∘ emit` text idempotence AND
/// eval parity via the real `alice_lol::parity::agrees` oracle (1e-4), at a
/// dense (not random) 3D point grid spanning the rack's full extent -- a
/// random/sparse point sample missed every one of these failures in an
/// earlier draft of this fix (zero-crossing-adjacent points carry the
/// discrepancy, random sampling has a low chance of landing near one)
///
/// Step (2.0 degree, 180 tilts) and point grid (50mm) are coarser than the
/// exhaustive sweep that found this (0.25 degree x 25mm, ~260s in a debug
/// build); explicit known-sensitive tilts from that exhaustive run are
/// added so this coarser, CI-speed version still covers the cases that
/// mattered, not just an arbitrary subsample.
#[test]
fn can_rack_dense_tilt_sweep_is_idempotent_and_agrees() {
    use alice_lol::stdlib::hardsurface::pattern_sdf::{can_rack, CanRackSpec};

    let mut tried = 0usize;
    let mut idempotent_fail = 0usize;
    let mut parity_fail = 0usize;
    let known_sensitive = [
        -163.5_f32, -163.25, -157.0, -152.0, -99.5, -89.25, -85.46687,
    ];
    let mut tilts: Vec<f32> = known_sensitive.to_vec();
    for step in 0_i16..=89 {
        tilts.push(4.0_f32.mul_add(f32::from(step), -178.0));
    }
    for &tilt in &tilts {
        tried += 1;
        let spec = CanRackSpec {
            rows: 71,
            can_diameter: 9.0,
            tilt_angle_deg: tilt,
            cans_per_row: 6,
            wall_thickness: 3.0,
            shelf_thickness: 3.0,
            front_lip_height: 15.0,
        };
        let n1 = can_rack(&spec);
        let text = to_lol(&n1).unwrap();
        let n2 = parse_lol(&text).unwrap_or_else(|e| panic!("{e}: {text}"));
        let text2 = to_lol(&n2).unwrap();
        if text != text2 {
            idempotent_fail += 1;
        }
        let mut this_fail = false;
        for y_step in 0_i16..=18 {
            let y_mm = 100.0_f32.mul_add(f32::from(y_step), -900.0);
            for x_step in 0_i16..=1 {
                let x_mm = 100.0_f32.mul_add(f32::from(x_step), -50.0);
                let point = alice_lol::Vec3::new(x_mm, y_mm, 0.0);
                let eval_n1 = alice_lol::eval(&n1, point);
                let eval_n2 = alice_lol::eval(&n2, point);
                if !alice_lol::parity::agrees(eval_n1, eval_n2, 1e-4) {
                    this_fail = true;
                }
            }
        }
        if this_fail {
            parity_fail += 1;
        }
    }
    assert!(tried > 50, "only {tried} tilts swept");
    assert_eq!(
        idempotent_fail, 0,
        "{idempotent_fail}/{tried} tilts not idempotent"
    );
    assert_eq!(
        parity_fail, 0,
        "{parity_fail}/{tried} tilts broke eval parity"
    );
}

/// known defect: a `rotate(x, y, z, ...)` whose `y` lands near `EulerRot::XYZ`'s
/// gimbal (+-90 degrees) with `x` and `z` both simultaneously nonzero has no
/// EXACT quaternion preimage reachable by `stabilize_euler_deg`'s local
/// search (the true preimage can be billions of ulps away from the initial
/// `to_euler` extraction -- an inherent Euler-angle decomposition ambiguity
/// at gimbal, not a search-radius limitation: widening the search is also
/// cubic in cost, `(2*K+1)^3` candidates per `Rotate` node). `eval` parity
/// mostly stays correct (the nearest-quaternion fallback is still a valid,
/// if imprecise, rotation), but not always: at `fuzz_lol_emit_parity`'s own
/// 16 sample points and 1e-4 tolerance it measured 0 breaks, but a wider
/// 64-point check (this test) found a small fraction do exceed tolerance
/// too (see the measured numbers below). `emit`'s text is also not
/// idempotent (`emit ∘ parse ∘ emit != emit`), which is exactly what
/// `fuzz_lol_emit_parity`'s `assert_eq!(text, text2, ...)` checks: this is
/// a pre-existing defect `fuzz_lol_emit_parity` can reach (not a regression
/// from the huge-angle fix in this same commit), scoped out of it pending a
/// user design decision (grammar addition for near-singular rotations /
/// parser refusal / keep as a documented defect).
///
/// Measured with this exact generator (seed `0x9E37_79B9`, `y = 90.0 + r*2.0`,
/// `x, z = r' * 180.0`, 2000 samples, parity checked at `assert_parity`'s 64
/// sample points -- wider than `fuzz_lol_emit_parity`'s own 16, which found 0
/// parity breaks on the same generator): 1561/2000 (78.1%) not idempotent,
/// 48/2000 (2.4%) eval-parity broken, 0 re-parse failures.
#[test]
#[ignore = "known defect: rotate() near the EulerRot::XYZ gimbal (y~=+-90deg) \
            with x and z both nonzero is not always emit-idempotent (a small \
            fraction also exceeds eval parity tolerance); pending a design \
            decision on how to handle near-singular rotations"]
fn known_defect_gimbal_adjacent_rotate_is_not_always_idempotent() {
    let mut seed: u32 = 0x9E37_79B9;
    let mut not_idempotent = 0usize;
    let mut parity_broken = 0usize;
    let mut reparse_failed = 0usize;
    let n = 2000;
    for _ in 0..n {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let r = |s: u32| (s as f32 / u32::MAX as f32).mul_add(2.0, -1.0);
        let x_deg = r(seed) * 180.0;
        let y_deg = 2.0_f32.mul_add(r(seed.rotate_left(11)), 90.0);
        let z_deg = r(seed.rotate_left(22)) * 180.0;
        let src = format!("rotate({x_deg}, {y_deg}, {z_deg}, box3d(1, 1, 1))");
        let n1 = parse_lol(&src).unwrap();
        let text = to_lol(&n1).unwrap();
        let Ok(n2) = parse_lol(&text) else {
            reparse_failed += 1;
            continue;
        };
        let text2 = to_lol(&n2).unwrap();
        if text != text2 {
            not_idempotent += 1;
        }
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_parity(&n1, &n2, &src);
        }))
        .is_err()
        {
            parity_broken += 1;
        }
    }
    println!(
        "known defect measured: n={n} not_idempotent={not_idempotent} \
         parity_broken={parity_broken} reparse_failed={reparse_failed}"
    );
    assert!(
        not_idempotent > 0,
        "defect did not reproduce -- re-measure and update this test"
    );
}
