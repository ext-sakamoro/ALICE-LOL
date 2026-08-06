//! H.3 VRM import example — VRM 0.x file から `HumanoidTemplate` 構築
//!
//! Usage:
//! ```text
//! VRM_PATH=path/to/character.vrm cargo run -p alice-lol-humanoid \
//!     --features vrm --example humanoid_from_vrm
//! ```

use alice_lol::{eval, Vec3};
use alice_lol_humanoid::{HumanoidTemplate, Joint};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Ok(path) = std::env::var("VRM_PATH") else {
        eprintln!("VRM_PATH env var not set");
        eprintln!(
            "Usage: VRM_PATH=path/to/character.vrm cargo run -p alice-lol-humanoid \
             --features vrm --example humanoid_from_vrm"
        );
        return Ok(());
    };

    println!("VRM path: {path}");
    let template = match HumanoidTemplate::from_vrm(&path)? {
        None => {
            println!("VRM に humanoid bones が見つかりません");
            println!("(VRM 1.0 の可能性、または VRM extension 不在)");
            return Ok(());
        }
        Some(t) => t,
    };

    println!("HumanoidTemplate 構築成功");
    println!("  bones:  {}", template.bones.len());
    println!("  joints: {}", template.joints.len());

    println!("\nJoint world positions (VRM bind pose):");
    let sample_joints = [
        Joint::Head,
        Joint::Neck,
        Joint::Chest,
        Joint::Waist,
        Joint::LShoulder,
        Joint::LWrist,
        Joint::LHip,
        Joint::LAnkle,
    ];
    for j in sample_joints {
        if let Some(p) = template.joints.get(&j) {
            println!(
                "  {:<11} [{:>7.3}, {:>7.3}, {:>7.3}]",
                format!("{j:?}"),
                p[0],
                p[1],
                p[2]
            );
        }
    }

    // SDF 生成 + head / waist / ankle での距離 sample
    let sdf = template.to_sdf(0.02);
    let head = *template
        .joints
        .get(&Joint::Head)
        .ok_or("Head joint missing")?;
    let waist = *template
        .joints
        .get(&Joint::Waist)
        .ok_or("Waist joint missing")?;
    let ankle = *template
        .joints
        .get(&Joint::LAnkle)
        .ok_or("LAnkle joint missing")?;

    println!("\nSDF distance sampling (VRM bind pose):");
    println!(
        "  head  at {head:?}: {:.4}",
        eval(&sdf, Vec3::from_array(head))
    );
    println!(
        "  waist at {waist:?}: {:.4}",
        eval(&sdf, Vec3::from_array(waist))
    );
    println!(
        "  ankle at {ankle:?}: {:.4}",
        eval(&sdf, Vec3::from_array(ankle))
    );

    Ok(())
}
