//! Phase 3 Intent program — fail 型「Intent 構造」(seq/par、entities 省略、id 誤り)
//!
//! scene 1 個 + entity 1〜2 個 + verb 2〜3 個の `seq` / `par` caption は verb を
//! 順に文で書き、entity は「Entity 0: …」と番号付きで列挙する

use super::{coord, small};
use crate::caption::{fmt_mm, fmt_pos};
use crate::rng::Rng;
use crate::sample::{OraclePoint, Sample, VerifyError};
use glam::Vec3;

struct Verb {
    lol: String,
    en: String,
    ja: String,
}

fn hand(rng: &mut Rng) -> (&'static str, &'static str, &'static str) {
    *rng.pick(&[
        ("right", "the right hand", "右手"),
        ("left", "the left hand", "左手"),
        ("both", "both hands", "両手"),
    ])
}

#[allow(clippy::too_many_lines)] // 10 verb の template
fn verb(rng: &mut Rng, n_entities: u32) -> Verb {
    let id = rng.int(0, i64::from(n_entities) - 1) as u32;
    match rng.int(0, 9) {
        0 => {
            let (h, hen, hja) = hand(rng);
            let f = small(rng);
            Verb {
                lol: format!("grasp({id}, {h}, {f})"),
                en: format!("grasp entity {id} with {hen} with force {}", fmt_mm(f)),
                ja: format!("エンティティ {id} を{hja}で力 {} で掴む", fmt_mm(f)),
            }
        }
        1 => Verb {
            lol: format!("release({id})"),
            en: format!("release entity {id}"),
            ja: format!("エンティティ {id} を離す"),
        },
        2 => {
            let p = Vec3::new(coord(rng), 0.0, coord(rng));
            let s = rng.stepped(0.5, 2.0, 0.5);
            Verb {
                lol: format!("walk({}, {}, {}, {s})", p.x, p.y, p.z),
                en: format!("walk to {} at speed {}", fmt_pos(p), fmt_mm(s)),
                ja: format!("{} へ速度 {} で歩く", fmt_pos(p), fmt_mm(s)),
            }
        }
        3 => {
            let p = Vec3::new(coord(rng), small(rng), coord(rng));
            let ms = rng.int(1, 20) * 100;
            Verb {
                lol: format!("gaze({}, {}, {}, {ms})", p.x, p.y, p.z),
                en: format!("gaze at {} for {ms} ms", fmt_pos(p)),
                ja: format!("{} を {ms} ミリ秒見つめる", fmt_pos(p)),
            }
        }
        4 => {
            let (h, hen, hja) = hand(rng);
            let p = Vec3::new(coord(rng), small(rng), coord(rng));
            Verb {
                lol: format!("point({}, {}, {}, {h})", p.x, p.y, p.z),
                en: format!("point at {} with {hen}", fmt_pos(p)),
                ja: format!("{} を{hja}で指す", fmt_pos(p)),
            }
        }
        5 => {
            let (h, hen, hja) = hand(rng);
            let p = Vec3::new(coord(rng), small(rng), coord(rng));
            let f = small(rng);
            Verb {
                lol: format!("throw({}, {}, {}, {f}, {h})", p.x, p.y, p.z),
                en: format!(
                    "throw toward {} with force {} using {hen}",
                    fmt_pos(p),
                    fmt_mm(f)
                ),
                ja: format!("{} に向けて{hja}で力 {} で投げる", fmt_pos(p), fmt_mm(f)),
            }
        }
        6 => {
            let f = small(rng);
            let (dx, dz, den, dja) = *rng.pick(&[
                (1.0, 0.0, "+X", "+X"),
                (-1.0, 0.0, "-X", "-X"),
                (0.0, 1.0, "+Z", "+Z"),
                (0.0, -1.0, "-Z", "-Z"),
            ]);
            Verb {
                lol: format!("push({id}, {dx}, 0, {dz}, {f})"),
                en: format!(
                    "push entity {id} in the {den} direction ({}, 0, {}) with force {}",
                    fmt_mm(dx),
                    fmt_mm(dz),
                    fmt_mm(f)
                ),
                ja: format!(
                    "エンティティ {id} を {dja} 方向 ({}, 0, {}) に力 {} で押す",
                    fmt_mm(dx),
                    fmt_mm(dz),
                    fmt_mm(f)
                ),
            }
        }
        7 => {
            let d = small(rng);
            Verb {
                lol: format!("follow({id}, {d})"),
                en: format!("follow entity {id} keeping a distance of {}", fmt_mm(d)),
                ja: format!("エンティティ {id} を距離 {} を保って追う", fmt_mm(d)),
            }
        }
        8 => {
            let d = small(rng);
            Verb {
                lol: format!("avoid({id}, {d})"),
                en: format!("avoid entity {id} keeping at least {} away", fmt_mm(d)),
                ja: format!("エンティティ {id} から最低 {} 離れる", fmt_mm(d)),
            }
        }
        _ => {
            let ms = rng.int(1, 30) * 100;
            Verb {
                lol: format!("rest({ms})"),
                en: format!("rest for {ms} ms"),
                ja: format!("{ms} ミリ秒休む"),
            }
        }
    }
}

fn entity(rng: &mut Rng) -> (String, String, String, Vec3) {
    let p = Vec3::new(coord(rng), small(rng), coord(rng));
    if rng.chance(0.5) {
        let r = small(rng);
        (
            format!("translate({}, {}, {}, sphere({r}))", p.x, p.y, p.z),
            format!("a sphere of radius {} at {}", fmt_mm(r), fmt_pos(p)),
            format!("半径 {} の球を {} に", fmt_mm(r), fmt_pos(p)),
            p,
        )
    } else {
        let h = small(rng);
        (
            format!("translate({}, {}, {}, box3d({h}, {h}, {h}))", p.x, p.y, p.z),
            format!("a cube with half extents {} at {}", fmt_mm(h), fmt_pos(p)),
            format!("半辺 {} の立方体を {} に", fmt_mm(h), fmt_pos(p)),
            p,
        )
    }
}

pub(crate) fn generate(rng: &mut Rng) -> Result<Sample, VerifyError> {
    let scene_r = rng.stepped(5.0, 30.0, 5.0);
    let scene = format!("sphere({scene_r})");
    let n_entities = rng.int(1, 2) as u32;
    let mut ents = Vec::new();
    for _ in 0..n_entities {
        ents.push(entity(rng));
    }
    let n_verbs = rng.int(2, 3) as usize;
    let verbs: Vec<Verb> = (0..n_verbs).map(|_| verb(rng, n_entities)).collect();
    let par = rng.chance(0.3);
    let compose = if par { "par" } else { "seq" };
    let lol = format!(
        "program({scene}, entities({}), {compose}({}))",
        ents.iter()
            .map(|e| e.0.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        verbs
            .iter()
            .map(|v| v.lol.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let ents_en: Vec<String> = ents
        .iter()
        .enumerate()
        .map(|(i, e)| format!("Entity {i}: {}", e.1))
        .collect();
    let ents_ja: Vec<String> = ents
        .iter()
        .enumerate()
        .map(|(i, e)| format!("エンティティ {i}: {}", e.2))
        .collect();
    let verbs_en: Vec<&str> = verbs.iter().map(|v| v.en.as_str()).collect();
    let verbs_ja: Vec<&str> = verbs.iter().map(|v| v.ja.as_str()).collect();
    let en = format!(
        "Scene: a sphere of radius {} at the origin. {}. Intent{}: {}. Output a program(...).",
        fmt_mm(scene_r),
        ents_en.join(". "),
        if par { ", in parallel" } else { "" },
        if par {
            verbs_en.join(", and ")
        } else {
            verbs_en.join(", then ")
        }
    );
    let ja = format!(
        "シーン: 半径 {} の球を原点に。{}。意図{}: {}。program(...) を出力する。",
        fmt_mm(scene_r),
        ents_ja.join("。"),
        if par { " (並列)" } else { " (順番に)" },
        if par {
            verbs_ja.join("、同時に")
        } else {
            verbs_ja.join("、次に")
        }
    );
    // scene の oracle (entities は registry なので sdf には含まれない)
    let oracle = vec![
        OraclePoint::inside(0.0, 0.0, 0.0),
        OraclePoint::outside(scene_r * 2.0 + 5.0, 0.0, 0.0),
    ];
    Sample::new("intent_program", en, ja, &lol, oracle)
}
