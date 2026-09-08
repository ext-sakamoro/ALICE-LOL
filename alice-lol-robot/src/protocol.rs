//! [`IntentPacketStream`] — 8-byte Kinematics packet の [`Vec`] エクスポート
//!
//! [`IntentExecutor::translate`] の結果を 8-byte packet 群 (`Vec<[u8; 8]>`) に serialize
//! ROS2 / RTMFP / メーカー独自 protocol への load 前段として使う
//!
//! # 未実装 (Phase R.6+)
//!
//! - `RobotProtocol` trait (ROS2 `sensor_msgs::msg::JointState` / URDF / URDF-XML export)
//! - Universal Robots RTDE / KUKA IPO / ABB EGM / FANUC PDM の specific adapter

use alice_kinematics::intent::Intent;
use alice_lol::intent::Program;

use crate::error::RobotError;
use crate::executor::IntentExecutor;

/// [`Program`] → 8-byte packet [`Vec`]
///
/// [`IntentExecutor::translate`] の結果を [`Intent::encode`] で serialize、
/// network transport 前段の canonical output
///
/// # Examples
///
/// ```ignore
/// use alice_lol_robot::IntentPacketStream;
///
/// let packets = IntentPacketStream::from_program(&program).unwrap();
/// for packet in &packets {
///     assert_eq!(packet.len(), 8);
///     // ROS2 / RTMFP / 独自 protocol に流す
/// }
/// ```
pub struct IntentPacketStream;

impl IntentPacketStream {
    /// [`Program`] の Intent tree を 8-byte packet [`Vec`] に変換
    ///
    /// # Errors
    ///
    /// - [`RobotError::Unmappable`]: verb が Kinematics scope 外
    /// - [`RobotError::OutOfRange`]: [`alice_lol::intent::NodeId`] が `sdf_registry` の
    ///   range 外
    pub fn from_program(program: &Program) -> Result<Vec<[u8; 8]>, RobotError> {
        let executor = IntentExecutor::from_program(program);
        let intents = executor.translate()?;
        Ok(intents.iter().map(Intent::encode).collect())
    }

    /// Decode: packet [`Vec`] を [`Intent`] [`Vec`] に戻す (debug / testing 用)
    #[must_use]
    pub fn decode_all(packets: &[[u8; 8]]) -> Vec<Intent> {
        packets.iter().map(Intent::decode).collect()
    }

    /// packet total byte size
    #[must_use]
    pub const fn total_bytes(packets: &[[u8; 8]]) -> usize {
        packets.len() * 8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alice_lol::intent::{grasp, sequence, walk, HandSide, IntentNode, ProgramBuilder};
    use alice_lol::SdfNode;
    use glam::Vec3;
    use std::sync::Arc;

    fn make_program(intent: IntentNode) -> Program {
        let target = SdfNode::Translate {
            child: Arc::new(SdfNode::Sphere { radius: 0.05 }),
            offset: Vec3::new(0.5, 0.3, 0.2),
        };
        let mut builder = ProgramBuilder::new().with_sdf(SdfNode::Sphere { radius: 0.5 });
        let _id = builder.register(target);
        builder.with_intent(intent).build()
    }

    #[test]
    fn from_program_grasp_yields_single_packet() {
        let program = make_program(grasp(0, HandSide::Right, 3.0));
        let packets = IntentPacketStream::from_program(&program).unwrap();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].len(), 8);
    }

    #[test]
    fn from_program_sequence_flattens() {
        let program = make_program(sequence(vec![
            grasp(0, HandSide::Right, 3.0),
            walk(Vec3::new(1.0, 0.0, 0.0), 1.0),
        ]));
        let packets = IntentPacketStream::from_program(&program).unwrap();
        assert_eq!(packets.len(), 2);
    }

    #[test]
    fn decode_all_roundtrips_from_encode() {
        let program = make_program(grasp(0, HandSide::Right, 3.0));
        let packets = IntentPacketStream::from_program(&program).unwrap();
        let decoded = IntentPacketStream::decode_all(&packets);
        assert_eq!(decoded.len(), 1);
        // Grasp verb は IntentType::Grasp に翻訳される
        assert_eq!(
            decoded[0].flags.intent_type(),
            alice_kinematics::intent::IntentType::Grasp
        );
    }

    #[test]
    fn total_bytes_is_8x_count() {
        let program = make_program(sequence(vec![
            grasp(0, HandSide::Right, 3.0),
            walk(Vec3::new(1.0, 0.0, 0.0), 1.0),
        ]));
        let packets = IntentPacketStream::from_program(&program).unwrap();
        assert_eq!(IntentPacketStream::total_bytes(&packets), 16);
    }

    #[test]
    fn no_intent_yields_empty_packet_vec() {
        let program = ProgramBuilder::new()
            .with_sdf(SdfNode::Sphere { radius: 0.5 })
            .build();
        let packets = IntentPacketStream::from_program(&program).unwrap();
        assert!(packets.is_empty());
    }
}
