//! Pinned nodes of the stdlib generators whose arithmetic was moved onto the Law
//! helpers (`alice_det_math::powi` for the jewelry stand tier radii, the lane-wise
//! `det_vec` fold for the SKADIS peg round radius), so a later change of either
//! shows up here: the digest is the SHA-256 of the node's debug form, which prints
//! every float to its last bit
use alice_lol::stdlib::hardsurface::mount::skadis_peg_compat;
use alice_lol::stdlib::hardsurface::pattern_sdf::{jewelry_stand, JewelryStandSpec};
use alice_lol::SdfNode;
use sha2::{Digest, Sha256};

fn digest(node: &SdfNode) -> String {
    let text = format!("{node:?}");
    assert!(text.len() > 16, "the debug form is empty");
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::new(), |mut out, b| {
            use std::fmt::Write as _;
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// jewelry stand tiers: radius = bottom / 2 * ratio^i through `alice_det_math::powi`
#[test]
fn jewelry_stand_tier_radii_are_pinned() {
    let three = JewelryStandSpec::standard_3_tier();
    let mut eight = JewelryStandSpec::standard_3_tier();
    eight.tier_count = 8;
    // 0.77: repeated squaring and a left-to-right product differ from i = 4 on
    eight.tier_ratio = 0.77;
    let got = [
        digest(&jewelry_stand(&three)),
        digest(&jewelry_stand(&eight)),
    ];
    assert_eq!(
        got,
        [
            "7940714be61986ad487b7bc174ee5b375e9ea6d75398c67b6bbe4f55941beb11",
            "421e0c964b88c56e05c50d130fc5f4443c50decb18ae5302061b205b96a68502"
        ]
    );
}

/// SKADIS peg: round radius = `max(min(R, min_element(half)), 0)` through `det_vec`
#[test]
fn skadis_peg_round_radius_is_pinned() {
    let got = [
        digest(&skadis_peg_compat(5.0)),
        digest(&skadis_peg_compat(3.0)),
    ];
    assert_eq!(
        got,
        [
            "9cf691a86f2e60fdd0bebd79f7112b57d7730dc4c4f78e504223fb58bb1db942",
            "f8019c01d304a2fd75d6bd8e4a4dda15a9062e21f00d31734198908877596b05"
        ]
    );
}
