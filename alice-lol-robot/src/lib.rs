//! # alice-lol-robot
//!
//! Robot template for `alice_lol` DSL — Phase 3 Intent verb を humanoid + Kinematics に結線
//!
//! # 三相原理での位置付け
//!
//! Phase 3 Intent (`IntentNode` 14 verb) → humanoid FK + 8-byte Kinematics packet の
//! **結線層 (glue layer)** 既存資産:
//!
//! - `alice_lol::intent::IntentNode` (14 verb + Sequence/Parallel + Program)
//! - `alice_lol_humanoid::HumanoidTemplate` (16 joint FK + BVH/VRM import)
//! - `alice_kinematics::Intent` (8-byte packet、Reach/Point/Grasp/Release)
//! - `alice_kinematics::lol_bridge::intent_to_kinematics` (LOL → Kinematics 翻訳)
//!
//! 本 crate は上記 3 資産を **additive に glue** し、`IntentExecutor` として MVP 化
//! humanoid / kinematics / alice-lol いずれの crate も破壊変更なし
//!
//! # Roadmap (Phase R.0-R.5)
//!
//! - R.0 Scaffolding + Cargo.toml + CI stub (本 commit)
//! - R.1 [`executor::IntentExecutor`] core (LOL → Kinematics 翻訳)
//! - R.2 [`humanoid_ext::HumanoidIntent`] adapter (humanoid H.6 相当を additive)
//! - R.3 [`protocol::IntentPacketStream`] (8-byte packet Vec エクスポート)
//! - R.4 [`law::SafetyLaw`] (ISO 10218 参考 velocity/workspace/collision guard)
//! - R.5 examples + tests + polish
//!
//! # Quick start (Phase R.1+ で有効化予定)
//!
//! ```ignore
//! use alice_lol::intent::{grasp, HandSide, ProgramBuilder};
//! use alice_lol_humanoid::HumanoidTemplate;
//! use alice_lol_robot::IntentExecutor;
//!
//! let humanoid = HumanoidTemplate::default();
//! let intent = grasp(0, HandSide::Right, 3.0);
//! let program = ProgramBuilder::new()
//!     .with_sdf(humanoid.to_sdf(0.15))
//!     .with_intent(intent)
//!     .build();
//! let executor = IntentExecutor::new(humanoid, program);
//! let packets = executor.translate().unwrap();
//! // → Vec<alice_kinematics::Intent> (8-byte packet 群)
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod humanoid_ext;
pub mod law;

#[cfg(feature = "kinematics")]
pub mod executor;

#[cfg(feature = "kinematics")]
pub mod protocol;

pub use error::RobotError;
pub use humanoid_ext::HumanoidIntent;
pub use law::{SafetyLaw, SafetyViolation};

#[cfg(feature = "kinematics")]
pub use executor::IntentExecutor;

#[cfg(feature = "kinematics")]
pub use protocol::IntentPacketStream;
