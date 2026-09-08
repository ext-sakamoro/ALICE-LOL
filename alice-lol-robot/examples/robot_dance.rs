//! Sequence / Parallel 合成 demo
//!
//! LOL Program の合成 verb (`Sequence` / `Parallel`) が Kinematics packet に flatten される
//! 挙動と humanoid 側の逐次適用を確認
//!
//! 実行:
//! ```bash
//! cargo run --example robot_dance -p alice-lol-robot
//! ```

use alice_lol::intent::{grasp, parallel, release, rest, sequence, walk, HandSide, ProgramBuilder};
use alice_lol::SdfNode;
use alice_lol_humanoid::{HumanoidTemplate, Joint};
use alice_lol_robot::{HumanoidIntent, IntentPacketStream, SafetyLaw};
use glam::Vec3;
use std::sync::Arc;

fn main() {
    println!("=== alice-lol-robot: Sequence / Parallel composition demo ===\n");

    let humanoid = HumanoidTemplate::default();

    // 対象カップを (0.6, 1.0, 0.2) に置く
    let cup = SdfNode::Translate {
        child: Arc::new(SdfNode::Sphere { radius: 0.04 }),
        offset: Vec3::new(0.6, 1.0, 0.2),
    };
    let mut builder = ProgramBuilder::new().with_sdf(humanoid.to_sdf(0.15));
    let cup_id = builder.register(cup);

    // Sequence: walk → grasp → walk → release → rest
    let dance = sequence(vec![
        walk(Vec3::new(0.5, 0.0, 0.0), 1.0),
        grasp(cup_id, HandSide::Right, 3.0),
        walk(Vec3::new(2.5, 0.0, 0.5), 1.2),
        release(cup_id),
        rest(500),
    ]);
    // Parallel 例: 歩きながら休む (concurrency semantic 情報は Kinematics flatten で失われる)
    let dance_with_par = sequence(vec![
        dance,
        parallel(vec![walk(Vec3::new(0.0, 0.0, 0.0), 0.8), rest(1000)]),
    ]);

    let program = builder.with_intent(dance_with_par).build();

    // safety check
    let law = SafetyLaw::default_collab();
    let violations = law.check_program(&program);
    println!("safety violations: {}", violations.len());
    for v in &violations {
        println!("  [{}] {}", v.rule, v.detail);
    }
    if !violations.is_empty() {
        return;
    }

    // humanoid apply (Sequence = fold left、Parallel = 先頭のみ)
    let final_pose = program
        .intent
        .as_ref()
        .map_or_else(|| humanoid.clone(), |i| humanoid.apply_intent(i).unwrap());
    let waist = final_pose.joints[&Joint::Waist];
    println!(
        "\nfinal waist after dance: ({:.2}, {:.2}, {:.2})",
        waist[0], waist[1], waist[2]
    );

    // packet stream
    let packets = IntentPacketStream::from_program(&program).unwrap();
    println!(
        "\npackets: {} × 8 byte = {} bytes",
        packets.len(),
        IntentPacketStream::total_bytes(&packets)
    );

    let decoded = IntentPacketStream::decode_all(&packets);
    for (i, intent) in decoded.iter().enumerate() {
        println!(
            "  [{i}] type={:?} target=({:.2}, {:.2}, {:.2}) duration={}ms",
            intent.flags.intent_type(),
            intent.target.x,
            intent.target.y,
            intent.target.z,
            intent.duration_ms
        );
    }

    println!("\n=== done ===");
}
