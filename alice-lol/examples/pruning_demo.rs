//! ALICE-LOL 空間枝刈りコンパイラ デモ
//!
//! 離れた複数ボディのシーンで、セルごとの枝刈り効果を可視化する。

use alice_lol::pruned_compile::{analyze_grid, count_nodes, summary, CellKind, PruneConfig};
use alice_lol::{lol, Vec3};

fn main() {
    println!("=== ALICE-LOL 空間枝刈りコンパイラ ===\n");

    // ── シーン: 離れた 3 ボディ ──
    let scene = lol! {
        field PruneDemo {
            smooth_union(0.3,
                sphere(1.0),
                translate(6.0, 0.0, 0.0, box3d(0.8, 0.8, 0.8)),
                translate(0.0, 6.0, 0.0, torus(0.8, 0.3))
            )
        }
    };

    let original_nodes = count_nodes(&scene);
    println!("元のノード数: {original_nodes}");

    // ── 4×4×4 グリッドで解析 ──
    let config = PruneConfig {
        aabb_min: Vec3::splat(-3.0),
        aabb_max: Vec3::new(9.0, 9.0, 3.0),
        grid_resolution: 4,
    };

    let result = analyze_grid(&scene, &config);
    println!("\n{}", summary(&result));

    // ── セルごとの枝刈り効果 ──
    println!("\n--- Crossing セルの枝刈り効果 ---");
    for cell in &result.cells {
        if cell.kind != CellKind::Crossing {
            continue;
        }
        if let Some(ref pruned) = cell.pruned_node {
            let pruned_nodes = count_nodes(pruned);
            let reduction = if original_nodes > 0 {
                #[allow(clippy::cast_precision_loss)]
                let pct = (1.0 - pruned_nodes as f64 / original_nodes as f64) * 100.0;
                format!("{pct:.0}% 削減")
            } else {
                String::from("N/A")
            };
            println!(
                "  Cell({},{},{}) [{:.1}..{:.1}]x[{:.1}..{:.1}]x[{:.1}..{:.1}]: {} → {} nodes ({})",
                cell.ix,
                cell.iy,
                cell.iz,
                cell.bounds.x.lo,
                cell.bounds.x.hi,
                cell.bounds.y.lo,
                cell.bounds.y.hi,
                cell.bounds.z.lo,
                cell.bounds.z.hi,
                original_nodes,
                pruned_nodes,
                reduction,
            );
        }
    }

    // ── GLSL 出力 ──
    #[cfg(feature = "glsl")]
    {
        use alice_lol::pruned_compile::to_pruned_glsl;

        let glsl = to_pruned_glsl(&scene, &result);
        let full_glsl = alice_lol::to_glsl(&scene);

        println!("\n--- GLSL サイズ比較 ---");
        println!("  フル GLSL: {} chars", full_glsl.len());
        println!("  枝刈り GLSL (含ディスパッチャ): {} chars", glsl.len());
        println!(
            "  Crossing セル数: {} / {} = 実行パスが限定される",
            result.crossing_count,
            result.cells.len(),
        );
    }

    println!("\n枝刈りデモ完了。");
}
