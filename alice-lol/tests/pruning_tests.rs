//! 空間枝刈りコンパイラのテスト

use alice_lol::lol;
use alice_lol::pruned_compile::{analyze_grid, count_nodes, CellKind, PruneConfig};
use glam::Vec3;

/// 単一 sphere — 外側セルが存在する
#[test]
fn sphere_has_outside_cells() {
    let scene = lol! { sphere(1.0) };
    let config = PruneConfig {
        aabb_min: Vec3::splat(-3.0),
        aabb_max: Vec3::splat(3.0),
        grid_resolution: 4,
    };
    let result = analyze_grid(&scene, &config);
    assert!(result.outside_count > 0, "sphere は角セルが outside のはず");
    assert!(
        result.crossing_count > 0,
        "sphere 表面を横切るセルがあるはず"
    );
}

/// 原点の sphere(1) — 原点を含むセルは Inside か Crossing
#[test]
fn sphere_origin_cell_not_outside() {
    let scene = lol! { sphere(1.0) };
    let config = PruneConfig {
        aabb_min: Vec3::splat(-2.0),
        aabb_max: Vec3::splat(2.0),
        grid_resolution: 4,
    };
    let result = analyze_grid(&scene, &config);
    // 原点は ix=1,iy=1,iz=1 または ix=2,iy=2,iz=2 のセルに含まれる
    let origin_cells: Vec<_> = result
        .cells
        .iter()
        .filter(|c| {
            c.bounds.x.lo <= 0.0
                && c.bounds.x.hi >= 0.0
                && c.bounds.y.lo <= 0.0
                && c.bounds.y.hi >= 0.0
                && c.bounds.z.lo <= 0.0
                && c.bounds.z.hi >= 0.0
        })
        .collect();
    assert!(!origin_cells.is_empty());
    for c in origin_cells {
        assert_ne!(
            c.kind,
            CellKind::Outside,
            "原点を含むセルは Outside ではない"
        );
    }
}

/// セル数 = N^3
#[test]
fn cell_count_matches_resolution() {
    let scene = lol! { sphere(1.0) };
    for n in [2, 3, 4, 8] {
        let config = PruneConfig {
            aabb_min: Vec3::splat(-2.0),
            aabb_max: Vec3::splat(2.0),
            grid_resolution: n,
        };
        let result = analyze_grid(&scene, &config);
        assert_eq!(result.cells.len(), n * n * n);
        assert_eq!(
            result.outside_count + result.inside_count + result.crossing_count,
            n * n * n,
        );
    }
}

/// 離れた 2 body の smooth_union — 枝刈りでノード数が減る
#[test]
fn pruning_reduces_node_count() {
    let scene = lol! {
        smooth_union(0.3,
            sphere(1.0),
            translate(8.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5))
        )
    };
    let original_count = count_nodes(&scene);

    let config = PruneConfig {
        aabb_min: Vec3::splat(-3.0),
        aabb_max: Vec3::new(11.0, 3.0, 3.0),
        grid_resolution: 4,
    };
    let result = analyze_grid(&scene, &config);

    // Crossing セルのうち、枝刈りでノード数が減ったものがあるはず
    let mut found_smaller = false;
    for cell in &result.cells {
        if let Some(ref pruned) = cell.pruned_node {
            let pruned_count = count_nodes(pruned);
            if pruned_count < original_count {
                found_smaller = true;
                break;
            }
        }
    }
    assert!(
        found_smaller,
        "離れた 2 body なので少なくとも一部のセルで枝刈りが効くはず"
    );
}

/// count_nodes: プリミティブ = 1
#[test]
fn count_nodes_primitive() {
    let n = lol! { sphere(1.0) };
    assert_eq!(count_nodes(&n), 1);
}

/// count_nodes: union(a, b) = 3
#[test]
fn count_nodes_union() {
    let n = lol! { union(sphere(1.0), box3d(0.5, 0.5, 0.5)) };
    assert_eq!(count_nodes(&n), 3);
}

/// count_nodes: translate + sphere = 2
#[test]
fn count_nodes_translate() {
    let n = lol! { translate(1.0, 0.0, 0.0, sphere(1.0)) };
    assert_eq!(count_nodes(&n), 2);
}

/// 枝刈り結果の正しさ: 枝刈り後も原点での SDF 値が一致
#[test]
fn pruned_eval_matches_original() {
    let scene = lol! {
        smooth_union(0.3,
            sphere(1.0),
            translate(8.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5))
        )
    };
    let original_val = alice_lol::eval(&scene, Vec3::ZERO);

    let config = PruneConfig {
        aabb_min: Vec3::splat(-3.0),
        aabb_max: Vec3::new(11.0, 3.0, 3.0),
        grid_resolution: 4,
    };
    let result = analyze_grid(&scene, &config);

    // 原点を含むセルの pruned_node で eval
    for cell in &result.cells {
        if cell.bounds.x.lo <= 0.0
            && cell.bounds.x.hi >= 0.0
            && cell.bounds.y.lo <= 0.0
            && cell.bounds.y.hi >= 0.0
            && cell.bounds.z.lo <= 0.0
            && cell.bounds.z.hi >= 0.0
        {
            if let Some(ref pruned) = cell.pruned_node {
                let pruned_val = alice_lol::eval(pruned, Vec3::ZERO);
                let diff = (original_val - pruned_val).abs();
                assert!(
                    diff < 1e-6,
                    "枝刈り後の eval 値が一致しない: original={original_val}, pruned={pruned_val}"
                );
            }
        }
    }
}

/// GLSL 出力にディスパッチャが含まれる
#[cfg(feature = "glsl")]
#[test]
fn pruned_glsl_has_dispatcher() {
    use alice_lol::pruned_compile::to_pruned_glsl;

    let scene = lol! {
        smooth_union(0.3,
            sphere(1.0),
            translate(5.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5))
        )
    };
    let config = PruneConfig {
        aabb_min: Vec3::splat(-3.0),
        aabb_max: Vec3::new(8.0, 3.0, 3.0),
        grid_resolution: 3,
    };
    let result = analyze_grid(&scene, &config);
    let glsl = to_pruned_glsl(&scene, &result);

    assert!(glsl.contains("sdf_pruned"), "ディスパッチャ関数が含まれる");
    assert!(glsl.contains("switch(cell)"), "switch-case が含まれる");
    assert!(glsl.contains("sdf_cell_"), "セル関数が含まれる");
}

/// 完全に外側の小さな AABB — 全セル Outside
#[test]
fn entirely_outside_aabb() {
    let scene = lol! { sphere(1.0) };
    let config = PruneConfig {
        aabb_min: Vec3::splat(10.0),
        aabb_max: Vec3::splat(12.0),
        grid_resolution: 2,
    };
    let result = analyze_grid(&scene, &config);
    assert_eq!(result.outside_count, 8);
    assert_eq!(result.crossing_count, 0);
    assert_eq!(result.inside_count, 0);
}

/// Default config の確認
#[test]
fn default_config() {
    let config = PruneConfig::default();
    assert_eq!(config.grid_resolution, 4);
    assert_eq!(config.aabb_min, Vec3::splat(-5.0));
    assert_eq!(config.aabb_max, Vec3::splat(5.0));
}
