//! An intent plan over a scene: the verbs and their composition, built with the `intent` constructors
//!
//! A robot runtime (such as `alice-lol-robot` in ALICE-Kinematics) takes a `Program` like this one and
//! executes its intent tree: walk to the table, pick the cup with the right hand while the left hand
//! picks the lid, then put both down.

use alice_lol::intent::{grasp, parallel, release, sequence, walk, HandSide, Program};
use alice_lol::{lol, Vec3};

fn main() {
    // the entities the verbs name by index: 0 = cup, 1 = lid
    let cup = lol! { cylinder(0.04, 0.05) };
    let lid = lol! { cylinder(0.045, 0.005) };
    let plan = sequence(vec![
        walk(Vec3::new(1.2, 0.0, 0.4), 0.6),
        parallel(vec![
            grasp(0, HandSide::Right, 4.0),
            grasp(1, HandSide::Left, 1.5),
        ]),
        parallel(vec![release(0), release(1)]),
    ]);
    let program = Program {
        sdf: lol! { box3d(0.6, 0.02, 0.4) },
        sdf_registry: vec![cup, lid],
        intent: Some(plan),
    };

    println!("entities in the registry: {}", program.registry_len());
    println!("has an intent tree: {}", program.has_intent());
    if let Some(intent) = &program.intent {
        println!("the plan as LOL text:\n{}", intent.to_lol());
    }
}
