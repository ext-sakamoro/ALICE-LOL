use alice_lol::lol;

fn main() {
    // NOT a TPMS-thickness-contract case: a literal f32 value cannot be
    // non-finite without overflowing (there is no NaN/inf literal syntax),
    // and any overflowing literal is already refused as a parse error one
    // layer up (parser.rs's parse_val), before it would ever reach
    // tpms_literal_error's `!thickness.is_finite()` check -- a non-finite
    // literal thickness reaching that check specifically is unreachable,
    // the same way a non-finite literal reaching the runtime text parser's
    // own TPMS check is (see runtime_parser.rs's
    // tpms_fields_validation_rejects_non_finite_values_directly, which
    // tests that branch directly for the same reason). This pins the
    // generic overflow-literal parse error instead -- any numeric literal
    // anywhere in the grammar, not specific to TPMS.
    let _ = lol! { lidinoid(3.0, 3.4028235e39) };
}
