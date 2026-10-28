//! LOL の書き出し (STL / 3MF / FBX) が producer に LOL の幾何の意味の識別子を持つ
//!
//! 主の識別子は alice-sdf の mesh の意味 (`Provenance::sdf`)、producer は
//! `LOL_GEOMETRY_SEMANTICS_ID` 読み戻しは alice-sdf の reader と、それとは別の読み手
//! (header の byte、`stl_io`、STORE の zip を自前で歩く、`fbxcel`) の両方で確かめる
//!
//! golden は書き出した file 全体の SHA-256
//! (`LOL_DIGEST_RECORD=1 cargo test --test export_producer_stamp -- --nocapture` で記録)

use alice_lol::geometry_id::LOL_GEOMETRY_SEMANTICS_ID;
use alice_lol::print_export::{
    node_to_3mf, node_to_3mf_dual_contouring, node_to_fbx, node_to_mesh, node_to_stl,
    node_to_stl_dual_contouring, PrintConfig, LOL_EXPORT_STAMP,
};
use alice_lol::{lol, SdfNode, Vec3};
use alice_sdf::io::export_stl;
use alice_sdf::provenance::{self, Provenance, ProvenanceError, StoredForm, StoredSemantics};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lol-producer-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

const fn node() -> SdfNode {
    lol! { sphere(1.0) }
}

const fn config() -> PrintConfig {
    PrintConfig {
        resolution: 16,
        bounds_min: Vec3::splat(-1.5),
        bounds_max: Vec3::splat(1.5),
        scale_mm: 10.0,
    }
}

const fn lol_producer() -> Provenance {
    Provenance::new(LOL_GEOMETRY_SEMANTICS_ID)
}

fn sha_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

type Writer = fn(&SdfNode, &Path, &PrintConfig);

/// 書き出し関数ごとの (名前, 拡張子, 呼び出し)
fn writers() -> Vec<(&'static str, &'static str, Writer)> {
    #[cfg_attr(not(feature = "roblox"), allow(unused_mut))]
    let mut w: Vec<(&'static str, &'static str, Writer)> = vec![
        ("node_to_stl", "stl", |n, p, c| {
            node_to_stl(n, p, c).unwrap();
        }),
        ("node_to_3mf", "3mf", |n, p, c| {
            node_to_3mf(n, p, c).unwrap();
        }),
        ("node_to_fbx", "fbx", |n, p, c| {
            node_to_fbx(n, p, c).unwrap();
        }),
        ("node_to_stl_dual_contouring", "stl", |n, p, c| {
            node_to_stl_dual_contouring(n, p, c).unwrap();
        }),
        ("node_to_3mf_dual_contouring", "3mf", |n, p, c| {
            node_to_3mf_dual_contouring(n, p, c).unwrap();
        }),
    ];
    #[cfg(feature = "roblox")]
    w.push(("node_to_fbx_roblox", "fbx", |n, p, c| {
        let rc = alice_lol::roblox_export::RobloxConfig {
            resolution: c.resolution,
            ..Default::default()
        };
        alice_lol::roblox_export::node_to_fbx_roblox(n, p, &rc).unwrap();
    }));
    #[cfg(feature = "roblox")]
    w.push(("node_to_obj_roblox", "obj", |n, p, c| {
        let rc = alice_lol::roblox_export::RobloxConfig {
            resolution: c.resolution,
            ..Default::default()
        };
        alice_lol::roblox_export::node_to_obj_roblox(n, p, &rc).unwrap();
    }));
    w
}

/// file 全体の SHA-256 (書き出し関数ごと) 識別子の位置・値が変わると変わる
const GOLDEN: &[(&str, &str)] = &[
    (
        "node_to_stl",
        "c7143e5063a35d01208f06c19617066944778ca094917d99eb36208c462cc2ca",
    ),
    (
        "node_to_3mf",
        "4aebab3e8db50056f95d9fae1432bcbdd0a0be5b8f24246f4cdb6ed2618beb36",
    ),
    (
        "node_to_fbx",
        "063d6327e87154c4645c105812cb6c9a64b04a8bc21738e1b8207994528dd3c2",
    ),
    (
        "node_to_stl_dual_contouring",
        "ab87f0ddf2588aea8f065c25c8f698b4365c5da5c6a7a04c2dba093a3b3ffb97",
    ),
    (
        "node_to_3mf_dual_contouring",
        "2d3975625145f1923f00e0d5950839ee3fd0dcff0f6e6197ef18b56b363d207e",
    ),
    (
        "node_to_fbx_roblox",
        "dc1de32e3bd16af6ff47a3fbd7769a86e89f6f04e2680b8174ab649b88eea60b",
    ),
    (
        "node_to_obj_roblox",
        "bd045ae96ab9b0ba55e768f7199cf5c00bcb22d315ff314892e78150206b3010",
    ),
];

#[test]
fn every_lol_writer_stores_the_lol_producer_and_it_reads_back() {
    let other = Provenance::new([0x5a; 32]);
    let mut seen = 0;
    for (name, ext, write) in writers() {
        let path = temp(&format!("{name}.{ext}"));
        write(&node(), &path, &config());
        let (primary, producer) = provenance::read_with_producer(&path).unwrap();
        assert_eq!(
            primary.judge(&Provenance::sdf(), &[]).unwrap(),
            StoredSemantics::Current,
            "{name}: the primary is alice-sdf's current meaning ({primary})"
        );
        assert_eq!(producer, Some(lol_producer()), "{name}");
        provenance::check_producer(&path, &lol_producer()).unwrap();
        match provenance::check_producer(&path, &other) {
            Err(ProvenanceError::ProducerMismatch { stored, expected }) => {
                assert_eq!((stored, expected), (lol_producer(), other), "{name}");
            }
            r => panic!("{name}: another producer must be a mismatch, got {r:?}"),
        }
        std::fs::remove_file(&path).ok();
        seen += 1;
    }
    assert!(seen >= 5, "compared {seen} writers");
}

#[test]
fn the_stamp_is_the_sdf_primary_with_the_lol_producer() {
    assert_eq!(LOL_EXPORT_STAMP.primary, Provenance::sdf());
    assert_eq!(LOL_EXPORT_STAMP.producer, Some(lol_producer()));
    // the constant id_move_check reads is the identifier the exports write
    assert_eq!(
        Provenance::new(alice_lol::print_export::LOL_EXPORT_PRIMARY_SEMANTICS_ID),
        Provenance::sdf()
    );
}

/// STL: header の byte を直接読む、主の識別子の後は版 2 の配置、producer を除けば alice-sdf の
/// 既定の STL と byte 一致 (producer が無い時の配置は変わらない)、`stl_io` も読める
#[test]
fn the_stl_header_carries_the_producer_and_nothing_else_moves() {
    let path = temp("independent.stl");
    node_to_stl(&node(), &path, &config()).unwrap();
    let ours = std::fs::read(&path).unwrap();
    assert_eq!(&ours[..8], b"ALICESDF");
    assert_eq!(ours[8], 2, "version 2: a producer is present");
    // the primary slot holds the meaning identifier of the binary STL form (ALICE-Law G8)
    assert_eq!(
        &ours[9..41],
        &Provenance::sdf().slot_for(StoredForm::StlBinary)
    );
    assert_eq!(&ours[41..48], &[0u8; 7]);
    assert_eq!(&ours[48..80], &LOL_GEOMETRY_SEMANTICS_ID);

    let plain_path = temp("plain.stl");
    export_stl(&node_to_mesh(&node(), &config()), &plain_path).unwrap();
    let plain = std::fs::read(&plain_path).unwrap();
    assert_eq!(plain[8], 1);
    assert_eq!(&plain[48..80], &[0u8; 32]);
    let mut masked = ours.clone();
    masked[8] = 1;
    masked[48..80].copy_from_slice(&[0u8; 32]);
    assert_eq!(
        masked, plain,
        "only the version byte and the producer slot differ"
    );

    let mut f = std::fs::File::open(&path).unwrap();
    let parsed = stl_io::read_stl(&mut f).expect("stl_io reads the file");
    assert_eq!(
        u32::try_from(parsed.faces.len()).unwrap(),
        u32::from_le_bytes(ours[80..84].try_into().unwrap())
    );
    assert!(!parsed.faces.is_empty());
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(&plain_path).ok();
}

/// STORE の zip を自前で歩いて `name` の中身を返す (alice-sdf の reader を使わない)
fn stored_entry(zip: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut at = 0;
    while zip.get(at..at + 4)? == b"PK\x03\x04" {
        let u16_at = |o: usize| u16::from_le_bytes([zip[at + o], zip[at + o + 1]]) as usize;
        let u32_at =
            |o: usize| u32::from_le_bytes(zip[at + o..at + o + 4].try_into().unwrap()) as usize;
        assert_eq!(u16_at(8), 0, "STORE");
        let (size, name_len, extra_len) = (u32_at(18), u16_at(26), u16_at(28));
        let body = at + 30 + name_len + extra_len;
        if &zip[at + 30..at + 30 + name_len] == name.as_bytes() {
            return Some(zip[body..body + size].to_vec());
        }
        at = body + size;
    }
    None
}

#[test]
fn the_3mf_model_metadata_carries_the_producer() {
    let path = temp("independent.3mf");
    node_to_3mf(&node(), &path, &config()).unwrap();
    let model = String::from_utf8(
        stored_entry(&std::fs::read(&path).unwrap(), "3D/3dmodel.model").unwrap(),
    )
    .unwrap();
    let (key, value) = Provenance::sdf().named_for(StoredForm::ThreeMf);
    let primary = format!("<metadata name=\"{key}\">{value}</metadata>");
    let producer = format!(
        "<metadata name=\"alice:producer_semantics_id\">{}</metadata>",
        lol_producer().hex()
    );
    assert_eq!(model.matches(&primary).count(), 1, "{model}");
    assert_eq!(model.matches(&producer).count(), 1, "{model}");
    let resources = model.find("<resources>").unwrap();
    assert!(
        model.find(&primary).unwrap() < resources && model.find(&producer).unwrap() < resources
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn fbxcel_reads_the_producer_beside_the_creator() {
    let path = temp("independent.fbx");
    node_to_fbx(&node(), &path, &config()).unwrap();
    let f = std::io::BufReader::new(std::fs::File::open(&path).unwrap());
    let fbxcel::tree::any::AnyTree::V7400(_, tree, footer) =
        fbxcel::tree::any::AnyTree::from_seekable_reader(f).expect("fbxcel reads the file")
    else {
        panic!("not an FBX 7.4 tree")
    };
    footer.expect("fbxcel reads the footer");
    let header = tree
        .root()
        .first_child_by_name("FBXHeaderExtension")
        .expect("FBXHeaderExtension");
    let string_of = |name: &str| match header
        .first_child_by_name(name)
        .unwrap_or_else(|| panic!("{name}"))
        .attributes()
    {
        [fbxcel::low::v7400::AttributeValue::String(s)] => s.clone(),
        other => panic!("{name} attributes: {other:?}"),
    };
    assert_eq!(
        string_of("Creator"),
        format!(
            "ALICE-SDF {}",
            Provenance::sdf().tagged_for(StoredForm::FbxBinary)
        )
    );
    assert_eq!(
        string_of("ProducerSemanticsId"),
        format!("alice:producer_semantics_id={}", lol_producer().hex())
    );
    std::fs::remove_file(&path).ok();
}

/// The OBJ of `roblox_export` carries the producer in its header comment, read as text here (not through
/// alice-sdf's reader): `alice:producer_semantics_id=<hex of LOL_GEOMETRY_SEMANTICS_ID>` beside the primary
#[cfg(feature = "roblox")]
#[test]
fn the_obj_header_comment_carries_the_producer() {
    let path = temp("header.obj");
    let rc = alice_lol::roblox_export::RobloxConfig {
        resolution: config().resolution,
        ..Default::default()
    };
    alice_lol::roblox_export::node_to_obj_roblox(&node(), &path, &rc).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).ok();
    let hex = LOL_GEOMETRY_SEMANTICS_ID
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        });
    let header: Vec<&str> = text.lines().take_while(|l| l.starts_with('#')).collect();
    assert!(
        header.iter().any(
            |l| l.contains(&format!("alice:producer_semantics_id={hex}"))
                && l.contains(&Provenance::sdf().tagged_for(StoredForm::ObjText))
        ),
        "{header:?}"
    );
}

#[test]
fn every_lol_writer_matches_its_golden_bytes() {
    let record = std::env::var_os("LOL_DIGEST_RECORD").is_some();
    let mut compared = 0;
    for (name, ext, write) in writers() {
        let path = temp(&format!("golden-{name}.{ext}"));
        write(&node(), &path, &config());
        let got = sha_hex(&std::fs::read(&path).unwrap());
        std::fs::remove_file(&path).ok();
        if record {
            println!("    (\"{name}\", \"{got}\"),");
            continue;
        }
        let want = GOLDEN
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("{name}: no golden (record with LOL_DIGEST_RECORD=1)"))
            .1;
        assert_eq!(got, want, "{name}");
        compared += 1;
    }
    assert!(record || compared >= 5, "compared {compared} goldens");
}
