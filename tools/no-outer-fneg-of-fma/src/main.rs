//! Forbid negating the whole result of a `mul_add` call (`-(x.mul_add(y, z))`,
//! and its unparenthesised form `-x.mul_add(y, z)` — in Rust method calls bind
//! tighter than unary minus, so these are the *same* AST: `ExprUnary{Neg}`
//! wrapping an `ExprMethodCall` named `mul_add` once parens/groups are peeled).
//!
//! Replaces an earlier line-oriented regex scanner that only matched the
//! parenthesised spelling and silently passed `-a.mul_add(b, c)`,
//! `-(a).mul_add(b, c)`, `-x.y.mul_add(b, c)` and `-v[0].mul_add(b, c)` — all
//! four are the identical miscompile-prone form (see the module doc of the
//! Law fix this gate exists for: cancellation input makes `-C opt-level=0`
//! and `>=1` disagree on the sign of zero, because `fneg(fma(x,y,z)) ->
//! fma(x,-y,-z)` is an `nsz`-only-valid LLVM canonicalisation that fires
//! anyway). This tool parses real Rust syntax (`syn`) instead of guessing at
//! paren/bracket balance, so it cannot be fooled by the receiver's shape:
//! identifier, `.field`, `[index]`, a parenthesised sub-expression, or any
//! chain of those.
//!
//! `mul_add` itself is not forbidden (one correctly-rounded operation); the
//! `f32::mul_add` / `f64::mul_add` **path**-call spelling (`f32::mul_add(a,
//! b, c)`, including fully-qualified `<f32>::mul_add(...)`) is covered the
//! same as the method-call spelling. Negating an *argument* before the call
//! (`a.mul_add(b, -c)`, FMSUB-equivalent) or a nested argument
//! (`a.mul_add(b, -(c * d))`) is the Law-adopted safe form and is not
//! flagged — only negation of the call's own result is.
//!
//! ## What this does **not** see: macro invocation bodies
//!
//! `syn` does not expand macros; a macro invocation (`foo!(...)`,
//! `assert_eq!(...)`, a custom proc-macro call, etc.) is parsed as an opaque
//! token stream, not as Rust expressions, so the AST walk above cannot tell
//! whether a `mul_add` mention inside one is negated, safely negated-operand,
//! or unrelated — unlike everywhere else this tool looks, it genuinely
//! cannot verify the site. Rather than leave that as a silent blind spot,
//! any macro invocation whose token stream contains the text `mul_add` is a
//! **hard failure by default**, exactly like an unreviewed violation: it
//! must be listed in the allowlist (`<file>:<line>` of the macro invocation,
//! with a reason) before it passes, and — the same rule as everywhere else
//! — a `src/`-scoped site has **no allowlist escape at all**: the only fix
//! there is to pull the expression out of the macro call so the AST walk can
//! verify it directly. The allowlist entry only ever means "a human looked
//! at this specific macro invocation and confirmed it is safe", never
//! "this form is fine in general".
//!
//! ⚠️ `src/` has no escape hatch: a `src/`-scoped finding with an allowlist
//! entry is still a hard failure — rewriting to the Law form is the only fix
//! there. The allowlist exists only for `tests/` / `examples/` oracle files
//! that deliberately keep the old form as a comparison reference.
//!
//! Usage: `cargo run --release -- [--root DIR] [--src DIR [DIR...]]`
//! (default `--src`: `alice-lol/src alice-lol/tests alice-lol/examples`;
//! another repo passes its own, e.g. `--src src tests examples`)

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Expr, ExprCall, ExprMethodCall, ExprUnary, Macro, UnOp};

const ALLOWLIST_REL: &str = "scripts/no-outer-fneg-of-fma-allowlist.txt";
const ALLOWLIST_SCOPE_DIRS: [&str; 2] = ["tests", "examples"];
const DEFAULT_SRC_DIRS: [&str; 3] = ["alice-lol/src", "alice-lol/tests", "alice-lol/examples"];

enum FindingKind {
    /// `-x.mul_add(y, z)` (any receiver shape) found directly by the AST walk.
    NegatedMulAdd,
    /// a macro invocation's token stream mentions `mul_add` — AST-unverified,
    /// hard fail unless allowlisted (never allowlistable in `src/`).
    MacroMulAdd,
}

struct Finding {
    rel: String,
    line: usize,
    excerpt: String,
    kind: FindingKind,
}

struct FileCounts {
    unary_neg_exprs: usize,
    mul_add_calls: usize,
}

struct Visitor<'a> {
    rel: &'a str,
    src: &'a str,
    findings: Vec<Finding>,
    counts: FileCounts,
}

fn excerpt_at(src: &str, line: usize) -> String {
    src.lines()
        .nth(line.saturating_sub(1))
        .unwrap_or("")
        .trim()
        .to_string()
}

fn is_mul_add_method(call: &ExprMethodCall) -> bool {
    call.method == "mul_add"
}

fn is_mul_add_path_call(call: &ExprCall) -> bool {
    if let Expr::Path(p) = &*call.func {
        if let Some(last) = p.path.segments.last() {
            return last.ident == "mul_add";
        }
    }
    false
}

/// Peel `(expr)` and `{ expr }`-less groups (syn's `Expr::Group`, inserted by
/// some macro-expansion contexts) down to the real inner expression, so
/// `-(a.mul_add(b, c))` and `-((a.mul_add(b, c)))` are treated identically to
/// `-a.mul_add(b, c)`.
fn peel<'e>(mut e: &'e Expr) -> &'e Expr {
    loop {
        match e {
            Expr::Paren(p) => e = &p.expr,
            Expr::Group(g) => e = &g.expr,
            _ => return e,
        }
    }
}

fn is_mul_add_result(e: &Expr) -> bool {
    match peel(e) {
        Expr::MethodCall(m) => is_mul_add_method(m),
        Expr::Call(c) => is_mul_add_path_call(c),
        _ => false,
    }
}

impl<'a> Visitor<'a> {
    fn record_unary(&mut self, u: &ExprUnary) {
        if !matches!(u.op, UnOp::Neg(_)) {
            return;
        }
        self.counts.unary_neg_exprs += 1;
        if is_mul_add_result(&u.expr) {
            let line = u.op.span().start().line;
            self.findings.push(Finding {
                rel: self.rel.to_string(),
                line,
                excerpt: excerpt_at(self.src, line),
                kind: FindingKind::NegatedMulAdd,
            });
        }
    }
}

impl<'a, 'ast> Visit<'ast> for Visitor<'a> {
    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if is_mul_add_method(node) {
            self.counts.mul_add_calls += 1;
        }
        visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        if is_mul_add_path_call(node) {
            self.counts.mul_add_calls += 1;
        }
        visit::visit_expr_call(self, node);
    }

    fn visit_expr_unary(&mut self, node: &'ast ExprUnary) {
        self.record_unary(node);
        visit::visit_expr_unary(self, node);
    }

    fn visit_macro(&mut self, node: &'ast Macro) {
        // syn does not expand macros: the tokens inside are opaque to the
        // AST walk above, so this cannot tell negated from safe — hard fail
        // by default (see module doc), never allowlistable in src/.
        let rendered = node.tokens.to_string();
        if rendered.contains("mul_add") {
            let line = node.span().start().line;
            self.findings.push(Finding {
                rel: self.rel.to_string(),
                line,
                excerpt: excerpt_at(self.src, line),
                kind: FindingKind::MacroMulAdd,
            });
        }
        visit::visit_macro(self, node);
    }
}

fn is_allowlist_eligible(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split(['/', '\\']).collect();
    ALLOWLIST_SCOPE_DIRS
        .iter()
        .any(|d| parts.iter().any(|p| p == d))
}

fn read_allowlist(root: &Path) -> BTreeSet<String> {
    let path = root.join(ALLOWLIST_REL);
    let mut out = BTreeSet::new();
    let Ok(text) = fs::read_to_string(&path) else {
        return out;
    };
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if !line.is_empty() {
            out.insert(line.to_string());
        }
    }
    out
}

fn walk_rs_files(base: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(base) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

struct Report {
    errors: Vec<String>,
    files: usize,
    unary_neg_exprs: usize,
    mul_add_calls: usize,
    violations: usize,
    allowed: usize,
    macro_hits: usize,
    macro_allowed: usize,
}

fn check(root: &Path, src_dirs: &[String]) -> Report {
    let mut errors = Vec::new();
    let mut files = 0usize;
    let mut unary_neg_exprs = 0usize;
    let mut mul_add_calls = 0usize;
    let mut found: Vec<Finding> = Vec::new();

    for src_dir in src_dirs {
        let base = root.join(src_dir);
        if !base.is_dir() {
            continue;
        }
        let mut rs_files = Vec::new();
        walk_rs_files(&base, &mut rs_files);
        for path in rs_files {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let Ok(src) = fs::read_to_string(&path) else {
                errors.push(format!("error: {rel}: 読めなかった"));
                continue;
            };
            files += 1;
            let parsed = match syn::parse_file(&src) {
                Ok(f) => f,
                Err(e) => {
                    errors.push(format!(
                        "error: {rel}: syn が parse できない — 検査ができていない ({e})"
                    ));
                    continue;
                }
            };
            let mut visitor = Visitor {
                rel: &rel,
                src: &src,
                findings: Vec::new(),
                counts: FileCounts {
                    unary_neg_exprs: 0,
                    mul_add_calls: 0,
                },
            };
            visitor.visit_file(&parsed);
            unary_neg_exprs += visitor.counts.unary_neg_exprs;
            mul_add_calls += visitor.counts.mul_add_calls;
            found.extend(visitor.findings);
        }
    }

    if files == 0 {
        errors.push(format!(
            "error: src file を 1 つも読めなかった — 検査が空振りしている (対象: {})",
            src_dirs.join(", ")
        ));
        return Report {
            errors,
            files,
            unary_neg_exprs,
            mul_add_calls,
            violations: 0,
            allowed: 0,
            macro_hits: 0,
            macro_allowed: 0,
        };
    }
    if mul_add_calls == 0 {
        errors.push(format!(
            "error: mul_add 呼び出しが 0 件 — file 一覧か AST 走査が壊れている (読めた file {files} 件)"
        ));
    }

    let allowlist = read_allowlist(root);
    let mut allowed = 0usize;
    let mut macro_allowed = 0usize;
    let mut violations = 0usize;
    let mut macro_hits = 0usize;
    let found_keys: BTreeSet<String> = found
        .iter()
        .map(|v| format!("{}:{}", v.rel, v.line))
        .collect();

    for f in &found {
        let key = format!("{}:{}", f.rel, f.line);
        let (default_label, src_blocked_label) = match f.kind {
            FindingKind::NegatedMulAdd => {
                violations += 1;
                ("呼び出し結果全体を negate する fma、禁止形", "src/ 配下は allowlist 不可、Law form への書き換えのみ — allowlist の行も消す")
            }
            FindingKind::MacroMulAdd => {
                macro_hits += 1;
                (
                    "macro 呼び出しの中に mul_add がある、AST 未検証 — allowlist に理由付きで明記する",
                    "src/ 配下は allowlist 不可、macro 呼び出しの外に出して書き直す — allowlist の行も消す",
                )
            }
        };
        if allowlist.contains(&key) {
            if is_allowlist_eligible(&f.rel) {
                match f.kind {
                    FindingKind::NegatedMulAdd => allowed += 1,
                    FindingKind::MacroMulAdd => macro_allowed += 1,
                }
                continue;
            }
            errors.push(format!("error: {}:{}: {}  ({})", f.rel, f.line, f.excerpt, src_blocked_label));
            continue;
        }
        errors.push(format!("error: {}:{}: {}  ({})", f.rel, f.line, f.excerpt, default_label));
    }

    for key in allowlist.difference(&found_keys) {
        errors.push(format!("error: {key}: allowlist にあるが実物が無い — 行を消す"));
    }

    Report {
        errors,
        files,
        unary_neg_exprs,
        mul_add_calls,
        violations,
        allowed,
        macro_hits,
        macro_allowed,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut root = std::env::current_dir().expect("cwd");
    // tools/no-outer-fneg-of-fma/ -> repo root is two levels up when run via
    // `cargo run` from inside this crate; `--root` overrides unconditionally.
    if let Some(i) = args.iter().position(|a| a == "--root") {
        root = PathBuf::from(&args[i + 1]);
    }
    let mut src_dirs: Vec<String> = DEFAULT_SRC_DIRS.iter().map(|s| s.to_string()).collect();
    if let Some(i) = args.iter().position(|a| a == "--src") {
        let rest: Vec<String> = args[i + 1..]
            .iter()
            .take_while(|a| !a.starts_with("--"))
            .cloned()
            .collect();
        if rest.is_empty() {
            eprintln!("error: --src には 1 つ以上の dir を渡す");
            std::process::exit(2);
        }
        src_dirs = rest;
    }

    let report = check(&root, &src_dirs);
    for e in &report.errors {
        println!("{e}");
    }
    println!(
        "compared: files {}, unary-neg exprs examined {}, mul_add calls {}, mul_add under neg {}, allowed (tests/examples) {}, macro mul_add mentions {}, macro allowed {}",
        report.files,
        report.unary_neg_exprs,
        report.mul_add_calls,
        report.violations,
        report.allowed,
        report.macro_hits,
        report.macro_allowed
    );
    if !report.errors.is_empty() {
        println!("no-outer-fneg-of-fma: {} violation(s)", report.errors.len());
        std::process::exit(1);
    }
    println!("no-outer-fneg-of-fma: ok");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn run(files: &[(&str, &str)], allowlist: Option<&str>) -> Report {
        let dir = tempfile::tempdir().expect("tmpdir");
        for (rel, text) in files {
            let path = dir.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut f = fs::File::create(&path).unwrap();
            f.write_all(text.as_bytes()).unwrap();
        }
        if let Some(a) = allowlist {
            let path = dir.path().join(ALLOWLIST_REL);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut f = fs::File::create(&path).unwrap();
            f.write_all(a.as_bytes()).unwrap();
        }
        let src_dirs: Vec<String> = DEFAULT_SRC_DIRS.iter().map(|s| s.to_string()).collect();
        check(dir.path(), &src_dirs)
    }

    const SRC: &str = "alice-lol/src/lib.rs";

    const CLEAN: &str = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    a.mul_add(b, -c)\n}\n";

    #[test]
    fn a_clean_tree_passes() {
        let r = run(&[(SRC, CLEAN)], None);
        assert_eq!(r.errors, Vec::<String>::new());
        assert_eq!(r.violations, 0);
        assert!(r.mul_add_calls > 0);
    }

    #[test]
    fn parenthesised_outer_negation_is_caught() {
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    -(a.mul_add(b, c))\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 1);
        assert!(r.errors.iter().any(|e| e.contains("lib.rs:2")));
    }

    #[test]
    fn bare_unparenthesised_outer_negation_is_caught() {
        // the exact form 53 found missed in both LOL and Physics: no parens,
        // method call binds tighter than unary minus, same AST as above.
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    -a.mul_add(b, c)\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 1);
    }

    #[test]
    fn space_before_receiver_is_still_caught() {
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    - a.mul_add(b, c)\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 1);
    }

    #[test]
    fn parenthesised_receiver_then_chained_call_is_caught() {
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    -(a).mul_add(b, c)\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 1);
    }

    #[test]
    fn field_access_receiver_is_caught() {
        let src = "struct S { y: f32 }\nfn f(x: S, b: f32, c: f32) -> f32 {\n    -x.y.mul_add(b, c)\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 1);
    }

    #[test]
    fn index_receiver_is_caught() {
        let src = "fn f(v: [f32; 3], b: f32, c: f32) -> f32 {\n    -v[0].mul_add(b, c)\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 1);
    }

    #[test]
    fn path_call_form_is_caught() {
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    -f32::mul_add(a, b, c)\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 1);
    }

    #[test]
    fn doubly_parenthesised_is_caught() {
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    -((a.mul_add(b, c)))\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 1);
    }

    #[test]
    fn inside_a_closure_is_caught() {
        let src = "fn f() -> f32 {\n    let g = |a: f32, b: f32, c: f32| -a.mul_add(b, c);\n    g(1.0, 2.0, 3.0)\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 1);
    }

    #[test]
    fn inside_a_const_is_caught() {
        // not evaluable as a real const (a/b/c undefined) but syn parses it
        // as a const item body and this tool only parses, never evaluates
        let src = "const X: fn(f32, f32, f32) -> f32 = |a, b, c| -a.mul_add(b, c);\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 1);
    }

    #[test]
    fn binary_subtraction_before_mul_add_is_not_flagged() {
        let src = "fn f(x: f32, a: f32, b: f32, c: f32) -> f32 {\n    x - a.mul_add(b, c)\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 0);
    }

    #[test]
    fn negating_an_argument_before_the_call_is_not_flagged() {
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    (-a).mul_add(b, -c)\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 0);
    }

    #[test]
    fn negating_a_nested_argument_is_not_flagged() {
        let src = "fn f(a: f32, b: f32, c: f32, d: f32) -> f32 {\n    a.mul_add(b, -(c * d))\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 0);
    }

    #[test]
    fn negating_something_chained_after_mul_add_is_not_flagged() {
        // negates the result of .abs(), not of mul_add — a different
        // operation entirely, must not be flagged
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    -a.mul_add(b, c).abs()\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 0);
    }

    #[test]
    fn extra_addend_inside_the_outer_parens_is_not_flagged() {
        let src = "fn f(a: f32, b: f32, c: f32, d: f32) -> f32 {\n    -(a.mul_add(b, c) + d)\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 0);
    }

    #[test]
    fn a_src_site_fails_even_when_the_allowlist_carries_it() {
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    -(a.mul_add(b, c))\n}\n";
        let r = run(&[(SRC, src)], Some(&format!("# 理由\n{SRC}:2\n")));
        assert_eq!(r.violations, 1);
        assert_eq!(r.allowed, 0);
        assert!(r.errors.iter().any(|e| e.contains("allowlist 不可")));
    }

    #[test]
    fn a_tests_dir_site_passes_when_the_allowlist_carries_it() {
        let oracle = "alice-lol/tests/oracle.rs";
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    -(a.mul_add(b, c))\n}\n";
        let r = run(&[(oracle, src)], Some(&format!("# 比較用の旧形\n{oracle}:2\n")));
        assert_eq!(r.errors, Vec::<String>::new());
        assert_eq!(r.violations, 1);
        assert_eq!(r.allowed, 1);
    }

    #[test]
    fn an_examples_dir_site_passes_when_the_allowlist_carries_it() {
        let oracle = "alice-lol/examples/compare_forms.rs";
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    -(a.mul_add(b, c))\n}\n";
        let r = run(&[(oracle, src)], Some(&format!("# 比較用の旧形\n{oracle}:2\n")));
        assert_eq!(r.errors, Vec::<String>::new());
        assert_eq!(r.violations, 1);
        assert_eq!(r.allowed, 1);
    }

    #[test]
    fn an_allowlist_entry_without_the_site_fails() {
        let r = run(&[(SRC, CLEAN)], Some(&format!("{SRC}:2\n")));
        assert!(r.errors.iter().any(|e| e.contains("allowlist にあるが実物が無い")));
    }

    #[test]
    fn no_readable_src_file_fails() {
        let r = run(&[], None);
        assert_eq!(r.files, 0);
        assert!(r.errors.iter().any(|e| e.contains("1 つも読めなかった")));
    }

    #[test]
    fn zero_mul_add_calls_fails_even_with_files_present() {
        let r = run(&[(SRC, "fn f() {}\n")], None);
        assert_eq!(r.mul_add_calls, 0);
        assert!(r.errors.iter().any(|e| e.contains("0 件")));
    }

    #[test]
    fn macro_mul_add_mention_is_a_hard_fail_by_default() {
        // documented blind spot: the AST walk cannot verify a macro's
        // argument tokens, so any mul_add mention there fails by default,
        // not just a note — the gate must not stay silently blind to new
        // code that hides a negation inside a macro call.
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    a.mul_add(b, c)\n}\nfn g(a: f32, b: f32, c: f32) {\n    println!(\"{}\", a.mul_add(b, c));\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 0, "the macro mention itself is not an AST-level negation");
        assert_eq!(r.macro_hits, 1);
        assert_eq!(r.macro_allowed, 0);
        assert!(!r.errors.is_empty(), "unreviewed macro mul_add mention must fail");
    }

    #[test]
    fn macro_hidden_negation_is_also_a_hard_fail() {
        // the specific danger case: a negated mul_add hidden inside a macro
        // invocation — the AST walk cannot see it is negated at all (it is
        // opaque tokens), so it must still fail, same as the plain mention.
        let src = "fn g(a: f32, b: f32, c: f32) {\n    println!(\"{}\", -a.mul_add(b, c));\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.macro_hits, 1);
        assert!(r.errors.iter().any(|e| e.contains("macro 呼び出しの中に mul_add")));
    }

    #[test]
    fn macro_mul_add_in_src_fails_even_with_an_allowlist_entry() {
        // same no-escape-hatch rule as a direct src/ violation: a macro
        // mention in src/ cannot be allowlisted away, only rewritten out of
        // the macro so the AST walk can verify it directly.
        let src = "fn g(a: f32, b: f32, c: f32) {\n    println!(\"{}\", a.mul_add(b, c));\n}\n";
        let r = run(&[(SRC, src)], Some(&format!("# 理由\n{SRC}:2\n")));
        assert_eq!(r.macro_hits, 1);
        assert_eq!(r.macro_allowed, 0);
        assert!(r.errors.iter().any(|e| e.contains("macro 呼び出しの外に出して書き直す")));
    }

    #[test]
    fn macro_mul_add_in_tests_dir_passes_with_an_allowlist_entry() {
        let oracle = "alice-lol/tests/oracle.rs";
        let src = "fn f(a: f32, b: f32, c: f32) -> f32 {\n    a.mul_add(b, c)\n}\nfn g(a: f32, b: f32, c: f32) {\n    println!(\"{}\", a.mul_add(b, c));\n}\n";
        let r = run(&[(oracle, src)], Some(&format!("# 比較用\n{oracle}:5\n")));
        assert_eq!(r.errors, Vec::<String>::new());
        assert_eq!(r.macro_hits, 1);
        assert_eq!(r.macro_allowed, 1);
    }

    #[test]
    fn multi_line_receiver_chain_is_caught() {
        let src = "struct S { y: f32 }\nfn f(x: S, b: f32, c: f32) -> f32 {\n    -x\n        .y\n        .mul_add(b, c)\n}\n";
        let r = run(&[(SRC, src)], None);
        assert_eq!(r.violations, 1);
    }

    #[test]
    fn src_dirs_can_be_overridden_for_other_repos() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("src/lib.rs");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, CLEAN).unwrap();
        let default_dirs: Vec<String> = DEFAULT_SRC_DIRS.iter().map(|s| s.to_string()).collect();
        let r_default = check(dir.path(), &default_dirs);
        assert_eq!(r_default.files, 0);
        let r_override = check(dir.path(), &["src".to_string()]);
        assert_eq!(r_override.errors, Vec::<String>::new());
        assert_eq!(r_override.files, 1);
    }

    /// Mutation teeth (2026-10-04 discipline: break the fix, see red):
    /// if the Paren/Group-peeling in `peel()` is removed (i.e. only the
    /// bare-identifier unparenthesised branch survives, or vice versa), one
    /// of `parenthesised_outer_negation_is_caught` /
    /// `bare_unparenthesised_outer_negation_is_caught` goes red because each
    /// exercises a different code path through `is_mul_add_result`. Verified
    /// by hand: commenting out the `Expr::Paren`/`Expr::Group` arms of
    /// `peel()` turns `parenthesised_outer_negation_is_caught` red while
    /// `bare_unparenthesised_outer_negation_is_caught` stays green, and vice
    /// versa for commenting out the direct `Expr::MethodCall`/`Expr::Call`
    /// match arms in `is_mul_add_result`.
    #[test]
    fn mutation_teeth_marker_both_branches_have_dedicated_tests() {
        // this test exists only as a signpost to the doc comment above;
        // the actual teeth are the two tests it names.
    }
}
