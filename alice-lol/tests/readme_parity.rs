//! README の「2 つの入口」の code example が実際に一致することを検査する
//!
//! ALICE-SDF / ALICE-LOL の README は「同じ形を builder API と LOL DSL で書くと
//! こうなる」を対で載せている doc の例が腐ると **入口の説明が嘘になる** ので、
//! 例そのものを test にして固定する
//!
//! 併せて、両者で**引数の意味が違う**唯一の箇所を pin する: `SdfNode::box3d` は
//! **全長**を取り (内部で半分にする)、LOL DSL の `box3d` は **半幅**を取る
//! (DSL は variant の field を直接書く) README はこの差を注記しているので、
//! 注記が実装と合っていることをここで確かめる
//!
//! 起票: 2026-09-28 (ALICE-SDF の README から LOL に辿り着けるようにする)

// 厳密比較が **契約そのもの**: builder API と DSL はどちらも同じ `SdfNode` tree を
// 作り、同じ `alice_sdf::eval` を通るので、場は bit 単位で一致しなければならない
// (許容を入れると「同じ tree に parse される」という主張が検査されなくなる)
#![allow(clippy::float_cmp)]

use alice_lol::runtime_parser::parse_lol;
use alice_lol::{eval, SdfNode};
use glam::Vec3;

/// 標本点 — 面の内外と角をまたぐ決定論的な 7 点
fn probes() -> Vec<Vec3> {
    vec![
        Vec3::ZERO,
        Vec3::new(0.7, 0.2, 0.1),
        Vec3::new(0.4, 0.4, 0.4),
        Vec3::new(1.2, 0.0, 0.0),
        Vec3::new(-0.6, 0.3, -0.2),
        Vec3::new(0.51, 0.51, 0.51),
        Vec3::new(0.0, 0.9, 0.0),
    ]
}

/// README の example A (builder API) と B (LOL DSL) が同じ場を返す
#[test]
fn readme_builder_and_dsl_agree() {
    // A: ALICE-SDF の builder API (`box3d` は全長なので 1.0 = 半幅 0.5)
    let a = SdfNode::sphere(1.0).subtract(SdfNode::box3d(1.0, 1.0, 1.0));
    // B: LOL DSL (`box3d` は半幅なので 0.5)
    let b = parse_lol("subtract(sphere(1.0), box3d(0.5, 0.5, 0.5))")
        .expect("README の LOL snippet が parse できない");

    for p in probes() {
        assert_eq!(
            eval(&a, p),
            eval(&b, p),
            "README の 2 例が {p:?} で一致しない (builder {} vs DSL {})",
            eval(&a, p),
            eval(&b, p)
        );
    }
}

/// 全長 / 半幅の差を pin する — README の注記が実装と合っているか
#[test]
fn box3d_is_full_extent_in_rust_and_half_extent_in_the_dsl() {
    let full = SdfNode::box3d(1.0, 1.0, 1.0);
    let half = SdfNode::box3d_half_extents(0.5, 0.5, 0.5);
    let dsl = parse_lol("box3d(0.5, 0.5, 0.5)").expect("DSL box3d が parse できない");

    for p in probes() {
        assert_eq!(eval(&full, p), eval(&half, p), "全長 1.0 = 半幅 0.5");
        assert_eq!(eval(&dsl, p), eval(&half, p), "DSL の box3d は半幅");
    }

    // 取り違えると 2 倍ずれることも pin しておく (注記が要る理由)
    let mistaken = parse_lol("box3d(1.0, 1.0, 1.0)").expect("parse");
    assert_ne!(
        eval(&mistaken, Vec3::new(0.75, 0.0, 0.0)),
        eval(&full, Vec3::new(0.75, 0.0, 0.0)),
        "半幅と全長を取り違えても同じ値になるなら、注記は不要ということになる"
    );
}
