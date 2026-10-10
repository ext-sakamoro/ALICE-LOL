//! LOL の数は有限: parser は有限の literal だけを読み、emit は有限の数だけを書く
//!
//! - 桁あふれの literal (`1e39`) は ±∞ に読まれていた parser はそれを受け、emit は `inf` と
//!   書き、書いた text は読み戻せなかった (fuzz `fuzz_lol_emit_parity` の crash 入力)
//! - stdlib の構成で有限の引数から ±∞ が出ることもある emit は NaN / ±∞ を見たら
//!   [`EmitError::NonFinite`] を返し、書かない
//! - 2 つの書き方を選ぶ判定 (`capsule` / `capsule_ab`、可変長 op の平坦化) は書く値
//!   (`|v| < 1e-6` を 0 に丸めた値) で行う 丸める前の値で判定すると、読み戻した時に別の形になる
//! - fuzz `fuzz_lol_parse` / `fuzz_lol_eval` の crash 入力 (`skadis_panel` の負の一辺) は
//!   panic でなく parse error
//!
//! 性質: 生成した入力 x ごとに `parse(x)` が Ok なら、`to_lol` は `NonFinite` か text t を返し、
//! t は `inf` / `NaN` を含まず、`parse(t)` が Ok で `to_lol(parse(t)) == t`

use alice_lol::emit::{to_lol, EmitError};
use alice_lol::runtime_parser::parse_lol;
use alice_lol::stdlib::hardsurface::pattern_sdf::{try_gridfinity_bin, GridfinitySpec};
use alice_lol::stdlib::hardsurface::skadis_sdf::skadis_panel_sdf;
use alice_lol::{SdfNode, Vec3};
use std::sync::Arc;

mod common;
use common::corpus::grammar_corpus;

/// the crash inputs of the fuzz targets (CI run 38050442101), as the bytes the fuzzer wrote
const CRASH_PARSE: &str = "skadis_panel(-130,4,4031)1c";
const CRASH_EVAL: &str = "\x0cskadis_panel(-2,3\r)helf_di";
const CRASH_EMIT: &str =
    "boss(0,022222222222222222222222222222222222222222222222222222222222222222222)\n\r      \n";

#[test]
fn the_crash_inputs_are_errors_not_panics() {
    for src in [CRASH_PARSE, CRASH_EVAL, CRASH_EMIT] {
        assert!(parse_lol(src).is_err(), "{src:?}");
    }
}

#[test]
fn a_literal_past_the_f32_range_is_refused() {
    for src in [
        "sphere(1e39)",
        "sphere(-1e39)",
        "sphere(3.5e38)",
        "sphere(1e400)",
    ] {
        let e = parse_lol(src).expect_err(src);
        assert!(e.to_string().contains("out of range"), "{src}: {e}");
    }
    // the largest finite f32 and a value that underflows to 0 are numbers
    for src in [
        "sphere(3.4028235e38)",
        "sphere(-3.4028235e38)",
        "sphere(1e-50)",
    ] {
        assert!(parse_lol(src).is_ok(), "{src}");
    }
}

#[test]
fn a_skadis_panel_side_must_be_greater_than_zero() {
    for src in [
        "skadis_panel(-130, 4, 4031)",
        "skadis_panel(0)",
        "skadis_panel(-0.5)",
        "skadis_panel(2000.5)",
    ] {
        assert!(parse_lol(src).is_err(), "{src}");
    }
    for src in [
        "skadis_panel(0.001)",
        "skadis_panel(2000)",
        "skadis_panel()",
    ] {
        assert!(parse_lol(src).is_ok(), "{src}");
    }
    // 2026-10-10: the public infallible builder's contract changed again. It no
    // longer silently accepts a side of 0 or less (the corner-radius clamp fix
    // only ever stopped THAT panic, not a degenerate size reaching the
    // connector-hole loop): it now validates and panics with a documented
    // message, matching `gridfinity_bin`'s precedent. Untrusted input must go
    // through `try_skadis_panel_sdf` (asserted not to panic, just above, via
    // `parse_lol`); the direct infallible builder is for callers who already
    // know their size is in range.
    for size in [-130.0, 0.0, -0.0] {
        let result = std::panic::catch_unwind(|| skadis_panel_sdf(size, 4.0, 4031.0));
        assert!(result.is_err(), "skadis_panel_sdf({size}, ..) should panic");
    }
}

/// 2026-10-10 real regression: `gridfinity_bin_ex` bounded each divider axis
/// independently (`count_trunc`, `MAX_STDLIB_COUNT` = 1024) but never their
/// *product*, so `gridfinity_bin_ex(1,1,1,101,100,0,0)` (10,100 dividers,
/// each axis within the per-axis bound) was accepted before this fix (~6MB)
/// and PANICKED once the product-level `MAX_NODE_EXPANSION` check was added
/// to the infallible `gridfinity_bin` without also updating this parser call
/// site to the fallible `try_gridfinity_bin`. User `.lol` text must never
/// reach a `.expect()` panic inside a stdlib builder; this is the regression
/// test for that class, not just this one keyword.
#[test]
fn gridfinity_bin_ex_dividers_product_is_a_parse_error_not_a_panic() {
    for src in [
        "gridfinity_bin_ex(1,1,1,101,100,0,0)", // 10,100: over the limit, was Ok before this fix
        "gridfinity_bin_ex(1,1,1,1024,1024,0,0)", // 1,048,576: far over, was Ok (474MB) before this fix
    ] {
        assert!(parse_lol(src).is_err(), "{src}");
    }
    for src in [
        "gridfinity_bin_ex(1,1,1,100,100,0,0)", // 10,000: exactly at the limit
        "gridfinity_bin_ex(1,1,1,2,2,0,0)",     // a realistic request
    ] {
        assert!(parse_lol(src).is_ok(), "{src}");
    }
}

/// Same boundary, through the public `try_*` entry point directly (the
/// non-`.lol` Rust API surface), independent of the parser.
#[test]
fn try_gridfinity_bin_dividers_product_is_an_error_not_a_panic() {
    let over = GridfinitySpec {
        dividers: Some((101, 100)),
        ..GridfinitySpec::default_2x2()
    };
    assert!(try_gridfinity_bin(&over).is_err());
    let way_over = GridfinitySpec {
        dividers: Some((1024, 1024)),
        ..GridfinitySpec::default_2x2()
    };
    assert!(try_gridfinity_bin(&way_over).is_err());
    let at_limit = GridfinitySpec {
        dividers: Some((100, 100)),
        ..GridfinitySpec::default_2x2()
    };
    assert!(try_gridfinity_bin(&at_limit).is_ok());
}

#[test]
fn emit_refuses_a_number_that_is_not_finite() {
    let s = |r: f32| SdfNode::Sphere { radius: r };
    for r in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
        assert_eq!(to_lol(&s(r)), Err(EmitError::NonFinite), "{r}");
        // inside a child and in a vector
        let t = SdfNode::Translate {
            child: Arc::new(s(1.0)),
            offset: Vec3::new(0.0, r, 0.0),
        };
        assert_eq!(to_lol(&t), Err(EmitError::NonFinite), "{r}");
        // the numeric argument of a variadic op
        let u = SdfNode::SmoothUnion {
            a: Arc::new(s(1.0)),
            b: Arc::new(s(2.0)),
            k: r,
        };
        assert_eq!(to_lol(&u), Err(EmitError::NonFinite), "{r}");
    }
    assert!(to_lol(&s(f32::MAX)).is_ok());
}

#[test]
fn the_form_is_chosen_on_the_values_as_written() {
    for src in [
        "capsule_ab(1e-30, 1e-30, 1e-30, 1e-30, 1e-30, 1e-30, 1e-30)",
        "capsule_ab(5e-7, -1.0, 0.0, -5e-7, 1.0, 0.0, 0.5)",
        "smooth_union(1e-30, smooth_union(2e-30, sphere(1.0), sphere(2.0)), sphere(3.0))",
        "smooth_union(5e-7, smooth_union(-5e-7, sphere(1.0), sphere(2.0)), sphere(3.0))",
    ] {
        let t = to_lol(&parse_lol(src).unwrap()).unwrap();
        let t2 = to_lol(&parse_lol(&t).unwrap()).unwrap();
        assert_eq!(t, t2, "{src}");
    }
}

/// every numeric literal of `src` replaced, the k-th by `values[(k + offset) % len]`
fn with_numbers(src: &str, values: &[&str], offset: usize) -> String {
    let bytes = src.as_bytes();
    let (mut out, mut at, mut nth) = (String::new(), 0, 0);
    while at < bytes.len() {
        let byte = bytes[at];
        let after_ident =
            at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        let starts = byte.is_ascii_digit()
            || (byte == b'-' && bytes.get(at + 1).is_some_and(u8::is_ascii_digit));
        if starts && !after_ident {
            let mut end = at + 1;
            while end < bytes.len()
                && (bytes[end].is_ascii_digit()
                    || matches!(bytes[end], b'.' | b'e' | b'E')
                    || (matches!(bytes[end], b'-' | b'+') && matches!(bytes[end - 1], b'e' | b'E')))
            {
                end += 1;
            }
            out.push_str(values[(nth + offset) % values.len()]);
            nth += 1;
            at = end;
        } else {
            out.push(char::from(byte));
            at += 1;
        }
    }
    out
}

fn has_non_finite_token(text: &str) -> bool {
    text.split(|c: char| matches!(c, '(' | ')' | ',') || c.is_whitespace())
        .any(|t| {
            matches!(
                t.trim_start_matches('-').to_ascii_lowercase().as_str(),
                "inf" | "nan"
            )
        })
}

#[test]
fn what_parses_is_written_finite_and_reads_back_to_the_same_text() {
    const VALUES: [&str; 13] = [
        "0.0", "-1.0", "1e-30", "1e30", "-1e30", "3.0e38", "-3.0e38", "5e-7", "-5e-7", "1e-6",
        "0.5", "1e20", "7.0",
    ];
    let (mut parsed, mut refused, mut checked, mut failures) = (0usize, 0usize, 0usize, Vec::new());
    for (name, src) in grammar_corpus() {
        // every value in every literal position (offset 0..len), and the values mixed
        for offset in 0..VALUES.len() {
            for values in [&VALUES[offset..=offset], &VALUES[..]] {
                let x = with_numbers(&src, values, offset);
                let Ok(n) = parse_lol(&x) else { continue };
                parsed += 1;
                match to_lol(&n) {
                    Err(EmitError::NonFinite) => refused += 1,
                    Err(e) => failures.push(format!("{name}: {x}: emit: {e}")),
                    Ok(t) => {
                        if has_non_finite_token(&t) {
                            failures.push(format!("{name}: {x}: wrote a non-finite number: {t}"));
                        }
                        match parse_lol(&t).map(|n2| to_lol(&n2)) {
                            Ok(Ok(t2)) if t2 == t => checked += 1,
                            other => failures.push(format!("{name}: {x} -> {t} -> {other:?}")),
                        }
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // the corpus has 230+ constructs: the floor keeps the property from passing on nothing
    assert!(
        checked >= 4000,
        "only {checked} inputs round-tripped (parsed {parsed}, refused {refused})"
    );
    assert!(
        refused > 0,
        "no input reached the non-finite refusal: the large values no longer overflow"
    );
}
