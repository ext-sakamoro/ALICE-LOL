//! SKADIS の板: spec を持つ builder の試験
//!
//! 既存の `skadis_panel(...)` の文 (= `skadis_panel_sdf`) は builder の既定の spec に写し、
//! 出力の node は byte 一致のまま (下の golden は builder を入れる前の出力から記録した)

// 参照値 (generate.py の中心・格子・肉の幅) は generate.py と同じ順序の素の算術で作り、
// 板の値とは丸めの差まで含めて一致を見るので、比較は厳密 (float_cmp)、走査は float の while、
// 式は fma にしない (suboptimal_flops)、座標名は 1 文字 (many_single_char_names)
#![allow(
    clippy::float_cmp,
    clippy::while_float,
    clippy::suboptimal_flops,
    clippy::many_single_char_names,
    clippy::format_collect
)]

use alice_lol::stdlib::hardsurface::skadis_sdf::skadis_panel_sdf;
use sha2::{Digest, Sha256};

fn digest(node: &alice_sdf::SdfNode) -> String {
    let mut h = Sha256::new();
    h.update(format!("{node:?}").as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// (size, thickness, `corner_radius`) と builder 前の出力の digest
const CLASSIC_GOLDEN: &[((f32, f32, f32), &str)] = &[
    (
        (300.0, 5.0, 5.0),
        "2cd706bde146c41c016d0826f182aece2288bedad9e2faf0030648e65630c628",
    ),
    (
        (300.0, 5.0, 0.0),
        "189c214dec3913fa88c05495e7a9430c9e04e4418e42aff83f44b9bad3e81737",
    ),
    (
        (200.0, 4.0, 3.0),
        "72f7329d863a4b2e27ca3f56686ba7cfec43ca39a18d3ffaba761a5123d6fc7b",
    ),
    (
        (120.0, 5.0, 5.0),
        "5aa34692b42863173bf1d33b3b18e381aad6170ef9bafd7d296c131dcb0405c3",
    ),
];

#[test]
fn classic_panel_output_is_unchanged() {
    assert!(!CLASSIC_GOLDEN.is_empty(), "compared nothing");
    let mut bad = Vec::new();
    for &((s, t, r), want) in CLASSIC_GOLDEN {
        let got = digest(&skadis_panel_sdf(s, t, r));
        if got != want {
            bad.push(format!("({s}, {t}, {r}): {got}"));
        }
    }
    assert!(
        bad.is_empty(),
        "classic skadis_panel output moved:\n{}",
        bad.join("\n")
    );
}

// ── V10 (印刷の pipeline の generate.py) ──
//
// 期待値は generate.py の規則をこの試験の中で書き直して出す (builder の code を使わない)

use alice_lol::limits::SpecError;
use alice_lol::stdlib::hardsurface::skadis_sdf::{try_skadis_panel, SkadisPanelSpec};
use alice_sdf::{eval::eval, SdfNode};
use glam::Vec3;

const SIZE: f32 = 300.0;
const MARGIN: f32 = 20.0;
const FRAME: f32 = 12.0;

/// generate.py `get_peg_positions` (板の左下が原点の座標)
fn gp_pegs() -> Vec<(f32, f32)> {
    gp_pegs_m(MARGIN)
}

fn gp_pegs_m(margin: f32) -> Vec<(f32, f32)> {
    let mut v = Vec::new();
    for off in [0.0f32, 20.0] {
        let mut y = margin + off;
        while y <= SIZE - margin {
            let mut x = margin + off;
            while x <= SIZE - margin {
                v.push((x, y));
                x += 40.0;
            }
            y += 40.0;
        }
    }
    v
}

fn gp_mounts() -> Vec<(f32, f32)> {
    let mut v = Vec::new();
    for x in [60.0f32, 150.0, 220.0] {
        v.push((x, 6.0));
        v.push((x, SIZE - 6.0));
    }
    v
}

fn gp_conns() -> Vec<(f32, f32)> {
    let mut v = Vec::new();
    let mut x = 40.0f32;
    while x < SIZE {
        v.extend([(x, 6.0), (x, SIZE - 6.0), (6.0, x), (SIZE - 6.0, x)]);
        x += 40.0;
    }
    v.extend([
        (6.0, 6.0),
        (SIZE - 6.0, 6.0),
        (6.0, SIZE - 6.0),
        (SIZE - 6.0, SIZE - 6.0),
    ]);
    v
}

/// generate.py `get_empty_cells` + `build_panel` の keepout と frame の除外
fn gp_cutouts() -> Vec<(f32, f32)> {
    gp_cutouts_m(MARGIN).0
}

/// (作る cutout、mount の keepout だけで除いた cell)
type Cells = Vec<(f32, f32)>;

fn gp_cutouts_m(margin: f32) -> (Cells, Cells) {
    let pegs = gp_pegs_m(margin);
    let mounts = gp_mounts();
    let mut v = Vec::new();
    let mut kept_out = Vec::new();
    let mut y = margin;
    while y <= SIZE - margin {
        let mut x = margin;
        while x <= SIZE - margin {
            let is_peg = pegs.iter().any(|&(px, py)| px == x && py == y);
            let near_mount = mounts
                .iter()
                .any(|&(mx, my)| (x - mx).mul_add(x - mx, (y - my) * (y - my)).sqrt() < 25.0);
            let crosses = x - 14.0 < FRAME
                || x + 14.0 > SIZE - FRAME
                || y - 9.0 < FRAME
                || y + 9.0 > SIZE - FRAME;
            if !is_peg && near_mount && !crosses {
                kept_out.push((x, y));
            }
            if !is_peg && !near_mount && !crosses {
                v.push((x, y));
            }
            x += 20.0;
        }
        y += 20.0;
    }
    (v, kept_out)
}

/// generate.py の座標 → 板の座標 (面は X-Z、中心が原点)
fn at(p: (f32, f32), dx: f32, dz: f32) -> Vec3 {
    Vec3::new(p.0 - SIZE * 0.5 + dx, 0.0, p.1 - SIZE * 0.5 + dz)
}

fn v10() -> SdfNode {
    try_skadis_panel(&SkadisPanelSpec::v10(SIZE), 1.2).expect("v10 passes 1.2")
}

#[test]
fn v10_holes_are_open_at_every_generate_py_centre() {
    let n = v10();
    let mut counts = Vec::new();
    for (name, set) in [
        ("peg", gp_pegs()),
        ("cutout", gp_cutouts()),
        ("conn", gp_conns()),
        ("mount", gp_mounts()),
    ] {
        assert!(!set.is_empty(), "{name}: compared nothing");
        let open = set
            .iter()
            .filter(|&&p| eval(&n, at(p, 0.0, 0.0)) > 0.0)
            .count();
        counts.push(format!("{name} {open}/{}", set.len()));
        assert_eq!(open, set.len(), "{name}: {open}/{} open", set.len());
    }
    // 98 = generate.py の peg の数 (7 × 7 × 2)
    assert_eq!(gp_pegs().len(), 98, "{counts:?}");
}

#[test]
fn v10_cutouts_sit_only_where_generate_py_puts_them() {
    let n = v10();
    let cut = gp_cutouts();
    let pegs = gp_pegs();
    // peg でも cutout でもない副格子の cell (keepout / frame で除いた cell) の中心は肉
    let mut checked = 0;
    let mut y = MARGIN;
    while y <= SIZE - MARGIN {
        let mut x = MARGIN;
        while x <= SIZE - MARGIN {
            let p = (x, y);
            if !cut.contains(&p) && !pegs.contains(&p) {
                assert!(
                    eval(&n, at(p, 0.0, 0.0)) < 0.0,
                    "cell {p:?} cut but generate.py skips it"
                );
                checked += 1;
            }
            x += 20.0;
        }
        y += 20.0;
    }
    assert!(checked > 0, "compared nothing");
    // cutout は 28 × 18 R4: 縁の内 0.3 は空洞、外 0.3 は肉、角 (13.7, 8.7) は R4 で肉
    for &p in &cut {
        assert!(
            eval(&n, at(p, 13.7, 0.0)) > 0.0 && eval(&n, at(p, 0.0, 8.7)) > 0.0,
            "cutout {p:?} too small"
        );
        assert!(
            eval(&n, at(p, 14.3, 0.0)) < 0.0 && eval(&n, at(p, 0.0, 9.3)) < 0.0,
            "cutout {p:?} too large"
        );
        assert!(
            eval(&n, at(p, 13.7, 8.7)) < 0.0,
            "cutout {p:?} corner not R4"
        );
    }
}

#[test]
fn v10_pegs_are_5_3_by_15_3_stadia_and_the_corner_is_r9() {
    let n = v10();
    for &p in &gp_pegs() {
        // 幅 5.3 (半 2.65)、高さ 15.3 (半 7.65)、両端は半円 (R 2.65)
        assert!(
            eval(&n, at(p, 2.5, 0.0)) > 0.0 && eval(&n, at(p, 0.0, 7.5)) > 0.0,
            "peg {p:?} too small"
        );
        assert!(
            eval(&n, at(p, 2.8, 0.0)) < 0.0 && eval(&n, at(p, 0.0, 7.8)) < 0.0,
            "peg {p:?} too large"
        );
        // 半円の端: (2.0, 6.6) は中心 (0, 5) から 2.56 < 2.65 で空洞、(2.4, 6.9) は 2.95 で肉
        assert!(eval(&n, at(p, 2.0, 6.6)) > 0.0, "peg {p:?} end not round");
        assert!(eval(&n, at(p, 2.4, 6.9)) < 0.0, "peg {p:?} end is square");
    }
    // 角 R9 (円の中心 (141, 141)): (148, 148) は 9.9 で外 (R5 なら中心 (145, 145) から 4.2 で内)、
    // (146, 146) は 7.1 で内
    assert!(
        eval(&n, Vec3::new(148.0, 0.0, 148.0)) > 0.0,
        "corner is not R9"
    );
    assert!(
        eval(&n, Vec3::new(146.0, 0.0, 146.0)) < 0.0,
        "corner rounded more than R9"
    );
    // 厚み 5
    assert!(
        eval(&n, Vec3::new(0.0, 2.4, -140.0)) < 0.0 && eval(&n, Vec3::new(0.0, 2.6, -140.0)) > 0.0
    );
}

/// 板の面 (y = 0) の肉の点から、勾配の向きの前後に面まで進んだ距離の和 (= その点を通る肉の幅)
fn wall_through(n: &SdfNode, p: Vec3) -> Option<f32> {
    let h = 1e-3;
    let g = Vec3::new(
        eval(n, p + Vec3::X * h) - eval(n, p - Vec3::X * h),
        0.0,
        eval(n, p + Vec3::Z * h) - eval(n, p - Vec3::Z * h),
    );
    if g.length_squared() < 1e-12 {
        return None;
    }
    let dir = g.normalize();
    let mut sum = 0.0;
    for s in [1.0f32, -1.0] {
        let mut t = 0.0f32;
        let mut k = 0;
        loop {
            let d = eval(n, p + dir * (s * t));
            if d >= -1e-4 {
                break;
            }
            t += (-d).max(1e-3);
            k += 1;
            if k > 2000 {
                return None;
            }
        }
        sum += t;
    }
    Some(sum)
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "runtime only: 90k points × ray march, run in release"
)]
fn v10_thinnest_wall_sampled_on_the_sdf_agrees_with_the_builder() {
    let n = v10();
    let spec = SkadisPanelSpec::v10(SIZE);
    let analytic = spec.min_wall_mm().unwrap();
    let step = 0.5f32;
    let mut min = f32::INFINITY;
    let mut sampled = 0usize;
    let half = SIZE * 0.5;
    let mut x = -half + step * 0.5;
    while x < half {
        let mut z = -half + step * 0.5;
        while z < half {
            let p = Vec3::new(x, 0.0, z);
            let d = eval(&n, p);
            // 肉の点のうち、面から半 step 以内 (細い肉はどこでもこの帯を持つ)
            if d < 0.0 && d > -1.5 {
                if let Some(w) = wall_through(&n, p) {
                    min = min.min(w);
                    sampled += 1;
                }
            }
            z += step;
        }
        x += step;
    }
    assert!(sampled > 1000, "sampled {sampled}");
    // 2D で厳密な値 2.0 (斜めに隣り合う cutout の間: 20 - 18)
    assert!((analytic - 2.0).abs() < 1e-3, "analytic {analytic}");
    assert!(
        (min - analytic).abs() < 0.05,
        "sampled {min} vs analytic {analytic}"
    );
}

#[test]
fn a_bound_the_panel_cannot_meet_is_an_error_and_builds_nothing() {
    let spec = SkadisPanelSpec::v10(SIZE);
    match try_skadis_panel(&spec, 2.5) {
        Err(SpecError::Geometry(e)) => {
            assert_eq!(e.kind, "skadis_panel_wall");
            assert!((e.measured_mm - 2.0).abs() < 1e-3, "{e:?}");
        }
        other => panic!("2.5 mm bound on a 2.0 mm panel: {other:?}"),
    }
    // peg の clearance を広げると peg と cutout の間が 3.35 → 0.85 になる
    let mut wide = SkadisPanelSpec::v10(SIZE);
    wide.peg_clearance_mm = 5.3;
    assert!(matches!(
        try_skadis_panel(&wide, 1.2),
        Err(SpecError::Geometry(_))
    ));
    // 縁から 12 にすると最初の peg の下端が frame の connector 穴の近くに来る
    let mut near = SkadisPanelSpec::v10(SIZE);
    near.edge_margin = 9.0;
    assert!(
        matches!(try_skadis_panel(&near, 1.2), Err(SpecError::Geometry(_))),
        "{:?}",
        near.min_wall_mm()
    );
    // spec の数の field が数でない / 負の時は数の検査で落ちる (NaN の穴の位置は Law の min に捨てられ、
    // 肉の計算をすり抜けるので、計算の前に止める)
    for set in [
        |s: &mut SkadisPanelSpec| s.edge_margin = f32::NAN,
        |s: &mut SkadisPanelSpec| s.peg_clearance_mm = f32::NAN,
        |s: &mut SkadisPanelSpec| s.peg_clearance_mm = -0.1,
        |s: &mut SkadisPanelSpec| s.corner_radius = f32::INFINITY,
        |s: &mut SkadisPanelSpec| s.thickness = 0.0,
    ] {
        let mut bad = SkadisPanelSpec::v10(SIZE);
        set(&mut bad);
        assert!(
            matches!(try_skadis_panel(&bad, 1.2), Err(SpecError::FloatLimit(_))),
            "{bad:?} was not refused"
        );
        assert!(
            bad.min_wall_mm().is_err(),
            "{bad:?}: min_wall_mm accepted it"
        );
    }
    // 下限が数でない時は数の検査で落ちる
    assert!(matches!(
        try_skadis_panel(&spec, f32::NAN),
        Err(SpecError::FloatLimit(_))
    ));
    assert!(try_skadis_panel(&spec, 2.0).is_ok());
}

#[test]
fn v10_mount_keepout_removes_the_cells_near_a_mount() {
    // 縁の余白 22 では 1 列目の cell (y = 22) が frame に掛からず、mount の近くの cell は keepout
    // だけで除かれる (余白 20 では frame が先に除くので keepout が効く cell は無い)
    let mut spec = SkadisPanelSpec::v10(SIZE);
    spec.edge_margin = 22.0;
    let n = try_skadis_panel(&spec, 1.2).expect("margin 22 passes 1.2");
    let (cut, kept_out) = gp_cutouts_m(22.0);
    assert!(
        !kept_out.is_empty(),
        "no cell is removed by the keepout alone: the test exercises nothing"
    );
    assert!(!cut.is_empty(), "compared nothing");
    for &p in &kept_out {
        assert!(
            eval(&n, at(p, 0.0, 0.0)) < 0.0,
            "cell {p:?} within 25 mm of a mount is cut"
        );
    }
    for &p in &cut {
        assert!(eval(&n, at(p, 0.0, 0.0)) > 0.0, "cutout {p:?} missing");
    }
    for &p in &gp_pegs_m(22.0) {
        assert!(eval(&n, at(p, 0.0, 0.0)) > 0.0, "peg {p:?} missing");
    }
}

/// 角の丸みが `-0` の板も、正規形の文を読んで書き戻すと同じ文になる (fuzz `fuzz_lol_emit_parity` の
/// `skadis_panel(21,9,-0)`: 1 回目は `rounded_box(…, -0.0)`、読み直した 2 回目は `0.0` だった)
#[test]
fn a_negative_zero_corner_radius_emits_a_fixed_point() {
    use alice_lol::emit::to_lol_unstamped;
    use alice_lol::runtime_parser::parse_lol;
    for text in [
        "skadis_panel(21,9,-0)",
        "skadis_panel(300,5,-0)",
        "skadis_panel(300,5,0)",
    ] {
        let first = to_lol_unstamped(&parse_lol(text).unwrap()).unwrap();
        let second = to_lol_unstamped(&parse_lol(&first).unwrap()).unwrap();
        assert_eq!(first, second, "{text}");
        assert!(!first.contains("-0.0)"), "{text}: {first}");
    }
    assert_eq!(
        digest(&skadis_panel_sdf(300.0, 5.0, -0.0)),
        digest(&skadis_panel_sdf(300.0, 5.0, 0.0))
    );
}
