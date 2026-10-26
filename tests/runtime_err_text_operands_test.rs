//! Every runtime error text embeds the program's text through `err_text`
//! (C-418, #4126, security table row 21).
//!
//! C-418 escapes the control and bidi characters of the user data an error
//! text embeds, so a path holding `\n` cannot forge a log line and one holding
//! ESC cannot drive the reader's terminal. The rule is one function
//! (`almide_base::err_text`), and it holds only where it is called. This gate
//! finds the places it is not, by reading the runtime's own source: it is
//! derived from the code, not from a list of functions.
//!
//! What it reads: every `.rs` file of the native runtime (`runtime/rs/src`),
//! the shared cores (`crates/almide-rt-core/src`), the embedded wasm host
//! (`crates/almide-wasm-run/src`) and the interp oracle
//! (`crates/almide-interp/src`); generated files and `#[cfg(test)]` code
//! excluded.
//!
//! What it asks, per function:
//!
//! * TEXT is a parameter whose type is text (`str`, `String`, `AlmideString`,
//!   `Path`, `PathBuf`, `OsStr`, `OsString`, or a container of them) — a
//!   `&'static str` is the runtime's own literal and is not — and a local
//!   bound from text (`let raw = &s[i..]`, `let p = path.trim()`, a `format!`
//!   that interpolates text).
//! * A call passes text on only through the parameters whose text reaches the
//!   callee's result: each function's SUMMARY, computed from its body to a
//!   fixpoint. The escaper's two entry points (`almide_rt_errtext_escape` /
//!   `_quote`) pass nothing on, so neither does `fs_q(s) =
//!   almide_rt_errtext_quote(s)`, while a helper that formats its operand raw
//!   passes it on to its caller's error.
//! * An ERROR VALUE is the argument of `Err(..)`, of `.ok_or(..)`, the result
//!   of the closure given to `.map_err(..)` / `.ok_or_else(..)`, and the
//!   message of `panic!`.
//! * The gate fails on every error value that carries text: the text reaches
//!   the error raw, formatted with `{:?}` (Rust's escaping is not C-418's: it
//!   differs on every leg that is not Rust, and it quotes), or through a
//!   quoting loop of its own (`out.push(c)` makes `out` text: only the one
//!   rule escapes). A text handed back unchanged (`Err(e)`,
//!   `.ok_or(err.to_string())`) is the program's own value and embeds nothing;
//!   the error a `.map_err` closure receives is its producer's, gated there.
//!   A structured error (`Err((path, e))`) is not a text yet; the gate meets
//!   it where a function formats it. A function that returns no text passes
//!   none on.
//!
//! The analysis is local to a function and does not follow fields or values
//! read from guest memory; a text that reaches an error that way is held by the
//! cross-target fixtures of C-418 instead. The control
//! `control_a_raw_operand_and_a_debug_operand_are_caught` runs the same
//! analysis over twins of the real shapes and requires each to be caught.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use quote::ToTokens;
use syn::visit::{self, Visit};
use syn::{Expr, FnArg, Pat, Stmt};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const SOURCES: [&str; 4] = [
    "runtime/rs/src",
    "crates/almide-rt-core/src",
    "crates/almide-wasm-run/src",
    "crates/almide-interp/src",
];

/// The ONE escaper's entry points (crates/almide-base/src/err_text.rs).
const ESCAPE_ROOTS: [&str; 2] = ["almide_rt_errtext_escape", "almide_rt_errtext_quote"];

const TEXT_TYPES: [&str; 7] = [
    "str",
    "String",
    "AlmideString",
    "Path",
    "PathBuf",
    "OsStr",
    "OsString",
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name()
                .is_some_and(|n| n == "generated" || n == "tests")
            {
                continue;
            }
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.parse_args::<syn::Meta>()
                .is_ok_and(|m| m.path().is_ident("test"))
    })
}

fn is_text_type(ty: &syn::Type) -> bool {
    let text = ty.to_token_stream().to_string();
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|w| TEXT_TYPES.contains(&w))
}

fn last_segment(path: &syn::Path) -> String {
    path.segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default()
}

/// A function's tail expression: the last statement of its block when that
/// is an expression without `;`, looked through blocks and parentheses.
fn tail(block: &syn::Block) -> Option<&Expr> {
    match block.stmts.last()? {
        Stmt::Expr(e, None) => Some(peel(e)),
        _ => None,
    }
}

fn peel(e: &Expr) -> &Expr {
    match e {
        Expr::Paren(p) => peel(&p.expr),
        Expr::Group(g) => peel(&g.expr),
        Expr::Block(b) => match b.block.stmts.last() {
            Some(Stmt::Expr(inner, None)) => peel(inner),
            _ => e,
        },
        _ => e,
    }
}

/// The text parameters of a signature, by position among its typed
/// parameters: a `&'static str` is a literal of the runtime's own (a call
/// name such as `"unix.write_fds"`), never the program's data, so it is not one.
fn text_params(sig: &syn::Signature) -> Vec<Option<String>> {
    sig.inputs
        .iter()
        .filter_map(|a| match a {
            FnArg::Typed(t) => Some(t),
            FnArg::Receiver(_) => None,
        })
        .map(|t| {
            let is_static = t.ty.to_token_stream().to_string().contains("'static");
            match &*t.pat {
                Pat::Ident(i) if is_text_type(&t.ty) && !is_static => Some(i.ident.to_string()),
                _ => None,
            }
        })
        .collect()
}

/// What the analysis knows of every function it read: by name, the
/// positions of the text parameters whose text reaches the function's result
/// unescaped (its SUMMARY). The escaper roots have an empty summary by
/// definition; every other summary is computed from the function's body, to a
/// fixpoint, so `fs_q(s) = almide_rt_errtext_quote(s)` has an empty one and a
/// helper that formats its operand raw has that operand in its own.
struct FnFacts {
    summaries: BTreeMap<String, BTreeSet<usize>>,
}

struct Defs<'a> {
    defs: &'a mut Vec<(syn::Signature, syn::Block)>,
}

impl<'ast> Visit<'ast> for Defs<'_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if !is_cfg_test(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if is_cfg_test(&f.attrs) {
            return;
        }
        self.defs.push((f.sig.clone(), (*f.block).clone()));
        visit::visit_item_fn(self, f);
    }
    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.defs.push((f.sig.clone(), f.block.clone()));
        visit::visit_impl_item_fn(self, f);
    }
}

impl FnFacts {
    fn of(files: &[syn::File]) -> FnFacts {
        let mut defs = Vec::new();
        for f in files {
            Defs { defs: &mut defs }.visit_file(f);
        }
        let mut facts = FnFacts {
            summaries: BTreeMap::new(),
        };
        for (sig, _) in &defs {
            facts.summaries.entry(sig.ident.to_string()).or_default();
        }
        loop {
            let mut grew = false;
            for (sig, block) in &defs {
                let name = sig.ident.to_string();
                // A function that does not return text cannot pass text on
                // (`fn fields(half: &str) -> Option<Vec<i64>>`).
                let returns_text =
                    matches!(&sig.output, syn::ReturnType::Type(_, ty) if is_text_type(ty));
                if ESCAPE_ROOTS.contains(&name.as_str()) || !returns_text {
                    continue;
                }
                for (i, p) in text_params(sig).into_iter().enumerate() {
                    let Some(p) = p else { continue };
                    if facts.summaries[&name].contains(&i) {
                        continue;
                    }
                    if result_carries(&facts, &p, block) {
                        facts.summaries.get_mut(&name).unwrap().insert(i);
                        grew = true;
                    }
                }
            }
            if !grew {
                return facts;
            }
        }
    }
}

/// Does `param`'s text reach the result of `block` (its tail or a `return`)?
fn result_carries(facts: &FnFacts, param: &str, block: &syn::Block) -> bool {
    let mut sink = Vec::new();
    let mut s = Scan {
        facts,
        text: [param.to_string()].into_iter().collect(),
        file: String::new(),
        func: String::new(),
        findings: &mut sink,
        returns_text: false,
    };
    s.visit_block(block);
    let tail_text = tail(block).is_some_and(|t| s.carries(t));
    tail_text || s.returns_text
}

/// Methods that keep a text a text (`s.trim()` is still `s`'s bytes).
const TEXT_KEEPING_METHODS: &[&str] = &[
    "to_string",
    "to_owned",
    "as_str",
    "as_ref",
    "trim",
    "trim_start",
    "trim_end",
    "trim_matches",
    "to_lowercase",
    "to_uppercase",
    "clone",
    "into",
    "display",
    "to_string_lossy",
    "to_str",
    "as_path",
    "strip_prefix",
    "strip_suffix",
    "unwrap",
    "unwrap_or",
    "unwrap_or_default",
    "replace",
    "join",
    "as_os_str",
    "iter",
    "into_iter",
    "lines",
    "split",
    "split_whitespace",
    "chars",
    "first",
    "last",
    "get",
    "expect",
    "split_once",
    "rsplit_once",
    "splitn",
    "rsplit",
    "map",
    "filter",
    "collect",
    "next",
    "nth",
    "ok",
    "ok_or",
    "cloned",
    "copied",
    "skip",
    "take",
    "unwrap_or_else",
    "trim_start_matches",
    "trim_end_matches",
    "find_map",
    "and_then",
];

/// One function's analysis.
struct Scan<'a> {
    facts: &'a FnFacts,
    text: BTreeSet<String>,
    file: String,
    func: String,
    findings: &'a mut Vec<String>,
    /// Set when a `return` carries text (the summary pass reads it).
    returns_text: bool,
}

/// The arguments of a format-like macro: (template, inline captures, args).
fn format_parts(m: &syn::Macro) -> Option<(String, Vec<String>, Vec<Expr>)> {
    let args = m
        .parse_body_with(syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated)
        .ok()?;
    let mut it = args.into_iter();
    let template = match it.next()? {
        Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) => s.value(),
        _ => return None,
    };
    let rest: Vec<Expr> = it
        .map(|e| match e {
            Expr::Assign(a) => *a.right,
            e => e,
        })
        .collect();
    let captures = format_captures(&template);
    Some((template, captures, rest))
}

/// The inline captures of a format template (`{raw}`, `{s:?}` -> `raw`, `s`).
fn format_captures(template: &str) -> Vec<String> {
    let mut captures = Vec::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                chars.next();
                continue;
            }
            let inner: String = chars.by_ref().take_while(|&c| c != '}').collect();
            let name = inner.split(':').next().unwrap_or("");
            if name
                .chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
            {
                captures.push(name.to_string());
            }
        }
    }
    captures
}

fn macro_name(m: &syn::Macro) -> String {
    last_segment(&m.path)
}

impl Scan<'_> {
    /// Does `e`'s value carry text that has not been through an escaper?
    fn carries(&self, e: &Expr) -> bool {
        match e {
            Expr::Path(p) => p
                .path
                .get_ident()
                .is_some_and(|i| self.text.contains(&i.to_string())),
            Expr::Reference(r) => self.carries(&r.expr),
            Expr::Paren(p) => self.carries(&p.expr),
            Expr::Group(g) => self.carries(&g.expr),
            Expr::Unary(u) if matches!(u.op, syn::UnOp::Deref(_)) => self.carries(&u.expr),
            Expr::Index(i) => self.carries(&i.expr),
            Expr::Try(t) => self.carries(&t.expr),
            Expr::Binary(b) if matches!(b.op, syn::BinOp::Add(_)) => {
                self.carries(&b.left) || self.carries(&b.right)
            }
            // `(halves.next(), halves.next())`, `[a, b]`: a part carries.
            Expr::Tuple(t) => t.elems.iter().any(|e| self.carries(e)),
            Expr::Array(a) => a.elems.iter().any(|e| self.carries(e)),
            Expr::MethodCall(m) => {
                let name = m.method.to_string();
                if TEXT_KEEPING_METHODS.contains(&name.as_str()) {
                    return self.carries(&m.receiver) || m.args.iter().any(|a| self.carries(a));
                }
                // A method of the runtime's own: through its summary.
                self.facts.summaries.get(&name).is_some_and(|sum| {
                    m.args
                        .iter()
                        .enumerate()
                        .any(|(i, a)| sum.contains(&i) && self.carries(a))
                })
            }
            Expr::Call(c) => {
                let Expr::Path(p) = &*c.func else {
                    return false;
                };
                let name = last_segment(&p.path);
                match self.facts.summaries.get(&name) {
                    // A function of the runtime's own: through its summary.
                    Some(sum) => c
                        .args
                        .iter()
                        .enumerate()
                        .any(|(i, a)| sum.contains(&i) && self.carries(a)),
                    // `String::from`, `PathBuf::from`, `Path::new`, `Some(..)`, …
                    None => {
                        matches!(
                            name.as_str(),
                            "from" | "new" | "Some" | "Ok" | "Err" | "Box"
                        ) && c.args.iter().any(|a| self.carries(a))
                    }
                }
            }
            Expr::Macro(m) => self.macro_carries(&m.mac),
            Expr::Block(b) => match b.block.stmts.last() {
                Some(Stmt::Expr(t, None)) => self.carries(t),
                _ => false,
            },
            Expr::If(i) => {
                let then = matches!(i.then_branch.stmts.last(), Some(Stmt::Expr(t, None)) if self.carries(t));
                then || i.else_branch.as_ref().is_some_and(|(_, e)| self.carries(e))
            }
            Expr::Match(m) => m.arms.iter().any(|a| self.carries(&a.body)),
            _ => false,
        }
    }

    /// A `format!`-like macro carries text when an argument or an inline
    /// capture does, or when it formats a text with `{:?}`.
    fn macro_carries(&self, m: &syn::Macro) -> bool {
        if !matches!(macro_name(m).as_str(), "format" | "panic" | "format_args") {
            return false;
        }
        let Some((_, captures, args)) = format_parts(m) else {
            return false;
        };
        captures.iter().any(|c| self.text.contains(c)) || args.iter().any(|a| self.carries(a))
    }

    fn error_value(&mut self, e: &Expr, site: &str) {
        // A text handed back unchanged — `option.to_result(o, err)`'s `err`, a
        // collected `Err(e)` — is the program's own value: nothing is embedded
        // in a text of the runtime's.
        if is_pass_through(e) {
            return;
        }
        // A structured error (`Err((rel.to_string(), e))`) is not a text yet:
        // the text is made where a caller formats it, and gated there.
        if matches!(peel(e), Expr::Tuple(_) | Expr::Struct(_)) {
            return;
        }
        if self.carries(e) {
            let shown: String = e.to_token_stream().to_string().chars().take(160).collect();
            self.findings
                .push(format!("{} :: {} :: {site}: {shown}", self.file, self.func));
        }
    }

    fn closure_result<'e>(&self, e: &'e Expr) -> Option<&'e Expr> {
        match peel(e) {
            Expr::Closure(c) => Some(peel(&c.body)),
            _ => None,
        }
    }

    /// Bind every name `pat` introduces: text when `from_text`, otherwise
    /// not (a shadowing binding clears an earlier text of the same name).
    fn bind(&mut self, pat: &Pat, from_text: bool) {
        let mut names = Vec::new();
        pat_idents(pat, &mut names);
        for n in names {
            if from_text {
                self.text.insert(n);
            } else {
                self.text.remove(&n);
            }
        }
    }
}

/// A bare name, or one through methods that keep it as it is (`err.to_string()`,
/// `e.clone()`, `msg.into()`).
fn is_pass_through(e: &Expr) -> bool {
    match peel(e) {
        Expr::Path(p) => p.path.get_ident().is_some(),
        Expr::Reference(r) => is_pass_through(&r.expr),
        Expr::MethodCall(m) => {
            matches!(
                m.method.to_string().as_str(),
                "to_string" | "to_owned" | "clone" | "into"
            ) && m.args.is_empty()
                && is_pass_through(&m.receiver)
        }
        _ => false,
    }
}

/// Every identifier a pattern binds (`(a, b)`, `Some(x)`, `S { f, .. }`, …).
fn pat_idents(p: &Pat, out: &mut Vec<String>) {
    match p {
        Pat::Ident(i) => {
            out.push(i.ident.to_string());
            if let Some((_, sub)) = &i.subpat {
                pat_idents(sub, out);
            }
        }
        Pat::Type(t) => pat_idents(&t.pat, out),
        Pat::Reference(r) => pat_idents(&r.pat, out),
        Pat::Paren(pp) => pat_idents(&pp.pat, out),
        Pat::Tuple(t) => t.elems.iter().for_each(|e| pat_idents(e, out)),
        Pat::TupleStruct(t) => t.elems.iter().for_each(|e| pat_idents(e, out)),
        Pat::Slice(sl) => sl.elems.iter().for_each(|e| pat_idents(e, out)),
        Pat::Struct(st) => st.fields.iter().for_each(|f| pat_idents(&f.pat, out)),
        Pat::Or(o) => o.cases.iter().for_each(|c| pat_idents(c, out)),
        _ => {}
    }
}

impl<'ast> Visit<'ast> for Scan<'_> {
    fn visit_local(&mut self, l: &'ast syn::Local) {
        visit::visit_local(self, l);
        let explicit_text = matches!(&l.pat, Pat::Type(t) if is_text_type(&t.ty));
        let from_text = match &l.init {
            Some(init) => self.carries(&init.expr),
            None => explicit_text,
        };
        self.bind(&l.pat, from_text);
    }

    // A pattern matched against text binds text: `for p in paths`,
    // `if let Some((a, b)) = s.split_once(..)`, a `match` arm, and the
    // parameters of a closure given to a method of text (`s.split(..).map(|p| ..)`).
    fn visit_expr_for_loop(&mut self, f: &'ast syn::ExprForLoop) {
        let from_text = self.carries(&f.expr);
        self.visit_expr(&f.expr);
        self.bind(&f.pat, from_text);
        self.visit_block(&f.body);
    }

    fn visit_expr_closure(&mut self, c: &'ast syn::ExprClosure) {
        // A closure parameter typed as text is text (`|s: &str| ..`); the
        // error a `.map_err` closure receives is its producer's, not one.
        for p in &c.inputs {
            if matches!(p, Pat::Type(t) if is_text_type(&t.ty)) {
                self.bind(p, true);
            }
        }
        let outer = self.returns_text;
        visit::visit_expr_closure(self, c);
        self.returns_text = outer;
    }

    fn visit_expr_let(&mut self, l: &'ast syn::ExprLet) {
        self.visit_expr(&l.expr);
        let from_text = self.carries(&l.expr);
        self.bind(&l.pat, from_text);
    }

    fn visit_expr_match(&mut self, m: &'ast syn::ExprMatch) {
        self.visit_expr(&m.expr);
        let from_text = self.carries(&m.expr);
        for arm in &m.arms {
            self.bind(&arm.pat, from_text);
            self.visit_arm(arm);
        }
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let Expr::Path(p) = &*c.func {
            if last_segment(&p.path) == "Err" {
                for a in &c.args {
                    self.error_value(a, "Err(..)");
                }
            }
        }
        visit::visit_expr_call(self, c);
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        // Writing text into a buffer makes the buffer text: `out.push(c)` for
        // a `c` of the program's text. So a hand-rolled quoting loop is not
        // an escaper unless it goes through err_text; only the one rule is.
        if matches!(
            m.method.to_string().as_str(),
            "push" | "push_str" | "extend" | "insert" | "insert_str"
        ) && m.args.iter().any(|a| self.carries(a))
        {
            if let Expr::Path(p) = peel(&m.receiver) {
                if let Some(i) = p.path.get_ident() {
                    self.text.insert(i.to_string());
                }
            }
        }
        let error_handler = matches!(
            m.method.to_string().as_str(),
            "map_err" | "ok_or_else" | "unwrap_or_else" | "or_else"
        );
        if !error_handler && self.carries(&m.receiver) {
            for a in &m.args {
                if let Expr::Closure(c) = peel(a) {
                    c.inputs.iter().for_each(|p| self.bind(p, true));
                }
            }
        }
        match m.method.to_string().as_str() {
            "map_err" | "ok_or_else" => {
                if let Some(body) = m.args.first().and_then(|a| self.closure_result(a)) {
                    self.error_value(body, &format!(".{}(..)", m.method));
                }
            }
            "ok_or" => {
                if let Some(a) = m.args.first() {
                    self.error_value(a, ".ok_or(..)");
                }
            }
            _ => {}
        }
        visit::visit_expr_method_call(self, m);
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        if matches!(macro_name(m).as_str(), "write" | "writeln") {
            if let Ok(args) = m.parse_body_with(
                syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated,
            ) {
                let mut it = args.iter();
                if let (
                    Some(dst),
                    Some(Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(t),
                        ..
                    })),
                ) = (it.next(), it.next())
                {
                    let rest: Vec<&Expr> = it.collect();
                    let inline = format_captures(&t.value())
                        .iter()
                        .any(|c| self.text.contains(c));
                    if inline || rest.iter().any(|a| self.carries(a)) {
                        if let Expr::Path(p) = peel(dst) {
                            if let Some(i) = p.path.get_ident() {
                                self.text.insert(i.to_string());
                            }
                        }
                    }
                }
            }
        }
        if macro_name(m) == "panic" && self.macro_carries(m) {
            let shown: String = m.to_token_stream().to_string().chars().take(160).collect();
            self.findings
                .push(format!("{} :: {} :: panic!: {shown}", self.file, self.func));
        }
        // The arguments of a format-like macro are expressions too: an `Err`
        // built inside one is still an error value.
        if let Some((_, _, args)) = format_parts(m) {
            for a in &args {
                self.visit_expr(a);
            }
        }
    }

    fn visit_expr_return(&mut self, r: &'ast syn::ExprReturn) {
        if r.expr.as_ref().is_some_and(|e| self.carries(e)) {
            self.returns_text = true;
        }
        visit::visit_expr_return(self, r);
    }

    // A nested fn is its own scope.
    fn visit_item_fn(&mut self, _: &'ast syn::ItemFn) {}
}

struct PerFn<'a> {
    facts: &'a FnFacts,
    file: String,
    findings: &'a mut Vec<String>,
}

impl PerFn<'_> {
    fn scan(&mut self, sig: &syn::Signature, block: &syn::Block) {
        let text = text_params(sig).into_iter().flatten().collect();
        let mut s = Scan {
            facts: self.facts,
            text,
            file: self.file.clone(),
            func: sig.ident.to_string(),
            findings: self.findings,
            returns_text: false,
        };
        s.visit_block(block);
    }
}

impl<'ast> Visit<'ast> for PerFn<'_> {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if !is_cfg_test(&m.attrs) {
            visit::visit_item_mod(self, m);
        }
    }
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if is_cfg_test(&f.attrs) {
            return;
        }
        self.scan(&f.sig, &f.block);
        visit::visit_item_fn(self, f);
    }
    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        self.scan(&f.sig, &f.block);
        visit::visit_impl_item_fn(self, f);
    }
}

/// The findings over `(label, source)` files, the summaries derived from all
/// of them.
fn findings_in(files: &[(String, String)]) -> Vec<String> {
    let parsed: Vec<(String, syn::File)> = files
        .iter()
        .map(|(label, src)| {
            (
                label.clone(),
                syn::parse_file(src).unwrap_or_else(|e| panic!("{label}: {e}")),
            )
        })
        .collect();
    let asts: Vec<syn::File> = parsed.iter().map(|(_, f)| f.clone()).collect();
    let facts = FnFacts::of(&asts);
    let mut findings = Vec::new();
    for (label, ast) in &parsed {
        PerFn {
            facts: &facts,
            file: label.clone(),
            findings: &mut findings,
        }
        .visit_file(ast);
    }
    findings
}

fn runtime_sources() -> Vec<(String, String)> {
    let mut paths = Vec::new();
    for dir in SOURCES {
        rust_files(&root().join(dir), &mut paths);
    }
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let label = p.strip_prefix(root()).unwrap_or(&p).display().to_string();
            let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{label}: {e}"));
            (label, src)
        })
        .collect()
}

#[test]
fn every_runtime_error_text_embeds_text_through_err_text() {
    let findings = findings_in(&runtime_sources());
    assert!(
        findings.is_empty(),
        "{} runtime error text(s) embed the program's text without err_text (C-418): route each through \
         almide_rt_errtext_quote / almide_rt_errtext_escape or a function whose result is theirs\n{}",
        findings.len(),
        findings.join("\n")
    );
}

/// The control: the analysis catches each shape it claims to, in twins of
/// the real ones, with the real sources present so the summaries are the real
/// ones — and it passes the escaped originals.
#[test]
fn control_a_raw_operand_and_a_debug_operand_are_caught() {
    let twins = r#"
        fn io_err(call: &str, operand: &str, e: std::io::Error) -> String { format!("{call}({operand}): {e}") }
        fn raw_q(s: &str) -> String { format!("\"{s}\"") }
        pub fn twin_raw_quote(path: &str) -> Result<String, String> {
            std::fs::read_to_string(path).map_err(|e| io_err("fs.read_text", &raw_q(path), e))
        }
        pub fn twin_inline_capture(raw: &str) -> Result<i64, String> { Err(format!("invalid offset: {raw}")) }
        pub fn twin_let_slice(s: &str, i: usize) -> Result<i64, String> {
            let raw = &s[i..];
            Err(format!("invalid offset: {}", raw))
        }
        pub fn twin_destructure(s: &str) -> Result<i64, String> {
            let Some((_, tz)) = s.split_once('T') else { return Err("no T".into()) };
            match tz.find('+') { Some(i) => { let raw = &tz[i..]; Err(format!("invalid offset: {raw}")) } None => Ok(0) }
        }
        pub fn twin_tuple_let(t: &str) -> Result<i64, String> {
            let mut halves = t.split('T');
            let (Some(_d), Some(tz), None) = (halves.next(), halves.next(), halves.next()) else { return Err("format".into()) };
            let (z, _) = match tz.strip_suffix('Z') { Some(x) => (x, true), None => (tz, false) };
            Err(format!("invalid offset: {z}"))
        }
        pub fn twin_debug(text: String) -> Result<(), String> { Err(format!("bad line {:?}", text)) }
        pub fn twin_ok_or(host: &str) -> Result<u16, String> { None.ok_or(format!("{host}: no port")) }
        pub fn twin_panic(cmd: &str) { panic!("exec {cmd} failed") }
        fn hand_quote(s: &str) -> String { let mut out = String::from("\""); for c in s.chars() { if c == '"' { out.push_str("\\\""); } else { out.push(c); } } out.push('"'); out }
        pub fn twin_hand_rolled_quote(host: &str) -> Result<(), String> { Err(format!("bad host {}", hand_quote(host))) }
        pub fn twin_escaped(path: &str) -> Result<String, String> {
            std::fs::read_to_string(path).map_err(|e| io_err("fs.read_text", &almide_rt_errtext_quote(path), e))
        }
        pub fn twin_length_only(path: &str) -> Result<(), String> { Err(format!("path of {} bytes", path.len())) }
    "#;
    let mut files = runtime_sources();
    files.push(("<twins>".to_string(), twins.to_string()));
    let found: Vec<String> = findings_in(&files)
        .into_iter()
        .filter(|f| f.starts_with("<twins>"))
        .collect();
    let caught = |f: &str| found.iter().any(|x| x.contains(&format!(":: {f} ::")));
    for twin in [
        "twin_raw_quote",
        "twin_inline_capture",
        "twin_let_slice",
        "twin_destructure",
        "twin_tuple_let",
        "twin_debug",
        "twin_ok_or",
        "twin_panic",
        "twin_hand_rolled_quote",
    ] {
        assert!(caught(twin), "{twin} was not caught:\n{}", found.join("\n"));
    }
    for clean in ["twin_escaped", "twin_length_only"] {
        assert!(!caught(clean), "{clean} was flagged:\n{}", found.join("\n"));
    }
}
