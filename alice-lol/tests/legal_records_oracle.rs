//! The rule laws of the legal domain compiled into ALICE-Legal's records (docs/wiring-debt.md,
//! `legal-records`)
//!
//! oracle: ALICE-Legal's own constructors (`StatuteTree::add_clause`, `Contract::new` /
//! `add_condition` / `add_obligation`, `Procedure::add_step`) build the records from what a statute,
//! contract or procedure law reads, and the identifiers they compute are compared with the ones
//! compiled from the law, so the compiler cannot report identifiers of its own
//!
//! Until alice-legal is linked (it brings another alice-zip and alice-det-math than alice-lol's
//! other dependencies), this pins that the link and the compiler are not there: the test fails, and
//! `scripts/src_gap_check.py --run` keeps it failing; when the dependency and its feature are added
//! it passes, the check fails, and this body is replaced by the comparison above.

#[test]
#[ignore = "src gap: legal-records: alice-legal is not linked, so no record is built by ALICE-Legal's constructors"]
fn the_legal_laws_compile_to_records_alice_legal_rebuilds_with_the_same_identifiers() {
    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )
    .expect("alice-lol/Cargo.toml");
    let linked = manifest
        .lines()
        .any(|l| l.trim_start().starts_with("alice-legal"));
    let feature = manifest
        .lines()
        .any(|l| l.trim_start().starts_with("legal ="));
    assert!(
        linked && feature,
        "alice-legal is not a dependency of alice-lol behind a `legal` feature (linked: {linked}, feature: {feature})"
    );
}
