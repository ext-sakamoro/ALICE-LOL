//! 3Dプリント構造検証
//!
//! lattice_infill が正しく内部構造を生成しているか、
//! SDF距離場とメッシュ断面を数値的に検証する。
//!
//! ```bash
//! cargo run --example print_verify
//! ```

use alice_lol::{eval, lol};
use glam::Vec3;

fn main() {
    println!("=== ALICE-LOL 3D Print Structure Verification ===\n");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 検証1: SDF距離場のレイキャスト断面
    // X軸に沿ってサンプリングし、inside/outside 遷移を確認
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- Test 1: Radial SDF sampling (lattice_infill sphere) ---");
    let infill_sphere = lol! { lattice_infill(0.05, 5.0, 0.02, sphere(1.0)) };
    let solid_sphere = lol! { sphere(1.0) };
    let hollow_sphere = lol! { onion(0.05, sphere(1.0)) };

    println!("  r     | Solid    | Hollow   | Gyroid Infill | Infill状態");
    println!("  ------|----------|----------|---------------|----------");
    let mut infill_transitions = 0;
    let mut prev_inside = false;
    for i in 0..=50 {
        let r = i as f32 * 0.02; // 0.0 → 1.0
        let p = Vec3::new(r, 0.0, 0.0);
        let d_solid = eval(&solid_sphere, p);
        let d_hollow = eval(&hollow_sphere, p);
        let d_infill = eval(&infill_sphere, p);
        let inside = d_infill < 0.0;
        if i > 0 && inside != prev_inside {
            infill_transitions += 1;
        }
        prev_inside = inside;
        if i % 5 == 0 {
            let state = if inside { "INSIDE" } else { "outside" };
            println!(
                "  {r:.2}  | {d_solid:+.4}  | {d_hollow:+.4}  | {d_infill:+.4}       | {state}"
            );
        }
    }
    println!("\n  inside/outside遷移回数: {infill_transitions}");
    assert!(
        infill_transitions >= 3,
        "ジャイロイド格子なら3回以上のin/out遷移が必要（実際: {infill_transitions}）"
    );
    println!("  -> PASS: ジャイロイド格子構造を確認（{infill_transitions}回遷移）\n");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 検証2: Y=0断面のASCIIビジュアル
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- Test 2: Y=0 Cross-section (40x40 grid) ---\n");
    println!("  Legend: # = shell, * = lattice, . = void, SPACE = outside\n");

    let shell_node = lol! { onion(0.05, sphere(1.0)) };
    let res = 40;
    for iz in 0..res {
        let z = -1.2 + (iz as f32 / res as f32) * 2.4;
        let mut line = String::with_capacity(res);
        for ix in 0..res {
            let x = -1.2 + (ix as f32 / res as f32) * 2.4;
            let p = Vec3::new(x, 0.0, z);
            let d_infill = eval(&infill_sphere, p);
            let d_shell = eval(&shell_node, p);
            let d_solid = eval(&solid_sphere, p);

            if d_solid > 0.0 {
                // 外部
                line.push(' ');
            } else if d_shell < 0.0 {
                // シェル内
                line.push('#');
            } else if d_infill < 0.0 {
                // ラティス内
                line.push('*');
            } else {
                // 内部の空洞
                line.push('.');
            }
        }
        println!("  {line}");
    }
    println!();

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 検証3: 3種のインフィルが異なるTPMSパターンを生成するか
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- Test 3: TPMS pattern differentiation ---");
    let gyroid = lol! { lattice_infill(0.05, 5.0, 0.02, sphere(1.0)) };
    let diamond = lol! { diamond_infill(0.05, 5.0, 0.02, sphere(1.0)) };
    let schwarz = lol! { schwarz_infill(0.05, 5.0, 0.02, sphere(1.0)) };

    // 内部100点をサンプリングして、各TPMSのパターンが異なることを確認
    let mut g_vals = Vec::new();
    let mut d_vals = Vec::new();
    let mut s_vals = Vec::new();
    for i in 0..100 {
        let t = i as f32 * 0.015;
        let p = Vec3::new(t * 0.7, t * 0.5, t * 0.3); // 斜め方向にサンプリング
        g_vals.push(eval(&gyroid, p));
        d_vals.push(eval(&diamond, p));
        s_vals.push(eval(&schwarz, p));
    }

    // 相関が低い = 異なるパターン
    let corr_gd = correlation(&g_vals, &d_vals);
    let corr_gs = correlation(&g_vals, &s_vals);
    let corr_ds = correlation(&d_vals, &s_vals);
    println!("  Gyroid vs Diamond  correlation: {corr_gd:.3}");
    println!("  Gyroid vs Schwarz  correlation: {corr_gs:.3}");
    println!("  Diamond vs Schwarz correlation: {corr_ds:.3}");
    assert!(
        corr_gd.abs() < 0.99 && corr_gs.abs() < 0.99,
        "3種のTPMSは異なるパターンを生成すべき"
    );
    println!("  -> PASS: 3種のTPMSは異なるパターンを確認\n");

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 検証4: シェル厚の妥当性
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- Test 4: Shell thickness validation ---");
    // 表面近傍（r ≈ 1.0）でシェルが存在するか
    let shell_samples = [0.96_f32, 0.97, 0.98, 0.99, 1.00, 1.01, 1.02, 1.03, 1.04];
    let mut shell_inside_count = 0;
    for &r in &shell_samples {
        let d = eval(&infill_sphere, Vec3::new(r, 0.0, 0.0));
        let inside = d < 0.0;
        if inside {
            shell_inside_count += 1;
        }
        println!(
            "  r={r:.2}: d={d:+.4} [{:}]",
            if inside { "INSIDE" } else { "outside" }
        );
    }
    assert!(shell_inside_count >= 2, "シェル領域に最低2点はINSIDEが必要");
    println!(
        "  -> PASS: シェル構造を確認 ({shell_inside_count}/{} inside)\n",
        shell_samples.len()
    );

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 検証5: 体積比較（ソリッド vs インフィル）
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- Test 5: Volume comparison (Monte Carlo) ---");
    let n_samples = 50000;
    let solid_vol = monte_carlo_volume(&solid_sphere, n_samples);
    let hollow_vol = monte_carlo_volume(&hollow_sphere, n_samples);
    let infill_vol = monte_carlo_volume(&infill_sphere, n_samples);

    let solid_pct = 100.0;
    let hollow_pct = hollow_vol / solid_vol * 100.0;
    let infill_pct = infill_vol / solid_vol * 100.0;

    println!("  Solid sphere:  vol ≈ {solid_vol:.3}  ({solid_pct:.1}%)");
    println!("  Hollow (0.05): vol ≈ {hollow_vol:.3}  ({hollow_pct:.1}%)");
    println!("  Gyroid infill: vol ≈ {infill_vol:.3}  ({infill_pct:.1}%)");

    assert!(
        infill_vol > hollow_vol,
        "インフィルは中空より体積が大きいはず"
    );
    assert!(
        infill_vol < solid_vol,
        "インフィルはソリッドより体積が小さいはず"
    );
    println!("  -> PASS: hollow < infill < solid の関係を確認");
    println!("  -> フィラメント節約率: {:.1}%\n", 100.0 - infill_pct);

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // 検証6: ランタイムパーサー経由の等価性
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    println!("--- Test 6: Runtime parser equivalence ---");
    let macro_node = lol! { lattice_infill(0.05, 5.0, 0.02, sphere(1.0)) };
    let runtime_node =
        alice_lol::runtime_parser::parse_lol("lattice_infill(0.05, 5.0, 0.02, sphere(1.0))")
            .unwrap();

    let mut max_diff: f32 = 0.0;
    for i in 0..200 {
        let t = i as f32 * 0.01 - 1.0;
        let p = Vec3::new(t, t * 0.3, t * 0.7);
        let d_macro = eval(&macro_node, p);
        let d_runtime = eval(&runtime_node, p);
        max_diff = max_diff.max((d_macro - d_runtime).abs());
    }
    println!("  proc_macro vs runtime_parser max diff: {max_diff:.6}");
    assert!(
        max_diff < 1e-5,
        "マクロとランタイムパーサーの出力は一致すべき"
    );
    println!("  -> PASS: 完全一致\n");

    println!("=== All 6 verifications PASSED ===");
}

fn monte_carlo_volume(node: &alice_lol::SdfNode, n: usize) -> f32 {
    let bounds = 1.3_f32; // sphere r=1.0 に余裕
    let box_vol = (2.0 * bounds).powi(3);
    let mut inside = 0u32;
    // 疑似ランダム（Halton sequence 風）
    for i in 0..n {
        let x = halton(i, 2) * 2.0 * bounds - bounds;
        let y = halton(i, 3) * 2.0 * bounds - bounds;
        let z = halton(i, 5) * 2.0 * bounds - bounds;
        if eval(node, Vec3::new(x, y, z)) < 0.0 {
            inside += 1;
        }
    }
    box_vol * inside as f32 / n as f32
}

fn halton(mut index: usize, base: usize) -> f32 {
    let mut f = 1.0_f32;
    let mut r = 0.0_f32;
    let b = base as f32;
    index += 1; // 0-indexed → 1-indexed
    while index > 0 {
        f /= b;
        r += f * (index % base) as f32;
        index /= base;
    }
    r
}

fn correlation(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len() as f32;
    let mean_a: f32 = a.iter().sum::<f32>() / n;
    let mean_b: f32 = b.iter().sum::<f32>() / n;
    let mut cov = 0.0_f32;
    let mut var_a = 0.0_f32;
    let mut var_b = 0.0_f32;
    for (ai, bi) in a.iter().zip(b.iter()) {
        let da = ai - mean_a;
        let db = bi - mean_b;
        cov += da * db;
        var_a += da * da;
        var_b += db * db;
    }
    let denom = (var_a * var_b).sqrt();
    if denom < 1e-10 {
        0.0
    } else {
        cov / denom
    }
}
