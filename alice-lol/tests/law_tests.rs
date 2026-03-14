//! 法則（Law）制約チェッカーのテスト

use alice_lol::law::{check_laws, CheckConfig, Constraint, Law, Priority};
use alice_lol::lol;
use glam::Vec3;

/// 重なる 2 sphere → NonOverlap 違反検出
#[test]
fn non_overlap_overlapping_spheres() {
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(0.5, 0.0, 0.0, sphere(1.0)) };

    let laws = vec![Law::hard("no_overlap", Constraint::NonOverlap { a, b })];

    let config = CheckConfig {
        aabb_min: Vec3::splat(-2.0),
        aabb_max: Vec3::splat(2.0),
        resolution: 16,
    };

    let report = check_laws(&laws, &config);
    assert!(
        report.has_hard_violations(),
        "重なる sphere は NonOverlap 違反を検出すべき"
    );
    assert!(!report.all_passed());
    assert_eq!(report.violations.len(), 1);
    assert!(report.violations[0].residual < 0.0, "残差は負（侵入深さ）");
}

/// 離れた 2 sphere → NonOverlap パス
#[test]
fn non_overlap_separated_spheres() {
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(5.0, 0.0, 0.0, sphere(1.0)) };

    let laws = vec![Law::hard("no_overlap", Constraint::NonOverlap { a, b })];

    let config = CheckConfig {
        aabb_min: Vec3::splat(-3.0),
        aabb_max: Vec3::new(8.0, 3.0, 3.0),
        resolution: 8,
    };

    let report = check_laws(&laws, &config);
    assert!(
        report.all_passed(),
        "離れた sphere は NonOverlap をパスすべき"
    );
}

/// 内包: 小 sphere が大 sphere の中 → Containment パス
#[test]
fn containment_inside() {
    let inner = lol! { sphere(0.5) };
    let outer = lol! { sphere(2.0) };

    let laws = vec![Law::hard(
        "contained",
        Constraint::Containment { inner, outer },
    )];

    let config = CheckConfig {
        aabb_min: Vec3::splat(-3.0),
        aabb_max: Vec3::splat(3.0),
        resolution: 8,
    };

    let report = check_laws(&laws, &config);
    assert!(report.all_passed(), "小 sphere は大 sphere の中にあるはず");
}

/// 内包: sphere がはみ出る → Containment 違反
#[test]
fn containment_overflow() {
    // translate(2.5, 0, 0, sphere(1.0)) → 中心 2.5 + 半径 1.0 = 3.5 まで到達
    // outer = sphere(2.0) → 半径 2.0 のみ
    let inner = lol! { translate(2.5, 0.0, 0.0, sphere(1.0)) };
    let outer = lol! { sphere(2.0) };

    let laws = vec![Law::hard(
        "contained",
        Constraint::Containment { inner, outer },
    )];

    let config = CheckConfig {
        aabb_min: Vec3::splat(-4.0),
        aabb_max: Vec3::splat(4.0),
        resolution: 16,
    };

    let report = check_laws(&laws, &config);
    assert!(
        report.has_hard_violations(),
        "はみ出た sphere は Containment 違反を検出すべき"
    );
}

/// ソフト制約: 違反は WARN として報告
#[test]
fn soft_constraint_reports_warn() {
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(0.5, 0.0, 0.0, sphere(1.0)) };

    let laws = vec![Law::soft(
        "soft_no_overlap",
        0.3,
        Constraint::NonOverlap { a, b },
    )];

    let config = CheckConfig {
        aabb_min: Vec3::splat(-2.0),
        aabb_max: Vec3::splat(2.0),
        resolution: 16,
    };

    let report = check_laws(&laws, &config);
    assert!(!report.all_passed());
    assert!(
        !report.has_hard_violations(),
        "ソフト制約はハード違反として報告されない"
    );
    assert_eq!(report.violations[0].priority, Priority::Soft(0.3));
}

/// MinThickness: 薄い box → 肉厚不足検出
#[test]
fn min_thickness_thin_object() {
    // box3d(2.0, 0.3, 2.0) → Y方向の半径 0.3（肉厚は表面距離で最大 0.3）
    // resolution=16, AABB [-2,2] → ステップ 0.25, セル中心 0.125 が内部に入る
    // SDF(0.125, 0.125, 0.125) = max(-1.875, -0.175, -1.875) = -0.175
    // |d| = 0.175 < min_thickness(0.5) → 違反
    let node = lol! { box3d(2.0, 0.3, 2.0) };

    let laws = vec![Law::hard(
        "min_wall",
        Constraint::MinThickness {
            node,
            min_thickness: 0.5,
        },
    )];

    let config = CheckConfig {
        aabb_min: Vec3::splat(-2.0),
        aabb_max: Vec3::splat(2.0),
        resolution: 16,
    };

    let report = check_laws(&laws, &config);
    assert!(
        report.has_hard_violations(),
        "Y方向の肉厚 0.3 は最小肉厚 0.5 を下回るため違反"
    );
}

/// MinThickness: 十分な肉厚 → パス
#[test]
fn min_thickness_thick_solid() {
    // sphere(2.0) を AABB [-1,1] でサンプル → 深い内部のみ
    // 最も表面に近い角 (0.875,0.875,0.875) で r≈1.516, SDF≈-0.484
    // |d| = 0.484 > min_thickness(0.1) → パス
    let node = lol! { sphere(2.0) };

    let laws = vec![Law::hard(
        "min_wall",
        Constraint::MinThickness {
            node,
            min_thickness: 0.1,
        },
    )];

    let config = CheckConfig {
        aabb_min: Vec3::splat(-1.0),
        aabb_max: Vec3::splat(1.0),
        resolution: 8,
    };

    let report = check_laws(&laws, &config);
    assert!(report.all_passed(), "十分な肉厚なのでパスすべき");
}

/// 複数法則の同時検証
#[test]
fn multiple_laws() {
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(0.5, 0.0, 0.0, sphere(1.0)) };
    let inner = lol! { sphere(0.5) };
    let outer = lol! { sphere(2.0) };

    let laws = vec![
        Law::hard(
            "no_overlap",
            Constraint::NonOverlap {
                a: a.clone(),
                b: b.clone(),
            },
        ),
        Law::hard("contained", Constraint::Containment { inner, outer }),
    ];

    let config = CheckConfig {
        aabb_min: Vec3::splat(-2.0),
        aabb_max: Vec3::splat(2.0),
        resolution: 16,
    };

    let report = check_laws(&laws, &config);
    // no_overlap は違反、contained はパス → 1 件の違反
    assert_eq!(report.violations.len(), 1);
    assert_eq!(report.violations[0].law_name, "no_overlap");
}

/// format_report の出力確認
#[test]
fn format_report_output() {
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(0.5, 0.0, 0.0, sphere(1.0)) };

    let laws = vec![Law::hard("no_overlap", Constraint::NonOverlap { a, b })];

    let config = CheckConfig {
        aabb_min: Vec3::splat(-2.0),
        aabb_max: Vec3::splat(2.0),
        resolution: 16,
    };

    let report = check_laws(&laws, &config);
    let text = alice_lol::law::format_report(&report);
    assert!(text.contains("ERROR"));
    assert!(text.contains("no_overlap"));
    assert!(text.contains("residual"));
}

/// 空の法則リスト → 全パス
#[test]
fn empty_laws() {
    let config = CheckConfig::default();
    let report = check_laws(&[], &config);
    assert!(report.all_passed());
    assert_eq!(report.total_laws, 0);
    assert_eq!(report.passed, 0);
}

/// デフォルト設定の確認
#[test]
fn default_config() {
    let config = CheckConfig::default();
    assert_eq!(config.resolution, 8);
    assert_eq!(config.aabb_min, Vec3::splat(-5.0));
    assert_eq!(config.aabb_max, Vec3::splat(5.0));
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// v1.0 Phase 1: LawSet ビルダー
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

use alice_lol::law::{hard_violations, soft_violations, top_violations, Contradiction, LawSet};

/// LawSet ビルダーで複数制約を一括検証
#[test]
fn lawset_builder_basic() {
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(0.5, 0.0, 0.0, sphere(1.0)) };
    let inner = lol! { sphere(0.5) };
    let outer = lol! { sphere(2.0) };

    let set = LawSet::new()
        .hard(
            "no_overlap",
            Constraint::NonOverlap {
                a: a.clone(),
                b: b.clone(),
            },
        )
        .hard("contained", Constraint::Containment { inner, outer });

    let config = CheckConfig {
        aabb_min: Vec3::splat(-2.0),
        aabb_max: Vec3::splat(2.0),
        resolution: 16,
    };

    let report = set.check(&config);
    assert_eq!(report.total_laws, 2);
    // no_overlap は違反、contained はパス
    assert_eq!(report.violations.len(), 1);
    assert_eq!(report.violations[0].law_name, "no_overlap");
}

/// LawSet ビルダーにソフト制約を混在
#[test]
fn lawset_mixed_hard_soft() {
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(0.5, 0.0, 0.0, sphere(1.0)) };

    let set = LawSet::new()
        .hard(
            "hard_no_overlap",
            Constraint::NonOverlap {
                a: a.clone(),
                b: b.clone(),
            },
        )
        .soft("soft_no_overlap", 0.5, Constraint::NonOverlap { a, b });

    let config = CheckConfig {
        aabb_min: Vec3::splat(-2.0),
        aabb_max: Vec3::splat(2.0),
        resolution: 16,
    };

    let report = set.check(&config);
    assert_eq!(report.violations.len(), 2);
    assert!(report.has_hard_violations());
}

/// LawSet: 空の法則セット
#[test]
fn lawset_empty() {
    let set = LawSet::new();
    let report = set.check(&CheckConfig::default());
    assert!(report.all_passed());
    assert_eq!(report.total_laws, 0);
}

/// LawSet: laws() でリスト参照
#[test]
fn lawset_laws_accessor() {
    let a = lol! { sphere(1.0) };
    let b = lol! { sphere(2.0) };

    let set = LawSet::new()
        .hard(
            "law1",
            Constraint::NonOverlap {
                a: a.clone(),
                b: b.clone(),
            },
        )
        .soft("law2", 0.3, Constraint::NonOverlap { a, b });

    assert_eq!(set.laws().len(), 2);
    assert_eq!(set.laws()[0].name, "law1");
    assert_eq!(set.laws()[1].name, "law2");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// v1.0 Phase 1: 静的矛盾検出
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// NonOverlap + Containment を同一ペアに適用 → 矛盾検出
#[test]
fn contradiction_non_overlap_and_containment() {
    let a = lol! { sphere(1.0) };
    let b = lol! { sphere(2.0) };

    let set = LawSet::new()
        .hard(
            "no_overlap",
            Constraint::NonOverlap {
                a: a.clone(),
                b: b.clone(),
            },
        )
        .hard("a_in_b", Constraint::Containment { inner: a, outer: b });

    let contradictions = set.detect_contradictions();
    assert_eq!(contradictions.len(), 1);
    assert_eq!(contradictions[0].law_a, "no_overlap");
    assert_eq!(contradictions[0].law_b, "a_in_b");
    assert!(contradictions[0].reason.contains("NonOverlap"));
    assert!(contradictions[0].reason.contains("Containment"));
}

/// 矛盾のないセット → 空リスト
#[test]
fn no_contradiction_different_nodes() {
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(5.0, 0.0, 0.0, sphere(1.0)) };
    let c = lol! { sphere(0.5) };
    let d = lol! { sphere(3.0) };

    let set = LawSet::new()
        .hard("no_overlap_ab", Constraint::NonOverlap { a, b })
        .hard("c_in_d", Constraint::Containment { inner: c, outer: d });

    let contradictions = set.detect_contradictions();
    assert!(
        contradictions.is_empty(),
        "異なるノードペアへの制約は矛盾しない"
    );
}

/// 同じ制約タイプの重複は矛盾とみなさない
#[test]
fn no_contradiction_same_type() {
    let a = lol! { sphere(1.0) };
    let b = lol! { sphere(2.0) };

    let set = LawSet::new()
        .hard(
            "overlap1",
            Constraint::NonOverlap {
                a: a.clone(),
                b: b.clone(),
            },
        )
        .soft("overlap2", 0.5, Constraint::NonOverlap { a, b });

    let contradictions = set.detect_contradictions();
    assert!(
        contradictions.is_empty(),
        "同タイプの制約の重複は矛盾ではない"
    );
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// v1.0 Phase 1: 残差フィルタリング
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// top_violations: 上位 N 件の取得
#[test]
fn top_violations_filter() {
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(0.5, 0.0, 0.0, sphere(1.0)) };
    let thin = lol! { box3d(2.0, 0.3, 2.0) };

    let set = LawSet::new()
        .hard(
            "no_overlap",
            Constraint::NonOverlap {
                a: a.clone(),
                b: b.clone(),
            },
        )
        .hard(
            "min_wall",
            Constraint::MinThickness {
                node: thin,
                min_thickness: 0.5,
            },
        );

    let config = CheckConfig {
        aabb_min: Vec3::splat(-2.0),
        aabb_max: Vec3::splat(2.0),
        resolution: 16,
    };

    let report = set.check(&config);
    assert_eq!(report.violations.len(), 2);

    // top 1 件
    let top1 = top_violations(&report, 1);
    assert_eq!(top1.len(), 1);

    // top 100 件（全件より多い）→ 全件返す
    let top_all = top_violations(&report, 100);
    assert_eq!(top_all.len(), 2);
}

/// hard_violations: ハード違反のみ抽出
#[test]
fn filter_hard_violations() {
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(0.5, 0.0, 0.0, sphere(1.0)) };

    let set = LawSet::new()
        .hard(
            "hard_rule",
            Constraint::NonOverlap {
                a: a.clone(),
                b: b.clone(),
            },
        )
        .soft("soft_rule", 0.3, Constraint::NonOverlap { a, b });

    let config = CheckConfig {
        aabb_min: Vec3::splat(-2.0),
        aabb_max: Vec3::splat(2.0),
        resolution: 16,
    };

    let report = set.check(&config);
    let hard = hard_violations(&report);
    let soft = soft_violations(&report);

    assert_eq!(hard.len(), 1);
    assert_eq!(hard[0].law_name, "hard_rule");
    assert_eq!(soft.len(), 1);
    assert_eq!(soft[0].law_name, "soft_rule");
}

/// soft_violations: ソフト違反のみ抽出
#[test]
fn filter_soft_violations_only() {
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(0.5, 0.0, 0.0, sphere(1.0)) };

    let set = LawSet::new().soft("soft_overlap", 0.7, Constraint::NonOverlap { a, b });

    let config = CheckConfig {
        aabb_min: Vec3::splat(-2.0),
        aabb_max: Vec3::splat(2.0),
        resolution: 16,
    };

    let report = set.check(&config);
    assert!(hard_violations(&report).is_empty());
    assert_eq!(soft_violations(&report).len(), 1);
}

/// 違反なしの場合のフィルタリング
#[test]
fn filter_no_violations() {
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(5.0, 0.0, 0.0, sphere(1.0)) };

    let set = LawSet::new().hard("separated", Constraint::NonOverlap { a, b });

    let config = CheckConfig {
        aabb_min: Vec3::splat(-3.0),
        aabb_max: Vec3::new(8.0, 3.0, 3.0),
        resolution: 8,
    };

    let report = set.check(&config);
    assert!(top_violations(&report, 5).is_empty());
    assert!(hard_violations(&report).is_empty());
    assert!(soft_violations(&report).is_empty());
}
