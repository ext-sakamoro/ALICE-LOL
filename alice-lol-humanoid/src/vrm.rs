//! # VRM import module (feature `vrm`)
//!
//! VRM 0.x binary GLB からhumanoid bone を抽出し [`HumanoidTemplate`] を構築する
//!
//! 対応 VRM version: [`HumanoidTemplate::from_vrm`] の bone 読み込みは **0.x のみ** (`extensions.VRM.humanoid.humanBones`)、
//! 読み込みの診断 [`diagnose_glb`] は 0.x と 1.0 (`VRMC_vrm`) の両方を読む
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
//! // a file whose hips, height or feet would make the template wrong is refused (`VrmError::Rejected`)
//! let (template, warnings) = HumanoidTemplate::from_vrm("character.vrm")?;
//! for w in &warnings {
//!     println!("warning: {w:?}");
//! }
//! match template {
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
//! GLB JSON parser + node hierarchy walker + humanoid bone 読み込み +
//! VRM bone name → [`Joint`] mapping は 参照実装の `src/vrm_import.rs` から抜粋
//! Phase H.5 で Manga / LOL 間の reconciliation 判断

use std::collections::BTreeMap;
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
    /// The checks of [`diagnose_glb`] found what would make the template wrong (the findings for
    /// which [`VrmDiagnostic::is_error`] holds)
    Rejected(Vec<VrmDiagnostic>),
}

impl std::fmt::Display for VrmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::MissingData(s) => write!(f, "VRM data missing: {s}"),
            Self::Rejected(d) => write!(f, "VRM refused: {d:?}"),
        }
    }
}

impl std::error::Error for VrmError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::MissingData(_) | Self::Rejected(_) => None,
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
/// VRM から読んだ humanoid bones ([`HumanoidTemplate::from_vrm_bones`] の入力) VRM 標準の bone name (英字 camelCase、
/// 例: `"hips"` / `"leftUpperArm"`) を key に、bind pose (T-pose) の world position を value とする
#[derive(Debug, Clone, Default)]
pub struct VrmBoneMap {
    /// bone name → world position
    pub bones: BTreeMap<String, Vec3>,
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

    /// 全 bone name list (名前順、debug 用途)
    #[must_use]
    pub fn bone_names(&self) -> Vec<&str> {
        self.bones.keys().map(String::as_str).collect()
    }
}

// ============================================================================
// humanoid bones (read only through the checked HumanoidTemplate::from_vrm)
// ============================================================================

/// The world positions of the VRM 0.x humanoid bones of a parsed glTF JSON
///
/// Private: a file is read for its bones only by [`HumanoidTemplate::from_vrm`], which checks it
/// with [`diagnose_glb`] first (an unchecked reader would hand back the bones of a file whose hips
/// are not the pelvis)
///
/// - `Some(VrmBoneMap)`: VRM extension が存在し 1 個以上の bone が抽出できた
/// - `None`: VRM extension 不在 (glTF 通常 file or VRM 1.0)、または humanoid.humanBones 欠落
///
/// # Errors
///
/// - [`VrmError::MissingData`]: `glTF nodes` 欠落
fn humanoid_bones_of(json: &serde_json::Value) -> Result<Option<VrmBoneMap>, VrmError> {
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
    let node_world_translations = compute_node_world_translations(json)?;
    let mut map: BTreeMap<String, Vec3> = BTreeMap::new();
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
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    parse_glb_json_bytes(&bytes)
}

/// The JSON chunk of a GLB held in memory (the same reading as [`parse_glb_json`])
fn parse_glb_json_bytes(bytes: &[u8]) -> Result<serde_json::Value, VrmError> {
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        return Err(VrmError::MissingData(
            "not a GLB file (missing glTF magic)".into(),
        ));
    }
    let chunk_length = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
    if &bytes[16..20] != b"JSON" {
        return Err(VrmError::MissingData(
            "first chunk is not JSON in GLB".into(),
        ));
    }
    let json_data = bytes
        .get(20..20 + chunk_length)
        .ok_or_else(|| VrmError::MissingData("JSON chunk runs past the end of the GLB".into()))?;
    let json_str = std::str::from_utf8(json_data)
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
    /// VRM file path から [`HumanoidTemplate`] を構築 (VRM 0.x の humanoid bones を読み [`Self::from_vrm_bones`] に渡す)
    ///
    /// A file with humanoid bones is first checked with [`diagnose_glb`]: a finding that would make
    /// the template wrong ([`VrmDiagnostic::is_error`]: the hips not the pelvis, the height outside
    /// the human range, the feet off the ground) refuses the file; the others are returned beside
    /// the template as warnings.
    ///
    /// # 戻り値
    ///
    /// - `Ok((Some(HumanoidTemplate), warnings))`: VRM が humanoid bones を持ち、必須 bone (head + hips) が揃っている
    /// - `Ok((None, []))`: VRM extension 不在 or 必須 bone 欠落 (warnings is empty when no humanoid was read)
    ///
    /// # Errors
    ///
    /// - [`VrmError::Io`]: file open / read 失敗
    /// - [`VrmError::MissingData`]: GLB header 不正、JSON parse 失敗、`glTF nodes` 欠落 ([`diagnose_glb`] 参照)
    /// - [`VrmError::Rejected`]: the findings that would make the template wrong
    pub fn from_vrm<P: AsRef<Path>>(
        path: P,
    ) -> Result<(Option<Self>, Vec<VrmDiagnostic>), VrmError> {
        let json = parse_glb_json(path.as_ref())?;
        let Some(bones) = humanoid_bones_of(&json)? else {
            return Ok((None, Vec::new()));
        };
        let (errors, warnings): (Vec<_>, Vec<_>) = diagnose_json(&json)?
            .diagnostics
            .into_iter()
            .partition(VrmDiagnostic::is_error);
        if !errors.is_empty() {
            return Err(VrmError::Rejected(errors));
        }
        Ok((Self::from_vrm_bones(&bones), warnings))
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
        let mut joints: BTreeMap<Joint, [f32; 3]> = BTreeMap::new();
        for (vrm_name, joint) in mappings {
            if let Some(pos) = vrm_bones.get(vrm_name) {
                joints.insert(*joint, pos.into());
            }
        }
        // chest fallback: chest 不在時 upperChest 採用
        if let std::collections::btree_map::Entry::Vacant(e) = joints.entry(Joint::Chest) {
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

// ============================================================================
// Import diagnostics
// ============================================================================

/// One finding of [`diagnose_glb`]: a property of the file that general VRM readers handle badly
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum VrmDiagnostic {
    /// The humanoid `hips` is not the pelvis: not the lowest common ancestor of the two upper legs
    /// (a grounding root above it in the tree, say), or below the feet
    HipsNotPelvis {
        /// The node mapped as `hips`
        hips_node: usize,
        /// The lowest common ancestor of the two upper legs, where the pelvis is
        pelvis_node: Option<usize>,
    },
    /// The rig's height (head joint over the lowest foot joint, in metres) is outside the human
    /// range [`HUMAN_HEIGHT_RANGE`]: usually a unit mix-up at export
    HeightOutOfRange {
        /// The measured height
        height: f32,
        /// The scale that would bring it to [`NOMINAL_HEIGHT`]
        scale_to_nominal: f32,
    },
    /// The lowest foot joint is not just above the ground (`y = 0`): below it, or floating above
    /// [`MAX_ANKLE_HEIGHT`]
    FeetNotOnGround {
        /// The lowest foot joint's height
        lowest_foot_y: f32,
        /// Whether the rig spans the origin (feet below it, head above it)
        crosses_origin: bool,
    },
    /// `POSITION` accessors without `min` / `max` (required by glTF 2.0)
    PositionBoundsMissing {
        /// How many lack them
        missing: usize,
        /// Of how many
        of: usize,
    },
    /// Materials left at the glTF default `metallicFactor` 1 (fully metallic) with no metallic
    /// texture and no unlit or toon material extension
    DefaultMetallicMaterials {
        /// How many
        count: usize,
        /// Of how many
        of: usize,
    },
    /// An extension named in `extensionsUsed` that no object of the file carries
    DeclaredExtensionUnused {
        /// Its name
        name: String,
    },
    /// Morph targets present but no expression preset binds any of them (standard VRM readers
    /// then cannot blink or lip-sync)
    MorphsWithoutExpressions {
        /// How many morph targets the meshes carry
        targets: usize,
    },
}

impl VrmDiagnostic {
    /// Whether the finding makes a template read from the file wrong: the joints at the wrong node
    /// (the hips), in the wrong unit (the height) or off the ground (the feet). The others leave
    /// the joints right and are warnings
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(
            self,
            Self::HipsNotPelvis { .. }
                | Self::HeightOutOfRange { .. }
                | Self::FeetNotOnGround { .. }
        )
    }
}

/// What [`diagnose_glb`] compared, per check (a check whose input is absent compares 0)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VrmCompared {
    /// Check 1: the hips against the upper legs (1 when both legs and the hips are mapped)
    pub hips: usize,
    /// Check 2: the rig's height (1 when the head and a foot are mapped)
    pub height: usize,
    /// Check 3: the feet against the ground (1 when a foot is mapped)
    pub feet: usize,
    /// Check 4: `POSITION` accessors
    pub positions: usize,
    /// Check 5: materials
    pub materials: usize,
    /// Check 6: declared extensions
    pub extensions: usize,
    /// Check 7: meshes with morph targets
    pub morphs: usize,
}

impl VrmCompared {
    /// The total compared over all checks
    #[must_use]
    pub const fn total(&self) -> usize {
        self.hips
            + self.height
            + self.feet
            + self.positions
            + self.materials
            + self.extensions
            + self.morphs
    }
}

/// The result of [`diagnose_glb`]: the findings and what was compared
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VrmReport {
    /// The findings, in check order
    pub diagnostics: Vec<VrmDiagnostic>,
    /// What each check compared
    pub compared: VrmCompared,
}

/// The human range of a rig's height (head joint over the lowest foot joint), metres
pub const HUMAN_HEIGHT_RANGE: (f32, f32) = (0.5, 2.6);
/// The height [`VrmDiagnostic::HeightOutOfRange`] reports a scale to, metres
pub const NOMINAL_HEIGHT: f32 = 1.4;
/// The highest the lowest foot joint (the ankle) may sit above the ground, metres
pub const MAX_ANKLE_HEIGHT: f32 = 0.2;
/// How far below the ground the lowest foot joint may sit before it counts as sunk, metres
pub const GROUND_TOLERANCE: f32 = 0.01;

/// Check a VRM GLB (0.x or 1.0) held in memory for what general VRM readers handle badly
///
/// The checks: a `hips` that is not the pelvis, a height outside the human range, feet off the
/// ground, `POSITION` accessors without bounds, materials left fully metallic, extensions declared
/// but absent, and morph targets with no expression preset.
///
/// # Errors
///
/// [`VrmError::MissingData`] when the bytes are not a GLB, the file has no humanoid mapping, or
/// the checks compared nothing (an empty pass is not a pass).
pub fn diagnose_glb(bytes: &[u8]) -> Result<VrmReport, VrmError> {
    diagnose_json(&parse_glb_json_bytes(bytes)?)
}

/// [`diagnose_glb`] on a parsed glTF JSON
fn diagnose_json(json: &serde_json::Value) -> Result<VrmReport, VrmError> {
    let bones = humanoid_nodes(json).ok_or_else(|| {
        VrmError::MissingData("no humanoid mapping (neither VRM 0.x nor VRM 1.0)".into())
    })?;
    let world = compute_node_world_translations(json)?;
    let mut rep = VrmReport::default();
    check_skeleton(json, &bones, &world, &mut rep);
    check_positions(json, &mut rep);
    check_materials(json, &mut rep);
    check_extensions(json, &mut rep);
    check_morphs(json, &mut rep);
    if rep.compared.total() == 0 {
        return Err(VrmError::MissingData(
            "the diagnostics compared nothing".into(),
        ));
    }
    Ok(rep)
}

/// Checks 1-3: the hips are the pelvis (the lowest common ancestor of the two upper legs, above the
/// feet), the height is in the human range, and the feet sit just above the ground
fn check_skeleton(
    json: &serde_json::Value,
    bones: &BTreeMap<String, usize>,
    world: &[Vec3],
    rep: &mut VrmReport,
) {
    let y = |name: &str| bones.get(name).and_then(|&n| world.get(n)).map(|p| p.y);
    let lowest_foot = ["leftFoot", "rightFoot"]
        .iter()
        .filter_map(|b| y(b))
        // ordered by comparison, not f32::min (whose zero sign differs between targets)
        .fold(None, |m: Option<f32>, v| {
            Some(m.map_or(v, |m| if v < m { v } else { m }))
        });
    if let (Some(&hips), Some(&l), Some(&r)) = (
        bones.get("hips"),
        bones.get("leftUpperLeg"),
        bones.get("rightUpperLeg"),
    ) {
        rep.compared.hips = 1;
        let pelvis = lowest_common_ancestor(&node_parents(json), l, r);
        let below_feet = lowest_foot
            .zip(world.get(hips))
            .is_some_and(|(f, h)| h.y < f);
        if pelvis != Some(hips) || below_feet {
            rep.diagnostics.push(VrmDiagnostic::HipsNotPelvis {
                hips_node: hips,
                pelvis_node: pelvis,
            });
        }
    }
    if let (Some(head), Some(foot)) = (y("head"), lowest_foot) {
        rep.compared.height = 1;
        let height = head - foot;
        if !(HUMAN_HEIGHT_RANGE.0..=HUMAN_HEIGHT_RANGE.1).contains(&height) {
            rep.diagnostics.push(VrmDiagnostic::HeightOutOfRange {
                height,
                scale_to_nominal: NOMINAL_HEIGHT / height,
            });
        }
    }
    if let Some(foot) = lowest_foot {
        rep.compared.feet = 1;
        if foot < -GROUND_TOLERANCE || foot > MAX_ANKLE_HEIGHT {
            let crosses_origin =
                foot < -GROUND_TOLERANCE && y("head").is_some_and(|h| h > GROUND_TOLERANCE);
            rep.diagnostics.push(VrmDiagnostic::FeetNotOnGround {
                lowest_foot_y: foot,
                crosses_origin,
            });
        }
    }
}

/// Check 4: `POSITION` accessors (the primitives' and their morph targets') carry `min` and `max`
fn check_positions(json: &serde_json::Value, rep: &mut VrmReport) {
    let mut positions: Vec<usize> = Vec::new();
    for prim in json_array(json, &["meshes"])
        .iter()
        .flat_map(|m| json_array(m, &["primitives"]))
    {
        let sets = std::iter::once(prim.get("attributes"))
            .chain(json_array(prim, &["targets"]).iter().map(Some));
        for set in sets.flatten() {
            if let Some(i) = set
                .get("POSITION")
                .and_then(serde_json::Value::as_u64)
                .and_then(|i| usize::try_from(i).ok())
            {
                positions.push(i);
            }
        }
    }
    positions.sort_unstable();
    positions.dedup();
    rep.compared.positions = positions.len();
    let accessors = json_array(json, &["accessors"]);
    let missing = positions
        .iter()
        .filter(|&&i| {
            accessors
                .get(i)
                .is_none_or(|a| a.get("min").is_none() || a.get("max").is_none())
        })
        .count();
    if missing > 0 {
        rep.diagnostics.push(VrmDiagnostic::PositionBoundsMissing {
            missing,
            of: positions.len(),
        });
    }
}

/// Check 5: materials left at glTF's default `metallicFactor` 1 with no metallic texture and no
/// unlit or toon extension
fn check_materials(json: &serde_json::Value, rep: &mut VrmReport) {
    let materials = json_array(json, &["materials"]);
    rep.compared.materials = materials.len();
    let count = materials
        .iter()
        .filter(|m| {
            let pbr = m.get("pbrMetallicRoughness");
            let metallic = pbr
                .and_then(|p| p.get("metallicFactor"))
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0);
            let texture = pbr
                .and_then(|p| p.get("metallicRoughnessTexture"))
                .is_some();
            let shaded = m.get("extensions").is_some_and(|e| {
                ["KHR_materials_unlit", "VRMC_materials_mtoon"]
                    .iter()
                    .any(|k| e.get(k).is_some())
            });
            metallic >= 1.0 && !texture && !shaded
        })
        .count();
    if count > 0 {
        rep.diagnostics
            .push(VrmDiagnostic::DefaultMetallicMaterials {
                count,
                of: materials.len(),
            });
    }
}

/// Check 6: every extension named in `extensionsUsed` has a body somewhere in the file
fn check_extensions(json: &serde_json::Value, rep: &mut VrmReport) {
    let used: Vec<&str> = json_array(json, &["extensionsUsed"])
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    rep.compared.extensions = used.len();
    for name in used {
        if !extension_body_present(json, name) {
            rep.diagnostics
                .push(VrmDiagnostic::DeclaredExtensionUnused {
                    name: name.to_string(),
                });
        }
    }
}

/// Check 7: morph targets are bound by some expression preset
fn check_morphs(json: &serde_json::Value, rep: &mut VrmReport) {
    let mut targets = 0;
    for mesh in json_array(json, &["meshes"]) {
        let n = json_array(mesh, &["primitives"])
            .iter()
            .map(|p| json_array(p, &["targets"]).len())
            .max()
            .unwrap_or(0);
        if n > 0 {
            rep.compared.morphs += 1;
            targets += n;
        }
    }
    if targets > 0 && expression_binds(json) == 0 {
        rep.diagnostics
            .push(VrmDiagnostic::MorphsWithoutExpressions { targets });
    }
}

/// The humanoid bone → node map of VRM 1.0 (`VRMC_vrm.humanoid.humanBones`, an object) or 0.x
/// (`VRM.humanoid.humanBones`, an array of `{bone, node}`); `None` when neither is present
fn humanoid_nodes(json: &serde_json::Value) -> Option<BTreeMap<String, usize>> {
    let ext = json.get("extensions")?;
    let node = |v: &serde_json::Value| {
        v.get("node")
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
    };
    let mut map = BTreeMap::new();
    if let Some(bones) = ext
        .get("VRMC_vrm")
        .and_then(|v| v.get("humanoid"))
        .and_then(|h| h.get("humanBones"))
        .and_then(|b| b.as_object())
    {
        for (name, v) in bones {
            if let Some(n) = node(v) {
                map.insert(name.clone(), n);
            }
        }
    } else if let Some(bones) = ext
        .get("VRM")
        .and_then(|v| v.get("humanoid"))
        .and_then(|h| h.get("humanBones"))
        .and_then(|b| b.as_array())
    {
        for v in bones {
            if let (Some(name), Some(n)) = (v.get("bone").and_then(|b| b.as_str()), node(v)) {
                map.insert(name.to_string(), n);
            }
        }
    }
    (!map.is_empty()).then_some(map)
}

/// The parent of every node (from the `children` lists)
fn node_parents(json: &serde_json::Value) -> Vec<Option<usize>> {
    let nodes = json_array(json, &["nodes"]);
    let mut parents = vec![None; nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        for c in json_array(n, &["children"]) {
            if let Some(c) = c.as_u64().and_then(|c| usize::try_from(c).ok()) {
                if c < parents.len() {
                    parents[c] = Some(i);
                }
            }
        }
    }
    parents
}

/// The lowest node that is an ancestor of (or equal to) both `a` and `b`
fn lowest_common_ancestor(parents: &[Option<usize>], a: usize, b: usize) -> Option<usize> {
    let mut up = Vec::new();
    let mut n = Some(a);
    while let Some(i) = n {
        if up.contains(&i) || up.len() > parents.len() {
            break;
        }
        up.push(i);
        n = parents.get(i).copied().flatten();
    }
    let mut n = Some(b);
    let mut steps = 0;
    while let Some(i) = n {
        if up.contains(&i) {
            return Some(i);
        }
        steps += 1;
        if steps > parents.len() {
            break;
        }
        n = parents.get(i).copied().flatten();
    }
    None
}

/// The array at a key path (empty when absent or not an array)
fn json_array<'a>(v: &'a serde_json::Value, path: &[&str]) -> &'a [serde_json::Value] {
    path.iter()
        .try_fold(v, |v, k| v.get(k))
        .and_then(serde_json::Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Whether some `extensions` object anywhere in the file carries `name`
fn extension_body_present(v: &serde_json::Value, name: &str) -> bool {
    match v {
        serde_json::Value::Object(m) => {
            m.get("extensions").and_then(|e| e.get(name)).is_some()
                || m.values().any(|x| extension_body_present(x, name))
        }
        serde_json::Value::Array(a) => a.iter().any(|x| extension_body_present(x, name)),
        _ => false,
    }
}

/// How many morph-target bindings the expression presets carry (VRM 1.0 `expressions`, preset
/// and custom; VRM 0.x `blendShapeMaster.blendShapeGroups[].binds`)
fn expression_binds(json: &serde_json::Value) -> usize {
    let Some(ext) = json.get("extensions") else {
        return 0;
    };
    let mut n = 0;
    if let Some(ex) = ext.get("VRMC_vrm").and_then(|v| v.get("expressions")) {
        for group in ["preset", "custom"] {
            if let Some(m) = ex.get(group).and_then(|g| g.as_object()) {
                n += m
                    .values()
                    .map(|e| json_array(e, &["morphTargetBinds"]).len())
                    .sum::<usize>();
            }
        }
    }
    n += json_array(ext, &["VRM", "blendShapeMaster", "blendShapeGroups"])
        .iter()
        .map(|g| json_array(g, &["binds"]).len())
        .sum::<usize>();
    n
}

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
        let mut bones = BTreeMap::new();
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
        let mut bones = BTreeMap::new();
        bones.insert("hips".to_string(), Vec3::new(0.0, 0.85, 0.0));
        let map = VrmBoneMap { bones };
        assert!(HumanoidTemplate::from_vrm_bones(&map).is_none());
    }

    #[test]
    fn from_vrm_bones_missing_hips_returns_none() {
        let mut bones = BTreeMap::new();
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
        let mut bones = BTreeMap::new();
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
