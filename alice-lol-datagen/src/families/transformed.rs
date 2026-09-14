//! rotate / scale — fail 型「変換の欠落・誤用」
//!
//! 角度は 15° 刻み、回転軸を 1 本だけ指定して caption と対応させる

use super::dim;
use crate::caption::fmt_mm;
use crate::rng::Rng;
use crate::sample::{OraclePoint, Sample, VerifyError};

pub(crate) fn generate(rng: &mut Rng) -> Result<Sample, VerifyError> {
    if rng.chance(0.5) {
        rotated_box(rng)
    } else {
        scaled(rng)
    }
}

type AxisPoint = fn(f32) -> (f32, f32, f32);

fn rotated_box(rng: &mut Rng) -> Result<Sample, VerifyError> {
    let hx = dim(rng) / 2.0;
    let hy = dim(rng) / 2.0;
    let hz = dim(rng) / 2.0;
    let axis = rng.int(0, 2);
    let deg = rng.stepped(15.0, 75.0, 15.0);
    let (rx, ry, rz) = match axis {
        0 => (deg, 0.0, 0.0),
        1 => (0.0, deg, 0.0),
        _ => (0.0, 0.0, deg),
    };
    let axis_name = ["X", "Y", "Z"][axis as usize];
    let lol = format!("rotate({rx}, {ry}, {rz}, box3d({hx}, {hy}, {hz}))");
    let en = format!(
        "A box {} wide (X), {} tall (Y), {} deep (Z) centered at the origin, rotated {} degrees about the {axis_name} axis.",
        fmt_mm(hx * 2.0), fmt_mm(hy * 2.0), fmt_mm(hz * 2.0), fmt_mm(deg)
    );
    let ja = format!(
        "幅 {} (X)、高さ {} (Y)、奥行 {} (Z) の箱を原点に置き、{axis_name} 軸まわりに {} 度回転する。",
        fmt_mm(hx * 2.0), fmt_mm(hy * 2.0), fmt_mm(hz * 2.0), fmt_mm(deg)
    );
    // 回転軸上の点は不変: 軸方向の半寸法で内外が決まる
    let (ax_half, ax_pt): (f32, AxisPoint) = match axis {
        0 => (hx, |v| (v, 0.0, 0.0)),
        1 => (hy, |v| (0.0, v, 0.0)),
        _ => (hz, |v| (0.0, 0.0, v)),
    };
    let (ix, iy, iz) = ax_pt(ax_half * 0.9);
    let (ox, oy, oz) = ax_pt(ax_half * 1.1);
    let oracle = vec![
        OraclePoint::inside(0.0, 0.0, 0.0),
        OraclePoint::inside(ix, iy, iz),
        OraclePoint::outside(ox, oy, oz),
        OraclePoint::outside(hx * 3.0, hy * 3.0, hz * 3.0),
    ];
    Sample::new("transformed", en, ja, &lol, oracle)
}

fn scaled(rng: &mut Rng) -> Result<Sample, VerifyError> {
    let r = rng.stepped(1.0, 5.0, 1.0);
    let s = rng.stepped(2.0, 10.0, 1.0);
    let lol = format!("scale({s}, sphere({r}))");
    let en = format!(
        "A sphere of radius {} scaled uniformly by {}, centered at the origin.",
        fmt_mm(r),
        fmt_mm(s)
    );
    let ja = format!(
        "半径 {} の球を {} 倍に一様拡大して原点に置く。",
        fmt_mm(r),
        fmt_mm(s)
    );
    let eff = r * s;
    let oracle = vec![
        OraclePoint::inside(0.0, 0.0, 0.0),
        OraclePoint::inside(eff * 0.9, 0.0, 0.0),
        OraclePoint::outside(eff * 1.1, 0.0, 0.0),
    ];
    Sample::new("transformed", en, ja, &lol, oracle)
}
