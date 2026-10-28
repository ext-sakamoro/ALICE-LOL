//! A material law's residual (`kind material` with `alice_lol::law_residual`): which bound law a
//! residual belongs to, and the texel grids added to the fields
//!
//! The material kind lays a texel residual over the tile: domain `texels <W> <H>` (any whole
//! positive W, H: the residual chooses its resolution), blocks `height` (W × H), `albedo`
//! (W × H × 3) and `roughness` (W × H), texel (i, j) centred at `((i + 0.5)/W·tu, (j + 0.5)/H·tv)`,
//! row-major with i fastest, bilinear between centres, wrapping around the tile. The residual is
//! added to the fields after the law (a zero adds nothing, so a residual of zeros samples as the
//! law alone, bit for bit), and the albedo and roughness ranges are checked after it.
//! Checked by hand: the bound hash (every parameter value but the excluded ones), the refusals,
//! the texel centres, a point between centres, the wrap and the zero residual.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use alice_lol::law_residual::{bound_hash, LawResidual, ResidualOwner};
use alice_lol::material_law::{MaterialError, MaterialLaw, MATERIAL_LAW_KIND};
use sha2::{Digest, Sha256};

const LAW: &str = "law slab
kind material
param shade number = 0.4 range 0 1
param tilt number = 0.002 range 0 0.01
tile = (2, 1)
normal_step = 0.001
field height = tilt * u
field albedo = (shade, shade, shade)
field roughness = 0.5
";

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn f64s(v: &[f64]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// The bytes of a residual for `law` bound at `values`, W x H = 4 x 2, with these block payloads
/// written as type `ty` (`f64x2` pairs or `f64`) and an optional quantum
fn build(
    law: &MaterialLaw,
    values: &[(&str, f64)],
    payloads: [Vec<u8>; 3],
    ty: &str,
    quantum: Option<&str>,
) -> Vec<u8> {
    let b = law.bind(values).unwrap();
    let bound = bound_hash(&b.canonical_bound(&[]));
    let mut head = format!(
        "residual slab_fit\nfor {} {}\nbound {}\nframe world\nindex texels 4 2\nbetween texels bilinear wrap\n",
        std::str::from_utf8(MATERIAL_LAW_KIND).unwrap(),
        hex(&law.law_id()),
        hex(&bound)
    );
    if let Some(q) = quantum {
        head += &format!("quantum {q}\n");
    }
    let shapes = ["4 2", "4 2 3", "4 2"];
    for ((n, bytes), shape) in ["height", "albedo", "roughness"]
        .iter()
        .zip(&payloads)
        .zip(shapes)
    {
        head += &format!(
            "block {n} {ty} texels {shape} {}\n",
            hex(&Sha256::digest(bytes))
        );
    }
    head += "payload\n";
    let mut out = head.into_bytes();
    for bytes in payloads {
        out.extend(bytes);
    }
    out
}

/// pairs (hi, lo) as the payload of an `f64x2` block
fn pairs(v: &[(f64, f64)]) -> Vec<u8> {
    v.iter()
        .flat_map(|(h, l)| h.to_le_bytes().into_iter().chain(l.to_le_bytes()))
        .collect()
}

/// A texel residual whose values are exact pairs (value, 0)
fn residual(
    law: &MaterialLaw,
    values: &[(&str, f64)],
    h: &[f64],
    a: &[f64],
    r: &[f64],
) -> LawResidual {
    let p = |v: &[f64]| pairs(&v.iter().map(|x| (*x, 0.0)).collect::<Vec<_>>());
    LawResidual::from_bytes(&build(law, values, [p(h), p(a), p(r)], "f64x2", None)).unwrap()
}

fn grid(f: impl Fn(usize) -> f64, n: usize) -> Vec<f64> {
    (0..n).map(f).collect()
}

#[test]
fn a_bound_material_is_the_owner_of_its_residuals() {
    let law = MaterialLaw::from_file(LAW).unwrap();
    let b = law.bind(&[]).unwrap();
    assert_eq!(b.kind_tag().as_bytes(), MATERIAL_LAW_KIND);
    assert_eq!(b.law_id(), law.law_id());
    // every parameter value is in the bound; an excluded one is not
    let other = law.bind(&[("tilt", 0.003)]).unwrap();
    assert_ne!(b.canonical_bound(&[]), other.canonical_bound(&[]));
    assert_eq!(
        b.canonical_bound(&["tilt"]),
        other.canonical_bound(&["tilt"])
    );
    // texels of any whole positive size; nothing else
    assert!(b.accepts_index("texels", &["4".into(), "2".into()]));
    assert!(b.accepts_index("texels", &["1024".into(), "1".into()]));
    assert!(!b.accepts_index("texels", &["0".into(), "2".into()]));
    assert!(!b.accepts_index("texels", &["4".into()]));
    assert!(!b.accepts_index("mesh", &["vertices".into(), "4".into()]));
}

#[test]
fn a_residual_of_another_instance_is_refused() {
    let law = MaterialLaw::from_file(LAW).unwrap();
    let zeros = |n| vec![0.0; n];
    let r = residual(&law, &[], &zeros(8), &zeros(24), &zeros(8));
    let b = law.bind(&[]).unwrap();
    assert!(b.with_residual(&r).is_ok());
    let other = law.bind(&[("shade", 0.5)]).unwrap();
    assert!(matches!(
        other.with_residual(&r),
        Err(MaterialError::Residual(_))
    ));
    let law2 = MaterialLaw::from_file(&LAW.replace("0.5\n", "0.6\n")).unwrap();
    assert!(
        law2.bind(&[]).unwrap().with_residual(&r).is_err(),
        "another law"
    );
}

#[test]
fn a_residual_of_zeros_samples_as_the_law() {
    let law = MaterialLaw::from_file(LAW).unwrap();
    let b = law.bind(&[]).unwrap();
    let r = residual(&law, &[], &vec![0.0; 8], &vec![0.0; 24], &vec![0.0; 8]);
    let with = b.with_residual(&r).unwrap();
    for k in 0..40 {
        let (u, v) = (f64::from(k) * 0.137 - 1.0, f64::from(k) * 0.071);
        assert_eq!(with.at(u, v).unwrap(), b.at(u, v).unwrap(), "({u}, {v})");
    }
}

#[test]
fn a_zero_residual_keeps_a_negative_zero() {
    // the law's height is -0.0 everywhere: adding a residual of +0.0 would make it +0.0, so a
    // zero adds nothing (bit for bit the law)
    let law = MaterialLaw::from_file(&LAW.replace("tilt * u", "(0 - 1) * 0")).unwrap();
    let b = law.bind(&[]).unwrap();
    assert_eq!(
        b.at(0.3, 0.4).unwrap().height.to_bits(),
        (-0.0f64).to_bits()
    );
    let r = residual(&law, &[], &vec![0.0; 8], &vec![0.0; 24], &vec![0.0; 8]);
    let got = b.with_residual(&r).unwrap().at(0.3, 0.4).unwrap();
    assert_eq!(got.height.to_bits(), (-0.0f64).to_bits());
}

#[test]
fn at_a_texel_centre_the_residual_is_its_value() {
    let law = MaterialLaw::from_file(LAW).unwrap();
    let b = law.bind(&[]).unwrap();
    let h = grid(|k| (k as f64) * 0.001, 8);
    let a = grid(|k| (k as f64) * 0.01 - 0.1, 24);
    let ro = grid(|k| 0.05 - (k as f64) * 0.01, 8);
    let r = residual(&law, &[], &h, &a, &ro);
    let with = b.with_residual(&r).unwrap();
    for j in 0..2usize {
        for i in 0..4usize {
            // the centre of texel (i, j) on the 2 x 1 tile
            let (u, v) = ((i as f64 + 0.5) / 4.0 * 2.0, (j as f64 + 0.5) / 2.0);
            let (law_s, got) = (b.at(u, v).unwrap(), with.at(u, v).unwrap());
            let t = j * 4 + i;
            assert_eq!(got.height, law_s.height + h[t], "height ({i}, {j})");
            for c in 0..3 {
                assert_eq!(
                    got.albedo[c],
                    law_s.albedo[c] + a[3 * t + c],
                    "albedo ({i}, {j}) {c}"
                );
            }
            assert_eq!(
                got.roughness,
                law_s.roughness + ro[t],
                "roughness ({i}, {j})"
            );
        }
    }
}

#[test]
fn between_centres_the_residual_is_bilinear_and_wraps() {
    let law = MaterialLaw::from_file(LAW).unwrap();
    let b = law.bind(&[]).unwrap();
    // height residual: texel (i, j) = i + 10 j (times 1e-4)
    let h = grid(|k| ((k % 4) as f64 + 10.0 * (k / 4) as f64) * 1e-4, 8);
    let r = residual(&law, &[], &h, &vec![0.0; 24], &vec![0.0; 8]);
    let with = b.with_residual(&r).unwrap();
    let res = |u: f64, v: f64| with.at(u, v).unwrap().height - b.at(u, v).unwrap().height;
    // halfway between texels (1, 0) and (2, 0), a quarter of the way to row 1:
    // 1.5 + 10 x 0.25 = 4, times 1e-4
    let (u, v) = (2.0 / 4.0 * 2.0, 0.25 + 0.25 * 0.5);
    assert!((res(u, v) - 4e-4).abs() < 1e-15, "{}", res(u, v));
    // between the last column (3) and the first (0) across the tile's edge: halfway gives 1.5
    let edge_u = 2.0; // the tile's edge, halfway between the centres of columns 3 and 0
    assert!(
        (res(edge_u, 0.25) - 1.5e-4).abs() < 1e-15,
        "{}",
        res(edge_u, 0.25)
    );
    // the same point one tile over gives the same sample, bit for bit (a law whose fields do not
    // read u, v, so the whole sample repeats with the tile)
    let flat = MaterialLaw::from_file(&LAW.replace("tilt * u", "tilt")).unwrap();
    let fb = flat.bind(&[]).unwrap();
    let fr = residual(&flat, &[], &h, &vec![0.0; 24], &vec![0.0; 8]);
    let fw = fb.with_residual(&fr).unwrap();
    for (du, dv) in [(2.0, 0.0), (0.0, -1.0), (4096.0, 3.0)] {
        assert_eq!(
            fw.at(0.75 + du, 0.25 + dv).unwrap(),
            fw.at(0.75, 0.25).unwrap(),
            "+({du}, {dv})"
        );
    }
}

#[test]
fn a_residual_that_leaves_the_range_names_the_field() {
    let law = MaterialLaw::from_file(LAW).unwrap();
    let b = law.bind(&[]).unwrap();
    // roughness 0.5 + 0.6 at every texel: 1.1, out of 0..=1
    let r = residual(&law, &[], &vec![0.0; 8], &vec![0.0; 24], &vec![0.6; 8]);
    match b.with_residual(&r).unwrap().at(0.3, 0.3) {
        Err(MaterialError::OutOfRange { field }) => assert_eq!(field, "roughness"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_texel_far_from_the_law_is_still_exact() {
    use alice_lol::law_residual::two_sum;
    // the reference is more than 2x from the law at every texel centre (outside Sterbenz): the
    // pair (hi, lo) = two_sum(f, -law) reproduces f exactly where a single f64 difference cannot
    let law =
        MaterialLaw::from_file(&LAW.replace("tilt * u", "3.115e-5 + tilt * u * 1e-3")).unwrap();
    let b = law.bind(&[]).unwrap();
    let centre = |t: usize| {
        (
            ((t % 4) as f64 + 0.5) / 4.0 * 2.0,
            ((t / 4) as f64 + 0.5) / 2.0,
        )
    };
    let refs: Vec<f64> = (0..8).map(|t| 1.455e-5 * (1.0 + t as f64 * 0.01)).collect();
    let hp: Vec<(f64, f64)> = (0..8)
        .map(|t| {
            let (u, v) = centre(t);
            two_sum(refs[t], -b.at(u, v).unwrap().height)
        })
        .collect();
    // at least one texel where the single difference is not exact (the case has teeth)
    let single_fails = (0..8).filter(|&t| {
        let (u, v) = centre(t);
        let l = b.at(u, v).unwrap().height;
        l + (refs[t] - l) != refs[t]
    });
    assert!(
        single_fails.count() > 0,
        "every single f64 difference is exact here"
    );
    let zero = |n: usize| pairs(&vec![(0.0, 0.0); n]);
    let r = LawResidual::from_bytes(&build(
        &law,
        &[],
        [pairs(&hp), zero(24), zero(8)],
        "f64x2",
        None,
    ))
    .unwrap();
    let with = b.with_residual(&r).unwrap();
    for t in 0..8 {
        let (u, v) = centre(t);
        assert_eq!(with.at(u, v).unwrap().height, refs[t], "texel {t}");
    }
}

#[test]
fn an_exact_single_f64_grid_is_refused_a_quantised_one_is_not() {
    let law = MaterialLaw::from_file(LAW).unwrap();
    let b = law.bind(&[]).unwrap();
    let f = |n: usize| f64s(&vec![0.001; n]);
    let exact =
        LawResidual::from_bytes(&build(&law, &[], [f(8), f(24), f(8)], "f64", None)).unwrap();
    assert!(matches!(
        b.with_residual(&exact),
        Err(MaterialError::Residual(_))
    ));
    let quantised = LawResidual::from_bytes(&build(
        &law,
        &[],
        [f(8), f(24), f(8)],
        "f64",
        Some("0.0001"),
    ))
    .unwrap();
    let with = b.with_residual(&quantised).unwrap();
    // a single f64 grid adds its value as is
    let (u, v) = (0.25, 0.25);
    assert_eq!(
        with.at(u, v).unwrap().roughness,
        b.at(u, v).unwrap().roughness + 0.001
    );
}
