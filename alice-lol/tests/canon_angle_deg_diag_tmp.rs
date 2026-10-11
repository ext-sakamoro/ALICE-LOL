//! Temporary diagnostic: dumps every probe's `canon_angle_deg` stages as
//! `input_bits mod_bits output_bits` lines to stdout (run with
//! `--ignored --nocapture`). Compares an arm64 run against CI's ubuntu-latest
//! run to find the first differing input and stage; not part of the oracle,
//! not meant to stay in the tree.

fn canon_angle_deg_stages(deg: f32) -> (f32, f32) {
    let r = deg % 360.0;
    let out = if r > 180.0 {
        r - 360.0
    } else if r <= -180.0 {
        r + 360.0
    } else {
        r
    };
    (r, out)
}

#[allow(clippy::cast_precision_loss)]
fn dense_sweep(n: usize) -> impl Iterator<Item = f32> {
    (0..n).map(move |i| {
        let t = i as f32 / (n - 1) as f32;
        2160.0_f32.mul_add(t, -1080.0)
    })
}

#[test]
#[ignore = "temporary diagnostic, not part of the oracle"]
fn dump_canon_angle_deg_stages() {
    let probe_values: &[f32] = &[
        10.0,
        -15.0,
        -0.1,
        0.0,
        180.0,
        -180.0,
        1e9,
        -1e9,
        f32::MAX,
        f32::MIN_POSITIVE / 2.0,
        -(f32::MIN_POSITIVE / 2.0),
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    println!("# special values");
    for &deg in probe_values {
        let (r, out) = canon_angle_deg_stages(deg);
        println!(
            "{:08x} {:08x} {:08x}",
            deg.to_bits(),
            r.to_bits(),
            out.to_bits()
        );
    }
    println!("# dense sweep (2161 steps, -1080..=1080)");
    for deg in dense_sweep(2161) {
        let (r, out) = canon_angle_deg_stages(deg);
        println!(
            "{:08x} {:08x} {:08x}",
            deg.to_bits(),
            r.to_bits(),
            out.to_bits()
        );
    }
}
