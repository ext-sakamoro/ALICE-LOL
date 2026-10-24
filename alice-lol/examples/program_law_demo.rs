//! A LOL program as a law reference: name the shape by its program, hand on the bytes, rebuild
//! the shape from them (`lol.law.program/v1`, `alice_lol::program_law`)
//!
//! ```text
//! cargo run -p alice-lol --example program_law_demo
//! ```

use alice_lol::program_law::{program_law_ref, resolve_program, SOURCE_ROW};
use alice_zip::law_ref::{LawRef, Value};
use glam::Vec3;

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter()
        .fold(String::with_capacity(b.len() * 2), |mut s, x| {
            // String への write は失敗しない
            let _ = write!(s, "{x:02x}");
            s
        })
}

fn main() {
    let program = "sphere(10.0)";
    // the sender names the shape by its program (no mesh is sent)
    let sent = program_law_ref(program).expect("one row of this kind is always a valid reference");
    let bytes = sent.to_bytes();
    println!("program      {program}");
    println!("law id       {}", hex(sent.law_id()));
    println!("reference    {} bytes", bytes.len());

    // the receiver checks the identifier from the bytes alone, then rebuilds the shape
    let received = LawRef::from_bytes(&bytes).expect("the identifier is recomputed and matches");
    if let Some(Value::Str(s)) = received.get(SOURCE_ROW) {
        println!("source row   {s}");
    }
    let shape = resolve_program(&received).expect("this version's kind and semantics");
    for p in [
        Vec3::ZERO,
        Vec3::new(10.0, 0.0, 0.0),
        Vec3::new(20.0, 0.0, 0.0),
    ] {
        println!("f({:>4}, 0, 0) = {:>6.2}", p.x, alice_sdf::eval(&shape, p));
    }
}
