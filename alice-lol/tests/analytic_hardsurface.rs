//! 解析解突合 oracle — `stdlib::hardsurface::{fastener, joint}`
//!
//! `rules/analytic-oracle-tests.md` 準拠 既存 33 unit test (fastener 20 / joint 13) は
//! 構築時の struct field を照合する形が主で、**組み上がった形** (面の位置 / テーパー角 /
//! 嵌合すきま / 連続性) を測っていない そのため配置 offset・角度・すきまの変異が素通りする
//!
//! 本 file の方針は 2 段:
//!
//! 1. **規格式との突合** — ISO 4762 の `k = d`、ISO 10642 の `dk = 2·d`、
//!    pipeline 実プリント公式 (`0.85·d + 2·A`)、片持ち梁の `σ = 3·E·t·δ / (2·L²)`
//! 2. **出来上がった形の計測** — `eval` の符号変化を二分法で拾って幾何量を復元し、
//!    閉形式の予測と突合する (構築時の算術を読まずに測るので、配置や角度の誤りが必ず出る)
//!
//! ⚠️ 既存実装の出力をそのまま pin した test は置かない (それは変化検出であって oracle でない)
//! 規格値は実装の doc comment / 定数に書かれた根拠と突合し、
//! 裏が取れなかったものは「実装の自己申告と内部整合」に留めて test 名と comment に明記する

// ── 測定器としての書き方を優先する allow (理由付き、production 側には掛けない) ──
//
// ⚠️ `suboptimal_flops` (`mul_add` の提案) は **採用しない** — `a*b+c` は丸め 2 回 /
//    `mul_add` は 1 回で、FMA の有無で結果の bit が変わる 本 file は幾何量を
//    二分法で復元して閉形式と突合する測定器なので、式は素朴な形のまま読めることを優先する
//    (罠 `mul-add-breaks-bit-exactness`、ALICE-det-math が原型)
#![allow(clippy::suboptimal_flops)]
// ⚠️ 二分法の中点は `(lo + hi) * 0.5` の形で残す — `f32::midpoint` は意味は同じだが、
//    教科書の二分法としてそのまま読める形を測定器では優先する (f32 の範囲で overflow しない)
#![allow(clippy::manual_midpoint)]
// 幾何の式では `a` / `b` / `c` / `d` / `h` が数式の記号そのままなので改名しない
#![allow(clippy::many_single_char_names)]
// doc 中の規格名・記号 (ISO 10642 / dk / M2.5 等) に backtick を強制しない
#![allow(clippy::doc_markdown)]
// 符号の反転を見る判定は `!=` のまま書く方が「符号が変わった」と読める
#![allow(clippy::if_not_else)]

use alice_lol::stdlib::hardsurface::fastener::{
    bolt, counterbore, countersink, dowel_hole, heat_set_insert_hole, screw_hole, tap_hole,
    wood_screw_pilot, MetricSize, CLEARANCE_H2D_FDM, COUNTERSUNK_TAPER_ANGLE_DEG, DEFAULT_ACCURACY,
    HEAT_SET_SINK_MARGIN,
};
use alice_lol::stdlib::hardsurface::joint::{
    dovetail, jst_ph_slot, pin_hinge_knuckle, slot, snap_fit_annular, snap_fit_cantilever,
    t_slot_2020, SnapFitCantileverSpec, ANNULAR_BULGE_STANDARD_HEIGHT, DOVETAIL_TAPER_DEG,
    HINGE_CLEARANCE, JST_PH_PITCH, PETG_ELASTIC_MODULUS_GPA, PLA_ELASTIC_MODULUS_GPA,
    PLA_YIELD_STRESS_MPA, T_SLOT_2020_INNER_DEPTH, T_SLOT_2020_INNER_WIDTH,
    T_SLOT_2020_OPENING_DEPTH, T_SLOT_2020_OPENING_WIDTH,
};
use alice_sdf::{eval, SdfNode};
use glam::{Vec2, Vec3};

/// 全 `MetricSize` を呼び径昇順で並べたもの (単調性 / 規格式の全件検査用)
const ALL_SIZES: [MetricSize; 7] = [
    MetricSize::M2,
    MetricSize::M2_5,
    MetricSize::M3,
    MetricSize::M4,
    MetricSize::M5,
    MetricSize::M6,
    MetricSize::M8,
];

/// 幾何計測の許容差 (mm) 二分法 60 回 + f32 の丸めを見込んだ値
const GEO_TOL: f32 = 1e-3;

fn approx(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

// ────────────────────────────────────────────────────────
// 計測器 — 構築時の算術を読まずに「出来上がった形」を測る
// ────────────────────────────────────────────────────────

/// `from` から `dir` 方向に進み、SDF の符号が変わる位置までの距離を返す
///
/// 構築時の struct field ではなく **評価された場** を測るので、
/// 配置 offset / 回転 / 角度 / すきまの誤りがそのまま数値に出る
/// 符号変化が見つからなければ panic する (silent に 0 を返さない)
///
/// ⚠️ 内部判定は `<= 0.0` にする 本 crate の合成には **接する面** が現れる
/// (T スロットの開口と内部チャンバー / 環状 snap-fit の torus 内縁と shaft 表面) ので、
/// 接触面上では SDF がちょうど 0 になる `< 0.0` だとそこを「外」と読んで
/// 走査が手前で止まり、測る対象を取り違える
fn boundary(node: &SdfNode, from: Vec3, dir: Vec3, t_max: f32) -> f32 {
    let d = dir.normalize();
    let inside0 = eval(node, from) <= 0.0;
    // 粗い走査で符号変化区間を挟む (u16 → f32 は可逆なので精度損失なし)
    let steps: u16 = 8192;
    let mut lo = 0.0_f32;
    let mut hi = f32::NAN;
    for i in 1..=steps {
        let t = t_max * f32::from(i) / f32::from(steps);
        if (eval(node, from + d * t) <= 0.0) != inside0 {
            hi = t;
            break;
        }
        lo = t;
    }
    assert!(
        hi.is_finite(),
        "no sign change within t_max={t_max} from {from:?} dir {dir:?} (start inside={inside0})"
    );
    for _ in 0..60 {
        let mid = f32::midpoint(lo, hi);
        if (eval(node, from + d * mid) <= 0.0) != inside0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    f32::midpoint(lo, hi)
}

/// 差分商の上限を測る (厳密 SDF なら 1-Lipschitz、= |∇f| ≤ 1)
fn max_difference_quotient(node: &SdfNode, span: f32) -> f32 {
    let h = 1e-3_f32;
    let mut worst = 0.0_f32;
    let n: u16 = 11;
    for ix in 0..n {
        for iy in 0..n {
            for iz in 0..n {
                let u = |i: u16| span.mul_add(2.0 * f32::from(i) / f32::from(n - 1), -span);
                let p = Vec3::new(u(ix), u(iy), u(iz));
                let f0 = eval(node, p);
                for axis in 0..3_usize {
                    let mut q = p;
                    q[axis] += h;
                    let quotient = (eval(node, q) - f0).abs() / h;
                    if quotient.is_finite() && quotient > worst {
                        worst = quotient;
                    }
                }
            }
        }
    }
    worst
}

// ════════════════════════════════════════════════════════
// fastener — 規格式との突合
// ════════════════════════════════════════════════════════

/// ISO 4762 ソケットキャップボルトは 頭高 k が呼び径 d と一致する
///
/// oracle: ISO 4762 の構造則 `k = d` (実装の 7 サイズ全部で成立するかを検査)
/// これは表の値を写したのではなく、表の 2 列が満たすべき関係
#[test]
fn iso4762_head_height_equals_nominal_diameter_for_every_size() {
    for size in ALL_SIZES {
        let d = size.nominal_diameter();
        let k = size.head_height_socket();
        assert!(
            approx(k, d, 1e-6),
            "ISO 4762 k = d 違反: {size:?} は d={d} に対し k={k}"
        );
    }
}

/// ISO 10642 皿頭の頭径 dk は呼び径の 2 倍 — ⚠️ M2.5 だけ 1 件外れる
///
/// oracle: `dk = 2·d` M2 / M3 / M4 / M5 / M6 / M8 の 6 件で成立し、
/// **M2_5 のみ 4.5 (規格式なら 5.0) で 0.5 mm 外れている**
/// ⚠️ この 1 件は silent な例外にせず「外れていること自体」を pin する
/// (4.5 は同サイズの `head_diameter_socket` と同値 = 皿頭表が socket 表から複製された形)
/// ⚠️ M2.5 皿頭の正しい規格値は本 repo 内に根拠が無く未確認 (ISO 10642 は M3 以上が範囲)
/// 4.5 を 5.0 に直すかは仕様判断なので本 oracle は現状を「規格式からの逸脱 1 件」として固定する
#[test]
fn iso10642_countersunk_head_is_twice_nominal_except_the_single_m2_5_deviation() {
    let mut deviations = Vec::new();
    for size in ALL_SIZES {
        let d = size.nominal_diameter();
        let dk = size.head_diameter_countersunk();
        if !approx(dk, 2.0 * d, 1e-6) {
            deviations.push((size, d, dk));
        }
    }
    assert_eq!(
        deviations.len(),
        1,
        "ISO 10642 dk = 2·d から外れるサイズ数が 1 でない: {deviations:?}"
    );
    let (size, d, dk) = deviations[0];
    assert_eq!(
        size,
        MetricSize::M2_5,
        "逸脱しているのが M2_5 以外になった: {size:?}"
    );
    assert!(
        approx(dk, 4.5, 1e-6) && approx(d, 2.5, 1e-6),
        "M2_5 の逸脱内容が変わった: d={d} dk={dk} (規格式なら 5.0)"
    );
    // 逸脱の正体 = socket 頭径と同値
    assert!(
        approx(dk, MetricSize::M2_5.head_diameter_socket(), 1e-6),
        "M2_5 の皿頭径が socket 頭径と一致しなくなった"
    );
}

/// 呼び径 / 頭径 / 頭高 / insert 径は サイズ昇順で単調非減少
///
/// oracle: サイズ表は呼び径について単調 ⚠️ **heat-set 埋込深さだけ M4 == M5 で等しい**
/// (実装が `Self::M4 | Self::M5 => 5.7` と 1 本にまとめている) ので狭義単調ではない
/// その 1 箇所を明示 pin して、他が崩れたら red にする
#[test]
fn metric_size_tables_are_monotone_with_one_documented_tie() {
    for w in ALL_SIZES.windows(2) {
        let (a, b) = (w[0], w[1]);
        assert!(
            b.nominal_diameter() > a.nominal_diameter(),
            "呼び径が単調増加でない: {a:?} -> {b:?}"
        );
        for (name, lo, hi) in [
            (
                "head_diameter_socket",
                a.head_diameter_socket(),
                b.head_diameter_socket(),
            ),
            (
                "head_height_socket",
                a.head_height_socket(),
                b.head_height_socket(),
            ),
            (
                "head_diameter_countersunk",
                a.head_diameter_countersunk(),
                b.head_diameter_countersunk(),
            ),
            (
                "heat_set_insert_diameter",
                a.heat_set_insert_diameter(),
                b.heat_set_insert_diameter(),
            ),
            (
                "heat_set_insert_depth",
                a.heat_set_insert_depth(),
                b.heat_set_insert_depth(),
            ),
        ] {
            assert!(
                hi >= lo,
                "{name} が単調非減少でない: {a:?}={lo} -> {b:?}={hi}"
            );
        }
    }
    // 唯一の等号 (McMaster 表で M4 / M5 が同じ埋込深さ)
    assert!(
        approx(
            MetricSize::M4.heat_set_insert_depth(),
            MetricSize::M5.heat_set_insert_depth(),
            1e-6
        ),
        "M4 / M5 の heat-set 埋込深さの同値が崩れた (単調性の唯一の等号)"
    );
    // それ以外の隣接ペアは狭義単調であること
    for w in ALL_SIZES.windows(2) {
        if w[0] == MetricSize::M4 {
            continue;
        }
        assert!(
            w[1].heat_set_insert_depth() > w[0].heat_set_insert_depth(),
            "M4/M5 以外で heat-set 深さの等号が出た: {:?} -> {:?}",
            w[0],
            w[1]
        );
    }
}

// ════════════════════════════════════════════════════════
// fastener — 径の閉形式 (場から測って突合)
// ════════════════════════════════════════════════════════

/// クリアランス穴の半径を **場から測って** `(d + 0.2)/2` と突合
///
/// oracle: 直径 = 呼び径 + `CLEARANCE_H2D_FDM`、全長 = `depth`
/// struct field を見ずに半径方向 / 軸方向の境界を二分法で拾う
#[test]
fn screw_hole_radius_and_length_measured_from_the_field_match_the_clearance_formula() {
    let depth = 5.0;
    for size in ALL_SIZES {
        let node = screw_hole(size, depth);
        let expect_r = (size.nominal_diameter() + CLEARANCE_H2D_FDM) * 0.5;
        let r = boundary(&node, Vec3::ZERO, Vec3::X, 50.0);
        assert!(
            approx(r, expect_r, GEO_TOL),
            "{size:?} クリアランス穴半径: 測定 {r} / 閉形式 {expect_r}"
        );
        let half = boundary(&node, Vec3::ZERO, Vec3::Y, 50.0);
        assert!(
            approx(half, depth * 0.5, GEO_TOL),
            "{size:?} クリアランス穴 半長: 測定 {half} / 閉形式 {}",
            depth * 0.5
        );
    }
}

/// タップ下穴は accuracy について 傾き 1 の 1 次式 (pipeline `tap_hole()` と同式)
///
/// oracle: 直径 = `0.85·d + 2·A` ⇒ 半径 = `0.425·d + A`
/// ⇒ 切片 = `0.425·d`、**A についての傾きはちょうど 1**
/// 既存 test は A を 2 点しか見ておらず係数 2 の誤りを 1 点でも通すので、掃引で押さえる
#[test]
fn tap_hole_radius_is_affine_in_accuracy_with_unit_slope() {
    for size in ALL_SIZES {
        let d = size.nominal_diameter();
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..7_u16 {
            let acc = f32::from(i) * 0.05;
            let r = boundary(&tap_hole(size, 4.0, acc), Vec3::ZERO, Vec3::X, 50.0);
            let expect = 0.425_f32.mul_add(d, acc);
            assert!(
                approx(r, expect, GEO_TOL),
                "{size:?} A={acc} タップ下穴半径: 測定 {r} / 閉形式 {expect}"
            );
            if let Some((pa, pr)) = prev {
                let slope = (r - pr) / (acc - pa);
                assert!(
                    approx(slope, 1.0, 1e-2),
                    "{size:?} A についての傾きが 1 でない: {slope}"
                );
            }
            prev = Some((acc, r));
        }
        // 切片 (A = 0) = 0.425·d
        let r0 = boundary(&tap_hole(size, 4.0, 0.0), Vec3::ZERO, Vec3::X, 50.0);
        assert!(
            approx(r0, 0.425 * d, GEO_TOL),
            "{size:?} 切片が 0.425·d でない: {r0}"
        );
    }
}

/// タップ下穴は常にクリアランス穴より小さい (下穴とすきま穴の順序)
///
/// oracle: `0.85·d + 2·A < d + 0.2` ⇔ A < 0.075·d + 0.1 default A=0.1 では全サイズ成立
#[test]
fn tap_hole_is_always_tighter_than_the_clearance_hole() {
    for size in ALL_SIZES {
        let tap = boundary(
            &tap_hole(size, 4.0, DEFAULT_ACCURACY),
            Vec3::ZERO,
            Vec3::X,
            50.0,
        );
        let clear = boundary(&screw_hole(size, 4.0), Vec3::ZERO, Vec3::X, 50.0);
        assert!(
            tap < clear,
            "{size:?} タップ下穴 {tap} がクリアランス穴 {clear} 以上になっている"
        );
    }
}

/// すきま嵌め: クリアランス穴は同サイズのボルト軸に対し 径方向 +0.1 mm
///
/// oracle: 穴半径 − 軸半径 = `CLEARANCE_H2D_FDM / 2` = 0.1 mm (全サイズ)
/// ⚠️ 2 primitive を跨ぐ関係 = 片方だけ変えると崩れる
#[test]
fn clearance_hole_accepts_its_own_bolt_with_exactly_half_the_clearance_radially() {
    for size in ALL_SIZES {
        let hole_r = boundary(&screw_hole(size, 10.0), Vec3::ZERO, Vec3::X, 50.0);
        let shank_r = boundary(&bolt(size, 20.0), Vec3::ZERO, Vec3::X, 50.0);
        let gap = hole_r - shank_r;
        assert!(
            approx(gap, CLEARANCE_H2D_FDM * 0.5, GEO_TOL),
            "{size:?} すきま嵌めの径方向すきま: 測定 {gap} / 閉形式 {}",
            CLEARANCE_H2D_FDM * 0.5
        );
        assert!(gap > 0.0, "{size:?} ボルトが穴を通らない (すきま {gap})");
    }
}

/// ヒートセット下穴は insert 外径 +0.2 / 埋込深さ +0.3 をちょうど足す
///
/// oracle: 直径 = `insert_od + CLEARANCE_H2D_FDM`、深さ = `insert_depth + HEAT_SET_SINK_MARGIN`
/// ⚠️ McMaster / Voxel8 の insert 実寸表そのものは本 repo 外の値で未検証
/// ここで押さえるのは「表の値に足す量」が定数どおりであること
#[test]
fn heat_set_hole_adds_exactly_the_documented_clearance_and_sink_margin() {
    for size in ALL_SIZES {
        let node = heat_set_insert_hole(size);
        let r = boundary(&node, Vec3::ZERO, Vec3::X, 50.0);
        let half = boundary(&node, Vec3::ZERO, Vec3::Y, 50.0);
        let expect_r = (size.heat_set_insert_diameter() + CLEARANCE_H2D_FDM) * 0.5;
        let expect_half = (size.heat_set_insert_depth() + HEAT_SET_SINK_MARGIN) * 0.5;
        assert!(
            approx(r, expect_r, GEO_TOL),
            "{size:?} heat-set 下穴半径: 測定 {r} / 閉形式 {expect_r}"
        );
        assert!(
            approx(half, expect_half, GEO_TOL),
            "{size:?} heat-set 下穴 半深: 測定 {half} / 閉形式 {expect_half}"
        );
        // insert は呼び径より必ず太い (ねじ山を包む)
        assert!(
            size.heat_set_insert_diameter() > size.nominal_diameter(),
            "{size:?} insert 外径が呼び径以下"
        );
    }
}

/// 木ネジ下穴: 硬材 / 軟材の係数比と 深さ = 5·径
///
/// oracle: 軟材 0.7·d / 硬材 0.9·d ⇒ 半径比はちょうど 9/7、深さ/径 = 5
#[test]
fn wood_screw_pilot_ratios_and_depth_follow_the_closed_form() {
    for i in 2..9_u16 {
        let d = f32::from(i);
        let soft = wood_screw_pilot(d, false);
        let hard = wood_screw_pilot(d, true);
        let rs = boundary(&soft, Vec3::ZERO, Vec3::X, 100.0);
        let rh = boundary(&hard, Vec3::ZERO, Vec3::X, 100.0);
        assert!(approx(rs, 0.35 * d, GEO_TOL), "軟材下穴半径 d={d}: {rs}");
        assert!(approx(rh, 0.45 * d, GEO_TOL), "硬材下穴半径 d={d}: {rh}");
        assert!(
            approx(rh / rs, 9.0 / 7.0, 1e-3),
            "硬材/軟材 の半径比が 9/7 でない: {}",
            rh / rs
        );
        assert!(rh > rs, "硬材下穴が軟材以下 (ネジ切れ防止にならない)");
        // 深さ = 5·d (両材で同じ)
        for (name, node) in [("軟材", &soft), ("硬材", &hard)] {
            let half = boundary(node, Vec3::ZERO, Vec3::Y, 100.0);
            assert!(
                approx(half, d * 2.5, GEO_TOL),
                "{name} 下穴 半深 d={d}: 測定 {half} / 閉形式 {}",
                d * 2.5
            );
        }
    }
}

/// ダウエル穴は 呼び径 +0.1 mm ちょうど (接着剤余裕)
#[test]
fn dowel_hole_adds_exactly_one_tenth_of_a_millimetre_to_the_diameter() {
    for i in 4..11_u16 {
        let d = f32::from(i);
        let r = boundary(&dowel_hole(d, 15.0), Vec3::ZERO, Vec3::X, 100.0);
        assert!(
            approx(r, (d + 0.1) * 0.5, GEO_TOL),
            "Ø{d} ダウエル穴半径: 測定 {r} / 閉形式 {}",
            (d + 0.1) * 0.5
        );
    }
}

// ════════════════════════════════════════════════════════
// fastener — 配置の代数恒等式 (面一 / テーパー角)
// ════════════════════════════════════════════════════════

/// 座ぐりの頭部沈み穴の上端は 板上面とちょうど面一
///
/// oracle: 配置 `offset = (T − b)/2`、半高 `b/2` ⇒ 上端 `= offset + b/2 = T/2`
/// = **板厚に依らず板上面と一致** する代数恒等式 (`b` = 頭高 + 0.5 沈み余裕)
/// 既存 test は Union の構造 (`Cylinder` / `Translate`) しか見ておらず配置を検査していない
#[test]
fn counterbore_bore_top_is_flush_with_the_plate_top_face() {
    for size in ALL_SIZES {
        for i in 0..5_u16 {
            let plate = f32::from(i).mul_add(2.0, 6.0);
            let node = counterbore(size, plate);
            let bore_depth = size.head_height_socket() + 0.5;
            let head_r = size.head_diameter_socket() * 0.5;
            let shank_r = (size.nominal_diameter() + CLEARANCE_H2D_FDM) * 0.5;
            // 貫通穴より外、頭部沈み穴より内 の半径で測ると沈み穴だけが効く
            let x = f32::midpoint(shank_r, head_r);
            let y = plate * 0.5 - bore_depth * 0.5;
            let start = Vec3::new(x, y, 0.0);
            assert!(
                eval(&node, start) < 0.0,
                "{size:?} T={plate} 沈み穴内部の測定開始点が内部でない"
            );
            let up = boundary(&node, start, Vec3::Y, 4.0 * plate);
            assert!(
                approx(y + up, plate * 0.5, GEO_TOL),
                "{size:?} T={plate} 沈み穴上端: 測定 {} / 板上面 {}",
                y + up,
                plate * 0.5
            );
            // 下端 = 板上面 − 沈み深さ
            let down = boundary(&node, start, -Vec3::Y, 4.0 * plate);
            assert!(
                approx(y - down, plate * 0.5 - bore_depth, GEO_TOL),
                "{size:?} T={plate} 沈み穴下端: 測定 {} / 閉形式 {}",
                y - down,
                plate * 0.5 - bore_depth
            );
        }
    }
}

/// 座ぐり穴の径は ISO 4762 頭径そのまま = 径方向すきま 0
///
/// ⚠️ **クリアランス穴は +0.2 を足すのに、頭部沈み穴は径方向に 0 しか足していない**
/// (深さ方向には +0.5 mm の沈み余裕がある) 非対称なので FDM では頭が入らない可能性がある
/// 仕様判断を含むため本 oracle は現状の関係を「すきま 0」として明示 pin するに留める
/// (修正は別判断)
#[test]
fn counterbore_bore_diameter_equals_the_iso_head_diameter_with_zero_radial_clearance() {
    let plate = 8.0;
    for size in ALL_SIZES {
        let node = counterbore(size, plate);
        let bore_depth = size.head_height_socket() + 0.5;
        let y = plate * 0.5 - bore_depth * 0.5;
        let bore_r = boundary(&node, Vec3::new(0.0, y, 0.0), Vec3::X, 50.0);
        let head_r = size.head_diameter_socket() * 0.5;
        assert!(
            approx(bore_r, head_r, GEO_TOL),
            "{size:?} 沈み穴半径: 測定 {bore_r} / ISO 4762 頭半径 {head_r}"
        );
        // 同サイズのボルト頭に対する径方向すきま = 0 (クリアランス穴の +0.1 と非対称)
        let bolt_head_r = boundary(
            &bolt(size, 20.0),
            Vec3::new(0.0, 10.0 + size.head_height_socket() * 0.5, 0.0),
            Vec3::X,
            50.0,
        );
        assert!(
            approx(bore_r - bolt_head_r, 0.0, GEO_TOL),
            "{size:?} 沈み穴とボルト頭の径方向すきま: {} (現状仕様は 0)",
            bore_r - bolt_head_r
        );
    }
}

/// 皿穴のテーパーを **円錐の場から復元** して 90° と突合
///
/// oracle: ISO 10642 皿頭は全角 90° ⇒ 半角 45° ⇒ 高さあたりの半径増加 `tan45° = 1`
/// 2 高さで半径を測って `atan(Δr/Δy)` を求め、`COUNTERSUNK_TAPER_ANGLE_DEG` と突合する
/// ⚠️ 測る高さは 貫通穴半径より円錐半径が大きい領域に取る (でないと貫通穴の径を測ってしまう)
/// ⚠️ `COUNTERSUNK_TAPER_ANGLE_DEG` は実装から参照されていない (`countersink` は
/// `head_dia * 0.5` を直書き) ので、本 test が定数と幾何を結ぶ唯一の経路
#[test]
fn countersink_taper_recovered_from_the_field_is_the_documented_90_degrees() {
    let plate = 12.0;
    for size in ALL_SIZES {
        let node = countersink(size, plate);
        let head_r = size.head_diameter_countersunk() * 0.5;
        let cone_h = size.head_diameter_countersunk() * 0.5;
        let tip_y = plate * 0.5 - cone_h;
        let shank_r = (size.nominal_diameter() + CLEARANCE_H2D_FDM) * 0.5;
        // 円錐半径が貫通穴半径を超える高さ範囲
        let y_lo = tip_y + shank_r + 0.15 * (head_r - shank_r);
        let y_hi = tip_y + shank_r + 0.85 * (head_r - shank_r);
        assert!(y_hi > y_lo, "{size:?} 測定可能な高さ範囲が無い");
        let r_lo = boundary(&node, Vec3::new(0.0, y_lo, 0.0), Vec3::X, 50.0);
        let r_hi = boundary(&node, Vec3::new(0.0, y_hi, 0.0), Vec3::X, 50.0);
        let half_angle = ((r_hi - r_lo) / (y_hi - y_lo)).atan().to_degrees();
        assert!(
            approx(2.0 * half_angle, COUNTERSUNK_TAPER_ANGLE_DEG, 0.5),
            "{size:?} 皿テーパー全角: 測定 {} / 規格 {COUNTERSUNK_TAPER_ANGLE_DEG}",
            2.0 * half_angle
        );
    }
}

/// 皿穴の円錐底面は 板上面と面一 かつ ISO 10642 頭径
///
/// oracle: 配置 `offset = (T − h)/2`、半高 `h/2` ⇒ 底面 `= T/2` (板厚に依らない恒等式)
/// 底面半径 = `dk/2` 円錐の尖端が板内部を向いている (= 回転が効いている) ことも確認
#[test]
fn countersink_cone_base_is_flush_with_the_plate_top_and_tapers_inward() {
    let plate = 12.0;
    for size in ALL_SIZES {
        let node = countersink(size, plate);
        let head_r = size.head_diameter_countersunk() * 0.5;
        let cone_h = head_r;
        // 板上面のわずか下で 半径 = dk/2 に漸近
        let r_top = boundary(
            &node,
            Vec3::new(0.0, plate * 0.5 - 1e-3, 0.0),
            Vec3::X,
            50.0,
        );
        assert!(
            approx(r_top, head_r, 5e-3),
            "{size:?} 皿穴 上面半径: 測定 {r_top} / ISO 10642 {head_r}"
        );
        // 板上面より上は外部 (円錐がはみ出していない)
        // ⚠️ 測る半径は 貫通穴より外 / 皿頭径より内 に取る
        // (貫通穴は板上下に 5mm ずつ伸びているので、軸寄りで測ると常に内部になる)
        let shank_r = (size.nominal_diameter() + CLEARANCE_H2D_FDM) * 0.5;
        assert!(
            eval(
                &node,
                Vec3::new(f32::midpoint(shank_r, head_r), plate * 0.5 + 0.05, 0.0)
            ) > 0.0,
            "{size:?} 皿穴が板上面より上にはみ出している"
        );
        // 尖端側 (下) は必ず細い = テーパーが内向き (回転 180° が効いている)
        let y_mid = plate * 0.5 - cone_h * 0.5;
        let r_mid = boundary(&node, Vec3::new(0.0, y_mid, 0.0), Vec3::X, 50.0);
        assert!(
            r_mid < r_top,
            "{size:?} 皿穴が下方で細くなっていない (上 {r_top} / 中 {r_mid})"
        );
    }
}

/// ボルト頭の下面は 軸の上端とちょうど面一
///
/// oracle: 配置 `offset = (L + k)/2`、半高 `k/2`
/// ⇒ 頭下面 `= offset − k/2 = L/2` (軸上端)、頭上面 `= L/2 + k`
/// ⇒ 全長 `= L + k` すきま無く重なり無く積む代数恒等式
#[test]
fn bolt_head_underside_is_flush_with_the_shank_top_and_total_length_is_l_plus_k() {
    for size in ALL_SIZES {
        for i in 0..4_u16 {
            let shank = f32::from(i).mul_add(5.0, 10.0);
            let node = bolt(size, shank);
            let k = size.head_height_socket();
            let head_r = size.head_diameter_socket() * 0.5;
            let shank_r = size.nominal_diameter() * 0.5;
            // 軸より外 / 頭より内 の半径では頭だけが効く
            let x = f32::midpoint(shank_r, head_r);
            let y = shank * 0.5 + k * 0.5;
            let start = Vec3::new(x, y, 0.0);
            assert!(
                eval(&node, start) < 0.0,
                "{size:?} L={shank} 頭部内の測定開始点が内部でない"
            );
            let down = boundary(&node, start, -Vec3::Y, 4.0 * (shank + k));
            assert!(
                approx(y - down, shank * 0.5, GEO_TOL),
                "{size:?} L={shank} 頭下面: 測定 {} / 軸上端 {}",
                y - down,
                shank * 0.5
            );
            // 全長 = 軸下端 (−L/2) 〜 頭上面 (L/2 + k)
            let top = boundary(&node, Vec3::ZERO, Vec3::Y, 4.0 * (shank + k));
            let bottom = boundary(&node, Vec3::ZERO, -Vec3::Y, 4.0 * (shank + k));
            assert!(
                approx(top, shank * 0.5 + k, GEO_TOL),
                "{size:?} L={shank} 頭上面: 測定 {top} / 閉形式 {}",
                shank * 0.5 + k
            );
            assert!(
                approx(top + bottom, shank + k, GEO_TOL),
                "{size:?} L={shank} 全長: 測定 {} / 閉形式 {}",
                top + bottom,
                shank + k
            );
        }
    }
}

// ════════════════════════════════════════════════════════
// fastener — snap / 退化
// ════════════════════════════════════════════════════════

/// `from_f32_snap` は候補 7 個の中で真に最近接を返す (性質 oracle)
///
/// oracle: 返り値 s は任意の候補 t に対し `|nominal(s) − x| ≤ |nominal(t) − x|`
/// (実装の argmin ループを写すのでなく、最近接の定義そのものを検査)
#[test]
fn from_f32_snap_returns_a_true_nearest_candidate() {
    for i in 0..=220_u16 {
        let x = f32::from(i) * 0.05;
        let got = MetricSize::from_f32_snap(x);
        let got_d = (got.nominal_diameter() - x).abs();
        for other in ALL_SIZES {
            let other_d = (other.nominal_diameter() - x).abs();
            assert!(
                got_d <= other_d + 1e-6,
                "x={x}: {got:?} (距離 {got_d}) より {other:?} (距離 {other_d}) が近い"
            );
        }
    }
}

/// 等距離の境界は 小さいサイズ側に倒れる (first-wins)
#[test]
fn from_f32_snap_breaks_exact_ties_toward_the_smaller_size() {
    for (x, expect) in [
        (2.25_f32, MetricSize::M2),
        (2.75, MetricSize::M2_5),
        (3.5, MetricSize::M3),
        (4.5, MetricSize::M4),
        (5.5, MetricSize::M5),
        (7.0, MetricSize::M6),
    ] {
        assert_eq!(
            MetricSize::from_f32_snap(x),
            expect,
            "x={x} の等距離 tie が小さい側に倒れていない"
        );
    }
}

/// 規格値を入れたら同じサイズが返る (冪等) + 単調非減少
#[test]
fn from_f32_snap_is_idempotent_on_nominals_and_monotone_in_the_input() {
    for size in ALL_SIZES {
        assert_eq!(
            MetricSize::from_f32_snap(size.nominal_diameter()),
            size,
            "{size:?} の呼び径を snap して別サイズになった"
        );
    }
    let mut prev = 0.0_f32;
    for i in 0..=400_u16 {
        let x = f32::from(i) * 0.05;
        let d = MetricSize::from_f32_snap(x).nominal_diameter();
        assert!(d >= prev, "snap が単調非減少でない: x={x} で {prev} -> {d}");
        prev = d;
    }
}

/// 退化入力: 非有限値は **3 通りすべて初期値 `M4` に落ちる** (clamp ではない)
///
/// 非有限は `try_from_f32` が `None`、`from_f32_snap` は最小サイズに倒す
///
/// ⚠️ 2026-09-30 時点の実装は `d < best_dist` の狭義比較で argmin を取っており、
/// `NaN` は全比較が false / `±∞` は全候補との距離が `∞` で `∞ < ∞` が false のため
/// **1 度も更新されずループ初期値の `M4` が返っていた**
/// ⇒ `M4` が「最近接」でなく「更新されなかった」ことを意味する状態だった
///
/// 2026-10-01 の修正で、判定できない入力は `try_from_f32` が `None` を返す
/// (`runtime_parser` はこれを parse error にする) `from_f32_snap` は `Self` を
/// 返す契約なので最小サイズ `M2` に倒す — ⚠️ **「最も害が小さい側」であり
/// 「最近接」ではない**ので、呼び出し側が弾けるよう `Option` 版を用意している
#[test]
fn non_finite_input_is_rejected_by_try_from_f32() {
    for (label, x) in [
        ("NaN", f32::NAN),
        ("+inf", f32::INFINITY),
        ("-inf", f32::NEG_INFINITY),
    ] {
        assert_eq!(
            MetricSize::try_from_f32(x),
            None,
            "{label} が None で弾かれない (判定材料が無い入力なのでサイズを返してはいけない)"
        );
        assert_eq!(
            MetricSize::from_f32_snap(x),
            MetricSize::M2,
            "{label} の fallback が最小サイズでない"
        );
    }
    // 負側 / 0 は素直に下限へ clamp される (有限なので `Some`)
    for x in [-1.0_f32, 0.0, f32::MIN] {
        assert_eq!(MetricSize::try_from_f32(x), Some(MetricSize::M2), "x={x}");
    }
}

/// 上限 clamp は全ての有限値で効く (桁落ちで最遠候補に落ちない)
///
/// ⚠️ 2026-09-30 時点の実装は `(nominal − x).abs()` を **f32** で計算していたため、
/// x が大きいと候補間の差 (最大 6mm) が仮数に吸収されて `d` が全候補で同値になり、
/// 狭義比較で 2 番目以降が更新されず**先頭の最小サイズが勝っていた**:
///
/// | x | 旧実装の返り値 |
/// |---|---|
/// | `1e2` 〜 `2e7` | M8 (正しい) |
/// | `5e7` | M6 |
/// | `1e8` | M5 |
/// | `1e9` 以上 / `f32::MAX` | **M2 (= 最遠)** |
///
/// 既存 test は `100.0` の 1 点しか見ないのでこの劣化を捕まえなかった
/// doc は「上限 clamp」と書いていたので実挙動と食い違っていた
///
/// 2026-10-01 の修正で距離計算を f64 にしたので、候補間の差は `f32::MAX` でも
/// 区別できる ⚠️ **同値時の first-wins は維持**している (`4.5` → M4 の規約)
#[test]
fn the_upper_clamp_holds_for_every_finite_input() {
    for x in [
        1.0e2_f32,
        1.0e4,
        1.0e6,
        1.0e7,
        2.0e7,
        5.0e7,
        1.0e8,
        1.0e9,
        f32::MAX,
    ] {
        assert_eq!(
            MetricSize::from_f32_snap(x),
            MetricSize::M8,
            "x={x} が上限 clamp されない (f32 の桁落ちで最遠候補に落ちていないか)"
        );
    }
    // ⚠️ 距離計算では原理的に区別できないことを示す — だから clamp を明示している
    //    (精度を上げる方向では直らない: f64 でも `f32::MAX` では相対 1.8e-38 で潰れる)
    for x in [1.0e9_f32, f32::MAX] {
        assert_eq!(
            (2.0_f32 - x).abs().to_bits(),
            (8.0_f32 - x).abs().to_bits(),
            "x={x} で f32 の距離が区別できてしまう (前提が変わった)"
        );
    }
    assert_eq!(
        (2.0_f64 - f64::from(f32::MAX)).abs().to_bits(),
        (8.0_f64 - f64::from(f32::MAX)).abs().to_bits(),
        "f32::MAX では f64 でも距離が区別できないはず (clamp が必要な理由)"
    );
    // 下限側も同じく clamp で決まる
    for x in [-1.0e9_f32, f32::MIN] {
        assert_eq!(MetricSize::from_f32_snap(x), MetricSize::M2, "x={x}");
    }
    // 同値時の first-wins 規約は維持 (M4 と M5 が等距離)
    assert_eq!(MetricSize::from_f32_snap(4.5), MetricSize::M4);
}

/// 退化寸法 (0 / 負) でも SDF が有限値を返し panic しない
#[test]
fn fastener_primitives_stay_finite_on_degenerate_dimensions() {
    let probes = [
        Vec3::ZERO,
        Vec3::new(0.3, 0.3, 0.3),
        Vec3::new(-7.0, 4.0, 2.0),
    ];
    let mut nodes: Vec<(String, SdfNode)> = Vec::new();
    for depth in [0.0_f32, -1.0, 1e-6] {
        nodes.push((
            format!("screw_hole depth={depth}"),
            screw_hole(MetricSize::M3, depth),
        ));
        nodes.push((
            format!("tap_hole depth={depth}"),
            tap_hole(MetricSize::M3, depth, 0.0),
        ));
        nodes.push((
            format!("counterbore T={depth}"),
            counterbore(MetricSize::M3, depth),
        ));
        nodes.push((
            format!("countersink T={depth}"),
            countersink(MetricSize::M3, depth),
        ));
        nodes.push((format!("dowel_hole dia={depth}"), dowel_hole(depth, depth)));
        nodes.push((
            format!("wood_pilot dia={depth}"),
            wood_screw_pilot(depth, false),
        ));
        nodes.push((format!("bolt L={depth}"), bolt(MetricSize::M3, depth)));
    }
    for (name, node) in &nodes {
        for p in probes {
            let d = eval(node, p);
            assert!(d.is_finite(), "{name} が {p:?} で非有限値 {d}");
        }
    }
}

/// 全 fastener primitive は 1-Lipschitz (厳密 SDF の必要条件)
///
/// oracle: 厳密な符号付き距離場は `|∇f| ≤ 1` 差分商の最大値で測る
#[test]
fn fastener_primitives_are_one_lipschitz() {
    let nodes: [(&str, SdfNode); 6] = [
        ("screw_hole", screw_hole(MetricSize::M4, 6.0)),
        ("tap_hole", tap_hole(MetricSize::M4, 6.0, DEFAULT_ACCURACY)),
        ("counterbore", counterbore(MetricSize::M4, 8.0)),
        ("countersink", countersink(MetricSize::M4, 12.0)),
        ("bolt", bolt(MetricSize::M4, 20.0)),
        ("heat_set_insert_hole", heat_set_insert_hole(MetricSize::M4)),
    ];
    for (name, node) in &nodes {
        let q = max_difference_quotient(node, 15.0);
        assert!(
            q <= 1.0 + 1e-2,
            "{name} の差分商が 1 を超えた ({q}) = 厳密 SDF でない"
        );
    }
}

// ════════════════════════════════════════════════════════
// joint — 片持ち梁応力の閉形式
// ════════════════════════════════════════════════════════

/// 片持ち梁の根本応力を 掃引して閉形式と突合
///
/// oracle: `σ = 3·E·t·δ / (2·L²)` (E は GPa 入力 → MPa は ×1000)
/// ⚠️ 既存 test 2 本はどちらも `PLA_STANDARD` (t=2, δ=0.5 ⇒ t·δ = 1.0) の 1 点しか見ないので、
/// `t·δ` を `t²·δ²` に変えても値が変わらず素通りする 4 変数を独立に振って押さえる
#[test]
fn cantilever_peak_stress_matches_the_closed_form_over_a_sweep() {
    for il in 0..5_u16 {
        for it in 0..4_u16 {
            for ih in 0..4_u16 {
                for ie in 0..3_u16 {
                    let length = f32::from(il).mul_add(4.0, 8.0);
                    let thickness = f32::from(it).mul_add(0.75, 1.0);
                    let hook_height = f32::from(ih).mul_add(0.3, 0.4);
                    let e_gpa = f32::from(ie).mul_add(0.8, 2.2);
                    let spec = SnapFitCantileverSpec {
                        length,
                        width: 5.0,
                        thickness,
                        hook_height,
                        hook_offset: 1.5,
                    };
                    let got = spec.peak_stress_mpa(e_gpa);
                    let expect =
                        3.0 * (e_gpa * 1000.0) * thickness * hook_height / (2.0 * length * length);
                    assert!(
                        approx(got, expect, expect.abs() * 1e-4),
                        "σ(L={length}, t={thickness}, δ={hook_height}, E={e_gpa}): \
                         実装 {got} / 閉形式 {expect}"
                    );
                }
            }
        }
    }
}

/// 応力のスケーリング指数: E / t / δ に 1 次、L に −2 次
///
/// oracle: `σ ∝ E·t·δ·L⁻²` 各変数を 2 倍した時の比で指数を直接測る
/// (絶対値が合っていても指数が違えば別の式 = 逆も真なので両方必要)
#[test]
fn cantilever_stress_scaling_exponents_are_one_one_one_and_minus_two() {
    let base = SnapFitCantileverSpec {
        length: 12.0,
        width: 5.0,
        thickness: 2.0,
        hook_height: 0.6,
        hook_offset: 1.5,
    };
    let s0 = base.peak_stress_mpa(PLA_ELASTIC_MODULUS_GPA);

    // E について 1 次
    let r_e = base.peak_stress_mpa(2.0 * PLA_ELASTIC_MODULUS_GPA) / s0;
    assert!(approx(r_e, 2.0, 1e-3), "E の指数が 1 でない (比 {r_e})");

    // t について 1 次
    let mut t2 = base;
    t2.thickness *= 2.0;
    let r_t = t2.peak_stress_mpa(PLA_ELASTIC_MODULUS_GPA) / s0;
    assert!(approx(r_t, 2.0, 1e-3), "t の指数が 1 でない (比 {r_t})");

    // δ について 1 次
    let mut h2 = base;
    h2.hook_height *= 2.0;
    let r_h = h2.peak_stress_mpa(PLA_ELASTIC_MODULUS_GPA) / s0;
    assert!(approx(r_h, 2.0, 1e-3), "δ の指数が 1 でない (比 {r_h})");

    // L について −2 次
    let mut l2 = base;
    l2.length *= 2.0;
    let r_l = l2.peak_stress_mpa(PLA_ELASTIC_MODULUS_GPA) / s0;
    assert!(approx(r_l, 0.25, 1e-3), "L の指数が −2 でない (比 {r_l})");
}

/// PLA 安全判定の境界は 降伏応力/2 に対する **狭義** 不等号
///
/// oracle: 閾値 = `PLA_YIELD_STRESS_MPA / 2` = 30 MPa
/// `σ = 30.0` ちょうどになる spec (L=35, t=14, δ=0.5 ⇒ 3·3500·14·0.5/(2·1225) = 30) を作り、
/// `<` なので **false** になることを pin する (`<=` に変えると red)
#[test]
fn is_safe_for_pla_uses_a_strict_inequality_at_exactly_half_the_yield_stress() {
    let threshold = PLA_YIELD_STRESS_MPA / 2.0;
    let exact = SnapFitCantileverSpec {
        length: 35.0,
        width: 5.0,
        thickness: 14.0,
        hook_height: 0.5,
        hook_offset: 1.5,
    };
    let s = exact.peak_stress_mpa(PLA_ELASTIC_MODULUS_GPA);
    assert!(
        approx(s, threshold, 1e-4),
        "境界 spec の応力が閾値 {threshold} にならない: {s}"
    );
    assert!(
        !exact.is_safe_for_pla(),
        "σ = 閾値ちょうど で safe と判定された (狭義 < のはず)"
    );

    // 閾値の両側
    let mut safer = exact;
    safer.length = 36.0;
    assert!(
        safer.is_safe_for_pla(),
        "閾値より低い応力で unsafe と判定された (σ={})",
        safer.peak_stress_mpa(PLA_ELASTIC_MODULUS_GPA)
    );
    let mut riskier = exact;
    riskier.length = 34.0;
    assert!(
        !riskier.is_safe_for_pla(),
        "閾値より高い応力で safe と判定された (σ={})",
        riskier.peak_stress_mpa(PLA_ELASTIC_MODULUS_GPA)
    );

    // PLA_STANDARD は安全率 2 未満 (doc の自己申告と一致、= 実プリント baseline)
    let std_stress = SnapFitCantileverSpec::PLA_STANDARD.peak_stress_mpa(PLA_ELASTIC_MODULUS_GPA);
    assert!(
        std_stress > threshold && !SnapFitCantileverSpec::PLA_STANDARD.is_safe_for_pla(),
        "PLA_STANDARD の安全率 2 未満という doc 記載と食い違う (σ={std_stress})"
    );
    // PETG は弾性率が低いので同 spec で応力も低い (E 比そのまま)
    let petg = SnapFitCantileverSpec::PLA_STANDARD.peak_stress_mpa(PETG_ELASTIC_MODULUS_GPA);
    assert!(
        approx(
            petg / std_stress,
            PETG_ELASTIC_MODULUS_GPA / PLA_ELASTIC_MODULUS_GPA,
            1e-4
        ),
        "PETG/PLA の応力比が弾性率比と一致しない"
    );
}

/// 片持ち snap-fit のフックは 梁上面に載り 先端と揃う
///
/// oracle: フック配置 `x = L/2 − o/2`、`y = t/2 + δ/2` ⇒
/// フック先端 `= L/2` (梁先端と一致)、フック下面 `= t/2` (梁上面と一致)、
/// フック上端 `= t/2 + δ` (突出量ちょうど δ)
#[test]
fn cantilever_hook_sits_on_the_beam_top_and_ends_flush_with_the_beam_tip() {
    let spec = SnapFitCantileverSpec::PLA_STANDARD;
    let node = snap_fit_cantilever(spec);
    // フック内部の点 (梁上面より上、フック区間内)
    let hx = spec.length * 0.5 - spec.hook_offset * 0.5;
    let hy = spec.thickness * 0.5 + spec.hook_height * 0.5;
    let start = Vec3::new(hx, hy, 0.0);
    assert!(
        eval(&node, start) < 0.0,
        "フック内部の測定開始点が内部でない"
    );

    let up = boundary(&node, start, Vec3::Y, 50.0);
    assert!(
        approx(hy + up, spec.thickness * 0.5 + spec.hook_height, GEO_TOL),
        "フック上端: 測定 {} / 閉形式 {}",
        hy + up,
        spec.thickness * 0.5 + spec.hook_height
    );
    let fwd = boundary(&node, start, Vec3::X, 50.0);
    assert!(
        approx(hx + fwd, spec.length * 0.5, GEO_TOL),
        "フック先端: 測定 {} / 梁先端 {}",
        hx + fwd,
        spec.length * 0.5
    );
    // 梁自体の寸法 (原点は梁の中)
    assert!(
        approx(
            boundary(&node, Vec3::ZERO, Vec3::X, 50.0),
            spec.length * 0.5,
            GEO_TOL
        ),
        "梁半長が L/2 でない"
    );
    assert!(
        approx(
            boundary(&node, Vec3::ZERO, Vec3::Z, 50.0),
            spec.width * 0.5,
            GEO_TOL
        ),
        "梁半幅が w/2 でない"
    );
    // 梁下面は δ の突出を受けない (フックは上面だけ)
    assert!(
        approx(
            boundary(&node, Vec3::ZERO, -Vec3::Y, 50.0),
            spec.thickness * 0.5,
            GEO_TOL
        ),
        "梁下面が t/2 でない (フックが下側にも出ている)"
    );
}

// ════════════════════════════════════════════════════════
// joint — 環状 snap-fit / スロット
// ════════════════════════════════════════════════════════

/// 環状 snap-fit の bulge は shaft 表面から ちょうど `bulge_height` 突出し、内縁は接する
///
/// oracle: `major = r + b/2`、`minor = b/2` ⇒
/// 外径 `= major + minor = r + b` (突出量ちょうど b)、内径 `= major − minor = r` (shaft 表面に接する)
/// ⚠️ `+b/2` が抜けると突出が b/2 になり、内縁が shaft に食い込む
#[test]
fn annular_bulge_protrudes_exactly_its_height_and_is_tangent_to_the_shaft() {
    for id in 0..4_u16 {
        for ib in 0..3_u16 {
            let shaft_d = f32::from(id).mul_add(3.0, 6.0);
            let bulge = f32::from(ib).mul_add(0.3, 0.4);
            let y_off = 7.0;
            let node = snap_fit_annular(shaft_d, 20.0, bulge, y_off);
            let shaft_r = shaft_d * 0.5;

            // bulge の高さで測った外径 = shaft 半径 + 突出量
            let outer = boundary(&node, Vec3::new(0.0, y_off, 0.0), Vec3::X, 100.0);
            assert!(
                approx(outer, shaft_r + bulge, GEO_TOL),
                "Ø{shaft_d} b={bulge} bulge 外径: 測定 {outer} / 閉形式 {}",
                shaft_r + bulge
            );
            // shaft だけの高さ (原点) では突出しない
            let plain = boundary(&node, Vec3::ZERO, Vec3::X, 100.0);
            assert!(
                approx(plain, shaft_r, GEO_TOL),
                "Ø{shaft_d} shaft 部の半径: 測定 {plain} / 閉形式 {shaft_r}"
            );
            // 突出量そのもの
            assert!(
                approx(outer - plain, bulge, GEO_TOL),
                "Ø{shaft_d} 突出量: 測定 {} / 指定 {bulge}",
                outer - plain
            );
            // 内縁が shaft 表面に接する = torus は shaft を削らない
            // (shaft 表面のすぐ内側は shaft 由来で内部のまま)
            assert!(
                eval(&node, Vec3::new(shaft_r - 0.05, y_off, 0.0)) < 0.0,
                "Ø{shaft_d} bulge が shaft を削っている"
            );
        }
    }
}

/// スロットの全長 / 幅 / 深さは指定値そのもの (両端半円を含めて)
///
/// oracle: 中央 box 長 `= L − w`、両端円 半径 `w/2` を `±(L−w)/2` に置く
/// ⇒ X 全長 `= (L−w)/2 + w/2 = L/2` (片側)、Z 半幅 `= w/2`、Y 半深 `= d/2`
/// ⚠️ 中央 box 長から `− w` が抜けると全長が `L + w` になる (既存 test は端点 1 点しか見ない)
#[test]
fn slot_overall_extents_equal_the_requested_length_width_and_depth() {
    for il in 0..4_u16 {
        for iw in 0..3_u16 {
            let width = f32::from(iw).mul_add(1.5, 3.0);
            let length = f32::from(il).mul_add(6.0, width + 6.0);
            let depth = 6.0;
            let node = slot(length, width, depth);
            let half_x = boundary(&node, Vec3::ZERO, Vec3::X, 10.0 * length);
            let half_z = boundary(&node, Vec3::ZERO, Vec3::Z, 10.0 * length);
            let half_y = boundary(&node, Vec3::ZERO, Vec3::Y, 10.0 * length);
            assert!(
                approx(half_x, length * 0.5, GEO_TOL),
                "L={length} w={width} スロット半長: 測定 {half_x} / 指定 {}",
                length * 0.5
            );
            assert!(
                approx(half_z, width * 0.5, GEO_TOL),
                "L={length} w={width} スロット半幅: 測定 {half_z} / 指定 {}",
                width * 0.5
            );
            assert!(
                approx(half_y, depth * 0.5, GEO_TOL),
                "L={length} w={width} スロット半深: 測定 {half_y} / 指定 {}",
                depth * 0.5
            );
        }
    }
}

/// スロット外部の距離場は 押出スタジアムの閉形式と一致
///
/// oracle: 領域は「XZ 平面の線分 ±(L−w)/2 を半径 w/2 で太らせた stadium」を Y に押し出した形
/// 外部では `d(p, A∪B) = min(d(p,A), d(p,B))` なので min 合成は厳密
/// ⇒ 外部点では 押出スタジアムの解析距離と一致するはず (内部は min 合成が保守的なので符号のみ見る)
#[test]
fn slot_exterior_distance_matches_the_extruded_stadium_closed_form() {
    let (length, width, depth) = (20.0_f32, 4.0_f32, 6.0_f32);
    let node = slot(length, width, depth);
    let center_len = length - width;

    // 押出スタジアムの解析距離 (独立に導出、実装を呼ばない)
    let reference = |p: Vec3| -> f32 {
        let qx = (p.x.abs() - center_len * 0.5).max(0.0);
        let d_xz = Vec2::new(qx, p.z).length() - width * 0.5;
        let d_y = p.y.abs() - depth * 0.5;
        Vec2::new(d_xz.max(0.0), d_y.max(0.0)).length() + d_xz.max(d_y).min(0.0)
    };

    let mut checked = 0_u32;
    let n: u16 = 13;
    for ix in 0..n {
        for iy in 0..n {
            for iz in 0..n {
                let u = |i: u16, s: f32| s.mul_add(2.0 * f32::from(i) / f32::from(n - 1), -s);
                let p = Vec3::new(u(ix, 18.0), u(iy, 9.0), u(iz, 9.0));
                let got = eval(&node, p);
                let want = reference(p);
                if want > 1e-3 {
                    // 外部: 厳密一致するはず
                    assert!(
                        approx(got, want, 2e-3),
                        "スロット外部 {p:?}: 実装 {got} / スタジアム閉形式 {want}"
                    );
                    checked += 1;
                } else {
                    // 内部 / 境界: 符号の一致のみ (min 合成は内部で保守的)
                    assert!(got <= 1e-3, "スタジアム内部の点 {p:?} が実装で外部 ({got})");
                }
            }
        }
    }
    assert!(
        checked > 500,
        "外部点の突合が {checked} 点しかない (空振りの疑い)"
    );
}

/// スロットの退化: 幅 ≥ 全長 でも有限値 (中央 box の半長が負になる領域)
///
/// ⚠️ `width > length` では `center_length` が負になり `Box3d` の半 extent が負になる
/// panic しないこと / 有限であること を確認する (正しい形かは仕様判断なので主張しない)
#[test]
fn slot_stays_finite_when_width_meets_or_exceeds_length() {
    for (length, width) in [(4.0_f32, 4.0_f32), (4.0, 6.0), (0.0, 0.0), (5.0, 0.0)] {
        let node = slot(length, width, 6.0);
        for p in [
            Vec3::ZERO,
            Vec3::new(0.2, 0.2, 0.2),
            Vec3::new(9.0, -4.0, 3.0),
        ] {
            let d = eval(&node, p);
            assert!(d.is_finite(), "slot(L={length}, w={width}) が {p:?} で {d}");
        }
    }
}

// ════════════════════════════════════════════════════════
// joint — T スロット / アリ継ぎ
// ════════════════════════════════════════════════════════

/// 2020 T スロットの開口と内部チャンバーは すきま無く連続する
///
/// oracle: 開口は X ∈ [−`od`/2, +`od`/2]、内部は中心 `−(od+id)/2` 半 extent `id`/2
/// ⇒ 内部の上端 `= −(od+id)/2 + id/2 = −od/2` = 開口の下端 で **ちょうど接する**
/// ⇒ 開口面から内部最深まで X 方向に切れ目なく内部が続く
/// ⚠️ 配置係数が変わると隙間が開き、subtract した時にチャンバーが孤立する
#[test]
fn t_slot_opening_and_inner_chamber_are_contiguous_with_no_gap() {
    let node = t_slot_2020(100.0);
    let od = T_SLOT_2020_OPENING_DEPTH;
    let id = T_SLOT_2020_INNER_DEPTH;

    // 接合面の代数恒等式
    let inner_center = -(od + id) * 0.5;
    assert!(
        approx(inner_center + id * 0.5, -od * 0.5, 1e-6),
        "内部チャンバー上端 {} が開口下端 {} と一致しない",
        inner_center + id * 0.5,
        -od * 0.5
    );

    // 開口面 (+od/2) から内部最深 (−od/2 − id) まで、中心線上が全部内部
    let x_from = od * 0.5 - 1e-3;
    let x_to = -od * 0.5 - id + 1e-3;
    let n: u16 = 400;
    for i in 0..=n {
        let x = x_from + (x_to - x_from) * f32::from(i) / f32::from(n);
        let d = eval(&node, Vec3::new(x, 0.0, 0.0));
        // 接合面上では SDF がちょうど 0 になるので境界込みで判定する
        // (すきまが開けば d は明確に正になるので、これでも隙間は捕まる)
        assert!(
            d <= 1e-4,
            "T スロットの通路が X={x} で途切れている (SDF {d}) = 開口と内部が不連続"
        );
    }
    // 最深より外は外部
    assert!(
        eval(&node, Vec3::new(-od * 0.5 - id - 0.2, 0.0, 0.0)) > 0.0,
        "内部チャンバーが公称深さより深い"
    );
    // 内部チャンバー最深面 = −(od/2 + id)
    // ⚠️ 起点を接合面の内側 (チャンバー中心) に取る 原点から測ると接合面で止まり得る
    let deep = boundary(&node, Vec3::new(inner_center, 0.0, 0.0), -Vec3::X, 200.0);
    assert!(
        approx(inner_center - deep, -(od * 0.5 + id), GEO_TOL),
        "T スロット最深面: 測定 {} / 閉形式 {}",
        inner_center - deep,
        -(od * 0.5 + id)
    );
}

/// 2020 T スロットの開口幅 6 mm / 内部幅 11 mm を **場から測って** 突合
///
/// oracle: MISUMI / `OpenBuilds` 2020 規格 (開口 6 / 内部 11、M5 ナット収納)
/// 内部が開口より広い = T 字断面が成立している
#[test]
fn t_slot_widths_measured_from_the_field_match_the_2020_profile_spec() {
    let length = 100.0_f32;
    let node = t_slot_2020(length);
    // 開口中央で Z 幅
    let open_half = boundary(&node, Vec3::ZERO, Vec3::Z, 100.0);
    assert!(
        approx(open_half, T_SLOT_2020_OPENING_WIDTH * 0.5, GEO_TOL),
        "開口半幅: 測定 {open_half} / 規格 {}",
        T_SLOT_2020_OPENING_WIDTH * 0.5
    );
    // 内部チャンバー中央で Z 幅
    let inner_x = -(T_SLOT_2020_OPENING_DEPTH + T_SLOT_2020_INNER_DEPTH) * 0.5;
    let inner_half = boundary(&node, Vec3::new(inner_x, 0.0, 0.0), Vec3::Z, 100.0);
    assert!(
        approx(inner_half, T_SLOT_2020_INNER_WIDTH * 0.5, GEO_TOL),
        "内部半幅: 測定 {inner_half} / 規格 {}",
        T_SLOT_2020_INNER_WIDTH * 0.5
    );
    assert!(
        inner_half > open_half,
        "内部幅 {inner_half} が開口幅 {open_half} 以下 = T 字断面になっていない"
    );
    // プロファイル長 (Y) は指定どおり
    let half_len = boundary(&node, Vec3::ZERO, Vec3::Y, 10.0 * length);
    assert!(
        approx(half_len, length * 0.5, GEO_TOL),
        "プロファイル半長: 測定 {half_len} / 指定 {}",
        length * 0.5
    );
}

/// アリ継ぎの半幅は高さの 1 次関数、傾きは `tan(10°)` で **上に向かって狭まる**
///
/// oracle: 左右平面 `dot(p, (±cos θ, sin θ, 0)) = d`、`d = cos θ·b/2 − sin θ·h/2`
/// ⇒ 高さ y での半幅 `w(y) = b/2 − (y + h/2)·tan θ`
/// ⇒ 底 (`y = −h/2`) で `b/2`、頂 (`y = +h/2`) で `b/2 − h·tan θ`、傾き `−tan θ`
/// ⚠️ 上に向かって **狭まる** ことが差し込み方向の抜け止め そこが逆になると継手が機能しない
#[test]
fn dovetail_half_width_is_linear_in_height_with_the_documented_taper() {
    let tan_t = DOVETAIL_TAPER_DEG.to_radians().tan();
    for ib in 0..3_u16 {
        for ih in 0..3_u16 {
            let base = f32::from(ib).mul_add(4.0, 10.0);
            let height = f32::from(ih).mul_add(2.0, 4.0);
            let node = dovetail(base, height, 20.0);

            let mut samples: Vec<(f32, f32)> = Vec::new();
            let n: u16 = 6;
            for i in 0..=n {
                // 上下端を避けた内部の高さ
                let frac = 0.1 + 0.8 * f32::from(i) / f32::from(n);
                let y = height.mul_add(frac, -height * 0.5);
                let w = boundary(&node, Vec3::new(0.0, y, 0.0), Vec3::X, 10.0 * base);
                let expect = (y + height * 0.5).mul_add(-tan_t, base * 0.5);
                assert!(
                    approx(w, expect, GEO_TOL),
                    "b={base} h={height} y={y} 半幅: 測定 {w} / 閉形式 {expect}"
                );
                samples.push((y, w));
            }
            // 傾き = −tan(10°)
            let (y0, w0) = *samples.first().expect("高さ sample が空");
            let (y1, w1) = *samples.last().expect("高さ sample が空");
            let slope = (w1 - w0) / (y1 - y0);
            assert!(
                approx(slope, -tan_t, 1e-3),
                "b={base} h={height} 半幅の傾き: 測定 {slope} / 閉形式 {}",
                -tan_t
            );
            // 上に向かって狭まる = 抜け止めが成立
            assert!(
                w1 < w0,
                "b={base} h={height} アリ継ぎが上に向かって広がっている (抜け止めが逆)"
            );
        }
    }
}

/// アリ継ぎの底辺幅 / 上辺幅 / 高さ が閉形式どおり
///
/// oracle: 底辺 `= b`、上辺 `= b − 2·h·tan(10°)`、高さ `= h`、押出 `= depth`
#[test]
fn dovetail_bottom_and_top_widths_match_the_trapezoid_closed_form() {
    let tan_t = DOVETAIL_TAPER_DEG.to_radians().tan();
    let (base, height, depth) = (10.0_f32, 5.0_f32, 20.0_f32);
    let node = dovetail(base, height, depth);

    let eps = 1e-3;
    let bottom = boundary(
        &node,
        Vec3::new(0.0, -height * 0.5 + eps, 0.0),
        Vec3::X,
        100.0,
    );
    assert!(
        approx(2.0 * bottom, base, 5e-3),
        "底辺幅: 測定 {} / 指定 {base}",
        2.0 * bottom
    );
    let top = boundary(
        &node,
        Vec3::new(0.0, height * 0.5 - eps, 0.0),
        Vec3::X,
        100.0,
    );
    let expect_top = 2.0_f32.mul_add(-(height * tan_t), base);
    assert!(
        approx(2.0 * top, expect_top, 5e-3),
        "上辺幅: 測定 {} / 閉形式 {expect_top}",
        2.0 * top
    );
    // 高さ / 押出
    assert!(
        approx(
            boundary(&node, Vec3::ZERO, Vec3::Y, 100.0),
            height * 0.5,
            GEO_TOL
        ),
        "アリ継ぎ半高が h/2 でない"
    );
    assert!(
        approx(
            boundary(&node, Vec3::ZERO, Vec3::Z, 100.0),
            depth * 0.5,
            GEO_TOL
        ),
        "アリ継ぎ半押出が depth/2 でない"
    );
}

// ════════════════════════════════════════════════════════
// joint — ピンヒンジ / コネクタ
// ════════════════════════════════════════════════════════

/// knuckle の pin 穴は pin 径より ちょうど `HINGE_CLEARANCE` 太い
///
/// oracle: 穴直径 `= pin_dia + HINGE_CLEARANCE` ⇒ 径方向すきま `= HINGE_CLEARANCE/2` = 0.15 mm
/// ⇒ 呼び径 `pin_dia` のピンは穴の中で必ず回る (すきま嵌め)
/// ⚠️ すきまが 0 になると圧入になりヒンジが回らないが、既存 test は
/// 「中心が空」「X=3 が材料」しか見ないので 0.15 mm の消失を捕まえない
#[test]
fn pin_hinge_bore_exceeds_the_pin_by_exactly_the_hinge_clearance() {
    for ip in 0..5_u16 {
        let pin = f32::from(ip).mul_add(0.5, 2.0);
        let knuckle_len = 8.0;
        let od = pin.mul_add(2.5, 1.0);
        let node = pin_hinge_knuckle(pin, knuckle_len, od);

        // 穴の内側 (原点) から半径方向に出ると 穴壁 = 材料の始まり
        let bore_r = boundary(&node, Vec3::ZERO, Vec3::X, 10.0 * od);
        let expect = (pin + HINGE_CLEARANCE) * 0.5;
        assert!(
            approx(bore_r, expect, GEO_TOL),
            "pin Ø{pin}: 穴半径 測定 {bore_r} / 閉形式 {expect}"
        );
        // 径方向すきま
        assert!(
            approx(bore_r - pin * 0.5, HINGE_CLEARANCE * 0.5, GEO_TOL),
            "pin Ø{pin}: 径方向すきま 測定 {} / 閉形式 {}",
            bore_r - pin * 0.5,
            HINGE_CLEARANCE * 0.5
        );
        // 呼び径のピン表面が穴の中で浮いている (すきま嵌めの実質検査)
        let n: u16 = 24;
        for i in 0..n {
            let a = std::f32::consts::TAU * f32::from(i) / f32::from(n);
            let p = Vec3::new(pin * 0.5 * a.cos(), 0.0, pin * 0.5 * a.sin());
            assert!(
                eval(&node, p) > 0.0,
                "pin Ø{pin}: ピン表面の点 {p:?} が材料に当たる (すきま嵌めでない)"
            );
        }
        // 肉厚 = 外半径 − 穴半径 を材料内の点から両方向に測って突合
        // oracle: 閉形式 `(od − (pin + clearance)) / 2`
        let x_mat = f32::midpoint(bore_r, od * 0.5);
        assert!(
            eval(&node, Vec3::new(x_mat, 0.0, 0.0)) < 0.0,
            "pin Ø{pin}: 肉厚測定の起点が材料内でない"
        );
        let out_r = x_mat + boundary(&node, Vec3::new(x_mat, 0.0, 0.0), Vec3::X, 10.0 * od);
        let in_r = x_mat - boundary(&node, Vec3::new(x_mat, 0.0, 0.0), -Vec3::X, 10.0 * od);
        assert!(
            approx(out_r, od * 0.5, GEO_TOL),
            "pin Ø{pin}: 外半径 測定 {out_r} / 指定 {}",
            od * 0.5
        );
        assert!(
            approx(in_r, expect, GEO_TOL),
            "pin Ø{pin}: 穴半径 (材料側から) 測定 {in_r} / 閉形式 {expect}"
        );
        let wall = out_r - in_r;
        assert!(
            approx(wall, (od - pin - HINGE_CLEARANCE) * 0.5, GEO_TOL) && wall > 0.0,
            "pin Ø{pin}: 肉厚 測定 {wall} / 閉形式 {}",
            (od - pin - HINGE_CLEARANCE) * 0.5
        );
    }
}

/// knuckle の外径と 軸方向の貫通余裕 5 mm
///
/// oracle: barrel 半高 `= knuckle_len/2`、穴 半高 `= knuckle_len/2 + 5`
/// ⇒ 穴は両端に 5 mm はみ出す = preview MC で確実に貫通する
#[test]
fn pin_hinge_hole_punches_through_both_ends_with_the_documented_margin() {
    let (pin, knuckle_len, od) = (3.0_f32, 8.0_f32, 8.0_f32);
    let node = pin_hinge_knuckle(pin, knuckle_len, od);

    // 材料内部 (穴壁より外、外径より内) の半径
    let bore_r = (pin + HINGE_CLEARANCE) * 0.5;
    let x = f32::midpoint(bore_r, od * 0.5);
    // 軸方向の材料端 = knuckle_len/2
    let half = boundary(&node, Vec3::new(x, 0.0, 0.0), Vec3::Y, 100.0);
    assert!(
        approx(half, knuckle_len * 0.5, GEO_TOL),
        "knuckle 半長: 測定 {half} / 指定 {}",
        knuckle_len * 0.5
    );
    // 外径 (⚠️ boundary は「起点からの距離」を返すので絶対座標に直して比べる)
    let r_out = boundary(&node, Vec3::new(x, 0.0, 0.0), Vec3::X, 100.0);
    assert!(
        approx(x + r_out, od * 0.5, GEO_TOL),
        "knuckle 外半径: 測定 {} / 指定 {}",
        x + r_out,
        od * 0.5
    );
    // 穴は材料端を越えて続く (軸上は端の外でも空)
    for dy in [0.1_f32, 2.0, 4.9] {
        let p = Vec3::new(0.0, knuckle_len * 0.5 + dy, 0.0);
        assert!(
            eval(&node, p) > 0.0,
            "軸上 {p:?} が材料内 = 穴が貫通していない"
        );
    }
}

/// JST-PH スロット幅は doc の spec 表どおり、1 pin 増えるとちょうど 1 pitch 増える
///
/// oracle: 幅 `= JST_PH_PITCH · (pins + 1)`
/// doc の表 (S2B 6 / S3B 8 / S4B 10 / S5B 12 mm) と 4 点で一致し、
/// 隣接ピン数の差はちょうど `JST_PH_PITCH` = 2.0 mm
/// ⚠️ JST の housing 実寸 (depth 4.5 / height 5.5) は本 repo 外の値で未検証
#[test]
fn jst_ph_slot_width_follows_the_pitch_table_with_one_pitch_per_pin() {
    for (pins, want) in [(2_u32, 6.0_f32), (3, 8.0), (4, 10.0), (5, 12.0)] {
        let node = jst_ph_slot(pins);
        let half = boundary(&node, Vec3::ZERO, Vec3::X, 100.0);
        assert!(
            approx(2.0 * half, want, GEO_TOL),
            "{pins}-pin 幅: 測定 {} / doc の表 {want}",
            2.0 * half
        );
    }
    // 1 pin あたりちょうど 1 pitch
    for pins in 2..12_u32 {
        let a = boundary(&jst_ph_slot(pins), Vec3::ZERO, Vec3::X, 100.0);
        let b = boundary(&jst_ph_slot(pins + 1), Vec3::ZERO, Vec3::X, 100.0);
        assert!(
            approx(2.0 * (b - a), JST_PH_PITCH, GEO_TOL),
            "{pins} -> {} pin の幅差: 測定 {} / pitch {JST_PH_PITCH}",
            pins + 1,
            2.0 * (b - a)
        );
    }
}

/// JST-PH のピン数は 2..=12 に clamp される (退化入力)
#[test]
fn jst_ph_slot_clamps_the_pin_count_to_the_supported_range() {
    let w = |pins: u32| 2.0 * boundary(&jst_ph_slot(pins), Vec3::ZERO, Vec3::X, 200.0);
    let w2 = w(2);
    let w12 = w(12);
    for low in [0_u32, 1] {
        assert!(
            approx(w(low), w2, GEO_TOL),
            "pins={low} が 2-pin 幅に clamp されていない ({} vs {w2})",
            w(low)
        );
    }
    for high in [13_u32, 100, u32::MAX] {
        assert!(
            approx(w(high), w12, GEO_TOL),
            "pins={high} が 12-pin 幅に clamp されていない ({} vs {w12})",
            w(high)
        );
    }
}

/// 全 joint primitive は 1-Lipschitz (厳密 SDF の必要条件)
#[test]
fn joint_primitives_are_one_lipschitz() {
    let nodes: [(&str, SdfNode); 6] = [
        (
            "snap_fit_cantilever",
            snap_fit_cantilever(SnapFitCantileverSpec::PLA_STANDARD),
        ),
        (
            "snap_fit_annular",
            snap_fit_annular(8.0, 20.0, ANNULAR_BULGE_STANDARD_HEIGHT, 7.0),
        ),
        ("slot", slot(20.0, 4.0, 6.0)),
        ("t_slot_2020", t_slot_2020(40.0)),
        ("dovetail", dovetail(10.0, 5.0, 20.0)),
        ("pin_hinge_knuckle", pin_hinge_knuckle(3.0, 8.0, 8.0)),
    ];
    for (name, node) in &nodes {
        let q = max_difference_quotient(node, 15.0);
        assert!(
            q <= 1.0 + 1e-2,
            "{name} の差分商が 1 を超えた ({q}) = 厳密 SDF でない"
        );
    }
}

/// joint primitive の退化寸法 (0 / 負 / 逆転) でも有限
#[test]
fn joint_primitives_stay_finite_on_degenerate_dimensions() {
    let probes = [
        Vec3::ZERO,
        Vec3::new(0.2, 0.2, 0.2),
        Vec3::new(-6.0, 3.0, 2.0),
    ];
    let zero_spec = SnapFitCantileverSpec {
        length: 0.0,
        width: 0.0,
        thickness: 0.0,
        hook_height: 0.0,
        hook_offset: 0.0,
    };
    let nodes: [(&str, SdfNode); 7] = [
        ("cantilever zero", snap_fit_cantilever(zero_spec)),
        ("annular zero", snap_fit_annular(0.0, 0.0, 0.0, 0.0)),
        (
            "annular negative bulge",
            snap_fit_annular(8.0, 20.0, -0.4, 7.0),
        ),
        ("t_slot zero length", t_slot_2020(0.0)),
        ("dovetail zero", dovetail(0.0, 0.0, 0.0)),
        // 外径 < 穴径 = barrel が穴に飲まれる
        ("knuckle od below bore", pin_hinge_knuckle(5.0, 8.0, 2.0)),
        ("knuckle zero", pin_hinge_knuckle(0.0, 0.0, 0.0)),
    ];
    for (name, node) in &nodes {
        for p in probes {
            let d = eval(node, p);
            assert!(d.is_finite(), "{name} が {p:?} で非有限値 {d}");
        }
    }
    // 応力式の退化 (L = 0) は無限大になる ⇒ 安全判定は必ず false
    let degenerate = SnapFitCantileverSpec {
        length: 0.0,
        width: 5.0,
        thickness: 2.0,
        hook_height: 0.5,
        hook_offset: 1.5,
    };
    assert!(
        !degenerate.is_safe_for_pla(),
        "L=0 (応力が発散) で safe と判定された"
    );
}
