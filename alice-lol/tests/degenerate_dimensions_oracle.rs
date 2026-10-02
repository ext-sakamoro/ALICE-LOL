//! 退化した寸法・個数の oracle — `parse_lol` と `stdlib::hardsurface` の生成器
//!
//! `rules/analytic-oracle-tests.md` 準拠 LLM が生成した LOL テキストの数値はそのまま生成器に入る
//! 旧実装は個数 (`rows` / `cols` / `count`) と板の寸法に上限がなく、
//! `pill_organizer(1e6, 2, 3)` は stack overflow で abort (`catch_unwind` でも拾えない)、
//! `skadis_panel(1e30, 2, 1)` は穴を (size / 40)² 個確保して数 GB、`stairs_prim(.., 1e30, ..)` は
//! 評価器が 40 億段を走査した
//!
//! 独立の参照は
//! (a) 構成則の同値 (硬い union / subtract は結合則が成り立つので、線形 fold と balanced fold は
//!     同じ距離場になる)
//! (b) 契約 (上限以下は通る / 超えたら `ParseError` / 非有限は `ParseError`)
//! (c) 資源の上限 (最大個数でも評価が実用時間で終わり、test thread の 2 MiB stack で足りる)

#![allow(clippy::float_cmp)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::literal_string_with_formatting_args)]
#![allow(clippy::too_many_lines)]

mod common;

use alice_lol::runtime_parser::{
    parse_lol, MAX_SKADIS_PANEL_MM, MAX_STDLIB_COUNT, MAX_STDLIB_GRID_CELLS,
    MAX_STDLIB_SMOOTH_FOLD_COUNT,
};
use alice_lol::stdlib::hardsurface::cavity::{blind_heat_set_cutter, subtract_blind_heat_set};
use alice_lol::stdlib::hardsurface::fastener::MetricSize;
use alice_lol::{eval, SdfNode, Vec3};
use std::time::Instant;

const PROBE_POINTS: [Vec3; 5] = [
    Vec3::new(0.1, 0.1, 0.1),
    Vec3::new(3.0, -2.0, 7.0),
    Vec3::new(-12.0, 4.5, 9.0),
    Vec3::new(40.0, 40.0, 40.0),
    Vec3::new(-1e3, 5.0, 1e3),
];

fn eval_all(node: &SdfNode) {
    for p in PROBE_POINTS {
        let d = eval(node, p);
        assert!(!d.is_nan(), "NaN at {p:?}");
    }
}

/// (名前, 個数を `{}` に入れる式) — 単一の個数引数を取る生成器
const COUNT_FORMS: [(&str, &str); 6] = [
    ("token_well", "token_well(3.0, 2.0, {})"),
    ("wrench_holder", "wrench_holder(3.0, 2.0, {})"),
    ("socket_rail", "socket_rail(3.0, 2.0, {})"),
    ("spice_rack", "spice_rack({}, 2.0, 1.0)"),
    ("cotton_dispenser", "cotton_dispenser({}, 90.0, 100.0)"),
    ("battery_18650_holder", "battery_18650_holder({}, 2.0, 1.0)"),
];

#[test]
fn a_count_at_the_limit_builds_and_evaluates_in_bounded_time() {
    // 最大個数でも、木の深さは log2 に収まり (linear fold なら 2 MiB の test thread で足りない)
    // 評価は個数に比例する時間で終わる
    for (name, form) in COUNT_FORMS {
        let src = form.replace("{}", &format!("{MAX_STDLIB_COUNT}"));
        let start = Instant::now();
        let node = parse_lol(&src).unwrap_or_else(|e| panic!("{name} at the limit: {e:?}"));
        eval_all(&node);
        assert!(
            start.elapsed().as_secs() < 20,
            "{name} at the limit took {:?}",
            start.elapsed()
        );
    }
}

#[test]
fn a_count_beyond_the_limit_or_not_finite_is_a_parse_error() {
    for (name, form) in COUNT_FORMS {
        for bad in [
            format!("{}", MAX_STDLIB_COUNT + 1.0),
            "1e6".to_string(),
            "1e30".to_string(),
            "1e999".to_string(),
        ] {
            let src = form.replace("{}", &bad);
            assert!(
                parse_lol(&src).is_err(),
                "{name}: `{src}` must be rejected, not generated"
            );
        }
    }
}

/// (名前, rows と cols を `{r}` `{c}` に入れる式) — 格子の生成器
const GRID_FORMS: [(&str, &str); 4] = [
    ("egg_tray", "egg_tray({r}, {c}, 5.0)"),
    ("pill_organizer", "pill_organizer({r}, {c}, 20.0)"),
    ("boss_array", "boss_array({r}, {c}, 3.0, 5.0, 15.0, 3.0)"),
    ("heat_set_array", "heat_set_array({r}, {c}, 3.0, 20.0, 3.0)"),
];

fn grid(form: &str, r: f32, c: f32) -> String {
    grid_text(form, &format!("{r}"), &format!("{c}"))
}

fn grid_text(form: &str, r: &str, c: &str) -> String {
    form.replace("{r}", r).replace("{c}", c)
}

#[test]
fn a_grid_at_the_cell_limit_builds_and_evaluates() {
    // 64 × 64 = 4096 要素 (線形 fold なら深さ 4096 で stack overflow)
    let side = MAX_STDLIB_GRID_CELLS.sqrt();
    for (name, form) in GRID_FORMS {
        let start = Instant::now();
        let node = parse_lol(&grid(form, side, side)).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        eval_all(&node);
        assert!(
            start.elapsed().as_secs() < 60,
            "{name} took {:?}",
            start.elapsed()
        );
    }
}

#[test]
fn a_grid_beyond_the_cell_limit_is_a_parse_error() {
    // 各軸は個数の上限以下でも、積が要素数の上限を超える
    let over = (MAX_STDLIB_GRID_CELLS / MAX_STDLIB_COUNT).floor() + 1.0;
    assert!(over * MAX_STDLIB_COUNT > MAX_STDLIB_GRID_CELLS);
    for (name, form) in GRID_FORMS {
        let src = grid(form, over, MAX_STDLIB_COUNT);
        assert!(parse_lol(&src).is_err(), "{name}: `{src}` must be rejected");
        // 1 軸が上限を超えるものと、非有限
        assert!(
            parse_lol(&grid(form, 1.0, MAX_STDLIB_COUNT + 1.0)).is_err(),
            "{name}"
        );
        assert!(
            parse_lol(&grid_text(form, "1e30", "2.0")).is_err(),
            "{name}"
        );
        assert!(
            parse_lol(&grid_text(form, "2.0", "1e999")).is_err(),
            "{name}"
        );
    }
}

#[test]
fn heat_set_array_cutters_match_the_pocket_helper_and_the_array_is_symmetric() {
    // 旧実装は `subtract_blind_heat_set` を 1 個ずつ重ねていた 硬い subtract は
    // `max(a, -min(b, c)) == max(max(a, -b), -c)` なので、cutter を集めて 1 回引いても距離場は
    // 1 bit も変わらない (1) cutter は旧 helper が引いていた円柱と同じ
    let plate = SdfNode::Box3d {
        half_extents: Vec3::new(40.0, 3.0, 40.0),
    };
    let linear = subtract_blind_heat_set(plate.clone(), MetricSize::M3, 6.0, 5.0, -7.0);
    let balanced = SdfNode::Subtraction {
        a: std::sync::Arc::new(plate),
        b: std::sync::Arc::new(blind_heat_set_cutter(MetricSize::M3, 6.0, 5.0, -7.0)),
    };
    for p in PROBE_POINTS
        .iter()
        .chain(&[Vec3::new(5.0, 0.0, -7.0), Vec3::new(5.0, 3.0, -7.0)])
    {
        assert_eq!(
            eval(&linear, *p),
            eval(&balanced, *p),
            "cutter differs at {p:?}"
        );
    }
    // (2) 格子の pocket は原点について対称に並ぶ 板の面内 (Z-up 後の X / Y) の符号反転で距離場が
    // 変わらない 落とした / ずらした cutter は対称を壊す
    let node = parse_lol("heat_set_array(3.0, 4.0, 3.0, 20.0, 3.0)").unwrap();
    let mut probed = 0;
    for ix in -8..=8 {
        for iy in -6..=6 {
            for z in [-2.0f32, 0.0, 1.0, 1.4, 2.5] {
                let p = Vec3::new(ix as f32 * 4.7, iy as f32 * 4.1, z);
                let d = eval(&node, p);
                for q in [Vec3::new(-p.x, p.y, p.z), Vec3::new(p.x, -p.y, p.z)] {
                    let dq = eval(&node, q);
                    assert!(
                        (d - dq).abs() <= 1e-4,
                        "asymmetric: {p:?} = {d}, {q:?} = {dq}"
                    );
                }
                probed += 1;
            }
        }
    }
    assert!(probed > 1000);
}

#[test]
fn clamp_rack_limits_its_non_associative_smooth_fold() {
    let ok = format!("clamp_rack({MAX_STDLIB_SMOOTH_FOLD_COUNT}, 30.0, 150.0)");
    eval_all(&parse_lol(&ok).unwrap());
    let over = format!(
        "clamp_rack({}, 30.0, 150.0)",
        MAX_STDLIB_SMOOTH_FOLD_COUNT + 1.0
    );
    assert!(parse_lol(&over).is_err());
}

#[test]
fn skadis_panel_size_is_bounded() {
    let ok = format!("skadis_panel({MAX_SKADIS_PANEL_MM}, 5.0, 5.0)");
    let start = Instant::now();
    eval_all(&parse_lol(&ok).unwrap());
    assert!(start.elapsed().as_secs() < 60, "took {:?}", start.elapsed());
    for bad in [
        format!("{}", MAX_SKADIS_PANEL_MM + 1.0),
        "1e30".into(),
        "1e999".into(),
    ] {
        for form in [
            "skadis_panel({})",
            "skadis_panel({}, 5.0)",
            "skadis_panel({}, 5.0, 5.0)",
        ] {
            let src = form.replace("{}", &bad);
            assert!(parse_lol(&src).is_err(), "`{src}` must be rejected");
        }
    }
}

#[test]
fn stairs_step_count_is_bounded() {
    // 評価器は段数だけ箱を走査する (`n_steps as u32` で 40 億段)
    let ok = format!("stairs_prim(0.3, 0.2, {MAX_STDLIB_COUNT}, 0.5)");
    eval_all(&parse_lol(&ok).unwrap());
    for bad in [
        format!("{}", MAX_STDLIB_COUNT + 1.0),
        "1e30".to_string(),
        "1e999".to_string(),
    ] {
        let src = format!("stairs_prim(0.3, 0.2, {bad}, 0.5)");
        assert!(parse_lol(&src).is_err(), "`{src}` must be rejected");
    }
}

#[test]
fn every_construct_survives_an_extreme_number_in_any_argument() {
    // 全 construct の snippet の数値リテラルを 1 つずつ極端な値にして、parse と評価が
    // panic せず実用時間で終わる (Err は可 — 上限を超えた入力を弾くのが契約)
    let start = Instant::now();
    let mut cases = 0usize;
    for (name, src) in common::corpus::grammar_corpus() {
        let b = src.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i].is_ascii_digit() {
                let st = i;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                for rep in ["1e30", "1e6", "1e999", "4096.0"] {
                    let s = format!("{}{}{}", &src[..st], rep, &src[i..]);
                    if let Ok(node) = parse_lol(&s) {
                        let _ = eval(&node, Vec3::new(0.3, -0.2, 0.7));
                    }
                    cases += 1;
                }
            } else {
                i += 1;
            }
        }
        assert!(
            start.elapsed().as_secs() < 600,
            "stalled at construct {name} after {cases} cases"
        );
    }
    assert!(cases > 500, "only {cases} cases ran");
}
