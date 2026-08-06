//! H.1 Static template example — canonical humanoid T-pose `SdfNode` 生成
//!
//! Usage: `cargo run -p alice-lol-humanoid --example humanoid_default`

use alice_lol::{eval, Vec3};
use alice_lol_humanoid::{HumanoidTemplate, MuscleWidths};

fn main() {
    let template = HumanoidTemplate::default();
    println!("HumanoidTemplate::default()");
    println!("  bones:  {}", template.bones.len());
    println!("  joints: {}", template.joints.len());

    let sdf = template.to_sdf(0.15);

    let samples = [
        ("head center", Vec3::new(0.0, 2.5, 0.0)),
        ("waist center", Vec3::new(0.0, 0.0, 0.0)),
        ("foot L", Vec3::new(-0.4, -2.5, 0.0)),
        ("outside +X 5.0", Vec3::new(5.0, 0.0, 0.0)),
    ];

    println!("\nSDF distance sampling (canonical T-pose, Y up):");
    for (label, point) in samples {
        let dist = eval(&sdf, point);
        println!("  {label:<18} at {point:?}: distance = {dist:.4}");
    }

    println!("\nMuscleWidths preset 比較 (head center point):");
    let point = Vec3::new(0.0, 2.5, 0.0);
    for (name, w) in [
        ("chibi", MuscleWidths::chibi()),
        ("shounen", MuscleWidths::shounen()),
        ("slim", MuscleWidths::slim()),
    ] {
        let sdf_p = template.to_sdf_with_widths(0.15, &w);
        let d = eval(&sdf_p, point);
        println!(
            "  {name:<8}: head thickness = {:.2}, distance = {d:.4}",
            w.head
        );
    }
}
