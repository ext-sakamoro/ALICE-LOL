//! 単体 primitive + 配置 — fail 型「translate 欠落」「half 引数の誤解」
//!
//! caption は **全寸** (幅 / 高さ / 直径) で書き、LOL は half 引数で書く 半分の
//! 確率で原点以外に置き、caption に座標を明記する

use super::{coord, dim};
use crate::caption::{at_en, at_ja, fmt_mm};
use crate::rng::Rng;
use crate::sample::{OraclePoint, Sample, VerifyError};
use glam::Vec3;

#[allow(clippy::too_many_lines)] // 4 primitive の template
pub(crate) fn generate(rng: &mut Rng) -> Result<Sample, VerifyError> {
    let pos = if rng.chance(0.5) {
        Vec3::new(coord(rng), coord(rng), coord(rng))
    } else {
        Vec3::ZERO
    };
    let kind = rng.int(0, 3);
    let (core, en, ja, mut oracle): (String, String, String, Vec<OraclePoint>) = match kind {
        0 => {
            let r = dim(rng) / 2.0;
            (
                format!("sphere({r})"),
                format!("A sphere of diameter {} {}.", fmt_mm(r * 2.0), at_en(pos)),
                format!("直径 {} の球を{}置く。", fmt_mm(r * 2.0), at_ja(pos)),
                vec![
                    OraclePoint::inside(pos.x, pos.y, pos.z),
                    OraclePoint::inside(pos.x + r * 0.9, pos.y, pos.z),
                    OraclePoint::outside(pos.x + r * 1.1, pos.y, pos.z),
                    OraclePoint::outside(pos.x, pos.y - r * 1.1, pos.z),
                ],
            )
        }
        1 => {
            let (w, h, d) = (dim(rng), dim(rng), dim(rng));
            (
                format!("box3d({}, {}, {})", w / 2.0, h / 2.0, d / 2.0),
                format!(
                    "A box {} wide (X), {} tall (Y), {} deep (Z), {}.",
                    fmt_mm(w),
                    fmt_mm(h),
                    fmt_mm(d),
                    at_en(pos)
                ),
                format!(
                    "幅 {} (X)、高さ {} (Y)、奥行 {} (Z) の箱を{}置く。",
                    fmt_mm(w),
                    fmt_mm(h),
                    fmt_mm(d),
                    at_ja(pos)
                ),
                vec![
                    OraclePoint::inside(pos.x, pos.y, pos.z),
                    OraclePoint::inside(pos.x + w * 0.45, pos.y + h * 0.45, pos.z + d * 0.45),
                    OraclePoint::outside(pos.x + w * 0.55, pos.y, pos.z),
                    OraclePoint::outside(pos.x, pos.y + h * 0.55, pos.z),
                    OraclePoint::outside(pos.x, pos.y, pos.z + d * 0.55),
                ],
            )
        }
        2 => {
            let r = dim(rng) / 2.0;
            let h = dim(rng);
            (
                format!("cylinder({r}, {})", h / 2.0),
                format!(
                    "A cylinder of diameter {} and total height {} along the Y axis, {}.",
                    fmt_mm(r * 2.0),
                    fmt_mm(h),
                    at_en(pos)
                ),
                format!(
                    "直径 {}、全高 {} の Y 軸方向の円柱を{}置く。",
                    fmt_mm(r * 2.0),
                    fmt_mm(h),
                    at_ja(pos)
                ),
                vec![
                    OraclePoint::inside(pos.x, pos.y + h * 0.45, pos.z),
                    OraclePoint::inside(pos.x + r * 0.9, pos.y, pos.z),
                    OraclePoint::outside(pos.x, pos.y + h * 0.55, pos.z),
                    OraclePoint::outside(pos.x + r * 1.1, pos.y, pos.z),
                ],
            )
        }
        _ => {
            let big = dim(rng) / 2.0 + 5.0;
            let small = (big / 4.0).max(1.0).floor();
            (
                format!("torus({big}, {small})"),
                format!(
                    "A torus lying in the XZ plane with major radius {} and minor radius {}, {}.",
                    fmt_mm(big),
                    fmt_mm(small),
                    at_en(pos)
                ),
                format!(
                    "XZ 平面に寝た大半径 {}、小半径 {} のトーラスを{}置く。",
                    fmt_mm(big),
                    fmt_mm(small),
                    at_ja(pos)
                ),
                vec![
                    OraclePoint::inside(pos.x + big, pos.y, pos.z),
                    OraclePoint::inside(pos.x, pos.y, pos.z + big),
                    OraclePoint::outside(pos.x, pos.y, pos.z),
                    OraclePoint::outside(pos.x + big + small * 1.5, pos.y, pos.z),
                ],
            )
        }
    };
    let lol = if pos == Vec3::ZERO {
        core
    } else {
        format!("translate({}, {}, {}, {core})", pos.x, pos.y, pos.z)
    };
    oracle.retain(|o| o.p.is_finite());
    Sample::new("primitive_placed", en, ja, &lol, oracle)
}
