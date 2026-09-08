//! Grasp verb → humanoid + 8-byte packet demo
//!
//! LOL Program (`ProgramBuilder` + `grasp` verb + target entity) を組み立てて
//! - `SafetyLaw::guard_program` で静的検査
//! - `HumanoidIntent::apply_intent` で humanoid pose 反映 (MVP: Grasp は変化なし)
//! - `IntentPacketStream::from_program` で 8-byte packet 群を出力
//!
//! 実行:
//! ```bash
//! cargo run --example robot_grasp -p alice-lol-robot
//! ```

use alice_lol::intent::{grasp, HandSide, ProgramBuilder};
use alice_lol::SdfNode;
use alice_lol_humanoid::HumanoidTemplate;
use alice_lol_robot::{HumanoidIntent, IntentPacketStream, SafetyLaw};
use glam::Vec3;
use std::sync::Arc;

fn main() {
    println!("=== alice-lol-robot: Grasp verb demo ===\n");

    // Step 1: humanoid template (10 頭身 canonical)
    let humanoid = HumanoidTemplate::default();
    println!(
        "humanoid: {} joints, waist at {:?}",
        humanoid.joints.len(),
        humanoid.joints[&alice_lol_humanoid::Joint::Waist]
    );

    // Step 2: LOL Program 組立 — 対象カップを右手で掴む
    let cup = SdfNode::Translate {
        child: Arc::new(SdfNode::Sphere { radius: 0.05 }),
        offset: Vec3::new(0.5, 1.0, 0.3),
    };
    let mut builder = ProgramBuilder::new().with_sdf(humanoid.to_sdf(0.15));
    let cup_id = builder.register(cup);
    let program = builder
        .with_intent(grasp(cup_id, HandSide::Right, 3.0))
        .build();

    println!(
        "\nProgram: sdf_registry = {} entities, intent = {}",
        program.registry_len(),
        if program.has_intent() { "yes" } else { "no" }
    );

    // Step 3: SafetyLaw 静的検査
    let law = SafetyLaw::default_collab();
    match law.guard_program(&program) {
        Ok(()) => println!("safety: PASS (default collab robot params)"),
        Err(e) => {
            println!("safety: FAIL: {e}");
            return;
        }
    }

    // Step 4: humanoid pose 反映 (MVP: Grasp は変化なし、Phase R.6+ で IK)
    let posed = humanoid
        .apply_intent(&grasp(cup_id, HandSide::Right, 3.0))
        .unwrap();
    println!(
        "humanoid pose: {} joints (Grasp MVP = unchanged)",
        posed.joints.len()
    );

    // Step 5: 8-byte packet stream 出力
    let packets = IntentPacketStream::from_program(&program).unwrap();
    println!(
        "\npackets: {} × 8 byte = {} bytes total",
        packets.len(),
        IntentPacketStream::total_bytes(&packets)
    );

    for (i, packet) in packets.iter().enumerate() {
        print!("  packet[{i}]: ");
        for byte in packet {
            print!("{byte:02x} ");
        }
        println!();
    }

    // Step 6: packet decode round-trip 検証
    let decoded = IntentPacketStream::decode_all(&packets);
    for (i, intent) in decoded.iter().enumerate() {
        println!(
            "  decoded[{i}]: type={:?} target=({:.2}, {:.2}, {:.2}) duration={}ms",
            intent.flags.intent_type(),
            intent.target.x,
            intent.target.y,
            intent.target.z,
            intent.duration_ms
        );
    }

    println!("\n=== done ===");
}
