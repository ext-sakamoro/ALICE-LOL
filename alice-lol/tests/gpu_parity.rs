//! Backend parity Level 2: LOL → `SdfNode` → WGSL を **実 GPU で実行** して
//! CPU `eval` と突合する (Milestone A.4.1、2026-09-17)
//!
//! `alice_lol::to_wgsl` は `WgslShader::transpile(node, Hardcoded).source` の
//! 薄い wrapper なので、law の GPU parity 自体は alice-sdf 側の `gpu-parity`
//! job (`tests/test_gpu_law_parity.rs`、corpus 144 node) が持つ 本 test が
//! 足すのは **LOL の grammar corpus と深い合成 fixture** — grammar から到達
//! できる node の組合せ (product の subtract 連鎖 / stdlib helper 展開) が
//! GPU 上でも CPU と同じ場になることを、LOL 側の入口 (`parse_lol`) から確認する
//!
//! GPU adapter が無い環境では skip、CI の `gpu-parity` job は lavapipe
//! (Mesa software Vulkan) + `ALICE_SDF_REQUIRE_GPU=1` で skip を fail にする

#![cfg(feature = "wgsl")]

mod common;

use alice_lol::runtime_parser::parse_lol;
use alice_lol::{eval, SdfNode};
use alice_sdf::compiled::{GpuError, GpuEvaluator};
use common::corpus::{fixtures, grammar_corpus, sample_points};

fn require_gpu() -> bool {
    std::env::var_os("ALICE_SDF_REQUIRE_GPU").is_some()
}

/// GPU 実行と CPU 評価の最大相対差 (|c| < 1 では絶対差)
///
/// `Err` = GPU pipeline 構築失敗 (adapter なし or shader compile 失敗)
fn gpu_cpu_drift(
    node: &SdfNode,
    pts: &[glam::Vec3],
) -> Result<(f32, glam::Vec3, f32, f32), GpuError> {
    let gpu = GpuEvaluator::new(node)?;
    let got = gpu.eval_batch(pts)?;
    let mut worst = (0.0_f32, glam::Vec3::ZERO, 0.0_f32, 0.0_f32);
    for (p, g) in pts.iter().zip(&got) {
        let c = eval(node, *p);
        if c.is_nan() && g.is_nan() {
            continue;
        }
        // 両方 ±∞ (INFINITY を返す law) は一致扱い、片方だけなら drift
        if c.is_infinite() && g.is_infinite() && c.signum() == g.signum() {
            continue;
        }
        let diff = (g - c).abs() / c.abs().max(1.0);
        if diff.is_nan() || diff > worst.0 {
            worst = (if diff.is_nan() { f32::INFINITY } else { diff }, *p, c, *g);
        }
    }
    Ok(worst)
}

/// GPU の超越関数 (atan2 / sin / pow) は libm と最終 ulp が違うので、
/// 構文ごとの許容差 既定 1e-4、周期 / 角度依存の law は緩める
fn tolerance(name: &str) -> f32 {
    match name {
        // 極座標 tie / 周期関数の位相 (SDF 側 gpu-parity と同じ根拠)
        "polar_repeat"
        | "twist"
        | "bend"
        | "gyroid"
        | "schwarz_p"
        | "diamond_surface"
        | "neovius"
        | "lidinoid"
        | "iwp"
        | "frd"
        | "fischer_koch_s"
        | "pmy"
        | "noise"
        | "displacement"
        | "surface_roughness"
        | "lattice_infill"
        | "diamond_infill"
        | "schwarz_infill"
        | "helix"
        | "icosahedral_symmetry"
        | "octant_mirror" => 5e-3,
        _ => 1e-4,
    }
}

#[test]
fn every_grammar_construct_matches_cpu_on_gpu() {
    let corpus = grammar_corpus();
    let pts = sample_points(512, 2.5);
    let mut checked = 0usize;
    let mut failures = Vec::new();
    let mut skipped_no_gpu = false;

    for (name, src) in &corpus {
        let node = parse_lol(src).unwrap_or_else(|e| panic!("{name}: parse failed: {e} ({src})"));
        // wgpu は shader の validation error を Err でなく panic で報告するので、
        // 構文名を付けて failure に変換する (1 件で test 全体が止まらないように)
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| gpu_cpu_drift(&node, &pts)));
        let outcome = match outcome {
            Ok(o) => o,
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
                    .unwrap_or_else(|| "non-string panic".to_string());
                failures.push(format!("{name}: GPU pipeline panicked: {msg} src=`{src}`"));
                continue;
            }
        };
        match outcome {
            Ok((drift, p, c, g)) => {
                if drift > tolerance(name) {
                    failures.push(format!(
                        "{name}: GPU/CPU drift {drift:.3e} at {p:?} (cpu={c} gpu={g}) src=`{src}`"
                    ));
                }
                checked += 1;
            }
            Err(e @ GpuError::NoAdapter) if !require_gpu() && checked == 0 => {
                eprintln!("skipping GPU parity (no adapter): {e}");
                skipped_no_gpu = true;
                break;
            }
            Err(e) => failures.push(format!("{name}: GPU pipeline failed: {e} src=`{src}`")),
        }
    }

    if skipped_no_gpu {
        return;
    }
    assert!(
        failures.is_empty(),
        "{}\n({} of {} constructs checked)",
        failures.join("\n"),
        checked,
        corpus.len()
    );
    // 257 parser construct − Intent verb 18 − program/entities 2 = 237 (2026-09-14)
    assert!(checked >= 230, "only {checked} constructs ran on the GPU");
    eprintln!("GPU parity: {checked} grammar constructs within tolerance");
}

#[test]
fn fixtures_match_cpu_on_gpu() {
    let pts = sample_points(512, 40.0);
    let mut failures = Vec::new();
    for (name, src) in fixtures() {
        let node = parse_lol(src).unwrap_or_else(|e| panic!("{name}: parse failed: {e}"));
        match gpu_cpu_drift(&node, &pts) {
            Ok((drift, p, c, g)) => {
                if drift > 1e-3 {
                    failures.push(format!(
                        "{name}: GPU/CPU drift {drift:.3e} at {p:?} (cpu={c} gpu={g})"
                    ));
                } else {
                    eprintln!("{name}: max drift {drift:.3e}");
                }
            }
            Err(e @ GpuError::NoAdapter) if !require_gpu() => {
                eprintln!("skipping GPU parity (no adapter): {e}");
                return;
            }
            Err(e) => failures.push(format!("{name}: GPU pipeline failed: {e}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
