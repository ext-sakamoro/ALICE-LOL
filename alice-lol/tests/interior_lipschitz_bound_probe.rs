//! 内部点からの Lipschitz 下界 `|f(p)| / L` が健全かを総当たりで反証する
//!
//! # 測る主張
//!
//! `alice_sdf::interval::eval_lipschitz` の契約 (`ALICE-SDF/src/interval.rs`) は逐語で
//!
//! > `|f(p) − f(q)| ≤ L·|p − q|` whenever `f(p) ≥ 0` **or** `f(q) ≥ 0`
//!
//! と書かれている **`or`** なので、`p` が内部 (`f(p) < 0`) であっても相手 `q` が
//! `f(q) ≥ 0` を満たせば pair は契約内に入る したがって
//!
//! - `q` が表面上 (`f(q) = 0`) なら `|f(p)| ≤ L·|p − q|` ⇒ `|p − q| ≥ |f(p)| / L`
//! - `q` が外部 (`f(q) > 0`) なら `|f(p)| + f(q) ≤ L·|p − q|` ⇒ さらに強い
//!
//! ⇒ **内部点 `p` を中心とする半径 `|f(p)| / L` の開球には `f ≥ 0` な点が 1 つも無い**
//!
//! これは `law::check_min_thickness` / `check_stress` が要求する「内部距離の下界」
//! そのものだが、現在 `probe_ball` はこの早期判定を持たない (octree を
//! `BALL_PROBE_DEPTH` まで降りて決まらなければ `SurfaceProximity` で未決定にする)
//!
//! # 本 test が測る 2 つ
//!
//! 1. **現状 pin** — 素朴な内部拡張が破れることを固定する (2026-09-30 実測 813 件)
//! 2. **対照実験** — ⚠️ **両端が外部** の pair で同じ反証を回す 契約の真の scope なので、
//!    ここに出る construct は **`eval_lipschitz` 側の不健全** 実測で内部と**同一の 3 件**
//!    (`plane` / `parabola_segment` / `superellipsoid`) が出た ⇒ 真因は契約の scope でなく `L` の過小
//! 3. **発火率 (print)** — 主張が `MinThickness` の未決定をどれだけ潰しうるか (実測 89.6%)
//!
//! # ⚠️ 検査器自身が 4 回 誤報した (手順として残す)
//!
//! (a) 退化点 (`|f(p)| ≈ 0` で probe 半径 ≈ 0) が 0/0 の ratio 8.0 を報告 → `MIN_RADIUS`
//! (b) 厳密距離場は `|Δf| = |Δp|` で ratio が厳密 1、f32 丸めで 1+1ulp → `RATIO_TOL`
//! (c) 「外部対照」のつもりで `q` が**内部**の pair を数えていた (内部版の鏡像で対照にならない)
//! (d) 破れ幅を出すまで (a) に気付けなかった
//! ⇒ **件数だけ見ず必ず margin と実例座標を出す**
//!
//! # ⚠️ production を変えない
//!
//! 本 test は `src/` を 1 行も変えずに public API だけで測る 早期判定を実装に
//! 入れてよいかは、ここで健全性が確認できてから決める (合格側の主張なので、
//! `L` が過小な construct で入れると偽 `proven` になる)
//!
//! 起票: 2026-09-30 (World Auditor 段 3 の欠けている primitive 2 本のうち
//! 「`MinThickness` の内部距離の上界」の実現可能性調査)

// grid の index ⇄ 座標変換に限った許容 (値域は高々 16 で f32 の仮数に収まる)
// ⚠️ `suboptimal_flops` は **採用しない** — clippy が勧める `mul_add` は FMA の有無で
//    結果の bit が変わるので、bit 一致を要求する系では使えない (罠 `mul-add-breaks-bit-exactness`)
//    本 test は SDF の値を直接比べるので同じ規律を適用する
#![allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]

mod common;

use alice_lol::runtime_parser::parse_lol;
use alice_lol::{eval, SdfNode};
use common::corpus::grammar_corpus;
use glam::Vec3;

/// 検査範囲 (`law_corpus_oracle.rs` の `AABB` と揃える)
const AABB: f32 = 2.0;
/// 内部点を拾う grid の 1 軸分割数
const GRID: usize = 10;
/// 反証で球殻をなぞる方向数 (フィボナッチ球)
const DIRECTIONS: usize = 64;
/// 1 方向あたり半径を何段階見るか (`r` に向かって内側から詰める)
const RADIAL_STEPS: usize = 6;
/// 反証半径に掛ける係数 — 主張は開球なので境界ちょうどは違反にしない
const SHELL: f32 = 0.995;
/// `MinThickness` の既定要求値 (`law.rs` の doctest / corpus oracle と同じ桁)
const REQUIRED: f32 = 0.2;
/// ⚠️ **退化除外の下限半径** — `|f(p)|` が 0 近傍だと probe 半径が ~0 になり、
/// 距離 ~1e-7 の隣接点との差が f32 丸めに支配されて **偽の反例**になる
/// (2026-09-30 実測: filter 無しだと `superellipsoid` が `f(p)=0 / |p-q|=0` で
/// ratio 8.0 を報告した = 0/0 の比) 本 filter 自身が空振りしないことは
/// 検査件数の assert で担保する
const MIN_RADIUS: f32 = 1.0e-3;
/// ⚠️ **契約違反とみなす ratio の下限** — 厳密距離場では `|Δf| = |Δp|` かつ `L = 1` で
/// ratio が **厳密に 1** になり、f32 丸めで `1 + 1ulp` に振れる (2026-09-30 実測:
/// filter 無しだと `wrench_holder` / `gridfinity_bin_ex` 等が「最大 ratio 1.000」で
/// 大量に報告された = 境界 artifact) 本当の破れは ratio 1.7〜9.9 の桁で出る
const RATIO_TOL: f32 = 1.05;

fn nodes() -> Vec<(String, SdfNode)> {
    let mut out = Vec::new();
    for (name, snippet) in grammar_corpus() {
        if let Ok(node) = parse_lol(&snippet) {
            out.push((name, node));
        }
    }
    out
}

/// 一様に近い方向ベクトル (フィボナッチ球)
fn directions() -> Vec<Vec3> {
    let golden = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
    (0..DIRECTIONS)
        .map(|i| {
            let y = 1.0 - (i as f32 / (DIRECTIONS - 1) as f32) * 2.0;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let theta = golden * i as f32;
            Vec3::new(theta.cos() * r, y, theta.sin() * r)
        })
        .collect()
}

/// AABB 内の格子点
fn grid_points() -> Vec<Vec3> {
    let mut pts = Vec::with_capacity(GRID * GRID * GRID);
    let step = (AABB * 2.0) / (GRID - 1) as f32;
    for ix in 0..GRID {
        for iy in 0..GRID {
            for iz in 0..GRID {
                pts.push(Vec3::new(
                    -AABB + step * ix as f32,
                    -AABB + step * iy as f32,
                    -AABB + step * iz as f32,
                ));
            }
        }
    }
    pts
}

/// `p` を中心とする半径 `r` の球内に `f ≥ 0` な点があれば返す (= 主張の反例)
///
/// 主張が真なら `None` `Some((q, f(q)))` が返ったらその construct の `L` は過小
fn find_counterexample(node: &SdfNode, p: Vec3, r: f32, dirs: &[Vec3]) -> Option<(Vec3, f32)> {
    for d in dirs {
        for s in 1..=RADIAL_STEPS {
            let rad = r * SHELL * (s as f32 / RADIAL_STEPS as f32);
            let q = p + *d * rad;
            let fq = eval(node, q);
            if fq >= 0.0 {
                return Some((q, fq));
            }
        }
    }
    None
}

/// ⚠️ **現状 pin** — 素朴な内部拡張 `|f(p)| / L` は **健全でない** ことを固定する
///
/// 2026-09-30 実測: corpus 226 construct / 内部点 107,380 に対し **反例 1,036 件**
/// (`parabola_segment` ほか) ⇒ **`probe_ball` に早期 `Clear` を入れてはいけない**
///
/// ⚠️⚠️ **真因は契約の scope でなく `eval_lipschitz` の不健全** — 対照実験
/// (`the_exterior_control_separates_contract_scope_from_unsound_l`) が
/// **両端が外部** の pair でも同じ 3 construct を検出した:
///
/// | construct | 内部で反例 | **外部 (契約の真の scope) で反例** | 最大 ratio |
/// |---|---|---|---|
/// | `plane` | 767 点 | **206 点** | **3.708** |
/// | `parabola_segment` | 38 点 | **169 点** | **9.861** |
/// | `superellipsoid` | 8 点 | **981 点** | **2.617** |
///
/// ⚠️ **内部だけで破れる construct は 1 つも無い** (223 construct / 約 107,000 内部点で反例 0)
/// ⇒ 素朴な内部拡張が落ちるのは **`eval_lipschitz` が自分の契約を守れていない 3 件のみ**
/// ⇒ **`eval_lipschitz` を parameter 依存に健全化すれば内部拡張は成立しうる**
///
/// ⚠️ ただし **標本は離散** (方向 64 × 半径 6 段) なので「内部固有の反例が無い」は
/// 証拠であって証明ではない doc の rationale が内部の不連続を明示的に許している以上、
/// 健全化後に改めて測り直すこと
///
/// 反転の合図: **`eval_lipschitz` の 3 件が健全化されたら** 本 pin は red になり、
/// `the_interior_distance_bound_has_no_counterexample` (下、`#[ignore]`) に置き換わる
#[test]
fn the_naive_interior_extension_of_the_lipschitz_bound_is_refuted() {
    let (checked_points, violations) = measure_refutations();
    assert!(
        checked_points > 1000,
        "内部点を {checked_points} 点しか見ていない (grid / corpus が縮退している)"
    );
    // ⚠️ 「反例がある」を pin する 0 件になったら契約か実装が変わった合図なので red にする
    assert!(
        violations > 0,
        "素朴な内部拡張の反例が 0 件になった — 契約か eval_lipschitz が変わった可能性があるので、\
         早期 Clear の可否を測り直すこと (旧実測: 226 construct / 107,380 内部点 で 1,036 件)"
    );
}

/// ⚠️ **目標 oracle** — 健全な内部距離下界が入れば green になる
///
/// 現状は `|f(p)| / L` をそのまま使っているので red (上の現状 pin が測っている 1,036 件)
/// **健全な下界の実装が landing したら `#[ignore]` を外す**
#[test]
#[ignore = "健全な内部距離下界が未実装 (素朴な |f|/L は 2026-09-30 実測で 1,036 件の反例、現状は上の pin が固定)"]
fn the_interior_distance_bound_has_no_counterexample() {
    let (checked_points, violations) = measure_refutations();
    assert!(
        checked_points > 1000,
        "内部点を {checked_points} 点しか見ていない (grid / corpus が縮退している)"
    );
    assert_eq!(
        violations, 0,
        "内部距離の下界が {violations} 件で破れた (= 下界が健全でない、早期判定を入れると偽 proven になる)"
    );
}

/// 反例の総当たり 戻り値は `(検査した内部点数, 反例件数)` 数値は必ず print する
///
/// ⚠️ 2 つの test が同じ測定を共有する (双子構造なので測定がずれると意味が壊れる)
fn measure_refutations() -> (usize, usize) {
    let dirs = directions();
    let pts = grid_points();
    let all = nodes();
    assert!(
        all.len() >= 100,
        "corpus が {} 件しか取れていない (grammar_corpus の取得失敗)",
        all.len()
    );

    let mut checked_nodes = 0usize;
    let mut checked_points = 0usize;
    let mut skipped_infinite_l = 0usize;
    let mut violations: Vec<(String, Vec3, f32, f32, Vec3, f32)> = Vec::new();

    for (name, node) in &all {
        let l = alice_sdf::interval::eval_lipschitz(node);
        if !l.is_finite() || l <= 0.0 {
            skipped_infinite_l += 1;
            continue; // L を主張しない construct は下界を作れない (対象外)
        }
        checked_nodes += 1;

        for p in &pts {
            let fp = eval(node, *p);
            if fp >= 0.0 {
                continue; // 内部点のみ
            }
            let r = fp.abs() / l;
            if !r.is_finite() || r < MIN_RADIUS {
                continue; // 退化 (表面近傍で半径 ~0) は f32 丸めに支配されるので除外
            }
            checked_points += 1;
            if let Some((q, fq)) = find_counterexample(node, *p, r, &dirs) {
                violations.push((name.clone(), *p, fp, r, q, fq));
            }
        }
    }

    println!("  素朴な内部拡張 |f(p)|/L の反証結果");
    println!(
        "    construct: 全 {} / L 有限 {checked_nodes} / L 非有限で対象外 {skipped_infinite_l}",
        all.len()
    );
    println!("    検査した内部点: {checked_points}");
    println!("    反例: {}", violations.len());
    let mut by_name: Vec<(String, usize)> = Vec::new();
    for (name, ..) in &violations {
        match by_name.iter_mut().find(|(n, _)| n == name) {
            Some((_, c)) => *c += 1,
            None => by_name.push((name.clone(), 1)),
        }
    }
    by_name.sort_by_key(|e| std::cmp::Reverse(e.1));
    println!("    反例を出した construct: {}", by_name.len());
    for (name, count) in by_name.iter().take(10) {
        println!("      {name}: {count} 点");
    }
    for (name, p, fp, r, q, fq) in violations.iter().take(3) {
        let d = p.distance(*q);
        println!(
            "      例 {name}: p={p:?} f(p)={fp:.6} 主張 r={r:.6} だが q={q:?} f(q)={fq:.6} |p-q|={d:.6}"
        );
    }

    (checked_points, violations.len())
}

/// ⚠️ **対照実験** — 同じ反証を **外部点** `p` (`f(p) > 0`) で回す
///
/// 契約は逐語で `{f ≥ 0}` 上の主張なので、**外部点で破れたらそれは `eval_lipschitz`
/// 側の不健全** (契約が守られていない) 内部でだけ破れるなら、契約が内部を
/// 除外しているのが効いている = 素朴な内部拡張が誤りだった、という読みになる
///
/// ⚠️ **対照群を置かないと「内部だから破れた」と「そもそも L が過小」が区別できない**
/// (判別力の証拠を置く形)
///
/// 数値を print するだけで合否は決めない (どちらの結果も情報であり、閾値を先に
/// 決めていないため) ⚠️ ただし **検査件数 0 での空振りだけは red にする**
#[test]
fn the_exterior_control_separates_contract_scope_from_unsound_l() {
    let dirs = directions();
    let pts = grid_points();
    let all = nodes();

    let mut checked_points = 0usize;
    #[allow(clippy::type_complexity)]
    let mut by_name: Vec<(String, usize, f32, Option<(Vec3, f32, Vec3, f32)>)> = Vec::new();

    for (name, node) in &all {
        let l = alice_sdf::interval::eval_lipschitz(node);
        if !l.is_finite() || l <= 0.0 {
            continue;
        }
        for p in &pts {
            let fp = eval(node, *p);
            if fp <= 0.0 {
                continue; // ⚠️ 外部点のみ (内部版の鏡像)
            }
            let r = fp / l;
            if !r.is_finite() || r < MIN_RADIUS {
                continue; // 同上
            }
            checked_points += 1;
            // 外部点なら「半径 r の球内に f <= 0 な点が無い」が契約から従う
            // ⚠️ 破れ幅も測る — 契約は |f(p) − f(q)| ≤ L·|p − q| なので
            //    ratio = |f(p) − f(q)| / (L·|p − q|) が 1 を超えた分が破れ
            //    discretization 由来の偽陽性なら ratio は 1 の直上に張り付く
            let mut worst: Option<(f32, Vec3, f32)> = None;
            for d in &dirs {
                for s in 1..=RADIAL_STEPS {
                    let rad = r * SHELL * (s as f32 / RADIAL_STEPS as f32);
                    let q = *p + *d * rad;
                    let fq = eval(node, q);
                    // ⚠️⚠️ 契約の scope は **両端が f >= 0** (doc の rationale
                    //    「内部の不連続で bound を汚さない」が成立するための条件)
                    //    q が内部の pair を数えると、内部版と同じ「契約外」を測ることになり
                    //    対照群にならない (2026-09-30 に実際にその誤りを 1 度踏んだ)
                    if fq < 0.0 {
                        continue;
                    }
                    let dist = p.distance(q);
                    if dist < MIN_RADIUS {
                        continue; // 退化 (f32 丸めに支配される)
                    }
                    let ratio = (fp - fq).abs() / (l * dist);
                    if ratio <= RATIO_TOL {
                        continue; // 契約を満たしている (境界 artifact を含む)
                    }
                    if worst.is_none_or(|(w, _, _)| ratio > w) {
                        worst = Some((ratio, q, fq));
                    }
                }
            }
            if let Some((ratio, q, fq)) = worst {
                match by_name.iter_mut().find(|(n, _, _, _)| n == name) {
                    Some((_, c, w, ex)) => {
                        *c += 1;
                        if ratio > *w {
                            *w = ratio;
                            *ex = Some((*p, fp, q, fq));
                        }
                    }
                    None => by_name.push((name.clone(), 1, ratio, Some((*p, fp, q, fq)))),
                }
            }
        }
    }

    by_name.sort_by_key(|e| std::cmp::Reverse(e.1));
    let total: usize = by_name.iter().map(|(_, c, _, _)| *c).sum();
    println!("  対照実験: **両端が外部** の pair での反証結果 (契約の真の scope)");
    println!("    検査した外部点: {checked_points}");
    println!(
        "    反例: {total} / 反例を出した construct: {}",
        by_name.len()
    );
    println!("    ⚠️ ratio = |f(p)-f(q)| / (L*|p-q|) が 1 を超えた分が破れ幅 (1 の直上なら discretization 由来を疑う)");
    for (name, count, ratio, ex) in by_name.iter().take(10) {
        println!("      {name}: {count} 点 最大 ratio {ratio:.3}");
        if let Some((p, fp, q, fq)) = ex {
            println!(
                "        p={p:?} f(p)={fp:.6} / q={q:?} f(q)={fq:.6} |p-q|={:.6}",
                p.distance(*q)
            );
        }
    }
    println!(
        "    ⇒ ここに出る construct は eval_lipschitz 側の不健全、\
         内部にだけ出る construct は契約が内部を除外している効果"
    );

    assert!(
        checked_points > 1000,
        "外部点を {checked_points} 点しか見ていない (対照群が空振りしている)"
    );
}

/// 早期判定が `MinThickness` の未決定をどれだけ潰しうるか (発火率の実測)
///
/// ⚠️ 本 test は数値を **print するだけ** で合否を決めない (閾値を先に決めずに
/// 測ると出た数字に基準を合わせることになる)
/// ただし **0 件なら仮説が即座に否定される**ので、その 1 点だけ assert する
#[test]
fn the_interior_bound_fires_on_some_interior_points() {
    let pts = grid_points();
    let all = nodes();

    let mut interior_total = 0usize;
    let mut fired = 0usize;
    let mut nodes_with_any_fire = 0usize;
    let mut nodes_all_interior_fired = 0usize;
    let mut nodes_with_interior = 0usize;

    for (_, node) in &all {
        let l = alice_sdf::interval::eval_lipschitz(node);
        if !l.is_finite() || l <= 0.0 {
            continue;
        }
        let (mut node_interior, mut node_fired) = (0usize, 0usize);
        for p in &pts {
            let fp = eval(node, *p);
            if fp >= 0.0 {
                continue;
            }
            node_interior += 1;
            // 早期 Clear の条件: 要求半径ぶん表面から離れていることが下界で言える
            if fp.abs() / l >= REQUIRED {
                node_fired += 1;
            }
        }
        interior_total += node_interior;
        fired += node_fired;
        if node_interior > 0 {
            nodes_with_interior += 1;
            if node_fired > 0 {
                nodes_with_any_fire += 1;
            }
            if node_fired == node_interior {
                nodes_all_interior_fired += 1;
            }
        }
    }

    let rate = if interior_total == 0 {
        0.0
    } else {
        fired as f32 / interior_total as f32 * 100.0
    };
    println!("  早期判定の発火率 (required = {REQUIRED})");
    println!("    内部点 {interior_total} 中 {fired} が発火 ({rate:.1}%)");
    println!(
        "    construct: 内部点あり {nodes_with_interior} / 1 点以上発火 {nodes_with_any_fire} / 全内部点が発火 {nodes_all_interior_fired}"
    );

    assert!(
        interior_total > 1000,
        "内部点を {interior_total} 点しか見ていない (grid / corpus が縮退している)"
    );
    assert!(
        fired > 0,
        "早期判定が 1 点でも発火しないなら仮説は否定される (|f(p)|/L >= {REQUIRED} が全滅)"
    );
}
