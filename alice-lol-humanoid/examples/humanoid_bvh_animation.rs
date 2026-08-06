//! H.4 BVH import + pose example — BVH の複数 frame を bind pose に適用
//!
//! Usage:
//! ```text
//! BVH_PATH=path/to/motion.bvh cargo run -p alice-lol-humanoid \
//!     --example humanoid_bvh_animation
//! ```
//!
//! Optional `VRM_PATH`: 設定すれば VRM bind pose を使用 (feature `vrm` 必要)、
//! 未設定なら [`HumanoidTemplate::default`] を bind pose に使う
//!
//! BVH の bone naming は Mixamo と CMU を試行、どちらかがヒットした mapping を採用

use alice_lol_humanoid::{
    bvh::{cmu_to_vrm, mixamo_to_vrm, BvhFile},
    HumanoidTemplate, Joint,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Ok(bvh_path) = std::env::var("BVH_PATH") else {
        eprintln!("BVH_PATH env var not set");
        eprintln!(
            "Usage: BVH_PATH=path/to/motion.bvh cargo run -p alice-lol-humanoid \
             --example humanoid_bvh_animation"
        );
        return Ok(());
    };

    // bind pose 選択: VRM_PATH 有 (feature vrm 必要) → VRM、無 → default
    let bind_pose = load_bind_pose()?;
    println!("bind pose joints:  {}", bind_pose.joints.len());
    println!("bind pose bones:   {}", bind_pose.bones.len());

    let bvh = BvhFile::load(&bvh_path)?;
    println!("\nBVH loaded: {bvh_path}");
    println!("  root joint:      {}", bvh.skeleton.root.name);
    println!("  total channels:  {}", bvh.skeleton.total_channels);
    println!("  frame count:     {}", bvh.animation.frame_count);
    println!("  frame time (s):  {:.6}", bvh.animation.frame_time_s);

    // Mixamo → VRM 変換を先に試行、mapping empty なら CMU を試す
    let mixamo_map = mixamo_to_vrm();
    let cmu_map = cmu_to_vrm();

    let frames_to_sample = [
        0_usize,
        bvh.animation.frame_count / 4,
        bvh.animation.frame_count / 2,
    ];
    for frame_idx in frames_to_sample {
        if frame_idx >= bvh.animation.frame_count {
            continue;
        }
        let mixamo_rotations = bvh.frame_rotations_named(frame_idx, |n| mixamo_map.get(n).cloned());
        let use_cmu = mixamo_rotations.is_empty();
        let rotations = if use_cmu {
            bvh.frame_rotations_named(frame_idx, |n| cmu_map.get(n).cloned())
        } else {
            mixamo_rotations
        };
        let mapper_name = if use_cmu { "cmu" } else { "mixamo" };

        println!(
            "\n=== frame {frame_idx} (mapper={mapper_name}, mapped rotations={}) ===",
            rotations.len()
        );
        let posed = bind_pose.with_pose(&rotations);

        let sample = [
            Joint::Head,
            Joint::Chest,
            Joint::Waist,
            Joint::LWrist,
            Joint::LAnkle,
        ];
        for j in sample {
            if let Some([x, y, z]) = posed.joints.get(&j).copied() {
                println!("  {:<11} [{x:>7.3}, {y:>7.3}, {z:>7.3}]", format!("{j:?}"),);
            }
        }
    }

    Ok(())
}

#[cfg(feature = "vrm")]
fn load_bind_pose() -> Result<HumanoidTemplate, Box<dyn std::error::Error>> {
    if let Ok(vrm_path) = std::env::var("VRM_PATH") {
        println!("VRM bind pose from: {vrm_path}");
        match HumanoidTemplate::from_vrm(&vrm_path)? {
            Some(t) => return Ok(t),
            None => {
                eprintln!("VRM に humanoid bones が見つからないため default 使用");
            }
        }
    }
    println!("VRM_PATH 未設定 (or 抽出失敗) → default template 使用");
    Ok(HumanoidTemplate::default())
}

#[cfg(not(feature = "vrm"))]
fn load_bind_pose() -> Result<HumanoidTemplate, Box<dyn std::error::Error>> {
    if std::env::var("VRM_PATH").is_ok() {
        eprintln!("VRM_PATH set だが feature vrm off → --features vrm 追加要");
    }
    println!("default template を bind pose に使用");
    Ok(HumanoidTemplate::default())
}
