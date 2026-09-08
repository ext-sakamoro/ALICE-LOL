//! Walk verb → humanoid translation + 8-byte packet demo
//!
//! `Walk` verb は humanoid crate の `apply_intent` で **実際に joint 位置が動く** 唯一の
//! MVP verb (他 verb は Phase R.6+ で IK 実装予定)
//!
//! 実行:
//! ```bash
//! cargo run --example robot_walk -p alice-lol-robot
//! ```

use alice_lol::intent::{walk, ProgramBuilder};
use alice_lol_humanoid::{HumanoidTemplate, Joint};
use alice_lol_robot::{HumanoidIntent, IntentPacketStream, SafetyLaw};
use glam::Vec3;

fn main() {
    println!("=== alice-lol-robot: Walk verb demo ===\n");

    let humanoid = HumanoidTemplate::default();
    let initial_waist = humanoid.joints[&Joint::Waist];
    let initial_head = humanoid.joints[&Joint::Head];
    println!(
        "initial waist: ({:.2}, {:.2}, {:.2})",
        initial_waist[0], initial_waist[1], initial_waist[2]
    );
    println!(
        "initial head:  ({:.2}, {:.2}, {:.2})",
        initial_head[0], initial_head[1], initial_head[2]
    );

    // Walk verb: (2.5, 0, 1.0) へ 1.0 m/s で移動
    let destination = Vec3::new(2.5, 0.0, 1.0);
    let intent = walk(destination, 1.0);

    // Safety check
    let law = SafetyLaw::default_collab();
    let violations = law.check_intent(&intent);
    if violations.is_empty() {
        println!("\nsafety: PASS (Walk to {destination:?} at 1.0 m/s)");
    } else {
        for v in &violations {
            println!("safety violation: [{}] {}", v.rule, v.detail);
        }
        return;
    }

    // humanoid pose 反映 (Walk = rigid body translation)
    let posed = humanoid.apply_intent(&intent).unwrap();
    let new_waist = posed.joints[&Joint::Waist];
    let new_head = posed.joints[&Joint::Head];
    println!("\nafter walk:");
    println!(
        "  new waist: ({:.2}, {:.2}, {:.2})",
        new_waist[0], new_waist[1], new_waist[2]
    );
    println!(
        "  new head:  ({:.2}, {:.2}, {:.2})",
        new_head[0], new_head[1], new_head[2]
    );
    println!(
        "  delta:     ({:.2}, {:.2}, {:.2})",
        new_waist[0] - initial_waist[0],
        new_waist[1] - initial_waist[1],
        new_waist[2] - initial_waist[2]
    );

    // 8-byte packet 出力 (Walk → Kinematics IntentType::Reach)
    let program = ProgramBuilder::new()
        .with_sdf(humanoid.to_sdf(0.15))
        .with_intent(intent)
        .build();
    let packets = IntentPacketStream::from_program(&program).unwrap();
    println!("\npackets: {} × 8 byte", packets.len());
    for (i, packet) in packets.iter().enumerate() {
        print!("  packet[{i}]: ");
        for byte in packet {
            print!("{byte:02x} ");
        }
        println!();
    }

    let decoded = IntentPacketStream::decode_all(&packets);
    for (i, intent) in decoded.iter().enumerate() {
        println!(
            "  decoded[{i}]: type={:?} target=({:.2}, {:.2}, {:.2})",
            intent.flags.intent_type(),
            intent.target.x,
            intent.target.y,
            intent.target.z
        );
    }

    println!("\n=== done ===");
}
