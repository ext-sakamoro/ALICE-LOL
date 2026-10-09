//! `print_export` の閉形式 oracle
//!
//! `src/print_export.rs` は 389 行で **`#[test]` が 0 本**、`alice-lol/tests/` の
//! 17 file のどれも本 module を呼んでいなかった (呼ぶのは `examples/` 3 本だけ =
//! `cargo test` では走らない) そこへ変異を 10 種入れたところ **10 種すべてが
//! 既存 suite を全 green で通過** した (2026-09-30 実測、lib 604 + integration 16 target)
//! 通過した変異: 全 facet の winding 反転 / mm 係数の 1/2 化 / `scale_mm == 1` の
//! 恒等 guard 反転 / `triangle_count` が頂点数を数える / `EmptyMesh` guard 削除 /
//! `resolution` 無視 / `bounds_min` と `bounds_max` の入れ替え / DC の resolution 半減 /
//! DC の mm 変換脱落 / `.3mf` に STL bytes を書く
//!
//! 本 file はそれらを**閉形式**で押さえる 期待値はすべて幾何の公式から来ており、
//! 実装関数を呼んで作った値は 1 つも無い:
//!
//! | 対象 | oracle の出所 |
//! |---|---|
//! | 符号付き体積 | 発散定理 `V = Σ a·(b×c)/6` と 球 `4πr³/3` / 直方体 `w·h·d` / トーラス `2π²Rr²` |
//! | 収束 | 内接多面体の誤差は解像度で単調減少 (resolution を 2 倍で相対誤差が半分以下) |
//! | 法線の向き | 幾何法線 `(b−a)×(c−a)` と SDF 勾配 `∇f` の内積 > 0 が全 facet |
//! | 水密性 | 有向 edge の対消滅 (境界 edge 0) と Euler 標数 `V−E+F = 2−2g` |
//! | 単位 | `scale_mm` は位置の一様スカラー倍 ⇒ 体積は厳密に `scale_mm³` 倍 |
//! | STL binary | `84 + 50n` byte の layout と facet 数、facet 法線と三角形幾何の符号一致 |
//! | 3MF | `unit="millimeter"` 宣言と実座標が同じ mm であること |
//! | 縮退 | 零面積三角形 0 枚、`NaN` / `Inf` 座標 0 個 |
//! | 引数の意味 | LOL DSL `box3d` は**半**寸法 / Rust `SdfNode::box3d` は**全**寸法 (逆) |
//!
//! 精度 parameter (`resolution`) を振って不変を assert する
//! (`rules/analytic-oracle-tests.md`「精度 parameter を振って不変を assert」)
//!
//! Author: Moroya Sakamoto

// 三角形数 / 解像度は高々 10^6 で f64 の仮数に収まる 比率を取るためだけの変換
#![allow(clippy::cast_precision_loss)]

use alice_lol::print_export::{
    lol_to_stl, node_to_3mf, node_to_3mf_dual_contouring, node_to_mesh,
    node_to_mesh_dual_contouring, node_to_stl, node_to_stl_dual_contouring, sdf_to_mesh,
    ExportError, MarchingCubesConfig, Mesh, PrintConfig,
};
use alice_lol::SdfNode;
use alice_sdf::eval::normal;
use glam::Vec3;
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::path::PathBuf;

// ── 測定器 (実装を呼ばない、mesh の生データだけから計算する) ──

/// 発散定理による符号付き体積 外向き CCW なら正
fn signed_volume(m: &Mesh) -> f64 {
    m.indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| {
            let a = m.vertices[t[0] as usize].position.as_dvec3();
            let b = m.vertices[t[1] as usize].position.as_dvec3();
            let c = m.vertices[t[2] as usize].position.as_dvec3();
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

/// 有向 edge の対消滅で数える境界 edge 閉じた多様体なら 0
fn open_edges(m: &Mesh) -> usize {
    let mut dir: HashMap<(u32, u32), i32> = HashMap::new();
    for t in m.indices.as_chunks::<3>().0 {
        for (u, v) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let key = if u < v { (u, v) } else { (v, u) };
            *dir.entry(key).or_insert(0) += if u < v { 1 } else { -1 };
        }
    }
    dir.values().filter(|&&c| c != 0).count()
}

/// ちょうど 2 枚に共有されていない edge の数 多様体なら 0
fn non_manifold_edges(m: &Mesh) -> usize {
    edge_multiplicity(m).values().filter(|&&c| c != 2).count()
}

fn edge_multiplicity(m: &Mesh) -> HashMap<(u32, u32), usize> {
    let mut cnt: HashMap<(u32, u32), usize> = HashMap::new();
    for t in m.indices.as_chunks::<3>().0 {
        for (u, v) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            let key = if u < v { (u, v) } else { (v, u) };
            *cnt.entry(key).or_insert(0) += 1;
        }
    }
    cnt
}

/// Euler 標数 `V − E + F` 孤立頂点は位相に寄与しないので参照されている頂点だけ数える
fn euler_characteristic(m: &Mesh) -> i64 {
    let v = i64::try_from(m.indices.iter().collect::<HashSet<_>>().len())
        .expect("頂点数が i64 に収まらない");
    let e = i64::try_from(edge_multiplicity(m).len()).expect("edge 数が i64 に収まらない");
    let f = i64::try_from(m.indices.len() / 3).expect("三角形数が i64 に収まらない");
    v - e + f
}

/// 幾何法線と SDF 勾配の内積の符号で外向き / 内向きを数える
///
/// `scale` で mm 座標を world に戻して勾配を取る
///
/// # ⚠️ 勾配が信頼できない facet は除外する (`skipped`)
///
/// 除外は 2 条件:
/// 1. セル面の 1e-3 未満の sliver (面積が小さすぎて向きが定まらない)
/// 2. ⚠️ **`|∇f|` が 1 から離れている点** — 距離場の勾配は大きさ 1 だが、
///    CSG の稜線 (`max(d_a, −d_b)` の折れ目) では有限差分がそれを満たさない
///
/// 2 を入れた理由 (2026-10-01 実測): `box3d(1.8³) − sphere(1.1)` を bounds ±2.0 /
/// res 32 で刻んだ**生 mesh** に対し、⚠️ **外向き判定の計器 3 つが互いに違う答えを
/// 出した** — 有限差分勾配 (eps 1e-3) は内向き **144 枚** / facet 法線方向の
/// SDF 値差 (δ = cell/4) は **96 枚** / 頂点法線は **264 枚**
/// 一方その mesh は **境界 edge 0** = 有向 edge が各向きにちょうど 1 回ずつ辿られる
/// = ⚠️ **winding は大域的に整合**しており、整合した閉曲面は「全部外向き」か
/// 「全部内向き」のどちらかにしかなりえない ⇒ **144 枚だけ内向きは原理的に不可能**で、
/// 3 計器すべてが稜線で誤っていたと判る (符号付き体積は正 = 全体として外向き)
///
/// ⇒ 稜線を除いた**信頼できる facet** で向きを assert し、大域的な向きは
/// 符号付き体積と境界 edge で別途 assert する (どちらも計器が壊れない量)
fn winding(node: &SdfNode, m: &Mesh, scale: f32, cell: f32) -> (usize, usize, usize) {
    // 距離場なら `|∇f| = 1` 稜線ではこれを外れるので、その facet は判定に使わない
    const GRAD_TOLERANCE: f32 = 0.05;
    let (mut outward, mut inward, mut skipped) = (0, 0, 0);
    let min_area = 1e-3 * cell * cell;
    for t in m.indices.as_chunks::<3>().0 {
        let a = m.vertices[t[0] as usize].position / scale;
        let b = m.vertices[t[1] as usize].position / scale;
        let c = m.vertices[t[2] as usize].position / scale;
        let geo = (b - a).cross(c - a);
        if 0.5 * geo.length() < min_area {
            skipped += 1;
            continue;
        }
        let grad = normal(node, (a + b + c) / 3.0, 1e-3);
        if (grad.length() - 1.0).abs() > GRAD_TOLERANCE {
            skipped += 1; // 稜線 — 勾配が距離場の性質を満たさない
            continue;
        }
        if geo.dot(grad) > 0.0 {
            outward += 1;
        } else {
            inward += 1;
        }
    }
    (outward, inward, skipped)
}

fn degenerate_triangles(m: &Mesh) -> usize {
    m.indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|t| {
            let a = m.vertices[t[0] as usize].position;
            let b = m.vertices[t[1] as usize].position;
            let c = m.vertices[t[2] as usize].position;
            (b - a).cross(c - a).length() == 0.0
        })
        .count()
}

/// 全頂点座標の絶対値の最大 (mm 単位の広がり)
fn max_abs_coord(m: &Mesh) -> f32 {
    m.vertices
        .iter()
        .fold(0.0f32, |acc, v| acc.max(v.position.abs().max_element()))
}

const fn cfg(resolution: usize, scale_mm: f32) -> PrintConfig {
    PrintConfig {
        resolution,
        bounds_min: Vec3::splat(-2.0),
        bounds_max: Vec3::splat(2.0),
        scale_mm,
    }
}

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("alice_lol_oracle_{}_{name}", std::process::id()));
    p
}

// ── STL binary の読み戻し (書き手の実装を呼ばず、仕様通りに byte を読む) ──

struct StlFacet {
    normal: Vec3,
    v: [Vec3; 3],
}

/// 80 byte header + u32 facet 数 + facet ごとに 50 byte (法線 3f32 + 頂点 9f32 + u16)
fn read_stl(path: &std::path::Path) -> (u32, Vec<StlFacet>, usize) {
    let bytes = std::fs::read(path).expect("STL を読めない");
    assert!(
        bytes.len() >= 84,
        "STL が header より短い: {} byte",
        bytes.len()
    );
    let declared = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]);
    let f32_at = |off: usize| {
        f32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
    };
    let v3_at = |off: usize| Vec3::new(f32_at(off), f32_at(off + 4), f32_at(off + 8));
    let mut facets = Vec::new();
    let mut off = 84;
    while off + 50 <= bytes.len() {
        facets.push(StlFacet {
            normal: v3_at(off),
            v: [v3_at(off + 12), v3_at(off + 24), v3_at(off + 36)],
        });
        off += 50;
    }
    (declared, facets, bytes.len())
}

/// 3MF は STORE (無圧縮) zip なので model XML が byte 列にそのまま現れる
fn read_3mf_model_xml(path: &std::path::Path) -> String {
    let bytes = std::fs::read(path).expect("3MF を読めない");
    let start = find(&bytes, b"<?xml").expect("3MF 内に XML が無い");
    let end = find(&bytes, b"</model>").expect("3MF 内に </model> が無い") + b"</model>".len();
    // [Content_Types].xml が先に来るので model 側を探し直す
    let model_start = find(&bytes, b"<model ").expect("3MF 内に <model> が無い");
    let head = if model_start > start {
        model_start
    } else {
        start
    };
    String::from_utf8_lossy(&bytes[head..end]).into_owned()
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

// ── 体積: 閉形式との突合 + 解像度収束 ──

/// 球 `V = 4πr³/3` 単発の誤差率でなく **resolution 2 倍で誤差が半分以下**まで見る
/// (内接多面体なので誤差は単調減少しなければならない)
#[test]
fn signed_volume_converges_to_the_sphere_closed_form() {
    let node = SdfNode::sphere(1.0);
    let truth = 4.0 / 3.0 * PI;
    let mut prev: Option<f64> = None;
    for res in [16usize, 32, 64, 128] {
        let v = signed_volume(&node_to_mesh(&node, &cfg(res, 1.0)));
        assert!(
            v > 0.0,
            "res {res}: 符号付き体積 {v} は正でなければならない (外向き CCW)"
        );
        let rel = (v - truth).abs() / truth;
        assert!(
            rel < 0.05,
            "res {res}: 体積 {v} vs 解析解 {truth} (相対誤差 {rel:.4})"
        );
        if let Some(p) = prev {
            assert!(
                rel < p * 0.5,
                "res {res}: 相対誤差 {rel:.5} が前段 {p:.5} の半分以下に縮まっていない (収束していない)"
            );
        }
        prev = Some(rel);
    }
}

/// 直方体 `V = w·h·d` ⚠️ Rust の `SdfNode::box3d(w,h,d)` は **全**寸法を取り
/// 内部で半分にする (`half_extents = w/2`) LOL DSL の `box3d` とは逆
#[test]
fn signed_volume_converges_to_the_box_closed_form() {
    let node = SdfNode::box3d(1.0, 1.2, 1.4);
    let truth = 1.0 * 1.2 * 1.4;
    let mut prev: Option<f64> = None;
    for res in [32usize, 64, 128] {
        let v = signed_volume(&node_to_mesh(&node, &cfg(res, 1.0)));
        let rel = (v - truth).abs() / truth;
        assert!(v > 0.0, "res {res}: 体積 {v} は正でなければならない");
        assert!(
            rel < 0.05,
            "res {res}: 体積 {v} vs 解析解 {truth} (相対誤差 {rel:.4})"
        );
        if let Some(p) = prev {
            assert!(
                rel < p * 0.5,
                "res {res}: 誤差 {rel:.5} が前段 {p:.5} の半分以下に縮まっていない"
            );
        }
        prev = Some(rel);
    }
}

/// トーラス `V = 2π²Rr²` (Pappus の定理)
#[test]
fn signed_volume_converges_to_the_torus_closed_form() {
    let (big, small) = (0.8f32, 0.3f32);
    let node = SdfNode::torus(big, small);
    let truth = 2.0 * PI * PI * f64::from(big) * f64::from(small) * f64::from(small);
    let mut prev: Option<f64> = None;
    for res in [32usize, 64] {
        let v = signed_volume(&node_to_mesh(&node, &cfg(res, 1.0)));
        let rel = (v - truth).abs() / truth;
        assert!(
            rel < 0.05,
            "res {res}: トーラス体積 {v} vs 解析解 {truth} (相対誤差 {rel:.4})"
        );
        if let Some(p) = prev {
            assert!(
                rel < p * 0.5,
                "res {res}: 誤差 {rel:.5} が前段 {p:.5} の半分以下に縮まっていない"
            );
        }
        prev = Some(rel);
    }
}

/// `resolution` が実際に使われていること 表面を三角形で覆うので
/// resolution 2 倍で三角形数はおよそ 4 倍になる (面積スケーリング)
#[test]
fn resolution_is_honoured_not_ignored() {
    let node = SdfNode::sphere(1.0);
    let mut prev: Option<usize> = None;
    for res in [16usize, 32, 64] {
        let n = node_to_mesh(&node, &cfg(res, 1.0)).indices.len() / 3;
        if let Some(p) = prev {
            assert!(
                n as f64 > 3.0 * p as f64,
                "res {res}: 三角形数 {n} が前段 {p} の 3 倍を超えない (resolution が反映されていない)"
            );
        }
        prev = Some(n);
    }
}

// ── 単位: mm スケーリングの閉形式 ──

/// `scale_mm` は位置の一様スカラー倍なので、体積は厳密に `scale_mm³` 倍になる
/// (f32 丸めの範囲でのみずれる)
#[test]
fn scale_mm_scales_the_volume_exactly_by_its_cube() {
    let node = SdfNode::sphere(1.0);
    let base = signed_volume(&node_to_mesh(&node, &cfg(32, 1.0)));
    assert!(base > 0.0, "scale_mm = 1 の体積 {base} が正でない");
    for s in [2.0f32, 10.0, 25.4] {
        let v = signed_volume(&node_to_mesh(&node, &cfg(32, s)));
        let cube = f64::from(s).powi(3);
        let rel = (v / base / cube - 1.0).abs();
        assert!(
            rel < 1e-5,
            "scale_mm = {s}: V(s)/V(1) = {:.6} は s³ = {cube:.6} と一致しなければならない (相対差 {rel:.3e})",
            v / base
        );
    }
}

/// `scale_mm = 1.0` は恒等 半径 1 の球は world 1.0 のまま、
/// `scale_mm = 10.0` なら 10 mm になる (bbox で単位を直接読む)
#[test]
fn scale_mm_maps_unit_world_radius_onto_millimetres() {
    let node = SdfNode::sphere(1.0);
    for s in [1.0f32, 10.0, 25.4] {
        let m = node_to_mesh(&node, &cfg(64, s));
        let reach = max_abs_coord(&m);
        // 半径 1 の球の最遠点は軸上の 1.0 world = s mm セル幅 (4/64 world) 分の離散化誤差を許容
        let tol = s * 4.0 / 64.0;
        assert!(
            (reach - s).abs() < tol,
            "scale_mm = {s}: 最遠座標 {reach} mm は半径 {s} mm でなければならない (許容 {tol})"
        );
    }
}

/// 既定 config (`PrintConfig::default()` = res 128 / `scale_mm` 10) で
/// 半径 1 の球が 4πr³/3 · 10³ mm³ になること 「一番普通の使い方」を通す
#[test]
fn the_default_config_produces_the_analytic_volume_in_cubic_millimetres() {
    let node = SdfNode::sphere(1.0);
    let m = node_to_mesh(&node, &PrintConfig::default());
    let truth = 4.0 / 3.0 * PI * 1000.0;
    let v = signed_volume(&m);
    let rel = (v - truth).abs() / truth;
    assert!(
        rel < 0.01,
        "既定 config: 体積 {v} mm³ vs 解析解 {truth} mm³ (相対誤差 {rel:.4})"
    );
}

// ── 向き: 全 facet が外向き ──

/// CSG の稜線を持つ形状で一部の facet が内向きに測られる既知欠陥
///
/// ⚠️ **この pin が落ちたら上流が直った合図**なので、`box - sphere` の分岐を消して
/// 他の形状と同じ `inward == 0` の strict assert に寄せること
const KNOWN_BROKEN_CSG_RIDGE_WINDING: &str =
    "`box3d(1.8³) − sphere(1.1)` (= 6 面に穴が開く genus 5) を \
bounds ±2.0 / res 32 で刻むと、幾何法線と SDF 勾配の内積が負の facet が 144/3136 枚出る \
⚠️ この mesh は 境界 edge 0 (winding は大域的に整合) かつ 符号付き体積 > 0 (全体として外向き) かつ \
χ = −8 (genus 5 で正しい) なので、**位相と大域的な向きはすべて正しい** \
⚠️ 外向き判定の計器 3 つが互いに違う答えを出す: 有限差分勾配 (eps 1e-3) 144 枚 / \
facet 法線方向の SDF 値差 (δ = cell/4) 96 枚 / marching cubes の頂点法線 264 枚 \
さらに `|∇f|` は 1 から 5% 以内に収まっているので「稜線だから勾配が壊れている」でも説明できない \
原因は上流 `alice_sdf` の marching cubes が格子と非整合な bounds で CSG の稜線を刻む時の \
頂点配置と推定 (ALICE-SDF `tests/test_mesh_orientation.rs` は bounds ±1.6 / res 32 で \
箱の面を格子面に載せているため、この欠陥を検出できない) \
3D プリント上は slicer が winding を自前で補正するため実害は限定的で、\
水密性 (境界 edge 0 / 非多様体 edge 0 / χ) は満たしている";

/// facet が外を向いている — 3 段で押さえる
///
/// 1. **大域的な向き**: 符号付き体積 > 0 (発散定理、計器が壊れない量)
/// 2. **winding の整合**: 境界 edge 0 = 全 edge が各向きにちょうど 1 回辿られる
///    ⚠️ 1 と 2 が揃えば「整合した閉曲面が外向き」= **全 facet が外向き**が従う
/// 3. **局所判定**: 勾配が信頼できる facet (`|∇f| ≈ 1`) で内向き 0
///    ⚠️ 稜線は除外するので、⚠️ **除外が多すぎて空振りしていないこと**も assert する
///
/// 1 枚でも内向きなら slicer は中身を裏返しに解釈する
#[test]
fn every_facet_faces_outward_against_the_sdf_gradient() {
    for (name, node) in [
        ("sphere", SdfNode::sphere(1.0)),
        ("box3d", SdfNode::box3d(1.4, 1.0, 1.8)),
        ("torus", SdfNode::torus(0.8, 0.3)),
        (
            "box - sphere",
            SdfNode::box3d(1.8, 1.8, 1.8).subtract(SdfNode::sphere(1.1)),
        ),
    ] {
        for res in [32usize, 64] {
            for scale in [1.0f32, 10.0] {
                let m = node_to_mesh(&node, &cfg(res, scale));
                // 1. 大域的な向き — 発散定理 (座標が mm 倍されても符号は不変)
                assert!(
                    signed_volume(&m) > 0.0,
                    "{name} res {res} scale {scale}: 符号付き体積が正でない = 全体が裏返っている"
                );
                // 2. winding の整合 — これと 1 で「全 facet が外向き」が従う
                assert_eq!(
                    open_edges(&m),
                    0,
                    "{name} res {res} scale {scale}: 有向 edge が対にならない = winding が不整合"
                );
                // 3. 局所判定 — 滑らかな形状は全枚外向き
                let (outward, inward, skipped) = winding(&node, &m, scale, 4.0 / res as f32);
                let total = outward + inward + skipped;
                if name == "box - sphere" {
                    // ⚠️ CSG の稜線を持つ形状は上流の既知欠陥で一部が内向きに出る
                    //    (下記 KNOWN_BROKEN_CSG_RIDGE_WINDING、直ると red になる)
                    assert!(
                        inward > 0,
                        "{name} res {res} scale {scale}: 内向き 0 になった\n{KNOWN_BROKEN_CSG_RIDGE_WINDING}"
                    );
                    assert!(
                        inward * 10 < total,
                        "{name} res {res} scale {scale}: 内向き {inward}/{total} が 10% を超えた (欠陥が広がった)"
                    );
                } else {
                    assert!(
                        inward == 0 && outward > 0,
                        "{name} res {res} scale {scale}: 外向き {outward} / 内向き {inward} / 除外 {skipped} (滑らかな形状は全枚外向きでなければならない)"
                    );
                    // ⚠️ 空振り防止 — 除外が多すぎると 3 の assert が意味を失う
                    assert!(
                        outward * 10 >= total * 9,
                        "{name} res {res} scale {scale}: 判定できた facet が {outward}/{total} しかない (除外 {skipped}) = 局所判定が空振りしている"
                    );
                }
            }
        }
    }
}

// ── 水密性と位相 ──

/// Euler 標数が位相と一致すること 球 `χ=2` / トーラス `χ=0` (genus 1) /
/// 半径 1.1 の球で 1.8 角の立方体を 6 面貫通して抜くと 6 穴 = genus 5 で `χ=−8`
#[test]
fn euler_characteristic_matches_the_topological_genus() {
    for (name, node, chi) in [
        ("sphere (genus 0)", SdfNode::sphere(1.0), 2i64),
        ("torus (genus 1)", SdfNode::torus(0.8, 0.3), 0),
        (
            "box - sphere (genus 5)",
            SdfNode::box3d(1.8, 1.8, 1.8).subtract(SdfNode::sphere(1.1)),
            -8,
        ),
    ] {
        let m = node_to_mesh(&node, &cfg(32, 1.0));
        assert_eq!(
            euler_characteristic(&m),
            chi,
            "{name}: Euler 標数が {chi} でない (位相が壊れている)"
        );
    }
}

/// `node_to_mesh` 内の `MeshRepair::repair_all(&mesh, 5e-3)` (`print_export.rs:175`) が
/// 現に水密性を壊していることの pin `law_corpus_oracle.rs` の `KNOWN_UNSOUND` と
/// 同じ構造で、⚠️ **`repair_all` を直すとこの test が red になる** ので直した人が必ず気付く
/// MC 経路の export は全解像度で水密 (境界 edge 0 / 非多様体 edge 0 / `χ = 2`)
///
/// # ⚠️ この test は「修復が壊す」ことを pin した test の後継
///
/// 2026-09-30 時点の `print_export.rs` は `MeshRepair::repair_all(&mesh, 5e-3)` を
/// 無条件に掛けており、⚠️ **水密な生 mesh を非水密にしていた**
/// (res 64 で境界 edge 111 / 既定 res 128 で 741 + 非多様体 285 + `χ = −86` /
/// res 256 で 3561、⚠️ **測った全 case で「修復が変えたなら必ず悪化」**)
///
/// 2026-10-01 の修正で `node_to_mesh` は非回帰になった:
/// - 位相が健全な mesh には破壊的操作を**掛けない**
/// - 掛けた結果が悪化したら**採らない**
/// - 頂点マージ許容量を cell 幅相対にした (旧 `5e-3` は res 128 で cell の 16%)
///
/// ⚠️ **生 mesh が水密であることも併せて確認する** — それが崩れたら破れの原因が
/// 後処理でなく上流 (`alice_sdf` の marching cubes) に移ったことになるので、
/// この test は「どちらが壊れたか」を切り分けられる形にしてある
#[test]
fn the_marching_cubes_export_is_watertight_at_every_resolution() {
    let node = SdfNode::sphere(1.0);
    let mc = |res: usize| MarchingCubesConfig {
        resolution: res,
        compute_normals: true,
        ..MarchingCubesConfig::default()
    };

    for res in [32usize, 64, 128, 192, 256] {
        // 上流 (alice-sdf の marching cubes) が閉じていることを先に確認する
        let raw = sdf_to_mesh(&node, Vec3::splat(-2.0), Vec3::splat(2.0), &mc(res));
        assert_eq!(
            open_edges(&raw),
            0,
            "res {res}: 生の sdf_to_mesh に境界 edge がある (破れの原因が上流に移った)"
        );
        assert_eq!(
            non_manifold_edges(&raw),
            0,
            "res {res}: 生の sdf_to_mesh に非多様体 edge がある (同上)"
        );
        assert_eq!(
            euler_characteristic(&raw),
            2,
            "res {res}: 生の sdf_to_mesh の χ が 2 でない (同上)"
        );

        // export 経路 (後処理込み) が水密性を保っていること
        let m = node_to_mesh(&node, &cfg(res, 1.0));
        assert_eq!(
            open_edges(&m),
            0,
            "res {res}: export 経路が境界 edge を作った (後処理が水密性を壊している)"
        );
        assert_eq!(
            non_manifold_edges(&m),
            0,
            "res {res}: export 経路が非多様体 edge を作った"
        );
        assert_eq!(
            euler_characteristic(&m),
            2,
            "res {res}: export 経路の χ が 2 でない (球の位相が壊れた)"
        );
    }

    // 既定 config (res 128) と high_quality (res 256) も同じ invariant を満たす
    for (name, config) in [
        ("default", PrintConfig::default()),
        ("high_quality", PrintConfig::high_quality()),
    ] {
        let m = node_to_mesh(&node, &config);
        assert_eq!(open_edges(&m), 0, "{name}: 境界 edge がある");
        assert_eq!(non_manifold_edges(&m), 0, "{name}: 非多様体 edge がある");
        assert_eq!(euler_characteristic(&m), 2, "{name}: 球の χ が 2 でない");
    }
}

/// 零面積三角形 0 枚、`NaN` / `Inf` 座標 0 個 どちらも slicer が拒否する
#[test]
fn no_degenerate_triangles_and_no_non_finite_coordinates() {
    for (name, node) in [
        ("sphere", SdfNode::sphere(1.0)),
        ("torus", SdfNode::torus(0.8, 0.3)),
    ] {
        let m = node_to_mesh(&node, &PrintConfig::default());
        assert_eq!(degenerate_triangles(&m), 0, "{name}: 零面積三角形がある");
        assert_eq!(
            m.vertices
                .iter()
                .filter(|v| !v.position.is_finite())
                .count(),
            0,
            "{name}: NaN / Inf 座標がある"
        );
    }
}

/// mesh は必ず config の bounds (× `scale_mm`) の内側に収まる
/// `bounds_min` と `bounds_max` を入れ違えたら成立しない
#[test]
fn the_mesh_stays_inside_the_configured_bounds() {
    let node = SdfNode::sphere(1.0);
    let c = cfg(64, 10.0);
    let m = node_to_mesh(&node, &c);
    assert!(
        !m.vertices.is_empty(),
        "mesh が空 (bounds の指定が効いていない)"
    );
    for v in &m.vertices {
        let p = v.position;
        assert!(
            p.cmpge(c.bounds_min * c.scale_mm).all() && p.cmple(c.bounds_max * c.scale_mm).all(),
            "頂点 {p:?} が bounds [{:?}, {:?}] mm の外",
            c.bounds_min * c.scale_mm,
            c.bounds_max * c.scale_mm
        );
    }
}

// ── STL / 3MF の file format ──

/// STL binary の layout は `84 + 50n` byte で、宣言 facet 数 = 実 facet 数 =
/// `ExportStats::triangle_count` の 3 つが一致すること
#[test]
fn exported_stl_binary_layout_matches_the_reported_facet_count() {
    let path = tmp("layout.stl");
    let stats =
        node_to_stl(&SdfNode::sphere(1.0), &path, &cfg(32, 10.0)).expect("STL 書き出し失敗");
    let (declared, facets, len) = read_stl(&path);
    std::fs::remove_file(&path).ok();

    assert_eq!(
        len,
        84 + 50 * facets.len(),
        "STL の長さ {len} が 84 + 50×{} と一致しない",
        facets.len()
    );
    assert_eq!(
        declared as usize,
        facets.len(),
        "header の facet 数 {declared} と実 facet 数 {} が一致しない",
        facets.len()
    );
    assert_eq!(
        stats.triangle_count,
        facets.len(),
        "ExportStats.triangle_count {} が実際に書かれた facet 数 {} と一致しない",
        stats.triangle_count,
        facets.len()
    );
    assert!(
        facets.len() > 100,
        "facet が {} 枚しかない (res 32 の球としては少なすぎる)",
        facets.len()
    );
    assert_eq!(
        stats.path,
        path.display().to_string(),
        "ExportStats.path が渡した path と違う"
    );
    for (i, f) in facets.iter().enumerate() {
        assert!(
            f.normal.is_finite() && f.v.iter().all(|p| p.is_finite()),
            "facet {i}: NaN / Inf が書かれている"
        );
    }
}

/// STL の facet 法線 (頂点法線の平均 = SDF 勾配) と三角形の幾何法線が同じ向きであること
/// 向きが逆なら slicer は facet 法線を信じて中身を裏返す
#[test]
fn exported_stl_facet_normals_agree_with_the_triangle_geometry() {
    let path = tmp("normals.stl");
    node_to_stl(&SdfNode::sphere(1.0), &path, &cfg(32, 10.0)).expect("STL 書き出し失敗");
    let (_, facets, _) = read_stl(&path);
    std::fs::remove_file(&path).ok();

    let mut disagreeing = 0;
    let mut compared = 0;
    for f in &facets {
        let geo = (f.v[1] - f.v[0]).cross(f.v[2] - f.v[0]);
        if geo.length() < 1e-6 || f.normal.length() < 1e-6 {
            continue;
        }
        compared += 1;
        if geo.dot(f.normal) <= 0.0 {
            disagreeing += 1;
        }
    }
    assert!(
        compared > 100,
        "比較できた facet が {compared} 枚しかない (空振り)"
    );
    assert_eq!(
        disagreeing, 0,
        "{disagreeing}/{compared} 枚の facet 法線が三角形の幾何と逆向き"
    );
}

/// 3MF は `unit="millimeter"` を宣言し、実座標も同じ mm であること
/// 半径 1 world の球を `scale_mm = 10` で出せば座標は ±10 付近に来る
#[test]
fn exported_3mf_declares_millimetres_and_the_coordinates_are_millimetres() {
    let path = tmp("units.3mf");
    let stats =
        node_to_3mf(&SdfNode::sphere(1.0), &path, &cfg(32, 10.0)).expect("3MF 書き出し失敗");
    let xml = read_3mf_model_xml(&path);
    std::fs::remove_file(&path).ok();

    assert!(
        xml.contains(r#"unit="millimeter""#),
        "3MF が mm 単位を宣言していない: {}",
        &xml[..xml.len().min(200)]
    );
    assert_eq!(
        xml.matches("<vertex ").count(),
        stats.vertex_count,
        "3MF の <vertex> 数が ExportStats.vertex_count {} と一致しない",
        stats.vertex_count
    );
    assert_eq!(
        xml.matches("<triangle ").count(),
        stats.triangle_count,
        "3MF の <triangle> 数が ExportStats.triangle_count {} と一致しない",
        stats.triangle_count
    );

    // x 属性を読んで mm の広がりを直接測る (宣言と実座標の突合)
    let mut reach = 0.0f32;
    for chunk in xml.split(r#"<vertex x=""#).skip(1) {
        let x: f32 = chunk[..chunk.find('"').expect("x 属性が閉じていない")]
            .parse()
            .expect("x が数値でない");
        reach = reach.max(x.abs());
    }
    assert!(
        (reach - 10.0).abs() < 10.0 * 4.0 / 32.0,
        "3MF の最大 |x| は {reach} mm だが、半径 1 world × scale_mm 10 なら 10 mm でなければならない"
    );
}

/// ジオメトリが bounds 内に無い時は `EmptyMesh` で落ちること
/// 0 facet の file を Ok で返すと、下流は「出力できた」と読む
#[test]
fn empty_geometry_is_an_error_not_an_empty_file() {
    let far = SdfNode::sphere(0.2).translate(50.0, 0.0, 0.0);
    for (kind, res) in [("stl", 0), ("3mf", 1)] {
        let path = tmp(&format!("empty.{kind}"));
        let r = if res == 0 {
            node_to_stl(&far, &path, &PrintConfig::preview())
        } else {
            node_to_3mf(&far, &path, &PrintConfig::preview())
        };
        let existed = path.exists();
        std::fs::remove_file(&path).ok();
        assert!(
            matches!(r, Err(ExportError::EmptyMesh)),
            "{kind}: bounds 外のジオメトリが EmptyMesh にならない (結果 {:?}、file 生成 {existed})",
            r.map(|s| s.triangle_count)
        );
    }
}

// ── LOL テキスト経路 ──

/// LOL テキストからの経路も同じ解析解に載ること
/// ⚠️ **LOL DSL の `box3d(hx,hy,hz)` は半寸法** (`runtime_parser.rs:612` が
/// `half_extents` に直入れ) なので体積は `8·hx·hy·hz` Rust の
/// `SdfNode::box3d(w,h,d)` は全寸法で `w·h·d` = **2 つの API で逆**
#[test]
fn the_lol_text_path_hits_the_same_closed_form() {
    let path = tmp("text.stl");
    let stats = lol_to_stl("sphere(1.0)", &path, &cfg(32, 1.0)).expect("LOL → STL 失敗");
    let (_, facets, _) = read_stl(&path);
    std::fs::remove_file(&path).ok();
    let v: f64 = facets
        .iter()
        .map(|f| {
            f.v[0]
                .as_dvec3()
                .dot(f.v[1].as_dvec3().cross(f.v[2].as_dvec3()))
                / 6.0
        })
        .sum();
    let truth = 4.0 / 3.0 * PI;
    let rel = (v - truth).abs() / truth;
    assert!(
        rel < 0.05,
        "LOL テキスト経路: 体積 {v} vs 4π/3 = {truth} (相対誤差 {rel:.4})"
    );
    assert_eq!(
        stats.triangle_count,
        facets.len(),
        "stats と file の facet 数が不一致"
    );

    // 半寸法 convention: box3d(0.5, 0.6, 0.7) の体積は 8·0.5·0.6·0.7 = 1.68
    let path = tmp("text_box.stl");
    lol_to_stl("box3d(0.5, 0.6, 0.7)", &path, &cfg(128, 1.0)).expect("LOL → STL 失敗");
    let (_, facets, _) = read_stl(&path);
    std::fs::remove_file(&path).ok();
    let v: f64 = facets
        .iter()
        .map(|f| {
            f.v[0]
                .as_dvec3()
                .dot(f.v[1].as_dvec3().cross(f.v[2].as_dvec3()))
                / 6.0
        })
        .sum();
    let truth = 8.0 * 0.5 * 0.6 * 0.7;
    let rel = (v - truth).abs() / truth;
    assert!(
        rel < 0.01,
        "LOL `box3d` は半寸法なので体積は 8·hx·hy·hz = {truth} のはず (実測 {v}、相対誤差 {rel:.4})"
    );
}

// ── Dual Contouring 経路 (module doc の「watertight 保証」の検証) ──

/// DC も同じ解析解に収束すること 解像度を半減させたら誤差は約 4 倍になるので
/// 各段の上限で `resolution` が素通しされていることまで見る
#[test]
fn dual_contouring_volume_converges_to_the_sphere_closed_form() {
    let node = SdfNode::sphere(1.0);
    let truth = 4.0 / 3.0 * PI;
    // 実測 (2026-09-30): res 16 で 1.4e-2 / 32 で 3.7e-3 / 64 で 8.9e-4 ≈ 2 次収束
    for (res, bound) in [(16usize, 0.02f64), (32, 0.005), (64, 0.0015)] {
        let v = signed_volume(&node_to_mesh_dual_contouring(&node, &cfg(res, 1.0)));
        assert!(v > 0.0, "DC res {res}: 体積 {v} が正でない");
        let rel = (v - truth).abs() / truth;
        assert!(
            rel < bound,
            "DC res {res}: 体積 {v} vs 解析解 {truth} (相対誤差 {rel:.5} が上限 {bound} を超えた)"
        );
    }
}

/// **DC 経路は repair を通らないので、ここでは水密性の invariant が真に検査される**
/// (MC 経路の `known_broken_*` が pin なのに対し、本 test は本物の invariant)
/// module doc の「DC は全 resolution で `non_manifold_edges = 0`」の主張を、
/// 有向 edge の対消滅 (各 edge がちょうど 2 枚に、互いに逆向きで共有される) と
/// Euler 標数 `χ = 2 − 2g` の両方で検査する
/// 2026-09-30 実測: 球 res 32/64/128 / トーラス / `box − sphere` で全部成立
#[test]
fn dual_contouring_output_is_watertight_at_every_resolution() {
    for (name, node, chi) in [
        ("sphere (genus 0)", SdfNode::sphere(1.0), 2i64),
        ("torus (genus 1)", SdfNode::torus(0.8, 0.3), 0),
        (
            "box - sphere (genus 5)",
            SdfNode::box3d(1.8, 1.8, 1.8).subtract(SdfNode::sphere(1.1)),
            -8,
        ),
    ] {
        for res in [32usize, 64, 128] {
            let m = node_to_mesh_dual_contouring(&node, &cfg(res, 1.0));
            assert!(
                !m.indices.is_empty(),
                "DC {name} res {res}: mesh が空 (空振り)"
            );
            assert_eq!(
                open_edges(&m),
                0,
                "DC {name} res {res}: 境界 edge が 0 でない"
            );
            assert_eq!(
                non_manifold_edges(&m),
                0,
                "DC {name} res {res}: ちょうど 2 枚に共有されていない edge がある"
            );
            assert_eq!(
                euler_characteristic(&m),
                chi,
                "DC {name} res {res}: Euler 標数が {chi} でない (位相が壊れている)"
            );
        }
    }
}

/// DC 経路も MC と同じ mm スケーリングを掛けること
/// (片方だけ掛け忘れると同じ形が経路によって 10 倍違う寸法で出る)
#[test]
fn dual_contouring_applies_the_same_millimetre_scaling() {
    let node = SdfNode::sphere(1.0);
    for s in [1.0f32, 10.0, 25.4] {
        let m = node_to_mesh_dual_contouring(&node, &cfg(64, s));
        let reach = max_abs_coord(&m);
        let tol = s * 4.0 / 64.0;
        assert!(
            (reach - s).abs() < tol,
            "DC scale_mm = {s}: 最遠座標 {reach} mm は {s} mm でなければならない (許容 {tol})"
        );
    }
}

/// DC 経路の export 2 本も format と facet 数が整合すること
#[test]
fn dual_contouring_export_files_are_consistent() {
    let node = SdfNode::sphere(1.0);
    let stl = tmp("dc.stl");
    let stats = node_to_stl_dual_contouring(&node, &stl, &cfg(32, 10.0)).expect("DC STL 失敗");
    let (declared, facets, len) = read_stl(&stl);
    std::fs::remove_file(&stl).ok();
    assert_eq!(
        len,
        84 + 50 * facets.len(),
        "DC STL の長さが layout と不整合"
    );
    assert_eq!(
        declared as usize, stats.triangle_count,
        "DC STL の facet 数が stats と不一致"
    );

    let mf = tmp("dc.3mf");
    let stats = node_to_3mf_dual_contouring(&node, &mf, &cfg(32, 10.0)).expect("DC 3MF 失敗");
    let xml = read_3mf_model_xml(&mf);
    std::fs::remove_file(&mf).ok();
    assert!(
        xml.contains(r#"unit="millimeter""#),
        "DC 3MF が mm 単位を宣言していない"
    );
    assert_eq!(
        xml.matches("<triangle ").count(),
        stats.triangle_count,
        "DC 3MF の <triangle> 数が stats と不一致"
    );
}

/// 同位置の頂点の統合は「位置を bit で一致させた分だけ」畳む
///
/// ⚠️ **既存の「零面積三角形 0」だけでは実装を区別できない** — 距離や量子化で
/// 溶接しても零面積は 0 になるが、別の位置の頂点まで巻き込んで非多様体 edge が
/// 出る (上流が旧方式で報告した形) 本 test は上流の閉形式
/// `落とした三角形 = 2 × 畳んだ頂点` を LOL の export 経路の出力で突き合わせ、
/// **畳んだ数そのもの**を固定する
///
/// 歯の範囲 (2026-10-09 に量子化へ差し替える変異で実測、res 128 / cell 0.03125):
/// 刻み **0.00125** (= `VERTEX_MERGE_CELL_RATIO * cell`、この repo が破壊的修復で
/// 使う許容量) / 0.003125 / 0.005 / 0.01 / 0.02 / 0.05 はすべて red
/// ⚠️ 刻み **0.0001** (cell の 1/312) だけは green — この scene では隣接頂点を
/// 1 つも巻き込まないので bit 一致と出力が同じ = 等価変異であって歯の欠落ではない
#[test]
fn coincident_vertices_are_welded_by_exact_position_only() {
    // 生 mesh (修復前) を同じ config で取り直して、統合の前後を数える
    let node = SdfNode::torus(0.8, 0.3);
    // ⚠️ `scale_mm` は修復の **後** に掛かるので、既定 (10.0) のままだと位置の bit が
    //    一致しない 統合そのものを見たいので 1.0 にする (他の性質は scale 非依存)
    let c = PrintConfig {
        scale_mm: 1.0,
        ..PrintConfig::default()
    };
    let raw = alice_sdf::mesh::sdf_to_mesh(
        &node,
        c.bounds_min,
        c.bounds_max,
        &alice_sdf::mesh::MarchingCubesConfig {
            resolution: c.resolution,
            compute_normals: true,
            ..alice_sdf::mesh::MarchingCubesConfig::default()
        },
    );
    let exported = node_to_mesh(&node, &c);

    // 位置の相異なる個数 (bit で数える) は統合で変わらない
    let positions = |m: &Mesh| -> HashSet<[u32; 3]> {
        m.vertices
            .iter()
            .map(|v| v.position.to_array().map(f32::to_bits))
            .collect()
    };
    let raw_distinct = positions(&raw);
    assert_eq!(
        raw_distinct.len(),
        exported.vertices.len(),
        "統合後の頂点数が「生 mesh の相異なる位置の数」と違う \
         (多ければ畳み漏れ、少なければ別の位置まで巻き込んでいる)"
    );
    assert_eq!(
        raw_distinct,
        positions(&exported),
        "統合で位置の集合が変わった (頂点を動かしてはいけない)"
    );

    // 上流の閉形式: 畳んだ頂点 1 つにつき三角形が 2 枚潰れる
    let welded = raw.vertices.len() - exported.vertices.len();
    assert!(
        welded > 0,
        "この scene では頂点が 1 つも重なっていない — 閉形式を検査できていない \
         (上流の marching cubes が変わったか bounds/res が変わった)"
    );
    assert_eq!(
        raw.indices.len() / 3 - exported.indices.len() / 3,
        2 * welded,
        "落とした三角形が 2 × 畳んだ頂点 と一致しない"
    );

    // 畳んだ結果も閉じている (torus なので χ = 0)
    assert_eq!(open_edges(&exported), 0, "統合後に境界 edge がある");
    assert_eq!(
        non_manifold_edges(&exported),
        0,
        "統合後に非多様体 edge がある"
    );
    assert_eq!(
        euler_characteristic(&exported),
        0,
        "torus の χ が 0 でない (統合で位相が変わった)"
    );
}
