//! `lol.gbnf` の name bucket から **全 construct** の既定引数 snippet を生成する
//! 共通 corpus (`emit_roundtrip.rs` / `gpu_parity.rs` が共有)
//!
//! grammar に construct が増えれば自動的に対象になる (`grammar_covers_every_
//! runtime_parser_construct` と合わせて parser ⊆ grammar ⊆ emit ⊆ GPU parity
//! の 4 者を CI で固定)

#![allow(dead_code, clippy::cast_precision_loss)] // 各 test binary が使う subset は異なる / 標本点生成

/// `name_xxx ::= "a" | "b" ...` を (rule, names) に
pub fn name_buckets() -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut cur: Option<(String, Vec<String>)> = None;
    for line in alice_lol::LOL_GBNF.lines() {
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

/// bucket 名 → snippet (Intent 系 bucket は SDF ではないので None)
pub fn snippet(rule: &str, name: &str) -> Option<String> {
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
        "name_k_children" | "name_1f_ab" => format!("{name}(0.3, {CHILD_A}, {CHILD_B})"),
        "name_2k_children" => format!("{name}(0.3, 3.0, {CHILD_A}, {CHILD_B})"),
        "name_ab" => format!("{name}({CHILD_A}, {CHILD_B})"),
        "name_2f_ab" => format!("{name}(0.3, 0.2, {CHILD_A}, {CHILD_B})"),
        "name_1f_child" => format!("{name}(2.0, {CHILD_A})"),
        "name_2f_child" => format!("{name}(1.0, 0.5, {CHILD_A})"),
        "name_3f_child" => format!("{name}(1.0, 2.0, 3.0, {CHILD_A})"),
        "name_6f_child" => format!("{name}(2.0, 2.0, 1.0, 3.0, 3.0, 3.0, {CHILD_A})"),
        "name_child_only" => format!("{name}({CHILD_A})"),
        _ => return None,
    };
    Some(s)
}

/// (construct 名, snippet) の全 SDF construct — grammar 順
pub fn grammar_corpus() -> Vec<(String, String)> {
    let buckets = name_buckets();
    assert!(
        buckets.len() >= 20,
        "bucket parse failed: {}",
        buckets.len()
    );
    let mut out = Vec::new();
    for (rule, names) in &buckets {
        for name in names {
            if let Some(src) = snippet(rule, name) {
                out.push((name.clone(), src));
            }
        }
    }
    out
}

/// 深い合成の fixture (product / stdlib / 手書き)
pub fn fixtures() -> Vec<(&'static str, &'static str)> {
    vec![
        ("sword", include_str!("../../../examples/sword.lol")),
        (
            "pen_cup_torus",
            "union(pen_cup(50,100), translate(33,0,50, rotate(0,90,0, torus(15,5))))",
        ),
        (
            "cylinder_torus",
            "union(cylinder(25,50), translate(25,0,50, rotate(0,90,0, torus(12,4))))",
        ),
        (
            "rounded_box_screw_hole",
            "subtract(rounded_box(30,2.5,30,3), screw_hole(4,15))",
        ),
        (
            "smooth_union_scale_rotate",
            "translate(0.0, 1.2, 0.0, smooth_union(0.2, scale(0.3, sphere(1.0)), rotate(0.0, 0.5, 0.0, box3d(0.5, 0.5, 0.5))))",
        ),
        ("lattice_infill", "lattice_infill(1.0, 4.0, 0.5, sphere(10.0))"),
        ("gridfinity_bin_ex", "gridfinity_bin_ex(2, 3, 6, 1, 2, 1.2, 1.0)"),
    ]
}

/// 決定論的な標本点 (原点近傍 + 少し外、`scale` 倍)
pub fn sample_points(n: usize, scale: f32) -> Vec<glam::Vec3> {
    let mut seed: u32 = 0x9E37_79B9;
    (0..n)
        .map(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            #[allow(clippy::cast_precision_loss)]
            let r = |s: u32| (s as f32 / u32::MAX as f32).mul_add(2.0, -1.0);
            let k = if i % 4 == 0 { scale * 3.2 } else { scale };
            glam::Vec3::new(
                r(seed) * k,
                r(seed.rotate_left(11)) * k,
                r(seed.rotate_left(22)) * k,
            )
        })
        .collect()
}
