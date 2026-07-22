//! End-to-end demo: prompt → LOL DSL → `SdfNode` → optional STL
//! (Phase X.8 B-8).
//!
//! Wraps [`alice_lol::bridge::generate_sdf_from_prompt`] with a small
//! CLI so downstream can see the full high-level pipeline in one file.
//! The generated `SdfNode` is always dumped; passing `--stl <path>`
//! also writes a printable mesh via [`alice_lol::print_export::node_to_stl`].
//!
//! # Usage
//!
//! ```bash
//! cargo run --example prompt_to_sword --features llm-bridge -- \
//!     --model  path/to/model.gguf \
//!     [--prompt "a chess knight for 3D print"] \
//!     [--max-tokens 256] \
//!     [--stl output.stl]
//! ```
//!
//! The GGUF file is not shipped; download separately. Real-model smoke
//! runs on Mac Metal / Jetson Vulkan are covered by Phase X.8 B-9.

use alice_llm::gguf::GgufFile;
use alice_lol::bridge::{generate_sdf_from_prompt, GgufTokenizer, Llama3Model};
use alice_lol::print_export::{node_to_stl, PrintConfig};
use std::env;
use std::fs;
use std::process;
use std::time::Instant;

const DEFAULT_PROMPT: &str = "a chess knight for 3D print";

fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn parse_arg<T: std::str::FromStr>(args: &[String], flag: &str) -> Option<T> {
    arg_after(args, flag).and_then(|s| s.parse().ok())
}

fn usage_and_exit(msg: &str) -> ! {
    eprintln!("error: {msg}");
    eprintln!();
    eprintln!("Usage:");
    eprintln!(
        "  cargo run --example prompt_to_sword --features llm-bridge -- \\\n\
         \x20   --model <path.gguf> \\\n\
         \x20   [--prompt \"<text>\"] [--max-tokens 256] [--stl <output.stl>]"
    );
    process::exit(2);
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let model_path =
        arg_after(&args, "--model").unwrap_or_else(|| usage_and_exit("--model missing"));
    let prompt = arg_after(&args, "--prompt").unwrap_or(DEFAULT_PROMPT);
    let max_new_tokens: usize = parse_arg(&args, "--max-tokens").unwrap_or(256);
    let stl_path = arg_after(&args, "--stl");

    // --- Load model + tokenizer ---
    println!("Loading GGUF: {model_path}");
    let t0 = Instant::now();
    let data = fs::read(model_path).unwrap_or_else(|e| {
        eprintln!("failed to read GGUF file {model_path}: {e}");
        process::exit(1);
    });
    let gguf = GgufFile::parse(&data).unwrap_or_else(|| {
        eprintln!("failed to parse GGUF (malformed header or truncated)");
        process::exit(1);
    });
    let tokenizer = GgufTokenizer::from_gguf(&gguf).unwrap_or_else(|| {
        eprintln!("failed to load tokenizer from GGUF");
        process::exit(1);
    });
    let mut model = Llama3Model::from_gguf(&gguf).unwrap_or_else(|| {
        eprintln!("failed to load model from GGUF");
        process::exit(1);
    });
    println!(
        "  loaded in {}ms — vocab={}",
        t0.elapsed().as_millis(),
        tokenizer.vocab_size()
    );

    // --- Generate SdfNode via the bridge one-shot API ---
    println!();
    println!("Prompt: {prompt:?}");
    println!("max_new_tokens: {max_new_tokens}");
    println!();

    let gen_start = Instant::now();
    let node = generate_sdf_from_prompt(&mut model, &tokenizer, prompt, max_new_tokens)
        .unwrap_or_else(|e| {
            eprintln!("generate_sdf_from_prompt failed: {e}");
            process::exit(1);
        });
    println!("generated SdfNode in {}ms", gen_start.elapsed().as_millis());
    println!();
    println!("--- SdfNode (Debug) ---");
    println!("{node:#?}");
    println!("--- /SdfNode ---");

    // --- Optional STL export ---
    if let Some(stl) = stl_path {
        println!();
        println!("Exporting STL: {stl} (resolution=128, bounds=[-2, 2])");
        let stl_start = Instant::now();
        let stats = node_to_stl(&node, stl, &PrintConfig::default()).unwrap_or_else(|e| {
            eprintln!("node_to_stl failed: {e}");
            process::exit(1);
        });
        println!(
            "  {} triangles in {}ms — {:?}",
            stats.triangle_count,
            stl_start.elapsed().as_millis(),
            stats
        );
    }
}
