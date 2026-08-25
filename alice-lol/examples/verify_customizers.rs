//! Verify text-to-print customizer archetypes render as expected shape
//!
//! Renders each broken-reported archetype at preview resolution (96)
//! and computes fill ratio (mesh_volume / aabb_volume) to detect
//! "just a box" bug pattern (ratio > 0.85 = suspicious).
//!
//! ```bash
//! cargo run --example verify_customizers --release
//! ```
//!
//! Related memory: `feedback_alice_sdf_rounded_box_six_face_inflate`,
//! `success_skadis_panel_canonical_alignment_2026_08_09`,
//! `success_text_to_print_customizer_verification_methodology_2026_08_25`

use alice_lol::runtime_parser::parse_lol;
use alice_sdf::mesh::{sdf_to_mesh, MarchingCubesConfig, Mesh};
use glam::Vec3;

/// Compute signed volume of a closed triangle mesh
/// Formula: V = (1/6) Σ (v0 · (v1 × v2))
fn signed_mesh_volume(mesh: &Mesh) -> f32 {
    let mut vol = 0.0_f32;
    for tri in mesh.indices.chunks_exact(3) {
        let v0 = mesh.vertices[tri[0] as usize].position;
        let v1 = mesh.vertices[tri[1] as usize].position;
        let v2 = mesh.vertices[tri[2] as usize].position;
        vol += v0.dot(v1.cross(v2)) / 6.0;
    }
    vol.abs()
}

/// ASCII STL writer (minimal, for user visual verification)
fn write_stl_ascii(mesh: &Mesh, path: &std::path::Path) -> std::io::Result<()> {
    use std::io::Write;
    let mut w = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(w, "solid verify")?;
    for tri in mesh.indices.chunks_exact(3) {
        let v0 = mesh.vertices[tri[0] as usize].position;
        let v1 = mesh.vertices[tri[1] as usize].position;
        let v2 = mesh.vertices[tri[2] as usize].position;
        let n = (v1 - v0).cross(v2 - v0).normalize_or_zero();
        writeln!(w, "  facet normal {} {} {}", n.x, n.y, n.z)?;
        writeln!(w, "    outer loop")?;
        writeln!(w, "      vertex {} {} {}", v0.x, v0.y, v0.z)?;
        writeln!(w, "      vertex {} {} {}", v1.x, v1.y, v1.z)?;
        writeln!(w, "      vertex {} {} {}", v2.x, v2.y, v2.z)?;
        writeln!(w, "    endloop")?;
        writeln!(w, "  endfacet")?;
    }
    writeln!(w, "endsolid verify")?;
    Ok(())
}

/// Compute AABB from mesh vertices (empirical, more reliable than
/// SDF-derived AABB which returns 0 for many primitives)
fn mesh_aabb(mesh: &Mesh) -> (Vec3, Vec3) {
    if mesh.vertices.is_empty() {
        return (Vec3::ZERO, Vec3::ZERO);
    }
    let mut min = mesh.vertices[0].position;
    let mut max = min;
    for v in &mesh.vertices {
        min = min.min(v.position);
        max = max.max(v.position);
    }
    (min, max)
}

struct Case {
    label: &'static str,
    lol: &'static str,
    /// Expected fill ratio range (mesh_vol / aabb_vol)
    /// Working archetypes: 0.20-0.75, Box-bug: > 0.85
    expected: &'static str,
}

fn main() {
    let cases = [
        // ── User-reported "just a box" (broken candidates) ──
        Case {
            label: "tissue_box_cover (231, 116, 53)",
            lol: "tissue_box_cover(231, 116, 53)",
            expected: "hollow shell (< 0.60)",
        },
        Case {
            label: "storage_box (150, 100, 60)",
            lol: "storage_box(150, 100, 60)",
            expected: "hollow box (< 0.30)",
        },
        Case {
            label: "card_tray (63, 88, 12)",
            lol: "card_tray(63, 88, 12)",
            expected: "tray w/ notch (< 0.70)",
        },
        Case {
            label: "token_well (30, 4, 20)",
            lol: "token_well(30, 4, 20)",
            expected: "4 wells (< 0.70)",
        },
        Case {
            label: "wrench_holder (8, 19, 6)",
            lol: "wrench_holder(8, 19, 6)",
            expected: "6 slots (< 0.75)",
        },
        Case {
            label: "esp32_enclosure (60, 30, 20)",
            lol: "esp32_enclosure(60, 30, 20)",
            expected: "PCB enclosure (< 0.60)",
        },
        Case {
            label: "battery_18650_holder (4, 21, 6)",
            lol: "battery_18650_holder(4, 21, 6)",
            expected: "4 cylinder wells (< 0.75)",
        },
        Case {
            label: "toothbrush_holder (4, 12, 60)",
            lol: "toothbrush_holder(4, 12, 60)",
            expected: "4 wells (< 0.75)",
        },
        Case {
            label: "drill_bit_holder (2, 10, 8)",
            lol: "drill_bit_holder(2, 10, 8)",
            expected: "8 sized wells (< 0.75)",
        },
        Case {
            label: "pliers_rack (4, 150, 40)",
            lol: "pliers_rack(4, 150, 40)",
            expected: "4 slots (< 0.75)",
        },
        Case {
            label: "egg_tray (3, 4, 25)",
            lol: "egg_tray(3, 4, 25)",
            expected: "12 cups (< 0.55)",
        },
        Case {
            label: "cutting_board_rack (3, 12, 220)",
            lol: "cutting_board_rack(3, 12, 220)",
            expected: "3 slots (< 0.75)",
        },
        // ── Working reference (baseline for comparison) ──
        Case {
            label: "REF: pen_cup (30, 100)",
            lol: "pen_cup(30, 100)",
            expected: "hollow cylinder (~0.15-0.25)",
        },
        Case {
            label: "REF: gridfinity_bin(2,2,6)",
            lol: "gridfinity_bin(2, 2, 6)",
            expected: "hollow bin (< 0.35)",
        },
        Case {
            label: "REF: sticky_note_holder(80, 80, 30)",
            lol: "sticky_note_holder(80, 80, 30)",
            expected: "hollow (< 0.60)",
        },
        // ── Sanity: solid box (must be ~1.0) ──
        Case {
            label: "SANITY: box3d(50, 30, 20)",
            lol: "box3d(50, 30, 20)",
            expected: "SOLID (~1.0)",
        },
    ];

    println!(
        "{:<48} | {:>8} | {:>9} | {:>10} | {:>7} | {}",
        "archetype", "tri", "aabb_vol", "mesh_vol", "ratio", "expected"
    );
    println!("{}", "-".repeat(140));

    let mut box_bug_hits = Vec::new();

    for case in cases {
        let node = match parse_lol(case.lol) {
            Ok(n) => n,
            Err(e) => {
                println!("{:<48} | PARSE FAIL: {e:?}", case.label);
                continue;
            }
        };

        // Adaptive probe: escalate resolution / shrink bbox until we find
        // the shape (compute_tight_aabb returns 0 for many primitives,
        // and a coarse probe misses small/thin shapes)
        let probe_stages: &[(f32, usize)] = &[(500.0, 32), (250.0, 64), (100.0, 96), (50.0, 128)];
        let mut probe_min = Vec3::ZERO;
        let mut probe_max = Vec3::ZERO;
        for &(bbox, res) in probe_stages {
            let cfg = MarchingCubesConfig {
                resolution: res,
                ..Default::default()
            };
            let m = sdf_to_mesh(&node, Vec3::splat(-bbox), Vec3::splat(bbox), &cfg);
            if !m.vertices.is_empty() {
                let (pmin, pmax) = mesh_aabb(&m);
                probe_min = pmin;
                probe_max = pmax;
                break;
            }
        }
        let margin = Vec3::splat(5.0);
        let min = probe_min - margin;
        let max = probe_max + margin;

        let cfg = MarchingCubesConfig {
            resolution: 96,
            ..Default::default()
        };
        let mesh = sdf_to_mesh(&node, min, max, &cfg);

        let (bmin, bmax) = mesh_aabb(&mesh);
        let ext = bmax - bmin;
        let aabb_vol = ext.x * ext.y * ext.z;
        let mesh_vol = signed_mesh_volume(&mesh);
        let ratio = if aabb_vol > 0.0 {
            mesh_vol / aabb_vol
        } else {
            0.0
        };

        let flag = if ratio > 0.85 {
            " ← BOX-BUG"
        } else if case.label.starts_with("SANITY") {
            ""
        } else if case.label.starts_with("REF") {
            ""
        } else if ratio > 0.75 {
            " ← suspicious"
        } else {
            ""
        };

        if ratio > 0.85 && !case.label.starts_with("SANITY") {
            box_bug_hits.push(case.label);
        }

        println!(
            "{:<48} | {:>8} | {:>9.0} | {:>10.0} | {:>6.3} | {}{}",
            case.label,
            mesh.triangle_count(),
            aabb_vol,
            mesh_vol,
            ratio,
            case.expected,
            flag,
        );

        // Export STL to /tmp/verify_customizers/ so user can open in
        // Bambu Studio / MeshLab and confirm the true 3D shape
        // (bypasses the app's egui preview renderer entirely)
        if std::env::var("EXPORT_STL").is_ok() && !mesh.vertices.is_empty() {
            let name: String = case
                .label
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            let dir = std::path::Path::new("/tmp/verify_customizers");
            let _ = std::fs::create_dir_all(dir);
            let path = dir.join(format!("{name}.stl"));
            let _ = write_stl_ascii(&mesh, &path);
        }
    }

    println!();
    println!("=== Summary ===");
    println!(
        "Box-bug hits (ratio > 0.85, excluding SANITY solid): {}",
        box_bug_hits.len()
    );
    for label in &box_bug_hits {
        println!("  - {label}");
    }
    if box_bug_hits.is_empty() {
        println!("No box-bug detected. If user still sees 'just a box',");
        println!("check: (a) preview renderer path (not mesh), (b) to_z_up axis flip,");
        println!("(c) DC/MC route selection.");
    }
}
