//! Derived values and the identifier of a layout law

use alloc::borrow::Cow;
use alloc::vec::Vec;

use sha2::{Digest as _, Sha256};

use crate::decl::{Const, DeriveItem, Layout};

/// The kind tag of a layout law, hashed into its identifier
pub const LAYOUT_LAW_KIND: &[u8] = b"lol.law.layout/v1";

/// What the reading of a layout means, as pins measured from the behaviour of this crate.
///
/// `tests/layout_law.rs` recomputes every one: a change that alters how bytes are read
/// changes a pin, and so the identifier of every layout law
pub const LAYOUT_SEMANTICS_PINS: &[(&str, [u8; 32])] = &[(
    "decode-probe/v1",
    crate::hex32("125f6d4594fcaa36d451d1e9440912e3728e471443df6de8f6549bd55d1810ec"),
)];

/// The fold of [`LAYOUT_SEMANTICS_PINS`]
pub const LAYOUT_SEMANTICS_ID: [u8; 32] =
    crate::hex32("b8e981760e505340471d1d2fc0b2db42f2ee4284fbe1988de2a5457805fc4f87");

fn lenprefix(h: &mut Sha256, b: &[u8]) {
    h.update((b.len() as u64).to_be_bytes());
    h.update(b);
}

/// The fold of semantics pins: SHA-256 over the domain and each pin's name and value, each
/// prefixed with its length as a big-endian `u64`
#[must_use]
pub fn fold_pins(pins: &[(&str, [u8; 32])]) -> [u8; 32] {
    let mut h = Sha256::new();
    lenprefix(&mut h, b"lol.law.layout/semantics");
    for (name, v) in pins {
        lenprefix(&mut h, name.as_bytes());
        lenprefix(&mut h, v);
    }
    h.finalize().into()
}

/// The bytes of a constant inside a derive
fn const_bytes(c: &Const, enc: Option<crate::decl::IntEncoding>) -> Option<Vec<u8>> {
    match (c, enc) {
        (Const::Integer(v), Some(e)) => e.encode(*v),
        (Const::Bytes(b), None) => Some(b.clone()),
        (Const::Text(t), None) => Some(t.as_bytes().to_vec()),
        (Const::Tags(ts), None) => Some(ts.iter().flatten().copied().collect()),
        _ => None,
    }
}

/// A derive, with `segment` giving the bytes of a segment (`None` when it is not available)
pub fn derive_value<'a>(
    layout: &Layout,
    name: &str,
    segment: &dyn Fn(&str) -> Option<Cow<'a, [u8]>>,
) -> Option<[u8; 32]> {
    let d = layout.derives.iter().find(|d| d.name == name)?;
    let mut h = Sha256::new();
    for it in &d.items {
        let bytes: Cow<'_, [u8]> = match it {
            DeriveItem::Const(c, enc) => Cow::Owned(const_bytes(layout.constant(c)?, *enc)?),
            DeriveItem::Segment(s) => segment(s)?,
        };
        if d.length_prefixed {
            lenprefix(&mut h, &bytes);
        } else {
            h.update(&bytes);
        }
    }
    Some(h.finalize().into())
}

impl Layout {
    /// A derive that names only constants, computed from the declaration alone (a format
    /// identifier); `None` when it names a segment or does not exist
    #[must_use]
    pub fn derive_static(&self, name: &str) -> Option<[u8; 32]> {
        derive_value(self, name, &|_| None)
    }

    /// The identifier of the law: SHA-256 over [`LAYOUT_LAW_KIND`], [`LAYOUT_SEMANTICS_ID`]
    /// and every line that declares something, each prefixed with its length as a big-endian
    /// `u64`; comments, blank lines, spacing and the lines for people (`claim` / `source` /
    /// `verdict`) are not part of it
    #[must_use]
    pub fn law_id(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        lenprefix(&mut h, LAYOUT_LAW_KIND);
        lenprefix(&mut h, &LAYOUT_SEMANTICS_ID);
        for l in &self.id_lines {
            lenprefix(&mut h, l.as_bytes());
        }
        h.finalize().into()
    }
}
