//! 超ゴージャス王冠 — UE5品質の装飾王冠をRoblox用OBJとして出力
//!
//! ```bash
//! cargo run --example royal_crown --features roblox --release
//! ```

#[cfg(feature = "roblox")]
fn main() {
    use alice_lol::roblox_export::{node_to_obj_roblox, RobloxConfig};
    use alice_lol::runtime_parser::parse_lol;

    // ── 超ゴージャス王冠 LOL 定義 ──
    //
    // 構造:
    //   1. ベースバンド: 厚みのあるトーラスリング
    //   2. 5本の尖塔: polar_repeat で配置した rounded_cone
    //   3. 尖塔先端の宝石: 球体を各尖塔の頂上に配置
    //   4. 中間装飾: 尖塔間の小さな突起
    //   5. ベース装飾リム: 上下の縁取りリング
    //   6. round で全体を宝飾品品質に仕上げ
    let crown_lol = r#"
        round(0.02,
            smooth_union(0.06,
                smooth_union(0.04,
                    smooth_union(0.03,
                        onion(0.04,
                            subtract(
                                cylinder(0.9, 0.12),
                                cylinder(0.78, 0.20)
                            )
                        ),
                        smooth_union(0.05,
                            translate(0.0, 0.04, 0.0,
                                torus(0.88, 0.035)
                            ),
                            translate(0.0, -0.04, 0.0,
                                torus(0.88, 0.035)
                            )
                        )
                    ),
                    smooth_union(0.04,
                        polar_repeat(5,
                            translate(0.85, 0.0, 0.0,
                                smooth_union(0.06,
                                    smooth_union(0.04,
                                        rotate(0.0, 0.0, 90.0,
                                            rounded_cone(0.10, 0.03, 0.35)
                                        ),
                                        translate(0.0, 0.38, 0.0,
                                            sphere(0.07)
                                        )
                                    ),
                                    translate(0.0, 0.20, 0.0,
                                        scale(0.8,
                                            torus(0.06, 0.025)
                                        )
                                    )
                                )
                            )
                        ),
                        polar_repeat(5,
                            translate(0.87, 0.0, 0.0,
                                rotate(0.0, 0.0, 0.0,
                                    smooth_union(0.03,
                                        rotate(0.0, 0.0, 90.0,
                                            rounded_cone(0.06, 0.02, 0.18)
                                        ),
                                        translate(0.0, 0.20, 0.0,
                                            sphere(0.045)
                                        )
                                    )
                                )
                            )
                        )
                    )
                ),
                smooth_union(0.03,
                    polar_repeat(10,
                        translate(0.88, 0.0, 0.0,
                            sphere(0.035)
                        )
                    ),
                    polar_repeat(20,
                        translate(0.85, 0.0, 0.0,
                            scale_non_uniform(1.0, 0.6, 1.0,
                                sphere(0.02)
                            )
                        )
                    )
                )
            )
        )
    "#;

    let node = parse_lol(crown_lol).expect("LOL parse failed");

    // MeshPart上限 (10,000 tri) に収める — 複雑形状なので解像度を絞る
    let config = RobloxConfig {
        resolution: 42,
        bounds_min: glam::Vec3::new(-1.5, -0.8, -1.5),
        bounds_max: glam::Vec3::new(1.5, 0.8, 1.5),
        scale_studs: 1.5,
        max_triangles: 10_000,
        max_size_studs: glam::Vec3::new(10.0, 10.0, 10.0),
    };

    let out_path = "royal_crown.obj";
    match node_to_obj_roblox(&node, out_path, &config) {
        Ok(stats) => {
            println!("=== Royal Crown ===");
            println!("{stats}");
            if stats.validation.is_valid() {
                println!("Roblox import ready!");
            } else {
                println!("Warning: {}", stats.validation);
            }
        }
        Err(e) => eprintln!("Export error: {e}"),
    }
}

#[cfg(not(feature = "roblox"))]
fn main() {
    eprintln!("roblox feature required: cargo run --example royal_crown --features roblox --release");
}
