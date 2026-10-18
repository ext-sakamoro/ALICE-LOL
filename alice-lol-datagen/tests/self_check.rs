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

/// ALICE-Law G8: every record carries the identifiers of the meaning that produced it (the LOL text and
/// the geometry its oracle was judged in), as the lower-case hex of the crate's constants
#[test]
fn every_record_carries_the_lol_identifiers() {
    fn hex(id: &[u8; 32]) -> String {
        // written out without fmt (the crate's own helper uses write!), so a shared formatting slip is not
        // compared with itself
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        id.iter()
            .flat_map(|b| [DIGITS[usize::from(b >> 4)], DIGITS[usize::from(b & 15)]])
            .map(char::from)
            .collect()
    }
    let head = format!(
        "{{\"lol_semantics_id\":\"{}\",\"lol_geometry_semantics_id\":\"{}\",\"id\":",
        hex(&alice_lol::law_id::LOL_SEMANTICS_ID),
        hex(&alice_lol::geometry_id::LOL_GEOMETRY_SEMANTICS_ID)
    );
    assert_ne!(
        alice_lol::law_id::LOL_SEMANTICS_ID,
        alice_lol::geometry_id::LOL_GEOMETRY_SEMANTICS_ID
    );
    with_big_stack(move || {
        let (samples, _) = generate(ALL_FAMILIES, 16, 7);
        assert!(!samples.is_empty());
        for s in &samples {
            let j = s.to_jsonl();
            assert!(j.starts_with(&head), "{j}");
        }
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
