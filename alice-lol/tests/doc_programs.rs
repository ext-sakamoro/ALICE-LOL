//! The LOL programs the documents show are programs: each one is extracted and run
//!
//! A fenced block in a tracked `*.md` (CHANGELOGs excepted, they record history) is a LOL
//! program when its info string starts with `lol`, and the second word says what it is:
//!
//! - `lol print`: a part meant to be printed; it is declared `units(<length unit>, print, ...)`,
//!   it parses, and the export path for a declared program (`print_export::declared_config`: the
//!   unit's scale, the shape's tight box, which also refuses a shape that is empty or unbounded;
//!   then `node_to_mesh` at the default resolution, with its repair) gives a closed mesh (no
//!   boundary edge and no non-manifold edge) of at most 1M triangles that fits the build volume
//! - `lol render`: a scene for the shader path (a ground `plane`, an infinite repeat); it is
//!   declared `units(<unit>, render, ...)` and parses
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
//! The declaration's purpose is the fence's second word. Printability beyond the size (wall
//! thickness, overhang) is not checked here: those criteria are measured by the printing pipeline
//! and are not part of the language definition yet

use alice_lol::print_export::{declared_config, node_to_mesh, validate_mesh, PrintConfig};
use alice_lol::runtime_parser::{parse_declared, parse_program};
use alice_lol::units::{Declared, Purpose};
use alice_lol::SdfNode;
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

/// the build volume a `lol print` part must fit, in mm (the printer the print guide assumes,
/// `skills/lol-sdf/references/print-guide.md`: 315 × 310 × 315 with 5 mm margin per side)
const BUILD_VOLUME_MM: [f32; 3] = [305.0, 300.0, 305.0];

/// the checks every leg runs: no comment, a units declaration whose purpose is the fence's, and
/// the program parses
fn check_text(p: &Program) -> Result<Declared, String> {
    if p.src.contains("//") {
        return Err("has a `//` comment (the grammar does not accept one)".into());
    }
    let declared = parse_declared(&p.src)
        .map_err(|e| format!("does not parse: {} at {}", e.message, e.position))?;
    let Some(units) = declared.units else {
        return Err("has no units declaration (`units(<unit>, <purpose>, ...)`)".into());
    };
    let want = if p.kind == Kind::Print {
        Purpose::Print
    } else {
        Purpose::Render
    };
    if units.purpose != want {
        return Err(format!(
            "declares purpose `{}` in a `lol {}` fence",
            units.purpose.name(),
            want.name()
        ));
    }
    Ok(declared)
}

/// the export of a `print` program (heavy: run in release)
fn check_export(p: &Program) -> Result<(), String> {
    let declared = check_text(p)?;
    // the export's own default resolution, the one a program is printed at unless the caller asks
    let (config, _) =
        declared_config(&declared, &PrintConfig::default()).map_err(|e| e.to_string())?;
    let mesh = node_to_mesh(&declared.program.sdf, &config);
    if mesh.indices.is_empty() {
        return Err("the export gives no triangles".into());
    }
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for v in &mesh.vertices {
        lo = lo.min(v.position);
        hi = hi.max(v.position);
    }
    let mut size = (hi - lo).to_array();
    let mut volume = BUILD_VOLUME_MM;
    size.sort_by(f32::total_cmp);
    volume.sort_by(f32::total_cmp);
    if size.iter().zip(volume).any(|(s, v)| *s > v) {
        return Err(format!(
            "the part is {:.1} × {:.1} × {:.1} mm, it does not fit the build volume {BUILD_VOLUME_MM:?} mm",
            hi.x - lo.x,
            hi.y - lo.y,
            hi.z - lo.z
        ));
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

/// how many documents hold tagged programs, and how many programs of each kind they hold
const EXPECTED_DOCUMENTS_PRINT_RENDER_FRAGMENT: (usize, usize, usize, usize) = (5, 33, 6, 10);

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
    // the exact numbers, so a document that leaves the scan (renamed, moved, a fence's info
    // string broken) is a failure and not a smaller green run; a fence added, removed or
    // retagged on purpose updates them
    let documents: std::collections::BTreeSet<&str> = programs
        .iter()
        .map(|p| p.at.rsplit_once(':').unwrap().0)
        .collect();
    let counts = (
        documents.len(),
        prints,
        run.len() - prints,
        programs.len() - run.len(),
    );
    assert_eq!(
        counts, EXPECTED_DOCUMENTS_PRINT_RENDER_FRAGMENT,
        "(documents, print, render, fragment) changed; documents scanned: {documents:?}"
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

// ━━ resolved at the export resolution (A9) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// whether the export's topology does not depend on its grid: the Euler characteristic and the
/// number of separate shells are the same at the configured resolution and at 1.5 times it
fn resolved_at_export_resolution(node: &SdfNode, config: &PrintConfig) -> bool {
    let finer = PrintConfig {
        resolution: config.resolution * 3 / 2,
        ..config.clone()
    };
    topology(&node_to_mesh(node, config)) == topology(&node_to_mesh(node, &finer))
}

/// the representative of `x`'s set in a union-find forest (halving the path on the way)
const fn root(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

/// (Euler characteristic over the vertices a triangle uses, number of separate shells)
///
/// Written here rather than taken from `print_export`, so the test does not grade the export with
/// its own measure; the export welds vertices at the same position, so two triangles of one surface
/// share vertex indices
fn topology(mesh: &alice_lol::print_export::Mesh) -> (i64, usize) {
    use std::collections::BTreeSet;
    let n = mesh.vertices.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut used = vec![false; n];
    let mut edges = BTreeSet::new();
    for t in mesh.indices.chunks(3) {
        let [a, b, c] = [t[0] as usize, t[1] as usize, t[2] as usize];
        for (u, v) in [(a, b), (b, c), (c, a)] {
            edges.insert((u.min(v), u.max(v)));
            let (ru, rv) = (root(&mut parent, u), root(&mut parent, v));
            parent[ru] = rv;
        }
        used[a] = true;
        used[b] = true;
        used[c] = true;
    }
    let vertices = used.iter().filter(|&&u| u).count();
    let shells: BTreeSet<usize> = (0..n)
        .filter(|&i| used[i])
        .map(|i| root(&mut parent, i))
        .collect();
    let as_i64 = |x: usize| i64::try_from(x).unwrap();
    (
        as_i64(vertices) - as_i64(edges.len()) + as_i64(mesh.indices.len() / 3),
        shells.len(),
    )
}

fn default_box(node: &str) -> (SdfNode, PrintConfig) {
    (parse_program(node).unwrap().sdf, PrintConfig::default())
}

// oracle: a solid sphere / box is resolved by any grid finer than its size (one shell, χ = 2)
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "meshes at two resolutions: run in release (CI job doc-programs, preflight)"
)]
fn a_solid_sphere_and_box_are_resolved() {
    for src in ["sphere(1.0)", "box3d(1.0, 0.5, 0.25)"] {
        let (node, config) = default_box(src);
        assert!(resolved_at_export_resolution(&node, &config), "{src}");
    }
}

// oracle: with the default box ±2 at resolution 128 a cell is 4/128 = 0.031; a hollow sphere whose
// wall is 0.03 thick (onion 0.015) is thinner than a cell and comes out in pieces, one whose wall is
// 0.06 thick (onion 0.03) is two cells and is resolved (measured: χ −2036 / 21 shells at 128 against
// 4 / 2 at 192 for the first, 4 / 2 at both for the second)
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "meshes at two resolutions: run in release (CI job doc-programs, preflight)"
)]
fn a_wall_thinner_than_a_cell_is_not_resolved_and_one_of_two_cells_is() {
    let (thin, config) = default_box("onion(0.015, sphere(1.0))");
    assert!(
        !resolved_at_export_resolution(&thin, &config),
        "a 0.03 wall in a 0.031 cell"
    );
    let (two_cells, config) = default_box("onion(0.03, sphere(1.0))");
    assert!(
        resolved_at_export_resolution(&two_cells, &config),
        "a 0.06 wall in a 0.031 cell"
    );
}

// oracle: the number of shells is compared, not only χ — a torus whose hole (0.01 wide, its axis
// between grid points) is closed at 128 and open at 192 (χ 2 → 0) beside a sphere of radius 0.006
// that only 192 samples (+1 shell, χ +2) has χ 2 at both resolutions and 1 against 2 shells
// (measured with the default box ±2; a χ-only comparison calls it resolved)
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "meshes at two resolutions: run in release (CI job doc-programs, preflight)"
)]
fn a_change_in_the_number_of_shells_alone_is_not_resolved() {
    let (node, config) = default_box(
        "union(translate(0.0155, -1.0, 0.0155, torus(0.31, 0.3)), translate(0.0, 0.0, 1.4, sphere(0.006)))",
    );
    let finer = PrintConfig {
        resolution: config.resolution * 3 / 2,
        ..config
    };
    let (coarse, fine) = (
        topology(&node_to_mesh(&node, &config)),
        topology(&node_to_mesh(&node, &finer)),
    );
    assert_eq!((coarse.0, fine.0), (2, 2), "the scene keeps χ equal");
    assert_ne!(coarse.1, fine.1, "the scene changes the number of shells");
    assert!(!resolved_at_export_resolution(&node, &config));
}

/// the `lol print` examples whose export is not resolved at the default resolution (A9), each as
/// (document, n-th `lol print` fence of that document, from 1); a warning until they are fixed —
/// the examples are fixed by their parameters or the resolution policy, never by weakening the
/// check — so the list only shrinks: a new entry or a stale one fails
const KNOWN_UNRESOLVED: &[(&str, usize)] = &[
    ("LLM_PRINT_PROMPT.md", 2),
    ("LLM_PRINT_PROMPT.md", 3),
    ("LLM_PRINT_PROMPT.md", 5),
    ("LLM_PRINT_PROMPT.md", 6),
    ("LLM_REFERENCE.md", 2),
    ("LLM_REFERENCE.md", 14),
    ("LLM_REFERENCE.md", 15),
];

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "meshes every print program at two resolutions: run in release (CI job doc-programs, preflight)"
)]
fn the_unresolved_print_programs_are_the_known_ones() {
    let (programs, _) = fences();
    let mut nth: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut unresolved: Vec<(String, usize, String)> = Vec::new();
    let mut checked = 0;
    for p in programs.iter().filter(|p| p.kind == Kind::Print) {
        let doc = p.at.rsplit_once(':').unwrap().0.to_string();
        let n = *nth.entry(doc.clone()).and_modify(|n| *n += 1).or_insert(1);
        let declared = check_text(p).unwrap();
        let (config, _) = declared_config(&declared, &PrintConfig::default()).unwrap();
        checked += 1;
        if !resolved_at_export_resolution(&declared.program.sdf, &config) {
            unresolved.extend([(doc, n, p.at.clone())]);
        }
    }
    assert!(
        checked > 0,
        "no `lol print` fence was found: nothing was compared"
    );
    for (_, _, at) in &unresolved {
        eprintln!("warning: {at}: the export's topology changes between resolution 128 and 192 (a feature thinner than the grid)");
    }
    let found: Vec<(String, usize)> = unresolved.iter().map(|(d, n, _)| (d.clone(), *n)).collect();
    let known: Vec<(String, usize)> = KNOWN_UNRESOLVED
        .iter()
        .map(|(d, n)| ((*d).to_string(), *n))
        .collect();
    let new: Vec<_> = found.iter().filter(|f| !known.contains(f)).collect();
    let stale: Vec<_> = known.iter().filter(|k| !found.contains(k)).collect();
    eprintln!("A9: {} of {checked} print programs unresolved", found.len());
    assert!(
        new.is_empty() && stale.is_empty(),
        "newly unresolved: {new:?}\nlisted but now resolved (remove from KNOWN_UNRESOLVED): {stale:?}"
    );
}

/// the size the documents' units baseline gives a part, pinned on one example so the baseline cannot
/// drift: the coaster of `LLM_REFERENCE.md` ("10cm round and 5mm thick") is exported at 100 mm across
/// and 5 mm thick (oracle: the prose; 0.4.x read the same program at its 10 mm/unit preset, half this)
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "exports the coaster: run in release (CI job doc-programs, preflight)"
)]
fn the_coaster_example_is_exported_at_the_size_its_prose_states() {
    let (programs, _) = fences();
    let coaster: Vec<&Program> = programs
        .iter()
        .filter(|p| {
            p.kind == Kind::Print
                && p.src.contains("hex_prism(5, 2)")
                && p.at.starts_with("LLM_REFERENCE.md:")
        })
        .collect();
    assert_eq!(
        coaster.len(),
        1,
        "the coaster fence of LLM_REFERENCE.md: {coaster:?}"
    );
    let declared = check_text(coaster[0]).unwrap();
    let (config, _) = declared_config(&declared, &PrintConfig::default()).unwrap();
    let mesh = node_to_mesh(&declared.program.sdf, &config);
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for v in &mesh.vertices {
        lo = lo.min(v.position);
        hi = hi.max(v.position);
    }
    let size = hi - lo;
    let cell = (config.bounds_max - config.bounds_min).max_element() * config.scale_mm
        / f32::from(u16::try_from(config.resolution).unwrap());
    for (got, want, what) in [
        (size.x, 100.0, "across x"),
        (size.z, 100.0, "across z"),
        (size.y, 5.0, "thick"),
    ] {
        assert!(
            (got - want).abs() <= 2.0 * cell,
            "the coaster is {got} mm {what}, the prose says {want} mm (cell {cell})"
        );
    }
}
