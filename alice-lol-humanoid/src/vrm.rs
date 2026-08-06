//! # VRM import module (feature `vrm`)
//!
//! VRM 0.x binary GLB からhumanoid bone を抽出し [`HumanoidTemplate`] を構築する
//!
//! 対応 VRM version: **0.x のみ** (`extensions.VRM.humanoid.humanBones`)
//! VRM 1.0 (`VRMC_vrm` extension) は future work
//!
//! # 依存
//!
//! `serde_json` optional dep (feature `vrm` で opt-in) 自前 GLB JSON parser を持つため
//! `gltf` crate は不要
//!
//! # 使い方
//!
//! ```no_run
//! use alice_lol_humanoid::HumanoidTemplate;
//!
//! # #[cfg(feature = "vrm")]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! match HumanoidTemplate::from_vrm("character.vrm")? {
//!     Some(t) => println!("bones: {}, joints: {}", t.bones.len(), t.joints.len()),
//!     None => println!("VRM に humanoid bones が含まれていない (VRM 1.0 の可能性)"),
//! }
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "vrm"))]
//! # fn main() {}
//! ```
//!
//! # H.3 実装ノート (duplication)
//!
//! GLB JSON parser + node hierarchy walker + `extract_humanoid_bones` +
//! VRM bone name → [`Joint`] mapping は ALICE-Manga `src/vrm_import.rs` から抜粋
//! Phase H.5 で Manga / LOL 間の reconciliation 判断

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use glam::{Mat4, Quat, Vec3};

use crate::{canonical_bones, HumanoidTemplate, Joint};

// ============================================================================
// VrmError
// ============================================================================

/// VRM parser エラー
#[derive(Debug)]
pub enum VrmError {
    /// I/O エラー (path が存在しない / 読み込み失敗)
    Io(std::io::Error),
    /// VRM data / schema 不整合
    MissingData(String),
}

impl std::fmt::Display for VrmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::MissingData(s) => write!(f, "VRM data missing: {s}"),
        }
    }
}

impl std::error::Error for VrmError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::MissingData(_) => None,
        }
    }
}

impl From<std::io::Error> for VrmError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

// ============================================================================
// VrmBoneMap
// ============================================================================

/// VRM humanoid bone name → world position の map
///
/// [`extract_humanoid_bones`] の返り値 VRM 標準の bone name (英字 camelCase、
/// 例: `"hips"` / `"leftUpperArm"`) を key に、bind pose (T-pose) の world position を value とする
#[derive(Debug, Clone, Default)]
pub struct VrmBoneMap {
    /// bone name → world position
    pub bones: HashMap<String, Vec3>,
}

impl VrmBoneMap {
    /// 指定 bone の world position を取得
    #[must_use]
    pub fn get(&self, bone_name: &str) -> Option<Vec3> {
        self.bones.get(bone_name).copied()
    }

    /// 全 bone 数
    #[must_use]
    pub fn len(&self) -> usize {
        self.bones.len()
    }

    /// 空かどうか
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bones.is_empty()
    }

    /// 全 bone name list (順不同、debug 用途)
    #[must_use]
    pub fn bone_names(&self) -> Vec<&str> {
        self.bones.keys().map(String::as_str).collect()
    }
}

// ============================================================================
// extract_humanoid_bones
// ============================================================================

/// VRM 0.x binary GLB から humanoid bones の world position を抽出
///
/// # 戻り値
///
/// - `Some(VrmBoneMap)`: VRM extension が存在し 1 個以上の bone が抽出できた
/// - `None`: VRM extension 不在 (glTF 通常 file or VRM 1.0)、または humanoid.humanBones 欠落
///
/// # Errors
///
/// - [`VrmError::Io`]: file open / read 失敗
/// - [`VrmError::MissingData`]: GLB header 不正、JSON parse 失敗、`glTF nodes` 欠落
pub fn extract_humanoid_bones<P: AsRef<Path>>(path: P) -> Result<Option<VrmBoneMap>, VrmError> {
    let json = parse_glb_json(path.as_ref())?;
    let Some(vrm_ext) = json.get("extensions").and_then(|e| e.get("VRM")) else {
        return Ok(None);
    };
    let Some(human_bones) = vrm_ext
        .get("humanoid")
        .and_then(|h| h.get("humanBones"))
        .and_then(|hb| hb.as_array())
    else {
        return Ok(None);
    };
    let node_world_translations = compute_node_world_translations(&json)?;
    let mut map: HashMap<String, Vec3> = HashMap::new();
    for entry in human_bones {
        let Some(name) = entry.get("bone").and_then(|b| b.as_str()) else {
            continue;
        };
        let Some(node_idx_u64) = entry.get("node").and_then(serde_json::Value::as_u64) else {
            continue;
        };
        let Ok(node_idx) = usize::try_from(node_idx_u64) else {
            continue;
        };
        if let Some(pos) = node_world_translations.get(node_idx) {
            map.insert(name.to_string(), *pos);
        }
    }
    if map.is_empty() {
        Ok(None)
    } else {
        Ok(Some(VrmBoneMap { bones: map }))
    }
}

// ============================================================================
// GLB / JSON helpers (private)
// ============================================================================

/// GLB (binary glTF) file の header + JSON chunk を parse
///
/// GLB format:
/// - Header 12 byte (magic `"glTF"` + version u32 + total length u32)
/// - Chunk 1: 8 byte header (length u32 + type u32 `"JSON"`) + length bytes JSON
/// - Chunk 2 (optional): binary data
fn parse_glb_json(path: &Path) -> Result<serde_json::Value, VrmError> {
    let mut file = File::open(path)?;
    let mut header = [0_u8; 12];
    file.read_exact(&mut header)?;
    if &header[0..4] != b"glTF" {
        return Err(VrmError::MissingData(
            "not a GLB file (missing glTF magic)".into(),
        ));
    }
    let mut chunk_header = [0_u8; 8];
    file.read_exact(&mut chunk_header)?;
    let chunk_length_u32 = u32::from_le_bytes([
        chunk_header[0],
        chunk_header[1],
        chunk_header[2],
        chunk_header[3],
    ]);
    let chunk_length = chunk_length_u32 as usize;
    let chunk_type = &chunk_header[4..8];
    if chunk_type != b"JSON" {
        return Err(VrmError::MissingData(
            "first chunk is not JSON in GLB".into(),
        ));
    }
    let mut json_data = vec![0_u8; chunk_length];
    file.read_exact(&mut json_data)?;
    let json_str = std::str::from_utf8(&json_data)
        .map_err(|e| VrmError::MissingData(format!("JSON chunk not valid UTF-8: {e}")))?;
    let value: serde_json::Value =
        serde_json::from_str(json_str.trim_end_matches('\x00').trim())
            .map_err(|e| VrmError::MissingData(format!("JSON parse error: {e}")))?;
    Ok(value)
}

/// glTF JSON の node hierarchy を走査、各 node の world translation を計算
///
/// glTF spec: `nodes[i]` は `translation` / `rotation` / `scale` (TRS) または `matrix` を持つ
/// `children` 配列で forward-only tree を形成
fn compute_node_world_translations(json: &serde_json::Value) -> Result<Vec<Vec3>, VrmError> {
    let nodes = json
        .get("nodes")
        .and_then(|n| n.as_array())
        .ok_or_else(|| VrmError::MissingData("glTF nodes array missing".into()))?;
    let node_count = nodes.len();

    let local_transforms: Vec<Mat4> = nodes.iter().map(extract_local_transform).collect();

    let mut children_list: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    for (idx, node) in nodes.iter().enumerate() {
        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            for child_idx in children {
                let Some(ci_u64) = child_idx.as_u64() else {
                    continue;
                };
                let Ok(ci) = usize::try_from(ci_u64) else {
                    continue;
                };
                if ci < node_count {
                    children_list[idx].push(ci);
                }
            }
        }
    }

    let mut is_child = vec![false; node_count];
    for children in &children_list {
        for &child in children {
            is_child[child] = true;
        }
    }
    let roots: Vec<usize> = (0..node_count).filter(|&i| !is_child[i]).collect();

    let mut world_transforms = vec![Mat4::IDENTITY; node_count];
    for &root in &roots {
        walk_node_hierarchy(
            root,
            Mat4::IDENTITY,
            &local_transforms,
            &children_list,
            &mut world_transforms,
        );
    }

    let world_translations: Vec<Vec3> = world_transforms
        .iter()
        .map(|m| m.w_axis.truncate())
        .collect();
    Ok(world_translations)
}

/// 単一 node の local transform を Mat4 として抽出 (TRS または matrix)
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn extract_local_transform(node: &serde_json::Value) -> Mat4 {
    if let Some(matrix) = node.get("matrix").and_then(|m| m.as_array()) {
        if matrix.len() == 16 {
            let mut arr = [0.0_f32; 16];
            for (i, v) in matrix.iter().enumerate() {
                arr[i] = v.as_f64().unwrap_or(0.0) as f32;
            }
            return Mat4::from_cols_array(&arr);
        }
    }
    let translation =
        node.get("translation")
            .and_then(|t| t.as_array())
            .map_or(Vec3::ZERO, |arr| {
                Vec3::new(
                    arr.first()
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0) as f32,
                    arr.get(1)
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0) as f32,
                    arr.get(2)
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0) as f32,
                )
            });
    let rotation = node
        .get("rotation")
        .and_then(|r| r.as_array())
        .map_or(Quat::IDENTITY, |arr| {
            Quat::from_xyzw(
                arr.first()
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0) as f32,
                arr.get(1)
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0) as f32,
                arr.get(2)
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0) as f32,
                arr.get(3)
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(1.0) as f32,
            )
        });
    let scale = node
        .get("scale")
        .and_then(|s| s.as_array())
        .map_or(Vec3::ONE, |arr| {
            Vec3::new(
                arr.first()
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(1.0) as f32,
                arr.get(1)
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(1.0) as f32,
                arr.get(2)
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(1.0) as f32,
            )
        });
    Mat4::from_scale_rotation_translation(scale, rotation, translation)
}

fn walk_node_hierarchy(
    node_idx: usize,
    parent_world: Mat4,
    local_transforms: &[Mat4],
    children_list: &[Vec<usize>],
    world_transforms: &mut [Mat4],
) {
    let world = parent_world * local_transforms[node_idx];
    world_transforms[node_idx] = world;
    for &child in &children_list[node_idx] {
        walk_node_hierarchy(
            child,
            world,
            local_transforms,
            children_list,
            world_transforms,
        );
    }
}

// ============================================================================
// HumanoidTemplate VRM extension
// ============================================================================

impl HumanoidTemplate {
    /// VRM file path から [`HumanoidTemplate`] を構築 ([`extract_humanoid_bones`] + [`Self::from_vrm_bones`] の組合せ)
    ///
    /// # 戻り値
    ///
    /// - `Ok(Some(HumanoidTemplate))`: VRM が humanoid bones を持ち、必須 bone (head + hips) が揃っている
    /// - `Ok(None)`: VRM extension 不在 or 必須 bone 欠落
    ///
    /// # Errors
    ///
    /// - [`VrmError::Io`] / [`VrmError::MissingData`] ([`extract_humanoid_bones`] 参照)
    pub fn from_vrm<P: AsRef<Path>>(path: P) -> Result<Option<Self>, VrmError> {
        let bones = extract_humanoid_bones(path)?;
        Ok(bones.as_ref().and_then(Self::from_vrm_bones))
    }

    /// [`VrmBoneMap`] から [`HumanoidTemplate`] を構築
    ///
    /// VRM 標準 bone name (英字 camelCase) を [`Joint`] enum に mapping、bind pose の world position を格納
    /// `chest` 不在時は `upperChest` fallback
    ///
    /// # 戻り値
    ///
    /// - `Some(HumanoidTemplate)`: 必須 bone (`head` + `hips`) 存在
    /// - `None`: 必須 bone 欠落
    #[must_use]
    pub fn from_vrm_bones(vrm_bones: &VrmBoneMap) -> Option<Self> {
        let mappings: &[(&str, Joint)] = &[
            ("head", Joint::Head),
            ("neck", Joint::Neck),
            ("chest", Joint::Chest),
            ("hips", Joint::Waist),
            ("leftUpperArm", Joint::LShoulder),
            ("rightUpperArm", Joint::RShoulder),
            ("leftLowerArm", Joint::LElbow),
            ("rightLowerArm", Joint::RElbow),
            ("leftHand", Joint::LWrist),
            ("rightHand", Joint::RWrist),
            ("leftUpperLeg", Joint::LHip),
            ("rightUpperLeg", Joint::RHip),
            ("leftLowerLeg", Joint::LKnee),
            ("rightLowerLeg", Joint::RKnee),
            ("leftFoot", Joint::LAnkle),
            ("rightFoot", Joint::RAnkle),
        ];
        let mut joints: HashMap<Joint, [f32; 3]> = HashMap::with_capacity(16);
        for (vrm_name, joint) in mappings {
            if let Some(pos) = vrm_bones.get(vrm_name) {
                joints.insert(*joint, pos.into());
            }
        }
        // chest fallback: chest 不在時 upperChest 採用
        if let std::collections::hash_map::Entry::Vacant(e) = joints.entry(Joint::Chest) {
            if let Some(pos) = vrm_bones.get("upperChest") {
                e.insert(pos.into());
            }
        }
        // 必須 bone: head + hips
        if !joints.contains_key(&Joint::Head) || !joints.contains_key(&Joint::Waist) {
            return None;
        }
        Some(Self {
            bones: canonical_bones(),
            joints,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_pos_eq(actual: [f32; 3], expected: [f32; 3]) {
        for i in 0..3 {
            assert!(
                (actual[i] - expected[i]).abs() < 1e-4,
                "axis {i}: actual {} vs expected {}",
                actual[i],
                expected[i]
            );
        }
    }

    fn t_pose_bones() -> VrmBoneMap {
        let mut bones = HashMap::new();
        for (name, pos) in [
            ("head", [0.0_f32, 1.5, 0.0]),
            ("neck", [0.0, 1.4, 0.0]),
            ("chest", [0.0, 1.2, 0.0]),
            ("hips", [0.0, 0.85, 0.0]),
            ("leftUpperArm", [-0.2, 1.3, 0.0]),
            ("rightUpperArm", [0.2, 1.3, 0.0]),
            ("leftLowerArm", [-0.4, 1.3, 0.0]),
            ("rightLowerArm", [0.4, 1.3, 0.0]),
            ("leftHand", [-0.6, 1.3, 0.0]),
            ("rightHand", [0.6, 1.3, 0.0]),
            ("leftUpperLeg", [-0.1, 0.85, 0.0]),
            ("rightUpperLeg", [0.1, 0.85, 0.0]),
            ("leftLowerLeg", [-0.1, 0.4, 0.0]),
            ("rightLowerLeg", [0.1, 0.4, 0.0]),
            ("leftFoot", [-0.1, 0.0, 0.05]),
            ("rightFoot", [0.1, 0.0, 0.05]),
        ] {
            bones.insert(name.to_string(), Vec3::from_array(pos));
        }
        VrmBoneMap { bones }
    }

    #[test]
    fn from_vrm_bones_missing_head_returns_none() {
        let mut bones = HashMap::new();
        bones.insert("hips".to_string(), Vec3::new(0.0, 0.85, 0.0));
        let map = VrmBoneMap { bones };
        assert!(HumanoidTemplate::from_vrm_bones(&map).is_none());
    }

    #[test]
    fn from_vrm_bones_missing_hips_returns_none() {
        let mut bones = HashMap::new();
        bones.insert("head".to_string(), Vec3::new(0.0, 1.5, 0.0));
        let map = VrmBoneMap { bones };
        assert!(HumanoidTemplate::from_vrm_bones(&map).is_none());
    }

    #[test]
    fn from_vrm_bones_maps_16_joints() {
        let map = t_pose_bones();
        let t =
            HumanoidTemplate::from_vrm_bones(&map).expect("valid bones should produce template");
        assert_eq!(t.joints.len(), 16);
        assert_pos_eq(t.joints[&Joint::Head], [0.0, 1.5, 0.0]);
        assert_pos_eq(t.joints[&Joint::Waist], [0.0, 0.85, 0.0]);
        assert_pos_eq(t.joints[&Joint::LWrist], [-0.6, 1.3, 0.0]);
        assert_pos_eq(t.joints[&Joint::RAnkle], [0.1, 0.0, 0.05]);
    }

    #[test]
    fn from_vrm_bones_upperchest_fallback_for_chest() {
        let mut bones = HashMap::new();
        bones.insert("head".to_string(), Vec3::new(0.0, 1.5, 0.0));
        bones.insert("hips".to_string(), Vec3::new(0.0, 0.85, 0.0));
        bones.insert("upperChest".to_string(), Vec3::new(0.0, 1.3, 0.0));
        let map = VrmBoneMap { bones };
        let t = HumanoidTemplate::from_vrm_bones(&map).unwrap();
        assert_pos_eq(t.joints[&Joint::Chest], [0.0, 1.3, 0.0]);
    }

    #[test]
    fn from_vrm_bones_preserves_bone_topology() {
        let map = t_pose_bones();
        let t = HumanoidTemplate::from_vrm_bones(&map).unwrap();
        assert_eq!(t.bones.len(), 15);
        // topology sanity: first bone is Head → Neck (canonical order)
        assert_eq!(t.bones[0].from, Joint::Head);
        assert_eq!(t.bones[0].to, Joint::Neck);
    }

    #[test]
    fn from_vrm_nonexistent_path_returns_io_error() {
        let result = HumanoidTemplate::from_vrm("/nonexistent/path/character.vrm");
        match result {
            Err(VrmError::Io(_)) => (),
            other => panic!("expected Io error, got {other:?}"),
        }
    }

    #[test]
    fn vrm_bone_map_default_is_empty() {
        let map = VrmBoneMap::default();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);
    }
}
