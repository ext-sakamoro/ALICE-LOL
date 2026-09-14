//! LOL 生成品質 benchmark (A2-3): grammar-only vs think→grammar の pass 率
//!
//! 20 prompt (T1 単体 primitive / T2 合成 / T3 変換・修飾 / T4 Phase 3 Intent) を
//! `MiniCPM5` 等の GGUF に投げ、出力を **oracle** で判定する 文字列一致ではなく
//! ALICE-SDF `eval` による点の内外判定 (T1-T3) と `Program.intent` の verb 構造
//! (T4) で判定するので、同じ形を別の式で書いても pass になる
//!
//! ```text
//! cargo run --release --example llm_bench --features llm-bridge -- \
//!     --model ~/ALICE-LLM/models/MiniCPM5-2B-Q4_K_M.gguf \
//!     [--mode grammar|think|both] [--prefix-budget 800] [--max-tokens 192] \
//!     [--only T2] [--out results.jsonl]
//! ```
//!
//! 出力: 1 行 / (prompt × mode) の表 + tier 別 pass 率 `--out` で JSONL も書く
//! (serde 非依存、手書き escape)

#![allow(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::similar_names
)] // benchmark harness

use alice_llm::gguf::GgufFile;
use alice_lol::bridge::{
    generate_program_from_prompt, generate_program_thinking, GgufTokenizer, Llama3Model,
};
use alice_lol::intent::{IntentNode, Program};
use alice_lol::{eval, Vec3};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::process;
use std::time::Instant;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Oracle
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Clone, Copy)]
enum Side {
    In,
    Out,
}

/// Intent verb の種類 (引数は見ない、対象 entity id だけ見る)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verb {
    Grasp(u32),
    Release(u32),
    Walk,
    Gaze,
    Point,
    Throw,
    Push(u32),
    Follow(u32),
    Avoid(u32),
    Rest,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Compose {
    Seq,
    Par,
}

enum Oracle {
    /// (点, 期待) の全てが一致
    Points(&'static [(f32, f32, f32, Side)]),
    /// intent が (合成種別, verb 列) と一致、entities 数 >= `min_entities`
    Intent {
        compose: Compose,
        verbs: &'static [Verb],
        min_entities: usize,
    },
}

struct Case {
    id: &'static str,
    tier: &'static str,
    prompt: &'static str,
    oracle: Oracle,
}

const CASES: &[Case] = &[
    // ── T1: 単体 primitive + 寸法 ──
    Case {
        id: "t1_sphere",
        tier: "T1",
        prompt: "A sphere of radius 10 centered at the origin.",
        oracle: Oracle::Points(&[
            (0.0, 0.0, 0.0, Side::In),
            (9.0, 0.0, 0.0, Side::In),
            (0.0, 0.0, -9.5, Side::In),
            (11.0, 0.0, 0.0, Side::Out),
        ]),
    },
    Case {
        id: "t1_box",
        tier: "T1",
        prompt: "A box 20 wide (X), 30 tall (Y), 40 deep (Z), centered at the origin.",
        oracle: Oracle::Points(&[
            (0.0, 0.0, 0.0, Side::In),
            (9.0, 14.0, 19.0, Side::In),
            (11.0, 0.0, 0.0, Side::Out),
            (0.0, 16.0, 0.0, Side::Out),
            (0.0, 0.0, 21.0, Side::Out),
        ]),
    },
    Case {
        id: "t1_cylinder",
        tier: "T1",
        prompt: "A cylinder of radius 5 and total height 40 along the Y axis, centered at the origin.",
        oracle: Oracle::Points(&[
            (0.0, 19.0, 0.0, Side::In),
            (4.5, 0.0, 0.0, Side::In),
            (0.0, 21.0, 0.0, Side::Out),
            (5.5, 0.0, 0.0, Side::Out),
        ]),
    },
    Case {
        id: "t1_torus",
        tier: "T1",
        prompt: "A torus lying in the XZ plane, major radius 20, minor radius 3, centered at the origin.",
        oracle: Oracle::Points(&[
            (20.0, 0.0, 0.0, Side::In),
            (0.0, 0.0, 20.0, Side::In),
            (20.0, 2.5, 0.0, Side::In),
            (0.0, 0.0, 0.0, Side::Out),
            (24.0, 0.0, 0.0, Side::Out),
        ]),
    },
    Case {
        id: "t1_translated_sphere",
        tier: "T1",
        prompt: "A sphere of radius 5 centered at (10, 20, 30).",
        oracle: Oracle::Points(&[
            (10.0, 20.0, 30.0, Side::In),
            (14.0, 20.0, 30.0, Side::In),
            (10.0, 26.0, 30.0, Side::Out),
            (0.0, 0.0, 0.0, Side::Out),
        ]),
    },
    // ── T2: 合成 2-3 要素 ──
    Case {
        id: "t2_snowman",
        tier: "T2",
        prompt: "A snowman: three spheres stacked along Y, unioned. Radius 10 centered at (0,0,0), radius 7 centered at (0,15,0), radius 5 centered at (0,25,0).",
        oracle: Oracle::Points(&[
            (0.0, 0.0, 0.0, Side::In),
            (0.0, 15.0, 0.0, Side::In),
            (0.0, 25.0, 0.0, Side::In),
            (0.0, 30.5, 0.0, Side::Out),
            (12.0, 0.0, 0.0, Side::Out),
        ]),
    },
    Case {
        id: "t2_mug",
        tier: "T2",
        prompt: "A mug: a cylinder of radius 25 and total height 100 centered at the origin, plus a torus handle (major radius 12, minor radius 4) centered at (25, 0, 0) on the +X side.",
        oracle: Oracle::Points(&[
            (0.0, 0.0, 0.0, Side::In),
            (0.0, 49.0, 0.0, Side::In),
            (37.0, 0.0, 0.0, Side::In),
            (0.0, 51.0, 0.0, Side::Out),
            (60.0, 0.0, 0.0, Side::Out),
        ]),
    },
    Case {
        id: "t2_table",
        tier: "T2",
        prompt: "A table: a top box 60 wide (X), 4 tall (Y), 40 deep (Z) centered at (0, 30, 0), and four cylindrical legs of radius 2 and total height 28, centered at (25, 14, 15), (-25, 14, 15), (25, 14, -15), (-25, 14, -15). Union everything.",
        oracle: Oracle::Points(&[
            (0.0, 30.0, 0.0, Side::In),
            (25.0, 14.0, 15.0, Side::In),
            (-25.0, 14.0, -15.0, Side::In),
            (0.0, 14.0, 0.0, Side::Out),
            (0.0, 34.0, 0.0, Side::Out),
        ]),
    },
    Case {
        id: "t2_arch",
        tier: "T2",
        prompt: "An arch: a box 40 wide (X), 30 tall (Y), 10 deep (Z) centered at (0, 15, 0), with a cylinder of radius 10 running along Z (rotate a Y cylinder 90 degrees about X), centered at (0, 10, 0), subtracted from it.",
        oracle: Oracle::Points(&[
            (0.0, 5.0, 0.0, Side::Out),
            (0.0, 29.0, 0.0, Side::In),
            (18.0, 10.0, 0.0, Side::In),
            (0.0, 25.0, 0.0, Side::In),
            (25.0, 15.0, 0.0, Side::Out),
        ]),
    },
    Case {
        id: "t2_pillar_ring",
        tier: "T2",
        prompt: "A cylinder of radius 5 and total height 40 centered at the origin, unioned with a torus in the XZ plane (major radius 10, minor radius 3) centered at (0, 20, 0).",
        oracle: Oracle::Points(&[
            (0.0, 0.0, 0.0, Side::In),
            (10.0, 20.0, 0.0, Side::In),
            (0.0, 19.0, 0.0, Side::In),
            (10.0, 25.0, 0.0, Side::Out),
            (0.0, 0.0, 15.0, Side::Out),
        ]),
    },
    // ── T3: 変換 / 修飾 ──
    Case {
        id: "t3_rotated_box",
        tier: "T3",
        prompt: "A cube with half extents 10 centered at the origin, rotated 45 degrees about the Y axis.",
        oracle: Oracle::Points(&[
            (0.0, 0.0, 0.0, Side::In),
            (13.0, 0.0, 0.0, Side::In),
            (0.0, 9.0, 0.0, Side::In),
            (13.0, 0.0, 13.0, Side::Out),
            (0.0, 11.0, 0.0, Side::Out),
        ]),
    },
    Case {
        id: "t3_plate_hole",
        tier: "T3",
        prompt: "A plate: a box 40 wide (X), 4 tall (Y), 40 deep (Z) centered at the origin, with a vertical cylindrical hole of radius 5 through its center (subtract a Y cylinder of radius 5 and total height 20).",
        oracle: Oracle::Points(&[
            (10.0, 0.0, 10.0, Side::In),
            (19.0, 0.0, 19.0, Side::In),
            (0.0, 0.0, 0.0, Side::Out),
            (0.0, 0.0, 4.0, Side::Out),
            (0.0, 3.0, 0.0, Side::Out),
        ]),
    },
    Case {
        id: "t3_polar_ring",
        tier: "T3",
        prompt: "Six spheres of radius 3 arranged evenly in a ring of radius 15 around the Y axis, one of them on the +X axis (polar_repeat of a sphere translated by 15 along X).",
        oracle: Oracle::Points(&[
            (15.0, 0.0, 0.0, Side::In),
            (7.5, 0.0, 12.99, Side::In),
            (0.0, 0.0, 0.0, Side::Out),
            (12.99, 0.0, 7.5, Side::Out),
            (15.0, 5.0, 0.0, Side::Out),
        ]),
    },
    Case {
        id: "t3_scaled_sphere",
        tier: "T3",
        prompt: "A sphere of radius 1 scaled uniformly by 8, centered at the origin.",
        oracle: Oracle::Points(&[
            (0.0, 0.0, 0.0, Side::In),
            (7.0, 0.0, 0.0, Side::In),
            (9.0, 0.0, 0.0, Side::Out),
        ]),
    },
    Case {
        id: "t3_smooth_union",
        tier: "T3",
        prompt: "A smooth union with blend radius 5 of two spheres of radius 10: one centered at the origin, one centered at (15, 0, 0).",
        oracle: Oracle::Points(&[
            (0.0, 0.0, 0.0, Side::In),
            (15.0, 0.0, 0.0, Side::In),
            (7.5, 0.0, 0.0, Side::In),
            (7.5, 0.0, 11.0, Side::Out),
            (30.0, 0.0, 0.0, Side::Out),
        ]),
    },
    // ── T4: Phase 3 Intent (program(...)) ──
    Case {
        id: "t4_grasp_release",
        tier: "T4",
        prompt: "Scene: a sphere of radius 10 at the origin. Entity 0: a box with half extents 2 at (20, 0, 0). Intent: grasp entity 0 with the right hand with force 5, rest for 1000 ms, then release entity 0. Output a program(...).",
        oracle: Oracle::Intent {
            compose: Compose::Seq,
            verbs: &[Verb::Grasp(0), Verb::Rest, Verb::Release(0)],
            min_entities: 1,
        },
    },
    Case {
        id: "t4_walk_gaze",
        tier: "T4",
        prompt: "Scene: a box with half extents 5 at the origin. Entity 0: a sphere of radius 3 at (0, 0, 30). Intent: walk to (0, 0, 20) at speed 1, then gaze at (0, 0, 30) for 500 ms. Output a program(...).",
        oracle: Oracle::Intent {
            compose: Compose::Seq,
            verbs: &[Verb::Walk, Verb::Gaze],
            min_entities: 1,
        },
    },
    Case {
        id: "t4_push_follow",
        tier: "T4",
        prompt: "Scene: a sphere of radius 10 at the origin. Entity 0: a cylinder of radius 2 and total height 10 at (15, 0, 0). Intent: push entity 0 in the +X direction (1, 0, 0) with force 3, then follow entity 0 keeping a distance of 5. Output a program(...).",
        oracle: Oracle::Intent {
            compose: Compose::Seq,
            verbs: &[Verb::Push(0), Verb::Follow(0)],
            min_entities: 1,
        },
    },
    Case {
        id: "t4_par_point_avoid",
        tier: "T4",
        prompt: "Scene: a sphere of radius 10 at the origin. Entity 0: a sphere of radius 2 at (10, 0, 0). Entity 1: a box with half extents 3 at (-10, 0, 0). Intent, in parallel: point at (1, 1, 1) with the left hand, and avoid entity 1 keeping at least 10 away. Output a program(...).",
        oracle: Oracle::Intent {
            compose: Compose::Par,
            verbs: &[Verb::Point, Verb::Avoid(1)],
            min_entities: 2,
        },
    },
    Case {
        id: "t4_throw_rest",
        tier: "T4",
        prompt: "Scene: a box with half extents 5 at the origin. Entity 0: a sphere of radius 2 at (0, 10, 0). Intent: throw toward (10, 5, 0) with force 8 using the right hand, then rest for 200 ms. Output a program(...).",
        oracle: Oracle::Intent {
            compose: Compose::Seq,
            verbs: &[Verb::Throw, Verb::Rest],
            min_entities: 1,
        },
    },
];

const fn verb_of(node: &IntentNode) -> Option<Verb> {
    Some(match node {
        IntentNode::Grasp { target_id, .. } => Verb::Grasp(*target_id),
        IntentNode::Release { target_id } => Verb::Release(*target_id),
        IntentNode::Walk { .. } => Verb::Walk,
        IntentNode::Gaze { .. } => Verb::Gaze,
        IntentNode::Point { .. } => Verb::Point,
        IntentNode::Throw { .. } => Verb::Throw,
        IntentNode::Push { target_id, .. } => Verb::Push(*target_id),
        IntentNode::Follow { target_id, .. } => Verb::Follow(*target_id),
        IntentNode::Avoid { target_id, .. } => Verb::Avoid(*target_id),
        IntentNode::Rest { .. } => Verb::Rest,
        _ => return None,
    })
}

/// oracle 判定 → (pass, 説明)
fn judge(oracle: &Oracle, program: &Program) -> (bool, String) {
    match oracle {
        Oracle::Points(pts) => {
            let mut fails = Vec::new();
            for &(x, y, z, side) in *pts {
                let d = eval(&program.sdf, Vec3::new(x, y, z));
                let ok = match side {
                    Side::In => d < 0.0,
                    Side::Out => d > 0.0,
                };
                if !ok {
                    fails.push(format!(
                        "({x},{y},{z}) d={d:.2} expected {}",
                        match side {
                            Side::In => "in",
                            Side::Out => "out",
                        }
                    ));
                }
            }
            (fails.is_empty(), fails.join("; "))
        }
        Oracle::Intent {
            compose,
            verbs,
            min_entities,
        } => {
            if program.sdf_registry.len() < *min_entities {
                return (
                    false,
                    format!("entities {} < {min_entities}", program.sdf_registry.len()),
                );
            }
            let Some(intent) = &program.intent else {
                return (false, "no intent".into());
            };
            let (got_compose, children): (Compose, &[IntentNode]) = match intent {
                IntentNode::Sequence(v) => (Compose::Seq, v),
                IntentNode::Parallel(v) => (Compose::Par, v),
                single => {
                    // 単一 verb は長さ 1 の Sequence と同一視
                    let got: Vec<Option<Verb>> = vec![verb_of(single)];
                    let want: Vec<Option<Verb>> = verbs.iter().map(|v| Some(*v)).collect();
                    return (got == want, format!("single verb {got:?} vs {want:?}"));
                }
            };
            let got: Vec<Option<Verb>> = children.iter().map(verb_of).collect();
            let want: Vec<Option<Verb>> = verbs.iter().map(|v| Some(*v)).collect();
            let ok = got_compose == *compose && got == want;
            (
                ok,
                format!("{got_compose:?} {got:?} vs {compose:?} {want:?}"),
            )
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Prompt (ChatML)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

const SYSTEM_PROMPT: &str = "You write ALICE-LOL, a tiny SDF DSL. Output exactly one expression and nothing else: no comments, no indentation, single spaces only, no trailing text.\n\
Primitives (all centered at the origin unless translated): sphere(r), box3d(hx, hy, hz) with HALF extents, cylinder(r, half_height) along Y, torus(major_r, minor_r) in the XZ plane.\n\
Operations: union(a, b, ...), subtract(base, cutter), smooth_union(k, a, b), translate(x, y, z, child), rotate(rx_deg, ry_deg, rz_deg, child), scale(s, child), polar_repeat(n, child).\n\
Numbers are plain decimals. Y is up.\n\
Example: a cup 40 wide and 80 tall with a ring handle on its +X side:\n\
union(cylinder(20,40), translate(26,0,0, torus(12,4)))\n\
Phase 3 Intent: to describe an action, wrap the scene as program(<scene sdf>, entities(<sdf>, ...), <intent>). Entity ids are 0-based positions inside entities(...). Intent verbs: grasp(id, left|right|both, force), release(id), catch(id), walk(x, y, z, speed), gaze(x, y, z, ms), point(x, y, z, hand), throw(x, y, z, force, hand), push(id, dx, dy, dz, force), pull(id, dx, dy, dz, force), turn(id, ax, ay, az, rad), align(id, rx, ry, rz), follow(id, dist), avoid(id, min_dist), rest(ms), seq(i1, i2, ...), par(i1, i2, ...).\n\
Example: program(sphere(10), entities(box3d(2,2,2)), seq(grasp(0, right, 5.0), rest(500), release(0)))";

fn chatml(system: &str, user: &str) -> String {
    format!("<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n")
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Harness
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// 集計用 (詳細は行出力 / JSONL 側)
struct Row {
    tier: &'static str,
    mode: &'static str,
    parse_ok: bool,
    pass: bool,
    ms: u128,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let model_path = arg_after(&args, "--model").unwrap_or_else(|| {
        eprintln!("--model <path.gguf> missing");
        process::exit(2);
    });
    let mode = arg_after(&args, "--mode").unwrap_or("both");
    let prefix_budget: usize = arg_after(&args, "--prefix-budget")
        .and_then(|s| s.parse().ok())
        .unwrap_or(800);
    let max_tokens: usize = arg_after(&args, "--max-tokens")
        .and_then(|s| s.parse().ok())
        .unwrap_or(192);
    let only = arg_after(&args, "--only");
    let out_path = arg_after(&args, "--out");

    let modes: Vec<&'static str> = match mode {
        "grammar" => vec!["grammar"],
        "think" => vec!["think"],
        _ => vec!["grammar", "think"],
    };

    let t0 = Instant::now();
    let data = fs::read(model_path).expect("read gguf");
    let gguf = GgufFile::parse(&data).expect("parse gguf");
    let tokenizer = GgufTokenizer::from_gguf(&gguf).expect("tokenizer");
    let mut model = Llama3Model::from_gguf(&gguf).expect("model");
    println!(
        "model loaded in {}ms (vocab {}), modes={modes:?}, prefix_budget={prefix_budget}, max_tokens={max_tokens}",
        t0.elapsed().as_millis(),
        tokenizer.vocab_size()
    );

    let mut rows: Vec<Row> = Vec::new();
    let mut out_file = out_path.map(|p| fs::File::create(p).expect("create --out"));

    for case in CASES {
        if let Some(t) = only {
            if !case.tier.eq_ignore_ascii_case(t) && case.id != t {
                continue;
            }
        }
        let prompt = chatml(SYSTEM_PROMPT, case.prompt);
        for &m in &modes {
            let t = Instant::now();
            let (parsed, text, prefix_tokens, marker_hit, err): (
                Option<Program>,
                String,
                usize,
                Option<bool>,
                String,
            ) = if m == "grammar" {
                match generate_program_from_prompt(&mut model, &tokenizer, &prompt, max_tokens) {
                    Ok(p) => (Some(p), String::new(), 0, None, String::new()),
                    Err(e) => (None, String::new(), 0, None, e.to_string()),
                }
            } else {
                match generate_program_thinking(
                    &mut model,
                    &tokenizer,
                    &prompt,
                    "</think>",
                    prefix_budget,
                    max_tokens,
                ) {
                    Ok(o) => (
                        Some(o.program),
                        o.text,
                        o.prefix_text.len(),
                        Some(o.prefix_marker_hit),
                        String::new(),
                    ),
                    Err(e) => (None, String::new(), 0, None, e.to_string()),
                }
            };
            let ms = t.elapsed().as_millis();
            let (parse_ok, pass, detail, shown) = parsed.as_ref().map_or_else(
                || (false, false, err.clone(), String::new()),
                |p| {
                    let (ok, d) = judge(&case.oracle, p);
                    // grammar 経路は text を返さないので intent / sdf の要約で代替
                    let shown = if text.is_empty() {
                        p.intent
                            .as_ref()
                            .map_or_else(|| format!("{:?}", p.sdf), IntentNode::to_lol)
                    } else {
                        text.clone()
                    };
                    (true, ok, d, shown)
                },
            );
            println!(
                "[{:<2}] {:<20} {:<7} parse={} pass={} {}ms prefix_chars={} marker={:?}\n      out: {}\n      why: {}",
                case.tier,
                case.id,
                m,
                u8::from(parse_ok),
                u8::from(pass),
                ms,
                prefix_tokens,
                marker_hit,
                shown.trim().chars().take(200).collect::<String>(),
                detail.chars().take(200).collect::<String>()
            );
            if let Some(f) = out_file.as_mut() {
                let _ = writeln!(
                    f,
                    "{{\"id\":\"{}\",\"tier\":\"{}\",\"mode\":\"{}\",\"parse_ok\":{},\"pass\":{},\"ms\":{},\"prefix_chars\":{},\"marker_hit\":{},\"text\":\"{}\",\"detail\":\"{}\"}}",
                    case.id,
                    case.tier,
                    m,
                    parse_ok,
                    pass,
                    ms,
                    prefix_tokens,
                    marker_hit.map_or_else(|| "null".to_string(), |b| b.to_string()),
                    json_escape(shown.trim()),
                    json_escape(&detail)
                );
            }
            rows.push(Row {
                tier: case.tier,
                mode: m,
                parse_ok,
                pass,
                ms,
            });
        }
    }

    // ── Summary ──
    println!("\n== pass rate by tier ==");
    println!(
        "{:<5} {:<8} {:>6} {:>6} {:>8}",
        "tier", "mode", "parse", "pass", "avg_ms"
    );
    for tier in ["T1", "T2", "T3", "T4", "ALL"] {
        for &m in &modes {
            let sel: Vec<&Row> = rows
                .iter()
                .filter(|r| r.mode == m && (tier == "ALL" || r.tier == tier))
                .collect();
            if sel.is_empty() {
                continue;
            }
            let n = sel.len();
            let parse = sel.iter().filter(|r| r.parse_ok).count();
            let pass = sel.iter().filter(|r| r.pass).count();
            let avg_ms = sel.iter().map(|r| r.ms).sum::<u128>() / n as u128;
            println!("{tier:<5} {m:<8} {parse:>3}/{n:<2} {pass:>3}/{n:<2} {avg_ms:>8}");
        }
    }
}
