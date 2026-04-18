//! レーザー彫刻向け2Dパターン生成
//!
//! SDF距離場を使わず、2D幾何学パターンをSVGパスデータとして生成する。
//! LOL DSL → LaserPattern → SVG のパイプライン。

use std::f64::consts::PI;
use std::fmt::Write;

// ──────────────────────────────────────────────────────
//  基本型
// ──────────────────────────────────────────────────────

/// 2Dレーザー要素
#[derive(Debug, Clone)]
pub enum LaserElement {
    /// 直線セグメント (x1, y1, x2, y2)
    Line(f64, f64, f64, f64),
    /// 円 (cx, cy, radius)
    Circle(f64, f64, f64),
    /// ドット (cx, cy) — 単一レーザーパルス
    Dot(f64, f64),
    /// ポリライン (連続する点列)
    Polyline(Vec<(f64, f64)>),
}

/// 2Dバウンディング矩形 (x, y, width, height)
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Bounds {
    #[must_use]
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// 点がバウンディング内か
    #[must_use]
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }

    /// 中心座標
    #[must_use]
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }
}

// ──────────────────────────────────────────────────────
//  Phase A: hatch / crosshatch / density_hatch
// ──────────────────────────────────────────────────────

/// 線形ハッチングパターン生成
///
/// `angle`: 度数法（0=水平、90=垂直、45=斜め）
/// `spacing`: 線間隔 (mm)
/// `bounds`: クリッピング矩形
///
/// 矩形内を平行線で埋める。角度は反時計回り。
#[must_use]
pub fn hatch(angle_deg: f64, spacing: f64, bounds: &Bounds) -> Vec<LaserElement> {
    if spacing <= 0.0 {
        return Vec::new();
    }

    let angle = angle_deg * PI / 180.0;
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    // 矩形の4頂点
    let corners = [
        (bounds.x, bounds.y),
        (bounds.x + bounds.w, bounds.y),
        (bounds.x + bounds.w, bounds.y + bounds.h),
        (bounds.x, bounds.y + bounds.h),
    ];

    // 法線方向への各頂点の射影距離
    let projections: Vec<f64> = corners
        .iter()
        .map(|&(cx, cy)| -sin_a * cx + cos_a * cy)
        .collect();
    let proj_min = projections.iter().cloned().fold(f64::INFINITY, f64::min);
    let proj_max = projections
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    let mut elements = Vec::new();
    let mut d = (proj_min / spacing).ceil() * spacing;

    while d <= proj_max {
        // d = -sin_a * x + cos_a * y の直線と矩形の交点を求める
        if let Some((x1, y1, x2, y2)) = clip_line_to_rect(cos_a, sin_a, d, bounds) {
            elements.push(LaserElement::Line(x1, y1, x2, y2));
        }
        d += spacing;
    }

    elements
}

/// クロスハッチングパターン（0° + 90°）
///
/// `spacing`: 線間隔 (mm)
/// `bounds`: クリッピング矩形
#[must_use]
pub fn crosshatch(spacing: f64, bounds: &Bounds) -> Vec<LaserElement> {
    let mut elements = hatch(0.0, spacing, bounds);
    elements.extend(hatch(90.0, spacing, bounds));
    elements
}

/// 可変密度ハッチング
///
/// SDF距離に基づいて線間隔を変調する。中心に近いほど密、端ほど疎。
///
/// `min_spacing`: 最密部の線間隔 (mm)
/// `max_spacing`: 最疎部の線間隔 (mm)
/// `angle_deg`: ハッチ角度（度数法）
/// `bounds`: クリッピング矩形
/// `density_fn`: (x, y) → 0.0..1.0 の密度関数。1.0=最密、0.0=最疎
#[must_use]
pub fn density_hatch(
    min_spacing: f64,
    max_spacing: f64,
    angle_deg: f64,
    bounds: &Bounds,
    density_fn: &dyn Fn(f64, f64) -> f64,
) -> Vec<LaserElement> {
    if min_spacing <= 0.0 || max_spacing <= 0.0 {
        return Vec::new();
    }

    let angle = angle_deg * PI / 180.0;
    let cos_a = angle.cos();
    let sin_a = angle.sin();

    let corners = [
        (bounds.x, bounds.y),
        (bounds.x + bounds.w, bounds.y),
        (bounds.x + bounds.w, bounds.y + bounds.h),
        (bounds.x, bounds.y + bounds.h),
    ];
    let projections: Vec<f64> = corners
        .iter()
        .map(|&(cx, cy)| -sin_a * cx + cos_a * cy)
        .collect();
    let proj_min = projections.iter().cloned().fold(f64::INFINITY, f64::min);
    let proj_max = projections
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    let mut elements = Vec::new();
    let mut d = proj_min;

    while d <= proj_max {
        if let Some((x1, y1, x2, y2)) = clip_line_to_rect(cos_a, sin_a, d, bounds) {
            elements.push(LaserElement::Line(x1, y1, x2, y2));

            // 次の線までの間隔を、この線の中点の密度で決定
            let mx = (x1 + x2) * 0.5;
            let my = (y1 + y2) * 0.5;
            let density = density_fn(mx, my).clamp(0.0, 1.0);
            let spacing = max_spacing - (max_spacing - min_spacing) * density;
            d += spacing;
        } else {
            d += min_spacing;
        }
    }

    elements
}

/// 直線 (`-sin_a * x + cos_a * y = d`) と矩形のクリッピング
///
/// Cohen-Sutherland 的に矩形の4辺との交点を求め、矩形内のセグメントを返す。
fn clip_line_to_rect(
    cos_a: f64,
    sin_a: f64,
    d: f64,
    bounds: &Bounds,
) -> Option<(f64, f64, f64, f64)> {
    let x_min = bounds.x;
    let x_max = bounds.x + bounds.w;
    let y_min = bounds.y;
    let y_max = bounds.y + bounds.h;

    let mut intersections = Vec::with_capacity(4);
    let eps = 1e-12;

    // 上辺 y = y_min: x = (d - cos_a * y_min) / (-sin_a)
    if sin_a.abs() > eps {
        let x = (d - cos_a * y_min) / (-sin_a);
        if x >= x_min - eps && x <= x_max + eps {
            intersections.push((x.clamp(x_min, x_max), y_min));
        }
    }
    // 下辺 y = y_max: x = (d - cos_a * y_max) / (-sin_a)
    if sin_a.abs() > eps {
        let x = (d - cos_a * y_max) / (-sin_a);
        if x >= x_min - eps && x <= x_max + eps {
            intersections.push((x.clamp(x_min, x_max), y_max));
        }
    }
    // 左辺 x = x_min: y = (d + sin_a * x_min) / cos_a
    if cos_a.abs() > eps {
        let y = (d + sin_a * x_min) / cos_a;
        if y >= y_min - eps && y <= y_max + eps {
            intersections.push((x_min, y.clamp(y_min, y_max)));
        }
    }
    // 右辺 x = x_max: y = (d + sin_a * x_max) / cos_a
    if cos_a.abs() > eps {
        let y = (d + sin_a * x_max) / cos_a;
        if y >= y_min - eps && y <= y_max + eps {
            intersections.push((x_max, y.clamp(y_min, y_max)));
        }
    }

    // 重複除去（角の交点で2回ヒットする場合）
    intersections.dedup_by(|a, b| (a.0 - b.0).abs() < eps && (a.1 - b.1).abs() < eps);

    if intersections.len() >= 2 {
        Some((
            intersections[0].0,
            intersections[0].1,
            intersections[1].0,
            intersections[1].1,
        ))
    } else {
        None
    }
}

// ──────────────────────────────────────────────────────
//  Phase B: halftone / dither
// ──────────────────────────────────────────────────────

/// AMハーフトーン（振幅変調）
///
/// 等間隔グリッド上にドットを配置し、ドット半径を輝度で変調する。
///
/// `lpi`: lines per inch（グリッド密度）
/// `max_radius`: 最大ドット半径 (mm)
/// `bounds`: クリッピング矩形
/// `brightness_fn`: (x, y) → 0.0(黒=最大ドット)..1.0(白=ドットなし)
#[must_use]
pub fn halftone(
    lpi: f64,
    max_radius: f64,
    bounds: &Bounds,
    brightness_fn: &dyn Fn(f64, f64) -> f64,
) -> Vec<LaserElement> {
    if lpi <= 0.0 || max_radius <= 0.0 {
        return Vec::new();
    }

    let cell_size = 25.4 / lpi; // mm per cell
    let mut elements = Vec::new();

    let cols = ((bounds.w / cell_size).ceil() as i32).max(0);
    let rows = ((bounds.h / cell_size).ceil() as i32).max(0);

    for row in 0..rows {
        for col in 0..cols {
            let cx = bounds.x + (col as f64 + 0.5) * cell_size;
            let cy = bounds.y + (row as f64 + 0.5) * cell_size;

            if !bounds.contains(cx, cy) {
                continue;
            }

            let brightness = brightness_fn(cx, cy).clamp(0.0, 1.0);
            let radius = max_radius * (1.0 - brightness);

            if radius > 0.01 {
                elements.push(LaserElement::Circle(cx, cy, radius));
            }
        }
    }

    elements
}

/// ディザリングアルゴリズム
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DitherAlgorithm {
    FloydSteinberg,
    Atkinson,
    Stucki,
    Jarvis,
    Bayer4x4,
}

/// ディザリングパターン生成
///
/// 輝度マップをバイナリ（ドット有/無）に変換する。
///
/// `algorithm`: ディザリング手法
/// `dpi`: 解像度
/// `bounds`: 対象領域
/// `brightness_fn`: (x, y) → 0.0(黒)..1.0(白)
#[must_use]
pub fn dither(
    algorithm: DitherAlgorithm,
    dpi: f64,
    bounds: &Bounds,
    brightness_fn: &dyn Fn(f64, f64) -> f64,
) -> Vec<LaserElement> {
    if dpi <= 0.0 {
        return Vec::new();
    }

    let pixel_size = 25.4 / dpi;
    let cols = ((bounds.w / pixel_size).ceil() as usize).max(1);
    let rows = ((bounds.h / pixel_size).ceil() as usize).max(1);

    // 輝度マップをサンプリング
    let mut grid: Vec<Vec<f64>> = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut scanline = Vec::with_capacity(cols);
        for col in 0..cols {
            let x = bounds.x + (col as f64 + 0.5) * pixel_size;
            let y = bounds.y + (row as f64 + 0.5) * pixel_size;
            let b = brightness_fn(x, y).clamp(0.0, 1.0);
            scanline.push(b);
        }
        grid.push(scanline);
    }

    match algorithm {
        DitherAlgorithm::Bayer4x4 => dither_ordered(&grid, pixel_size, bounds, cols, rows),
        _ => dither_error_diffusion(algorithm, &mut grid, pixel_size, bounds, cols, rows),
    }
}

/// 誤差拡散ディザリング
fn dither_error_diffusion(
    algorithm: DitherAlgorithm,
    grid: &mut [Vec<f64>],
    pixel_size: f64,
    bounds: &Bounds,
    cols: usize,
    rows: usize,
) -> Vec<LaserElement> {
    // (dx, dy, weight) — 合計が divisor になる
    let (offsets, divisor): (&[(i32, i32, f64)], f64) = match algorithm {
        DitherAlgorithm::FloydSteinberg => {
            (&[(1, 0, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)], 16.0)
        }
        DitherAlgorithm::Atkinson => (
            &[
                (1, 0, 1.0),
                (2, 0, 1.0),
                (-1, 1, 1.0),
                (0, 1, 1.0),
                (1, 1, 1.0),
                (0, 2, 1.0),
            ],
            8.0, // 6/8 = 75% のみ拡散
        ),
        DitherAlgorithm::Stucki => (
            &[
                (1, 0, 8.0),
                (2, 0, 4.0),
                (-2, 1, 2.0),
                (-1, 1, 4.0),
                (0, 1, 8.0),
                (1, 1, 4.0),
                (2, 1, 2.0),
                (-2, 2, 1.0),
                (-1, 2, 2.0),
                (0, 2, 4.0),
                (1, 2, 2.0),
                (2, 2, 1.0),
            ],
            42.0,
        ),
        DitherAlgorithm::Jarvis => (
            &[
                (1, 0, 7.0),
                (2, 0, 5.0),
                (-2, 1, 3.0),
                (-1, 1, 5.0),
                (0, 1, 7.0),
                (1, 1, 5.0),
                (2, 1, 3.0),
                (-2, 2, 1.0),
                (-1, 2, 3.0),
                (0, 2, 5.0),
                (1, 2, 3.0),
                (2, 2, 1.0),
            ],
            48.0,
        ),
        DitherAlgorithm::Bayer4x4 => unreachable!(),
    };

    let mut dots = Vec::new();

    for y in 0..rows {
        for x in 0..cols {
            let old = grid[y][x];
            let new_val = if old < 0.5 { 0.0 } else { 1.0 };
            let error = old - new_val;

            grid[y][x] = new_val;

            // 誤差拡散
            for &(dx, dy, weight) in offsets {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && (nx as usize) < cols && ny >= 0 && (ny as usize) < rows {
                    grid[ny as usize][nx as usize] += error * weight / divisor;
                }
            }

            // 黒ピクセル → ドット
            if new_val < 0.5 {
                let px = bounds.x + (x as f64 + 0.5) * pixel_size;
                let py = bounds.y + (y as f64 + 0.5) * pixel_size;
                dots.push(LaserElement::Dot(px, py));
            }
        }
    }

    dots
}

/// 順序ディザリング (Bayer 4x4)
fn dither_ordered(
    grid: &[Vec<f64>],
    pixel_size: f64,
    bounds: &Bounds,
    cols: usize,
    rows: usize,
) -> Vec<LaserElement> {
    #[rustfmt::skip]
    const BAYER4: [[f64; 4]; 4] = [
        [ 0.0/16.0,  8.0/16.0,  2.0/16.0, 10.0/16.0],
        [12.0/16.0,  4.0/16.0, 14.0/16.0,  6.0/16.0],
        [ 3.0/16.0, 11.0/16.0,  1.0/16.0,  9.0/16.0],
        [15.0/16.0,  7.0/16.0, 13.0/16.0,  5.0/16.0],
    ];

    let mut dots = Vec::new();

    for y in 0..rows {
        for x in 0..cols {
            let threshold = BAYER4[y & 3][x & 3];
            if grid[y][x] < threshold {
                let px = bounds.x + (x as f64 + 0.5) * pixel_size;
                let py = bounds.y + (y as f64 + 0.5) * pixel_size;
                dots.push(LaserElement::Dot(px, py));
            }
        }
    }

    dots
}

// ──────────────────────────────────────────────────────
//  Phase C: 装飾パターン
// ──────────────────────────────────────────────────────

/// ギョッシェ / スピログラフ（ヒポトロコイド曲線）
///
/// `big_r`: 固定円半径
/// `small_r`: 転がり円半径
/// `pen_d`: ペンオフセット
/// `steps`: 描画ステップ数（多いほど滑らか）
/// `bounds`: クリッピング（中心をboundsの中心に配置）
#[must_use]
pub fn guilloche(
    big_r: f64,
    small_r: f64,
    pen_d: f64,
    steps: u32,
    bounds: &Bounds,
) -> Vec<LaserElement> {
    if small_r.abs() < 1e-12 || steps == 0 {
        return Vec::new();
    }

    let (cx, cy) = bounds.center();

    // 完全な1周のためのt範囲を計算
    let t_max = 2.0 * PI * small_r / gcd_f64(big_r, small_r);
    let dt = t_max / steps as f64;

    let mut points = Vec::with_capacity(steps as usize + 1);
    for i in 0..=steps {
        let t = i as f64 * dt;
        let diff = big_r - small_r;
        let ratio = diff / small_r;
        let x = cx + diff * t.cos() + pen_d * (ratio * t).cos();
        let y = cy + diff * t.sin() - pen_d * (ratio * t).sin();
        points.push((x, y));
    }

    vec![LaserElement::Polyline(points)]
}

/// リサジュー曲線
///
/// `freq_a`: X周波数
/// `freq_b`: Y周波数
/// `delta`: 位相差（ラジアン）
/// `amplitude`: 振幅 (mm)
/// `steps`: 描画ステップ数
/// `bounds`: 中心配置用
#[must_use]
pub fn lissajous(
    freq_a: f64,
    freq_b: f64,
    delta: f64,
    amplitude: f64,
    steps: u32,
    bounds: &Bounds,
) -> Vec<LaserElement> {
    if steps == 0 {
        return Vec::new();
    }

    let (cx, cy) = bounds.center();
    let dt = 2.0 * PI / steps as f64;

    let mut points = Vec::with_capacity(steps as usize + 1);
    for i in 0..=steps {
        let t = i as f64 * dt;
        let x = cx + amplitude * (freq_a * t + delta).sin();
        let y = cy + amplitude * (freq_b * t).sin();
        points.push((x, y));
    }

    vec![LaserElement::Polyline(points)]
}

/// ローズカーブ（ロドネア曲線）
///
/// `k`: 花弁パラメータ（整数で閉曲線、奇数→k枚、偶数→2k枚）
/// `amplitude`: 振幅 (mm)
/// `steps`: 描画ステップ数
/// `bounds`: 中心配置用
#[must_use]
pub fn rose(k: f64, amplitude: f64, steps: u32, bounds: &Bounds) -> Vec<LaserElement> {
    if steps == 0 {
        return Vec::new();
    }

    let (cx, cy) = bounds.center();
    // k が整数なら 2π で閉じる（奇数）か π で閉じる（偶数として扱う）
    let t_max = 2.0 * PI;
    let dt = t_max / steps as f64;

    let mut points = Vec::with_capacity(steps as usize + 1);
    for i in 0..=steps {
        let t = i as f64 * dt;
        let r = amplitude * (k * t).cos();
        let x = cx + r * t.cos();
        let y = cy + r * t.sin();
        points.push((x, y));
    }

    vec![LaserElement::Polyline(points)]
}

/// フィロタキシス（向日葵螺旋）
///
/// `n`: ドット数
/// `scale`: スケール係数（大きいほど広がる）
/// `bounds`: 中心配置用
#[must_use]
pub fn phyllotaxis(n: u32, scale: f64, bounds: &Bounds) -> Vec<LaserElement> {
    let (cx, cy) = bounds.center();
    let golden_angle = 137.508_f64 * PI / 180.0;

    let mut dots = Vec::with_capacity(n as usize);
    for i in 0..n {
        let theta = i as f64 * golden_angle;
        let r = scale * (i as f64).sqrt();
        let x = cx + r * theta.cos();
        let y = cy + r * theta.sin();

        if bounds.contains(x, y) {
            dots.push(LaserElement::Dot(x, y));
        }
    }

    dots
}

/// チューリングパターン（Gray-Scottモデル反応拡散）
///
/// `feed`: フィード率 F (0.01-0.08)
/// `kill`: キル率 k (0.04-0.07)
/// `resolution`: グリッド解像度（一辺のセル数）
/// `iterations`: シミュレーションステップ数
/// `bounds`: 出力領域
/// `threshold`: 閾値（v がこの値以上のセルをドットとして出力）
#[must_use]
pub fn turing(
    feed: f64,
    kill: f64,
    resolution: u32,
    iterations: u32,
    bounds: &Bounds,
    threshold: f64,
) -> Vec<LaserElement> {
    let n = resolution as usize;
    if n < 4 {
        return Vec::new();
    }

    let du = 0.16_f64;
    let dv = 0.08_f64;
    let dt = 1.0_f64;

    // U=1, V=0 を初期条件、中央付近にV=0.25の種を複数箇所にまく
    let mut u = vec![vec![1.0_f64; n]; n];
    let mut v = vec![vec![0.0_f64; n]; n];

    // 中央の大きなシード
    let center = n / 2;
    let seed_r = (n / 5).max(2);
    for row in center.saturating_sub(seed_r)..=(center + seed_r).min(n - 1) {
        for col in center.saturating_sub(seed_r)..=(center + seed_r).min(n - 1) {
            u[row][col] = 0.5;
            v[row][col] = 0.25;
        }
    }
    // 四隅にも小さいシードを配置（パターンの伝播を促進）
    let small_r = (n / 8).max(1);
    for &(sy, sx) in &[
        (n / 4, n / 4),
        (n / 4, 3 * n / 4),
        (3 * n / 4, n / 4),
        (3 * n / 4, 3 * n / 4),
    ] {
        for row in sy.saturating_sub(small_r)..=(sy + small_r).min(n - 1) {
            for col in sx.saturating_sub(small_r)..=(sx + small_r).min(n - 1) {
                u[row][col] = 0.5;
                v[row][col] = 0.25;
            }
        }
    }

    let mut u_next = vec![vec![0.0_f64; n]; n];
    let mut v_next = vec![vec![0.0_f64; n]; n];

    for _ in 0..iterations {
        for y in 0..n {
            for x in 0..n {
                // ラプラシアン (周期境界)
                let yp = if y == 0 { n - 1 } else { y - 1 };
                let yn = if y == n - 1 { 0 } else { y + 1 };
                let xp = if x == 0 { n - 1 } else { x - 1 };
                let xn = if x == n - 1 { 0 } else { x + 1 };

                let lap_u = u[yp][x] + u[yn][x] + u[y][xp] + u[y][xn] - 4.0 * u[y][x];
                let lap_v = v[yp][x] + v[yn][x] + v[y][xp] + v[y][xn] - 4.0 * v[y][x];

                let uv2 = u[y][x] * v[y][x] * v[y][x];
                u_next[y][x] = u[y][x] + dt * (du * lap_u - uv2 + feed * (1.0 - u[y][x]));
                v_next[y][x] = v[y][x] + dt * (dv * lap_v + uv2 - (feed + kill) * v[y][x]);
            }
        }
        std::mem::swap(&mut u, &mut u_next);
        std::mem::swap(&mut v, &mut v_next);
    }

    // V がしきい値以上のセルをドットとして出力
    let cell_w = bounds.w / n as f64;
    let cell_h = bounds.h / n as f64;
    let mut dots = Vec::new();

    for y in 0..n {
        for x in 0..n {
            if v[y][x] >= threshold {
                let px = bounds.x + (x as f64 + 0.5) * cell_w;
                let py = bounds.y + (y as f64 + 0.5) * cell_h;
                dots.push(LaserElement::Dot(px, py));
            }
        }
    }

    dots
}

// ──────────────────────────────────────────────────────
//  SVG出力
// ──────────────────────────────────────────────────────

/// LaserElement群をSVG文字列に変換
///
/// `elements`: パターン要素
/// `stroke_color`: 線/ドットの色（SVG fill/stroke）
/// `stroke_width`: 線幅 (mm)
/// `bounds`: SVGのviewBox用
#[must_use]
pub fn elements_to_svg(
    elements: &[LaserElement],
    stroke_color: &str,
    stroke_width: f64,
    bounds: &Bounds,
) -> String {
    let mut svg = String::with_capacity(elements.len() * 80);
    let _ = write!(
        svg,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\"\n\
         \x20    viewBox=\"{} {} {} {}\"\n\
         \x20    width=\"{}mm\" height=\"{}mm\">\n",
        bounds.x, bounds.y, bounds.w, bounds.h, bounds.w, bounds.h
    );

    for elem in elements {
        match elem {
            LaserElement::Line(x1, y1, x2, y2) => {
                let _ = write!(
                    svg,
                    "<line x1=\"{x1:.3}\" y1=\"{y1:.3}\" x2=\"{x2:.3}\" y2=\"{y2:.3}\" \
                     stroke=\"{stroke_color}\" stroke-width=\"{stroke_width}\"/>\n"
                );
            }
            LaserElement::Circle(cx, cy, r) => {
                let _ = write!(
                    svg,
                    "<circle cx=\"{cx:.3}\" cy=\"{cy:.3}\" r=\"{r:.3}\" \
                     fill=\"{stroke_color}\"/>\n"
                );
            }
            LaserElement::Dot(x, y) => {
                let half = stroke_width * 0.5;
                let _ = write!(
                    svg,
                    "<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{stroke_width:.3}\" \
                     height=\"{stroke_width:.3}\" fill=\"{stroke_color}\"/>\n",
                    x - half,
                    y - half
                );
            }
            LaserElement::Polyline(points) => {
                if points.is_empty() {
                    continue;
                }
                svg.push_str("<polyline points=\"");
                for (i, (x, y)) in points.iter().enumerate() {
                    if i > 0 {
                        svg.push(' ');
                    }
                    let _ = write!(svg, "{x:.3},{y:.3}");
                }
                let _ = write!(
                    svg,
                    "\" fill=\"none\" stroke=\"{stroke_color}\" \
                     stroke-width=\"{stroke_width}\"/>\n"
                );
            }
        }
    }

    svg.push_str("</svg>\n");
    svg
}

// ──────────────────────────────────────────────────────
//  ユーティリティ
// ──────────────────────────────────────────────────────

/// 浮動小数の近似GCD（スピログラフの完全周期計算用）
fn gcd_f64(a: f64, b: f64) -> f64 {
    let mut x = a.abs();
    let mut y = b.abs();
    let eps = 1e-9;
    while y > eps {
        let t = y;
        y = x % y;
        if y < eps {
            break;
        }
        x = t;
    }
    x
}

// ──────────────────────────────────────────────────────
//  テスト
// ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn card_bounds() -> Bounds {
        Bounds::new(0.0, 0.0, 86.0, 54.0)
    }

    // --- hatch ---

    #[test]
    fn test_hatch_horizontal() {
        let elems = hatch(0.0, 2.0, &card_bounds());
        assert!(!elems.is_empty());
        // 水平線は y 座標が一定
        for elem in &elems {
            if let LaserElement::Line(_, y1, _, y2) = elem {
                assert!((y1 - y2).abs() < 1e-9, "水平線のy座標が不一致");
            }
        }
    }

    #[test]
    fn test_hatch_vertical() {
        let elems = hatch(90.0, 2.0, &card_bounds());
        assert!(!elems.is_empty());
        for elem in &elems {
            if let LaserElement::Line(x1, _, x2, _) = elem {
                assert!((x1 - x2).abs() < 1e-9, "垂直線のx座標が不一致");
            }
        }
    }

    #[test]
    fn test_hatch_diagonal() {
        let elems = hatch(45.0, 3.0, &card_bounds());
        assert!(!elems.is_empty());
    }

    #[test]
    fn test_hatch_zero_spacing() {
        let elems = hatch(0.0, 0.0, &card_bounds());
        assert!(elems.is_empty());
    }

    #[test]
    fn test_hatch_line_count() {
        let spacing = 2.0;
        let elems = hatch(0.0, spacing, &card_bounds());
        let expected = (54.0 / spacing).ceil() as usize;
        // 端数による±1の誤差を許容
        assert!(
            (elems.len() as i32 - expected as i32).unsigned_abs() <= 1,
            "線数 {} が期待値 {} と大幅に異なる",
            elems.len(),
            expected
        );
    }

    // --- crosshatch ---

    #[test]
    fn test_crosshatch() {
        let elems = crosshatch(3.0, &card_bounds());
        assert!(!elems.is_empty());
        let h_count = hatch(0.0, 3.0, &card_bounds()).len();
        let v_count = hatch(90.0, 3.0, &card_bounds()).len();
        assert_eq!(elems.len(), h_count + v_count);
    }

    // --- density_hatch ---

    #[test]
    fn test_density_hatch_uniform() {
        // 密度一定 → 等間隔ハッチと同等
        let elems = density_hatch(2.0, 2.0, 0.0, &card_bounds(), &|_, _| 0.5);
        assert!(!elems.is_empty());
    }

    #[test]
    fn test_density_hatch_center_dense() {
        let bounds = card_bounds();
        let (cx, cy) = bounds.center();
        let max_dist = (cx * cx + cy * cy).sqrt();
        let elems = density_hatch(0.5, 4.0, 0.0, &bounds, &|x, y| {
            let dx = x - cx;
            let dy = y - cy;
            1.0 - (dx * dx + dy * dy).sqrt() / max_dist
        });
        assert!(!elems.is_empty());
    }

    // --- halftone ---

    #[test]
    fn test_halftone_uniform_black() {
        let elems = halftone(20.0, 0.5, &card_bounds(), &|_, _| 0.0);
        // 全黒 → 全セルが最大半径
        assert!(!elems.is_empty());
        for elem in &elems {
            if let LaserElement::Circle(_, _, r) = elem {
                assert!((*r - 0.5).abs() < 0.01);
            }
        }
    }

    #[test]
    fn test_halftone_uniform_white() {
        let elems = halftone(20.0, 0.5, &card_bounds(), &|_, _| 1.0);
        // 全白 → ドットなし
        assert!(elems.is_empty());
    }

    #[test]
    fn test_halftone_gradient() {
        let bounds = card_bounds();
        let elems = halftone(30.0, 0.4, &bounds, &|x, _| x / bounds.w);
        assert!(!elems.is_empty());
    }

    // --- dither ---

    #[test]
    fn test_dither_floyd_steinberg() {
        let bounds = Bounds::new(0.0, 0.0, 10.0, 10.0);
        let elems = dither(DitherAlgorithm::FloydSteinberg, 50.0, &bounds, &|x, _| {
            x / 10.0
        });
        assert!(!elems.is_empty());
    }

    #[test]
    fn test_dither_atkinson() {
        let bounds = Bounds::new(0.0, 0.0, 10.0, 10.0);
        let elems = dither(DitherAlgorithm::Atkinson, 50.0, &bounds, &|_, y| y / 10.0);
        assert!(!elems.is_empty());
    }

    #[test]
    fn test_dither_stucki() {
        let bounds = Bounds::new(0.0, 0.0, 10.0, 10.0);
        let elems = dither(DitherAlgorithm::Stucki, 50.0, &bounds, &|x, y| {
            (x + y) / 20.0
        });
        assert!(!elems.is_empty());
    }

    #[test]
    fn test_dither_jarvis() {
        let bounds = Bounds::new(0.0, 0.0, 10.0, 10.0);
        let elems = dither(DitherAlgorithm::Jarvis, 50.0, &bounds, &|_, _| 0.3);
        assert!(!elems.is_empty());
    }

    #[test]
    fn test_dither_bayer() {
        let bounds = Bounds::new(0.0, 0.0, 10.0, 10.0);
        let elems = dither(DitherAlgorithm::Bayer4x4, 50.0, &bounds, &|x, _| x / 10.0);
        assert!(!elems.is_empty());
    }

    #[test]
    fn test_dither_all_white() {
        let bounds = Bounds::new(0.0, 0.0, 5.0, 5.0);
        let elems = dither(DitherAlgorithm::FloydSteinberg, 50.0, &bounds, &|_, _| 1.0);
        assert!(elems.is_empty());
    }

    // --- guilloche ---

    #[test]
    fn test_guilloche_basic() {
        let elems = guilloche(10.0, 7.0, 5.0, 1000, &card_bounds());
        assert_eq!(elems.len(), 1);
        if let LaserElement::Polyline(pts) = &elems[0] {
            assert!(pts.len() > 100);
        }
    }

    #[test]
    fn test_guilloche_equal_radii() {
        // R == r → 単なる円
        let elems = guilloche(5.0, 5.0, 3.0, 500, &card_bounds());
        assert_eq!(elems.len(), 1);
    }

    // --- lissajous ---

    #[test]
    fn test_lissajous_basic() {
        let elems = lissajous(3.0, 2.0, PI * 0.5, 20.0, 1000, &card_bounds());
        assert_eq!(elems.len(), 1);
        if let LaserElement::Polyline(pts) = &elems[0] {
            assert_eq!(pts.len(), 1001);
        }
    }

    // --- rose ---

    #[test]
    fn test_rose_3_petals() {
        let elems = rose(3.0, 20.0, 500, &card_bounds());
        assert_eq!(elems.len(), 1);
    }

    #[test]
    fn test_rose_4_petals() {
        let elems = rose(2.0, 20.0, 500, &card_bounds());
        assert_eq!(elems.len(), 1);
    }

    // --- phyllotaxis ---

    #[test]
    fn test_phyllotaxis_basic() {
        let elems = phyllotaxis(500, 0.5, &card_bounds());
        assert!(!elems.is_empty());
        assert!(elems.len() <= 500);
    }

    #[test]
    fn test_phyllotaxis_clipping() {
        // 小さいバウンズ → 多くのドットがクリップされる
        let small = Bounds::new(40.0, 24.0, 6.0, 6.0);
        let elems = phyllotaxis(1000, 0.5, &small);
        assert!(elems.len() < 1000);
    }

    // --- turing ---

    #[test]
    fn test_turing_spots() {
        let bounds = Bounds::new(0.0, 0.0, 20.0, 20.0);
        let elems = turing(0.055, 0.062, 64, 5000, &bounds, 0.15);
        assert!(!elems.is_empty());
    }

    #[test]
    fn test_turing_low_resolution() {
        let bounds = Bounds::new(0.0, 0.0, 10.0, 10.0);
        let elems = turing(0.040, 0.060, 3, 100, &bounds, 0.2);
        // 解像度 < 4 → 空
        assert!(elems.is_empty());
    }

    // --- SVG出力 ---

    #[test]
    fn test_elements_to_svg() {
        let bounds = card_bounds();
        let elems = hatch(0.0, 5.0, &bounds);
        let svg = elements_to_svg(&elems, "#FFFFFF", 0.1, &bounds);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("<line"));
    }

    #[test]
    fn test_svg_contains_polyline() {
        let bounds = card_bounds();
        let elems = guilloche(10.0, 7.0, 5.0, 200, &bounds);
        let svg = elements_to_svg(&elems, "#C0C0C0", 0.05, &bounds);
        assert!(svg.contains("<polyline"));
    }

    #[test]
    fn test_svg_contains_circles() {
        let bounds = card_bounds();
        let elems = halftone(20.0, 0.3, &bounds, &|_, _| 0.5);
        let svg = elements_to_svg(&elems, "#FFFFFF", 0.1, &bounds);
        assert!(svg.contains("<circle"));
    }

    #[test]
    fn test_svg_contains_dots() {
        let bounds = Bounds::new(0.0, 0.0, 10.0, 10.0);
        let elems = dither(DitherAlgorithm::FloydSteinberg, 30.0, &bounds, &|_, _| 0.3);
        let svg = elements_to_svg(&elems, "#FFFFFF", 0.08, &bounds);
        assert!(svg.contains("<rect"));
    }
}
