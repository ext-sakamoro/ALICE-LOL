//! `emit::to_lol` round-trip (Track C0)
//!
//! `lol.gbnf` の name bucket を走査して **全 construct** に既定引数を与えた
//! snippet を生成し、`parse → emit → parse` の 2 tree が eval parity を持つ
//! ことを検証する grammar に construct が増えれば自動的に対象になる
//! (`grammar_covers_every_runtime_parser_construct` と合わせて
//! parser ⊆ grammar ⊆ emit の 3 者を CI で固定)
//!
//! 判定が eval parity なのは `Rotate` の Quat ↔ Euler 往復等で bit 一致が
//! 期待できないため 2 回目の emit は 1 回目と文字列一致する (正規形の冪等性)

#![allow(
    clippy::match_same_arms,
    clippy::suboptimal_flops,
    clippy::cast_precision_loss
)] // corpus 生成 test

use alice_lol::emit::to_lol;
use alice_lol::runtime_parser::parse_lol;
use alice_lol::{eval, SdfNode, Vec3};

const LOL_GBNF: &str = alice_lol::LOL_GBNF;

/// `name_xxx ::= "a" | "b" ...` を (rule, names) に
fn name_buckets() -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut cur: Option<(String, Vec<String>)> = None;
    for line in LOL_GBNF.lines() {
        let t = line.trim();
        if t.starts_with('#') || t.is_empty() {
            if let Some(b) = cur.take() {
                out.push(b);
            }
            continue;
        }
        if let Some((lhs, rhs)) = t.split_once("::=") {
            if let Some(b) = cur.take() {
                out.push(b);
            }
            let lhs = lhs.trim();
            if lhs.starts_with("name_") {
                cur = Some((lhs.to_string(), quoted(rhs)));
            }
        } else if let Some(b) = cur.as_mut() {
            b.1.extend(quoted(t));
        }
    }
    if let Some(b) = cur.take() {
        out.push(b);
    }
    out
}

fn quoted(s: &str) -> Vec<String> {
    s.split('"')
        .skip(1)
        .step_by(2)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .collect()
}

/// 既定引数 (退化しにくい値、整数 slot にも安全)
fn nums(n: usize) -> String {
    (0..n)
        .map(|i| format!("{}.0", [3, 2, 1, 4, 2, 3, 2, 1, 2, 1][i % 10]))
        .collect::<Vec<_>>()
        .join(", ")
}

const CHILD_A: &str = "sphere(1.0)";
const CHILD_B: &str = "box3d(0.5, 0.5, 0.5)";

/// bucket 名 → snippet
fn snippet(rule: &str, name: &str) -> Option<String> {
    let s = match rule {
        "name_0f" => format!("{name}()"),
        "name_1f" => format!("{name}({})", nums(1)),
        "name_2f" => format!("{name}({})", nums(2)),
        "name_3f" => format!("{name}({})", nums(3)),
        "name_4f" => format!("{name}({})", nums(4)),
        "name_5f" => format!("{name}({})", nums(5)),
        "name_6f" => format!("{name}({})", nums(6)),
        "name_7f" => format!("{name}({})", nums(7)),
        "name_9f" => format!("{name}({})", nums(9)),
        "name_10f" => format!("{name}({})", nums(10)),
        "name_variadic" => format!("{name}({CHILD_A}, {CHILD_B}, cylinder(0.4, 1.0))"),
        "name_k_children" => format!("{name}(0.3, {CHILD_A}, {CHILD_B})"),
        "name_2k_children" => format!("{name}(0.3, 3.0, {CHILD_A}, {CHILD_B})"),
        "name_ab" => format!("{name}({CHILD_A}, {CHILD_B})"),
        "name_1f_ab" => format!("{name}(0.3, {CHILD_A}, {CHILD_B})"),
        "name_2f_ab" => format!("{name}(0.3, 0.2, {CHILD_A}, {CHILD_B})"),
        "name_1f_child" => format!("{name}(2.0, {CHILD_A})"),
        "name_2f_child" => format!("{name}(1.0, 0.5, {CHILD_A})"),
        "name_3f_child" => format!("{name}(1.0, 2.0, 3.0, {CHILD_A})"),
        "name_6f_child" => format!("{name}(2.0, 2.0, 1.0, 3.0, 3.0, 3.0, {CHILD_A})"),
        "name_child_only" => format!("{name}({CHILD_A})"),
        // Intent 系 bucket は SDF ではない
        _ => return None,
    };
    Some(s)
}

/// 64 sample 点 (原点近傍 + 少し外) で eval parity
fn assert_parity(a: &SdfNode, b: &SdfNode, label: &str) {
    let mut seed: u32 = 0x9E37_79B9;
    for i in 0..64 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let r = |s: u32| (s as f32 / u32::MAX as f32) * 2.0 - 1.0;
        let scale = if i % 4 == 0 { 8.0 } else { 2.5 };
        let p = Vec3::new(
            r(seed) * scale,
            r(seed.rotate_left(11)) * scale,
            r(seed.rotate_left(22)) * scale,
        );
        let da = eval(a, p);
        let db = eval(b, p);
        assert!(
            (da.is_nan() && db.is_nan()) || (da - db).abs() <= 1e-4 * da.abs().max(1.0),
            "{label}: eval mismatch at {p:?}: {da} vs {db}"
        );
    }
}

#[test]
fn every_grammar_construct_round_trips() {
    let buckets = name_buckets();
    assert!(
        buckets.len() >= 20,
        "bucket parse failed: {}",
        buckets.len()
    );
    let mut checked = 0usize;
    let mut failures = Vec::new();
    for (rule, names) in &buckets {
        for name in names {
            let Some(src) = snippet(rule, name) else {
                continue;
            };
            let n1 = match parse_lol(&src) {
                Ok(n) => n,
                Err(e) => {
                    failures.push(format!(
                        "{name}: parse of default snippet failed: {e} ({src})"
                    ));
                    continue;
                }
            };
            let text = match to_lol(&n1) {
                Ok(t) => t,
                Err(e) => {
                    failures.push(format!("{name}: emit failed: {e}"));
                    continue;
                }
            };
            let n2 = match parse_lol(&text) {
                Ok(n) => n,
                Err(e) => {
                    failures.push(format!("{name}: re-parse failed: {e} (emitted `{text}`)"));
                    continue;
                }
            };
            assert_parity(&n1, &n2, name);
            // 正規形の冪等性
            let text2 = to_lol(&n2).unwrap();
            assert_eq!(text, text2, "{name}: emit is not idempotent");
            checked += 1;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // 257 parser construct − Intent verb 18 − program/entities 2 = 237 (2026-09-14)
    assert!(checked >= 230, "only {checked} constructs round-tripped");
}

#[test]
fn fixtures_round_trip() {
    let sword = include_str!("../../examples/sword.lol");
    let fixtures = [
        sword,
        "union(pen_cup(50,100), translate(33,0,50, rotate(0,90,0, torus(15,5))))",
        "union(cylinder(25,50), translate(25,0,50, rotate(0,90,0, torus(12,4))))",
        "subtract(rounded_box(30,2.5,30,3), screw_hole(4,15))",
        "translate(0.0, 1.2, 0.0, smooth_union(0.2, scale(0.3, sphere(1.0)), rotate(0.0, 0.5, 0.0, box3d(0.5, 0.5, 0.5))))",
        "lattice_infill(1.0, 4.0, 0.5, sphere(10.0))",
        "gridfinity_bin_ex(2, 3, 6, 1, 2, 1.2, 1.0)",
    ];
    for src in fixtures {
        let n1 = parse_lol(src).unwrap_or_else(|e| panic!("{e}: {src}"));
        let text = to_lol(&n1).unwrap_or_else(|e| panic!("{e}: {src}"));
        let n2 = parse_lol(&text).unwrap_or_else(|e| panic!("{e}: emitted `{text}`"));
        assert_parity(&n1, &n2, src);
        assert_eq!(text, to_lol(&n2).unwrap());
    }
}

#[test]
fn emitted_text_is_accepted_by_llm_grammar_rules() {
    // emit は comment を書かず、token 間 whitespace は `", "` の 1 個だけ
    let n = parse_lol("union(pen_cup(50,100), translate(33,0,50, rotate(0,90,0, torus(15,5))))")
        .unwrap();
    let text = to_lol(&n).unwrap();
    assert!(!text.contains("//"));
    assert!(!text.contains("  "));
    assert!(!text.contains('\n'));
}
