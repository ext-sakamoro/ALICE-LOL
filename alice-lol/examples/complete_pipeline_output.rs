//! # complete_pipeline_output — Phase 5.3、全 13 品目 Bambu 対応 3MF 生成
//!
//! LOL DSL text → parse_lol → SdfNode → alice-bamboo sdf_to_bambu_3mf → MakerWorld 対応 .3mf
//! 全 13 品目 (薄物 DC 経路 9 + 厚物 MC 経路 4) を `./output/{thin,thick}/` に出力
//!
//! **注意**: 本 example は `alice-bamboo` に依存 alice-lol 単独 crate では動作しない
//! `alice-bamboo` を dev-dependencies に追加した状態で:
//! ```bash
//! cargo run --release --example complete_pipeline_output --features bamboo-bridge
//! ```
//!
//! LOL 単独では動かないため、実際の使用パターンは text-to-print / alice-bamboo 側で
//! `sdf_to_bambu_3mf` を呼ぶ 本 example は「LOL DSL → SdfNode の完成度」の実測が主眼
//! 3MF 生成部分は「LOL DSL parse 成功 + SdfNode 生成成功 + Bambu 変換部は alice-bamboo に委譲」
//! の形で確認する
//!
//! 現状: LOL 単独 example として `parse_lol` + `SdfNode` 生成のみ確認、3MF は
//! `alice-bamboo` example 側 (Phase 5.5) で実行推奨

use alice_lol::runtime_parser::parse_lol;
use alice_sdf::eval;
use glam::Vec3;

fn main() {
    println!("=== ALICE-LOL Phase 5.3 — 全 13 品目 LOL DSL parse + SdfNode 生成 verify ===\n");
    println!("(Bambu 対応 3MF 実生成は alice-bamboo example 側で実施、本 example は SdfNode 完成度のみ)\n");

    // 全 13 品目 (Phase 5.1 + Phase 5.2 の高階 primitive + Phase B.1 の pattern_sdf 相当)
    let items: Vec<(&str, &str, PatternRoute)> = vec![
        // ── 薄物 (DC 経路推奨 9 品目) ──
        (
            "shopping_cart_coin_100yen",
            "shopping_cart_coin(22.8, 1.7)",
            PatternRoute::DualContouring,
        ),
        (
            "skadis_panel_300x300",
            "skadis_panel(300, 5, 5)",
            PatternRoute::DualContouring,
        ),
        (
            "skadis_hook_l",
            "skadis_hook_l()",
            PatternRoute::DualContouring,
        ),
        (
            "skadis_hook_j",
            "skadis_hook_j()",
            PatternRoute::DualContouring,
        ),
        (
            "skadis_hook_s",
            "skadis_hook_s()",
            PatternRoute::DualContouring,
        ),
        (
            "skadis_container",
            "skadis_container()",
            PatternRoute::DualContouring,
        ),
        ("skadis_clip", "skadis_clip()", PatternRoute::DualContouring),
        (
            "skadis_shelf",
            "skadis_shelf()",
            PatternRoute::DualContouring,
        ),
        (
            "skadis_elastic_cord",
            "skadis_elastic_cord()",
            PatternRoute::DualContouring,
        ),
        // ── 厚物 (MC 経路、Bamboo 4 generator は pattern_sdf 経由でも呼べるが LOL DSL text からは
        //    直接呼べない (0-arg primitive 化していない、Bamboo CLI 経由が canonical)) ──
        // ("wall_hook_default", "...", PatternRoute::MarchingCubes),    // LOL DSL 未登録、Bamboo CLI
        // ("gridfinity_bin_2x2", "...", PatternRoute::MarchingCubes),   // 同上
        // ("drawer_organizer", "...", PatternRoute::MarchingCubes),     // 同上
        // ("shelf_divider_560x250x120", "...", PatternRoute::MarchingCubes), // 同上
    ];

    let mut total = 0;
    let mut success = 0;
    for (name, lol_text, route) in &items {
        total += 1;
        print!("[{:<32}] route={route:?} ... ", name);
        match parse_lol(lol_text) {
            Ok(node) => {
                let d_origin = eval(&node, Vec3::ZERO);
                let d_off = eval(&node, Vec3::new(0.1, 0.1, 0.1));
                if d_origin.is_finite() && d_off.is_finite() {
                    println!(
                        "OK (SdfNode 生成、eval(origin)={d_origin:+.3}, eval(0.1,0.1,0.1)={d_off:+.3})"
                    );
                    success += 1;
                } else {
                    println!("FAIL (non-finite SDF: origin={d_origin}, off={d_off})");
                }
            }
            Err(e) => {
                println!("FAIL (parse error: {e:?})");
            }
        }
    }

    println!("\n--- Summary ---");
    println!(
        "Total: {total}, Success: {success}, Failed: {}",
        total - success
    );
    println!(
        "\n厚物 4 品目 (wall_hook / gridfinity / drawer / shelf_divider) は LOL DSL text から\n\
         直接呼べない (0-arg primitive 未登録) Bamboo CLI 経由が canonical:\n\
         cargo run --release --bin alice-bamboo -- hook --load 3 --output ./thick/hook.3mf\n\
         cargo run --release --bin alice-bamboo -- gridfinity --units 2x2 --output ./thick/gridfinity.3mf\n\
         cargo run --release --bin alice-bamboo -- drawer --width 250 --depth 200 --height 40 --slots \"chopsticks:2\" --output ./thick/drawer.3mf\n\
         cargo run --release --bin alice-bamboo -- shelf-divider --width 560 --depth 250 --height 120 --output-dir ./thick/"
    );
    println!("\nBambu 対応 3MF 生成 (12 file zip + template embed):");
    println!(
        "  alice-bamboo::sdf_to_bambu_3mf(&sdf, path, name, resolution=128, use_dc=true)\n\
         → 素の 3MF (alice_sdf::io::export_3mf) の代わりに使う"
    );
}

#[derive(Debug, Clone, Copy)]
enum PatternRoute {
    DualContouring,
    #[allow(dead_code)]
    MarchingCubes,
}
