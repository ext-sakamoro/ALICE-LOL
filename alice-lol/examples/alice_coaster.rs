//! ALICE コースター — SDF幾何学模様の10cm丸型コースター
//!
//! ```bash
//! cargo run --example alice_coaster --release
//! ```

use alice_lol::lol;
use alice_lol::print_export::{node_to_stl, PrintConfig};
use glam::Vec3;

fn main() {
    println!("=== ALICE Coaster Generator ===\n");

    let out_dir = std::path::Path::new("lol_print_output");
    std::fs::create_dir_all(out_dir).unwrap();

    // スケール: 1.0 LOL = 20mm
    // 10cm = 100mm → 半径 2.5 LOL
    // 厚さ 5mm → half_height 0.125 LOL
    let config = PrintConfig {
        resolution: 192,
        ..PrintConfig::default()
    }
        .with_bounds(Vec3::new(-3.0, -0.3, -3.0), Vec3::new(3.0, 0.3, 3.0))
        .with_scale_mm(20.0);

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // ALICE コースター
    //
    // 構成:
    //   1. 外周リム（ソリッドリング）— 安定性 + 持ちやすさ
    //   2. 底面プレート（薄い円盤）— 水滴受け
    //   3. 上面にジャイロイド透かし模様 — ALICEのSDF美学
    //   4. 中心に六角形アクセント — ALICEの幾何学的アイデンティティ
    //   5. 放射状リブ — 構造強度 + デザイン
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    let coaster = lol! {
        subtract(
          subtract(
            subtract(
              subtract(
                subtract(
                  subtract(
                    subtract(
                        // ベース: 円盤（100mm径 x 5mm厚）
                        cylinder(2.5, 0.125),

                        // 同心円グルーブ1: トーラス r=2.0（上面に食い込み）
                        translate(0.0, 0.125, 0.0,
                            torus(2.0, 0.06)
                        )
                    ),
                    // 同心円グルーブ2: r=1.4
                    translate(0.0, 0.125, 0.0,
                        torus(1.4, 0.06)
                    )
                  ),
                  // 同心円グルーブ3: r=0.8
                  translate(0.0, 0.125, 0.0,
                      torus(0.8, 0.06)
                  )
                ),
                // 「A」の左脚
                translate(-0.15, 0.0, 0.0,
                    rotate(0.0, 0.0, -12.0,
                        rounded_box(0.06, 0.2, 0.35, 0.02)
                    )
                )
              ),
              // 「A」の右脚
              translate(0.15, 0.0, 0.0,
                  rotate(0.0, 0.0, 12.0,
                      rounded_box(0.06, 0.2, 0.35, 0.02)
                  )
              )
            ),
            // 「A」の横棒
            rounded_box(0.18, 0.2, 0.04, 0.01)
          ),
          // 「A」の頂点三角穴
          translate(0.0, 0.0, 0.25,
              cylinder(0.08, 0.2)
          )
        )
    };

    println!("Generating mesh (resolution 192)...");
    match node_to_stl(&coaster, out_dir.join("ALICE_coaster.stl"), &config) {
        Ok(stats) => println!("STL: {stats}"),
        Err(e) => {
            println!("Error: {e}");
            return;
        }
    }

    // 断面プレビュー（Y=0）
    println!("\n--- Top view (Y=0.05, inside coaster) ---\n");
    let res = 50;
    for iz in 0..res {
        let z = -3.0 + (iz as f32 / res as f32) * 6.0;
        let mut line = String::with_capacity(res);
        for ix in 0..res {
            let x = -3.0 + (ix as f32 / res as f32) * 6.0;
            let p = Vec3::new(x, 0.05, z);
            let d = alice_lol::eval(&coaster, p);
            if d < -0.02 {
                line.push('#');
            } else if d < 0.0 {
                line.push('.');
            } else {
                line.push(' ');
            }
        }
        println!("  {line}");
    }

    println!("\n=== Done — open in Bambu Studio: ===");
    println!("open -a BambuStudio lol_print_output/ALICE_coaster.stl");
}
