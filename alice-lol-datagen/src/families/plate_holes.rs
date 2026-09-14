//! 板 + 穴 — fail 型「`subtract` 省略」+ `polar_repeat` / `repeat_finite` の語彙
//!
//! 3 variant: 中央 1 穴 / 円周 n 穴 (`polar_repeat`) / 格子 穴 (`repeat_finite`)

use crate::caption::fmt_mm;
use crate::rng::Rng;
use crate::sample::{OraclePoint, Sample, VerifyError};

pub(crate) fn generate(rng: &mut Rng) -> Result<Sample, VerifyError> {
    let w = rng.stepped(40.0, 120.0, 10.0);
    let t = rng.stepped(2.0, 8.0, 2.0);
    let plate = format!("box3d({}, {}, {})", w / 2.0, t / 2.0, w / 2.0);
    let plate_en = format!(
        "a plate: a box {} wide (X), {} tall (Y), {} deep (Z) centered at the origin",
        fmt_mm(w),
        fmt_mm(t),
        fmt_mm(w)
    );
    let plate_ja = format!(
        "板: 幅 {} (X)、厚さ {} (Y)、奥行 {} (Z) の箱を原点に",
        fmt_mm(w),
        fmt_mm(t),
        fmt_mm(w)
    );
    let cut_h = t * 2.0; // 貫通させる cutter の全高
    match rng.int(0, 2) {
        0 => {
            let r = rng.stepped(3.0, (w / 4.0).floor().max(3.0), 1.0);
            let lol = format!("subtract({plate}, cylinder({r}, {cut_h}))");
            let en = format!(
                "{plate_en}, with a vertical cylindrical hole of radius {} through its center (subtract a Y cylinder of radius {} and total height {}).",
                fmt_mm(r), fmt_mm(r), fmt_mm(cut_h * 2.0)
            );
            let ja =
                format!(
                "{plate_ja}置き、中心に半径 {} の縦穴を開ける (半径 {}、全高 {} の Y 円柱を引く)。",
                fmt_mm(r), fmt_mm(r), fmt_mm(cut_h * 2.0)
            );
            let oracle = vec![
                OraclePoint::outside(0.0, 0.0, 0.0),
                OraclePoint::outside(r * 0.7, 0.0, 0.0),
                OraclePoint::inside(w / 2.0 - 3.0, 0.0, w / 2.0 - 3.0),
                OraclePoint::inside(r.midpoint(w / 2.0), 0.0, 0.0),
                OraclePoint::outside(0.0, t, 0.0),
            ];
            Sample::new("plate_holes", en, ja, &lol, oracle)
        }
        1 => {
            let n = rng.int(3, 8) as u32;
            let ring_r = (w / 3.0).floor();
            let hole_r = (ring_r * 0.25).floor().max(2.0);
            let lol = format!(
                "subtract({plate}, polar_repeat({n}, translate({ring_r}, 0, 0, cylinder({hole_r}, {cut_h}))))"
            );
            let en = format!(
                "{plate_en}, with {n} vertical holes of radius {} evenly spaced on a ring of radius {} around the Y axis, one of them on the +X axis (subtract a polar_repeat of a translated Y cylinder of total height {}).",
                fmt_mm(hole_r), fmt_mm(ring_r), fmt_mm(cut_h * 2.0)
            );
            let ja = format!(
                "{plate_ja}置き、半径 {} の円周上に半径 {} の縦穴を {n} 個等間隔 (1 つは +X 軸上) に開ける (translate した Y 円柱を polar_repeat して引く)。",
                fmt_mm(ring_r), fmt_mm(hole_r)
            );
            let oracle = vec![
                OraclePoint::outside(ring_r, 0.0, 0.0),
                OraclePoint::inside(0.0, 0.0, 0.0),
                OraclePoint::inside(w / 2.0 - 2.0, 0.0, w / 2.0 - 2.0),
                OraclePoint::outside(0.0, t, 0.0),
            ];
            Sample::new("plate_holes", en, ja, &lol, oracle)
        }
        _ => {
            // repeat_finite(cx, cy, cz, sx, sy, sz, child) = clamp(round(p/s), -c/2, c/2)
            // → 各軸のコピー数は 2·floor(c/2)+1 (常に奇数、原点にコピーあり)
            let cx = rng.int(2, 6) as u32;
            let cz = rng.int(2, 6) as u32;
            let (kx, kz) = (cx / 2, cz / 2);
            let (n_x, n_z) = (2 * kx + 1, 2 * kz + 1);
            let pitch = (w / (n_x.max(n_z) as f32 + 1.0)).floor().max(4.0);
            let r = (pitch * 0.25).floor().max(1.0);
            let lol = format!(
                "subtract({plate}, repeat_finite({cx}, 1, {cz}, {pitch}, 0, {pitch}, cylinder({r}, {cut_h})))"
            );
            let en = format!(
                "{plate_en}, with a {n_x} by {n_z} grid of vertical holes of radius {} at pitch {} centered on the plate, one hole at the center (subtract a repeat_finite of a Y cylinder of total height {}; repeat_finite counts {cx} and {cz} give {n_x} and {n_z} copies).",
                fmt_mm(r), fmt_mm(pitch), fmt_mm(cut_h * 2.0)
            );
            let ja = format!(
                "{plate_ja}置き、半径 {} の縦穴をピッチ {} で {n_x}×{n_z} の格子状 (中心にも穴) に開ける (Y 円柱を repeat_finite({cx}, 1, {cz}, …) して引く)。",
                fmt_mm(r), fmt_mm(pitch)
            );
            let oracle = vec![
                OraclePoint::outside(0.0, 0.0, 0.0),
                OraclePoint::outside(kx as f32 * pitch, 0.0, 0.0),
                OraclePoint::inside(pitch / 2.0, 0.0, pitch / 2.0),
                OraclePoint::outside(0.0, t, 0.0),
            ];
            Sample::new("plate_holes", en, ja, &lol, oracle)
        }
    }
}
