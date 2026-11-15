//! 全ての `stdlib` の template に、引数を 1 つずつ `-0` にして渡し、正規形の文が往復で変わらないことを
//! 確かめる (`f32::clamp(0.0, ..)` は `-0` を残し、LOL の `rounded_box` の helper は `+0` にするので、
//! `-0` を残す template は 1 回目と読み直した 2 回目の文が違う `skadis_panel(21, 9, -0)` がそうだった)
//!
//! 引数は `geometry_id::STDLIB_TEMPLATE_PROBE_LOL` の文から取る parse で断られる組み合わせは数えない
//! 試した組み合わせが 200 未満なら失敗にする (一覧や parser が変わって空振りしていないか)

use alice_lol::emit::to_lol_unstamped;
use alice_lol::geometry_id::STDLIB_TEMPLATE_PROBE_LOL;
use alice_lol::runtime_parser::parse_lol;

#[test]
fn every_template_with_a_negative_zero_argument_emits_a_fixed_point() {
    let mut broken = Vec::new();
    let mut tried = 0usize;
    for text in STDLIB_TEMPLATE_PROBE_LOL {
        let (name, rest) = text.split_once('(').expect("a call");
        let inner = rest.strip_suffix(')').expect("a closing parenthesis");
        if inner.is_empty() || inner.contains('(') {
            continue;
        }
        let args: Vec<&str> = inner.split(',').map(str::trim).collect();
        for i in 0..args.len() {
            let mut a: Vec<&str> = args.clone();
            a[i] = "-0";
            let probe = format!("{name}({})", a.join(", "));
            let Ok(node) = parse_lol(&probe) else {
                continue;
            };
            let first = to_lol_unstamped(&node).expect("canonical text");
            tried += 1;
            let second = parse_lol(&first)
                .ok()
                .and_then(|n| to_lol_unstamped(&n).ok());
            if second.as_deref() != Some(first.as_str()) {
                broken.push(probe);
            }
        }
    }
    assert!(tried >= 200, "only {tried} negative-zero probes parsed");
    assert!(
        broken.is_empty(),
        "the canonical text is not a fixed point: {broken:?}"
    );
}
