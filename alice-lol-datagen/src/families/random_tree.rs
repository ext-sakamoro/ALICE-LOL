//! grammar bucket からの random tree — 語彙カバレッジ用
//!
//! `alice_lol::LOL_GBNF` の `name_*` bucket を読み、深さ制限付きで construct を
//! random に組む caption は [`crate::caption::describe`] の構造列挙 (幾何的に
//! 正しいが自然文ではない) oracle 点は持たない (parse / emit 冪等のみ検証)
//! product / mechanical shortcut (`name_0f`〜`name_7f` の一部) も対象に入る

use crate::caption::describe;
use crate::rng::Rng;
use crate::sample::{Sample, VerifyError};
use alice_lol::runtime_parser::parse_lol;
use std::sync::OnceLock;

struct Buckets {
    prims: Vec<(String, usize)>, // (name, n floats)
    variadic: Vec<String>,
    k_children: Vec<String>,
    ab: Vec<String>,
    f1_ab: Vec<String>,
    m1: Vec<String>,
    m3: Vec<String>,
}

fn buckets() -> &'static Buckets {
    static B: OnceLock<Buckets> = OnceLock::new();
    B.get_or_init(|| {
        let mut b = Buckets {
            prims: Vec::new(),
            variadic: Vec::new(),
            k_children: Vec::new(),
            ab: Vec::new(),
            f1_ab: Vec::new(),
            m1: Vec::new(),
            m3: Vec::new(),
        };
        let mut cur: Option<String> = None;
        let mut push = |rule: &str, names: Vec<String>| match rule {
            "name_1f" => b.prims.extend(names.into_iter().map(|n| (n, 1))),
            "name_2f" => b.prims.extend(names.into_iter().map(|n| (n, 2))),
            "name_3f" => b.prims.extend(names.into_iter().map(|n| (n, 3))),
            "name_4f" => b.prims.extend(names.into_iter().map(|n| (n, 4))),
            "name_variadic" => b.variadic.extend(names),
            "name_k_children" => b.k_children.extend(names),
            "name_ab" => b.ab.extend(names),
            "name_1f_ab" => b.f1_ab.extend(names),
            "name_1f_child" => b.m1.extend(names),
            "name_3f_child" => b.m3.extend(names),
            _ => {}
        };
        for line in alice_lol::LOL_GBNF.lines() {
            let t = line.trim();
            if t.starts_with('#') || t.is_empty() {
                cur = None;
                continue;
            }
            if let Some((lhs, rhs)) = t.split_once("::=") {
                let lhs = lhs.trim().to_string();
                cur = lhs.starts_with("name_").then_some(lhs.clone());
                if cur.is_some() {
                    push(&lhs, quoted(rhs));
                }
            } else if let Some(rule) = &cur {
                push(rule, quoted(t));
            }
        }
        // primitive は真の primitive だけ (product / mechanical shortcut は
        // 数 KB の展開形になり random tree の語彙目的に合わない → product_shortcut family)
        let allow = [
            "sphere",
            "box3d",
            "rounded_box",
            "cylinder",
            "torus",
            "cone",
            "capsule",
            "ellipsoid",
            "octahedron",
            "pyramid",
            "hex_prism",
            "link",
            "capped_cone",
            "capped_torus",
            "rounded_cylinder",
            "tube",
            "barrel",
            "heart",
            "egg",
            "tetrahedron",
            "box_frame",
            "diamond",
            "star_polygon",
            "cross_shape",
            "triangular_prism",
            "cut_sphere",
            "solid_angle",
            "rhombus",
            "vesica",
            "chamfered_cube",
            "rounded_x",
            "pie",
            "trapezoid",
            "parallelogram",
            "tunnel",
            "uneven_capsule",
            "arc_shape",
            "moon",
            "regular_polygon",
            "dodecahedron",
            "icosahedron",
            "truncated_octahedron",
            "truncated_icosahedron",
        ];
        b.prims.retain(|(n, _)| allow.contains(&n.as_str()));
        let skip = ["with_material", "displacement", "noise", "animate"];
        b.m1.retain(|n| !skip.contains(&n.as_str()));
        b.m3.retain(|n| !skip.contains(&n.as_str()));
        b
    })
}

fn quoted(s: &str) -> Vec<String> {
    s.split('"')
        .skip(1)
        .step_by(2)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .collect()
}

fn num(rng: &mut Rng) -> String {
    format!("{}", rng.stepped(1.0, 20.0, 1.0))
}

fn nums(rng: &mut Rng, n: usize) -> String {
    (0..n).map(|_| num(rng)).collect::<Vec<_>>().join(", ")
}

fn tree(rng: &mut Rng, depth: u32) -> String {
    let b = buckets();
    if depth == 0 || rng.chance(0.35) {
        let (name, n) = rng.pick(&b.prims);
        return format!("{name}({})", nums(rng, *n));
    }
    match rng.int(0, 6) {
        0 => format!(
            "translate({}, {}, {}, {})",
            num(rng),
            num(rng),
            num(rng),
            tree(rng, depth - 1)
        ),
        1 => format!(
            "rotate({}, {}, {}, {})",
            rng.stepped(0.0, 90.0, 15.0),
            rng.stepped(0.0, 90.0, 15.0),
            0.0,
            tree(rng, depth - 1)
        ),
        2 => format!(
            "{}({}, {})",
            rng.pick(&b.variadic),
            tree(rng, depth - 1),
            tree(rng, depth - 1)
        ),
        3 => format!(
            "{}({}, {}, {})",
            rng.pick(&b.k_children),
            rng.stepped(0.5, 3.0, 0.5),
            tree(rng, depth - 1),
            tree(rng, depth - 1)
        ),
        4 => format!(
            "{}({}, {})",
            rng.pick(&b.ab),
            tree(rng, depth - 1),
            tree(rng, depth - 1)
        ),
        5 => format!(
            "{}({}, {})",
            rng.pick(&b.m1),
            rng.stepped(1.0, 4.0, 0.5),
            tree(rng, depth - 1)
        ),
        _ => format!(
            "{}({}, {})",
            rng.pick(&b.m3),
            nums(rng, 3),
            tree(rng, depth - 1)
        ),
    }
}

pub(crate) fn generate(rng: &mut Rng) -> Result<Sample, VerifyError> {
    let depth = rng.int(1, 3) as u32;
    let lol = tree(rng, depth);
    let node = parse_lol(&lol).map_err(|e| VerifyError::Parse(e.to_string()))?;
    let en = format!("{}.", capitalize(&describe(&node)));
    let ja = format!("次の形状を LOL で書く: {}", describe(&node));
    Sample::new("random_tree", en, ja, &lol, Vec::new())
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}
