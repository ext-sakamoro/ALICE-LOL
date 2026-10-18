//! stdlib の debug/release 間 mesh digest 一致 oracle (platform libm 直呼び回帰の歯止め)
//!
//! 以前は `hardsurface/skadis_sdf.rs` が centerline の点列を `.sin()`/`.cos()` の直呼びで
//! 組み立てており、release では LLVM が build 機の libm で定数畳み込みする一方、debug は実行時の
//! libm を呼ぶため、同じ LOL から profile によって異なる mesh bytes が出ていた (`skadis_hook_s()` /
//! `skadis_hook_j()`) すべての超越関数呼び出しを `alice_det_math` 経由に routing し
//! (`clippy.toml` の `disallowed-methods` が回帰を止める)、本 oracle で全ケース一致を確認している
//!
//! golden は release の build で記録する (`LOL_DIGEST_RECORD=1 cargo test --release --test stdlib_profile_determinism_oracle`)
//! 再び profile 間で不一致になったら、どのケースかがこの試験の失敗メッセージに出る

use alice_lol::print_export::{node_to_mesh, PrintConfig};
use alice_lol::runtime_parser::parse_lol;
use alice_sdf::mesh::Mesh;
use glam::Vec3;
use sha2::{Digest, Sha256};

#[path = "common/golden.rs"]
mod golden;

struct Case {
    lol: &'static str,
    bounds_min: Vec3,
    bounds_max: Vec3,
    resolution: usize,
}

/// text-to-print-ios `rust-bridge/src/lib.rs` の 12 `PARITY_CASES` に `skadis_hook_j()` を足した
/// 13 ケース bounds は alice-lol 自身の `node_to_mesh`(`PrintConfig`) 向けに個別に見積もった値
/// (text-to-print-ios 側は tight AABB 自動fit を使うため、ここの digest は golden 含め別物)
const CASES: &[Case] = &[
    Case {
        lol: "box3d(5, 5, 2.5)",
        bounds_min: Vec3::new(-4.0, -4.0, -4.0),
        bounds_max: Vec3::new(4.0, 4.0, 4.0),
        resolution: 64,
    },
    Case {
        lol: "sphere(8)",
        bounds_min: Vec3::splat(-9.0),
        bounds_max: Vec3::splat(9.0),
        resolution: 64,
    },
    Case {
        lol: "rotate(90, 0, 0, cylinder(3, 6))",
        bounds_min: Vec3::splat(-8.0),
        bounds_max: Vec3::splat(8.0),
        resolution: 64,
    },
    Case {
        lol: "rotate(180, 0, 0, box3d(4, 2, 1))",
        bounds_min: Vec3::splat(-4.0),
        bounds_max: Vec3::splat(4.0),
        resolution: 64,
    },
    Case {
        lol: "rotate(-0.0, 0, 0, torus(6, 1.5))",
        bounds_min: Vec3::splat(-9.0),
        bounds_max: Vec3::splat(9.0),
        resolution: 64,
    },
    Case {
        lol: "rotate(30, 45, 60, box3d(6, 3, 2))",
        bounds_min: Vec3::splat(-5.0),
        bounds_max: Vec3::splat(5.0),
        resolution: 64,
    },
    Case {
        lol: "smooth_union(1.5, sphere(4), translate(5, 0, 0, sphere(3)))",
        bounds_min: Vec3::new(-5.0, -5.0, -5.0),
        bounds_max: Vec3::new(9.0, 5.0, 5.0),
        resolution: 64,
    },
    Case {
        lol: "subtract(box3d(8, 8, 4), translate(0, 0, 2, cylinder(3, 4)))",
        bounds_min: Vec3::splat(-5.0),
        bounds_max: Vec3::splat(5.0),
        resolution: 64,
    },
    Case {
        lol: "twist(0.2, box3d(3, 3, 10))",
        bounds_min: Vec3::splat(-6.0),
        bounds_max: Vec3::splat(6.0),
        resolution: 64,
    },
    Case {
        lol: "rotate_q(0, 0, 0.7071068, 0.7071068, box3d(5, 2, 1))",
        bounds_min: Vec3::splat(-4.0),
        bounds_max: Vec3::splat(4.0),
        resolution: 64,
    },
    Case {
        lol: "shopping_cart_coin(22.8, 1.7)",
        bounds_min: Vec3::new(-12.0, -12.0, -2.0),
        bounds_max: Vec3::new(12.0, 12.0, 2.0),
        resolution: 64,
    },
    Case {
        lol: "skadis_hook_s()",
        bounds_min: Vec3::new(-5.0, -48.0, -6.0),
        bounds_max: Vec3::new(30.0, 6.0, 6.0),
        resolution: 64,
    },
    Case {
        lol: "skadis_hook_j()",
        bounds_min: Vec3::new(-6.0, -74.0, -9.0),
        bounds_max: Vec3::new(36.0, 6.0, 9.0),
        resolution: 64,
    },
];

/// 頂点の位置 (f32 の bits、little endian) と index の SHA-256
fn mesh_digest(mesh: &Mesh) -> String {
    let mut h = Sha256::new();
    for v in &mesh.vertices {
        for c in v.position.to_array() {
            h.update(c.to_bits().to_le_bytes());
        }
    }
    for i in &mesh.indices {
        h.update(i.to_le_bytes());
    }
    golden::hex32(&h.finalize())
}

fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stdlib_profile_digest_golden.txt")
}

#[test]
fn stdlib_mesh_digest_matches_release_profile_golden() {
    let now: Vec<golden::Row> = CASES
        .iter()
        .map(|c| {
            let node = parse_lol(c.lol).unwrap_or_else(|e| panic!("{}: parse failed: {e}", c.lol));
            let config = PrintConfig {
                resolution: c.resolution,
                bounds_min: c.bounds_min,
                bounds_max: c.bounds_max,
                scale_mm: 1.0,
            };
            let mesh = node_to_mesh(&node, &config);
            assert!(
                !mesh.indices.is_empty(),
                "{}: 0 三角形 (bounds / resolution を見直す)",
                c.lol
            );
            (
                c.lol.to_string(),
                (mesh.indices.len() / 3) as u64,
                mesh_digest(&mesh),
            )
        })
        .collect();

    if std::env::var("LOL_DIGEST_RECORD").as_deref() == Ok("1") {
        // deliberately a build-profile-constant condition, not a runtime one
        #[allow(clippy::assertions_on_constants)]
        {
            assert!(
                !cfg!(debug_assertions),
                "golden は --release で記録する (release を正典とする既存の device-parity golden と合わせる)"
            );
        }
        std::fs::write(golden_path(), golden::render(&now)).unwrap();
        return;
    }

    let golden_text = std::fs::read_to_string(golden_path()).expect(
        "golden 未記録 (先に `LOL_DIGEST_RECORD=1 cargo test --release --test stdlib_profile_determinism_oracle` を実行する)",
    );
    let want =
        golden::parse(&golden_text).unwrap_or_else(|e| panic!("{}: {e}", golden_path().display()));
    let compared =
        golden::compare(&now, &want).unwrap_or_else(|e| panic!("golden と違う (最初の違い): {e}"));
    assert_eq!(compared, CASES.len());
}
