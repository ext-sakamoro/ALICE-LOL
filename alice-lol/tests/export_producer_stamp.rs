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
    export_stl, node_to_3mf, node_to_3mf_dual_contouring, node_to_fbx, node_to_mesh, node_to_stl,
    node_to_stl_dual_contouring, PrintConfig, LOL_EXPORT_STAMP,
};
use alice_lol::{lol, SdfNode, Vec3};
use alice_sdf::provenance::{self, Provenance, ProvenanceError};
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
    w
}

/// file 全体の SHA-256 (書き出し関数ごと) 識別子の位置・値が変わると変わる
const GOLDEN: &[(&str, &str)] = &[
    (
        "node_to_stl",
        "663ddc62fb2e0fe794810a70cdbe62f8a2b442ad0928f83a82c2854d1ac936a5",
    ),
    (
        "node_to_3mf",
        "fb0e4aed519e202cc9c859eba947674c1331f321f21085a18f3d5a337fca7f3c",
    ),
    (
        "node_to_fbx",
        "1e318cdea7b0f48081046c5e25266807dfa76769032b2af4ceb49ad7c9db1c55",
    ),
    (
        "node_to_stl_dual_contouring",
        "3f71d2eecb9939e73121465419b8df7e64c5fb53132114b41aafb107e7a6f4c1",
    ),
    (
        "node_to_3mf_dual_contouring",
        "1aa24a208de3141f7ddc2686b52a58ec1c7d4c5d731c6b7cb154405042ee9533",
    ),
    (
        "node_to_fbx_roblox",
        "faec2cb8129daa21375e9c5e438736983ab5bace9b1f79cdedc70f444dce512b",
    ),
];

#[test]
fn every_lol_writer_stores_the_lol_producer_and_it_reads_back() {
    let other = Provenance::new([0x5a; 32]);
    let mut seen = 0;
    for (name, ext, write) in writers() {
        let path = temp(&format!("{name}.{ext}"));
        write(&node(), &path, &config());
        assert_eq!(
            provenance::read_with_producer(&path).unwrap(),
            (Provenance::sdf(), Some(lol_producer())),
            "{name}"
        );
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
    assert_eq!(&ours[9..41], &Provenance::sdf().semantics_id);
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
    let primary = format!(
        "<metadata name=\"alice:semantics_id\">{}</metadata>",
        Provenance::sdf().hex()
    );
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
        format!("ALICE-SDF alice:semantics_id={}", Provenance::sdf().hex())
    );
    assert_eq!(
        string_of("ProducerSemanticsId"),
        format!("alice:producer_semantics_id={}", lol_producer().hex())
    );
    std::fs::remove_file(&path).ok();
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
