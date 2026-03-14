//! ALICE-LOL 法則（Law）制約チェッカー デモ
//!
//! SDF シーンに物理的・幾何学的制約を宣言し、違反を検出する。

use alice_lol::law::{check_laws, format_report, CheckConfig, Constraint, Law};
use alice_lol::{lol, Vec3};

fn main() {
    println!("=== ALICE-LOL 法則（Law）制約チェッカー ===\n");

    let config = CheckConfig {
        aabb_min: Vec3::splat(-4.0),
        aabb_max: Vec3::splat(4.0),
        resolution: 8,
    };

    // ── シナリオ 1: 重なるオブジェクト（違反） ──
    println!("--- シナリオ 1: 重なる 2 sphere ---");
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(0.5, 0.0, 0.0, sphere(1.0)) };

    let laws = vec![Law::hard(
        "CollisionAvoidance",
        Constraint::NonOverlap { a, b },
    )];

    let report = check_laws(&laws, &config);
    print!("{}", format_report(&report));

    // ── シナリオ 2: 離れたオブジェクト（パス） ──
    println!("\n--- シナリオ 2: 離れた 2 sphere ---");
    let a = lol! { sphere(1.0) };
    let b = lol! { translate(5.0, 0.0, 0.0, sphere(1.0)) };

    let laws = vec![Law::hard(
        "CollisionAvoidance",
        Constraint::NonOverlap { a, b },
    )];

    let report = check_laws(&laws, &config);
    print!("{}", format_report(&report));

    // ── シナリオ 3: 内包チェック（はみ出し） ──
    println!("\n--- シナリオ 3: sphere がケースからはみ出し ---");
    let inner = lol! { translate(2.0, 0.0, 0.0, sphere(1.0)) };
    let outer = lol! { sphere(2.0) };

    let laws = vec![Law::hard(
        "FitsInCase",
        Constraint::Containment { inner, outer },
    )];

    let report = check_laws(&laws, &config);
    print!("{}", format_report(&report));

    // ── シナリオ 4: 最小肉厚チェック ──
    println!("\n--- シナリオ 4: 薄い shell の肉厚チェック ---");
    let thin_shell = lol! { onion(0.03, sphere(1.0)) };

    let laws = vec![Law::hard(
        "3DPrint_MinWall",
        Constraint::MinThickness {
            node: thin_shell,
            min_thickness: 0.1,
        },
    )];

    let report = check_laws(&laws, &config);
    print!("{}", format_report(&report));

    // ── シナリオ 5: 複合法則（ハード + ソフト） ──
    println!("\n--- シナリオ 5: 複合法則 ---");
    let part_a = lol! { sphere(1.0) };
    let part_b = lol! { translate(1.5, 0.0, 0.0, sphere(0.8)) };
    let enclosure = lol! { sphere(3.0) };

    let laws = vec![
        Law::hard(
            "NoCollision",
            Constraint::NonOverlap {
                a: part_a.clone(),
                b: part_b.clone(),
            },
        ),
        Law::hard(
            "FitsInEnclosure_A",
            Constraint::Containment {
                inner: part_a,
                outer: enclosure.clone(),
            },
        ),
        Law::hard(
            "FitsInEnclosure_B",
            Constraint::Containment {
                inner: part_b,
                outer: enclosure,
            },
        ),
    ];

    let report = check_laws(&laws, &config);
    print!("{}", format_report(&report));

    // ── シナリオ 6: LawSet ビルダー + 残差フィルタリング ──
    println!("\n--- シナリオ 6: LawSet ビルダー + 残差フィルタ ---");
    let s1 = lol! { sphere(1.0) };
    let s2 = lol! { translate(0.5, 0.0, 0.0, sphere(1.0)) };
    let thin = lol! { box3d(2.0, 0.3, 2.0) };

    let set = alice_lol::law::LawSet::new()
        .hard(
            "NoCollision",
            Constraint::NonOverlap {
                a: s1.clone(),
                b: s2.clone(),
            },
        )
        .soft("SoftOverlap", 0.3, Constraint::NonOverlap { a: s1, b: s2 })
        .hard(
            "MinWall",
            Constraint::MinThickness {
                node: thin,
                min_thickness: 0.5,
            },
        );

    let report = set.check(&config);
    print!("{}", format_report(&report));

    let hard = alice_lol::law::hard_violations(&report);
    let soft = alice_lol::law::soft_violations(&report);
    println!("  ハード違反: {} 件", hard.len());
    println!("  ソフト違反: {} 件", soft.len());

    let top2 = alice_lol::law::top_violations(&report, 2);
    println!("  残差トップ 2:");
    for v in &top2 {
        println!("    {}: residual={:.4}", v.law_name, v.residual);
    }

    // ── シナリオ 7: 静的矛盾検出 ──
    println!("\n--- シナリオ 7: 静的矛盾検出 ---");
    let x = lol! { sphere(1.0) };
    let y = lol! { sphere(3.0) };

    let contradictory_set = alice_lol::law::LawSet::new()
        .hard(
            "no_overlap_xy",
            Constraint::NonOverlap {
                a: x.clone(),
                b: y.clone(),
            },
        )
        .hard("x_inside_y", Constraint::Containment { inner: x, outer: y });

    let contradictions = contradictory_set.detect_contradictions();
    if contradictions.is_empty() {
        println!("  矛盾なし");
    } else {
        for c in &contradictions {
            println!("  矛盾検出: {} vs {} — {}", c.law_a, c.law_b, c.reason);
        }
    }

    println!("\n法則デモ完了。");
}
