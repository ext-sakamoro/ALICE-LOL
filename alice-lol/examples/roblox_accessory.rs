//! Roblox アクセサリー出力サンプル
//!
//! LOL DSL → OBJ/FBX (Roblox MeshPart 用) の動作確認。
//!
//! ```bash
//! cargo run --example roblox_accessory --features roblox
//! ```

#[cfg(feature = "roblox")]
fn main() {
    use alice_lol::lol;
    use alice_lol::roblox_export::{node_to_fbx_roblox, node_to_obj_roblox, RobloxConfig};

    // ── 1. 王冠アクセサリー ──
    let crown = lol! {
        subtract(
            smooth_union(0.1,
                translate(0.0, 0.3, 0.0, torus(0.8, 0.15)),
                translate(0.0, 0.0, 0.0, cylinder(0.15, 0.7))
            ),
            translate(0.0, -0.3, 0.0, box3d(2.0, 0.6, 2.0))
        )
    };

    let config = RobloxConfig::accessory();

    match node_to_obj_roblox(&crown, "crown.obj", &config) {
        Ok(stats) => println!("Crown OBJ: {stats}"),
        Err(e) => eprintln!("Crown OBJ error: {e}"),
    }

    // ── 2. 宝石 (ランタイムパーサー経由) ──
    let gem_lol = r#"
        smooth_intersection(0.05,
            octahedron(0.6),
            box3d(0.5, 0.8, 0.5)
        )
    "#;

    match alice_lol::roblox_export::lol_to_fbx_roblox(gem_lol, "gem.fbx", &config) {
        Ok(stats) => println!("Gem FBX:   {stats}"),
        Err(e) => eprintln!("Gem FBX error: {e}"),
    }

    // ── 3. 盾 (MeshPart 設定) ──
    let shield = lol! {
        subtract(
            smooth_intersection(0.1,
                scale(1.5, sphere(1.0)),
                box3d(0.1, 1.2, 0.9)
            ),
            translate(0.0, 0.0, 0.3, sphere(0.6))
        )
    };

    let meshpart_config = RobloxConfig::meshpart();

    match node_to_fbx_roblox(&shield, "shield.fbx", &meshpart_config) {
        Ok(stats) => println!("Shield FBX: {stats}"),
        Err(e) => eprintln!("Shield FBX error: {e}"),
    }
}

#[cfg(not(feature = "roblox"))]
fn main() {
    eprintln!("roblox feature required: cargo run --example roblox_accessory --features roblox");
}
