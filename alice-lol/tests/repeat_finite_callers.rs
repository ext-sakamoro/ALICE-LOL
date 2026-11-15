//! `RepeatFinite` (`repeat_finite`) は translate した点を中心に、軸ごとに添字を ±count/2 に切って
//! 並べる (角から n 個ではない) この読み違いで穴の大半が開いていなかった形が複数あったので、stdlib の
//! 全ての呼び出しと文書の例が、意図した中心の全部に複製を置くことを確かめる
//!
//! 呼び出しの一覧は src/stdlib を走査して数え、下の表の数と一致しなければ失敗する (新しい呼び出しは
//! 表に意図した中心の検査を足すまで通らない、走査 0 件も失敗)

use alice_lol::runtime_parser::parse_declared;
use alice_lol::stdlib::hardsurface::{mount::rack_shelf, reinforcement::honeycomb_infill};
use alice_sdf::{eval::eval, SdfNode};
use glam::Vec3;
use std::path::Path;

/// 直線 `from + t·dir` (t は 0..len) を `step` で標本し、空洞 (eval > 0) の連なりの数を数える
/// 格子の点だけでなく間も見る (半分に切れた複製や、点と点の間の余分な複製を見落とさない)
fn hollow_runs(n: &SdfNode, from: Vec3, dir: Vec3, len: f32, step: f32) -> usize {
    let (mut runs, mut inside) = (0, false);
    let mut t = 0.0f32;
    while t <= len {
        let h = eval(n, from + dir * t) > 0.0;
        if h && !inside {
            runs += 1;
        }
        inside = h;
        t += step;
    }
    runs
}

/// (file、その file の呼び出しの数) 検査は下の試験
const CALLERS: &[(&str, usize)] = &[
    ("stdlib/hardsurface/mount.rs", 1),         // rack_shelf
    ("stdlib/hardsurface/reinforcement.rs", 1), // honeycomb_infill
];

fn scan(dir: &Path, out: &mut Vec<(String, usize)>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            scan(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            let text = std::fs::read_to_string(&p).unwrap();
            let n = text
                .lines()
                .filter(|l| {
                    let t = l.trim_start();
                    !t.starts_with("//")
                        && (t.contains("RepeatFinite {") || t.contains("repeat_finite("))
                })
                .count();
            if n > 0 {
                let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
                out.push((
                    p.strip_prefix(&src)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    n,
                ));
            }
        }
    }
}

#[test]
fn every_stdlib_caller_is_listed() {
    let mut found = Vec::new();
    scan(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src/stdlib"),
        &mut found,
    );
    found.sort();
    let mut want: Vec<(String, usize)> = CALLERS
        .iter()
        .map(|(f, n)| ((*f).to_string(), *n))
        .collect();
    want.sort();
    assert!(!found.is_empty(), "scanned no caller");
    assert_eq!(
        found, want,
        "a repeat_finite caller was added or removed: add its intended-centre check"
    );
}

#[test]
fn rack_shelf_puts_its_2n_plus_1_notches_centred() {
    // 文書: notch_count n で 2n + 1 個、pitch 25、原点中心 (レールは 300 で、±4 の位置も板の中)
    let n = rack_shelf(300.0, 5.0, 30.0, 25.0, 6.0, 3);
    for k in -3..=3 {
        assert!(
            eval(&n, Vec3::new(k as f32 * 25.0, 0.0, 0.0)) > 0.0,
            "notch {k} missing"
        );
    }
    for k in [-4.0f32, 4.0] {
        assert!(
            eval(&n, Vec3::new(k * 25.0, 0.0, 0.0)) < 0.0,
            "extra notch at {k}"
        );
    }
    // 直線上の空洞は 7 個だけ (notch Ø6 の 1/40 刻み、点の間の複製・半分に切れた複製も数える)
    assert_eq!(
        hollow_runs(&n, Vec3::new(-140.0, 0.0, 0.0), Vec3::X, 280.0, 6.0 / 40.0),
        7
    );
}

#[test]
fn honeycomb_infill_puts_its_2c_plus_1_cells_centred() {
    let plate = SdfNode::Box3d {
        half_extents: Vec3::new(60.0, 5.0, 60.0),
    };
    let n = honeycomb_infill(plate, 8.0, 1.2, 2);
    let (sx, sz) = (8.0 * 3.0f32.sqrt(), 12.0);
    for k in -2..=2 {
        for l in -2..=2 {
            assert!(
                eval(&n, Vec3::new(k as f32 * sx, 0.0, l as f32 * sz)) > 0.0,
                "cell ({k}, {l}) missing"
            );
        }
    }
    assert!(eval(&n, Vec3::new(3.0 * sx, 0.0, 0.0)) < 0.0, "extra cell");
    // 中央の行と列の空洞はそれぞれ 5 個 (cell の 1/40 刻み)
    let step = 8.0 / 40.0;
    assert_eq!(
        hollow_runs(&n, Vec3::new(-59.0, 0.0, 0.0), Vec3::X, 118.0, step),
        5
    );
    assert_eq!(
        hollow_runs(&n, Vec3::new(0.0, 0.0, -59.0), Vec3::Z, 118.0, step),
        5
    );
}

/// LLM_REFERENCE.md「Staggered Grid」の例 (同じ文字列が文書にあることも確かめる)
const DOC_EXAMPLE: &str = "units(mm, print,
subtract(
box3d(150, 150, 2.5),
union(
translate(-10, -10, 0, repeat_finite(6, 6, 0, 40, 40, 0, rotate(90, 0, 0, cylinder(2.5, 3.5)))),
translate(10, 10, 0, repeat_finite(6, 6, 0, 40, 40, 0, rotate(90, 0, 0, cylinder(2.5, 3.5))))
)
)
)";

#[test]
fn the_documented_staggered_grid_opens_all_98_holes_and_no_other() {
    let doc =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../LLM_REFERENCE.md"))
            .unwrap();
    assert!(
        doc.contains(DOC_EXAMPLE),
        "LLM_REFERENCE.md no longer shows the checked example"
    );
    let n = parse_declared(DOC_EXAMPLE).expect("parses").program.sdf;
    let mut open = 0;
    let mut total = 0;
    for off in [0.0f32, 20.0] {
        for j in 0..7 {
            for i in 0..7 {
                let (x, y) = (
                    20.0 + off + 40.0 * i as f32 - 150.0,
                    20.0 + off + 40.0 * j as f32 - 150.0,
                );
                total += 1;
                open += usize::from(eval(&n, Vec3::new(x, y, 0.0)) > 0.0);
            }
        }
    }
    assert_eq!((open, total), (98, 98));
    // 格子の外 (8 個目の位置) には無い: 板を広げた同じ program で見る
    let wide = parse_declared(&DOC_EXAMPLE.replace("box3d(150, 150, 2.5)", "box3d(200, 200, 2.5)"))
        .expect("parses")
        .program
        .sdf;
    for (x, y) in [
        (-170.0f32, -10.0f32),
        (150.0, -10.0),
        (-150.0, 10.0),
        (170.0, 10.0),
    ] {
        assert!(
            eval(&wide, Vec3::new(x, y, 0.0)) < 0.0,
            "extra copy at ({x}, {y})"
        );
    }
    // 広げた板の行 y = -10 (1 つ目の格子) と y = 10 (2 つ目) の空洞はそれぞれ 7 個 (穴 Ø5 の 1/40 刻み)
    for y in [-10.0f32, 10.0] {
        assert_eq!(
            hollow_runs(&wide, Vec3::new(-199.0, y, 0.0), Vec3::X, 398.0, 5.0 / 40.0),
            7,
            "row y = {y}"
        );
    }
}
