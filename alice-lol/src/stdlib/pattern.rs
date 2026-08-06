//! # pattern — LOL pattern registry (Phase B.1.a、実プリント合格 baseline metadata)
//!
//! [`LolPattern`] 型で実プリント合格資産 (Bamboo `models/` 配下 10 品目) と
//! 未実地検証の pattern (Bamboo Rust generator 3 種) の metadata を統一管理する
//!
//! ## 目的
//!
//! LOL stdlib::hardsurface::{fastener, joint, reinforcement, mount, thin} の 27 primitive
//! を組み合わせた「完成 pattern」を registry として登録し、certification level
//! (実プリント検証済み / Bamboo simulation のみ / 未検証) を明示化する
//!
//! ## certified_by 分類
//!
//! - **`UserFieldTest`**: user が実プリントして動作確認済 (Bamboo `models/` 配下)
//! - **`BambooSimulation`**: Bamboo `alice_bamboo::safety::safety_validate` 通過のみ
//! - **`Both`**: 上記両方
//! - **`None`**: 未検証 (primitive 組立て段階、実プリント推奨せず)
//!
//! ## route 分類
//!
//! - **`SdfMarchingCubes`**: `SdfNode` + `sdf_to_mesh` (厚物、Marching Cubes 経路)
//! - **`SdfDualContouring`**: `SdfNode` + `dual_contouring` (薄物 <= 5mm を含む全物、Phase 3'' 追加 ALICE 三相原理 Phase 2 Law 準拠、Hermite data で watertight 保証、極薄物 1.7mm でも `non_manifold_edges = 0` 実測済)
//!
//! ## 実測データの補填 (Phase B.2)
//!
//! [`FieldTestRecord`] の date/material/printer/notes は user 実測記録待ち
//! 現状 [`registry`] の `field_test` は `None` で skeleton のみ、Phase B.2 で実データ登録

// ────────────────────────────────────────────────────────
// 型定義
// ────────────────────────────────────────────────────────

/// pattern の mesh 生成経路
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PatternRoute {
    /// `SdfNode` + marching cubes (厚物 > 5mm 向け)、`alice_lol::print_export::node_to_3mf`
    SdfMarchingCubes,
    /// `SdfNode` + `dual_contouring` (薄物 <= 5mm を含む全物向け)、`alice_lol::print_export::node_to_3mf_dual_contouring`
    /// Phase 3'' 追加、ALICE 三相原理 Phase 2 Law 準拠 Hermite data で watertight 保証
    SdfDualContouring,
}

impl PatternRoute {
    /// 経路名 (人間可読)
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SdfMarchingCubes => "SDF+MC (marching cubes)",
            Self::SdfDualContouring => "SDF+DC (dual contouring)",
        }
    }
}

/// certification level (実プリント検証状態)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CertificationSource {
    /// 未検証 (primitive 組立て段階、実プリント推奨せず)
    None,
    /// Bamboo `alice_bamboo::safety::safety_validate` 通過のみ、実プリント未実施
    BambooSimulation,
    /// user 実プリントで動作確認済 (Bamboo `models/` 配下に .3mf 記録あり)
    UserFieldTest,
    /// 上記両方 (最高信頼度)
    Both,
}

impl CertificationSource {
    /// certification 名 (人間可読)
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "uncertified",
            Self::BambooSimulation => "Bamboo simulation only",
            Self::UserFieldTest => "user field test",
            Self::Both => "user field test + Bamboo simulation",
        }
    }

    /// `UserFieldTest` を含むか (実プリント合格 baseline 判定)
    #[must_use]
    pub const fn includes_field_test(self) -> bool {
        matches!(self, Self::UserFieldTest | Self::Both)
    }
}

/// user 実地テスト成績 (Bamboo `models/` の実プリント記録)
///
/// フィールドはすべて `&'static str` で const 定義可能 (Phase B.2 で実データ埋め込み)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldTestRecord {
    /// 実プリント日付 (ISO 8601、例: `"2026-01-15"`)
    pub date: &'static str,
    /// 使用材料 (例: `"PLA"` / `"PETG"` / `"ABS"`)
    pub material: &'static str,
    /// 使用プリンタ (例: `"Bambu H2D"` / `"Bambu P1S"`)
    pub printer: &'static str,
    /// 実測メモ (自由記述、実プリント時の観察点等)
    pub notes: &'static str,
}

/// LOL pattern metadata (`stdlib::hardsurface` primitive の組み合わせ pattern)
#[derive(Debug, Clone, Copy)]
pub struct LolPattern {
    /// pattern 識別名 (`snake_case`)
    pub name: &'static str,
    /// pattern 説明 (人間可読、寸法・用途)
    pub description: &'static str,
    /// mesh 生成経路
    pub route: PatternRoute,
    /// certification level
    pub certified_by: CertificationSource,
    /// Bamboo `PrintabilityScore` (Phase D.1 で実装予定、0-100)
    /// 現状 None、Phase D.1 で `alice_bamboo::rating` 経由で埋める
    pub printability_score: Option<u8>,
    /// user 実測記録 (Phase B.2 で実データ埋め込み予定)
    pub field_test: Option<FieldTestRecord>,
    /// pattern 実装 crate (常に `"alice-lol"`)
    pub source_crate: &'static str,
    /// pattern 実装 crate version (`env!("CARGO_PKG_VERSION")` の compile-time 値)
    pub source_version: &'static str,
    /// Bamboo canonical 実装位置 (人間可読、例: `"models/accessories/shopping-cart-coin/generate.py"`)
    /// 未実地検証 pattern の場合は `None`
    pub bamboo_canonical: Option<&'static str>,
}

// ────────────────────────────────────────────────────────
// registry — 12 pattern metadata
// ────────────────────────────────────────────────────────

/// pattern registry (実プリント合格 baseline + 未検証 pattern 全 12 件)
pub mod registry {
    use super::{CertificationSource, LolPattern, PatternRoute};

    // ── 実プリント合格 baseline (certified_by = UserFieldTest、Bamboo models/ に .3mf あり) ──

    /// 100yen 型 shopping cart coin (Bamboo `models/accessories/shopping-cart-coin/`)
    pub const SHOPPING_CART_COIN_100YEN: LolPattern = LolPattern {
        name: "shopping_cart_coin_100yen",
        description: "Φ22.8 × 1.7mm 100 円硬貨型キーホルダーコイン (Bamboo 実プリント合格)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: None,
        field_test: None, // Phase B.2 で埋め込み
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/accessories/shopping-cart-coin/generate.py"),
    };

    /// IKEA SKADIS 300×300 panel (Bamboo `models/wall-organizer/skadis-300x300/`)
    pub const SKADIS_PANEL_300X300: LolPattern = LolPattern {
        name: "skadis_panel_300x300",
        description: "IKEA SKADIS 互換 300×300×5mm ペグボード + 千鳥ペグ穴 98 個",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: None,
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-300x300/generate.py"),
    };

    /// U 字棚仕切り (Bamboo `models/shelf/divider-560x250x120/`、Rust generator canonical)
    pub const SHELF_DIVIDER_560X250X120: LolPattern = LolPattern {
        name: "shelf_divider_560x250x120",
        description: "U 字棚仕切り 560×250×120mm + hex 抜き穴 (板反り対策)",
        route: PatternRoute::SdfMarchingCubes,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: None,
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("src/generators/shelf_divider.rs"),
    };

    // ── SKADIS アクセサリー 6 種 (Bamboo models/wall-organizer/、Phase B.1.c で primitive 実装予定) ──

    /// SKADIS J 型 hook
    pub const SKADIS_HOOK_J: LolPattern = LolPattern {
        name: "skadis_hook_j",
        description: "IKEA SKADIS 互換 J 型 hook (Bamboo Python 2D+extrude canonical)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: None,
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-hook-j/generate.py"),
    };

    /// SKADIS L 型 hook
    pub const SKADIS_HOOK_L: LolPattern = LolPattern {
        name: "skadis_hook_l",
        description: "IKEA SKADIS 互換 L 型 hook (直角曲げ、Bamboo Python 2D+extrude canonical)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: None,
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-hook-l/generate.py"),
    };

    /// SKADIS S 型 hook
    pub const SKADIS_HOOK_S: LolPattern = LolPattern {
        name: "skadis_hook_s",
        description: "IKEA SKADIS 互換 S 型 hook (S 字曲げ、Bamboo Python 2D+extrude canonical)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: None,
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-hook-s/generate.py"),
    };

    /// SKADIS container (小物入れ、2 peg)
    pub const SKADIS_CONTAINER: LolPattern = LolPattern {
        name: "skadis_container",
        description: "IKEA SKADIS 互換 container (2 peg、gusset ribs で補強)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: None,
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-container/generate.py"),
    };

    /// SKADIS clip (単 peg)
    pub const SKADIS_CLIP: LolPattern = LolPattern {
        name: "skadis_clip",
        description: "IKEA SKADIS 互換 clip (単 peg、細物ホルダー)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: None,
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-clip/generate.py"),
    };

    /// SKADIS shelf (2 peg 棚)
    pub const SKADIS_SHELF: LolPattern = LolPattern {
        name: "skadis_shelf",
        description: "IKEA SKADIS 互換 shelf (2 peg、rib 補強棚板)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: None,
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-shelf/generate.py"),
    };

    /// SKADIS elastic cord holder (伸縮ホルダー)
    pub const SKADIS_ELASTIC_CORD: LolPattern = LolPattern {
        name: "skadis_elastic_cord",
        description: "IKEA SKADIS 互換 elastic cord holder (伸縮バンドで固定)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: None,
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-elastic-cord/generate.py"),
    };

    // ── 未実地検証 pattern (certified_by = None、Bamboo Rust generator のみ、Phase B.1.b で LOL 移設) ──

    /// 壁掛けフック (Bamboo `generators/hook.rs`、実プリント未検証)
    pub const WALL_HOOK: LolPattern = LolPattern {
        name: "wall_hook",
        description: "壁掛けフック (荷重指定 kgf で応力逆算、Bamboo Rust generator canonical)",
        route: PatternRoute::SdfMarchingCubes,
        certified_by: CertificationSource::None,
        printability_score: None,
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("src/generators/hook.rs"),
    };

    /// Gridfinity bin (Bamboo `generators/gridfinity.rs`、実プリント未検証)
    pub const GRIDFINITY_BIN: LolPattern = LolPattern {
        name: "gridfinity_bin",
        description: "Gridfinity 互換 bin (42mm grid、任意 units × height × dividers)",
        route: PatternRoute::SdfMarchingCubes,
        certified_by: CertificationSource::None,
        printability_score: None,
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("src/generators/gridfinity.rs"),
    };

    /// Drawer organizer (Bamboo `generators/drawer.rs`、実プリント未検証)
    pub const DRAWER_ORGANIZER: LolPattern = LolPattern {
        name: "drawer_organizer",
        description: "引出し仕切り (chopsticks/fork/knife/spoon/marker/pen 等 slot 定義)",
        route: PatternRoute::SdfMarchingCubes,
        certified_by: CertificationSource::None,
        printability_score: None,
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("src/generators/drawer.rs"),
    };

    /// registry 全 pattern (12 件、iterate 用)
    pub const ALL: &[LolPattern] = &[
        SHOPPING_CART_COIN_100YEN,
        SKADIS_PANEL_300X300,
        SHELF_DIVIDER_560X250X120,
        SKADIS_HOOK_J,
        SKADIS_HOOK_L,
        SKADIS_HOOK_S,
        SKADIS_CONTAINER,
        SKADIS_CLIP,
        SKADIS_SHELF,
        SKADIS_ELASTIC_CORD,
        WALL_HOOK,
        GRIDFINITY_BIN,
        DRAWER_ORGANIZER,
    ];
}

// ────────────────────────────────────────────────────────
// filter helpers
// ────────────────────────────────────────────────────────

/// 実プリント合格 baseline pattern のみを filter (`certified_by.includes_field_test()`)
#[must_use]
pub fn field_tested_patterns() -> Vec<&'static LolPattern> {
    registry::ALL
        .iter()
        .filter(|p| p.certified_by.includes_field_test())
        .collect()
}

/// 経路別 filter (`route == PatternRoute::SdfDualContouring` 等)
#[must_use]
pub fn patterns_by_route(route: PatternRoute) -> Vec<&'static LolPattern> {
    registry::ALL.iter().filter(|p| p.route == route).collect()
}

/// name 完全一致で pattern 検索
#[must_use]
pub fn find_by_name(name: &str) -> Option<&'static LolPattern> {
    registry::ALL.iter().find(|p| p.name == name)
}

// ────────────────────────────────────────────────────────
// テスト
// ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_expected_pattern_count() {
        // Phase B.1.a: 12 pattern (実プリント合格 baseline 10 + 未検証 3)、待って 13
        // 実プリント合格 = SHOPPING_CART_COIN + SKADIS_PANEL + SHELF_DIVIDER + SKADIS_HOOK_J/L/S + CONTAINER + CLIP + SHELF + ELASTIC_CORD = 10
        // 未検証 = WALL_HOOK + GRIDFINITY_BIN + DRAWER_ORGANIZER = 3
        // 合計 13
        assert_eq!(registry::ALL.len(), 13);
    }

    #[test]
    fn field_tested_patterns_count() {
        // 実プリント合格 = 10 品目 (Bamboo models/ 配下)
        let ft = field_tested_patterns();
        assert_eq!(ft.len(), 10);
    }

    #[test]
    fn uncertified_patterns_count() {
        let uncertified: Vec<_> = registry::ALL
            .iter()
            .filter(|p| p.certified_by == CertificationSource::None)
            .collect();
        assert_eq!(uncertified.len(), 3);
    }

    #[test]
    fn dual_contouring_route_count() {
        let thin = patterns_by_route(PatternRoute::SdfDualContouring);
        // coin + skadis_panel + 7 skadis accessories (hook_j/l/s/container/clip/shelf/elastic) = 9
        assert_eq!(thin.len(), 9);
    }

    #[test]
    fn sdf_route_count() {
        let thick = patterns_by_route(PatternRoute::SdfMarchingCubes);
        // shelf_divider + wall_hook + gridfinity + drawer = 4
        assert_eq!(thick.len(), 4);
    }

    #[test]
    fn routes_sum_matches_registry_all() {
        let thin = patterns_by_route(PatternRoute::SdfDualContouring).len();
        let thick = patterns_by_route(PatternRoute::SdfMarchingCubes).len();
        assert_eq!(thin + thick, registry::ALL.len());
    }

    #[test]
    fn find_shopping_cart_coin() {
        let p = find_by_name("shopping_cart_coin_100yen").expect("registered");
        assert_eq!(p.route, PatternRoute::SdfDualContouring);
        assert_eq!(p.certified_by, CertificationSource::UserFieldTest);
    }

    #[test]
    fn find_wall_hook_is_uncertified() {
        let p = find_by_name("wall_hook").expect("registered");
        assert_eq!(p.certified_by, CertificationSource::None);
        assert!(p.bamboo_canonical.is_some());
    }

    #[test]
    fn find_missing_name_returns_none() {
        assert!(find_by_name("no_such_pattern").is_none());
    }

    #[test]
    fn certification_includes_field_test_flag() {
        assert!(CertificationSource::UserFieldTest.includes_field_test());
        assert!(CertificationSource::Both.includes_field_test());
        assert!(!CertificationSource::BambooSimulation.includes_field_test());
        assert!(!CertificationSource::None.includes_field_test());
    }

    #[test]
    fn all_patterns_have_alice_lol_source_crate() {
        for p in registry::ALL {
            assert_eq!(p.source_crate, "alice-lol");
        }
    }

    #[test]
    fn all_field_tested_have_bamboo_canonical() {
        for p in registry::ALL {
            if p.certified_by.includes_field_test() {
                assert!(
                    p.bamboo_canonical.is_some(),
                    "{}: field-tested pattern must have bamboo_canonical set",
                    p.name
                );
            }
        }
    }
}
