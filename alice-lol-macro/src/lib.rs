//! ALICE-LOL `proc_macro`: LOL DSL → `SdfNode` construction code
//!
//! # Syntax (v0.5)
//!
//! ```ignore
//! lol! {
//!     field SceneName {
//!         smooth_union(0.2,
//!             sphere(1.0),
//!             translate(2.0, 0.0, 0.0,
//!                 box3d(0.5, 0.5, 0.5)
//!             )
//!         )
//!     }
//! }
//! ```
//!
//! Also supports bare expressions without the `field` wrapper:
//! ```ignore
//! lol! { sphere(1.0) }
//! ```
//!
//! Runtime variable capture with `{expr}` in numeric positions:
//! ```ignore
//! let r = 1.5_f32;
//! lol! { sphere({r}) }
//! lol! { translate({x}, {y}, 0.0, sphere({r * 2.0})) }
//! ```
//!
//! # Module 構成 (private)
//!
//! - `ast` — LOL Internal AST (`Expr` enum + variants)
//! - `parser` — Parser (LOL DSL → `Expr` AST)
//! - `codegen` — Codegen (`Expr` → Rust `TokenStream`)
//!
//! `proc_macro` crate は 1 entry point (`lol!`) のみ export、
//! backward compat 用の re-export は不要 (macro invocation は変わらない)

#![allow(
    clippy::wildcard_imports,
    clippy::type_complexity,
    clippy::many_single_char_names
)]

mod ast;
mod codegen;
mod parser;

use crate::codegen::codegen;
use crate::parser::LolInput;
use proc_macro::TokenStream;

/// LOL (Law-Oriented Language) `proc_macro`.
///
/// Parses LOL DSL and generates Rust code that constructs an `SdfNode` tree.
///
/// # Usage
///
/// ```ignore
/// use alice_lol::lol;
///
/// // With field wrapper
/// let node = lol! {
///     field MyScene {
///         smooth_union(0.2,
///             sphere(1.0),
///             translate(2.0, 0.0, 0.0, box3d(0.5, 0.5, 0.5))
///         )
///     }
/// };
///
/// // Bare expression
/// let node = lol! { sphere(1.0) };
///
/// // Variable capture with {expr}
/// let r = 1.5_f32;
/// let node = lol! { sphere({r}) };
/// let node = lol! { translate({x}, {y}, 0.0, sphere({r * 2.0})) };
/// ```
#[proc_macro]
pub fn lol(input: TokenStream) -> TokenStream {
    let scene = syn::parse_macro_input!(input as LolInput);
    let node_code = codegen(&scene.body);
    node_code.into()
}
