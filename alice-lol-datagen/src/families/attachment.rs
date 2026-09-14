//! 本体 + 付属 — fail 型「合成省略」(取手 / 脚 / アーチの subtract)
//!
//! mug (円柱 + トーラス取手) / table (天板 + 脚 4 本) / arch (箱 − 横向き円柱)

use crate::caption::fmt_mm;
use crate::rng::Rng;
use crate::sample::{OraclePoint, Sample, VerifyError};

pub(crate) fn generate(rng: &mut Rng) -> Result<Sample, VerifyError> {
    match rng.int(0, 2) {
        0 => mug(rng),
        1 => table(rng),
        _ => arch(rng),
    }
}

fn mug(rng: &mut Rng) -> Result<Sample, VerifyError> {
    let r = rng.stepped(20.0, 45.0, 5.0);
    let h = rng.stepped(60.0, 120.0, 10.0);
    let major = (r * 0.5).floor().max(8.0);
    let minor = (major / 3.0).floor().max(2.0);
    let lol = format!(
        "union(cylinder({r}, {}), translate({r}, 0, 0, torus({major}, {minor})))",
        h / 2.0
    );
    let en = format!(
        "A mug: a cylinder of radius {} and total height {} centered at the origin, plus a torus handle (major radius {}, minor radius {}) centered at ({}, 0, 0) on the +X side.",
        fmt_mm(r), fmt_mm(h), fmt_mm(major), fmt_mm(minor), fmt_mm(r)
    );
    let ja = format!(
        "マグカップ: 半径 {}、全高 {} の円柱を原点に置き、大半径 {}、小半径 {} のトーラス取手を +X 側の ({}, 0, 0) に付ける。",
        fmt_mm(r), fmt_mm(h), fmt_mm(major), fmt_mm(minor), fmt_mm(r)
    );
    let oracle = vec![
        OraclePoint::inside(0.0, 0.0, 0.0),
        OraclePoint::inside(0.0, h * 0.45, 0.0),
        OraclePoint::inside(r + major, 0.0, 0.0),
        OraclePoint::outside(0.0, h * 0.55, 0.0),
        OraclePoint::outside(r + major + minor + 10.0, 0.0, 0.0),
    ];
    Sample::new("attachment", en, ja, &lol, oracle)
}

fn table(rng: &mut Rng) -> Result<Sample, VerifyError> {
    let w = rng.stepped(40.0, 120.0, 10.0);
    let d = rng.stepped(30.0, 80.0, 10.0);
    let t = rng.stepped(2.0, 6.0, 2.0);
    let leg_h = rng.stepped(20.0, 60.0, 10.0);
    let leg_r = 2.0;
    let top_y = leg_h + t / 2.0;
    let (lx, lz) = (w / 2.0 - 5.0, d / 2.0 - 5.0);
    let leg_y = leg_h / 2.0;
    let legs: Vec<String> = [(lx, lz), (-lx, lz), (lx, -lz), (-lx, -lz)]
        .iter()
        .map(|(x, z)| {
            format!(
                "translate({x}, {leg_y}, {z}, cylinder({leg_r}, {}))",
                leg_h / 2.0
            )
        })
        .collect();
    let lol = format!(
        "union(translate(0, {top_y}, 0, box3d({}, {}, {})), {})",
        w / 2.0,
        t / 2.0,
        d / 2.0,
        legs.join(", ")
    );
    let en = format!(
        "A table: a top box {} wide (X), {} tall (Y), {} deep (Z) centered at (0, {}, 0), and four cylindrical legs of radius {} and total height {} centered at ({}, {}, {}), ({}, {}, {}), ({}, {}, {}), ({}, {}, {}). Union everything.",
        fmt_mm(w), fmt_mm(t), fmt_mm(d), fmt_mm(top_y), fmt_mm(leg_r), fmt_mm(leg_h),
        fmt_mm(lx), fmt_mm(leg_y), fmt_mm(lz), fmt_mm(-lx), fmt_mm(leg_y), fmt_mm(lz),
        fmt_mm(lx), fmt_mm(leg_y), fmt_mm(-lz), fmt_mm(-lx), fmt_mm(leg_y), fmt_mm(-lz)
    );
    let ja = format!(
        "テーブル: 幅 {} (X)、厚さ {} (Y)、奥行 {} (Z) の天板を中心 (0, {}, 0) に、半径 {}、全高 {} の円柱の脚 4 本を ({}, {}, ±{}) と ({}, {}, ±{}) に置いて union する。",
        fmt_mm(w), fmt_mm(t), fmt_mm(d), fmt_mm(top_y), fmt_mm(leg_r), fmt_mm(leg_h),
        fmt_mm(lx), fmt_mm(leg_y), fmt_mm(lz), fmt_mm(-lx), fmt_mm(leg_y), fmt_mm(lz)
    );
    let oracle = vec![
        OraclePoint::inside(0.0, top_y, 0.0),
        OraclePoint::inside(lx, leg_y, lz),
        OraclePoint::inside(-lx, leg_y, -lz),
        OraclePoint::outside(0.0, leg_y, 0.0),
        OraclePoint::outside(0.0, top_y + t, 0.0),
    ];
    Sample::new("attachment", en, ja, &lol, oracle)
}

fn arch(rng: &mut Rng) -> Result<Sample, VerifyError> {
    let w = rng.stepped(40.0, 100.0, 10.0);
    let h = rng.stepped(30.0, 80.0, 10.0);
    let d = rng.stepped(10.0, 30.0, 5.0);
    // 穴の天井 (2r) が箱の上面 (h) より 8 以上低いことを保証 (oracle 点 (0, h-2, 0) が材料に残る)
    let r = (w / 4.0).floor().min(((h - 8.0) / 2.0).floor()).max(5.0);
    let cy = r; // 円柱中心の高さ = 半径 → 底まで貫通
    let lol = format!(
        "subtract(translate(0, {}, 0, box3d({}, {}, {})), translate(0, {cy}, 0, rotate(90, 0, 0, cylinder({r}, {}))))",
        h / 2.0,
        w / 2.0,
        h / 2.0,
        d / 2.0,
        d
    );
    let en = format!(
        "An arch: a box {} wide (X), {} tall (Y), {} deep (Z) centered at (0, {}, 0), with a cylinder of radius {} running along Z (rotate a Y cylinder 90 degrees about X), centered at (0, {}, 0), subtracted from it.",
        fmt_mm(w), fmt_mm(h), fmt_mm(d), fmt_mm(h / 2.0), fmt_mm(r), fmt_mm(cy)
    );
    let ja = format!(
        "アーチ: 幅 {} (X)、高さ {} (Y)、奥行 {} (Z) の箱を中心 (0, {}, 0) に置き、半径 {} の Z 方向の円柱 (Y 円柱を X 軸で 90 度回転) を中心 (0, {}, 0) で引く。",
        fmt_mm(w), fmt_mm(h), fmt_mm(d), fmt_mm(h / 2.0), fmt_mm(r), fmt_mm(cy)
    );
    let oracle = vec![
        OraclePoint::outside(0.0, cy * 0.5, 0.0),
        OraclePoint::inside(0.0, h - 2.0, 0.0),
        OraclePoint::inside(w / 2.0 - 2.0, cy, 0.0),
        OraclePoint::outside(w / 2.0 + 5.0, h / 2.0, 0.0),
    ];
    Sample::new("attachment", en, ja, &lol, oracle)
}
