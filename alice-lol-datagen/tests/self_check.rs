//! 全 family × 決定論 × 自己検証 (rejected 0) + JSONL 形式

use alice_lol_datagen::{generate, Family, Rng, Sample, ALL_FAMILIES};

/// parse / emit は alice-lol 側の stacker で深い tree (product の subtract 2,400
/// 連鎖) に耐えるが、`Arc<SdfNode>` の **再帰 Drop** は test thread の既定 2 MB では
/// 足りない (main thread 8 MB の bin は素通り) → 32 MB thread で回す
/// (alice-sdf 側の iterative Drop は follow-up)
fn with_big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn every_family_generates_without_rejections() {
    with_big_stack(|| {
        for &fam in ALL_FAMILIES {
            let (samples, rejected) = generate(&[fam], 300, 1234);
            assert!(
                rejected.is_empty(),
                "{}: {} rejected, first: {}",
                fam.name(),
                rejected.len(),
                rejected[0].1
            );
            assert_eq!(samples.len(), 300, "{}", fam.name());
            for s in &samples {
                assert_eq!(s.family, fam.name());
                assert!(!s.caption_en.is_empty() && !s.caption_ja.is_empty());
                assert!(!s.lol.contains("//") && !s.lol.contains("  "), "{}", s.lol);
                assert!(s.id.starts_with(fam.name()));
            }
        }
    });
}

#[test]
fn deterministic_for_same_seed() {
    with_big_stack(|| {
        let (a, _) = generate(ALL_FAMILIES, 64, 99);
        let (b, _) = generate(ALL_FAMILIES, 64, 99);
        let ja: Vec<String> = a.iter().map(Sample::to_jsonl).collect();
        let jb: Vec<String> = b.iter().map(Sample::to_jsonl).collect();
        assert_eq!(ja, jb);
        let (c, _) = generate(ALL_FAMILIES, 64, 100);
        assert_ne!(ja, c.iter().map(Sample::to_jsonl).collect::<Vec<_>>());
    });
}

#[test]
fn jsonl_has_expected_fields_and_escapes() {
    with_big_stack(|| {
        let s = Family::IntentProgram.generate(&mut Rng::new(5));
        let j = s.to_jsonl();
        for key in [
            "\"id\":",
            "\"family\":\"intent_program\"",
            "\"caption_en\":",
            "\"caption_ja\":",
            "\"lol\":\"program(",
            "\"lol_canonical\":\"program(",
            "\"oracle\":[",
            "\"intent_verbs\":[",
        ] {
            assert!(j.contains(key), "missing {key} in {j}");
        }
        assert!(!j.contains('\n'));
        assert!(j.starts_with('{') && j.ends_with('}'));
    });
}

#[test]
fn intent_samples_carry_verbs_and_entities() {
    with_big_stack(|| {
        let (samples, _) = generate(&[Family::IntentProgram], 50, 3);
        for s in samples {
            assert!(s.intent_verbs.len() >= 3, "{:?}", s.intent_verbs);
            assert!(matches!(s.intent_verbs[0].as_str(), "seq" | "par"));
            assert!(s.lol.contains("entities("));
        }
    });
}

#[test]
fn product_shortcut_keeps_shortcut_in_lol_but_expands_canonical() {
    with_big_stack(|| {
        let (samples, _) = generate(&[Family::ProductShortcut], 20, 8);
        for s in samples {
            assert!(
                !s.lol.contains("union("),
                "lol should be the shortcut call: {}",
                s.lol
            );
            assert!(s.lol_canonical.len() > s.lol.len());
        }
    });
}
