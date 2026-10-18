//! digest golden file の読み書きと照合 (`<case>|<count>|<sha256 hex>` を 1 行 1 ケース)
//!
//! `laser_pattern_digest_golden.rs` と `stdlib_profile_determinism_oracle.rs` が
//! `#[path = "common/golden.rs"] mod golden;` で取り込む 照合の歯 (違う digest・数・順・
//! 欠けた行・0 行を拒む) は `golden_compare.rs` が固定する

use std::fmt::Write as _;

/// 1 ケース: (ケース名, 個数, SHA-256 の小文字 16 進 64 桁)
pub type Row = (String, u64, String);

/// byte 列の小文字 16 進
pub fn hex32(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

/// golden の本文を行に読む 空行は飛ばす 3 欄でない行、個数が整数でない行、digest が
/// 小文字 16 進 64 桁でない行は、何行目かを添えて Err
pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.rsplitn(3, '|');
        let (Some(digest), Some(count), Some(name)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(format!(
                "line {}: not <case>|<count>|<sha256>: {line:?}",
                i + 1
            ));
        };
        let count: u64 = count
            .parse()
            .map_err(|_| format!("line {}: count {count:?} is not an integer", i + 1))?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(format!(
                "line {}: digest {digest:?} is not 64 lower-case hex digits",
                i + 1
            ));
        }
        rows.push((name.to_string(), count, digest.to_string()));
    }
    Ok(rows)
}

/// 行を golden の本文に書く (`parse` の逆)
pub fn render(rows: &[Row]) -> String {
    rows.iter().fold(String::new(), |mut s, (name, n, d)| {
        let _ = writeln!(s, "{name}|{n}|{d}");
        s
    })
}

/// 今の行と golden の行を順に照合し、照合した行数を返す 0 行 (何も比べていない)、
/// 行数の違い、ケース名・順の違い、個数の違い、digest の違いは、最初の違いを Err で返す
pub fn compare(now: &[Row], want: &[Row]) -> Result<usize, String> {
    if now.is_empty() || want.is_empty() {
        return Err(format!(
            "compared nothing ({} case(s) now, {} in the golden)",
            now.len(),
            want.len()
        ));
    }
    if now.len() != want.len() {
        return Err(format!(
            "{} case(s) now, {} in the golden",
            now.len(),
            want.len()
        ));
    }
    for ((name, n, digest), (w_name, w_n, w_digest)) in now.iter().zip(want) {
        if name != w_name {
            return Err(format!(
                "case {name:?} where the golden has {w_name:?} (order or name differs)"
            ));
        }
        if n != w_n {
            return Err(format!("{name}: count {n}, golden {w_n}"));
        }
        if digest != w_digest {
            return Err(format!("{name}: digest {digest}, golden {w_digest}"));
        }
    }
    Ok(now.len())
}
