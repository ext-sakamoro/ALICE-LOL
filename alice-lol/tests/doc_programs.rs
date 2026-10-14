//! The LOL programs the documents show are programs: each one is extracted and run
//!
//! A fenced block in a tracked `*.md` (CHANGELOGs excepted, they record history) is a LOL
//! program when its info string starts with `lol`, and the second word says what it is:
//!
//! - `lol print`: a part meant to be printed; it parses, it is one bounded solid (some point is
//!   inside, and there is a box whose faces are all outside), and the export path
//!   (`print_export::node_to_mesh` at its default resolution, with its repair) gives a closed mesh
//!   (no boundary edge and no non-manifold edge) of at most 1M triangles
//! - `lol render`: a scene for the shader path (a ground `plane`, an infinite repeat); it parses
//! - `lol fragment`: not a whole program (a template with names standing for numbers, a fragment
//!   of a larger one); not run
//!
//! `print` and `render` programs have no `//` comment: the LLM grammar does not accept one, and the
//! documents are what a model copies (with `llm-bridge`, the grammar itself is asked, below)
//!
//! A fence that holds a LOL program but is not tagged, i.e. whose first call is a construct of
//! `language/constructs.toml`, is reported here as a warning; it becomes a failure in the next
//! change of this test
//!
//! Units and printability (wall thickness, size, overhang) are not checked here: those criteria
//! are measured by the printing pipeline and are not part of the language definition yet

use alice_lol::print_export::{node_to_mesh, validate_mesh, PrintConfig};
use alice_lol::runtime_parser::parse_program;
use alice_lol::{eval, SdfNode};
use glam::Vec3;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Print,
    Render,
    Fragment,
}

#[derive(Debug)]
struct Program {
    at: String,
    kind: Kind,
    src: String,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn documents() -> Vec<PathBuf> {
    let root = repo_root();
    let out = Command::new("git")
        .args(["-C", root.to_str().unwrap(), "ls-files", "-z", "--", "*.md"])
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    String::from_utf8(out.stdout)
        .unwrap()
        .split('\0')
        .filter(|p| !p.is_empty() && !p.rsplit('/').next().unwrap().starts_with("CHANGELOG"))
        .map(|p| root.join(p))
        .collect()
}

/// construct names of the language definition (`name = "…"` of each row)
fn construct_names() -> Vec<String> {
    let text =
        std::fs::read_to_string(repo_root().join("alice-lol/language/constructs.toml")).unwrap();
    let names: Vec<String> = text
        .lines()
        .filter_map(|l| {
            l.split_once("name = \"")
                .map(|(_, r)| r.split('"').next().unwrap().to_string())
        })
        .collect();
    assert!(names.len() > 100, "read {} construct names", names.len());
    names
}

fn first_call(body: &str) -> Option<&str> {
    let line = body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("//"))?;
    let end = line.find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))?;
    line[end..]
        .trim_start()
        .starts_with('(')
        .then_some(&line[..end])
}

/// (tagged programs, untagged fences whose first call is a construct)
fn fences() -> (Vec<Program>, Vec<String>) {
    let names = construct_names();
    let root = repo_root();
    let mut programs = Vec::new();
    let mut untagged = Vec::new();
    for doc in documents() {
        let text = std::fs::read_to_string(&doc).unwrap();
        let rel = doc.strip_prefix(&root).unwrap().display().to_string();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let t = lines[i].trim_start();
            let ticks = t.len() - t.trim_start_matches('`').len();
            if ticks < 3 {
                i += 1;
                continue;
            }
            let info = t[ticks..].trim();
            let fence = &t[..ticks];
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim_start().starts_with(fence) {
                j += 1;
            }
            let body = lines[i + 1..j.min(lines.len())].join("\n");
            let at = format!("{rel}:{}", i + 1);
            let mut words = info.split_whitespace();
            if words.next() == Some("lol") {
                let kind = match words.next() {
                    Some("print") => Kind::Print,
                    Some("render") => Kind::Render,
                    Some("fragment") => Kind::Fragment,
                    other => panic!(
                        "{at}: `lol` fence needs `print`, `render` or `fragment`, got {other:?}"
                    ),
                };
                programs.push(Program {
                    at,
                    kind,
                    src: body,
                });
            } else if first_call(&body)
                .is_some_and(|n| n == "program" || names.iter().any(|c| c == n))
            {
                untagged.push(at);
            }
            i = j + 1;
        }
    }
    (programs, untagged)
}

/// a box around the solid: the smallest of a few cubes whose surface samples are all clear of it
///
/// A sample whose distance is below half a cell diagonal may have surface inside its cell (a
/// distance field changes by at most the distance moved), so such a sample extends the box by a
/// cell and must not lie on the cube's surface; counting only the samples that are inside would
/// miss a wall thinner than the sample spacing and cut it off
fn bounding_box(node: &SdfNode) -> Result<(Vec3, Vec3), String> {
    const N: u8 = 33;
    const RADII: [f32; 7] = [2.0, 5.0, 12.0, 30.0, 80.0, 200.0, 500.0];
    for (which, r) in RADII.into_iter().enumerate() {
        let step = 2.0 * r / f32::from(N - 1);
        let reach = step * 3.0_f32.sqrt() / 2.0;
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let (mut near, mut near_face) = (0_u32, 0_u32);
        for i in 0..N {
            for j in 0..N {
                for k in 0..N {
                    let p = Vec3::splat(-r)
                        + step * Vec3::new(f32::from(i), f32::from(j), f32::from(k));
                    if eval(node, p) < reach {
                        near += 1;
                        lo = lo.min(p);
                        hi = hi.max(p);
                        if [i, j, k].iter().any(|&c| c == 0 || c == N - 1) {
                            near_face += 1;
                        }
                    }
                }
            }
        }
        if near > 0 && near_face == 0 {
            let pad = Vec3::splat(2.0 * step);
            return Ok((lo - pad, hi + pad));
        }
        if which + 1 == RADII.len() {
            return Err(if near == 0 {
                "no point is inside".into()
            } else {
                "not bounded within ±500".into()
            });
        }
    }
    unreachable!()
}

/// the checks every leg runs: no comment, and the program parses
fn check_text(p: &Program) -> Result<alice_lol::intent::Program, String> {
    if p.src.contains("//") {
        return Err("has a `//` comment (the grammar does not accept one)".into());
    }
    parse_program(&p.src).map_err(|e| format!("does not parse: {} at {}", e.message, e.position))
}

/// the export of a `print` program (heavy: run in release)
fn check_export(p: &Program) -> Result<(), String> {
    let program = check_text(p)?;
    let (lo, hi) = bounding_box(&program.sdf)?;
    // the export's own default resolution, the one a program is printed at unless the caller asks
    let config = PrintConfig {
        bounds_min: lo,
        bounds_max: hi,
        scale_mm: 1.0,
        ..PrintConfig::default()
    };
    let mesh = node_to_mesh(&program.sdf, &config);
    if mesh.indices.is_empty() {
        return Err("the export gives no triangles".into());
    }
    // the slicer limit the documents state; measured on these examples: at 128 the largest is 0.72M
    let triangles = mesh.indices.len() / 3;
    if triangles > 1_000_000 {
        return Err(format!(
            "the export has {triangles} triangles, over the 1M a slicer takes"
        ));
    }
    let v = validate_mesh(&mesh);
    if v.boundary_edges + v.non_manifold_edges > 0 {
        return Err(format!(
            "the exported mesh is not closed: {} boundary / {} non-manifold edges",
            v.boundary_edges, v.non_manifold_edges
        ));
    }
    Ok(())
}

/// every leg: the fences are found, tagged, comment-free and parse (cheap)
#[test]
fn every_program_in_the_documents_parses() {
    let (programs, untagged) = fences();
    for at in &untagged {
        eprintln!("warning: {at}: a fence holds a LOL program but is not tagged `lol print` / `lol render` / `lol fragment`");
    }
    let run: Vec<&Program> = programs
        .iter()
        .filter(|p| p.kind != Kind::Fragment)
        .collect();
    let prints = run.iter().filter(|p| p.kind == Kind::Print).count();
    eprintln!(
        "doc programs: {prints} print, {} render, {} fragment, {} untagged",
        run.len() - prints,
        programs.len() - run.len(),
        untagged.len()
    );
    assert!(
        prints > 0 && run.len() > prints,
        "no `lol print` or no `lol render` fence was found: the test compared nothing"
    );
    let failures: Vec<String> = run
        .iter()
        .filter_map(|p| check_text(p).err().map(|e| format!("{}: {e}", p.at)))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} programs fail:\n{}",
        failures.len(),
        run.len(),
        failures.join("\n")
    );
}

/// the export of every `print` program (a mesh each: run in release by the `doc-programs` CI job
/// and by `scripts/preflight.sh`, about 20 s there and several minutes in a debug build)
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "exports every documented print program: run in release (CI job doc-programs, preflight)"
)]
fn every_print_program_in_the_documents_exports_a_closed_mesh() {
    let (programs, _) = fences();
    let prints: Vec<&Program> = programs.iter().filter(|p| p.kind == Kind::Print).collect();
    assert!(
        !prints.is_empty(),
        "no `lol print` fence was found: the test exported nothing"
    );
    let failures: Vec<String> = prints
        .iter()
        .filter_map(|p| check_export(p).err().map(|e| format!("{}: {e}", p.at)))
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} print programs fail:\n{}",
        failures.len(),
        prints.len(),
        failures.join("\n")
    );
}

#[cfg(feature = "llm-bridge")]
#[test]
fn every_program_in_the_documents_is_accepted_by_the_grammar() {
    use alice_lol::bridge::{lol_grammar, Fsm, LOL_FSM_MAX_DEPTH};
    let (programs, _) = fences();
    let mut checked = 0;
    let mut rejected = Vec::new();
    for p in programs.iter().filter(|p| p.kind != Kind::Fragment) {
        // the grammar takes one whitespace character between tokens; documents are laid out for
        // reading, so runs are collapsed first (as `lol_gbnf_test` does for `examples/sword.lol`)
        let src = p.src.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut fsm = Fsm::start(lol_grammar())
            .unwrap()
            .with_max_depth(LOL_FSM_MAX_DEPTH);
        let ok = src.chars().all(|c| fsm.advance(c).is_ok()) && fsm.is_final();
        checked += 1;
        if !ok {
            rejected.push(p.at.clone());
        }
    }
    assert!(checked > 0, "the grammar was asked about no program");
    assert!(
        rejected.is_empty(),
        "the grammar rejects:\n{}",
        rejected.join("\n")
    );
}
