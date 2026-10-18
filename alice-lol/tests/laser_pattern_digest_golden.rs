//! `laser_pattern` の debug/release・arch 間 point digest 一致 oracle
//!
//! SVG として書き出す laser-cut 命令は mesh と同じく Law が統一を要求する出力artifact
//! この file が加わる前、`hatch`/`density_hatch`/`guilloche`/`lissajous`/`rose`/
//! `phyllotaxis` は platform libm (`.sin()`/`.cos()` 直呼び) を使っており、
//! `det_math::sin64`/`cos64` 経由に route する前は release が build 機の libm で
//! 定数畳み込みする一方 debug は実行時に呼ぶため、同じ呼び出しから profile に
//! よって異なる byte が出ていた (`stdlib_profile_determinism_oracle.rs` と同型の
//! 欠陥、detail は `alice-lol/src/laser_pattern.rs` の各関数の呼び出しを参照)
//!
//! golden は release の build で記録する
//! (`LOL_DIGEST_RECORD=1 cargo test --release --test laser_pattern_digest_golden`)

use alice_lol::laser_pattern::{
    density_hatch, guilloche, hatch, lissajous, phyllotaxis, rose, Bounds, LaserElement,
};
use sha2::{Digest, Sha256};

#[path = "common/golden.rs"]
mod golden;

/// 各要素の座標 (f64 の bits、little endian) の SHA-256、要素の種類・個数も混ぜる
fn elements_digest(elements: &[LaserElement]) -> (usize, String) {
    let mut h = Sha256::new();
    for e in elements {
        match e {
            LaserElement::Line(x1, y1, x2, y2) => {
                h.update(b"L");
                for c in [x1, y1, x2, y2] {
                    h.update(c.to_bits().to_le_bytes());
                }
            }
            LaserElement::Circle(cx, cy, r) => {
                h.update(b"C");
                for c in [cx, cy, r] {
                    h.update(c.to_bits().to_le_bytes());
                }
            }
            LaserElement::Dot(x, y) => {
                h.update(b"D");
                for c in [x, y] {
                    h.update(c.to_bits().to_le_bytes());
                }
            }
            LaserElement::Polyline(points) => {
                h.update(b"P");
                for (x, y) in points {
                    h.update(x.to_bits().to_le_bytes());
                    h.update(y.to_bits().to_le_bytes());
                }
            }
        }
    }
    (elements.len(), golden::hex32(&h.finalize()))
}

const fn bounds() -> Bounds {
    Bounds::new(-50.0, -50.0, 100.0, 100.0)
}

struct Case {
    name: &'static str,
    elements: fn() -> Vec<LaserElement>,
}

const CASES: &[Case] = &[
    Case {
        name: "hatch(37, 2.3)",
        elements: || hatch(37.0, 2.3, &bounds()),
    },
    Case {
        name: "density_hatch(0.5, 4.0, 15)",
        elements: || {
            density_hatch(0.5, 4.0, 15.0, &bounds(), &|x, y| {
                1.0 - x.mul_add(x, y * y).sqrt() / 70.0
            })
        },
    },
    Case {
        name: "guilloche(31, 13, 2.0, steps=720)",
        elements: || guilloche(31.0, 13.0, 2.0, 720, &bounds()),
    },
    Case {
        name: "lissajous(3, 4, pi/2, steps=360)",
        elements: || lissajous(3.0, 4.0, std::f64::consts::FRAC_PI_2, 40.0, 360, &bounds()),
    },
    Case {
        name: "rose(5, steps=720)",
        elements: || rose(5.0, 40.0, 720, &bounds()),
    },
    Case {
        name: "phyllotaxis(500, 2.0)",
        elements: || phyllotaxis(500, 2.0, &bounds()),
    },
];

fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/laser_pattern_digest_golden.txt")
}

#[test]
fn laser_pattern_digest_matches_release_profile_golden() {
    let now: Vec<golden::Row> = CASES
        .iter()
        .map(|c| {
            let elements = (c.elements)();
            assert!(!elements.is_empty(), "{}: 要素が 0 件", c.name);
            let (n, digest) = elements_digest(&elements);
            (c.name.to_string(), n as u64, digest)
        })
        .collect();

    if std::env::var("LOL_DIGEST_RECORD").as_deref() == Ok("1") {
        #[allow(clippy::assertions_on_constants)]
        {
            assert!(
                !cfg!(debug_assertions),
                "golden は --release で記録する (release を正典とする既存の digest golden と合わせる)"
            );
        }
        std::fs::write(golden_path(), golden::render(&now)).unwrap();
        return;
    }

    let golden_text = std::fs::read_to_string(golden_path()).expect(
        "golden 未記録 (先に `LOL_DIGEST_RECORD=1 cargo test --release --test laser_pattern_digest_golden` を実行する)",
    );
    let want =
        golden::parse(&golden_text).unwrap_or_else(|e| panic!("{}: {e}", golden_path().display()));
    let compared =
        golden::compare(&now, &want).unwrap_or_else(|e| panic!("golden と違う (最初の違い): {e}"));
    assert_eq!(compared, CASES.len());
}
