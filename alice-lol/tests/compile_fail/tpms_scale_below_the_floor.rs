use alice_lol::lol;

fn main() {
    // positive and finite, but below the 1e-6 floor (not <= 0.0): a mutant
    // weakening the literal check's condition to `scale <= 0.0` would accept
    // this and only tpms_bad_scale.rs's exactly-zero case would still catch it
    let _ = lol! { gyroid(5e-7, 0.1) };
}
