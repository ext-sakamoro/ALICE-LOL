//! The residual of a law (`alice_lol::law_residual`): the exact difference between a law and the
//! reference it was fitted to, in an envelope that does not depend on the kind of law
//!
//! A residual file is a text header and a little-endian payload:
//! ```text
//! residual <name>
//! for <kind tag> <law_id, 64 hex>
//! bound <sha256 of the kind's canonical bound law without the transported and selector params>
//! frame world | sheet-local
//! [transport <frame> over <param> ...]
//! index <domain> <words ...>            (one per domain)
//! between <domain> <rule words ...>     (one per domain)
//! [quantum <x>]
//! block <name> f64 <domain> <dims ...> <sha256> [when <param>=<value>]
//! block <name> sparse <domain> <count> <components> <sha256> [when <param>=<value>]
//! payload
//! <each block's bytes in block order: f64 values; sparse = count u32 indices, then the values>
//! ```
//! A domain has as many elements as the product of the numbers among its index words (`grid 2 3`:
//! 6, `mesh vertices 10`: 10); a dense block holds a whole number of values per element, a sparse
//! block's indices are below the element count. With quantum `q` a value `r` costs
//! `1 + max(0, floor(log2(|r| / q)))` bits (the exponent, no libm), without one 64; a sparse index
//! costs 32.
//! Checked here by hand with a made-up kind of law (`Toy`, below): reading and writing, the
//! identifier, the refusals, which law (and which values) a residual belongs to, the selector
//! blocks, and the size in bits.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::collections::BTreeMap;

use alice_lol::law_residual::{bound_hash, Block, LawResidual, ResidualError, ResidualOwner};
use sha2::{Digest, Sha256};

const KIND: &str = "lol.law.example/v1";
const LAW: [u8; 32] = [7; 32];

/// A made-up kind of law: named parameters, islands each with a form, a part selector and two
/// index domains (a grid and a mesh)
#[derive(Clone)]
struct Toy {
    kind: &'static str,
    law: [u8; 32],
    params: BTreeMap<&'static str, f64>,
    forms: Vec<&'static str>,
    part: &'static str,
    grid: [usize; 2],
    verts: usize,
    /// the blocks that are differences added to the law's values
    additive: Vec<&'static str>,
}

impl Toy {
    fn new() -> Self {
        Self {
            kind: KIND,
            law: LAW,
            params: [("size", 0.25), ("cheek", 0.4)].into_iter().collect(),
            forms: vec!["plain", "cylindrical", "plain"],
            part: "smile",
            grid: [2, 3],
            verts: 10,
            additive: Vec::new(),
        }
    }
}

impl ResidualOwner for Toy {
    fn kind_tag(&self) -> &str {
        self.kind
    }
    fn law_id(&self) -> [u8; 32] {
        self.law
    }
    /// every parameter and the selector by name, and every island's form, but the excluded names
    fn canonical_bound(&self, excluded: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for (n, v) in &self.params {
            if !excluded.contains(n) {
                out.extend(n.as_bytes());
                out.extend(v.to_bits().to_le_bytes());
            }
        }
        if !excluded.contains(&"part") {
            out.extend(b"part");
            out.extend(self.part.as_bytes());
        }
        for (i, f) in self.forms.iter().enumerate() {
            out.extend((i as u64).to_le_bytes());
            out.extend(f.as_bytes());
        }
        out
    }
    fn index(&self, domain: &str) -> Option<Vec<String>> {
        match domain {
            "grid" => Some(vec![self.grid[0].to_string(), self.grid[1].to_string()]),
            "mesh" => Some(vec!["vertices".into(), self.verts.to_string()]),
            _ => None,
        }
    }
    fn selector(&self, param: &str) -> Option<String> {
        (param == "part").then(|| self.part.to_owned())
    }
    fn additive_block(&self, name: &str) -> bool {
        self.additive.contains(&name)
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn f64s(v: &[f64]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

const GRID: [f64; 6] = [0.5, -0.25, 0.0, 1.0, -0.0, 2.0];
// sparse: vertices 1 and 7, 3 components each
const SPARSE_IDX: [u32; 2] = [1, 7];
const SPARSE_VAL: [f64; 6] = [0.125, 0.0, -4.0, 0.0, 0.25, 0.5];
const PART_SMILE: [f64; 2] = [0.25, -0.5];
const PART_FROWN: [f64; 2] = [1.0, 1.0];

/// The residual file for `toy`, by hand: a grid block, a sparse mesh block and two part blocks
fn file(toy: &Toy, grid: &[f64], quantum: Option<&str>) -> Vec<u8> {
    let excluded = ["cheek", "part"];
    let bound: [u8; 32] = Sha256::digest(toy.canonical_bound(&excluded)).into();
    let sparse: Vec<u8> = SPARSE_IDX
        .iter()
        .flat_map(|i| i.to_le_bytes())
        .chain(f64s(&SPARSE_VAL))
        .collect();
    let part =
        |i: u32, v: &[f64]| -> Vec<u8> { i.to_le_bytes().into_iter().chain(f64s(v)).collect() };
    let (g, s, a, b) = (
        f64s(grid),
        sparse,
        part(3, &PART_SMILE),
        part(4, &PART_FROWN),
    );
    let mut h = format!(
        "residual toy_fit\nfor {KIND} {}\nbound {}\nframe sheet-local\ntransport sheet-local over cheek\n\
         index grid {} {}\nindex mesh vertices {}\nbetween grid nearest\nbetween mesh barycentric\n",
        hex(&LAW),
        hex(&bound),
        toy.grid[0],
        toy.grid[1],
        toy.verts
    );
    if let Some(q) = quantum {
        h += &format!("quantum {q}\n");
    }
    h += &format!("block g f64 grid 2 3 {}\n", hex(&Sha256::digest(&g)));
    h += &format!("block skin sparse mesh 2 3 {}\n", hex(&Sha256::digest(&s)));
    h += &format!(
        "block mouth sparse mesh 1 2 {} when part=smile\n",
        hex(&Sha256::digest(&a))
    );
    h += &format!(
        "block mouth_frown sparse mesh 1 2 {} when part=frown\n",
        hex(&Sha256::digest(&b))
    );
    h += "payload\n";
    let mut out = h.into_bytes();
    for x in [g, s, a, b] {
        out.extend(x);
    }
    out
}

fn read(toy: &Toy) -> LawResidual {
    LawResidual::from_bytes(&file(toy, &GRID, None)).unwrap()
}

#[test]
fn a_residual_reads_and_writes_the_same_bytes() {
    let toy = Toy::new();
    let raw = file(&toy, &GRID, None);
    let r = LawResidual::from_bytes(&raw).unwrap();
    assert_eq!((r.name(), r.kind()), ("toy_fit", KIND));
    assert_eq!(r.law_id(), LAW);
    assert_eq!(r.frame(), "sheet-local");
    assert_eq!(
        r.excluded_params(),
        ["cheek", "part"],
        "transported and selector params, sorted"
    );
    assert_eq!(r.index("grid").unwrap(), ["2", "3"]);
    assert_eq!(r.index("mesh").unwrap(), ["vertices", "10"]);
    assert_eq!(r.between("mesh").unwrap(), ["barycentric"]);
    match r.block("g").unwrap() {
        Block::Dense { shape, values, .. } => {
            assert_eq!(shape, &[2, 3]);
            assert_eq!(values, &GRID);
        }
        Block::Sparse { .. } => panic!("g is dense"),
    }
    match r.block("skin").unwrap() {
        Block::Sparse {
            indices,
            components,
            values,
            ..
        } => {
            assert_eq!(indices, &SPARSE_IDX);
            assert_eq!(*components, 3);
            assert_eq!(values, &SPARSE_VAL);
        }
        Block::Dense { .. } => panic!("skin is sparse"),
    }
    assert_eq!(
        r.to_bytes(),
        raw,
        "the canonical writer gives the hand-written file back"
    );
}

#[test]
fn the_identifier_is_the_canonical_header_and_every_payload_byte() {
    let toy = Toy::new();
    let raw = file(&toy, &GRID, None);
    let id = LawResidual::from_bytes(&raw).unwrap().residual_id();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (h, _) = text.split_once("payload\n").unwrap();
    let mut spaced = h
        .replace("index grid 2 3", "index   grid 2  3")
        .into_bytes();
    spaced.extend(b"payload\n");
    spaced.extend(&raw[raw.len() - (raw.len() - h.len() - 8)..]);
    assert_eq!(
        LawResidual::from_bytes(&spaced).unwrap().residual_id(),
        id,
        "spacing"
    );
    let mut g2 = GRID;
    g2[4] = 0.0; // -0.0 -> +0.0, one bit
    let mut other_toy = Toy::new();
    other_toy.params.insert("size", 0.5);
    for other in [
        file(&toy, &g2, None),
        file(&other_toy, &GRID, None),
        file(&toy, &GRID, Some("0.001")),
    ] {
        assert_ne!(LawResidual::from_bytes(&other).unwrap().residual_id(), id);
    }
}

#[test]
fn files_that_are_not_residuals_are_refused() {
    let toy = Toy::new();
    let good = file(&toy, &GRID, None);
    let edit = |f: &dyn Fn(&str) -> String| {
        let s = String::from_utf8_lossy(&good).into_owned();
        let (h, _) = s.split_once("payload\n").unwrap();
        let tail = good[h.len() + 8..].to_vec();
        let mut out = f(&format!("{h}payload\n")).into_bytes();
        out.extend(tail);
        out
    };
    let mut short = good.clone();
    short.pop();
    let mut long = good.clone();
    long.push(0);
    let mut flipped = good.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 1;
    let mut nan = GRID;
    nan[0] = f64::NAN;
    for (bad, why) in [
        (short, "payload one byte short"),
        (long, "payload one byte long"),
        (flipped, "a block whose bytes are not its sha"),
        (file(&toy, &nan, None), "a value that is not finite"),
        (
            edit(&|h| h.replace("residual toy_fit\n", "")),
            "no residual line",
        ),
        (edit(&|h| h.replacen("for ", "fr ", 1)), "no for line"),
        (
            edit(&|h| h.replace("frame sheet-local\n", "")),
            "no frame line",
        ),
        (
            edit(&|h| h.replace("frame sheet-local", "frame sideways")),
            "an unknown frame",
        ),
        (
            edit(&|h| h.replace("index mesh vertices 10\n", "")),
            "a block's domain not declared",
        ),
        (
            edit(&|h| h.replace("index grid 2 3\n", "index grid 2 3\nindex grid 4 4\n")),
            "a domain twice",
        ),
        (
            edit(&|h| h.replace("between mesh barycentric\n", "")),
            "a domain without a between rule",
        ),
        (
            edit(&|h| h.replace(" f64 grid 2 3 ", " f32 grid 2 3 ")),
            "an unknown element type",
        ),
        (
            edit(&|h| h.replace(" f64 grid 2 3 ", " f64 grid 3 3 ")),
            "a shape that is not the block's length",
        ),
        (
            edit(&|h| h.replace(" f64 grid 2 3 ", " f64 grid 0 6 ")),
            "a zero dimension",
        ),
        (
            edit(&|h| h.replace("block skin ", "block g ")),
            "a block name twice",
        ),
        (
            edit(&|h| h.replace(" when part=smile", " when part")),
            "a when without a value",
        ),
        (
            edit(&|h| {
                h.replace("quantum", "quantum").replace(
                    "between grid nearest\n",
                    "between grid nearest\nquantum -1\n",
                )
            }),
            "a negative quantum",
        ),
        (
            edit(&|h| {
                h.replace(
                    "between grid nearest\n",
                    "between grid nearest\nmystery 1\n",
                )
            }),
            "an unknown line",
        ),
        (Vec::new(), "empty"),
    ] {
        assert!(LawResidual::from_bytes(&bad).is_err(), "{why} read");
    }
    // a sparse index outside its domain is refused when the residual meets its law
    let mut big = Toy::new();
    big.verts = 7; // vertex 7 is now out of range
    let r = LawResidual::from_bytes(&file(&big, &GRID, None)).unwrap();
    assert!(
        r.check_belongs(&big).is_err(),
        "a sparse index past the domain accepted"
    );
    let five: Vec<u8> = f64s(&GRID[..5]);
    let odd = edit(&|h| {
        h.replace(
            &format!(" f64 grid 2 3 {}", hex(&Sha256::digest(f64s(&GRID)))),
            &format!(" f64 grid 5 {}", hex(&Sha256::digest(&five))),
        )
    });
    // drop the sixth value's 8 bytes from the payload so only the shape rule can refuse it
    let cut = odd.len()
        - (good.len() - (String::from_utf8_lossy(&good).find("payload\n").unwrap() + 8))
        + 5 * 8;
    let mut odd = odd;
    odd.drain(cut..cut + 8);
    assert!(
        LawResidual::from_bytes(&odd).is_err(),
        "5 values on a 6-element domain read"
    );
    let none = format!(
        "residual e\nfor {KIND} {}\nbound {}\nframe world\nindex grid 2 3\nbetween grid nearest\npayload\n",
        hex(&LAW),
        hex(&[0; 32])
    );
    assert!(matches!(
        LawResidual::from_bytes(none.as_bytes()),
        Err(ResidualError::NoBlock)
    ));
}

#[test]
fn a_residual_belongs_to_one_law_at_one_set_of_values() {
    let toy = Toy::new();
    let r = read(&toy);
    assert!(r.check_belongs(&toy).is_ok());
    // a structural key (one island's form): another law instance, refused
    let mut form = toy.clone();
    form.forms[2] = "cylindrical";
    assert!(
        r.check_belongs(&form).is_err(),
        "a flipped island form accepted"
    );
    // an ordinary parameter: refused
    let mut size = toy.clone();
    size.params.insert("size", 0.26);
    assert!(r.check_belongs(&size).is_err(), "another size accepted");
    // another kind or another law: refused
    let mut kind = toy.clone();
    kind.kind = "lol.law.other/v1";
    assert!(r.check_belongs(&kind).is_err(), "another kind accepted");
    let mut law = toy.clone();
    law.law = [8; 32];
    assert!(r.check_belongs(&law).is_err(), "another law accepted");
    // a transported parameter: accepted (the residual follows the frame)
    let mut cheek = toy.clone();
    cheek.params.insert("cheek", 0.9);
    assert!(
        r.check_belongs(&cheek).is_ok(),
        "a transported slider refused"
    );
    // a selector: accepted
    let mut frown = toy.clone();
    frown.part = "frown";
    assert!(r.check_belongs(&frown).is_ok(), "a selector change refused");
    // another domain size: refused, per domain
    let mut grid = toy.clone();
    grid.grid = [3, 2];
    assert!(r.check_belongs(&grid).is_err(), "another grid accepted");
    let mut mesh = toy.clone();
    mesh.verts = 11;
    assert!(r.check_belongs(&mesh).is_err(), "another mesh accepted");
    // the bound line is the hash of the canonical bound law without the excluded params
    assert_eq!(
        r.bound(),
        bound_hash(&toy.canonical_bound(&["cheek", "part"]))
    );
}

#[test]
fn a_selector_swaps_the_part_blocks_and_keeps_the_others() {
    let toy = Toy::new();
    let r = read(&toy);
    let names = |t: &Toy| -> Vec<String> {
        r.active_blocks(t)
            .unwrap()
            .iter()
            .map(|b| b.name().to_owned())
            .collect()
    };
    assert_eq!(names(&toy), ["g", "skin", "mouth"]);
    let mut frown = toy.clone();
    frown.part = "frown";
    assert_eq!(names(&frown), ["g", "skin", "mouth_frown"]);
    let mut none = toy.clone();
    none.part = "neutral";
    assert_eq!(
        names(&none),
        ["g", "skin"],
        "no part block for a value no block names"
    );
}

#[test]
fn the_size_in_bits_follows_the_mdl_rule() {
    let toy = Toy::new();
    // without a quantum every value is a full f64: 64 bits (a sparse block also pays 32 per index)
    let exact = read(&toy);
    assert_eq!(exact.block_bits("g"), Some(6 * 64));
    assert_eq!(exact.block_bits("skin"), Some(2 * 32 + 6 * 64));
    // with quantum q a value r costs 1 + max(0, log2(|r| / q)) bits (q = 0.125, by hand)
    // g: 0.5 -> 3, -0.25 -> 2, 0 -> 1, 1 -> 4, -0 -> 1, 2 -> 5            = 16
    // skin: 2 x 32 + (0.125 -> 1, 0 -> 1, -4 -> 6, 0 -> 1, 0.25 -> 2, 0.5 -> 3) = 64 + 14
    // mouth: 32 + (0.25 -> 2, -0.5 -> 3) = 37; mouth_frown: 32 + (1 -> 4, 1 -> 4) = 40
    let q = LawResidual::from_bytes(&file(&toy, &GRID, Some("0.125"))).unwrap();
    assert_eq!(q.block_bits("g"), Some(16));
    assert_eq!(q.block_bits("skin"), Some(78));
    assert_eq!(q.block_bits("mouth"), Some(37));
    assert_eq!(q.total_bits(), 16 + 78 + 37 + 40);
    assert_eq!(q.block_bits("nope"), None);
}

#[test]
fn the_compensated_add_is_exact_where_a_single_f64_is_not() {
    use alice_lol::law_residual::{compensated_add, two_sum};
    // law and reference more than 2x apart (outside Sterbenz): f - law is rounded
    let (law, f) = (3.115e-5_f64, 1.455e-5_f64);
    let r = f - law;
    assert_ne!(
        law + r,
        f,
        "the single f64 residual reproduces f here: the case has no teeth"
    );
    let (hi, lo) = two_sum(f, -law);
    assert_eq!(compensated_add(law, hi, lo), f);
    // two_sum is exact: hi + lo is f - law with no rounding (hi is the rounded sum, lo its error)
    assert_eq!(hi, r);
    assert_ne!(lo, 0.0);
    // many pairs, scales apart: always exact
    for k in 1..2000u32 {
        let law = f64::from(k).sqrt() * 1e-5 * if k % 3 == 0 { -1.0 } else { 1.0 };
        let f = law * (0.01 + f64::from(k % 97) / 13.0) + f64::from(k % 7) * 1e-9;
        let (hi, lo) = two_sum(f, -law);
        assert_eq!(compensated_add(law, hi, lo), f, "law {law} f {f}");
    }
}

#[test]
fn a_pair_block_reads_writes_and_counts_both_halves() {
    let toy = Toy::new();
    let pairs = [0.5, 1e-17, -0.25, 0.0, 2.0, -3e-18]; // 3 values on the 3-element... grid of 2 x 3
    let all: Vec<f64> = pairs.iter().copied().chain(pairs).collect(); // 6 pairs = 12 numbers
    let bytes = f64s(&all);
    let bound: [u8; 32] = Sha256::digest(toy.canonical_bound(&["cheek", "part"])).into();
    let mut h = format!(
        "residual pairs\nfor {KIND} {}\nbound {}\nframe world\nindex grid 2 3\nbetween grid nearest\nquantum 0.125\n",
        hex(&LAW),
        hex(&bound)
    );
    h += &format!(
        "block g f64x2 grid 2 3 {}\npayload\n",
        hex(&Sha256::digest(&bytes))
    );
    let mut raw = h.into_bytes();
    raw.extend(bytes);
    let r = LawResidual::from_bytes(&raw).unwrap();
    assert_eq!(r.to_bytes(), raw);
    assert_eq!(r.block("g").unwrap().values(), all);
    // q = 0.125: 0.5 -> 3, 1e-17 -> 1, -0.25 -> 2, 0 -> 1, 2 -> 5, -3e-18 -> 1 = 13, twice
    assert_eq!(r.block_bits("g"), Some(26));
    // a pair block holds two numbers per value: 6 numbers is 3 values, not the grid's 6
    let short = f64s(&pairs);
    let h2 = String::from_utf8_lossy(&raw[..raw.len() - 8 * 12]).replace(
        &hex(&Sha256::digest(f64s(&all))),
        &hex(&Sha256::digest(&short)),
    );
    let mut bad = h2.into_bytes();
    bad.extend(short);
    assert!(
        LawResidual::from_bytes(&bad).is_err(),
        "half a pair block read"
    );
}

#[test]
fn an_exact_additive_block_must_be_a_pair() {
    // the kind says block g is a difference added to the law: without a quantum a single f64 per
    // value cannot always be exact, so it is refused; with a quantum it is allowed
    let mut toy = Toy::new();
    toy.additive = vec!["g"];
    let exact = LawResidual::from_bytes(&file(&toy, &GRID, None)).unwrap();
    assert!(
        exact.check_belongs(&toy).is_err(),
        "an exact single-f64 additive block accepted"
    );
    let quantised = LawResidual::from_bytes(&file(&toy, &GRID, Some("0.125"))).unwrap();
    assert!(quantised.check_belongs(&toy).is_ok());
    // the same blocks when the kind says they are data: accepted
    assert!(exact.check_belongs(&Toy::new()).is_ok());
}
