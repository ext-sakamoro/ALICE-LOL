//! ALICE-LOL v0.5 全構文ショーケース
//! 76構文 + 変数キャプチャ + autodiff + CompiledSdf

use alice_lol::{eval, lol, Vec3};

fn eval_at(label: &str, node: &alice_lol::SdfNode, point: Vec3) {
    let d = eval(node, point);
    println!("  {label}: d={d:.4}");
}

fn section(title: &str) {
    println!("\n{}", "=".repeat(60));
    println!("  {title}");
    println!("{}", "=".repeat(60));
}

fn main() {
    let o = Vec3::ZERO;
    let p1 = Vec3::new(1.0, 0.0, 0.0);
    let p2 = Vec3::new(0.0, 1.0, 0.0);

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    section("PRIMITIVES (27)");
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    println!("\n-- sphere(1.0) --");
    eval_at("origin", &lol! { sphere(1.0) }, o);

    println!("\n-- box3d(0.5, 0.5, 0.5) --");
    eval_at("origin", &lol! { box3d(0.5, 0.5, 0.5) }, o);

    println!("\n-- rounded_box(0.5, 0.5, 0.5, 0.1) --");
    eval_at("origin", &lol! { rounded_box(0.5, 0.5, 0.5, 0.1) }, o);

    println!("\n-- cylinder(0.5, 1.0) --");
    eval_at("origin", &lol! { cylinder(0.5, 1.0) }, o);

    println!("\n-- torus(1.0, 0.3) --");
    eval_at("origin", &lol! { torus(1.0, 0.3) }, o);

    println!("\n-- cone(0.5, 1.0) --");
    eval_at("origin", &lol! { cone(0.5, 1.0) }, o);

    println!("\n-- capsule(0.3, 1.0) --");
    eval_at("origin", &lol! { capsule(0.3, 1.0) }, o);

    println!("\n-- ellipsoid(1.0, 0.5, 0.8) --");
    eval_at("origin", &lol! { ellipsoid(1.0, 0.5, 0.8) }, o);

    println!("\n-- plane(0, 1, 0, 0) --");
    eval_at("origin", &lol! { plane(0.0, 1.0, 0.0, 0.0) }, o);
    eval_at("(0,1,0)", &lol! { plane(0.0, 1.0, 0.0, 0.0) }, p2);

    println!("\n-- octahedron(1.0) --");
    eval_at("origin", &lol! { octahedron(1.0) }, o);

    // v0.4 プリミティブ
    println!("\n-- rounded_cone(0.5, 0.2, 1.0) --");
    eval_at("origin", &lol! { rounded_cone(0.5, 0.2, 1.0) }, o);

    println!("\n-- pyramid(1.0) --");
    eval_at("origin", &lol! { pyramid(1.0) }, o);

    println!("\n-- hex_prism(0.5, 1.0) --");
    eval_at("origin", &lol! { hex_prism(0.5, 1.0) }, o);

    println!("\n-- link(0.5, 0.3, 0.1) --");
    eval_at("origin", &lol! { link(0.5, 0.3, 0.1) }, o);

    println!("\n-- capped_cone(1.0, 0.5, 0.2) --");
    eval_at("origin", &lol! { capped_cone(1.0, 0.5, 0.2) }, o);

    println!("\n-- capped_torus(1.0, 0.3, 1.57) --");
    eval_at("origin", &lol! { capped_torus(1.0, 0.3, 1.57) }, o);

    println!("\n-- rounded_cylinder(0.5, 0.1, 1.0) --");
    eval_at("origin", &lol! { rounded_cylinder(0.5, 0.1, 1.0) }, o);

    println!("\n-- tube(0.5, 0.1, 1.0) --");
    eval_at("origin", &lol! { tube(0.5, 0.1, 1.0) }, o);

    println!("\n-- barrel(0.5, 1.0, 0.2) --");
    eval_at("origin", &lol! { barrel(0.5, 1.0, 0.2) }, o);

    println!("\n-- heart(1.0) --");
    eval_at("origin", &lol! { heart(1.0) }, o);

    println!("\n-- egg(0.5, 0.3) --");
    eval_at("origin", &lol! { egg(0.5, 0.3) }, o);

    println!("\n-- helix(1.0, 0.1, 0.5, 2.0) --");
    eval_at("origin", &lol! { helix(1.0, 0.1, 0.5, 2.0) }, o);

    println!("\n-- tetrahedron(1.0) --");
    eval_at("origin", &lol! { tetrahedron(1.0) }, o);

    println!("\n-- box_frame(0.5, 0.5, 0.5, 0.05) --");
    eval_at("origin", &lol! { box_frame(0.5, 0.5, 0.5, 0.05) }, o);

    println!("\n-- diamond(0.5, 1.0) --");
    eval_at("origin", &lol! { diamond(0.5, 1.0) }, o);

    println!("\n-- star_polygon(1.0, 5, 2, 0.3) --");
    eval_at("origin", &lol! { star_polygon(1.0, 5.0, 2.0, 0.3) }, o);

    println!("\n-- cross_shape(1.0, 0.3, 0.05, 0.5) --");
    eval_at("origin", &lol! { cross_shape(1.0, 0.3, 0.05, 0.5) }, o);

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    section("OPERATIONS (23)");
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    println!("\n-- union --");
    eval_at(
        "origin",
        &lol! { union(sphere(1.0), box3d(0.5, 0.5, 0.5)) },
        o,
    );

    println!("\n-- smooth_union(0.2, 3-way) --");
    eval_at(
        "origin",
        &lol! { smooth_union(0.2, sphere(0.5), translate(1.5, 0.0, 0.0, box3d(0.4, 0.4, 0.4)), translate(0.0, 1.5, 0.0, torus(0.5, 0.15))) },
        o,
    );

    println!("\n-- intersection --");
    eval_at(
        "origin",
        &lol! { intersection(sphere(1.0), box3d(0.5, 0.5, 0.5)) },
        o,
    );

    println!("\n-- smooth_intersection --");
    eval_at(
        "origin",
        &lol! { smooth_intersection(0.1, sphere(1.0), box3d(0.5, 0.5, 0.5)) },
        o,
    );

    println!("\n-- subtract --");
    eval_at(
        "origin",
        &lol! { subtract(sphere(1.0), box3d(0.6, 0.6, 0.6)) },
        o,
    );

    println!("\n-- smooth_subtract --");
    eval_at(
        "origin",
        &lol! { smooth_subtract(0.1, sphere(1.0), box3d(0.6, 0.6, 0.6)) },
        o,
    );

    println!("\n-- chamfer_union --");
    eval_at(
        "origin",
        &lol! { chamfer_union(0.2, sphere(1.0), translate(1.5, 0.0, 0.0, sphere(1.0))) },
        o,
    );

    println!("\n-- chamfer_intersection --");
    eval_at(
        "origin",
        &lol! { chamfer_intersection(0.2, sphere(1.0), box3d(0.8, 0.8, 0.8)) },
        o,
    );

    println!("\n-- chamfer_subtraction --");
    eval_at(
        "origin",
        &lol! { chamfer_subtraction(0.2, sphere(1.0), box3d(0.6, 0.6, 0.6)) },
        o,
    );

    println!("\n-- stairs_union --");
    eval_at(
        "origin",
        &lol! { stairs_union(0.2, 4.0, sphere(1.0), translate(1.5, 0.0, 0.0, sphere(1.0))) },
        o,
    );

    println!("\n-- xor --");
    eval_at(
        "origin",
        &lol! { xor(sphere(1.0), box3d(0.8, 0.8, 0.8)) },
        o,
    );

    println!("\n-- pipe --");
    eval_at(
        "origin",
        &lol! { pipe(0.1, sphere(1.0), box3d(0.8, 0.8, 0.8)) },
        o,
    );

    println!("\n-- engrave --");
    eval_at(
        "origin",
        &lol! { engrave(0.1, sphere(1.0), box3d(0.8, 0.8, 0.8)) },
        o,
    );

    println!("\n-- groove --");
    eval_at(
        "origin",
        &lol! { groove(0.2, 0.1, sphere(1.0), box3d(0.8, 0.8, 0.8)) },
        o,
    );

    println!("\n-- tongue --");
    eval_at(
        "origin",
        &lol! { tongue(0.2, 0.1, sphere(1.0), box3d(0.8, 0.8, 0.8)) },
        o,
    );

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    section("TRANSFORMS (4)");
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    println!("\n-- translate --");
    eval_at(
        "(3,0,0)",
        &lol! { translate(3.0, 0.0, 0.0, sphere(1.0)) },
        Vec3::new(3.0, 0.0, 0.0),
    );

    println!("\n-- rotate --");
    eval_at(
        "origin",
        &lol! { rotate(0.0, 0.0, 45.0, box3d(1.0, 0.2, 0.2)) },
        o,
    );

    println!("\n-- scale --");
    eval_at("origin", &lol! { scale(2.0, sphere(1.0)) }, o);

    println!("\n-- scale_non_uniform --");
    eval_at(
        "origin",
        &lol! { scale_non_uniform(2.0, 1.0, 0.5, sphere(1.0)) },
        o,
    );

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    section("MODIFIERS (19)");
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    println!("\n-- round --");
    eval_at("origin", &lol! { round(0.1, box3d(0.5, 0.5, 0.5)) }, o);

    println!("\n-- onion --");
    eval_at("origin", &lol! { onion(0.1, sphere(1.0)) }, o);

    println!("\n-- twist --");
    eval_at("origin", &lol! { twist(2.0, box3d(0.5, 0.5, 1.5)) }, o);

    println!("\n-- bend --");
    eval_at("origin", &lol! { bend(1.0, box3d(0.3, 0.3, 2.0)) }, o);

    println!("\n-- mirror --");
    eval_at(
        "(2,0,0)",
        &lol! { mirror(1.0, 0.0, 0.0, translate(2.0, 0.0, 0.0, sphere(0.5))) },
        Vec3::new(2.0, 0.0, 0.0),
    );

    println!("\n-- repeat --");
    eval_at("origin", &lol! { repeat(4.0, 4.0, 4.0, sphere(0.5)) }, o);

    println!("\n-- elongate --");
    eval_at("origin", &lol! { elongate(0.5, 0.0, 0.0, sphere(1.0)) }, o);

    println!("\n-- revolution --");
    eval_at("(1,0,0)", &lol! { revolution(0.5, sphere(0.3)) }, p1);

    println!("\n-- extrude --");
    eval_at("origin", &lol! { extrude(1.0, sphere(0.5)) }, o);

    println!("\n-- taper --");
    eval_at("origin", &lol! { taper(0.5, cylinder(0.5, 1.0)) }, o);

    println!("\n-- displacement --");
    eval_at("origin", &lol! { displacement(0.1, sphere(1.0)) }, o);

    println!("\n-- polar_repeat --");
    eval_at(
        "(2,0,0)",
        &lol! { polar_repeat(6.0, translate(2.0, 0.0, 0.0, sphere(0.3))) },
        Vec3::new(2.0, 0.0, 0.0),
    );

    println!("\n-- shear --");
    eval_at(
        "origin",
        &lol! { shear(0.5, 0.0, 0.0, box3d(0.5, 0.5, 0.5)) },
        o,
    );

    println!("\n-- noise --");
    eval_at("origin", &lol! { noise(0.1, 2.0, 42.0, sphere(1.0)) }, o);

    println!("\n-- repeat_finite --");
    eval_at(
        "origin",
        &lol! { repeat_finite(3.0, 1.0, 1.0, 2.0, 2.0, 2.0, sphere(0.3)) },
        o,
    );

    println!("\n-- octant_mirror --");
    eval_at(
        "(1,1,1)",
        &lol! { octant_mirror(translate(1.0, 1.0, 1.0, sphere(0.3))) },
        Vec3::new(1.0, 1.0, 1.0),
    );

    println!("\n-- icosahedral_symmetry --");
    eval_at(
        "(1,0,0)",
        &lol! { icosahedral_symmetry(translate(2.0, 0.0, 0.0, sphere(0.2))) },
        p1,
    );

    println!("\n-- with_material --");
    eval_at("origin", &lol! { with_material(1.0, sphere(1.0)) }, o);

    println!("\n-- surface_roughness --");
    eval_at(
        "origin",
        &lol! { surface_roughness(5.0, 0.05, 3.0, sphere(1.0)) },
        o,
    );

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    section("TIME (2)");
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    println!("\n-- animate --");
    eval_at("origin", &lol! { animate(2.0, 0.5, sphere(1.0)) }, o);

    println!("\n-- morph(0.5) --");
    eval_at(
        "origin",
        &lol! { morph(0.5, sphere(1.0), box3d(0.5, 0.5, 0.5)) },
        o,
    );

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    section("VARIABLE CAPTURE (v0.5)");
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    println!("\n-- {{expr}} capture --");
    let radius = 1.5_f32;
    let node = lol! { sphere({radius}) };
    eval_at("sphere({{radius}}=1.5)", &node, o);

    println!("\n-- bare variable --");
    let r = 2.0_f32;
    let node = lol! { sphere(r) };
    eval_at("sphere(r=2.0)", &node, o);

    println!("\n-- {{expr}} arithmetic --");
    let base = 1.0_f32;
    let node = lol! { sphere({base * 3.0}) };
    eval_at("sphere({{base*3.0}}=3.0)", &node, o);

    println!("\n-- parametric scene --");
    let arm_len = 1.5_f32;
    let arm_r = 0.2_f32;
    let joint_r = 0.35_f32;
    let node = lol! {
        smooth_union(0.15,
            sphere({joint_r}),
            translate({arm_len}, 0.0, 0.0,
                smooth_union(0.15,
                    capsule({arm_r}, {arm_len * 0.5}),
                    sphere({joint_r})
                )
            )
        )
    };
    eval_at("parametric arm", &node, o);
    eval_at("at joint", &node, Vec3::new(arm_len, 0.0, 0.0));

    println!("\n-- function call in capture --");
    fn make_radius(scale: f32) -> f32 {
        scale * 0.75
    }
    let node = lol! { sphere({make_radius(2.0)}) };
    eval_at("sphere(fn(2.0)=1.5)", &node, o);

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    section("AUTODIFF (gradient & curvature)");
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    let sphere_node = lol! { sphere(2.0) };
    let p = Vec3::new(2.0, 0.0, 0.0);

    let (dist, grad) = alice_lol::eval_with_gradient(&sphere_node, p);
    println!("\n  sphere(2.0) at (2,0,0):");
    println!("    distance = {dist:.4}");
    println!(
        "    gradient = ({:.4}, {:.4}, {:.4})",
        grad.x, grad.y, grad.z
    );
    println!("    (gradient = surface normal at the surface)");

    let mc = alice_lol::mean_curvature(&sphere_node, p, 0.001);
    println!("    mean curvature = {mc:.4} (expected 1/r = 0.5)");

    let gc = alice_lol::gaussian_curvature(&sphere_node, p, 0.001);
    println!("    gaussian curvature = {gc:.4} (expected 1/r^2 = 0.25)");

    let (k1, k2) = alice_lol::principal_curvatures(&sphere_node, p, 0.001);
    println!("    principal curvatures = ({k1:.4}, {k2:.4})");

    let torus_node = lol! { torus(2.0, 0.5) };
    let tp = Vec3::new(2.5, 0.0, 0.0);
    let (td, tg) = alice_lol::eval_with_gradient(&torus_node, tp);
    println!("\n  torus(2.0, 0.5) at (2.5,0,0):");
    println!("    distance = {td:.4}");
    println!("    gradient = ({:.4}, {:.4}, {:.4})", tg.x, tg.y, tg.z);

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    section("COMPILED SDF (high-performance eval)");
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    let scene = lol! {
        smooth_union(0.3,
            sphere(1.0),
            translate(2.5, 0.0, 0.0,
                smooth_intersection(0.2,
                    torus(1.0, 0.4),
                    box3d(0.8, 0.8, 0.8)
                )
            )
        )
    };

    let compiled = alice_lol::CompiledSdf::compile(&scene);
    let d_interp = alice_lol::eval_compiled(&compiled, o);
    let d_direct = eval(&scene, o);
    println!("\n  CompiledSdf vs direct eval at origin:");
    println!("    compiled = {d_interp:.6}");
    println!("    direct   = {d_direct:.6}");
    println!("    diff     = {:.2e}", (d_interp - d_direct).abs());

    // バッチ SIMD 評価
    let points: Vec<Vec3> = (0..8)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let x = i as f32 * 0.5;
            Vec3::new(x, 0.0, 0.0)
        })
        .collect();
    let results = alice_lol::eval_compiled_batch_simd(&compiled, &points);
    println!("\n  Batch SIMD evaluation along X axis:");
    for (p, d) in points.iter().zip(results.iter()) {
        println!("    x={:.1}: d={d:.4}", p.x);
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    section("GLSL OUTPUT");
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    let glsl = alice_lol::to_glsl(&scene);
    println!("\n  GLSL output: {} chars", glsl.len());

    println!("\n  LOL v0.5 showcase complete.");
}
