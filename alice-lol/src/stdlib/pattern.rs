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

    // ── 実プリント合格 baseline (Bamboo models/ 配下 .3mf 記録あり + Bamboo simulation で追加検証済) ──
    // Phase B.2 代替 (2026-08-06): Bamboo `examples/compute_pattern_scores.rs` で全 13 pattern の
    // Simulation score 実測、UserFieldTest + Simulation Excellent の pattern は Both に昇格

    /// 100yen 型 shopping cart coin (Bamboo `models/accessories/shopping-cart-coin/`)
    pub const SHOPPING_CART_COIN_100YEN: LolPattern = LolPattern {
        name: "shopping_cart_coin_100yen",
        description: "Φ22.8 × 1.7mm 100 円硬貨型キーホルダーコイン (Bamboo 実プリント合格 + Sim Excellent 88)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::Both,
        printability_score: Some(88),
        field_test: None, // Phase B.2 で date/material/printer/notes 埋め込み
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/accessories/shopping-cart-coin/generate.py"),
    };

    /// IKEA SKADIS 300×300 panel (Bamboo `models/wall-organizer/skadis-300x300/`)
    pub const SKADIS_PANEL_300X300: LolPattern = LolPattern {
        name: "skadis_panel_300x300",
        description:
            "IKEA SKADIS 互換 300×300×5mm ペグボード + 千鳥ペグ穴 98 個 (Sim Excellent 88、tight_aabb 修正 + RepeatFinite→Union で watertight 保証)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::Both,
        printability_score: Some(88),
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-300x300/generate.py"),
    };

    /// U 字棚仕切り (Bamboo `models/shelf/divider-560x250x120/`、Rust generator canonical)
    /// Sim 60 Acceptable (warp Critical + overhang 40、560mm 幅で env open-air PLA warp 高判定)
    /// だが 30lbs 荷重実プリント合格 baseline のため UserFieldTest 維持 (CI gate は field test 経由通過)
    pub const SHELF_DIVIDER_560X250X120: LolPattern = LolPattern {
        name: "shelf_divider_560x250x120",
        description:
            "U 字棚仕切り 560×250×120mm + hex 抜き穴 (板反り対策、30lbs 実荷重合格、Sim 60)",
        route: PatternRoute::SdfMarchingCubes,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: Some(60),
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("src/generators/shelf_divider.rs"),
    };

    // ── SKADIS アクセサリー 6 種 (Bamboo models/wall-organizer/、Phase B.1.c で primitive 実装予定) ──

    /// SKADIS J 型 hook (Sim 79 Good、hook 形状 overhang 30%+ が主因、Field test で通過)
    pub const SKADIS_HOOK_J: LolPattern = LolPattern {
        name: "skadis_hook_j",
        description: "IKEA SKADIS 互換 J 型 hook (Bamboo Python 2D+extrude canonical、Sim 79)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: Some(79),
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-hook-j/generate.py"),
    };

    /// SKADIS L 型 hook (Sim 79 Good、直角曲げの overhang が主因、Field test で通過)
    pub const SKADIS_HOOK_L: LolPattern = LolPattern {
        name: "skadis_hook_l",
        description:
            "IKEA SKADIS 互換 L 型 hook (直角曲げ、Bamboo Python 2D+extrude canonical、Sim 79)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: Some(79),
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-hook-l/generate.py"),
    };

    /// SKADIS S 型 hook (Sim 88 Excellent)
    pub const SKADIS_HOOK_S: LolPattern = LolPattern {
        name: "skadis_hook_s",
        description:
            "IKEA SKADIS 互換 S 型 hook (S 字曲げ、Bamboo Python 2D+extrude canonical、Sim 88)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::Both,
        printability_score: Some(88),
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-hook-s/generate.py"),
    };

    /// SKADIS container (小物入れ、2 peg、Sim 76 Good、Field test で通過)
    /// tight_aabb 修正で真の bbox (68×72×81mm) 取得後 overhang 40 で score 降格
    pub const SKADIS_CONTAINER: LolPattern = LolPattern {
        name: "skadis_container",
        description: "IKEA SKADIS 互換 container (2 peg、gusset ribs で補強、Sim 76)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: Some(76),
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-container/generate.py"),
    };

    /// SKADIS clip (単 peg、Sim 88 Excellent)
    pub const SKADIS_CLIP: LolPattern = LolPattern {
        name: "skadis_clip",
        description: "IKEA SKADIS 互換 clip (単 peg、細物ホルダー、Sim 88)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::Both,
        printability_score: Some(88),
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-clip/generate.py"),
    };

    /// SKADIS shelf (2 peg 棚、Sim 70 Good、Field test で通過)
    /// tight_aabb 修正で真の bbox (260×26×82mm) 取得後 overhang 10 で score 降格
    /// PETG 棚荷重 30 lbs 実プリント baseline のため UserFieldTest 維持
    pub const SKADIS_SHELF: LolPattern = LolPattern {
        name: "skadis_shelf",
        description: "IKEA SKADIS 互換 shelf (2 peg、rib 補強棚板、Sim 70)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::UserFieldTest,
        printability_score: Some(70),
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-shelf/generate.py"),
    };

    /// SKADIS elastic cord holder (伸縮ホルダー、Sim 88 Excellent)
    pub const SKADIS_ELASTIC_CORD: LolPattern = LolPattern {
        name: "skadis_elastic_cord",
        description: "IKEA SKADIS 互換 elastic cord holder (伸縮バンドで固定、Sim 88)",
        route: PatternRoute::SdfDualContouring,
        certified_by: CertificationSource::Both,
        printability_score: Some(88),
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("models/wall-organizer/skadis-elastic-cord/generate.py"),
    };

    // ── Bamboo simulation certified (未実地検証、Sim score のみで CI gate 判定) ──
    // gridfinity_bin: Sim 97 Excellent → gate 通過、wall_hook / drawer_organizer は
    // Sim < 85 なので gate 未通過 (実プリント baseline or 設計 revise 必要)

    /// 壁掛けフック (Bamboo `generators/hook.rs`、Sim 79 Good、CI gate 未通過)
    /// tight_aabb 修正で真の bbox (26×82×50mm) 取得、overhang 40 (前 10 誤値) から改善
    /// gate 通過には (a) spec 見直し (angle 45deg 制約) / (b) user field test で 85+ 判定
    pub const WALL_HOOK: LolPattern = LolPattern {
        name: "wall_hook",
        description:
            "壁掛けフック (荷重指定 kgf で応力逆算、Bamboo Rust generator canonical、Sim 79)",
        route: PatternRoute::SdfMarchingCubes,
        certified_by: CertificationSource::BambooSimulation,
        printability_score: Some(79),
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("src/generators/hook.rs"),
    };

    /// Gridfinity bin (Bamboo `generators/gridfinity.rs`、Sim 79 Good、CI gate 未通過)
    /// tight_aabb 修正で真の bbox (92×92×41mm) 取得、mesh 生成成功 (前 mesh 0 の bug 解消)
    /// overhang 40 (前 100 誤値) で score 降格、gate 通過には field test で 85+ 判定必要
    pub const GRIDFINITY_BIN: LolPattern = LolPattern {
        name: "gridfinity_bin",
        description: "Gridfinity 互換 bin (42mm grid、任意 units × height × dividers、Sim 79)",
        route: PatternRoute::SdfMarchingCubes,
        certified_by: CertificationSource::BambooSimulation,
        printability_score: Some(79),
        field_test: None,
        source_crate: "alice-lol",
        source_version: env!("CARGO_PKG_VERSION"),
        bamboo_canonical: Some("src/generators/gridfinity.rs"),
    };

    /// Drawer organizer (Bamboo `generators/drawer.rs`、Sim 72 Good、CI gate 未通過)
    /// warp High + overhang 30%+ (252×202×42mm 大型 flat) が主因、tight_aabb 修正で真の
    /// bbox 取得後 score 変動 gate 通過には field test で 85+ 判定必要
    pub const DRAWER_ORGANIZER: LolPattern = LolPattern {
        name: "drawer_organizer",
        description: "引出し仕切り (chopsticks/fork/knife/spoon/marker/pen 等 slot 定義、Sim 72)",
        route: PatternRoute::SdfMarchingCubes,
        certified_by: CertificationSource::BambooSimulation,
        printability_score: Some(72),
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
// Phase D.2: 85 点 CI gate contract
// ────────────────────────────────────────────────────────

/// pattern の CI merge gate 判定
///
/// 通過条件 (OR): `printability_score >= 85` (Bamboo `PrintabilityScore::Excellent`)
/// または `certified_by` が `UserFieldTest` を含む (実プリント合格 baseline)
///
/// 未検証 (score None + `certified_by = None`) の pattern は **通過しない**
///
/// Phase D.1 の `alice_bamboo::rating::PrintabilityScore` で計算した score を
/// registry の `printability_score` field に埋めた前提、Phase B.2 で
/// `field_test` を埋めた前提で機能する
#[must_use]
pub fn pattern_passes_ci_gate(pattern: &LolPattern) -> bool {
    let score_ok = pattern.printability_score.is_some_and(|s| s >= 85);
    let field_tested = pattern.certified_by.includes_field_test();
    score_ok || field_tested
}

/// registry 全 pattern の CI gate 判定 (未通過 pattern 名 list を返す)
///
/// # 使用例
///
/// ```
/// use alice_lol::stdlib::pattern::patterns_failing_ci_gate;
/// let failing = patterns_failing_ci_gate();
/// // Phase B.2 完成前は wall_hook / gridfinity_bin / drawer_organizer が未通過
/// // (certified_by = None、score None) 想定
/// ```
#[must_use]
pub fn patterns_failing_ci_gate() -> Vec<&'static LolPattern> {
    registry::ALL
        .iter()
        .filter(|p| !pattern_passes_ci_gate(p))
        .collect()
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
        // Phase B.2 代替 (2026-08-06): Bamboo simulation 反映後、全 13 pattern が
        // UserFieldTest / Both / BambooSimulation のいずれかで certified、None は 0
        let uncertified: Vec<_> = registry::ALL
            .iter()
            .filter(|p| p.certified_by == CertificationSource::None)
            .collect();
        assert_eq!(uncertified.len(), 0);
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
        // Phase B.2 代替 (2026-08-06): Bamboo Sim Excellent 88 + UserFieldTest = Both 昇格
        assert_eq!(p.certified_by, CertificationSource::Both);
        assert_eq!(p.printability_score, Some(88));
    }

    #[test]
    fn find_wall_hook_bamboo_sim_certified() {
        // Phase B.2 代替 (2026-08-07): tight_aabb 修正で真 bbox 取得、Sim 70 → 79 改善
        // 依然 Sim < 85 で CI gate 未通過 (user field test 待ち or spec 見直し必要)
        let p = find_by_name("wall_hook").expect("registered");
        assert_eq!(p.certified_by, CertificationSource::BambooSimulation);
        assert_eq!(p.printability_score, Some(79));
        assert!(p.bamboo_canonical.is_some());
        assert!(
            !pattern_passes_ci_gate(p),
            "wall_hook は Sim 79 なので CI gate 未通過"
        );
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
    fn ci_gate_field_tested_patterns_pass() {
        // 実プリント合格 baseline (UserFieldTest) は score なしでも通過
        for p in registry::ALL {
            if p.certified_by.includes_field_test() {
                assert!(
                    pattern_passes_ci_gate(p),
                    "{}: field-tested but CI gate fail",
                    p.name
                );
            }
        }
    }

    #[test]
    fn ci_gate_uncertified_without_score_fail() {
        // certified_by = None + score None は通過しない
        for p in registry::ALL {
            if p.certified_by == CertificationSource::None && p.printability_score.is_none() {
                assert!(
                    !pattern_passes_ci_gate(p),
                    "{}: uncertified + score None は CI gate 失敗すべき",
                    p.name
                );
            }
        }
    }

    #[test]
    fn ci_gate_hypothetical_high_score_passes() {
        // score >= 85 なら certified_by 無視で通過
        let p = LolPattern {
            printability_score: Some(90),
            certified_by: CertificationSource::None,
            ..registry::WALL_HOOK
        };
        assert!(pattern_passes_ci_gate(&p));
    }

    #[test]
    fn patterns_failing_ci_gate_matches_uncertified_count() {
        // Phase B.2 代替 (2026-08-07): tight_aabb 修正で全 pattern score 再測、
        // gridfinity_bin が Sim 97 → 79 で gate 通過 → 未通過に降格
        // gate 通過: UserFieldTest 系 10 pattern = 10
        // gate 未通過: wall_hook (Sim 79) + gridfinity_bin (Sim 79) + drawer_organizer (Sim 72) = 3
        let failing = patterns_failing_ci_gate();
        assert_eq!(
            failing.len(),
            3,
            "tight_aabb 修正で真 bbox 反映、wall_hook + gridfinity_bin + drawer_organizer が Sim < 85"
        );
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
