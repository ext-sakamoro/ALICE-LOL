//! The law of a LOL program's shape, as a law reference v2 (`alice_zip::law_ref::LawRef`)
//!
//! A program names a shape: [`crate::runtime_parser::parse_lol`] turns its text into an
//! `SdfNode`, and the geometry evaluation is pinned by [`LOL_GEOMETRY_SEMANTICS_ID`]. This module
//! gives that law a kind, [`PROGRAM_LAW_KIND`], so another product can hand the shape on as a law
//! reference instead of its mesh: [`program_law_ref`] builds the reference (one row, [`SOURCE_ROW`],
//! the program text), and [`resolve_program`] is the owner's evaluation of it. The law identifier
//! is the v2 identifier of ALICE-Zip's `LawIdHasher` (computed by `LawRef`), so it is checked from
//! the bytes alone, and the receiver rebuilds the shape from the program instead of trusting a mesh.
//!
//! What the reference does not carry: how a product meshes the shape (resolution, repair). A
//! product that sends a mesh digest with the reference states its own meshing alongside it.

use alice_sdf::SdfNode;
use alice_zip::law_ref::{LawRef, LawRefError, Value};

use crate::geometry_id::{check_lol_geometry_semantics, LOL_GEOMETRY_SEMANTICS_ID};
use crate::runtime_parser::{parse_lol, ParseError};

/// The kind of the law a LOL program's text names (its shape)
pub const PROGRAM_LAW_KIND: &[u8] = b"lol.law.program/v1";

/// The name of the reference's one row: the program text
pub const SOURCE_ROW: &str = "source";

/// Why a law reference does not resolve to a program's shape
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ProgramLawError {
    /// The reference names a law of another kind (its kind's owner evaluates it)
    OtherKind(Vec<u8>),
    /// The reference was written under a geometry semantics this version neither has nor
    /// reproduces bit for bit (`theirs` from the reference, `ours` of this version)
    SemanticsMismatch {
        /// The semantics identifier in the reference
        theirs: [u8; 32],
        /// This version's geometry semantics identifier
        ours: [u8; 32],
    },
    /// The rows are not exactly one [`SOURCE_ROW`] holding a string
    Rows(String),
    /// The program text does not parse
    Parse(ParseError),
}

fn hex(b: &[u8]) -> String {
    use core::fmt::Write as _;
    b.iter()
        .fold(String::with_capacity(b.len() * 2), |mut s, x| {
            // a write to a String does not fail
            let _ = write!(s, "{x:02x}");
            s
        })
}

impl core::fmt::Display for ProgramLawError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OtherKind(k) => write!(
                f,
                "the law reference is of kind `{}`, not `{}`",
                String::from_utf8_lossy(k),
                String::from_utf8_lossy(PROGRAM_LAW_KIND)
            ),
            Self::SemanticsMismatch { theirs, ours } => write!(
                f,
                "the law reference's geometry semantics {} is not this version's {}",
                hex(theirs),
                hex(ours)
            ),
            Self::Rows(why) => write!(f, "the law reference's rows: {why}"),
            Self::Parse(e) => write!(f, "the program in the law reference does not parse: {e}"),
        }
    }
}

impl std::error::Error for ProgramLawError {}

/// The law reference of the shape `source` names: kind [`PROGRAM_LAW_KIND`], this version's
/// geometry semantics, one [`SOURCE_ROW`] row
///
/// The text is not parsed here (a reference names the law; [`resolve_program`] evaluates it).
///
/// # Errors
/// What `LawRef::new` refuses (it cannot for this kind and one row).
pub fn program_law_ref(source: &str) -> Result<LawRef, LawRefError> {
    LawRef::new(
        PROGRAM_LAW_KIND,
        LOL_GEOMETRY_SEMANTICS_ID,
        vec![(SOURCE_ROW.into(), Value::Str(source.into()))],
    )
}

/// The shape a law reference of kind [`PROGRAM_LAW_KIND`] names
///
/// The kind, then the semantics (this version's, or an earlier one that reproduces the same
/// bits), then the rows are checked before the program is read. The reference's own law identifier
/// was checked when it was read (`LawRef::from_bytes`) or computed (`LawRef::new`).
///
/// # Errors
/// [`ProgramLawError`]: another kind, another semantics, rows other than one string
/// [`SOURCE_ROW`], or a program that does not parse.
pub fn resolve_program(r: &LawRef) -> Result<SdfNode, ProgramLawError> {
    if r.kind() != PROGRAM_LAW_KIND {
        return Err(ProgramLawError::OtherKind(r.kind().to_vec()));
    }
    if matches!(
        check_lol_geometry_semantics(r.semantics_id()),
        alice_det_math::SemanticsCheck::Mismatch
    ) {
        return Err(ProgramLawError::SemanticsMismatch {
            theirs: *r.semantics_id(),
            ours: LOL_GEOMETRY_SEMANTICS_ID,
        });
    }
    let source = match r.rows() {
        [(name, Value::Str(s))] if name == SOURCE_ROW => s,
        [(name, _)] if name == SOURCE_ROW => {
            return Err(ProgramLawError::Rows(format!(
                "`{SOURCE_ROW}` is not a string"
            )));
        }
        rows => {
            let names: Vec<&str> = rows.iter().map(|(n, _)| n.as_str()).collect();
            return Err(ProgramLawError::Rows(format!(
                "expected exactly one row `{SOURCE_ROW}`, found {names:?}"
            )));
        }
    };
    parse_lol(source).map_err(ProgramLawError::Parse)
}
