//! Y 軸に積む — fail 型「translate 欠落」「合成省略」
//!
//! 2〜4 個の球 or 円柱を下から順に積み、各要素の中心 Y を caption に明記する
//! (雪だるま / 段付き柱)

use super::dim;
use crate::caption::fmt_mm;
use crate::rng::Rng;
use crate::sample::{OraclePoint, Sample, VerifyError};

#[allow(clippy::branches_sharing_code)] // 球 / 円柱で前後処理が似るのは template の性質
pub(crate) fn generate(rng: &mut Rng) -> Result<Sample, VerifyError> {
    let n = rng.int(2, 4) as usize;
    let spheres = rng.chance(0.6);
    let mut y = 0.0f32;
    let mut parts = Vec::new();
    let mut en_parts = Vec::new();
    let mut ja_parts = Vec::new();
    let mut oracle = Vec::new();
    let mut top = 0.0f32;
    let mut widest = 0.0f32;
    for i in 0..n {
        if spheres {
            let r = (dim(rng) / 2.0).max(5.0);
            if i > 0 {
                // 前の要素の上面 - 少し埋め込む
                y = top - r * 0.3;
            }
            parts.push(if y == 0.0 {
                format!("sphere({r})")
            } else {
                format!("translate(0, {y}, 0, sphere({r}))")
            });
            en_parts.push(format!(
                "a sphere of radius {} centered at (0, {}, 0)",
                fmt_mm(r),
                fmt_mm(y)
            ));
            ja_parts.push(format!(
                "半径 {} の球を中心 (0, {}, 0) に",
                fmt_mm(r),
                fmt_mm(y)
            ));
            oracle.push(OraclePoint::inside(0.0, y, 0.0));
            top = y + r;
            widest = widest.max(r);
        } else {
            let r = (dim(rng) / 2.0).max(5.0);
            let h = dim(rng);
            if i > 0 {
                y = top + h / 2.0;
            }
            parts.push(if y == 0.0 {
                format!("cylinder({r}, {})", h / 2.0)
            } else {
                format!("translate(0, {y}, 0, cylinder({r}, {}))", h / 2.0)
            });
            en_parts.push(format!(
                "a cylinder of radius {} and total height {} centered at (0, {}, 0)",
                fmt_mm(r),
                fmt_mm(h),
                fmt_mm(y)
            ));
            ja_parts.push(format!(
                "半径 {}、全高 {} の円柱を中心 (0, {}, 0) に",
                fmt_mm(r),
                fmt_mm(h),
                fmt_mm(y)
            ));
            oracle.push(OraclePoint::inside(0.0, y, 0.0));
            top = y + h / 2.0;
            widest = widest.max(r);
        }
    }
    oracle.push(OraclePoint::outside(0.0, top + 5.0, 0.0));
    oracle.push(OraclePoint::outside(widest + 10.0, 0.0, 0.0));
    let lol = if parts.len() == 1 {
        parts[0].clone()
    } else {
        format!("union({})", parts.join(", "))
    };
    let what = if spheres {
        "snowman-like stack"
    } else {
        "stepped column"
    };
    let en = format!(
        "A {what}: union of {n} shapes stacked along Y — {}.",
        en_parts.join("; ")
    );
    let ja = format!(
        "{}: {} 個を Y 軸に積んで union する — {}。",
        if spheres {
            "雪だるま状の積み重ね"
        } else {
            "段付きの柱"
        },
        n,
        ja_parts.join("、")
    );
    Sample::new("stacked", en, ja, &lol, oracle)
}
