//! `laser_pattern` の閉形式 oracle
//!
//! 既存の `#[test]` 30 本 (`src/laser_pattern.rs` 内) は **構造しか見ていない**:
//! `test_rose_3_petals` は `assert_eq!(elems.len(), 1)` で Polyline が 1 本ある
//! ことだけを確かめており、**花弁が 3 枚あることを一切見ていない**
//! `test_guilloche_basic` は点数 >100 のみ、`test_dither_floyd_steinberg` は
//! `!is_empty()` のみ
//!
//! そこへ変異を 10 種入れたところ **9 種が全 green で通過** した (2026-09-30 実測):
//! rose の `k` を +1 / rose の `cos`→`sin` / phyllotaxis の黄金角 137.508°→137.0° /
//! phyllotaxis の半径則 `s√i`→`s·i` / hatch の線間隔 5% ずらし /
//! 誤差拡散核の係数 7→6 / hypotrochoid の sin 符号反転 / lissajous の位相差の移動 /
//! `cell_center` の半セルずらし 捕まったのは halftone の符号反転 1 件だけ
//!
//! 本 file はそれらを**閉形式**で押さえる ここで使う期待値はすべて教科書の
//! 式から来ており、実装関数を呼んで作った値は 1 つも無い:
//!
//! | 対象 | oracle の出所 |
//! |---|---|
//! | rose | ロドネア曲線 `r = a·cos(kθ)` の花弁数 (k 奇数 → k、偶数 → 2k) と `\|r\| ≤ a` |
//! | phyllotaxis | Vogel の螺旋 `r_i = s√i`, `θ_i = i·ψ` (ψ = 黄金角 360°/φ² = 137.50776…°) と面積一様性 |
//! | guilloche | ハイポトロコイドの極半径 `(R−r)±d`、周期 `2π·r/gcd(R,r)`、尖点数 `R/gcd(R,r)` |
//! | hatch | 平行線族の法線方向間隔がちょうど `spacing` (角度に依らない) |
//! | lissajous | `a=b`, `δ=π/2` は厳密な円 / 整数周波数で閉曲線 / 値域 `±A` |
//! | halftone | `radius = max_radius·(1−brightness)` と `lpi → 25.4/lpi` mm セル |
//! | dither | 誤差拡散の階調保存 (出力ドット密度 = 1−輝度) と Bayer 閾値行列の順列性 |
//!
//! 精度パラメータ (`steps`) を振っても幾何量が変わらないことも併せて見る
//! (`rules/analytic-oracle-tests.md`「精度 parameter を振って不変を assert」)

// grid index ⇄ 座標の変換のみ (ビン数は高々 1440、f64 の仮数に収まる)
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use alice_lol::laser_pattern::{
    crosshatch, density_hatch, dither, guilloche, halftone, hatch, lissajous, phyllotaxis, rose,
    turing, Bounds, DitherAlgorithm, LaserElement,
};
use std::f64::consts::{PI, TAU};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  取り出し helper
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 唯一の `Polyline` の点列を、中心を原点に移した相対座標で返す
fn polyline_about(elements: &[LaserElement], center: (f64, f64)) -> Vec<(f64, f64)> {
    assert_eq!(elements.len(), 1, "曲線は Polyline 1 本で返る約束");
    match &elements[0] {
        LaserElement::Polyline(points) => points
            .iter()
            .map(|&(x, y)| (x - center.0, y - center.1))
            .collect(),
        other => panic!("Polyline を期待したが {other:?} が返った"),
    }
}

fn dots(elements: &[LaserElement]) -> Vec<(f64, f64)> {
    elements
        .iter()
        .filter_map(|e| match *e {
            LaserElement::Dot(x, y) => Some((x, y)),
            _ => None,
        })
        .collect()
}

fn circles(elements: &[LaserElement]) -> Vec<(f64, f64, f64)> {
    elements
        .iter()
        .filter_map(|e| match *e {
            LaserElement::Circle(x, y, r) => Some((x, y, r)),
            _ => None,
        })
        .collect()
}

fn segments(elements: &[LaserElement]) -> Vec<(f64, f64, f64, f64)> {
    elements
        .iter()
        .filter_map(|e| match *e {
            LaserElement::Line(x1, y1, x2, y2) => Some((x1, y1, x2, y2)),
            _ => None,
        })
        .collect()
}

fn radius(p: (f64, f64)) -> f64 {
    p.0.hypot(p.1)
}

/// 教科書の Euclid 互除法 (実装側の `gcd_f64` とは独立、oracle 側の参照)
///
/// 終了条件が float 比較なのは、剰余が厳密に 0 にならない入力があるため
/// (実装側の `gcd_f64` も同じ eps で止める)
#[allow(clippy::while_float)]
fn gcd_reference(lhs: f64, rhs: f64) -> f64 {
    let (mut dividend, mut divisor) = (lhs.abs(), rhs.abs());
    while divisor > 1e-9 {
        let remainder = dividend % divisor;
        dividend = divisor;
        divisor = remainder;
    }
    dividend
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  数え方 (どちらも実装を参照せず点列だけから決まる)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// **幾何的な**花弁数 — 極角を 1440 分割し、そのビンに落ちた点の最大半径が
/// `tip` 以上になるビンの巡回連結成分を数える
///
/// 同じ花弁を 2 度なぞっても (奇数 k のロドネアは θ∈[0,2π] で 2 周する) 方向が
/// 同じなので 1 枚に数えられる
fn petal_count(points: &[(f64, f64)], tip: f64) -> usize {
    const BINS: usize = 1440;
    let mut max_radius = [0.0_f64; BINS];
    for &p in points {
        let mut theta = p.1.atan2(p.0);
        if theta < 0.0 {
            theta += TAU;
        }
        let bin = ((theta / TAU) * BINS as f64) as usize;
        let bin = bin.min(BINS - 1);
        max_radius[bin] = max_radius[bin].max(radius(p));
    }
    let hot: Vec<bool> = max_radius.iter().map(|&r| r >= tip).collect();
    if hot.iter().all(|&h| h) {
        return 1;
    }
    (0..BINS)
        .filter(|&i| hot[i] && !hot[(i + BINS - 1) % BINS])
        .count()
}

/// 閉曲線の点列 (末尾 = 先頭) 上で、半径が `threshold` を上回る巡回区間の個数
/// = ハイポトロコイドの尖点 (lobe) 数
fn lobe_count(points: &[(f64, f64)], threshold: f64) -> usize {
    let ring = &points[..points.len() - 1];
    let n = ring.len();
    let hot: Vec<bool> = ring.iter().map(|&p| radius(p) >= threshold).collect();
    if hot.iter().all(|&h| h) {
        return 1;
    }
    (0..n).filter(|&i| hot[i] && !hot[(i + n - 1) % n]).count()
}

const fn card() -> Bounds {
    Bounds::new(0.0, 0.0, 86.0, 54.0)
}

const fn wide() -> Bounds {
    Bounds::new(-200.0, -200.0, 400.0, 400.0)
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  rose — ロドネア曲線 r = a·cos(kθ)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// oracle: 整数 k のロドネア曲線は k が奇数なら k 枚、偶数なら 2k 枚の花弁を持つ
/// (r<0 の弧が k 奇数では既存の花弁に重なり、k 偶数では新しい方向に出るため)
#[test]
fn rose_petal_count_matches_rhodonea_closed_form() {
    let bounds = wide();
    let amplitude = 20.0;
    for k in 2_u32..=7 {
        let points = polyline_about(
            &rose(f64::from(k), amplitude, 20_000, &bounds),
            bounds.center(),
        );
        let expected = if k % 2 == 1 { k } else { 2 * k } as usize;
        let got = petal_count(&points, amplitude * 0.5);
        assert_eq!(
            got, expected,
            "k={k} の花弁数: 閉形式は {expected} 枚 (k 奇数→k / 偶数→2k) だが {got} 枚だった"
        );
    }
}

/// oracle: `|r| = a·|cos(kθ)| ≤ a` なので、全点が中心から半径 a 以内に入る
#[test]
fn rose_stays_within_its_amplitude() {
    let bounds = wide();
    let amplitude = 20.0;
    let points = polyline_about(&rose(5.0, amplitude, 4_000, &bounds), bounds.center());
    let farthest = points.iter().copied().map(radius).fold(0.0_f64, f64::max);
    assert!(
        farthest <= amplitude + 1e-9,
        "最遠点 {farthest} が振幅 {amplitude} を超えた (|cos| ≤ 1 に反する)"
    );
    // 先端 (cos(kθ)=±1) に十分近づく: 振幅が実際に使われていることの下界
    assert!(
        farthest > amplitude * 0.999,
        "最遠点 {farthest} が振幅 {amplitude} に届かない (花弁の先端が欠けている)"
    );
}

/// 精度 parameter を振っても幾何は変わらない (`rules/analytic-oracle-tests.md`)
#[test]
fn rose_petal_count_is_invariant_to_step_count() {
    let bounds = wide();
    let amplitude = 20.0;
    for steps in [5_000_u32, 10_000, 20_000, 40_000] {
        let points = polyline_about(&rose(4.0, amplitude, steps, &bounds), bounds.center());
        assert_eq!(
            petal_count(&points, amplitude * 0.5),
            8,
            "steps={steps} で k=4 の花弁数が 8 から動いた (刻み幅は物理量を変えてはいけない)"
        );
    }
}

/// oracle: 花弁の先端方向は `cos(kθ)=±1` すなわち θ = mπ/k
/// k=3 なら {0, π/3, 2π/3, …} のうち実際に描かれるのは 3 方向
#[test]
fn rose_tip_directions_lie_on_multiples_of_pi_over_k() {
    let bounds = wide();
    let amplitude = 20.0;
    let k = 3.0_f64;
    let points = polyline_about(&rose(k, amplitude, 20_000, &bounds), bounds.center());
    // 先端に極めて近い点はすべて mπ/k 方向 (±1°) を向く
    for p in points
        .iter()
        .copied()
        .filter(|&p| radius(p) > amplitude * 0.999)
    {
        let theta = p.1.atan2(p.0).rem_euclid(PI / k);
        let off = theta.min(PI / k - theta).to_degrees();
        assert!(
            off < 1.0,
            "先端の点 {p:?} が θ = mπ/k から {off} 度ずれている"
        );
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  phyllotaxis — Vogel の螺旋 r = s√i, θ = i·ψ
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 黄金角 ψ = 360°/φ² = 360°·(2−φ) = 137.507764050037854…°
const GOLDEN_ANGLE_DEG: f64 = 137.507_764_050_037_85;

/// oracle: 連続する 2 点の極角差は黄金角に等しい
///
/// 許容幅 0.01° は「実装が採っている 137.508° の丸め (真値との差 2.4e-4°) は通し、
/// 137.0° のような別の角は落とす」ように取ってある
#[test]
fn phyllotaxis_turns_by_the_golden_angle_each_step() {
    let bounds = wide();
    let points = dots(&phyllotaxis(600, 4.0, &bounds));
    let center = bounds.center();
    assert!(points.len() > 100, "評価に足る点数が出ていない");

    for pair in points.windows(2) {
        let a = (pair[0].0 - center.0, pair[0].1 - center.1);
        let b = (pair[1].0 - center.0, pair[1].1 - center.1);
        if radius(a) < 1e-6 || radius(b) < 1e-6 {
            continue; // 中心の 1 点は角が定まらない
        }
        let delta = (b.1.atan2(b.0) - a.1.atan2(a.0))
            .rem_euclid(TAU)
            .to_degrees();
        assert!(
            (delta - GOLDEN_ANGLE_DEG).abs() < 0.01,
            "隣接点の角度差 {delta}° が黄金角 {GOLDEN_ANGLE_DEG}° から外れた"
        );
    }
}

/// oracle: Vogel の螺旋の半径則は `r_i = scale·√i` (i は通し番号)
#[test]
fn phyllotaxis_radius_follows_the_square_root_law() {
    let bounds = wide();
    let scale = 4.0;
    let points = dots(&phyllotaxis(400, scale, &bounds));
    let center = bounds.center();
    // clip されていないことを前提に i と 1:1 対応させる (wide() は十分広い)
    assert_eq!(
        points.len(),
        400,
        "wide() の中では 1 点も clip されない想定"
    );
    for (i, &p) in points.iter().enumerate() {
        let got = radius((p.0 - center.0, p.1 - center.1));
        let want = scale * (i as f64).sqrt();
        assert!(
            (got - want).abs() <= 1e-9 * want.max(1.0),
            "i={i}: 半径 {got} が s√i = {want} と一致しない"
        );
    }
}

/// oracle: `r = s√i` は「i 番目までが半径 s√i の円に入る」= 面積あたりの点密度が一定
/// よって半径 R 以内の点数はちょうど `(R/s)²` (端数 ±1)
#[test]
fn phyllotaxis_point_density_is_uniform_per_unit_area() {
    let bounds = wide();
    let scale = 4.0;
    let points = dots(&phyllotaxis(900, scale, &bounds));
    let center = bounds.center();
    for r_cut in [40.0_f64, 60.0, 80.0, 100.0] {
        let inside = points
            .iter()
            .filter(|&&p| radius((p.0 - center.0, p.1 - center.1)) <= r_cut)
            .count();
        let expected = (r_cut / scale).powi(2);
        assert!(
            (inside as f64 - expected).abs() <= 1.0,
            "半径 {r_cut} 内の点数 {inside} が面積則 (R/s)² = {expected} と 1 点を超えてずれた"
        );
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  guilloche — ハイポトロコイド
//    x = (R−r)cos t + d·cos(((R−r)/r)t),  y = (R−r)sin t − d·sin(((R−r)/r)t)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 検査する (R, r, d) の組 — いずれも gcd が 1 でない / 1 の両方を含む
const SPIRO: [(f64, f64, f64); 5] = [
    (10.0, 5.0, 3.0),
    (12.0, 8.0, 4.0),
    (10.0, 7.0, 5.0),
    (100.0, 35.0, 10.0),
    (9.0, 6.0, 2.0),
];

/// oracle: 極半径は `|R−r| ± d` の間を動き、両端をちょうど取る
#[test]
fn guilloche_extreme_radii_match_the_hypotrochoid_closed_form() {
    let bounds = wide();
    for (big_r, small_r, pen_d) in SPIRO {
        let points = polyline_about(
            &guilloche(big_r, small_r, pen_d, 20_000, &bounds),
            bounds.center(),
        );
        let want_max = (big_r - small_r).abs() + pen_d;
        let want_min = ((big_r - small_r).abs() - pen_d).abs();
        let got_max = points.iter().copied().map(radius).fold(0.0_f64, f64::max);
        let got_min = points
            .iter()
            .copied()
            .map(radius)
            .fold(f64::INFINITY, f64::min);
        assert!(
            (got_max - want_max).abs() < 1e-6,
            "R={big_r} r={small_r} d={pen_d}: 最大半径 {got_max} が (R−r)+d = {want_max} と違う"
        );
        assert!(
            (got_min - want_min).abs() < 1e-6,
            "R={big_r} r={small_r} d={pen_d}: 最小半径 {got_min} が |(R−r)−d| = {want_min} と違う"
        );
    }
}

/// oracle: 周期は `2π·r/gcd(R,r)` なので、その範囲を描き切れば曲線は始点に戻る
///
/// 閉じないなら周期の計算が誤っている (= 模様が途中で切れる)
#[test]
fn guilloche_returns_to_its_start_point() {
    let bounds = wide();
    for (big_r, small_r, pen_d) in SPIRO {
        let points = polyline_about(
            &guilloche(big_r, small_r, pen_d, 20_000, &bounds),
            bounds.center(),
        );
        let first = points[0];
        let last = points[points.len() - 1];
        let gap = (first.0 - last.0).hypot(first.1 - last.1);
        assert!(
            gap < 1e-6,
            "R={big_r} r={small_r} d={pen_d}: 終点が始点から {gap} 離れている \
             (周期 2π·r/gcd(R,r) を描き切れていない)"
        );
    }
}

/// oracle: ハイポトロコイドの尖点数は `R/gcd(R,r)`
#[test]
fn guilloche_lobe_count_matches_r_over_gcd() {
    let bounds = wide();
    for (big_r, small_r, pen_d) in SPIRO {
        let points = polyline_about(
            &guilloche(big_r, small_r, pen_d, 20_000, &bounds),
            bounds.center(),
        );
        let expected = (big_r / gcd_reference(big_r, small_r)).round() as usize;
        let want_max = (big_r - small_r).abs() + pen_d;
        let want_min = ((big_r - small_r).abs() - pen_d).abs();
        let got = lobe_count(&points, f64::midpoint(want_max, want_min));
        assert_eq!(
            got, expected,
            "R={big_r} r={small_r} d={pen_d}: 尖点数が R/gcd(R,r) = {expected} でなく {got}"
        );
    }
}

/// oracle: y 成分の符号が `−d·sin(((R−r)/r)t)` (ハイポ) であってエピではない
///
/// t を固定して閉形式の値と直接突き合わせる
#[test]
fn guilloche_matches_the_hypotrochoid_point_by_point() {
    let bounds = wide();
    let (big_r, small_r, pen_d) = (10.0, 7.0, 5.0);
    let steps = 20_000_u32;
    let points = polyline_about(
        &guilloche(big_r, small_r, pen_d, steps, &bounds),
        bounds.center(),
    );
    let t_max = TAU * small_r / gcd_reference(big_r, small_r);
    let dt = t_max / f64::from(steps);
    let diff = big_r - small_r;
    let ratio = diff / small_r;
    for i in (0..=steps as usize).step_by(97) {
        let t = i as f64 * dt;
        let want = (
            diff.mul_add(t.cos(), pen_d * (ratio * t).cos()),
            diff.mul_add(t.sin(), -(pen_d * (ratio * t).sin())),
        );
        let got = points[i];
        let err = (got.0 - want.0).hypot(got.1 - want.1);
        assert!(
            err < 1e-6,
            "i={i} (t={t}): 点 {got:?} が閉形式 {want:?} と {err} ずれた"
        );
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  hatch / crosshatch
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 線分を法線 (−sinθ, cosθ) 方向へ射影した値 — 平行線族の「何本目か」を決める量
fn normal_offset(seg: (f64, f64, f64, f64), angle_deg: f64) -> f64 {
    let a = angle_deg.to_radians();
    (-a.sin()).mul_add(seg.0, a.cos() * seg.1)
}

/// oracle: 平行線族の隣り合う線の垂直距離はちょうど `spacing` — **角度に依らない**
#[test]
fn hatch_line_spacing_is_exact_at_every_angle() {
    let bounds = card();
    let spacing = 2.0;
    for angle in [0.0_f64, 17.0, 30.0, 45.0, 73.0, 90.0, 123.0] {
        let segs = segments(&hatch(angle, spacing, &bounds));
        assert!(
            segs.len() >= 3,
            "angle={angle} で線が 3 本未満しか出ていない"
        );
        let mut offsets: Vec<f64> = segs.iter().map(|&s| normal_offset(s, angle)).collect();
        offsets.sort_by(f64::total_cmp);
        for pair in offsets.windows(2) {
            let gap = pair[1] - pair[0];
            assert!(
                (gap - spacing).abs() < 1e-9,
                "angle={angle}: 隣接線の間隔 {gap} が spacing {spacing} と違う"
            );
        }
    }
}

/// oracle: 生成された線分の向きは要求した角度に一致する (mod 180°)
#[test]
fn hatch_line_direction_matches_the_requested_angle() {
    let bounds = card();
    for angle in [0.0_f64, 17.0, 30.0, 45.0, 73.0, 123.0] {
        for seg in segments(&hatch(angle, 3.0, &bounds)) {
            let (dx, dy) = (seg.2 - seg.0, seg.3 - seg.1);
            if dx.hypot(dy) < 1e-9 {
                continue; // 角で 1 点に潰れた線は向きを持たない
            }
            let got = dy.atan2(dx).to_degrees().rem_euclid(180.0);
            let want = angle.rem_euclid(180.0);
            let off = (got - want).abs().min(180.0 - (got - want).abs());
            assert!(off < 1e-6, "angle={angle}: 線の向きが {got}° になっている");
        }
    }
}

/// oracle: 線族は矩形を `spacing` 以下の隙間で覆う (端が抜けていない)
#[test]
fn hatch_covers_the_whole_rectangle() {
    let bounds = card();
    let spacing = 2.5;
    let angle = 35.0_f64;
    let segs = segments(&hatch(angle, spacing, &bounds));
    let a = angle.to_radians();
    let corners = [
        (bounds.x, bounds.y),
        (bounds.x + bounds.w, bounds.y),
        (bounds.x + bounds.w, bounds.y + bounds.h),
        (bounds.x, bounds.y + bounds.h),
    ];
    let projections: Vec<f64> = corners
        .iter()
        .map(|&(x, y)| (-a.sin()).mul_add(x, a.cos() * y))
        .collect();
    let lo = projections.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = projections
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let offsets: Vec<f64> = segs.iter().map(|&s| normal_offset(s, angle)).collect();
    let first = offsets.iter().copied().fold(f64::INFINITY, f64::min);
    let last = offsets.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!(
        first - lo < spacing && hi - last < spacing,
        "被覆に隙間: 矩形の射影 [{lo}, {hi}] に対し線は [{first}, {last}] までしか無い"
    );
}

/// oracle: 密度が一定 c なら線間隔は `max − (max−min)·c` の等間隔になる
/// (実装が宣言している内挿式そのもの)
#[test]
fn density_hatch_with_uniform_density_uses_the_interpolated_spacing() {
    let bounds = card();
    let (min_spacing, max_spacing) = (1.0_f64, 4.0);
    let angle = 20.0_f64;
    for density in [0.0_f64, 0.25, 0.5, 1.0] {
        let segs = segments(&density_hatch(
            min_spacing,
            max_spacing,
            angle,
            &bounds,
            &|_, _| density,
        ));
        assert!(segs.len() >= 3, "density={density} で線が 3 本未満");
        let want = (max_spacing - min_spacing).mul_add(-density, max_spacing);
        let mut offsets: Vec<f64> = segs.iter().map(|&s| normal_offset(s, angle)).collect();
        offsets.sort_by(f64::total_cmp);
        for pair in offsets.windows(2) {
            let gap = pair[1] - pair[0];
            assert!(
                (gap - want).abs() < 1e-9,
                "density={density}: 間隔 {gap} が max−(max−min)·density = {want} と違う"
            );
        }
    }
}

/// oracle: どんな密度関数でも、隣接線の間隔は `[min_spacing, max_spacing]` を出ない
/// (密度は `clamp(0,1)` されるので内挿の外へは行けない)
#[test]
fn density_hatch_spacing_stays_within_its_bounds() {
    let bounds = card();
    let (min_spacing, max_spacing) = (1.0_f64, 5.0);
    let angle = 0.0_f64;
    // clamp の外まで振る密度関数 (−3 〜 +3)
    let segs = segments(&density_hatch(
        min_spacing,
        max_spacing,
        angle,
        &bounds,
        &|x, y| 3.0 * ((x * 0.3).sin() + (y * 0.2).cos()),
    ));
    let mut offsets: Vec<f64> = segs.iter().map(|&s| normal_offset(s, angle)).collect();
    offsets.sort_by(f64::total_cmp);
    for pair in offsets.windows(2) {
        let gap = pair[1] - pair[0];
        assert!(
            gap >= min_spacing - 1e-9 && gap <= max_spacing + 1e-9,
            "間隔 {gap} が [{min_spacing}, {max_spacing}] の外に出た"
        );
    }
}

/// oracle: 密度が高い側ほど線が密になる (密度→間隔が単調減少なので)
#[test]
fn density_hatch_is_denser_where_the_density_is_higher() {
    let bounds = Bounds::new(0.0, 0.0, 40.0, 40.0);
    let mid = bounds.center().1;
    // 下半分だけ最密、上半分は最疎 (angle=0 なので法線は y 方向)
    let segs = segments(&density_hatch(1.0, 5.0, 0.0, &bounds, &|_, y| {
        if y < mid {
            1.0
        } else {
            0.0
        }
    }));
    let lower = segs.iter().filter(|s| s.1 < mid).count();
    let upper = segs.iter().filter(|s| s.1 >= mid).count();
    assert!(
        lower > upper,
        "最密側の線数 {lower} が最疎側 {upper} を上回らない"
    );
}

/// oracle: crosshatch は直交する 2 族の合併
#[test]
fn crosshatch_is_two_orthogonal_families() {
    let bounds = card();
    let spacing = 4.0;
    let all = segments(&crosshatch(spacing, &bounds));
    let horizontal = segments(&hatch(0.0, spacing, &bounds));
    let vertical = segments(&hatch(90.0, spacing, &bounds));
    assert_eq!(
        all.len(),
        horizontal.len() + vertical.len(),
        "crosshatch の本数が 0° 族 + 90° 族と合わない"
    );
    let mut directions: Vec<f64> = all
        .iter()
        .filter(|s| (s.2 - s.0).hypot(s.3 - s.1) > 1e-9)
        .map(|s| (s.3 - s.1).atan2(s.2 - s.0).to_degrees().rem_euclid(180.0))
        .collect();
    directions.sort_by(f64::total_cmp);
    directions.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    assert_eq!(directions.len(), 2, "向きが 2 種類でない: {directions:?}");
    assert!(
        (directions[1] - directions[0] - 90.0).abs() < 1e-6,
        "2 族が直交していない: {directions:?}"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  lissajous
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// oracle: `x = A sin(t+π/2) = A cos t`, `y = A sin t` は**厳密な円**
///
/// 位相差 δ が x 側にしか掛からないことがこの形の条件なので、δ の適用位置が
/// ずれると半径が一定でなくなる
#[test]
fn lissajous_with_quarter_phase_is_an_exact_circle() {
    let bounds = wide();
    let amplitude = 20.0;
    let points = polyline_about(
        &lissajous(1.0, 1.0, PI / 2.0, amplitude, 4_000, &bounds),
        bounds.center(),
    );
    for p in points {
        assert!(
            (radius(p) - amplitude).abs() < 1e-9,
            "点 {p:?} の半径 {} が振幅 {amplitude} から外れた (δ=π/2, a=b=1 は円)",
            radius(p)
        );
    }
}

/// oracle: 値域はちょうど `±A` (sin の値域そのもの)
#[test]
fn lissajous_spans_exactly_its_amplitude() {
    let bounds = wide();
    let amplitude = 20.0;
    let points = polyline_about(
        &lissajous(3.0, 2.0, 0.0, amplitude, 20_000, &bounds),
        bounds.center(),
    );
    for (axis, extract) in [
        ("x", (|p: (f64, f64)| p.0) as fn((f64, f64)) -> f64),
        ("y", |p: (f64, f64)| p.1),
    ] {
        let hi = points.iter().copied().map(extract).fold(f64::MIN, f64::max);
        let lo = points.iter().copied().map(extract).fold(f64::MAX, f64::min);
        assert!(hi <= amplitude + 1e-9, "{axis} の上端 {hi} が振幅を超えた");
        assert!(lo >= -amplitude - 1e-9, "{axis} の下端 {lo} が振幅を超えた");
        assert!(
            hi > amplitude * 0.999 && lo < -amplitude * 0.999,
            "{axis} が振幅 {amplitude} に届かない (上端 {hi} / 下端 {lo})"
        );
    }
}

/// oracle: 整数周波数なら t∈[0,2π] で閉じる
#[test]
fn lissajous_closes_for_integer_frequencies() {
    let bounds = wide();
    for (a, b) in [(1.0, 1.0), (3.0, 2.0), (5.0, 4.0)] {
        let points = polyline_about(&lissajous(a, b, 0.3, 20.0, 4_000, &bounds), bounds.center());
        let gap = (points[0].0 - points[points.len() - 1].0)
            .hypot(points[0].1 - points[points.len() - 1].1);
        assert!(gap < 1e-9, "a={a} b={b}: 終点が始点から {gap} 離れている");
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  halftone (AM)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// oracle: ドット半径は `max_radius·(1−brightness)`
#[test]
fn halftone_dot_radius_is_linear_in_ink() {
    let bounds = Bounds::new(0.0, 0.0, 20.0, 20.0);
    let max_radius = 0.5;
    for brightness in [0.0_f64, 0.25, 0.5, 0.75] {
        let found = circles(&halftone(25.4, max_radius, &bounds, &|_, _| brightness));
        assert!(
            !found.is_empty(),
            "brightness={brightness} でドットが出ない"
        );
        let want = max_radius * (1.0 - brightness);
        for (_, _, r) in found {
            assert!(
                (r - want).abs() < 1e-12,
                "brightness={brightness}: 半径 {r} が max_radius·(1−b) = {want} と違う"
            );
        }
    }
}

/// oracle: `lpi` は inch あたりの線数なので、セル幅は `25.4/lpi` mm
/// 最初のセル中心は原点から半セル (= `12.7/lpi` mm) の位置に来る
#[test]
fn halftone_grid_pitch_comes_from_lpi_and_is_cell_centered() {
    let bounds = Bounds::new(0.0, 0.0, 20.0, 20.0);
    let lpi = 10.0;
    let cell = 25.4 / lpi;
    let found = circles(&halftone(lpi, 0.4, &bounds, &|_, _| 0.0));
    let mut xs: Vec<f64> = found.iter().map(|&(x, _, _)| x).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    assert!(xs.len() >= 3, "列が 3 つ未満しか出ていない");
    assert!(
        (xs[0] - cell * 0.5).abs() < 1e-12,
        "最初の列 {} がセル中心 {} にない (半セルのずれ)",
        xs[0],
        cell * 0.5
    );
    for pair in xs.windows(2) {
        assert!(
            (pair[1] - pair[0] - cell).abs() < 1e-12,
            "列間隔 {} が 25.4/lpi = {cell} と違う",
            pair[1] - pair[0]
        );
    }
}

/// AM ハーフトーンの階調特性を明示的に固定する
///
/// ⚠️ 実装は**半径**を濃度に比例させるので、面積は `(1−b)²` に比例する
/// 印刷で言う標準の AM (面積 = 濃度) ではないので、中間調が理論値より淡く出る
/// 「そういう設計」であることをここで可視化しておく (変えるなら仕様判断)
#[test]
fn halftone_tone_response_is_radius_linear_not_area_linear() {
    let bounds = Bounds::new(0.0, 0.0, 40.0, 40.0);
    let max_radius = 0.6;
    let ink = 0.5_f64; // 50% 濃度 (brightness = 0.5)
    let found = circles(&halftone(12.7, max_radius, &bounds, &|_, _| 1.0 - ink));
    let cell = 25.4 / 12.7;
    let covered: f64 =
        found.iter().map(|&(_, _, r)| PI * r * r).sum::<f64>() / (found.len() as f64 * cell * cell);
    let radius_linear = PI * (max_radius * ink).powi(2) / (cell * cell);
    let area_linear = PI * max_radius.powi(2) * ink / (cell * cell);
    assert!(
        (covered - radius_linear).abs() < 1e-12,
        "被覆率 {covered} が半径線形モデル {radius_linear} と違う"
    );
    assert!(
        (covered - area_linear).abs() > 1e-3,
        "被覆率が面積線形モデル {area_linear} と一致してしまった \
         (実装が変わったならこの test の前提を見直すこと)"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  dither
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Bayer 4x4 の閾値行列 (0..15 の順列を 16 で割ったもの)
const BAYER4_ORDER: [u32; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

fn dot_density(algorithm: DitherAlgorithm, brightness: f64, dpi: f64, bounds: &Bounds) -> f64 {
    let cell = 25.4 / dpi;
    let cols = (bounds.w / cell).ceil() as usize;
    let rows = (bounds.h / cell).ceil() as usize;
    let found = dots(&dither(algorithm, dpi, bounds, &|_, _| brightness));
    found.len() as f64 / (cols * rows) as f64
}

/// oracle: 一様な輝度 b に対する Bayer のドット密度は
/// `#{m ∈ 0..16 : m/16 > b} / 16` — 閾値行列が 0..15 の順列であることから決まる
#[test]
fn bayer_dot_density_matches_the_threshold_matrix() {
    // 4 の倍数の格子にして端数の影響を消す
    let bounds = Bounds::new(0.0, 0.0, 25.4 * 4.0, 25.4 * 4.0);
    for brightness in [0.0_f64, 0.25, 0.5, 0.75, 1.0] {
        let expected = f64::from(
            BAYER4_ORDER
                .iter()
                .filter(|&&m| f64::from(m) / 16.0 > brightness)
                .count() as u32,
        ) / 16.0;
        let got = dot_density(DitherAlgorithm::Bayer4x4, brightness, 4.0, &bounds);
        assert!(
            (got - expected).abs() < 1e-12,
            "b={brightness}: Bayer のドット密度 {got} が閾値行列の閉形式 {expected} と違う"
        );
    }
}

/// oracle: 誤差拡散核の重みの総和が divisor に等しければ階調は保存される
/// ⇒ 一様輝度 b に対する出力ドット密度は 1−b に収束する
///
/// 許容 0.03 は境界で捨てられる誤差の上限から取っている (周長/面積のオーダー)
#[test]
fn error_diffusion_preserves_mean_tone() {
    let bounds = Bounds::new(0.0, 0.0, 100.0, 100.0);
    for algorithm in [
        DitherAlgorithm::FloydSteinberg,
        DitherAlgorithm::Stucki,
        DitherAlgorithm::Jarvis,
    ] {
        for brightness in [0.25_f64, 0.5, 0.75] {
            let got = dot_density(algorithm, brightness, 25.4, &bounds);
            let expected = 1.0 - brightness;
            assert!(
                (got - expected).abs() < 0.03,
                "{algorithm:?} b={brightness}: ドット密度 {got} が階調保存 1−b = {expected} から外れた \
                 (誤差拡散核の重み和が divisor と一致していない可能性)"
            );
        }
    }
}

/// oracle: 一様輝度 `b = (m+0.5)/16` でドットが出るのは、閾値行列の値が m より
/// 大きいセルだけ ⇒ **ドットの出る位置が閾値行列の配置そのもの**として決まる
///
/// 密度だけを見ると閾値行列の 2 要素を入れ替えても (順列のままなので) 気付けない
/// Bayer の要件は閾値が空間的に分散していることなので、配置まで固定する
#[test]
fn bayer_dot_positions_follow_the_threshold_matrix_layout() {
    let dpi = 4.0;
    let cell = 25.4 / dpi;
    let bounds = Bounds::new(0.0, 0.0, cell * 8.0, cell * 8.0);
    for m in 0..16_u32 {
        let brightness = (f64::from(m) + 0.5) / 16.0;
        let found = dots(&dither(DitherAlgorithm::Bayer4x4, dpi, &bounds, &|_, _| {
            brightness
        }));
        let mut got: Vec<(usize, usize)> = found
            .iter()
            .map(|&(x, y)| {
                let col = ((x - bounds.x) / cell - 0.5).round() as usize;
                let row = ((y - bounds.y) / cell - 0.5).round() as usize;
                (row % 4, col % 4)
            })
            .collect();
        got.sort_unstable();
        got.dedup();
        let want: Vec<(usize, usize)> = (0..4_usize)
            .flat_map(|r| (0..4_usize).map(move |c| (r, c)))
            .filter(|&(r, c)| BAYER4_ORDER[r * 4 + c] > m)
            .collect();
        assert_eq!(
            got, want,
            "b=(m+0.5)/16 (m={m}): ドットが出るセルの配置が Bayer 閾値行列と一致しない"
        );
    }
}

/// oracle: 重み和が divisor に一致する核は、**格子を細かくすると階調誤差が 0 に
/// 収束する** (捨てられるのは境界の分だけで、面積比 O(1/N) で減る)
///
/// 重みが 1 つでも足りない核は毎セルで誤差を取りこぼすので、細かくしても
/// 系統誤差が残って下げ止まる 一様輝度 0.5 は対称で差が消えるため、階調の
/// 端 (0.1) で測る
#[test]
fn error_diffusion_tone_error_vanishes_as_the_grid_refines() {
    let bounds = Bounds::new(0.0, 0.0, 100.0, 100.0);
    let brightness = 0.1_f64;
    for algorithm in [
        DitherAlgorithm::FloydSteinberg,
        DitherAlgorithm::Stucki,
        DitherAlgorithm::Jarvis,
    ] {
        // dpi 10.16 → 2.5 mm セル (40×40) / dpi 50.8 → 0.5 mm セル (200×200)
        let coarse =
            (dot_density(algorithm, brightness, 10.16, &bounds) - (1.0 - brightness)).abs();
        let fine = (dot_density(algorithm, brightness, 50.8, &bounds) - (1.0 - brightness)).abs();
        assert!(
            fine < 0.005,
            "{algorithm:?}: 200×200 でも階調誤差が {fine} 残る (核の重み和が divisor と違う)"
        );
        assert!(
            fine * 3.0 < coarse,
            "{algorithm:?}: 階調誤差が 40×40 の {coarse} から 200×200 の {fine} へ収束していない \
             (境界で捨てる分なら面積比で減るはず)"
        );
    }
}

/// Atkinson は誤差の 6/8 しか配らないので階調が**保存されない**
/// 既知の設計上の性質なので、保存しないことを明示的に固定する
#[test]
fn atkinson_loses_tone_because_it_diffuses_only_three_quarters() {
    let bounds = Bounds::new(0.0, 0.0, 100.0, 100.0);
    let brightness = 0.75_f64;
    let got = dot_density(DitherAlgorithm::Atkinson, brightness, 25.4, &bounds);
    assert!(
        (got - (1.0 - brightness)).abs() > 0.05,
        "Atkinson が階調を保存してしまった (密度 {got}) — 6/8 拡散の設計と合わない"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  turing (Gray-Scott 反応拡散)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 定常パターンが出る標準的な Gray-Scott parameter
const GS_FEED: f64 = 0.055;
const GS_KILL: f64 = 0.062;

fn turing_dots(resolution: u32, iterations: u32, threshold: f64) -> usize {
    let bounds = Bounds::new(0.0, 0.0, 50.0, 50.0);
    dots(&turing(
        GS_FEED, GS_KILL, resolution, iterations, &bounds, threshold,
    ))
    .len()
}

/// oracle: 閾値を上げるとドット集合は単調に縮む (同じ場に対する上側集合なので
/// 包含関係が保たれる) — 場の計算が閾値に依存していないことの確認でもある
#[test]
fn turing_dot_count_is_monotone_in_threshold() {
    let mut previous = usize::MAX;
    for threshold in [0.05_f64, 0.1, 0.2, 0.3, 0.5] {
        let count = turing_dots(48, 200, threshold);
        assert!(
            count <= previous,
            "threshold={threshold} でドット数が {previous} → {count} と増えた \
             (上側集合は閾値に対して単調でなければならない)"
        );
        previous = count;
    }
}

/// oracle: 反応拡散は決定論的な差分方程式なので、同じ入力は同じ出力になる
#[test]
fn turing_is_deterministic() {
    let bounds = Bounds::new(0.0, 0.0, 50.0, 50.0);
    let run = || dots(&turing(GS_FEED, GS_KILL, 32, 120, &bounds, 0.15));
    assert_eq!(run(), run(), "同じ入力で出力が変わった");
}

/// oracle: `iterations = 0` なら場は初期条件のまま — v はシード領域で 0.25、
/// それ以外は 0 なので、閾値 0.25 を挟んでドットの有無が切り替わる
#[test]
fn turing_without_iterations_only_shows_the_seed_amplitude() {
    assert!(
        turing_dots(40, 0, 0.2) > 0,
        "閾値 0.2 (シード 0.25 未満) でドットが 1 つも出ない"
    );
    assert_eq!(
        turing_dots(40, 0, 0.3),
        0,
        "閾値 0.3 (シード 0.25 超) でドットが出た"
    );
}

/// oracle: Gray-Scott の v は 1 を超えない (u,v は割合なので有界)
/// 閾値を 1 より上に置けば、どれだけ回してもドットは出ない
#[test]
fn turing_concentration_stays_below_one() {
    assert_eq!(
        turing_dots(32, 300, 1.0001),
        0,
        "v が 1 を超えたセルがある (Gray-Scott の有界性に反する)"
    );
}

/// oracle: 格子が 4 未満ではラプラシアンの近傍が自分自身に折り返すので計算しない
#[test]
fn turing_below_minimum_resolution_is_empty() {
    for resolution in 0..4_u32 {
        assert_eq!(
            turing_dots(resolution, 10, 0.1),
            0,
            "resolution={resolution} で出力が出た"
        );
    }
    assert!(
        turing_dots(4, 10, 0.05) > 0,
        "resolution=4 は計算される下限のはず"
    );
}

/// oracle: 両端は厳密 — 真っ黒は全セルがドット、真っ白は 1 つも出ない
#[test]
fn dither_endpoints_are_exact() {
    let bounds = Bounds::new(0.0, 0.0, 50.0, 50.0);
    for algorithm in [
        DitherAlgorithm::FloydSteinberg,
        DitherAlgorithm::Atkinson,
        DitherAlgorithm::Stucki,
        DitherAlgorithm::Jarvis,
    ] {
        let black = dot_density(algorithm, 0.0, 25.4, &bounds);
        let white = dot_density(algorithm, 1.0, 25.4, &bounds);
        assert!(
            (black - 1.0).abs() < 1e-12,
            "{algorithm:?}: 黒でドット密度が {black} (1.0 でない)"
        );
        assert!(
            white.abs() < 1e-12,
            "{algorithm:?}: 白でドット密度が {white} (0.0 でない)"
        );
    }
}
