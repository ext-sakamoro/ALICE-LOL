//! 解析解突合 oracle — `stdlib::hardsurface::{cavity, mount}`
//!
//! `rules/analytic-oracle-tests.md` 準拠 既存の unit test は構築時の式を **自分で再計算して**
//! 比べるものが主で (`subtract_blind_pocket_extends_5mm_above_plate` は関数を呼ばず式だけを検算)、
//! 出来上がった形を測っていない
//!
//! 本 file は `eval` の符号変化を二分法で拾って幾何量を復元し、閉形式 / 文書の仕様と突合する
//! - cavity: 穴の半径・位置・貫通の余裕 (±5mm)・blind pocket の底の位置・countersink の 90° 円錐
//! - mount: L 字 bracket の寸法と内角 fillet、flange の穴の個数と PCD、rack の notch 数と位置、
//!   SKADIS peg の寸法、アルミプロファイルの 4 回対称と **断面が 1 つに繋がっていること**
//!
//! ⚠️ 既存実装の出力をそのまま pin した test は置かない 独立の参照は
//! (a) 文書の仕様 (doc comment の寸法・個数) (b) 幾何の閉形式 (円弧 / 90° 円錐 / 4 回対称)
//! (c) ISO の呼び径 (M2..M8 の数値そのもの) (d) 押出プロファイルが 1 個の立体であること

#![allow(clippy::suboptimal_flops)]
#![allow(clippy::manual_midpoint)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::if_not_else)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
// 走査の添字 (u / n 等) は n x n <= 210^2 で i32 に収まる
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::too_many_lines)]

use alice_lol::stdlib::hardsurface::cavity::{
    subtract_blind_heat_set, subtract_blind_pocket, subtract_through_counterbore,
    subtract_through_countersink, subtract_through_cylinder, subtract_through_screw_hole,
    through_hole_cylinder, CAVITY_PUNCH_MARGIN,
};
use alice_lol::stdlib::hardsurface::fastener::{
    MetricSize, CLEARANCE_H2D_FDM, HEAT_SET_SINK_MARGIN,
};
use alice_lol::stdlib::hardsurface::mount::{
    bracket_l, flange_circular, profile_2020, profile_3030, rack_shelf, skadis_peg_compat,
    SKADIS_PEG_H, SKADIS_PEG_W,
};
use alice_sdf::{eval, SdfNode};
use glam::Vec3;
use std::f32::consts::{SQRT_2, TAU};

const TOL: f32 = 2e-3;

fn approx(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

// ────────────────────────────────────────────────────────
// 計測器
// ────────────────────────────────────────────────────────

/// `from` から `dir` 方向に進み、SDF の符号が変わる位置までの距離 (見つからなければ panic)
///
/// 内部判定は `<= 0.0` (接する面では SDF がちょうど 0 になるため)
fn boundary(node: &SdfNode, from: Vec3, dir: Vec3, t_max: f32) -> f32 {
    let d = dir.normalize();
    let inside0 = eval(node, from) <= 0.0;
    let steps: u16 = 8192;
    let (mut lo, mut hi) = (0.0_f32, f32::NAN);
    for i in 1..=steps {
        let t = t_max * f32::from(i) / f32::from(steps);
        if (eval(node, from + d * t) <= 0.0) != inside0 {
            hi = t;
            break;
        }
        lo = t;
    }
    assert!(
        hi.is_finite(),
        "符号変化なし: from {from:?} dir {dir:?} t_max {t_max}"
    );
    for _ in 0..60 {
        let mid = f32::midpoint(lo, hi);
        if (eval(node, from + d * mid) <= 0.0) != inside0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    f32::midpoint(lo, hi)
}

fn solid(node: &SdfNode, p: Vec3) -> bool {
    eval(node, p) <= 0.0
}

/// 直線 `from + t * dir` 上で、固体でない区間 (穴) の数と、各区間の (中心, 幅)
fn gaps_along(node: &SdfNode, from: Vec3, dir: Vec3, len: f32, step: f32) -> Vec<(f32, f32)> {
    let d = dir.normalize();
    let mut out = Vec::new();
    let mut start: Option<f32> = None;
    let n = (len / step) as u32;
    for i in 0..=n {
        let t = step * i as f32;
        let empty = !solid(node, from + d * t);
        match (empty, start) {
            (true, None) => start = Some(t),
            (false, Some(s)) => {
                out.push((f32::midpoint(s, t - step), t - step - s));
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// y = 0 断面を n x n で走査して連結成分数を数える (4 近傍)
fn components_xz(node: &SdfNode, half: f32, n: usize) -> usize {
    let mut cells = vec![false; n * n];
    for i in 0..n {
        for j in 0..n {
            let x = -half + 2.0 * half * (i as f32 + 0.5) / n as f32;
            let z = -half + 2.0 * half * (j as f32 + 0.5) / n as f32;
            cells[i * n + j] = solid(node, Vec3::new(x, 0.0, z));
        }
    }
    let mut seen = vec![false; n * n];
    let mut comps = 0;
    for s in 0..n * n {
        if !cells[s] || seen[s] {
            continue;
        }
        comps += 1;
        let mut q = vec![s];
        seen[s] = true;
        while let Some(u) = q.pop() {
            let (i, j) = ((u / n) as i32, (u % n) as i32);
            for (di, dj) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let (a, b) = (i + di, j + dj);
                if a < 0 || b < 0 || a >= n as i32 || b >= n as i32 {
                    continue;
                }
                let v = (a * n as i32 + b) as usize;
                if cells[v] && !seen[v] {
                    seen[v] = true;
                    q.push(v);
                }
            }
        }
    }
    comps
}

/// ISO の呼び径 (M2..M8、数値そのもの) 実装の `nominal_diameter()` を信用しない独立の表
const ISO_NOMINAL: [(MetricSize, f32); 7] = [
    (MetricSize::M2, 2.0),
    (MetricSize::M2_5, 2.5),
    (MetricSize::M3, 3.0),
    (MetricSize::M4, 4.0),
    (MetricSize::M5, 5.0),
    (MetricSize::M6, 6.0),
    (MetricSize::M8, 8.0),
];

/// 厚い板 (半厚 20) から subtract して、穴の Y 方向の長さを板厚に邪魔されず測る
const fn tall_plate() -> SdfNode {
    SdfNode::Box3d {
        half_extents: Vec3::new(60.0, 20.0, 60.0),
    }
}

/// 厚さ 5 mm (半厚 2.5) の板
const fn plate_5mm() -> SdfNode {
    SdfNode::Box3d {
        half_extents: Vec3::new(30.0, 2.5, 30.0),
    }
}

// ════════════════════════════════════════════════════════
// cavity
// ════════════════════════════════════════════════════════

#[test]
fn through_cylinder_has_the_documented_radius_position_and_5mm_margin() {
    // 板厚 5 mm の貫通穴を、**板より厚い**母材から抜いて、穴の Y 方向の長さを測る
    let (x, z) = (12.0, -7.0);
    let cut = subtract_through_cylinder(tall_plate(), 4.2, 5.0, x, z);
    let center = Vec3::new(x, 0.0, z);
    // 半径: 軸に直交する 2 方向 (X / Z) で同じ値 (Y 軸の円柱)
    for dir in [Vec3::X, Vec3::Z, -Vec3::X, -Vec3::Z] {
        let r = boundary(&cut, center, dir, 10.0);
        assert!(approx(r, 2.1, TOL), "半径 {r} (期待 2.1)  dir {dir:?}");
    }
    // 長さ: 板厚 5 + 余裕 5 x 2 = 15 mm なので、中心から上下に 7.5 mm
    for dir in [Vec3::Y, -Vec3::Y] {
        let h = boundary(&cut, center, dir, 19.0);
        assert!(
            approx(h, 5.0 / 2.0 + CAVITY_PUNCH_MARGIN, TOL),
            "穴の半長 {h} (期待 板厚/2 + 余裕 = 7.5)"
        );
    }
    // 位置: 原点ではなく (x, z) に穴がある
    assert!(solid(&cut, Vec3::ZERO), "原点は母材のまま");
    assert!(!solid(&cut, center));
    // 穴の外側の母材は削られていない
    assert!(solid(&cut, center + Vec3::new(2.2, 0.0, 0.0)));
}

#[test]
fn through_hole_cylinder_is_the_same_cutter_as_the_subtracted_one() {
    // `through_hole_cylinder` は archetype が自前で translate / subtract するための刃
    let cutter = through_hole_cylinder(4.2, 5.0);
    let r = boundary(&cutter, Vec3::ZERO, Vec3::X, 10.0);
    let h = boundary(&cutter, Vec3::ZERO, Vec3::Y, 20.0);
    assert!(approx(r, 2.1, TOL) && approx(h, 7.5, TOL), "r {r} h {h}");
}

#[test]
fn screw_hole_radius_is_the_iso_nominal_plus_the_h2d_clearance_for_every_size() {
    assert!(
        approx(CLEARANCE_H2D_FDM, 0.2, 1e-6),
        "H2D FDM の余裕は 0.2 mm"
    );
    for (m, nominal) in ISO_NOMINAL {
        let cut = subtract_through_screw_hole(tall_plate(), m, 5.0, 3.0, 4.0);
        let r = boundary(&cut, Vec3::new(3.0, 0.0, 4.0), Vec3::X, 10.0);
        assert!(
            approx(r, (nominal + 0.2) * 0.5, TOL),
            "{m:?}: 半径 {r} (期待 {})",
            (nominal + 0.2) * 0.5
        );
    }
}

#[test]
fn counterbore_and_countersink_open_the_hole_at_the_requested_xz() {
    let (x, z) = (12.0, -7.0);
    for (name, cut) in [
        (
            "counterbore",
            subtract_through_counterbore(plate_5mm(), MetricSize::M4, 5.0, x, z),
        ),
        (
            "countersink",
            subtract_through_countersink(plate_5mm(), MetricSize::M4, 5.0, x, z),
        ),
    ] {
        // 穴の中心は上面でも下面でも空 (貫通)、鏡像の位置 (-x, -z) は板のまま
        for y in [2.4_f32, 0.0, -2.4] {
            assert!(
                !solid(&cut, Vec3::new(x, y, z)),
                "{name}: (x, {y}, z) が穴でない"
            );
            assert!(
                solid(&cut, Vec3::new(-x, y, -z)),
                "{name}: 鏡像位置が削られた"
            );
        }
        // 中央付近の軸 (呼び径 4 mm の軸穴) の半径は、少なくとも呼び径の半分より大きい
        let r_mid = boundary(&cut, Vec3::new(x, 0.0, z), Vec3::X, 10.0);
        assert!(
            r_mid >= 2.0,
            "{name}: 軸穴の半径 {r_mid} が呼び径 4 mm の半分未満"
        );
    }
}

#[test]
fn countersink_cone_has_a_90_degree_included_angle() {
    // ISO 10642: 皿頭の円錐は挟み角 90 度 = 半角 45 度 = 深さ 1 mm あたり半径 1 mm
    // 上面 (y = 2.5) から 0.3 / 1.3 mm 下での穴の半径を測る (円錐の範囲内)
    let cut = subtract_through_countersink(plate_5mm(), MetricSize::M4, 5.0, 0.0, 0.0);
    let r = |depth: f32| boundary(&cut, Vec3::new(0.0, 2.5 - depth, 0.0), Vec3::X, 12.0);
    let (r_shallow, r_deep) = (r(0.3), r(1.3));
    assert!(r_shallow > r_deep, "上ほど広いはず: {r_shallow} {r_deep}");
    assert!(
        approx(r_shallow - r_deep, 1.0, 5e-3),
        "深さ 1 mm あたりの半径の変化 {} (90 度円錐なら 1.0)",
        r_shallow - r_deep
    );
}

#[test]
fn blind_pocket_has_its_floor_at_top_minus_depth_and_keeps_the_bottom_closed() {
    let (dia, t, depth, x, z) = (10.0, 6.0, 4.0, 20.0, -15.0);
    let plate = SdfNode::Box3d {
        half_extents: Vec3::new(40.0, t * 0.5, 40.0),
    };
    let cut = subtract_blind_pocket(plate, dia, t, depth, x, z);
    let top = t * 0.5;
    // 底: 上方から降りてきて最初に固体に当たる高さ = top - depth
    let start = Vec3::new(x, top + 3.0, z);
    let floor_y = start.y - boundary(&cut, start, -Vec3::Y, 20.0);
    assert!(
        approx(floor_y, top - depth, TOL),
        "底 y = {floor_y} (期待 {})",
        top - depth
    );
    // 半径は pocket の中ほどで測る
    for dir in [Vec3::X, Vec3::Z] {
        let r = boundary(&cut, Vec3::new(x, top - depth * 0.5, z), dir, 12.0);
        assert!(approx(r, dia * 0.5, TOL), "半径 {r} (期待 {})", dia * 0.5);
    }
    // 下面は貫通していない (blind)
    assert!(
        solid(&cut, Vec3::new(x, -top + 0.01, z)),
        "下面が貫通している"
    );
    // 別の位置は削られていない
    assert!(solid(&cut, Vec3::new(-x, top - 0.01, -z)));
}

#[test]
fn blind_heat_set_pocket_follows_the_insert_dimensions() {
    // 径 = 挿入ナットの外径 + 余裕、深さ = 挿入ナットの長さ + 沈み余裕 (doc の式を公開値から組む)
    for m in [
        MetricSize::M2,
        MetricSize::M3,
        MetricSize::M4,
        MetricSize::M5,
    ] {
        let t = 12.0;
        let plate = SdfNode::Box3d {
            half_extents: Vec3::new(40.0, t * 0.5, 40.0),
        };
        let cut = subtract_blind_heat_set(plate, m, t, 5.0, 5.0);
        let want_r = (m.heat_set_insert_diameter() + CLEARANCE_H2D_FDM) * 0.5;
        let want_depth = m.heat_set_insert_depth() + HEAT_SET_SINK_MARGIN;
        let center = Vec3::new(5.0, t * 0.5 - want_depth * 0.5, 5.0);
        let r = boundary(&cut, center, Vec3::X, 12.0);
        assert!(approx(r, want_r, TOL), "{m:?}: 半径 {r} (期待 {want_r})");
        let start = Vec3::new(5.0, t * 0.5 + 2.0, 5.0);
        let floor_y = start.y - boundary(&cut, start, -Vec3::Y, 20.0);
        assert!(
            approx(floor_y, t * 0.5 - want_depth, TOL),
            "{m:?}: 底 y = {floor_y} (期待 {})",
            t * 0.5 - want_depth
        );
    }
}

#[test]
fn cavity_helpers_survive_degenerate_inputs() {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    // 退化した入力で panic しない (結果の SDF は有限)
    let cases: [(&str, f32, f32); 5] = [
        ("径 0", 0.0, 5.0),
        ("径 負", -3.0, 5.0),
        ("板厚 0", 4.0, 0.0),
        ("板厚 負", 4.0, -5.0),
        ("巨大", 1.0e6, 1.0e6),
    ];
    for (name, dia, t) in cases {
        let r = catch_unwind(AssertUnwindSafe(|| {
            let a = subtract_through_cylinder(plate_5mm(), dia, t, 1.0, 1.0);
            let b = subtract_blind_pocket(plate_5mm(), dia, t, 2.0, 1.0, 1.0);
            [eval(&a, Vec3::splat(0.1)), eval(&b, Vec3::splat(0.1))]
        }));
        let v = r.unwrap_or_else(|_| panic!("{name}: panic した"));
        assert!(v.iter().all(|x| x.is_finite()), "{name}: 非有限 {v:?}");
    }
}

// ════════════════════════════════════════════════════════
// mount: bracket_l
// ════════════════════════════════════════════════════════

/// 60 x 40 x 4 (厚) x 40 (奥行) の L 字 bracket
const BR: (f32, f32, f32, f32) = (60.0, 40.0, 4.0, 40.0); // 水平長 / 垂直高 / 板厚 / 奥行

#[test]
fn bracket_l_has_the_documented_extents() {
    let (hl, vh, t, depth) = BR;
    for fillet in [0.0_f32, 3.0] {
        let b = bracket_l(hl, vh, t, depth, fillet);
        // 水平板: X は ±hl/2、厚さは ±t/2、奥行は ±depth/2 (結合部から離れた位置で測る)
        let away = Vec3::new(10.0, 0.0, 0.0);
        assert!(
            approx(boundary(&b, away, Vec3::X, 40.0), hl * 0.5 - 10.0, TOL),
            "fillet {fillet}: +X 端"
        );
        assert!(
            approx(boundary(&b, away, -Vec3::Y, 10.0), t * 0.5, TOL),
            "fillet {fillet}: 水平板の下面"
        );
        assert!(
            approx(boundary(&b, away, Vec3::Z, 40.0), depth * 0.5, TOL),
            "fillet {fillet}: 奥行"
        );
        // 垂直板: 水平板の -X 端に立ち、上面から vh 立ち上がる (上端 y = vh + t/2)
        let vc = Vec3::new(-hl * 0.5 + t * 0.5, t * 0.5 + vh * 0.5, 0.0);
        assert!(solid(&b, vc), "fillet {fillet}: 垂直板の中心が空");
        assert!(
            approx(boundary(&b, vc, Vec3::Y, 40.0), vh * 0.5, TOL),
            "fillet {fillet}: 垂直板の上端"
        );
        assert!(
            approx(boundary(&b, vc, -Vec3::X, 10.0), t * 0.5, TOL),
            "fillet {fillet}: 垂直板の外面 (x = -hl/2)"
        );
        assert!(
            approx(boundary(&b, vc, Vec3::X, 10.0), t * 0.5, TOL),
            "fillet {fillet}: 垂直板の内面"
        );
    }
}

#[test]
fn bracket_l_fillet_only_adds_material_near_the_inner_corner() {
    // fillet は材料を足すだけで削らず、足すのは内角の近傍だけ (固体かどうかの符号で比べる)
    // ⚠️ 場の値そのものは `SmoothUnion` が表面から離れた所でも変える (ゼロ集合は変わらない)
    let (hl, vh, t, depth) = BR;
    let r = 3.0_f32;
    let sharp = bracket_l(hl, vh, t, depth, 0.0);
    let smooth = bracket_l(hl, vh, t, depth, r);
    let corner = Vec3::new(-hl * 0.5 + t, t * 0.5, 0.0);
    let mut added = 0;
    for ix in -120_i32..=120 {
        for iy in -20_i32..=100 {
            let p = Vec3::new(ix as f32 * 0.5, iy as f32 * 0.5, 3.0);
            let (a, b) = (solid(&sharp, p), solid(&smooth, p));
            assert!(!a || b, "{p:?}: fillet が材料を削った");
            if !a && b {
                added += 1;
                let d = (p.x - corner.x).hypot(p.y - corner.y);
                assert!(
                    d < 2.0 * r,
                    "{p:?}: 内角から {d:.2} mm 離れた点に材料が足された"
                );
            }
        }
    }
    assert!(added > 0, "fillet が何も足していない");
}

#[test]
fn bracket_l_inner_fillet_has_the_requested_circular_radius() {
    // 内角 R = fillet_radius (doc)  直角の内角に半径 R の円弧を付けると、角から対角線上の
    // 面までの距離は R(√2 - 1) (円弧の中心 (R, R) から R だけ手前)
    let (hl, vh, t, depth) = BR;
    for r in [3.0_f32, 6.0] {
        let b = bracket_l(hl, vh, t, depth, r);
        let corner = Vec3::new(-hl * 0.5 + t, t * 0.5, 0.0);
        let start = corner + Vec3::new(r * 2.0, r * 2.0, 0.0);
        let dir = (corner - start).normalize();
        let s = (start - corner).length() - boundary(&b, start, dir, 40.0);
        let want = r * (SQRT_2 - 1.0);
        assert!(
            (s - want).abs() / want < 0.02,
            "R={r}: 角から対角線上の面まで {s:.4} (円弧 R(√2-1) = {want:.4}、差 {:.1}%)",
            (s - want) / want * 100.0
        );
    }
}

// ════════════════════════════════════════════════════════
// mount: flange_circular
// ════════════════════════════════════════════════════════

#[test]
fn flange_has_the_documented_radii_thickness_and_bolt_pattern() {
    let (od, bore, t, pcd, dia) = (60.0_f32, 20.0, 6.0, 45.0, 4.2);
    for n in [3_u32, 4, 5, 6, 8] {
        let f = flange_circular(od, bore, t, pcd, n, dia);
        // 外径 / 中央穴 / 厚さ (bolt 穴と重ならない向きで測る)
        let between = Vec3::new(
            (TAU / (2.0 * n as f32)).cos(),
            0.0,
            (TAU / (2.0 * n as f32)).sin(),
        );
        assert!(
            approx(
                boundary(&f, between * 15.0, between, 40.0),
                od * 0.5 - 15.0,
                TOL
            ),
            "n={n}: 外径"
        );
        assert!(
            approx(boundary(&f, Vec3::ZERO, between, 20.0), bore * 0.5, TOL),
            "n={n}: 中央穴"
        );
        assert!(
            approx(boundary(&f, between * 20.0, Vec3::Y, 10.0), t * 0.5, TOL),
            "n={n}: 厚さ"
        );
        // bolt 穴: PCD の円上に n 個、各 2π/n、半径 dia/2
        let mut centers = 0;
        for k in 0..n {
            let a = TAU * k as f32 / n as f32;
            let c = Vec3::new(pcd * 0.5 * a.cos(), 0.0, pcd * 0.5 * a.sin());
            assert!(!solid(&f, c), "n={n}: 穴 {k} が空でない");
            let r = boundary(&f, c, Vec3::new(a.cos(), 0.0, a.sin()), 6.0);
            assert!(approx(r, dia * 0.5, TOL), "n={n}: 穴 {k} の半径 {r}");
            centers += 1;
        }
        assert_eq!(centers, n);
        // PCD の円周を一周して、穴の数がちょうど n (余分な穴が無い)
        let mut runs = 0;
        let mut prev = !solid(&f, Vec3::new(pcd * 0.5, 0.0, 0.0));
        for i in 1..=2880_u32 {
            let a = TAU * i as f32 / 2880.0;
            let now = !solid(&f, Vec3::new(pcd * 0.5 * a.cos(), 0.0, pcd * 0.5 * a.sin()));
            if now && !prev {
                runs += 1;
            }
            prev = now;
        }
        assert_eq!(runs, n, "n={n}: PCD 円上の穴の数");
    }
}

#[test]
fn flange_without_a_center_bore_is_solid_at_the_center() {
    let f = flange_circular(60.0, 0.0, 6.0, 45.0, 4, 4.2);
    assert!(solid(&f, Vec3::ZERO));
    // 負の中央穴径も「穴なし」(0 以下は穴を開けない)
    assert!(solid(
        &flange_circular(60.0, -5.0, 6.0, 45.0, 4, 4.2),
        Vec3::ZERO
    ));
}

// ════════════════════════════════════════════════════════
// mount: rack_shelf
// ════════════════════════════════════════════════════════

#[test]
fn rack_shelf_has_two_n_plus_one_notches_at_multiples_of_the_pitch() {
    // doc: notch は ピッチ間隔で 2 * count + 1 個、板長辺 (X) に沿って中央配置
    let (length, t, width, pitch, dia) = (200.0_f32, 5.0, 30.0, 25.0, 6.0);
    for n in [1_u32, 2, 3] {
        let r = rack_shelf(length, t, width, pitch, dia, n);
        let gaps = gaps_along(
            &r,
            Vec3::new(-length * 0.5 + 0.1, 0.0, 0.0),
            Vec3::X,
            length - 0.2,
            0.05,
        );
        let expected = (2 * n + 1) as usize;
        assert_eq!(
            gaps.len(),
            expected,
            "n={n}: notch 数 {} (期待 {expected})  gaps {gaps:?}",
            gaps.len()
        );
        for (k, (center, w)) in gaps.iter().enumerate() {
            let want = (k as f32 - n as f32) * pitch;
            let x = center - length * 0.5 + 0.1;
            assert!(
                approx(x, want, 0.1),
                "n={n}: notch {k} の中心 {x} (期待 {want})"
            );
            assert!(
                approx(*w, dia, 0.15),
                "n={n}: notch {k} の幅 {w} (期待 {dia})"
            );
        }
    }
}

#[test]
fn rack_shelf_plate_has_the_documented_extents() {
    let (length, t, width) = (200.0_f32, 5.0, 30.0);
    let r = rack_shelf(length, t, width, 25.0, 6.0, 3);
    let p = Vec3::new(12.5, 0.0, 0.0); // notch の間
    assert!(solid(&r, p));
    // 全長は notch (z = 0 の線上) を避けた z = 10 の線で測る
    let off = Vec3::new(12.5, 0.0, 10.0);
    assert!(
        approx(boundary(&r, off, Vec3::X, 120.0), length * 0.5 - 12.5, TOL),
        "全長"
    );
    assert!(approx(boundary(&r, p, Vec3::Y, 10.0), t * 0.5, TOL), "板厚");
    assert!(
        approx(boundary(&r, p, Vec3::Z, 30.0), width * 0.5, TOL),
        "幅"
    );
    // notch は板厚方向に貫通する (Y)
    for y in [t * 0.5 - 0.01, 0.0, -t * 0.5 + 0.01] {
        assert!(
            !solid(&r, Vec3::new(0.0, y, 0.0)),
            "y={y}: notch が貫通していない"
        );
    }
}

// ════════════════════════════════════════════════════════
// mount: skadis_peg_compat
// ════════════════════════════════════════════════════════

#[test]
fn skadis_peg_has_the_documented_size() {
    // doc: 幅 SKADIS_PEG_W - FDM_CLEARANCE (4.8)、高 SKADIS_PEG_H (15)、厚 board_thickness
    for board in [5.0_f32, 8.0] {
        let peg = skadis_peg_compat(board);
        let w = 2.0 * boundary(&peg, Vec3::ZERO, Vec3::X, 30.0);
        let h = 2.0 * boundary(&peg, Vec3::ZERO, Vec3::Y, 40.0);
        let d = 2.0 * boundary(&peg, Vec3::ZERO, Vec3::Z, 30.0);
        assert!(
            approx(w, SKADIS_PEG_W - 0.2, 0.05),
            "board {board}: 幅 {w} (期待 4.8)"
        );
        assert!(
            approx(h, SKADIS_PEG_H, 0.05),
            "board {board}: 高さ {h} (期待 15)"
        );
        assert!(
            approx(d, board, 0.05),
            "board {board}: 厚さ {d} (期待 {board})"
        );
    }
}

// ════════════════════════════════════════════════════════
// mount: アルミプロファイル
// ════════════════════════════════════════════════════════

fn rot_y(p: Vec3, quarter_turns: u32) -> Vec3 {
    let mut q = p;
    for _ in 0..quarter_turns {
        q = Vec3::new(q.z, q.y, -q.x);
    }
    q
}

#[test]
fn profiles_are_four_fold_and_mirror_symmetric() {
    // 4 面に同じ T スロットを回転コピーするので、場は Y 軸まわり 90 度の回転と鏡映で不変
    for (name, p, half) in [
        ("2020", profile_2020(40.0), 10.0_f32),
        ("3030", profile_3030(40.0), 15.0),
    ] {
        let mut state: u32 = 0x9E37_79B9;
        let mut next = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((state >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * (half + 2.0)
        };
        for _ in 0..400 {
            let pt = Vec3::new(next(), next() * 0.5, next());
            let f0 = eval(&p, pt);
            for q in 1..4 {
                let fq = eval(&p, rot_y(pt, q));
                assert!(
                    (f0 - fq).abs() < 1e-3,
                    "{name}: {pt:?} の 90 度 x {q} 回転で場が変わる ({f0} vs {fq})"
                );
            }
            // 各スロットは自分の中心線について左右対称 (z → -z の鏡映でも場は不変)
            let fm = eval(&p, Vec3::new(pt.x, pt.y, -pt.z));
            assert!(
                (f0 - fm).abs() < 1e-3,
                "{name}: {pt:?} を z で鏡映すると場が変わる ({f0} vs {fm})"
            );
        }
    }
}

#[test]
fn profile_cross_sections_are_one_connected_solid() {
    // 押出プロファイルは 1 個の立体 (断面が 1 つの連結成分)  T スロットと中央穴が肉を食い
    // 切ると、角のブロックがバラバラの浮島になる
    let mut failures = Vec::new();
    for (name, p, half) in [
        ("2020", profile_2020(40.0), 10.5_f32),
        ("3030", profile_3030(40.0), 15.5),
    ] {
        let comps = components_xz(&p, half, 210);
        if comps != 1 {
            failures.push(format!("{name}: 断面が {comps} 個の連結成分に分かれている"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// 20 シリーズ T スロットの寸法 (供給元の寸法表、実装の定数ではなく独立の参照)
/// 開口幅 6.0 (6.0〜6.2)、首の厚み 1.8 (1.5〜1.8)、全深さ 6.1 (5.5〜6.1)、空洞の最大幅 11、斜壁 45 度
const DS_OPENING_HALF: f32 = 3.0;
const DS_NECK: f32 = 1.8;
const DS_DEPTH: f32 = 6.1;
const DS_CAVITY_HALF: f32 = 5.5;

#[test]
fn profile_t_slot_matches_the_supplier_cross_section() {
    // 表面から depth だけ内側の、z = 0 の線上の空間の半幅 (Z 方向) を測る
    // 首 (0 .. 1.8) は半幅 3、その奥の空洞は首の直下の半幅 5.5 から 45 度で狭まり、底 (6.1) で 5.5 - 4.3 = 1.2
    let mut failures = Vec::new();
    for (name, p, half) in [
        ("2020", profile_2020(40.0), 10.0_f32),
        ("3030", profile_3030(40.0), 15.0),
    ] {
        let half_width = |depth: f32| boundary(&p, Vec3::new(half - depth, 0.0, 0.0), Vec3::Z, 9.0);
        for depth in [0.5, 1.0, 1.7] {
            let got = half_width(depth);
            if !approx(got, DS_OPENING_HALF, TOL) {
                failures.push(format!(
                    "{name}: 首 (表面から {depth} mm) の半幅 {got:.3} (期待 {DS_OPENING_HALF})"
                ));
            }
        }
        for depth in [1.9, 3.0, 4.5, 6.0] {
            let want = DS_CAVITY_HALF - (depth - DS_NECK);
            let got = half_width(depth);
            if !approx(got, want, 5e-3) {
                failures.push(format!("{name}: 空洞 (表面から {depth} mm) の半幅 {got:.3} (期待 {want:.3}、45 度の斜壁)"));
            }
        }
        // 底: z = 0 の線上で、表面から奥へ最初に固体に当たるのは深さ 6.1
        let bottom = boundary(&p, Vec3::new(half - 0.5, 0.0, 0.0), -Vec3::X, 12.0) + 0.5;
        if !approx(bottom, DS_DEPTH, TOL) {
            failures.push(format!(
                "{name}: スロットの底 {bottom:.3} mm (期待 {DS_DEPTH})"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
