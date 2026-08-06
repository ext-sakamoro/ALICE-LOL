//! # BVH (Biovision Hierarchy) motion capture parser
//!
//! Mixamo / CMU / Blender export 等 universal motion library を読み込み、frame ごとの
//! joint local rotation を [`Quat`] map として抽出する
//!
//! # 対応範囲
//!
//! - HIERARCHY section (recursive joint tree + `OFFSET` + `CHANNELS`)
//! - MOTION section (frames × N floats、`Frame Time`)
//! - End Site (leaf joint、position only、no channels)
//! - Position channels (root joint のみ通常)
//! - Rotation channels (`Xrotation` / `Yrotation` / `Zrotation`、任意順)
//!
//! # 座標系
//!
//! 右手系、Y up (BVH の慣例) 単位は BVH file 依存 (通常 cm)
//! VRM / LOL は m + Y up、rotation は unit-less なので直接使用可
//!
//! # Rotation composition
//!
//! Blender / Maya / `MotionBuilder` 慣例に従い channels 順に right-multiply:
//! `M = R_channels[0] * R_channels[1] * R_channels[2]`
//! Point transform `v' = M * v` において `channels[N-1]` が最初に v に適用される
//!
//! # 使い方
//!
//! ```no_run
//! use alice_lol_humanoid::bvh::{BvhFile, mixamo_to_vrm};
//!
//! let bvh = BvhFile::load("motion.bvh").unwrap();
//! println!("Frames: {}, dt: {}s", bvh.animation.frame_count, bvh.animation.frame_time_s);
//!
//! // Frame 30 の rotation を Mixamo naming → VRM naming で取得
//! let map = mixamo_to_vrm();
//! let rotations = bvh.frame_rotations_named(30, |bvh_name| map.get(bvh_name).cloned());
//! ```
//!
//! # H.4 実装ノート (duplication)
//!
//! BVH parser + bone map converters は ALICE-Manga `src/bvh_import.rs` の code copy
//! Phase H.5 で reconciliation 判断

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use glam::{Quat, Vec3};

// ============================================================================
// BvhChannel
// ============================================================================

/// BVH channel type — 各 joint の 1 自由度
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BvhChannel {
    Xposition,
    Yposition,
    Zposition,
    Xrotation,
    Yrotation,
    Zrotation,
}

impl BvhChannel {
    /// 文字列トークンから変換 (`"Xposition"` → `Xposition` 等)
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Xposition" => Some(Self::Xposition),
            "Yposition" => Some(Self::Yposition),
            "Zposition" => Some(Self::Zposition),
            "Xrotation" => Some(Self::Xrotation),
            "Yrotation" => Some(Self::Yrotation),
            "Zrotation" => Some(Self::Zrotation),
            _ => None,
        }
    }

    /// rotation 系 channel か
    #[must_use]
    pub const fn is_rotation(self) -> bool {
        matches!(self, Self::Xrotation | Self::Yrotation | Self::Zrotation)
    }

    /// position 系 channel か
    #[must_use]
    pub const fn is_position(self) -> bool {
        matches!(self, Self::Xposition | Self::Yposition | Self::Zposition)
    }
}

// ============================================================================
// BvhJoint
// ============================================================================

/// BVH joint node — hierarchy tree の各節点
#[derive(Debug, Clone)]
pub struct BvhJoint {
    /// joint name (BVH file 内で unique、End Site は `"<parent>_End"` 形式)
    pub name: String,
    /// parent からの local offset (bind pose、右手系 Y up)
    pub offset: [f32; 3],
    /// この joint の channels (End Site は empty)
    pub channels: Vec<BvhChannel>,
    /// child joints (End Site は leaf として含まれる)
    pub children: Vec<Self>,
    /// End Site (leaf、rotation なし) フラグ
    pub is_end_site: bool,
    /// flat frame data における channel start index
    pub channel_offset: usize,
}

impl BvhJoint {
    /// 全 joint を depth-first pre-order で iterate (End Site 含む)
    #[must_use]
    pub fn iter_all(&self) -> BvhJointIter<'_> {
        BvhJointIter::new(self)
    }

    /// 名前で joint を検索 (recursive)
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&Self> {
        if self.name == name {
            return Some(self);
        }
        for child in &self.children {
            if let Some(j) = child.find(name) {
                return Some(j);
            }
        }
        None
    }
}

/// [`BvhJoint::iter_all`] の depth-first iterator
pub struct BvhJointIter<'a> {
    stack: Vec<&'a BvhJoint>,
}

impl<'a> BvhJointIter<'a> {
    fn new(root: &'a BvhJoint) -> Self {
        Self { stack: vec![root] }
    }
}

impl<'a> Iterator for BvhJointIter<'a> {
    type Item = &'a BvhJoint;

    fn next(&mut self) -> Option<Self::Item> {
        let j = self.stack.pop()?;
        // push children in reverse so pop yields them left-to-right
        for c in j.children.iter().rev() {
            self.stack.push(c);
        }
        Some(j)
    }
}

// ============================================================================
// BvhSkeleton + BvhAnimation + BvhFile
// ============================================================================

/// BVH hierarchy 全体
#[derive(Debug, Clone)]
pub struct BvhSkeleton {
    /// root joint
    pub root: BvhJoint,
    /// 全 joint 合計 channel 数 (= per-frame value count)
    pub total_channels: usize,
}

/// BVH animation (motion) data
#[derive(Debug, Clone)]
pub struct BvhAnimation {
    /// frame 数
    pub frame_count: usize,
    /// frame 間隔 (秒)
    pub frame_time_s: f32,
    /// flat frame data (`frames[frame_idx * total_channels + channel_offset]`)
    pub frames: Vec<f32>,
}

/// BVH file 全体 (skeleton + animation)
#[derive(Debug, Clone)]
pub struct BvhFile {
    pub skeleton: BvhSkeleton,
    pub animation: BvhAnimation,
}

// ============================================================================
// BvhError
// ============================================================================

/// BVH parse / IO エラー
#[derive(Debug)]
pub enum BvhError {
    /// I/O 失敗
    Io(std::io::Error),
    /// 想定外の EOF
    UnexpectedEof,
    /// 期待 token と実 token の不一致
    UnexpectedToken { expected: String, got: String },
    /// 不正な channel 名
    InvalidChannel(String),
    /// 不正な数値
    InvalidNumber(String),
    /// frame data 個数不足
    FrameDataShortage { expected: usize, got: usize },
}

impl std::fmt::Display for BvhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "BVH IO error: {e}"),
            Self::UnexpectedEof => write!(f, "unexpected end of BVH data"),
            Self::UnexpectedToken { expected, got } => {
                write!(f, "expected '{expected}', got '{got}'")
            }
            Self::InvalidChannel(s) => write!(f, "invalid BVH channel: '{s}'"),
            Self::InvalidNumber(s) => write!(f, "invalid BVH number: '{s}'"),
            Self::FrameDataShortage { expected, got } => write!(
                f,
                "BVH frame data shortage: expected {expected} values, got {got}"
            ),
        }
    }
}

impl std::error::Error for BvhError {}

// ============================================================================
// Parser (private)
// ============================================================================

struct Parser<'a> {
    tokens: Vec<&'a str>,
    idx: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        let mut tokens = Vec::new();
        for line in src.lines() {
            for part in line.split_whitespace() {
                tokens.push(part);
            }
        }
        Self { tokens, idx: 0 }
    }

    fn peek(&self) -> Option<&'a str> {
        self.tokens.get(self.idx).copied()
    }

    fn advance(&mut self) -> Option<&'a str> {
        let t = self.tokens.get(self.idx).copied();
        self.idx += 1;
        t
    }

    fn expect(&mut self, s: &str) -> Result<(), BvhError> {
        let got = self.advance().ok_or(BvhError::UnexpectedEof)?;
        if got != s {
            return Err(BvhError::UnexpectedToken {
                expected: s.into(),
                got: got.into(),
            });
        }
        Ok(())
    }

    fn parse_number(&mut self) -> Result<f32, BvhError> {
        let s = self.advance().ok_or(BvhError::UnexpectedEof)?;
        s.parse::<f32>()
            .map_err(|_| BvhError::InvalidNumber(s.into()))
    }

    fn parse_int(&mut self) -> Result<usize, BvhError> {
        let s = self.advance().ok_or(BvhError::UnexpectedEof)?;
        s.parse::<usize>()
            .map_err(|_| BvhError::InvalidNumber(s.into()))
    }

    fn parse(&mut self) -> Result<BvhFile, BvhError> {
        self.expect("HIERARCHY")?;
        self.expect("ROOT")?;
        let mut channel_counter = 0;
        let root = self.parse_joint(&mut channel_counter)?;
        let skeleton = BvhSkeleton {
            root,
            total_channels: channel_counter,
        };

        self.expect("MOTION")?;
        self.expect("Frames:")?;
        let frame_count = self.parse_int()?;
        self.expect("Frame")?;
        self.expect("Time:")?;
        let frame_time_s = self.parse_number()?;

        let expected_values = frame_count * skeleton.total_channels;
        let mut frames = Vec::with_capacity(expected_values);
        for _ in 0..expected_values {
            match self.parse_number() {
                Ok(v) => frames.push(v),
                Err(BvhError::UnexpectedEof) => {
                    return Err(BvhError::FrameDataShortage {
                        expected: expected_values,
                        got: frames.len(),
                    });
                }
                Err(e) => return Err(e),
            }
        }

        Ok(BvhFile {
            skeleton,
            animation: BvhAnimation {
                frame_count,
                frame_time_s,
                frames,
            },
        })
    }

    fn parse_joint(&mut self, channel_counter: &mut usize) -> Result<BvhJoint, BvhError> {
        // ROOT / JOINT keyword は呼び出し側で消費済、次は name
        let name = self.advance().ok_or(BvhError::UnexpectedEof)?.to_string();
        self.expect("{")?;

        let mut offset = [0.0_f32; 3];
        let mut channels: Vec<BvhChannel> = Vec::new();
        let mut children: Vec<BvhJoint> = Vec::new();
        let mut channel_offset = 0_usize;

        loop {
            let tok = self.peek().ok_or(BvhError::UnexpectedEof)?;
            match tok {
                "OFFSET" => {
                    self.advance();
                    offset = [
                        self.parse_number()?,
                        self.parse_number()?,
                        self.parse_number()?,
                    ];
                }
                "CHANNELS" => {
                    self.advance();
                    let n = self.parse_int()?;
                    channel_offset = *channel_counter;
                    for _ in 0..n {
                        let ch_str = self.advance().ok_or(BvhError::UnexpectedEof)?;
                        let ch = BvhChannel::parse(ch_str)
                            .ok_or_else(|| BvhError::InvalidChannel(ch_str.into()))?;
                        channels.push(ch);
                    }
                    *channel_counter += n;
                }
                "JOINT" => {
                    self.advance();
                    let child = self.parse_joint(channel_counter)?;
                    children.push(child);
                }
                "End" => {
                    self.advance();
                    self.expect("Site")?;
                    self.expect("{")?;
                    self.expect("OFFSET")?;
                    let end_offset = [
                        self.parse_number()?,
                        self.parse_number()?,
                        self.parse_number()?,
                    ];
                    self.expect("}")?;
                    children.push(BvhJoint {
                        name: format!("{name}_End"),
                        offset: end_offset,
                        channels: Vec::new(),
                        children: Vec::new(),
                        is_end_site: true,
                        channel_offset: 0,
                    });
                }
                "}" => {
                    self.advance();
                    break;
                }
                _ => {
                    return Err(BvhError::UnexpectedToken {
                        expected: "OFFSET|CHANNELS|JOINT|End|}".into(),
                        got: tok.into(),
                    });
                }
            }
        }

        Ok(BvhJoint {
            name,
            offset,
            channels,
            children,
            is_end_site: false,
            channel_offset,
        })
    }
}

// ============================================================================
// BvhFile impl
// ============================================================================

impl FromStr for BvhFile {
    type Err = BvhError;

    fn from_str(src: &str) -> Result<Self, Self::Err> {
        Parser::new(src).parse()
    }
}

impl BvhFile {
    /// BVH file をパス指定でロード
    ///
    /// # Errors
    ///
    /// - [`BvhError::Io`]: file 読み込み失敗
    /// - その他 parse エラー ([`BvhError`] variants 参照)
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, BvhError> {
        let src = fs::read_to_string(path).map_err(BvhError::Io)?;
        Self::from_str(&src)
    }

    /// Frame の flat data slice を取得
    #[must_use]
    pub fn frame_slice(&self, frame_idx: usize) -> Option<&[f32]> {
        if frame_idx >= self.animation.frame_count {
            return None;
        }
        let stride = self.skeleton.total_channels;
        let start = frame_idx * stride;
        Some(&self.animation.frames[start..start + stride])
    }

    /// 各 joint の local rotation quaternion を frame から抽出、`bone_map` で BVH joint name を
    /// 任意 (VRM etc) の名前に再マッピングする
    ///
    /// `bone_map` は BVH joint name → target name (`Option`) の変換関数
    /// `None` を返す joint は結果に含まれない
    #[must_use]
    pub fn frame_rotations_named<F>(&self, frame_idx: usize, bone_map: F) -> HashMap<String, Quat>
    where
        F: Fn(&str) -> Option<String>,
    {
        let mut out = HashMap::new();
        let Some(frame) = self.frame_slice(frame_idx) else {
            return out;
        };
        for joint in self.skeleton.root.iter_all() {
            if joint.is_end_site || joint.channels.is_empty() {
                continue;
            }
            let rot = compose_rotation(&joint.channels, joint.channel_offset, frame);
            if let Some(target_name) = bone_map(&joint.name) {
                out.insert(target_name, rot);
            }
        }
        out
    }

    /// Root joint の position channel (`Xposition` / `Yposition` / `Zposition`) を frame から抽出
    /// position channel なしなら `None`
    #[must_use]
    pub fn frame_root_position(&self, frame_idx: usize) -> Option<Vec3> {
        let frame = self.frame_slice(frame_idx)?;
        let root = &self.skeleton.root;
        let mut x = None;
        let mut y = None;
        let mut z = None;
        for (i, ch) in root.channels.iter().enumerate() {
            let val = frame[root.channel_offset + i];
            match ch {
                BvhChannel::Xposition => x = Some(val),
                BvhChannel::Yposition => y = Some(val),
                BvhChannel::Zposition => z = Some(val),
                _ => {}
            }
        }
        match (x, y, z) {
            (Some(x), Some(y), Some(z)) => Some(Vec3::new(x, y, z)),
            _ => None,
        }
    }
}

/// channels 順に right-multiply で Quat を合成 (Blender / Maya / `MotionBuilder` 慣例)
///
/// Result `q = q_channels[0] * q_channels[1] * ... * q_channels[N-1]`
/// Point transform `v' = q * v * q^-1` では `channels[N-1]` が最初に v に適用される
fn compose_rotation(channels: &[BvhChannel], channel_offset: usize, frame: &[f32]) -> Quat {
    let mut q = Quat::IDENTITY;
    for (i, ch) in channels.iter().enumerate() {
        if !ch.is_rotation() {
            continue;
        }
        let val_deg = frame[channel_offset + i];
        let rad = val_deg.to_radians();
        let q_ch = match ch {
            BvhChannel::Xrotation => Quat::from_axis_angle(Vec3::X, rad),
            BvhChannel::Yrotation => Quat::from_axis_angle(Vec3::Y, rad),
            BvhChannel::Zrotation => Quat::from_axis_angle(Vec3::Z, rad),
            _ => continue,
        };
        q *= q_ch;
    }
    q
}

// ============================================================================
// Bone mapping tables (Mixamo / CMU / Blender ↔ VRM humanoid)
// ============================================================================

/// Mixamo BVH joint name → VRM humanoid bone name の mapping
///
/// Mixamo は `"mixamorig:<BoneName>"` 形式、prefix 除去して VRM naming にマップ
/// prefix なし (Blender で prefix strip 済) も許容
#[must_use]
pub fn mixamo_to_vrm() -> HashMap<String, String> {
    let mut m = HashMap::new();
    let pairs: &[(&str, &str)] = &[
        ("Hips", "hips"),
        ("Spine", "spine"),
        ("Spine1", "chest"),
        ("Spine2", "upperChest"),
        ("Neck", "neck"),
        ("Head", "head"),
        ("LeftShoulder", "leftShoulder"),
        ("LeftArm", "leftUpperArm"),
        ("LeftForeArm", "leftLowerArm"),
        ("LeftHand", "leftHand"),
        ("RightShoulder", "rightShoulder"),
        ("RightArm", "rightUpperArm"),
        ("RightForeArm", "rightLowerArm"),
        ("RightHand", "rightHand"),
        ("LeftUpLeg", "leftUpperLeg"),
        ("LeftLeg", "leftLowerLeg"),
        ("LeftFoot", "leftFoot"),
        ("LeftToeBase", "leftToes"),
        ("RightUpLeg", "rightUpperLeg"),
        ("RightLeg", "rightLowerLeg"),
        ("RightFoot", "rightFoot"),
        ("RightToeBase", "rightToes"),
    ];
    for (bvh, vrm) in pairs {
        m.insert(format!("mixamorig:{bvh}"), (*vrm).to_string());
        m.insert((*bvh).to_string(), (*vrm).to_string()); // prefix なしも許容
    }
    m
}

/// CMU Motion Capture Database BVH joint name → VRM humanoid bone name の mapping
///
/// CMU の rig は Mixamo とほぼ同じ naming だが hip joint 名が異なる (`LHipJoint` 等)
#[must_use]
pub fn cmu_to_vrm() -> HashMap<String, String> {
    let mut m = HashMap::new();
    let pairs: &[(&str, &str)] = &[
        ("Hips", "hips"),
        ("LowerBack", "spine"),
        ("Spine", "chest"),
        ("Spine1", "upperChest"),
        ("Neck", "neck"),
        ("Neck1", "neck"),
        ("Head", "head"),
        ("LeftShoulder", "leftShoulder"),
        ("LeftArm", "leftUpperArm"),
        ("LeftForeArm", "leftLowerArm"),
        ("LeftHand", "leftHand"),
        ("RightShoulder", "rightShoulder"),
        ("RightArm", "rightUpperArm"),
        ("RightForeArm", "rightLowerArm"),
        ("RightHand", "rightHand"),
        ("LHipJoint", "leftUpperLeg"),
        ("LeftUpLeg", "leftUpperLeg"),
        ("LeftLeg", "leftLowerLeg"),
        ("LeftFoot", "leftFoot"),
        ("LeftToeBase", "leftToes"),
        ("RHipJoint", "rightUpperLeg"),
        ("RightUpLeg", "rightUpperLeg"),
        ("RightLeg", "rightLowerLeg"),
        ("RightFoot", "rightFoot"),
        ("RightToeBase", "rightToes"),
    ];
    for (bvh, vrm) in pairs {
        m.insert((*bvh).to_string(), (*vrm).to_string());
    }
    m
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_BVH: &str = "\
HIERARCHY
ROOT Hips
{
    OFFSET 0.0 0.0 0.0
    CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation
    JOINT LeftUpLeg
    {
        OFFSET 1.0 -1.0 0.0
        CHANNELS 3 Zrotation Xrotation Yrotation
        End Site
        {
            OFFSET 0.0 -0.5 0.0
        }
    }
}
MOTION
Frames: 2
Frame Time: 0.033333
0.0 10.0 0.0 0.0 0.0 0.0 30.0 0.0 0.0
1.0 11.0 2.0 5.0 0.0 0.0 -20.0 10.0 0.0
";

    #[test]
    fn bvh_parse_minimal_hierarchy_and_motion() {
        let bvh: BvhFile = MINIMAL_BVH.parse().expect("minimal BVH should parse");
        assert_eq!(bvh.skeleton.root.name, "Hips");
        assert_eq!(bvh.skeleton.root.channels.len(), 6);
        assert_eq!(bvh.skeleton.total_channels, 9);
        assert_eq!(bvh.skeleton.root.children.len(), 1);
        assert_eq!(bvh.skeleton.root.children[0].name, "LeftUpLeg");
        assert_eq!(bvh.animation.frame_count, 2);
        assert!((bvh.animation.frame_time_s - 0.033_333).abs() < 1e-4);
        assert_eq!(bvh.animation.frames.len(), 18);
    }

    #[test]
    fn bvh_frame_slice_bounds() {
        let bvh: BvhFile = MINIMAL_BVH.parse().unwrap();
        assert!(bvh.frame_slice(0).is_some());
        assert!(bvh.frame_slice(1).is_some());
        assert!(bvh.frame_slice(2).is_none());
        assert_eq!(bvh.frame_slice(0).unwrap().len(), 9);
    }

    #[test]
    fn bvh_frame_root_position_extracts_xyz() {
        let bvh: BvhFile = MINIMAL_BVH.parse().unwrap();
        let pos = bvh
            .frame_root_position(1)
            .expect("root has position channels");
        assert!((pos.x - 1.0).abs() < 1e-4);
        assert!((pos.y - 11.0).abs() < 1e-4);
        assert!((pos.z - 2.0).abs() < 1e-4);
    }

    #[test]
    fn bvh_frame_rotations_named_returns_expected_joints() {
        let bvh: BvhFile = MINIMAL_BVH.parse().unwrap();
        // identity mapper (BVH name → same name)
        let rotations = bvh.frame_rotations_named(0, |n| Some(n.to_string()));
        assert!(rotations.contains_key("Hips"));
        assert!(rotations.contains_key("LeftUpLeg"));
        // End Site は含まれない
        assert!(!rotations.contains_key("LeftUpLeg_End"));
    }

    #[test]
    fn bvh_frame_rotations_named_zero_yields_identity() {
        // frame 0 は全 rotation 0 → Quat::IDENTITY
        let bvh: BvhFile = MINIMAL_BVH.parse().unwrap();
        let rotations = bvh.frame_rotations_named(0, |n| Some(n.to_string()));
        let hips = rotations["Hips"];
        assert!(hips.abs_diff_eq(Quat::IDENTITY, 1e-4));
    }

    #[test]
    fn mixamo_to_vrm_maps_key_joints() {
        let m = mixamo_to_vrm();
        assert_eq!(m.get("Hips").map(String::as_str), Some("hips"));
        assert_eq!(m.get("LeftArm").map(String::as_str), Some("leftUpperArm"));
        // prefix 版
        assert_eq!(m.get("mixamorig:Hips").map(String::as_str), Some("hips"));
        assert_eq!(
            m.get("mixamorig:LeftForeArm").map(String::as_str),
            Some("leftLowerArm")
        );
    }

    #[test]
    fn cmu_to_vrm_maps_key_joints() {
        let m = cmu_to_vrm();
        assert_eq!(m.get("LHipJoint").map(String::as_str), Some("leftUpperLeg"));
        assert_eq!(
            m.get("RHipJoint").map(String::as_str),
            Some("rightUpperLeg")
        );
        assert_eq!(m.get("Hips").map(String::as_str), Some("hips"));
    }

    #[test]
    fn bvh_channel_parse_all_variants() {
        assert_eq!(BvhChannel::parse("Xposition"), Some(BvhChannel::Xposition));
        assert_eq!(BvhChannel::parse("Yrotation"), Some(BvhChannel::Yrotation));
        assert_eq!(BvhChannel::parse("invalid"), None);
        assert!(BvhChannel::Xrotation.is_rotation());
        assert!(BvhChannel::Xposition.is_position());
        assert!(!BvhChannel::Xrotation.is_position());
    }

    #[test]
    fn bvh_error_on_missing_hierarchy() {
        let result: Result<BvhFile, _> = "MOTION\nFrames: 0\nFrame Time: 0.03\n".parse();
        match result {
            Err(BvhError::UnexpectedToken { expected, .. }) => {
                assert_eq!(expected, "HIERARCHY");
            }
            other => panic!("expected UnexpectedToken, got {other:?}"),
        }
    }
}
