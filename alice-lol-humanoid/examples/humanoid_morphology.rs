//! H.2 Parametric morphology example — chibi / adult / hero / canonical 4 型比較
//!
//! Usage: `cargo run -p alice-lol-humanoid --example humanoid_morphology`

use alice_lol::{eval, Vec3};
use alice_lol_humanoid::{HumanoidTemplate, Joint, MorphologyParams};

fn main() {
    let presets: [(&str, MorphologyParams); 4] = [
        ("chibi (3-headed)", MorphologyParams::chibi()),
        ("adult (7.5-headed)", MorphologyParams::adult()),
        ("hero (8-headed)", MorphologyParams::hero()),
        ("canonical default (10-headed)", MorphologyParams::default()),
    ];

    for (name, params) in presets {
        println!("=== {name} ===");
        println!(
            "  height={:.2}  head_body_ratio={:.1}  arm_ratio={:.2}  leg_ratio={:.2}",
            params.height, params.head_body_ratio, params.arm_ratio, params.leg_ratio
        );
        println!(
            "  shoulder_ratio={:.2}  hip_ratio={:.2}",
            params.shoulder_ratio, params.hip_ratio
        );

        let t = HumanoidTemplate::from_morphology(&params);

        // 主要 joint 座標抜粋
        let sample_joints = [
            Joint::Head,
            Joint::Neck,
            Joint::Chest,
            Joint::Waist,
            Joint::LWrist,
            Joint::LHip,
            Joint::LAnkle,
        ];
        println!("  joints (抜粋):");
        for j in sample_joints {
            let p = t.joints[&j];
            println!(
                "    {:<11} [{:>6.2}, {:>6.2}, {:>6.2}]",
                format!("{j:?}"),
                p[0],
                p[1],
                p[2]
            );
        }

        // 頭頂 / 腰 / 足首 での SDF 距離 sample (shounen widths)
        let sdf = t.to_sdf(0.15);
        let head_top = *t.joints.get(&Joint::Head).expect("Head missing");
        let waist = *t.joints.get(&Joint::Waist).expect("Waist missing");
        let ankle = *t.joints.get(&Joint::LAnkle).expect("LAnkle missing");
        let d_head = eval(&sdf, Vec3::from_array(head_top));
        let d_waist = eval(&sdf, Vec3::from_array(waist));
        let d_ankle = eval(&sdf, Vec3::from_array(ankle));
        println!("  SDF sample (shounen widths):");
        println!("    head  distance = {d_head:>7.4}");
        println!("    waist distance = {d_waist:>7.4}");
        println!("    ankle distance = {d_ankle:>7.4}");

        // 身長 vs 頭高の実測
        let head_span = params.height / params.head_body_ratio;
        println!(
            "  実測: height={:.2}, head_span={:.2}, 頭身={:.1}",
            params.height,
            head_span,
            params.height / head_span
        );
        println!();
    }

    // builder chain demo
    println!("=== builder chain demo (real-world 1.7m / 7.5-headed) ===");
    let real = HumanoidTemplate::builder()
        .height(1.7)
        .head_body_ratio(7.5)
        .arm_ratio(0.38)
        .shoulder_ratio(0.23)
        .hip_ratio(0.16)
        .leg_ratio(0.52)
        .build();
    println!(
        "  head y = {:.3} m, waist y = {:.3} m, ankle y = {:.3} m",
        real.joints[&Joint::Head][1],
        real.joints[&Joint::Waist][1],
        real.joints[&Joint::LAnkle][1]
    );
}
