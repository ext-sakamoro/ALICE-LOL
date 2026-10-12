//! Cargo.lock から `alice-det-math` の実解決 version を読み、`rustc --version`
//! を実行して自分を compile している rustc の version も取り、
//! `alice_lol::angle` の cross-architecture oracle が記録済みの両 version と
//! 一致するかを `env!("ALICE_DET_MATH_LOCKED_VERSION")` /
//! `env!("RUSTC_LOCKED_VERSION")` 経由で test 時に突合できるようにする
//! (超越関数の instruction selection は LLVM の version = rustc の version に
//! 依存するため、det-math だけでなく rustc の version も変わると table が
//! 黙って stale になりうる)
//!
//! 読めなかった / 見つからなかった場合は **build 自体を fail させる**
//! (silent に空文字を emit して「比較 0 件のまま green」にしない)

// if/else-if chain with a panic branch reads top-to-bottom as the 3 cases it
// is (disambiguated / unambiguous single version / ambiguous and unreadable);
// `map_or_else` would nest a panic inside a closure, which is harder to read
// here, not easier
#![allow(clippy::option_if_let_else)]

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let lock_path = manifest_dir.join("..").join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock_path.display());

    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let rustc_version = Command::new(&rustc)
        .arg("--version")
        .output()
        .unwrap_or_else(|e| panic!("`{rustc} --version` を実行できない: {e}"));
    assert!(
        rustc_version.status.success(),
        "`{rustc} --version` が失敗 (status {:?}): {}",
        rustc_version.status,
        String::from_utf8_lossy(&rustc_version.stderr)
    );
    let rustc_version = String::from_utf8(rustc_version.stdout)
        .unwrap_or_else(|e| panic!("`{rustc} --version` の出力が UTF-8 でない: {e}"))
        .trim()
        .to_string();
    assert!(
        !rustc_version.is_empty(),
        "`{rustc} --version` が空文字を返した"
    );
    println!("cargo:rustc-env=RUSTC_LOCKED_VERSION={rustc_version}");

    let text = std::fs::read_to_string(&lock_path)
        .unwrap_or_else(|e| panic!("Cargo.lock を読めない ({}): {e}", lock_path.display()));

    let blocks = parse_packages(&text);

    let alice_lol = blocks
        .iter()
        .find(|b| b.name == "alice-lol")
        .unwrap_or_else(|| panic!("Cargo.lock に [[package]] name = \"alice-lol\" が無い"));

    // alice-lol 自身の dependencies 行は、同名 crate が複数 version linked されている時だけ
    // `"alice-det-math 0.5.0"` の形に disambiguate される (cargo の既定の表記)
    let det_math_versions: Vec<&str> = blocks
        .iter()
        .filter(|b| b.name == "alice-det-math")
        .map(|b| b.version.as_str())
        .collect();

    let resolved = if let Some(dep) = alice_lol
        .dependencies
        .iter()
        .find(|d| d.starts_with("alice-det-math"))
    {
        if let Some(v) = dep.strip_prefix("alice-det-math ") {
            v.to_string()
        } else if det_math_versions.len() == 1 {
            det_math_versions[0].to_string()
        } else {
            panic!(
                "alice-lol の依存行が 'alice-det-math' だけ (version 無し) なのに \
                 Cargo.lock に alice-det-math が {} 個 linked されている ({:?}) \
                 — disambiguate されているはずなのに読めていない",
                det_math_versions.len(),
                det_math_versions
            );
        }
    } else {
        panic!("alice-lol の dependencies に alice-det-math が無い");
    };

    println!("cargo:rustc-env=ALICE_DET_MATH_LOCKED_VERSION={resolved}");
}

struct Package {
    name: String,
    version: String,
    dependencies: Vec<String>,
}

/// `Cargo.lock` の `[[package]]` block を最小限だけ読む
/// (TOML parser を build-dependency に足さず、この file の形だけを前提にする)
fn parse_packages(text: &str) -> Vec<Package> {
    let mut packages = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim() != "[[package]]" {
            continue;
        }
        let mut name = String::new();
        let mut version = String::new();
        let mut dependencies = Vec::new();
        while let Some(&next) = lines.peek() {
            let t = next.trim();
            if t == "[[package]]" || t.starts_with("[[") {
                break;
            }
            lines.next();
            if let Some(v) = t
                .strip_prefix("name = \"")
                .and_then(|s| s.strip_suffix('"'))
            {
                name = v.to_string();
            } else if let Some(v) = t
                .strip_prefix("version = \"")
                .and_then(|s| s.strip_suffix('"'))
            {
                version = v.to_string();
            } else if t == "dependencies = [" {
                for dep_line in lines.by_ref() {
                    let dt = dep_line.trim().trim_end_matches(',');
                    if dt == "]" {
                        break;
                    }
                    if let Some(v) = dt.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                        dependencies.push(v.to_string());
                    }
                }
            }
        }
        if !name.is_empty() {
            packages.push(Package {
                name,
                version,
                dependencies,
            });
        }
    }
    if packages.is_empty() {
        let mut msg = String::new();
        let _ = write!(
            msg,
            "Cargo.lock から [[package]] を 1 件も読めなかった (形式が想定と違う)"
        );
        panic!("{msg}");
    }
    packages
}
