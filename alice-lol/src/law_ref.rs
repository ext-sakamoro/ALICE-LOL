//! A reference to a law with its parameter values, as bytes another program can read
//!
//! A [`LawRef`] names a law by its identifier, says which semantics it was written under, and
//! carries a value for every parameter of the law in the law's declared order. A reader
//! holding the law text resolves the reference: the identifier recomputed from the text must
//! be the one named, the semantics must be this build's, and every parameter must be the
//! law's and in its range. Any difference is an error naming both sides, never a silent
//! default.
//!
//! # Bytes
//!
//! The layout is declared once, as a layout law (`laws/layout/law_ref.layout`, read by
//! [`crate::layout`]); the tests decode every encoding against that declaration. In short:
//! a 72-byte header (`LOLR`, version `1` as a little-endian `u16`, the parameter count as a
//! `u16`, the semantics identifier, the law identifier), then one 40-byte row per parameter
//! (the name followed by zero bytes up to 32, the value's binary64 bits as a little-endian
//! `u64`), and nothing after. A name is at most [`MAX_NAME`] bytes, at most [`MAX_PARAMS`]
//! rows.

use std::fmt;

use crate::motion_law::{BoundMotion, MotionLaw, SampleError, MOTION_SEMANTICS_ID};
use crate::proportion_law::{BoundProportion, ProportionLaw, PROPORTION_SEMANTICS_ID};

/// The first four bytes of an encoded reference
pub const MAGIC: [u8; 4] = *b"LOLR";
/// The version of the encoding
pub const VERSION: u16 = 1;
/// The longest parameter name, in bytes
pub const MAX_NAME: usize = 32;
/// The most parameters
pub const MAX_PARAMS: usize = 64;
/// The bytes of the header
pub const HEADER_LEN: usize = 72;
/// The bytes of a parameter row
pub const ROW_LEN: usize = 40;

/// A law, the semantics it was written under and a value for each of its parameters
#[derive(Debug, Clone, PartialEq)]
pub struct LawRef {
    /// the semantics identifier of the law's kind when the reference was written
    pub semantics_id: [u8; 32],
    /// the law's identifier
    pub law_id: [u8; 32],
    /// every parameter of the law, in the law's declared order
    pub params: Vec<(String, f64)>,
}

/// Why a reference cannot be written, read or resolved
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LawRefError {
    /// the bytes are not an encoded reference (what is wrong, as the layout law labels it)
    Format(&'static str),
    /// the reference was written under other semantics than this build's
    SemanticsMismatch {
        /// this build's semantics identifier
        ours: [u8; 32],
        /// the one in the reference
        theirs: [u8; 32],
    },
    /// the law text is not the law the reference names
    LawMismatch {
        /// the identifier of the law text given
        ours: [u8; 32],
        /// the identifier in the reference
        theirs: [u8; 32],
    },
    /// the parameter names are not the law's, in the law's order
    ParamNames {
        /// the names the law declares
        law: Vec<String>,
        /// the names in the reference
        reference: Vec<String>,
    },
    /// a value is not one the law accepts
    Param(SampleError),
    /// a name longer than [`MAX_NAME`] bytes, or more than [`MAX_PARAMS`] parameters
    TooLarge,
}

fn hex(b: &[u8; 32]) -> String {
    use fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

impl fmt::Display for LawRefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(label) => write!(f, "not a law reference ({label})"),
            Self::SemanticsMismatch { ours, theirs } => write!(
                f,
                "the reference was written under semantics {}, this build reads {}",
                hex(theirs),
                hex(ours)
            ),
            Self::LawMismatch { ours, theirs } => write!(
                f,
                "the reference names law {}, the law text given is {}",
                hex(theirs),
                hex(ours)
            ),
            Self::ParamNames { law, reference } => write!(
                f,
                "the reference has parameters [{}], the law declares [{}]",
                reference.join(", "),
                law.join(", ")
            ),
            Self::Param(e) => write!(f, "{e}"),
            Self::TooLarge => write!(
                f,
                "a name longer than {MAX_NAME} bytes or more than {MAX_PARAMS} parameters"
            ),
        }
    }
}

impl std::error::Error for LawRefError {}

impl LawRef {
    /// A reference to a motion law with the given values (each named one checked as
    /// [`MotionLaw::bind`] checks it; the others take their defaults), so the reference
    /// carries every parameter
    ///
    /// # Errors
    ///
    /// [`LawRefError::Param`] when the values do not bind, [`LawRefError::TooLarge`] when the
    /// law has a name or a count the encoding cannot hold
    pub fn motion(law: &MotionLaw, values: &[(&str, f64)]) -> Result<Self, LawRefError> {
        let bound = law.bind(values).map_err(LawRefError::Param)?;
        let params: Vec<(String, f64)> = law
            .params()
            .iter()
            .zip(bound.values())
            .map(|(p, v)| (p.name.clone(), *v))
            .collect();
        let r = Self {
            semantics_id: MOTION_SEMANTICS_ID,
            law_id: law.law_id(),
            params,
        };
        r.check_size()?;
        Ok(r)
    }

    /// A reference to a proportion law with the given values (each named one checked as
    /// [`ProportionLaw::bind`] checks it; the others take their defaults), so the reference
    /// carries every parameter
    ///
    /// # Errors
    ///
    /// [`LawRefError::Param`] when the values do not bind, [`LawRefError::TooLarge`] when the
    /// law has a name or a count the encoding cannot hold
    pub fn proportion(law: &ProportionLaw, values: &[(&str, f64)]) -> Result<Self, LawRefError> {
        let bound = law.bind(values).map_err(LawRefError::Param)?;
        let params: Vec<(String, f64)> = law
            .params()
            .iter()
            .zip(bound.values())
            .map(|(p, v)| (p.name.clone(), *v))
            .collect();
        let r = Self {
            semantics_id: PROPORTION_SEMANTICS_ID,
            law_id: law.law_id(),
            params,
        };
        r.check_size()?;
        Ok(r)
    }

    fn check_size(&self) -> Result<(), LawRefError> {
        if self.params.len() > MAX_PARAMS || self.params.iter().any(|(n, _)| n.len() > MAX_NAME) {
            Err(LawRefError::TooLarge)
        } else {
            Ok(())
        }
    }

    /// The reference as bytes (the layout of the module documentation)
    ///
    /// # Errors
    ///
    /// [`LawRefError::TooLarge`]
    pub fn to_bytes(&self) -> Result<Vec<u8>, LawRefError> {
        self.check_size()?;
        let mut out = Vec::with_capacity(HEADER_LEN + ROW_LEN * self.params.len());
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        let count = u16::try_from(self.params.len()).map_err(|_| LawRefError::TooLarge)?;
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&self.semantics_id);
        out.extend_from_slice(&self.law_id);
        for (name, value) in &self.params {
            let mut field = [0u8; MAX_NAME];
            field[..name.len()].copy_from_slice(name.as_bytes());
            out.extend_from_slice(&field);
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        Ok(out)
    }

    /// Reads an encoded reference (it is not compared with any law yet: see
    /// [`LawRef::resolve_motion`])
    ///
    /// # Errors
    ///
    /// [`LawRefError::Format`] with the label of the layout law's check that fails:
    /// `too_short`, `bad_magic`, `version`, `count`, `layout` (rows missing or bytes after
    /// them), `name` (a name that is empty, not zero-padded or not UTF-8)
    pub fn from_bytes(b: &[u8]) -> Result<Self, LawRefError> {
        let header = b
            .get(..HEADER_LEN)
            .ok_or(LawRefError::Format("too_short"))?;
        if header[..4] != MAGIC {
            return Err(LawRefError::Format("bad_magic"));
        }
        if u16::from_le_bytes([header[4], header[5]]) != VERSION {
            return Err(LawRefError::Format("version"));
        }
        let count = usize::from(u16::from_le_bytes([header[6], header[7]]));
        if count > MAX_PARAMS {
            return Err(LawRefError::Format("count"));
        }
        if b.len() != HEADER_LEN + ROW_LEN * count {
            return Err(LawRefError::Format("layout"));
        }
        let mut semantics_id = [0u8; 32];
        semantics_id.copy_from_slice(&header[8..40]);
        let mut law_id = [0u8; 32];
        law_id.copy_from_slice(&header[40..72]);
        let mut params = Vec::with_capacity(count);
        let (rows, _) = b[HEADER_LEN..].as_chunks::<ROW_LEN>();
        for row in rows {
            let field = &row[..MAX_NAME];
            let len = field.iter().position(|&c| c == 0).unwrap_or(MAX_NAME);
            if len == 0 || field[len..].iter().any(|&c| c != 0) {
                return Err(LawRefError::Format("name"));
            }
            let name =
                std::str::from_utf8(&field[..len]).map_err(|_| LawRefError::Format("name"))?;
            let mut v = [0u8; 8];
            v.copy_from_slice(&row[MAX_NAME..]);
            params.push((name.to_owned(), f64::from_bits(u64::from_le_bytes(v))));
        }
        Ok(Self {
            semantics_id,
            law_id,
            params,
        })
    }

    /// The motion law the reference names, with the reference's values
    ///
    /// # Errors
    ///
    /// [`LawRefError::SemanticsMismatch`] when the reference was written under other motion
    /// semantics, [`LawRefError::LawMismatch`] when `law` is not the law it names (both
    /// identifiers named), [`LawRefError::ParamNames`] when the names are not the law's in its
    /// order, [`LawRefError::Param`] when a value is outside its range
    pub fn resolve_motion<'a>(&self, law: &'a MotionLaw) -> Result<BoundMotion<'a>, LawRefError> {
        if self.semantics_id != MOTION_SEMANTICS_ID {
            return Err(LawRefError::SemanticsMismatch {
                ours: MOTION_SEMANTICS_ID,
                theirs: self.semantics_id,
            });
        }
        let ours = law.law_id();
        if ours != self.law_id {
            return Err(LawRefError::LawMismatch {
                ours,
                theirs: self.law_id,
            });
        }
        let declared: Vec<String> = law.params().iter().map(|p| p.name.clone()).collect();
        let given: Vec<String> = self.params.iter().map(|(n, _)| n.clone()).collect();
        if declared != given {
            return Err(LawRefError::ParamNames {
                law: declared,
                reference: given,
            });
        }
        let values: Vec<(&str, f64)> = self.params.iter().map(|(n, v)| (n.as_str(), *v)).collect();
        law.bind(&values).map_err(LawRefError::Param)
    }

    /// The proportion law the reference names, with the reference's values
    ///
    /// # Errors
    ///
    /// [`LawRefError::SemanticsMismatch`] when the reference was written under other
    /// proportion semantics (or is a reference to another kind), [`LawRefError::LawMismatch`]
    /// when `law` is not the law it names, [`LawRefError::ParamNames`] when the names are not
    /// the law's in its order, [`LawRefError::Param`] when a value is outside its range or an
    /// output is not finite
    pub fn resolve_proportion<'a>(
        &self,
        law: &'a ProportionLaw,
    ) -> Result<BoundProportion<'a>, LawRefError> {
        if self.semantics_id != PROPORTION_SEMANTICS_ID {
            return Err(LawRefError::SemanticsMismatch {
                ours: PROPORTION_SEMANTICS_ID,
                theirs: self.semantics_id,
            });
        }
        let ours = law.law_id();
        if ours != self.law_id {
            return Err(LawRefError::LawMismatch {
                ours,
                theirs: self.law_id,
            });
        }
        let declared: Vec<String> = law.params().iter().map(|p| p.name.clone()).collect();
        let given: Vec<String> = self.params.iter().map(|(n, _)| n.clone()).collect();
        if declared != given {
            return Err(LawRefError::ParamNames {
                law: declared,
                reference: given,
            });
        }
        let values: Vec<(&str, f64)> = self.params.iter().map(|(n, v)| (n.as_str(), *v)).collect();
        law.bind(&values).map_err(LawRefError::Param)
    }
}
