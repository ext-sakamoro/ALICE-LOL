//! `runtime_parser::parse_program` — Phase 3 Intent text 構文 (A0) の round-trip / error test
#![allow(clippy::float_cmp, clippy::too_many_lines)] // parse した literal と期待値の exact 比較が目的 / verb 網羅 test

use alice_lol::intent::{HandSide, IntentNode, Program};
use alice_lol::runtime_parser::{parse_lol, parse_program};
use alice_lol::{SdfNode, Vec3};

fn intent_of(src: &str) -> IntentNode {
    let p = parse_program(&format!(
        "program(sphere(1.0), entities(sphere(0.1), sphere(0.2)), {src})"
    ))
    .unwrap_or_else(|e| panic!("parse failed for `{src}`: {e}"));
    p.intent.expect("intent slot")
}

#[test]
fn bare_sdf_is_sdf_only_program() {
    let p = parse_program("sphere(1.0)").unwrap();
    assert!(matches!(p.sdf, SdfNode::Sphere { radius } if radius == 1.0));
    assert!(p.sdf_registry.is_empty());
    assert!(p.intent.is_none());
}

#[test]
fn program_without_entities_or_intent() {
    let p = parse_program("program(sphere(2.0))").unwrap();
    assert!(matches!(p.sdf, SdfNode::Sphere { radius } if radius == 2.0));
    assert!(p.sdf_registry.is_empty());
    assert!(p.intent.is_none());
}

#[test]
fn program_with_entities_only() {
    let p =
        parse_program("program(sphere(1.0), entities(box3d(0.1, 0.2, 0.3), sphere(0.5)))").unwrap();
    assert_eq!(p.sdf_registry.len(), 2);
    assert!(matches!(p.sdf_registry[1], SdfNode::Sphere { radius } if radius == 0.5));
    assert!(p.intent.is_none());
    let empty = parse_program("program(sphere(1.0), entities())").unwrap();
    assert!(empty.sdf_registry.is_empty());
}

#[test]
fn every_verb_parses_to_expected_node() {
    assert_eq!(
        intent_of("grasp(1, right, 5.0)"),
        IntentNode::Grasp {
            target_id: 1,
            hand: HandSide::Right,
            force: 5.0
        }
    );
    assert_eq!(
        intent_of("release(0)"),
        IntentNode::Release { target_id: 0 }
    );
    assert_eq!(intent_of("catch(1)"), IntentNode::Catch { object_id: 1 });
    assert_eq!(
        intent_of("walk(1.0, 0.0, 2.0, 1.2)"),
        IntentNode::Walk {
            destination: Vec3::new(1.0, 0.0, 2.0),
            speed: 1.2
        }
    );
    assert_eq!(
        intent_of("gaze(0.0, 1.5, 0.0, 800)"),
        IntentNode::Gaze {
            target: Vec3::new(0.0, 1.5, 0.0),
            duration_ms: 800
        }
    );
    assert_eq!(
        intent_of("point(1.0, 1.0, 1.0, both)"),
        IntentNode::Point {
            target: Vec3::ONE,
            hand: HandSide::Both
        }
    );
    assert_eq!(
        intent_of("throw(3.0, 1.0, 0.0, 9.5, left)"),
        IntentNode::Throw {
            target: Vec3::new(3.0, 1.0, 0.0),
            force: 9.5,
            hand: HandSide::Left
        }
    );
    assert_eq!(
        intent_of("push(0, 1.0, 0.0, 0.0, 2.0)"),
        IntentNode::Push {
            target_id: 0,
            direction: Vec3::X,
            force: 2.0
        }
    );
    assert_eq!(
        intent_of("pull(0, -1.0, 0.0, 0.0, 2.0)"),
        IntentNode::Pull {
            target_id: 0,
            direction: -Vec3::X,
            force: 2.0
        }
    );
    assert_eq!(
        intent_of("turn(1, 0.0, 1.0, 0.0, 1.5)"),
        IntentNode::Rotate {
            target_id: 1,
            axis: Vec3::Y,
            angle_rad: 1.5
        }
    );
    assert_eq!(
        intent_of("align(0, 0.0, 0.0, 1.0)"),
        IntentNode::Align {
            target_id: 0,
            reference: Vec3::Z
        }
    );
    assert_eq!(
        intent_of("follow(1, 0.5)"),
        IntentNode::Follow {
            target_id: 1,
            distance: 0.5
        }
    );
    assert_eq!(
        intent_of("avoid(1, 0.3)"),
        IntentNode::Avoid {
            target_id: 1,
            min_distance: 0.3
        }
    );
    assert_eq!(
        intent_of("rest(250)"),
        IntentNode::Rest { duration_ms: 250 }
    );
    assert_eq!(
        intent_of("latent(0.1, 0.2, 0.3, 0.4, 0.5)"),
        IntentNode::LatentIntent {
            values: vec![0.1, 0.2, 0.3, 0.4, 0.5].into_boxed_slice(),
            semantic_hint: None
        }
    );
    assert_eq!(
        intent_of("music(0, 1, 2, 3, 4, 5, 6, 255)"),
        IntentNode::Music {
            packet: [0, 1, 2, 3, 4, 5, 6, 255]
        }
    );
    assert_eq!(
        intent_of("seq(rest(1), par(rest(2), rest(3)))"),
        IntentNode::Sequence(vec![
            IntentNode::Rest { duration_ms: 1 },
            IntentNode::Parallel(vec![
                IntentNode::Rest { duration_ms: 2 },
                IntentNode::Rest { duration_ms: 3 }
            ])
        ])
    );
}

#[test]
fn to_lol_round_trips_every_variant() {
    let cases = vec![
        IntentNode::Grasp {
            target_id: 1,
            hand: HandSide::Right,
            force: 5.0,
        },
        IntentNode::Release { target_id: 0 },
        IntentNode::Catch { object_id: 1 },
        IntentNode::Walk {
            destination: Vec3::new(1.0, -0.25, 2.0),
            speed: 1.2,
        },
        IntentNode::Gaze {
            target: Vec3::new(0.0, 1.5, 0.0),
            duration_ms: 800,
        },
        IntentNode::Point {
            target: Vec3::ONE,
            hand: HandSide::Both,
        },
        IntentNode::Throw {
            target: Vec3::new(3.0, 1.0, 0.0),
            force: 9.5,
            hand: HandSide::Left,
        },
        IntentNode::Push {
            target_id: 0,
            direction: Vec3::X,
            force: 2.0,
        },
        IntentNode::Pull {
            target_id: 0,
            direction: -Vec3::X,
            force: 2.0,
        },
        IntentNode::Rotate {
            target_id: 1,
            axis: Vec3::Y,
            angle_rad: 1.25,
        },
        IntentNode::Align {
            target_id: 0,
            reference: Vec3::Z,
        },
        IntentNode::Follow {
            target_id: 1,
            distance: 0.5,
        },
        IntentNode::Avoid {
            target_id: 1,
            min_distance: 0.3,
        },
        IntentNode::Rest { duration_ms: 250 },
        IntentNode::LatentIntent {
            values: vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6].into_boxed_slice(),
            semantic_hint: None,
        },
        IntentNode::Music {
            packet: [7, 0, 16, 200, 3, 1, 0x34, 0x12],
        },
        IntentNode::Sequence(vec![
            IntentNode::Rest { duration_ms: 1 },
            IntentNode::Parallel(vec![
                IntentNode::Walk {
                    destination: Vec3::ZERO,
                    speed: 1.0,
                },
                IntentNode::Grasp {
                    target_id: 0,
                    hand: HandSide::Left,
                    force: 3.0,
                },
            ]),
        ]),
    ];
    for node in cases {
        let text = node.to_lol();
        let back = intent_of(&text);
        assert_eq!(back, node, "round-trip mismatch via `{text}`");
    }
}

#[test]
fn to_lol_drops_semantic_hint() {
    let node = IntentNode::LatentIntent {
        values: vec![0.1, 0.2, 0.3, 0.4].into_boxed_slice(),
        semantic_hint: Some("grasp-like".into()),
    };
    assert_eq!(node.to_lol(), "latent(0.1, 0.2, 0.3, 0.4)");
    assert!(matches!(
        intent_of(&node.to_lol()),
        IntentNode::LatentIntent {
            semantic_hint: None,
            ..
        }
    ));
}

#[test]
fn entity_id_out_of_range_is_error() {
    let err = parse_program("program(sphere(1.0), entities(sphere(0.1)), release(1))").unwrap_err();
    assert!(err.message.contains("out of range"), "{err}");
    let err = parse_program("program(sphere(1.0), entities(), grasp(0, right, 1.0))").unwrap_err();
    assert!(err.message.contains("out of range"), "{err}");
}

#[test]
fn integer_slots_reject_fractions_and_negatives() {
    for src in [
        "release(0.5)",
        "release(-1)",
        "rest(1.5)",
        "gaze(0.0, 0.0, 0.0, 2.5)",
        "music(0, 1, 2, 3, 4, 5, 6, 256)",
        "music(0, 1, 2, 3, 4, 5, 6, -1)",
    ] {
        let full = format!("program(sphere(1.0), entities(sphere(0.1)), {src})");
        assert!(parse_program(&full).is_err(), "expected error for `{src}`");
    }
}

#[test]
fn malformed_program_is_error() {
    for src in [
        "program()",
        "program(sphere(1.0), sphere(0.1))", // entities keyword missing
        "program(sphere(1.0), entities(), nope(1))", // unknown verb
        "program(sphere(1.0), entities(), grasp(0, up, 1.0))",
        "program(sphere(1.0), entities(), latent(0.1, 0.2, 0.3))",
        "program(sphere(1.0), entities(), music(0, 1, 2))",
        "program(sphere(1.0), entities(), rest(1)) trailing",
        "program(sphere(1.0), entities(), rotate(0, 0.0, 1.0, 0.0, 1.0))",
    ] {
        assert!(parse_program(src).is_err(), "expected error for `{src}`");
    }
}

#[test]
fn parse_lol_rejects_program_wrapper() {
    // `parse_lol` の戻り型は SdfNode なので program(...) は受け付けない
    let err = parse_lol("program(sphere(1.0))").unwrap_err();
    assert!(err.message.contains("unknown"), "{err}");
}

#[test]
fn program_type_is_reexported_shape() {
    let p: Program = parse_program("program(sphere(1.0), entities(sphere(0.1)), rest(1))").unwrap();
    assert_eq!(p.registry_len(), 1);
    assert!(p.get(0).is_some());
    assert!(p.get(1).is_none());
}

#[test]
fn llm_reference_example_parses() {
    // LLM_REFERENCE.md "Phase 3 Intent" section の example と同一
    let src = r"program(
  box3d(1.0, 0.05, 0.6),                     // table top
  entities(box3d(0.05, 0.05, 0.05)),         // id 0: small box
  seq(
    walk(0.0, 0.0, 0.8, 1.0),
    grasp(0, right, 5.0),
    rest(300),
    release(0)
  )
)";
    let p = parse_program(src).unwrap();
    assert_eq!(p.registry_len(), 1);
    assert!(matches!(p.intent, Some(IntentNode::Sequence(ref v)) if v.len() == 4));
}
