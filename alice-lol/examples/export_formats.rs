//! 出力形式の入口を 1 本ずつ通す — STL / 3MF / FBX と解像度 preset.
//!
//! ```text
//! cargo run -p alice-lol --example export_formats
//! ```
//!
//! `print_export` には形式ごとに入口があり、同じ mesh を別の container に書く
//! ここでは **LOL text から** と **`SdfNode` から** の両方を 1 回ずつ呼んで、
//! 書かれた file を container の仕様側から読み直して三角形数を突き合わせる
//! (⚠️ 書き手の実装を信じずに、file の中身を数える)
//!
//! 解像度 preset は `PrintConfig::{preview, default, high_quality, ultra}` で、
//! 名前が示す順に cell が細かくなる 実際に刻むのは `preview` だけにしてある
//! (`ultra` は res 512 なので example の実行時間に乗せない)

use alice_lol::print_export::{
    lol_to_3mf, lol_to_fbx, node_to_fbx, node_to_mesh, node_to_stl, PrintConfig,
};

const LOL: &str = "smooth_union(0.1, sphere(0.9), translate(0.8, 0.0, 0.0, box3d(0.4, 0.4, 0.4)))";

/// 3MF は OPC (zip) container だが、中の `3D/3dmodel.model` は**非圧縮**で格納される
/// ので、byte 列として `<triangle ` を数えれば面数が出る
/// ⚠️ `read_to_string` では読めない (container 全体は UTF-8 でない)
fn triangles_in_3mf(path: &std::path::Path) -> usize {
    let bytes = std::fs::read(path).expect("3MF を読めない");
    let needle = b"<triangle ";
    bytes.windows(needle.len()).filter(|w| *w == needle).count()
}

fn main() {
    let dir = std::env::temp_dir().join(format!("alice_lol_export_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("出力先を作れない");
    let config = PrintConfig::preview();

    // 同じ形を 1 度だけ刻んで、面数の基準にする
    let node = alice_lol::runtime_parser::parse_lol(LOL).expect("LOL をパースできない");
    let mesh = node_to_mesh(&node, &config);
    let faces = mesh.indices.len() / 3;
    println!("=== 基準 (preview: res {}) ===", config.resolution);
    println!("  頂点 {} / 三角形 {faces}", mesh.vertices.len());

    println!("\n=== 形式ごとの入口 ===");
    let stl = dir.join("a.stl");
    let s = node_to_stl(&node, &stl, &config).expect("STL");
    // STL binary は 84 + 50n byte なので、file 長から面数が逆算できる
    let bytes = std::fs::metadata(&stl).expect("STL の大きさ").len();
    println!(
        "  node_to_stl   三角形 {} / {} byte (= 84 + 50 × {})",
        s.triangle_count,
        bytes,
        (bytes - 84) / 50
    );

    let three_mf = dir.join("b.3mf");
    let s = lol_to_3mf(LOL, &three_mf, &config).expect("3MF");
    println!(
        "  lol_to_3mf    三角形 {} / file 内の <triangle> {}",
        s.triangle_count,
        triangles_in_3mf(&three_mf)
    );

    for (label, path, result) in [
        (
            "node_to_fbx  ",
            dir.join("c.fbx"),
            node_to_fbx(&node, dir.join("c.fbx"), &config),
        ),
        (
            "lol_to_fbx   ",
            dir.join("d.fbx"),
            lol_to_fbx(LOL, dir.join("d.fbx"), &config),
        ),
    ] {
        match result {
            Ok(s) => {
                let n = std::fs::metadata(&path).map_or(0, |m| m.len());
                println!("  {label} 三角形 {} / {n} byte", s.triangle_count);
            }
            // ⚠️ 入口が壊れていても example を黙って緑にしない
            Err(e) => println!("  {label} ⚠️ 失敗: {e}"),
        }
    }

    println!("\n=== 解像度 preset (刻まずに値だけ出す) ===");
    for (name, c) in [
        ("preview", PrintConfig::preview()),
        ("default", PrintConfig::default()),
        ("high_quality", PrintConfig::high_quality()),
        ("ultra", PrintConfig::ultra()),
    ] {
        let span = c.bounds_max.x - c.bounds_min.x;
        // ⚠️ `as f32` は精度損失で clippy pedantic に落ちる preset の res は 64..=512 なので
        //    u32 に収まり、f32 は 2^24 までの整数を厳密に表すので lossless に変換できる
        let res = u32::try_from(c.resolution).expect("preset の resolution は u32 に収まる");
        let cell =
            span / f32::from(u16::try_from(res).expect("preset の resolution は u16 に収まる"));
        println!(
            "  {name:<13} res {:<4} cell {cell:.5} (world) / {:.4} mm",
            c.resolution,
            cell * c.scale_mm
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}
