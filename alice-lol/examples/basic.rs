//! ALICE-LOL v0.1 demo: LOL DSL → SdfNode → GLSL shader

use alice_lol::lol;

fn main() {
    // ── Scene 1: Simple sphere ──
    let sphere = lol! { sphere(1.0) };
    println!("=== Scene 1: Sphere ===");
    println!(
        "SDF eval at origin: {}",
        alice_lol::eval(&sphere, alice_lol::Vec3::ZERO)
    );
    println!(
        "SDF eval at (2,0,0): {}",
        alice_lol::eval(&sphere, alice_lol::Vec3::new(2.0, 0.0, 0.0))
    );
    println!();

    // ── Scene 2: Smooth union of primitives with transforms ──
    let scene = lol! {
        field LOLDemo {
            smooth_union(0.3,
                sphere(1.0),
                translate(2.5, 0.0, 0.0,
                    box3d(0.5, 0.5, 0.5)
                ),
                translate(0.0, 2.0, 0.0,
                    torus(0.8, 0.2)
                )
            )
        }
    };

    println!("=== Scene 2: LOL Demo (SmoothUnion) ===");
    println!(
        "SDF eval at origin: {}",
        alice_lol::eval(&scene, alice_lol::Vec3::ZERO)
    );
    println!();

    // ── Scene 3: Subtraction (carved sphere) ──
    let carved = lol! {
        field Carved {
            subtract(
                sphere(1.0),
                translate(0.5, 0.0, 0.0,
                    box3d(0.6, 0.6, 0.6)
                )
            )
        }
    };

    println!("=== Scene 3: Carved Sphere ===");
    println!(
        "SDF eval at origin: {}",
        alice_lol::eval(&carved, alice_lol::Vec3::ZERO)
    );
    println!();

    // ── Scene 4: Modifiers (twisted cylinder) ──
    let twisted = lol! {
        field Twisted {
            twist(2.0,
                cylinder(0.5, 2.0)
            )
        }
    };

    println!("=== Scene 4: Twisted Cylinder ===");
    println!(
        "SDF eval at origin: {}",
        alice_lol::eval(&twisted, alice_lol::Vec3::ZERO)
    );
    println!();

    // ── Transpile Scene 2 to GLSL ──
    let glsl = alice_lol::to_glsl(&scene);
    println!("=== Generated GLSL ({} chars) ===", glsl.len());
    // Print first 800 chars for preview
    let preview_len = glsl.len().min(800);
    println!("{}", &glsl[..preview_len]);
    if glsl.len() > 800 {
        println!("... ({} more chars)", glsl.len() - 800);
    }
}
