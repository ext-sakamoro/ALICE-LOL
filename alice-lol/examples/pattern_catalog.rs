//! # pattern_catalog — LOL pattern registry catalog view (Phase B.1.a)
//!
//! stdlib::pattern registry (13 pattern) を dump し、certification level と
//! route 別に集計する text-to-print GUI の「実プリント合格 pattern 一覧」
//! source として使う想定
//!
//! ```bash
//! cargo run --example pattern_catalog
//! ```

use alice_lol::stdlib::pattern::{
    field_tested_patterns, patterns_by_route, registry, CertificationSource, PatternRoute,
};

fn main() {
    println!("=== ALICE-LOL Phase B.1.a — pattern registry catalog ===\n");

    // ────────────────────────────────
    // Registry 全体
    // ────────────────────────────────
    println!("Registered patterns: {}", registry::ALL.len());
    println!();
    for (i, p) in registry::ALL.iter().enumerate() {
        println!(
            "  [{i:>2}] {:<32} route={:<28} cert={}",
            p.name,
            p.route.as_str(),
            p.certified_by.as_str()
        );
    }

    // ────────────────────────────────
    // 実プリント合格 baseline
    // ────────────────────────────────
    let ft = field_tested_patterns();
    println!("\n--- Field-tested baseline ({} patterns) ---", ft.len());
    for p in &ft {
        let bamboo = p.bamboo_canonical.unwrap_or("(none)");
        println!("  {:<32} → {bamboo}", p.name);
    }

    // ────────────────────────────────
    // route 別集計
    // ────────────────────────────────
    let thin = patterns_by_route(PatternRoute::SdfDualContouring);
    let thick = patterns_by_route(PatternRoute::SdfMarchingCubes);
    println!(
        "\n--- Route breakdown ---\n  SdfDualContouring (薄物): {} patterns\n  SdfMarchingCubes (厚物): {} patterns",
        thin.len(),
        thick.len()
    );

    // ────────────────────────────────
    // certification 別集計
    // ────────────────────────────────
    let uncertified = registry::ALL
        .iter()
        .filter(|p| p.certified_by == CertificationSource::None)
        .count();
    println!(
        "\n--- Certification breakdown ---\n  UserFieldTest: {}\n  BambooSimulation: {}\n  Both: {}\n  None: {}",
        registry::ALL
            .iter()
            .filter(|p| p.certified_by == CertificationSource::UserFieldTest)
            .count(),
        registry::ALL
            .iter()
            .filter(|p| p.certified_by == CertificationSource::BambooSimulation)
            .count(),
        registry::ALL
            .iter()
            .filter(|p| p.certified_by == CertificationSource::Both)
            .count(),
        uncertified,
    );

    println!("\n=== Done ===");
    println!(
        "field_test 実データ (date/material/printer/notes) は Phase B.2 で user 実測記録を登録"
    );
    println!("printability_score は Phase D.1 で alice_bamboo::rating 経由で埋める");
}
