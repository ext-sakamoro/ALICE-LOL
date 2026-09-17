//! Backend parity Level 1.5: LOL grammar corpus 全構文 + fixture の transpile 出力を
//! naga で parse + validate する (GPU 不要、CI の通常 matrix で走る)
//!
//! `backend_parity.rs` (Level 1) は「panic せず keyword が出る」まで、本 test は
//! 「shader として文法的 / 型的に成立する」まで、`gpu_parity.rs` (Level 2) は
//! 「実行結果が CPU と一致する」まで 2026-09-17 の初回実行で SDF 3.0.0 の
//! Terrain が WGSL / HLSL に GLSL 構文 (`float` / `vec2` / `for(int`) を直書き
//! していたのを GPU 無しで検出できる位置

mod common;

use alice_lol::runtime_parser::parse_lol;
use common::corpus::{fixtures, grammar_corpus};
use naga::valid::{Capabilities, ValidationFlags, Validator};

fn validate(module: &naga::Module) -> Result<(), String> {
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(module)
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}

/// corpus + fixture の (名前, source) 列
fn all_sources() -> Vec<(String, String)> {
    let mut out = grammar_corpus();
    out.extend(
        fixtures()
            .into_iter()
            .map(|(n, s)| (n.to_string(), s.to_string())),
    );
    out
}

#[cfg(feature = "wgsl")]
#[test]
fn every_lol_construct_transpiles_to_valid_wgsl() {
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for (name, src) in all_sources() {
        let node = parse_lol(&src).unwrap_or_else(|e| panic!("{name}: parse failed: {e}"));
        let source = alice_lol::to_wgsl(&node);
        let module = match naga::front::wgsl::parse_str(&source) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!(
                    "{name}: WGSL parse failed:\n{}",
                    e.emit_to_string(&source)
                ));
                continue;
            }
        };
        if let Err(e) = validate(&module) {
            failures.push(format!("{name}: WGSL validation failed: {e}"));
            continue;
        }
        checked += 1;
    }
    assert!(checked >= 230, "only {checked} constructs validated");
    assert!(
        failures.is_empty(),
        "{} invalid WGSL shaders:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[cfg(feature = "glsl")]
#[test]
fn every_lol_construct_transpiles_to_valid_glsl() {
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for (name, src) in all_sources() {
        let node = parse_lol(&src).unwrap_or_else(|e| panic!("{name}: parse failed: {e}"));
        let mut frontend = naga::front::glsl::Frontend::default();
        let options = naga::front::glsl::Options {
            stage: naga::ShaderStage::Fragment,
            defines: naga::FastHashMap::default(),
        };
        // transpiler は `sdf_eval` 関数群を出すので、最小の fragment entry で包む
        let source = format!(
            "{}\nout vec4 alice_frag;\nvoid main() {{ alice_frag = vec4(sdf_eval(vec3(0.1, 0.2, 0.3))); }}\n",
            alice_lol::to_glsl(&node)
        );
        let module = match frontend.parse(&options, &source) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("{name}: GLSL parse failed: {e:?}"));
                continue;
            }
        };
        if let Err(e) = validate(&module) {
            failures.push(format!("{name}: GLSL validation failed: {e}"));
            continue;
        }
        checked += 1;
    }
    assert!(checked >= 230, "only {checked} constructs validated");
    assert!(
        failures.is_empty(),
        "{} invalid GLSL shaders:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
