//! The VRM import diagnostics on GLB files built here, in metres: a clean rig (a root, the pelvis at
//! 0.95, two legs down to ankles at 0.08, a spine and a head at 1.5, one mesh with bounded
//! `POSITION`s and one morph target bound by an expression, one non-metallic material) gives no
//! finding, and each defect alone gives exactly its finding. Checks 1 and 7 read both the VRM 1.0
//! and the 0.x layout.
#![cfg(feature = "vrm")]

use alice_lol_humanoid::vrm::{diagnose_glb, VrmDiagnostic as D};
use serde_json::{json, Value};

/// A GLB around the JSON (no binary chunk: the diagnostics read no buffer data)
fn glb(j: &Value) -> Vec<u8> {
    let mut text = serde_json::to_vec(j).unwrap();
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    let total = 12 + 8 + text.len();
    let mut out = Vec::with_capacity(total);
    out.extend(b"glTF");
    out.extend(2u32.to_le_bytes());
    out.extend(u32::try_from(total).unwrap().to_le_bytes());
    out.extend(u32::try_from(text.len()).unwrap().to_le_bytes());
    out.extend(b"JSON");
    out.extend(text);
    out
}

const BONES: [(&str, usize); 9] = [
    ("hips", 1),
    ("leftUpperLeg", 2),
    ("leftLowerLeg", 3),
    ("leftFoot", 4),
    ("rightUpperLeg", 5),
    ("rightLowerLeg", 6),
    ("rightFoot", 7),
    ("spine", 8),
    ("head", 9),
];

/// The clean rig in the VRM 1.0 layout (`vrm0`: the 0.x layout)
fn clean(vrm0: bool) -> Value {
    let node = |name: &str, t: [f64; 3], children: &[usize]| json!({"name": name, "translation": t, "children": children});
    let nodes = json!([
        node("Root", [0.0, 0.0, 0.0], &[1]),
        node("Pelvis", [0.0, 0.95, 0.0], &[2, 5, 8]),
        node("UpperLeg.L", [0.1, -0.05, 0.0], &[3]),
        node("LowerLeg.L", [0.0, -0.42, 0.0], &[4]),
        node("Foot.L", [0.0, -0.4, 0.0], &[]),
        node("UpperLeg.R", [-0.1, -0.05, 0.0], &[6]),
        node("LowerLeg.R", [0.0, -0.42, 0.0], &[7]),
        node("Foot.R", [0.0, -0.4, 0.0], &[]),
        node("Spine", [0.0, 0.1, 0.0], &[9]),
        node("Head", [0.0, 0.45, 0.0], &[]),
        json!({"name": "Body", "mesh": 0}),
    ]);
    let ext = if vrm0 {
        let bones: Vec<Value> = BONES
            .iter()
            .map(|(b, n)| json!({"bone": b, "node": n}))
            .collect();
        json!({"VRM": {"humanoid": {"humanBones": bones},
            "blendShapeMaster": {"blendShapeGroups": [{"presetName": "blink", "binds": [{"mesh": 0, "index": 0, "weight": 100}]}]}}})
    } else {
        let bones: serde_json::Map<String, Value> = BONES
            .iter()
            .map(|(b, n)| ((*b).to_string(), json!({"node": n})))
            .collect();
        json!({"VRMC_vrm": {"specVersion": "1.0", "humanoid": {"humanBones": bones},
            "expressions": {"preset": {"blink": {"morphTargetBinds": [{"node": 10, "index": 0, "weight": 1.0}]}}}}})
    };
    json!({
        "asset": {"version": "2.0"},
        "extensionsUsed": [if vrm0 { "VRM" } else { "VRMC_vrm" }],
        "extensions": ext,
        "nodes": nodes,
        "scenes": [{"nodes": [0, 10]}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "targets": [{"POSITION": 1}], "material": 0}]}],
        "accessors": [
            {"componentType": 5126, "count": 3, "type": "VEC3", "min": [-0.2, 0.0, -0.1], "max": [0.2, 1.5, 0.1]},
            {"componentType": 5126, "count": 3, "type": "VEC3", "min": [0.0, 0.0, 0.0], "max": [0.01, 0.01, 0.0]}
        ],
        "materials": [{"pbrMetallicRoughness": {"metallicFactor": 0.0, "roughnessFactor": 0.8}}]
    })
}

fn report(j: &Value) -> Vec<D> {
    diagnose_glb(&glb(j)).expect("a VRM GLB").diagnostics
}

#[test]
fn a_clean_rig_has_no_finding_and_every_check_compares() {
    for vrm0 in [false, true] {
        let r = diagnose_glb(&glb(&clean(vrm0))).expect("a VRM GLB");
        assert_eq!(r.diagnostics, vec![], "vrm0 {vrm0}");
        let c = r.compared;
        assert_eq!(
            (
                c.hips,
                c.height,
                c.feet,
                c.positions,
                c.materials,
                c.extensions,
                c.morphs
            ),
            (1, 1, 1, 2, 1, 1, 1),
            "vrm0 {vrm0}"
        );
    }
}

#[test]
fn check1_hips_on_a_grounding_root() {
    for vrm0 in [false, true] {
        let mut j = clean(vrm0);
        if vrm0 {
            j["extensions"]["VRM"]["humanoid"]["humanBones"][0]["node"] = json!(0);
        } else {
            j["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"]["hips"]["node"] = json!(0);
        }
        assert_eq!(
            report(&j),
            vec![D::HipsNotPelvis {
                hips_node: 0,
                pelvis_node: Some(1)
            }],
            "vrm0 {vrm0}"
        );
    }
}

/// A hips above the feet but not the pelvis (the spine): only the common-ancestor rule sees it
#[test]
fn check1_hips_on_the_spine() {
    let mut j = clean(false);
    j["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"]["hips"]["node"] = json!(8);
    assert_eq!(
        report(&j),
        vec![D::HipsNotPelvis {
            hips_node: 8,
            pelvis_node: Some(1)
        }]
    );
}

#[test]
fn check2_a_quarter_scale_rig() {
    let mut j = clean(false);
    j["nodes"][0]["scale"] = json!([0.25, 0.25, 0.25]);
    let d = report(&j);
    assert_eq!(d.len(), 1, "{d:?}");
    let D::HeightOutOfRange {
        height,
        scale_to_nominal,
    } = d[0]
    else {
        panic!("{d:?}")
    };
    assert!((height - 0.355).abs() < 1e-5, "height {height}");
    assert!(
        (scale_to_nominal - 1.4 / 0.355).abs() < 1e-4,
        "scale {scale_to_nominal}"
    );
}

#[test]
fn check3_feet_below_the_origin() {
    let mut j = clean(false);
    j["nodes"][0]["translation"] = json!([0.0, -0.3, 0.0]);
    let d = report(&j);
    assert_eq!(d.len(), 1, "{d:?}");
    let D::FeetNotOnGround {
        lowest_foot_y,
        crosses_origin,
    } = d[0]
    else {
        panic!("{d:?}")
    };
    assert!(
        (lowest_foot_y + 0.22).abs() < 1e-5 && crosses_origin,
        "{d:?}"
    );
    // floating: the ankles 0.3 above the ground
    let mut f = clean(false);
    f["nodes"][0]["translation"] = json!([0.0, 0.3, 0.0]);
    let d = report(&f);
    assert!(
        matches!(
            d.as_slice(),
            [D::FeetNotOnGround {
                crosses_origin: false,
                ..
            }]
        ),
        "{d:?}"
    );
}

#[test]
fn check4_position_bounds_missing() {
    let mut j = clean(false);
    j["accessors"][0].as_object_mut().unwrap().remove("min");
    assert_eq!(
        report(&j),
        vec![D::PositionBoundsMissing { missing: 1, of: 2 }]
    );
    // either bound missing counts: here the morph target's max
    let mut k = clean(false);
    k["accessors"][1].as_object_mut().unwrap().remove("max");
    assert_eq!(
        report(&k),
        vec![D::PositionBoundsMissing { missing: 1, of: 2 }]
    );
}

#[test]
fn check5_default_metallic_materials() {
    let mut j = clean(false);
    // no pbrMetallicRoughness at all: glTF's default metallicFactor is 1
    j["materials"] = json!([{}, {"pbrMetallicRoughness": {"metallicFactor": 1.0}}, {"pbrMetallicRoughness": {"metallicFactor": 0.0}}]);
    assert_eq!(
        report(&j),
        vec![D::DefaultMetallicMaterials { count: 2, of: 3 }]
    );
    // a metallic texture or an unlit material is a choice, not a default
    let mut k = clean(false);
    k["materials"] = json!([{"pbrMetallicRoughness": {"metallicRoughnessTexture": {"index": 0}}}, {"extensions": {"KHR_materials_unlit": {}}}]);
    k["extensionsUsed"] = json!(["VRMC_vrm", "KHR_materials_unlit"]);
    assert_eq!(report(&k), vec![]);
}

#[test]
fn check6_declared_extension_with_no_body() {
    let mut j = clean(false);
    j["extensionsUsed"] = json!(["VRMC_vrm", "VRMC_springBone"]);
    assert_eq!(
        report(&j),
        vec![D::DeclaredExtensionUnused {
            name: "VRMC_springBone".into()
        }]
    );
}

#[test]
fn check7_morphs_with_no_expression() {
    for vrm0 in [false, true] {
        let mut j = clean(vrm0);
        if vrm0 {
            j["extensions"]["VRM"]
                .as_object_mut()
                .unwrap()
                .remove("blendShapeMaster");
        } else {
            j["extensions"]["VRMC_vrm"]
                .as_object_mut()
                .unwrap()
                .remove("expressions");
        }
        assert_eq!(
            report(&j),
            vec![D::MorphsWithoutExpressions { targets: 1 }],
            "vrm0 {vrm0}"
        );
    }
}

#[test]
fn no_humanoid_or_no_glb_is_an_error_not_an_empty_pass() {
    let mut j = clean(false);
    j["extensions"] = json!({});
    j["extensionsUsed"] = json!([]);
    assert!(diagnose_glb(&glb(&j)).is_err(), "no humanoid mapping");
    assert!(
        diagnose_glb(b"not a glb at all, just bytes").is_err(),
        "not a GLB"
    );
    assert!(
        diagnose_glb(&glb(&json!({"asset": {"version": "2.0"}}))).is_err(),
        "an empty glTF"
    );
    // a humanoid that maps only the spine, with no mesh, material or extension: every check
    // compares 0, so the run is an error, not a pass with no findings
    let bare = json!({"asset": {"version": "2.0"}, "nodes": [{"name": "Spine"}],
        "extensions": {"VRMC_vrm": {"humanoid": {"humanBones": {"spine": {"node": 0}}}}}});
    assert!(diagnose_glb(&glb(&bare)).is_err(), "compared nothing");
}

/// The load path: the template is read only from a file the checks accept. A finding that would
/// make the template wrong (the joints at the wrong node, in the wrong unit or off the ground) is
/// an error; one that leaves the joints right is returned beside the template as a warning
mod load {
    use super::{clean, glb, D};
    use alice_lol_humanoid::vrm::VrmError;
    use alice_lol_humanoid::HumanoidTemplate;
    use serde_json::{json, Value};

    fn load(name: &str, j: &Value) -> Result<(Option<HumanoidTemplate>, Vec<D>), VrmError> {
        let path = std::env::temp_dir().join(format!("vrm_load_{}_{name}.vrm", std::process::id()));
        std::fs::write(&path, glb(j)).unwrap();
        let r = HumanoidTemplate::from_vrm(&path);
        std::fs::remove_file(&path).unwrap();
        r
    }

    #[test]
    fn a_clean_rig_loads_with_no_warning() {
        let (t, w) = load("clean", &clean(true)).expect("a clean rig loads");
        assert!(t.is_some(), "the template");
        assert_eq!(w, vec![]);
    }

    #[test]
    fn hips_on_a_grounding_root_is_refused() {
        let mut j = clean(true);
        j["extensions"]["VRM"]["humanoid"]["humanBones"][0]["node"] = json!(0);
        match load("hips", &j) {
            Err(VrmError::Rejected(d)) => assert_eq!(
                d,
                vec![D::HipsNotPelvis {
                    hips_node: 0,
                    pelvis_node: Some(1)
                }]
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_quarter_scale_or_sunk_rig_is_refused() {
        let mut j = clean(true);
        j["nodes"][0]["scale"] = json!([0.25, 0.25, 0.25]);
        assert!(
            matches!(load("scale", &j), Err(VrmError::Rejected(d)) if matches!(d.as_slice(), [D::HeightOutOfRange { .. }]))
        );
        let mut k = clean(true);
        k["nodes"][0]["translation"] = json!([0.0, -0.3, 0.0]);
        assert!(
            matches!(load("sunk", &k), Err(VrmError::Rejected(d)) if matches!(d.as_slice(), [D::FeetNotOnGround { .. }]))
        );
    }

    #[test]
    fn cosmetic_findings_load_as_warnings() {
        let mut j = clean(true);
        j["materials"] = json!([{}]);
        j["extensionsUsed"] = json!(["VRM", "VRMC_springBone"]);
        let (t, w) = load("cosmetic", &j).expect("cosmetic findings do not refuse the file");
        assert!(t.is_some());
        assert_eq!(
            w,
            vec![
                D::DefaultMetallicMaterials { count: 1, of: 1 },
                D::DeclaredExtensionUnused {
                    name: "VRMC_springBone".into()
                }
            ]
        );
    }

    #[test]
    fn a_plain_gltf_is_no_template_and_no_check() {
        let j = json!({"asset": {"version": "2.0"}, "nodes": [{"name": "Box", "mesh": 0}], "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}]});
        let (t, w) = load("plain", &j).expect("a glTF that is not a VRM is not an error");
        assert!(t.is_none());
        assert_eq!(w, vec![]);
    }

    /// Which findings make the template wrong (errors) and which do not (warnings), one per kind
    #[test]
    fn the_error_and_warning_split() {
        let cases = [
            (
                D::HipsNotPelvis {
                    hips_node: 0,
                    pelvis_node: None,
                },
                true,
            ),
            (
                D::HeightOutOfRange {
                    height: 0.3,
                    scale_to_nominal: 4.6,
                },
                true,
            ),
            (
                D::FeetNotOnGround {
                    lowest_foot_y: -0.2,
                    crosses_origin: true,
                },
                true,
            ),
            (D::PositionBoundsMissing { missing: 1, of: 2 }, false),
            (D::DefaultMetallicMaterials { count: 1, of: 1 }, false),
            (D::DeclaredExtensionUnused { name: "X".into() }, false),
            (D::MorphsWithoutExpressions { targets: 1 }, false),
        ];
        for (d, err) in cases {
            assert_eq!(d.is_error(), err, "{d:?}");
        }
    }
}

/// The files `scripts/vrm_diagnose_check.sh` runs the `vrm_diagnose` example on (CI and preflight):
/// the clean rig and the rig whose hips sit on the grounding root, as built here. A difference
/// fails; `VRM_FIXTURES_WRITE=1` writes them again
#[test]
fn the_example_fixtures_are_the_rigs_built_here() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut hips = clean(false);
    hips["extensions"]["VRMC_vrm"]["humanoid"]["humanBones"]["hips"]["node"] = json!(0);
    let files = [
        ("vrm_clean.glb", glb(&clean(false))),
        ("vrm_hips_on_root.glb", glb(&hips)),
    ];
    for (name, bytes) in &files {
        let path = dir.join(name);
        if std::env::var_os("VRM_FIXTURES_WRITE").is_some() {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, bytes).unwrap();
        }
        assert_eq!(
            &std::fs::read(&path).unwrap_or_default(),
            bytes,
            "{name} differs from the rig built here"
        );
    }
}
