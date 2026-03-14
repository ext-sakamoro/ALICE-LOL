//! ALICE-LOL CompiledSdf デモ
//! 高速評価: 単一点、バッチ SIMD、法線計算

use alice_lol::{
    eval, eval_compiled, eval_compiled_batch_simd, eval_compiled_normal, lol, CompiledSdf, Vec3,
};

fn main() {
    println!("=== ALICE-LOL CompiledSdf Demo ===\n");

    // ── シーン構築 ──
    let scene = lol! {
        smooth_union(0.2,
            sphere(1.0),
            translate(2.5, 0.0, 0.0,
                smooth_union(0.15,
                    torus(0.8, 0.3),
                    translate(0.0, 1.0, 0.0, capsule(0.2, 0.5))
                )
            )
        )
    };

    // ── コンパイル ──
    let compiled = CompiledSdf::compile(&scene);
    println!("  CompiledSdf 生成完了");

    // ── 単一点評価の比較 ──
    println!("\n--- 単一点評価 ---");
    let test_points = [
        Vec3::ZERO,
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(2.5, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 5.0),
    ];

    for p in &test_points {
        let d_direct = eval(&scene, *p);
        let d_compiled = eval_compiled(&compiled, *p);
        let diff = (d_direct - d_compiled).abs();
        println!(
            "  ({:.1},{:.1},{:.1}): direct={:.6} compiled={:.6} diff={:.2e}",
            p.x, p.y, p.z, d_direct, d_compiled, diff
        );
    }

    // ── バッチ SIMD 評価 ──
    println!("\n--- バッチ SIMD 評価 (X軸上 16点) ---");
    let batch_points: Vec<Vec3> = (0..16)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let x = i as f32 * 0.25;
            Vec3::new(x, 0.0, 0.0)
        })
        .collect();
    let batch_results = eval_compiled_batch_simd(&compiled, &batch_points);
    for (p, d) in batch_points.iter().zip(batch_results.iter()) {
        println!("  x={:.2}: d={d:.4}", p.x);
    }

    // ── 法線計算 ──
    println!("\n--- 法線計算 (CompiledSdf) ---");
    let normal_points = [
        Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
        Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(2.5, 0.8, 0.0),
    ];
    for p in &normal_points {
        let n = eval_compiled_normal(&compiled, *p, 0.001);
        println!(
            "  ({:.1},{:.1},{:.1}): normal=({:.4},{:.4},{:.4}) |n|={:.4}",
            p.x,
            p.y,
            p.z,
            n.x,
            n.y,
            n.z,
            n.length()
        );
    }

    // ── 変数キャプチャ + CompiledSdf ──
    println!("\n--- 変数キャプチャ + CompiledSdf ---");
    let r = 2.0_f32;
    let offset = 3.0_f32;
    let parametric = lol! {
        smooth_union(0.2,
            sphere({r}),
            translate({offset}, 0.0, 0.0, sphere({r * 0.5}))
        )
    };
    let compiled_param = CompiledSdf::compile(&parametric);
    let d = eval_compiled(&compiled_param, Vec3::ZERO);
    println!("  r={r}, offset={offset}: d(origin) = {d:.4}");

    println!("\n=== CompiledSdf Demo Complete ===");
}
