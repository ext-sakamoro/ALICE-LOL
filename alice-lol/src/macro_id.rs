//! The `lol!` macro's identifier, as this release of `alice-lol` expects it (ALICE-Law G7)
//!
//! `lol!` is expanded by `alice-lol-macro`, which `alice-lol` requires by a caret range:
//! any release in that range may do the expansion, and the expansion reaches the binary as
//! source text, so no Rust type carries the meaning across. Every expansion therefore
//! carries the macro's identifier (`alice-lol-macro`'s `EXPANSION_ID_HEX`, a digest of
//! what its probe corpus expands to) and calls [`__expect_macro_id!`](crate::__expect_macro_id),
//! which accepts only [`EXPECTED_MACRO_EXPANSION_ID`] and turns any other identifier into a
//! compile error that names both.
//!
//! A program built against a macro release with another identifier stops at compile time:
//!
//! ```compile_fail
//! alice_lol::__expect_macro_id!("0000000000000000000000000000000000000000000000000000000000000000");
//! ```
//!
//! and the identifier this release expects compiles:
//!
//! ```
//! alice_lol::__expect_macro_id!("82a612c32513fdad4f34f617ce8c69ae541d593e4f04004f7fc9f129c64efa10");
//! ```

use crate::law_id::hex32;

/// The identifier of `alice-lol-macro`'s expansion that this release accepts
///
/// The arm of [`__expect_macro_id!`](crate::__expect_macro_id) spells the same value as a
/// literal (a `macro_rules!` arm cannot read a constant); a test in this module keeps the two
/// equal, and every `lol!` in this workspace keeps both equal to the macro's own value
// ALLOW-UNWIRED: the value the __expect_macro_id! arm spells as a literal (a macro_rules arm cannot read a constant); the arm test ties them and id_move_check pins its moves
pub const EXPECTED_MACRO_EXPANSION_ID: [u8; 32] =
    hex32("82a612c32513fdad4f34f617ce8c69ae541d593e4f04004f7fc9f129c64efa10");

/// Called by every `lol!` expansion with the macro's identifier: accepts the identifier this
/// release expects and refuses any other with a compile error naming both
#[doc(hidden)]
#[macro_export]
macro_rules! __expect_macro_id {
    ("82a612c32513fdad4f34f617ce8c69ae541d593e4f04004f7fc9f129c64efa10") => {};
    ($other:literal) => {
        ::core::compile_error!(::core::concat!(
            "lol!: the expansion's identifier is ",
            $other,
            " (alice-lol-macro), but this alice-lol expects ",
            "82a612c32513fdad4f34f617ce8c69ae541d593e4f04004f7fc9f129c64efa10",
            "; use the alice-lol-macro release this alice-lol was released with"
        ));
    };
}

#[cfg(test)]
mod tests {
    use super::EXPECTED_MACRO_EXPANSION_ID;

    fn hex(bytes: &[u8; 32]) -> String {
        bytes.iter().fold(String::new(), |mut acc, b| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{b:02x}");
            acc
        })
    }

    /// the literals in this file that look like an identifier, each with its line
    fn literals() -> Vec<(usize, String)> {
        let src = include_str!("macro_id.rs");
        let mut out = Vec::new();
        for (n, line) in src.lines().enumerate() {
            let mut rest = line;
            while let Some(start) = rest.find('"') {
                let tail = &rest[start + 1..];
                let Some(end) = tail.find('"') else { break };
                let lit = &tail[..end];
                if lit.len() == 64 && lit.bytes().all(|b| b.is_ascii_hexdigit()) {
                    out.push((n + 1, lit.to_owned()));
                }
                rest = &tail[end + 1..];
            }
        }
        out
    }

    #[test]
    fn the_macro_arm_and_the_constant_agree() {
        let want = hex(&EXPECTED_MACRO_EXPANSION_ID);
        let found = literals();
        // the constant, the accepting arm, the error message and the passing doctest
        // (the compile_fail doctest's all-zero value is the one other literal)
        let expected = found.iter().filter(|(_, l)| *l == want).count();
        let others: Vec<_> = found.iter().filter(|(_, l)| *l != want).collect();
        assert_eq!(expected, 4, "{found:?}");
        assert_eq!(
            others.len(),
            1,
            "a literal differs from the constant: {others:?}"
        );
        assert!(others[0].1.bytes().all(|b| b == b'0'), "{others:?}");
    }
}
