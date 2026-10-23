//! Check a layout law and bytes against it from the command line
//!
//! ```text
//! layout_check <law file>                 # the law as Markdown, and its identifier
//! layout_check <law file> <bytes file>    # decode: the status and every segment, or the refusal
//! layout_check --semantics                # the behaviour pin measured here against the built-in one
//! ```
//!
//! Exit status: 0 read / matched, 1 refused / differs, 2 usage or I/O error.

use std::process::ExitCode;

use alice_lol_layout::{
    decode, fold_pins, parse, semantics_probe, Budget, LAYOUT_SEMANTICS_ID, LAYOUT_SEMANTICS_PINS,
};

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn semantics() -> ExitCode {
    let measured = semantics_probe();
    let pinned = LAYOUT_SEMANTICS_PINS
        .iter()
        .find(|(n, _)| *n == "decode-probe/v1")
        .map(|(_, v)| *v);
    println!("decode-probe/v1 measured {}", hex(&measured));
    let folded = fold_pins(&[("decode-probe/v1", measured)]);
    println!("LAYOUT_SEMANTICS_ID measured {}", hex(&folded));
    if pinned == Some(measured) && folded == LAYOUT_SEMANTICS_ID {
        println!("matches the pinned semantics");
        ExitCode::SUCCESS
    } else {
        println!(
            "differs from the pinned semantics {}",
            hex(&LAYOUT_SEMANTICS_ID)
        );
        ExitCode::from(1)
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag] if flag == "--semantics" => semantics(),
        [law] => match read_law(law) {
            Ok(l) => {
                print!("{}", l.markdown());
                println!("\nlaw id {}", hex(&l.law_id()));
                ExitCode::SUCCESS
            }
            Err(code) => code,
        },
        [law, input] => {
            let l = match read_law(law) {
                Ok(l) => l,
                Err(code) => return code,
            };
            let bytes = match std::fs::read(input) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("{input}: {e}");
                    return ExitCode::from(2);
                }
            };
            match decode(&l, &bytes, Budget::for_input(bytes.len())) {
                Ok(d) => {
                    println!("read, status {}", d.status().unwrap_or("(none)"));
                    for s in &l.segments {
                        let len = d.segment_bytes(&s.name).map_or(0, <[u8]>::len);
                        let parts = d.runs(&s.name).max(d.rows(&s.name));
                        println!("segment {} {len} bytes, {parts} rows / runs", s.name);
                    }
                    for v in &l.derives {
                        if let Some(x) = d.derived(&v.name) {
                            println!("derive {} {}", v.name, hex(&x));
                        }
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    println!("{e}");
                    ExitCode::from(1)
                }
            }
        }
        _ => {
            eprintln!("usage: layout_check <law file> [<bytes file>] | --semantics");
            ExitCode::from(2)
        }
    }
}

fn read_law(path: &str) -> Result<alice_lol_layout::Layout, ExitCode> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("{path}: {e}");
        ExitCode::from(2)
    })?;
    parse(&text).map_err(|e| {
        eprintln!("{path}: {e}");
        ExitCode::from(2)
    })
}
