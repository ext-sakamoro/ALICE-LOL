//! ALICE-LOL Autodiff デモ
//! 勾配（法線）、曲率、ヘシアンの計算

use alice_lol::{
    eval_with_gradient, gaussian_curvature, lol, mean_curvature, principal_curvatures, Vec3,
};

fn main() {
    println!("=== ALICE-LOL Autodiff Demo ===\n");

    // ── 球の解析 ──
    println!("--- Sphere(r=2.0) ---");
    let sphere = lol! { sphere(2.0) };
    let surface_point = Vec3::new(2.0, 0.0, 0.0);

    let (dist, grad) = eval_with_gradient(&sphere, surface_point);
    println!("  表面点 (2,0,0):");
    println!("    距離   = {dist:.4} (表面上なので ≈ 0)");
    println!("    勾配   = ({:.4}, {:.4}, {:.4})", grad.x, grad.y, grad.z);
    println!("    |勾配| = {:.4} (SDF勾配の大きさは ≈ 1)", grad.length());

    let mc = mean_curvature(&sphere, surface_point, 0.001);
    let gc = gaussian_curvature(&sphere, surface_point, 0.001);
    let (k1, k2) = principal_curvatures(&sphere, surface_point, 0.001);
    println!("    平均曲率     = {mc:.4} (理論値: 1/r = 0.5)");
    println!("    ガウス曲率   = {gc:.4} (理論値: 1/r² = 0.25)");
    println!("    主曲率       = ({k1:.4}, {k2:.4}) (理論値: 0.5, 0.5)");

    // 内部点
    let inner = Vec3::new(0.5, 0.0, 0.0);
    let (d_in, g_in) = eval_with_gradient(&sphere, inner);
    println!("\n  内部点 (0.5,0,0):");
    println!("    距離 = {d_in:.4} (負 = 内部)");
    println!("    勾配 = ({:.4}, {:.4}, {:.4})", g_in.x, g_in.y, g_in.z);

    // ── トーラスの解析 ──
    println!("\n--- Torus(R=2.0, r=0.5) ---");
    let torus = lol! { torus(2.0, 0.5) };

    // 外側表面点
    let tp = Vec3::new(2.5, 0.0, 0.0);
    let (td, tg) = eval_with_gradient(&torus, tp);
    println!("  外側表面 (2.5,0,0):");
    println!("    距離 = {td:.4}");
    println!("    勾配 = ({:.4}, {:.4}, {:.4})", tg.x, tg.y, tg.z);

    let tmc = mean_curvature(&torus, tp, 0.001);
    println!("    平均曲率 = {tmc:.4} (理論値: (2R+r)/(2r(R+r)) ≈ 0.9)");

    // 内側表面点
    let tip = Vec3::new(1.5, 0.0, 0.0);
    let (tid, tig) = eval_with_gradient(&torus, tip);
    println!("  内側表面 (1.5,0,0):");
    println!("    距離 = {tid:.4}");
    println!("    勾配 = ({:.4}, {:.4}, {:.4})", tig.x, tig.y, tig.z);

    // ── 複合形状の解析 ──
    println!("\n--- Smooth Union (sphere + box) ---");
    let complex = lol! {
        smooth_union(0.3,
            sphere(1.0),
            translate(2.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5))
        )
    };

    // ブレンド領域の勾配
    let blend_point = Vec3::new(1.0, 0.0, 0.0);
    let (cd, cg) = eval_with_gradient(&complex, blend_point);
    println!("  ブレンド領域 (1,0,0):");
    println!("    距離 = {cd:.4}");
    println!("    勾配 = ({:.4}, {:.4}, {:.4})", cg.x, cg.y, cg.z);

    // ── 変数キャプチャ + autodiff ──
    println!("\n--- 変数キャプチャ + Autodiff ---");
    let r = 3.0_f32;
    let node = lol! { sphere({r}) };
    let (d, g) = eval_with_gradient(&node, Vec3::new(r, 0.0, 0.0));
    println!("  sphere({{r={r}}}) at ({r},0,0):");
    println!(
        "    距離 = {d:.4}, 勾配 = ({:.4}, {:.4}, {:.4})",
        g.x, g.y, g.z
    );

    println!("\n=== Autodiff Demo Complete ===");
}
