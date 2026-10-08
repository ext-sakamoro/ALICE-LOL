//! 文法 file の 2 つの写しが同一であること.
//!
//! `alice-lol/lol.gbnf` が正典 (`LOL_GBNF` が `include_str!` で読み、FSM が使う) で、
//! `skills/lol-sdf/references/lol.gbnf` は配布に同梱する写し
//!
//! ⚠️ **片方だけ編集すると気付けない** 正典だけ直すと同梱の文法が古くなり、
//! 写しだけ直すと `lol_gbnf_test` が parser との差を報告する (⚠️ しかもその test は
//! `llm-bridge` feature 配下なので、feature 無しの run では 0 test で通ってしまう)
//! ⇒ 本 test は feature を要求せず、常に 2 file を byte で比べる
//!
//! 2026-09-14 には「parser と downstream の写しには足したが正典には足していない」
//! 方向の drift が実際に起きている (`lol.gbnf` の冒頭 comment 参照)

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR は alice-lol/ なので 1 つ上が workspace の root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

#[test]
fn the_bundled_copy_of_the_grammar_matches_the_canonical_one() {
    let root = repo_root();
    let canonical = root.join("alice-lol/lol.gbnf");
    let copy = root.join("skills/lol-sdf/references/lol.gbnf");

    let a = fs::read_to_string(&canonical)
        .unwrap_or_else(|e| panic!("{} を読めない: {e}", canonical.display()));
    let b =
        fs::read_to_string(&copy).unwrap_or_else(|e| panic!("{} を読めない: {e}", copy.display()));

    // 0 件比較を避ける: 空 file 同士が「一致」になるのを防ぐ
    assert!(
        a.lines().count() > 50,
        "{} が {} 行しかない (読めていない疑い)",
        canonical.display(),
        a.lines().count()
    );

    if a != b {
        let first = a
            .lines()
            .zip(b.lines())
            .position(|(x, y)| x != y)
            .map_or_else(|| "行数が違う".to_owned(), |i| format!("{} 行目", i + 1));
        panic!(
            "文法の 2 つの写しが違う ({first}) — 片方だけ編集した可能性がある\n  正典: {}\n  写し: {}",
            canonical.display(),
            copy.display()
        );
    }
}
