//! 解析解突合 oracle — `stdlib::hardsurface::skadis_sdf`
//!
//! `rules/analytic-oracle-tests.md` 準拠 既存の 21 unit test は有限性 / 個数 / 型の確認が主で、
//! 出来上がった形の寸法 (板の外形・穴の位置・壁厚・peg の向き) を測っていない
//!
//! 独立の参照は
//! (a) 文書 / 定数の寸法 (`SKADIS_*` / `CONTAINER_*` / `SHELF_*` / `CLIP_*`)
//! (b) 幾何の閉形式 (polyline の tube の距離 = 点と線分の距離 - r、stadium の寸法)
//! (c) 機能の要請 (peg は板に差し込むので、取付面の裏側に BOARD_T 以上突き出る)
//! (d) 押出 / 配置の対称性 (peg 穴の格子は原点について点対称)

#![allow(clippy::suboptimal_flops)]
#![allow(clippy::manual_midpoint)]
#![allow(clippy::many_single_char_names)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::if_not_else)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::similar_names)]

use alice_lol::stdlib::hardsurface::skadis_sdf::{
    capsule_polyline_sdf, skadis_clip_sdf, skadis_container_sdf, skadis_hook_l_sdf,
    skadis_panel_sdf, skadis_peg_and_shoulder, skadis_shelf_sdf, BOARD_T, CLIP_BODY_T, CLIP_LENGTH,
    CLIP_SLOT_W, CLIP_WIDTH, CONTAINER_BOTTOM_T, CONTAINER_D, CONTAINER_H, CONTAINER_W,
    CONTAINER_WALL_T, PEG_BLADE_T, SHELF_BACK_H, SHELF_BOTTOM_T, SHELF_D, SHELF_LIP_H,
    SHELF_PEG_SPACING, SHELF_W, SHOULDER_DEPTH, SHOULDER_H, SKADIS_CONN_INSET, SKADIS_CONN_SCREW_D,
    SKADIS_EDGE_MARGIN, SKADIS_GRID_OFFSET, SKADIS_GRID_PITCH, SKADIS_MOUNT_HOLE_R,
    SKADIS_PANEL_THICKNESS, SKADIS_PEG_H, SKADIS_PEG_W,
};
use alice_sdf::{eval, SdfNode};
use glam::{Vec2, Vec3};

const TOL: f32 = 2e-3;

fn approx(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

/// `from` から `dir` 方向に進み、SDF の符号が変わる位置までの距離 (見つからなければ panic)
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

// ════════════════════════════════════════════════════════
// peg + shoulder
// ════════════════════════════════════════════════════════

#[test]
fn peg_and_shoulder_has_the_documented_blade_and_shoulder() {
    // doc: blade は X = -BOARD_T .. 0、厚 PEG_BLADE_T (Y)、hook_width (Z)
    //      shoulder は X = -BOARD_T - SHOULDER_DEPTH .. -BOARD_T、高 SHOULDER_H (Y)
    for w in [5.0_f32, 8.0, 15.0] {
        let peg = skadis_peg_and_shoulder(w);
        let blade_c = Vec3::new(-BOARD_T * 0.5, 0.0, 0.0);
        assert!(
            approx(boundary(&peg, blade_c, Vec3::X, 10.0), BOARD_T * 0.5, TOL),
            "w={w}: blade の先端 x=0"
        );
        assert!(
            approx(
                boundary(&peg, blade_c, Vec3::Y, 10.0),
                PEG_BLADE_T * 0.5,
                TOL
            ),
            "w={w}: blade の厚さ"
        );
        assert!(approx(
            boundary(&peg, blade_c, -Vec3::Y, 10.0),
            PEG_BLADE_T * 0.5,
            TOL
        ));
        assert!(
            approx(boundary(&peg, blade_c, Vec3::Z, 20.0), w * 0.5, TOL),
            "w={w}: 幅"
        );
        let sh_c = Vec3::new(-BOARD_T - SHOULDER_DEPTH * 0.5, 0.0, 0.0);
        assert!(
            approx(
                boundary(&peg, sh_c, -Vec3::X, 10.0),
                SHOULDER_DEPTH * 0.5,
                TOL
            ),
            "w={w}: shoulder の奥"
        );
        assert!(
            approx(boundary(&peg, sh_c, Vec3::Y, 10.0), SHOULDER_H * 0.5, TOL),
            "w={w}: shoulder の高さ"
        );
        assert!(approx(boundary(&peg, sh_c, Vec3::Z, 20.0), w * 0.5, TOL));
        // blade と shoulder は繋がっている (間に隙間が無い)
        assert!(solid(&peg, Vec3::new(-BOARD_T, 0.0, 0.0)));
    }
}

// ════════════════════════════════════════════════════════
// capsule_polyline_sdf
// ════════════════════════════════════════════════════════

#[test]
fn polyline_strip_is_the_exact_flat_strip_distance() {
    // 閉形式: Bamboo の `LineString.buffer(R)` + 押出 = 面内で polyline への距離 - R、Z は ±w/2 の帯
    // 場は円柱の厳密な距離 g(d1, d2) = (max(d1, d2) < 0 なら max(d1, d2)、さもなくば |max(d1, 0), max(d2, 0)|)
    // d1 = min_i dist2D(p, seg_i) - R、d2 = |z| - w/2
    let pts = [
        Vec2::new(0.0, 0.0),
        Vec2::new(20.0, 0.0),
        Vec2::new(30.0, 10.0),
        Vec2::new(30.0, 30.0),
        Vec2::new(10.0, 40.0),
    ];
    for (r, w) in [(3.5_f32, 8.0_f32), (3.5, 5.0), (2.0, 9.0)] {
        let node = capsule_polyline_sdf(&pts, r, w);
        let reference = |p: Vec3| {
            let d1 = pts
                .windows(2)
                .map(|s| {
                    let (a, b) = (Vec2::new(s[0].x, s[0].y), Vec2::new(s[1].x, s[1].y));
                    let ab = b - a;
                    let q = Vec2::new(p.x, p.y);
                    let t = ((q - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                    (q - (a + ab * t)).length()
                })
                .fold(f32::INFINITY, f32::min)
                - r;
            let d2 = p.z.abs() - w * 0.5;
            if d1.max(d2) < 0.0 {
                d1.max(d2)
            } else {
                Vec2::new(d1.max(0.0), d2.max(0.0)).length()
            }
        };
        let mut state: u32 = 0xC0FF_EE11;
        let mut next = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / (1u32 << 24) as f32
        };
        for _ in 0..2000 {
            let p = Vec3::new(
                next() * 50.0 - 10.0,
                next() * 60.0 - 10.0,
                (next() * 2.0 - 1.0) * (w * 0.5 + 4.0),
            );
            let (got, want) = (eval(&node, p), reference(p));
            assert!(
                (got - want).abs() < 2e-3,
                "r={r} w={w}: {p:?} eval {got} != 閉形式 {want}"
            );
        }
        // hook_width は帯の厚み (Z) そのもの: 管の半径より広くても狭くても ±w/2 で切れる
        assert!(
            solid(&node, Vec3::new(10.0, 0.0, w * 0.5 - 0.01)),
            "r={r} w={w}: 帯の端が無い"
        );
        assert!(
            !solid(&node, Vec3::new(10.0, 0.0, w * 0.5 + 0.01)),
            "r={r} w={w}: 帯が広すぎる"
        );
        assert!(!solid(&node, Vec3::new(10.0, 0.0, -w * 0.5 - 0.01)));
    }
}

#[test]
fn capsule_polyline_with_fewer_than_two_points_is_empty() {
    // doc: pts.len() < 2 は空 SDF  (どの点も固体にならない)
    for pts in [vec![], vec![Vec2::new(1.0, 1.0)]] {
        let node = capsule_polyline_sdf(&pts, 3.0, 8.0);
        for p in [
            Vec3::new(5.0, 5.0, 0.0),
            Vec3::new(0.1, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
        ] {
            assert!(
                !solid(&node, p),
                "{} 点の polyline が {p:?} を固体にした",
                pts.len()
            );
        }
    }
}

// ════════════════════════════════════════════════════════
// hook (L 型): 到達量と肉厚
// ════════════════════════════════════════════════════════

#[test]
fn hook_l_reaches_75mm_with_a_7mm_tube_and_8mm_width() {
    // doc: reach 75 mm、root_t 7 mm (= 管の太さ)、hook_width 8 mm
    // 中心線は水平 arm (0,0)→(67,0) + 半径 8 の 1/4 円で (75, 8) へ → 管の半径 3.5
    let h = skadis_hook_l_sdf();
    // arm の厚さ (arm の中ほど x = 30 で測る): ±3.5
    let p = Vec3::new(30.0, 0.0, 0.0);
    assert!(
        approx(boundary(&h, p, Vec3::Y, 10.0), 3.5, TOL),
        "arm の上面"
    );
    assert!(
        approx(boundary(&h, p, -Vec3::Y, 10.0), 3.5, TOL),
        "arm の下面"
    );
    // 幅 8 mm (Z)
    assert!(
        approx(boundary(&h, p, Vec3::Z, 20.0), 4.0, TOL),
        "hook の幅"
    );
    // 先端: 1/4 円の終点 (75, 8) の上に管の半径 3.5 → y = 11.5、x の最大は 75 + 3.5
    let tip = Vec3::new(75.0, 8.0, 0.0);
    assert!(solid(&h, tip));
    assert!(
        approx(boundary(&h, tip, Vec3::Y, 10.0), 3.5, TOL),
        "先端の上端 y = 11.5"
    );
    assert!(
        approx(boundary(&h, tip, Vec3::X, 10.0), 3.5, TOL),
        "先端の右端 x = 78.5"
    );
}

// ════════════════════════════════════════════════════════
// panel
// ════════════════════════════════════════════════════════

const SIZE: f32 = 300.0;

fn panel() -> SdfNode {
    skadis_panel_sdf(SIZE, SKADIS_PANEL_THICKNESS, 5.0)
}

#[test]
fn panel_thickness_is_exactly_the_requested_one() {
    let p = panel();
    // peg 穴に当たらない点 (z = -10 の線上) で板厚を測る
    let c = Vec3::new(0.0, 0.0, -10.0);
    assert!(solid(&p, c));
    assert!(approx(
        boundary(&p, c, Vec3::Y, 10.0),
        SKADIS_PANEL_THICKNESS * 0.5,
        TOL
    ));
    assert!(approx(
        boundary(&p, c, -Vec3::Y, 10.0),
        SKADIS_PANEL_THICKNESS * 0.5,
        TOL
    ));
}

#[test]
fn panel_outer_size_is_the_requested_size_in_x_and_z() {
    // doc: `size` は一辺 (通常 300)  角丸 R は外周の角だけを丸める
    // `RoundedBox` は全方向に round_radius を足す (外寸 = half_extents + R) ので、
    // 内側の寸法で渡さないと一辺が size + 2R になる
    let p = panel();
    let c = Vec3::new(0.0, 0.0, -10.0); // peg / connector 穴の線を避ける
    let x_edge = boundary(&p, c, Vec3::X, 200.0);
    let x_edge_neg = boundary(&p, c, -Vec3::X, 200.0);
    assert!(
        approx(x_edge, SIZE * 0.5, 0.05) && approx(x_edge_neg, SIZE * 0.5, 0.05),
        "X の端 +{x_edge} / -{x_edge_neg} (期待 ±{})",
        SIZE * 0.5
    );
    let c2 = Vec3::new(-10.0, 0.0, 0.0);
    let z_edge = boundary(&p, c2, Vec3::Z, 200.0);
    assert!(
        approx(z_edge, SIZE * 0.5, 0.05),
        "Z の端 +{z_edge} (期待 {})",
        SIZE * 0.5
    );
}

/// 文書どおりの peg 穴の中心 (base: pitch 40 の格子、stagger: base を (20, 20) ずらす)
/// 使える範囲は size - 2 * EDGE_MARGIN で、中心がその外に出る穴は開けない
fn expected_peg_centers() -> Vec<(f32, f32)> {
    let limit = SIZE * 0.5 - SKADIS_EDGE_MARGIN; // 130
    let mut out = Vec::new();
    for (ox, oz) in [(0.0, 0.0), (SKADIS_GRID_OFFSET, SKADIS_GRID_OFFSET)] {
        for ix in -10_i32..=10 {
            for iz in -10_i32..=10 {
                let (cx, cz) = (
                    ix as f32 * SKADIS_GRID_PITCH + ox,
                    iz as f32 * SKADIS_GRID_PITCH + oz,
                );
                if cx.abs() <= limit && cz.abs() <= limit {
                    out.push((cx, cz));
                }
            }
        }
    }
    out
}

#[test]
fn panel_peg_holes_are_stadiums_of_the_documented_size() {
    let p = panel();
    // 中心 (0, 0) の穴: X 半幅 PEG_W/2、Z 半長 PEG_H/2、板厚を貫通
    let c = Vec3::ZERO;
    assert!(!solid(&p, c));
    assert!(
        approx(boundary(&p, c, Vec3::X, 10.0), SKADIS_PEG_W * 0.5, TOL),
        "穴の幅"
    );
    assert!(
        approx(boundary(&p, c, Vec3::Z, 20.0), SKADIS_PEG_H * 0.5, TOL),
        "穴の長さ"
    );
    for y in [-2.45_f32, 0.0, 2.45] {
        assert!(!solid(&p, Vec3::new(0.0, y, 0.0)), "y={y}: 貫通していない");
    }
    // stadium: 端は半円 (角が丸い) — 長方形の角 (2.4, 7.4) は穴の外、半円の内側 (0, 7.4) は穴の中
    assert!(
        solid(&p, Vec3::new(2.4, 0.0, 7.4)),
        "穴の角が丸くない (長方形のまま)"
    );
    assert!(!solid(&p, Vec3::new(0.0, 0.0, 7.4)));
}

#[test]
fn panel_has_a_peg_hole_at_every_documented_center() {
    let p = panel();
    let centers = expected_peg_centers();
    assert!(
        centers.len() > 40,
        "期待する穴が少なすぎる: {}",
        centers.len()
    );
    for &(cx, cz) in &centers {
        assert!(
            !solid(&p, Vec3::new(cx, 0.0, cz)),
            "({cx}, {cz}) に peg 穴が無い"
        );
    }
}

#[test]
fn panel_has_no_peg_hole_inside_the_edge_margin() {
    let p = panel();
    // base / stagger の格子点のうち、端の余白 (EDGE_MARGIN) の中に入るものには穴を開けない
    let limit = SIZE * 0.5 - SKADIS_EDGE_MARGIN;
    let mut stray = Vec::new();
    for (ox, oz) in [(0.0, 0.0), (SKADIS_GRID_OFFSET, SKADIS_GRID_OFFSET)] {
        for ix in -5_i32..=5 {
            for iz in -5_i32..=5 {
                let (cx, cz) = (
                    ix as f32 * SKADIS_GRID_PITCH + ox,
                    iz as f32 * SKADIS_GRID_PITCH + oz,
                );
                if (cx.abs() > limit || cz.abs() > limit)
                    && cx.abs() < 149.0
                    && cz.abs() < 149.0
                    && !solid(&p, Vec3::new(cx, 0.0, cz))
                {
                    // connector / mount 穴との重なりは除く (peg 穴の中心そのものが穴のときだけ報告)
                    stray.push((cx, cz));
                }
            }
        }
    }
    assert!(
        stray.is_empty(),
        "端の余白 {SKADIS_EDGE_MARGIN} mm の中に peg 穴がある: {stray:?}"
    );
}

#[test]
fn panel_peg_hole_pattern_is_point_symmetric() {
    // 穴の格子 (base + stagger) は原点について点対称 (p が穴なら -p も穴)
    // connector / mount 穴は Bamboo canonical の固定位置なので対象外 (peg 穴の中心だけを見る)
    let p = panel();
    let mut asym = Vec::new();
    for ix in -4_i32..=4 {
        for iz in -4_i32..=4 {
            for (ox, oz) in [(0.0, 0.0), (SKADIS_GRID_OFFSET, SKADIS_GRID_OFFSET)] {
                let (cx, cz) = (
                    ix as f32 * SKADIS_GRID_PITCH + ox,
                    iz as f32 * SKADIS_GRID_PITCH + oz,
                );
                let a = !solid(&p, Vec3::new(cx, 0.0, cz));
                let b = !solid(&p, Vec3::new(-cx, 0.0, -cz));
                if a != b {
                    asym.push((cx, cz));
                }
            }
        }
    }
    assert!(asym.is_empty(), "点対称でない peg 穴: {asym:?}");
}

#[test]
fn panel_connector_and_mount_holes_are_at_the_documented_positions() {
    let p = panel();
    let inset = SIZE * 0.5 - SKADIS_CONN_INSET; // 144
                                                // 4 角の connector 穴 (Ø2.7)
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let c = Vec3::new(sx * inset, 0.0, sz * inset);
        assert!(!solid(&p, c), "角の connector 穴 {c:?}");
        assert!(
            approx(
                boundary(&p, c, Vec3::X, 5.0),
                SKADIS_CONN_SCREW_D * 0.5,
                TOL
            ),
            "connector 穴の半径"
        );
    }
    // 壁掛け mount 穴 (Ø5): x = 60 / 150 / 220 (板の左端から)、上下辺
    for x in [60.0_f32, 150.0, 220.0] {
        for sz in [-1.0_f32, 1.0] {
            let c = Vec3::new(x - SIZE * 0.5, 0.0, sz * inset);
            assert!(!solid(&p, c), "mount 穴 {c:?}");
            assert!(
                approx(boundary(&p, c, Vec3::X, 6.0), SKADIS_MOUNT_HOLE_R, TOL),
                "mount 穴の半径"
            );
        }
    }
}

// ════════════════════════════════════════════════════════
// container
// ════════════════════════════════════════════════════════

#[test]
fn container_has_the_documented_inner_size_wall_and_bottom() {
    // 定数は内寸 (CONTAINER_W x D x H)、壁 1.6、底 1.6  外形 = 内寸 + 2 * 壁、総高 = H + 底
    let c = skadis_container_sdf();
    let total_h = CONTAINER_H + CONTAINER_BOTTOM_T;
    let wall_x = Vec3::new(0.0, 0.0, 0.0);
    // 空洞の中心 (原点付近) は空
    assert!(!solid(&c, wall_x));
    // 内寸: 空洞の中心から X / Z の内壁まで
    let cavity = Vec3::new(0.0, 0.0, 0.0);
    assert!(
        approx(boundary(&c, cavity, Vec3::X, 60.0), CONTAINER_W * 0.5, TOL),
        "内幅"
    );
    assert!(
        approx(boundary(&c, cavity, Vec3::Z, 60.0), CONTAINER_D * 0.5, TOL),
        "内奥行 (前面側)"
    );
    // 壁厚: 内壁の外側が固体になる幅
    let wall_start = Vec3::new(CONTAINER_W * 0.5 + 0.01, 0.0, 0.0);
    assert!(
        approx(
            boundary(&c, wall_start, Vec3::X, 10.0),
            CONTAINER_WALL_T - 0.01,
            TOL
        ),
        "側壁の厚さ"
    );
    // 底厚: 空洞の底から外形の底まで  外形の底 = -total_h/2
    let floor_y = boundary(&c, cavity, -Vec3::Y, 60.0); // 原点から床の上面まで
    let bottom_outer = total_h * 0.5; // 原点から外形の底まで
    assert!(
        approx(bottom_outer - floor_y, CONTAINER_BOTTOM_T, TOL),
        "底厚 {} (期待 {CONTAINER_BOTTOM_T})",
        bottom_outer - floor_y
    );
    // 内高: 床の上面から上端 (開口) まで = CONTAINER_H
    let top_y = total_h * 0.5;
    assert!(
        approx(top_y + floor_y, CONTAINER_H, TOL),
        "内高 (床から上端まで) {} (期待 {CONTAINER_H})",
        top_y + floor_y
    );
}

#[test]
fn container_pegs_stick_out_of_the_back_wall_into_the_board() {
    // 背面 (Z = -outer_d/2) の裏側に peg が BOARD_T 以上突き出て板に差し込める (機能の要請)
    // 文書は「背面ペグ 2 個」
    let c = skadis_container_sdf();
    let outer_d = 2.0 * CONTAINER_WALL_T + CONTAINER_D;
    let back_face = -outer_d * 0.5;
    // 背面より裏 (Z < back_face) に固体がどれだけ伸びているか (Y の高さを走査、X は全幅)
    let mut deepest = 0.0_f32;
    let mut pegs_x: Vec<f32> = Vec::new();
    for ix in -340..=340 {
        let x = ix as f32 * 0.1;
        let mut hit = false;
        for iy in -40..=40 {
            let y = iy as f32 * 1.0;
            for k in 1..=120 {
                let z = back_face - k as f32 * 0.1;
                if solid(&c, Vec3::new(x, y, z)) {
                    deepest = deepest.max(back_face - z);
                    hit = true;
                }
            }
        }
        if hit && pegs_x.last().is_none_or(|&l| (x - l).abs() > 8.0) {
            pegs_x.push(x);
        }
    }
    assert!(
        deepest >= BOARD_T - 0.2,
        "背面の裏側へ突き出す量 {deepest:.2} mm (peg は板厚 {BOARD_T} mm 以上突き出るはず)"
    );
    assert!(
        pegs_x.len() >= 2,
        "背面ペグが {} 個 (文書は 2 個): x = {pegs_x:?}",
        pegs_x.len()
    );
}

// ════════════════════════════════════════════════════════
// clip
// ════════════════════════════════════════════════════════

#[test]
fn clip_has_two_plates_around_the_documented_slot() {
    let c = skadis_clip_sdf();
    let mid = Vec3::new(0.0, -CLIP_LENGTH * 0.5, 0.0);
    // スロット: 中心線上が空、幅 CLIP_SLOT_W
    assert!(!solid(&c, mid), "スロットの中心が固体");
    assert!(
        approx(boundary(&c, mid, Vec3::X, 5.0), CLIP_SLOT_W * 0.5, TOL),
        "スロット半幅"
    );
    assert!(approx(
        boundary(&c, mid, -Vec3::X, 5.0),
        CLIP_SLOT_W * 0.5,
        TOL
    ));
    // 前板 / 背板の厚さ CLIP_BODY_T、全長 CLIP_LENGTH、幅 CLIP_WIDTH
    let front = Vec3::new(
        CLIP_SLOT_W * 0.5 + CLIP_BODY_T * 0.5,
        -CLIP_LENGTH * 0.5,
        0.0,
    );
    assert!(solid(&c, front));
    assert!(
        approx(boundary(&c, front, Vec3::X, 5.0), CLIP_BODY_T * 0.5, TOL),
        "前板の厚さ"
    );
    assert!(
        approx(boundary(&c, front, Vec3::Z, 20.0), CLIP_WIDTH * 0.5, TOL),
        "幅"
    );
    assert!(
        approx(boundary(&c, front, -Vec3::Y, 40.0), CLIP_LENGTH * 0.5, TOL),
        "全長 (下端)"
    );
}

// ════════════════════════════════════════════════════════
// shelf
// ════════════════════════════════════════════════════════

#[test]
fn shelf_has_the_documented_u_section() {
    let s = skadis_shelf_sdf();
    // 底板: 幅 SHELF_W、奥行 SHELF_D、厚 SHELF_BOTTOM_T (peg / 壁から離れた位置で測る)
    let p = Vec3::new(0.0, 0.0, 0.0);
    assert!(solid(&s, p));
    assert!(
        approx(boundary(&s, p, Vec3::Y, 10.0), SHELF_BOTTOM_T * 0.5, TOL),
        "底厚"
    );
    assert!(
        approx(boundary(&s, p, Vec3::X, 200.0), SHELF_W * 0.5, TOL),
        "幅"
    );
    // 背面の高さ / 前リップの高さ
    let back = Vec3::new(0.0, SHELF_BACK_H * 0.5, -SHELF_D * 0.5 + 0.8);
    assert!(solid(&s, back));
    assert!(
        approx(boundary(&s, back, Vec3::Y, 40.0), SHELF_BACK_H * 0.5, TOL),
        "背面の高さ"
    );
    let lip = Vec3::new(0.0, SHELF_LIP_H * 0.5, SHELF_D * 0.5 - 0.8);
    assert!(solid(&s, lip));
    assert!(
        approx(boundary(&s, lip, Vec3::Y, 40.0), SHELF_LIP_H * 0.5, TOL),
        "前リップの高さ"
    );
}

#[test]
fn shelf_pegs_stick_out_of_the_back_wall_at_the_documented_spacing() {
    // 2 peg が背面の裏側へ板厚 BOARD_T 以上突き出し、SHELF_PEG_SPACING 離れている
    let s = skadis_shelf_sdf();
    let back_face = -SHELF_D * 0.5;
    let mut deepest = 0.0_f32;
    let mut xs: Vec<f32> = Vec::new();
    for ix in -1300..=1300 {
        let x = ix as f32 * 0.1;
        let mut hit = false;
        for iy in 0..=30 {
            let y = iy as f32;
            for k in 1..=100 {
                let z = back_face - k as f32 * 0.1 - 0.001;
                if solid(&s, Vec3::new(x, y, z)) {
                    deepest = deepest.max(back_face - z);
                    hit = true;
                }
            }
        }
        if hit && xs.last().is_none_or(|&l| (x - l).abs() > 8.0) {
            xs.push(x);
        }
    }
    assert!(
        deepest >= BOARD_T - 0.2,
        "背面の裏側へ突き出す量 {deepest:.2} mm (peg は板厚 {BOARD_T} mm 以上突き出るはず)"
    );
    assert_eq!(xs.len(), 2, "背面ペグの数 {} (x = {xs:?})", xs.len());
    if xs.len() == 2 {
        assert!(
            approx(xs[1] - xs[0], SHELF_PEG_SPACING, 6.0),
            "ペグ間隔 {} (期待 {SHELF_PEG_SPACING})",
            xs[1] - xs[0]
        );
    }
    let _ = SKADIS_GRID_PITCH;
}
