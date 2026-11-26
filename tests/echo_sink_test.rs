//! Decision row front.diagnostic-echo (#4819): the `almide` binary writes
//! its own stdout and stderr ONLY through the output sink of src/lib.rs —
//! `out`, `out_no_nl`, `err`, `err_no_nl` (each writes
//! `almide_base::echo::visible` of its text), `out_json_line` /
//! `out_lsp_frame` (`echo::json_visible`), and the one raw channel,
//! `relay_program_output`, for the bytes a user program wrote. So a hidden
//! character a caller forgot to escape is escaped at the sink, and a NEW
//! output path is escaped by construction: this gate refuses a print macro
//! (`println!`, `eprintln!`, `print!`, `eprint!`, `dbg!`) or a raw
//! `stdout()` / `stderr()` handle anywhere else in the binary's sources
//! (src/), test code excepted. Read from the parsed source (syn), so a print
//! inside a string literal — a generated program's text — is not a print.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};

#[path = "support/novel.rs"]
mod novel;

/// The sink: the functions of src/lib.rs that may write to the process's
/// stdout / stderr. Each one passes its text through `almide_base::echo`
/// (or is the program-output relay, or only flushes / asks for a terminal).
const SINK_FILE: &str = "src/lib.rs";
const SINK_FNS: &[&str] = &[
    "out",
    "out_no_nl",
    "err",
    "err_no_nl",
    "out_json_line",
    "out_lsp_frame",
    "relay_program_output",
    "flush_out",
    "stderr_is_terminal",
];

const PRINT_MACROS: &[&str] = &["println", "eprintln", "print", "eprint", "dbg"];
const RAW_HANDLES: &[&str] = &["stdout", "stderr"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg") && a.parse_args::<syn::Meta>().is_ok_and(|m| m.path().is_ident("test"))
    })
}

/// Every raw write in one parsed file: `(enclosing fn, what)`.
struct Finder {
    fns: Vec<String>,
    found: Vec<(String, String)>,
    /// `#[cfg(test)] #[path = "…"] mod` files this file declares.
    test_paths: Vec<String>,
}

impl Finder {
    fn hit(&mut self, what: String) {
        let at = self.fns.last().cloned().unwrap_or_else(|| "<item>".to_string());
        self.found.push((at, what));
    }

    /// A macro's tokens are not parsed as expressions by `syn::visit`: walk
    /// them for a print macro or a handle call (`vec![eprintln!(…)]`).
    fn tokens(&mut self, ts: proc_macro2::TokenStream) {
        let toks: Vec<proc_macro2::TokenTree> = ts.into_iter().collect();
        for (i, t) in toks.iter().enumerate() {
            match t {
                proc_macro2::TokenTree::Ident(id) => {
                    let s = id.to_string();
                    let next = toks.get(i + 1);
                    let bang = matches!(next, Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == '!');
                    let call = matches!(next, Some(proc_macro2::TokenTree::Group(g)) if g.delimiter() == proc_macro2::Delimiter::Parenthesis);
                    if bang && PRINT_MACROS.contains(&s.as_str()) {
                        self.hit(format!("{s}!"));
                    }
                    if call && RAW_HANDLES.contains(&s.as_str()) {
                        self.hit(format!("{s}()"));
                    }
                }
                proc_macro2::TokenTree::Group(g) => self.tokens(g.stream()),
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Finder {
    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        if is_cfg_test(&m.attrs) {
            for a in &m.attrs {
                if a.path().is_ident("path")
                    && let syn::Meta::NameValue(nv) = &a.meta
                    && let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) = &nv.value
                {
                    self.test_paths.push(s.value());
                }
            }
            if m.content.is_none() {
                self.test_paths.push(format!("{}.rs", m.ident));
            }
            return;
        }
        visit::visit_item_mod(self, m);
    }

    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        if is_cfg_test(&f.attrs) {
            return;
        }
        self.fns.push(f.sig.ident.to_string());
        visit::visit_item_fn(self, f);
        self.fns.pop();
    }

    fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
        if is_cfg_test(&f.attrs) {
            return;
        }
        self.fns.push(f.sig.ident.to_string());
        visit::visit_impl_item_fn(self, f);
        self.fns.pop();
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let name = m.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default();
        if PRINT_MACROS.contains(&name.as_str()) {
            self.hit(format!("{name}!"));
        }
        self.tokens(m.tokens.clone());
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*c.func
            && let Some(last) = p.path.segments.last()
            && RAW_HANDLES.contains(&last.ident.to_string().as_str())
        {
            self.hit(format!("{}()", last.ident));
        }
        visit::visit_expr_call(self, c);
    }
}

/// The raw writes of one source text: `(enclosing fn, what)`, and the test
/// module files it declares.
fn raw_writes(src: &str) -> (Vec<(String, String)>, Vec<String>) {
    let ast = syn::parse_file(src).unwrap_or_else(|e| panic!("parse: {e}"));
    let mut f = Finder { fns: Vec::new(), found: Vec::new(), test_paths: Vec::new() };
    f.visit_file(&ast);
    (f.found, f.test_paths)
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Every raw write in the binary's sources outside the sink.
fn violations(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    files.sort();
    let mut test_files: BTreeSet<PathBuf> = BTreeSet::new();
    let mut per_file = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(f).unwrap();
        let (found, tests) = raw_writes(&src);
        let dir = f.parent().unwrap();
        for t in tests {
            test_files.insert(dir.join(&t));
            // `mod foo;` in `a/mod.rs` or `a.rs` declares `a/foo.rs` too.
            if let Some(stem) = f.file_stem() {
                test_files.insert(dir.join(stem).join(&t));
            }
        }
        per_file.push((f.clone(), found));
    }
    let mut bad = Vec::new();
    for (f, found) in per_file {
        if test_files.contains(&f) {
            continue;
        }
        let rel = f.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        for (at, what) in found {
            if rel == SINK_FILE && SINK_FNS.contains(&at.as_str()) {
                continue;
            }
            bad.push(format!("{rel}: `{what}` in `{at}`"));
        }
    }
    bad
}

#[test]
fn the_binary_writes_its_output_only_through_the_escaping_sink() {
    let bad = violations(&root());
    assert!(
        bad.is_empty(),
        "raw writes to stdout / stderr outside the output sink (src/lib.rs) — write through `crate::out` / `err` \
         (escaped), `out_json_line` (JSON), or `relay_program_output` (a user program's own bytes):\n  {}",
        bad.join("\n  ")
    );
    // The sink itself is found by the same reader (the control that the
    // scan sees a print at all).
    let (sink, _) = raw_writes(&std::fs::read_to_string(root().join(SINK_FILE)).unwrap());
    let seen: BTreeSet<&str> = sink.iter().map(|(at, _)| at.as_str()).collect();
    for f in ["out", "err", "relay_program_output"] {
        assert!(seen.contains(f), "the scan sees no write in the sink fn `{f}`: {sink:?}");
    }
}

/// c2's novel item: a fresh output path, written every way a print can be
/// spelled, in a fresh function of a fresh file under src/ — each refused;
/// the same function writing through the sink is not.
#[test]
fn novel_a_fresh_output_path_outside_the_sink_is_refused() {
    let work = tempfile::TempDir::new().unwrap();
    let src = work.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("lib.rs"), "pub fn out(s: &str) { println!(\"{}\", s); }\n").unwrap();
    let spellings = [
        "eprintln!(\"{}\", m);",
        "println!(\"{m}\");",
        "print!(\"{}\", m);",
        "eprint!(\"{}\", m);",
        "let _ = dbg!(m);",
        "use std::io::Write as _; let _ = std::io::stderr().write_all(m.as_bytes());",
        "use std::io::Write as _; let _ = writeln!(std::io::stdout(), \"{m}\");",
        "let _v = vec![eprintln!(\"{m}\")];",
    ];
    for (i, body) in spellings.iter().enumerate() {
        let f = novel::ident("emit");
        let file = format!("{}.rs", novel::module());
        std::fs::write(src.join(&file), format!("pub fn {f}(m: &str) {{ {body} }}\n")).unwrap();
        let bad = violations(work.path());
        assert!(
            bad.iter().any(|b| b.contains(&file) && b.contains(&f)),
            "spelling {i} `{body}` in a fresh fn is not refused: {bad:?}"
        );
        std::fs::remove_file(src.join(&file)).unwrap();
    }
    let f = novel::ident("emit");
    std::fs::write(
        src.join(format!("{}.rs", novel::module())),
        format!("pub fn {f}(m: &str) {{ crate::out(m); let _t = \"println!(x)\"; }}\n#[cfg(test)]\nmod tests {{ #[test] fn t() {{ println!(\"x\"); }} }}\n"),
    )
    .unwrap();
    assert!(violations(work.path()).is_empty(), "the sink, a string literal and a test module are not raw writes");
}

fn almide(dir: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .output()
        .expect("almide runs")
}

/// c2 end to end: user text that no caller escapes on its own — a FRESH file
/// name holding hidden characters, echoed by the refusal that names it — is
/// escaped by the sink on every route that reads a file. The control is the
/// same refusal for a plain name, which names it as written.
#[test]
fn novel_a_fresh_path_with_a_hidden_character_is_echoed_escaped() {
    let work = tempfile::TempDir::new().unwrap();
    let hidden = ['\u{202E}', '\u{200B}', '\u{E0041}', '\u{1B}'];
    for c in hidden {
        let stem = novel::ident("p");
        let name = format!("{stem}{c}x.almd");
        // The file exists and is refused (a type error), and a missing twin
        // is refused at the read: both refusals name the path.
        std::fs::write(work.path().join(&name), "fn main() -> Unit = 1\n").unwrap();
        let q = novel::ident("q");
        let missing = format!("{q}{c}y.almd");
        for (file, args) in [
            (&name, vec!["check"]),
            (&name, vec!["check", "--json"]),
            (&name, vec!["run"]),
            (&name, vec!["build"]),
            (&missing, vec!["check"]),
            (&missing, vec!["run"]),
            (&missing, vec!["test"]),
        ] {
            let mut a = args.clone();
            a.push(file.as_str());
            let o = almide(work.path(), &a);
            let t = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            assert!(!o.status.success(), "`almide {}` admitted:\n{}", a.join(" "), t.escape_debug());
            assert!(!t.contains(c), "U+{:04X}: `almide {}` printed it raw:\n{}", c as u32, args.join(" "), t.escape_debug());
            if args == ["check"] && file == &missing {
                assert!(t.contains(&q), "the refusal does not name the file:\n{t}");
            }
        }
        // `fmt --check` admits the file (it formats; it does not type-check),
        // and its report names it: escaped too.
        let o = almide(work.path(), &["fmt", "--check", &name]);
        let t = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        assert!(!t.contains(c), "U+{:04X}: `almide fmt --check` printed it raw:\n{}", c as u32, t.escape_debug());
    }
    let plain = format!("{}.almd", novel::ident("plain"));
    let o = almide(work.path(), &["check", &plain]);
    let t = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert!(!o.status.success() && t.contains(&plain), "control: a missing plain file is named as written:\n{t}");
}

/// A JSON document through the sink stays one: a string literal holding a
/// hidden character (written as its escape in the source) reaches
/// `--emit-ast` as a JSON string, written `\uXXXX` — parseable, no raw
/// character — and decodes back to the character.
#[test]
fn a_json_document_through_the_sink_still_parses_to_what_it_said() {
    let work = tempfile::TempDir::new().unwrap();
    let f = format!("{}.almd", novel::ident("j"));
    std::fs::write(work.path().join(&f), "effect fn main() -> Unit = println(\"a\\u{202E}b\")\n").unwrap();
    let o = almide(work.path(), &["emit", &f, "--emit-ast"]);
    let t = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(!t.contains('\u{202E}'), "printed raw:\n{}", t.escape_debug());
    let v: serde_json::Value = serde_json::from_str(&t).expect("still JSON");
    assert!(v.to_string().contains('\u{202E}'), "the string no longer decodes to its character");
}
