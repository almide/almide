use std::io::{self, Write, BufRead};
use std::path::PathBuf;
use crate::{out, out_no_nl, err};

pub fn run_repl() {
    out(&format!("Almide REPL v{} — type expressions to evaluate, :q to quit",
             env!("CARGO_PKG_VERSION")));
    out("");

    let mut session = Session::new();
    let stdin = io::stdin();

    loop {
        out_no_nl(&format!(">>> "));
        io::stdout().flush().ok();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let input = line.trim();
        if input.is_empty() { continue; }

        if input.starts_with(':') {
            match input {
                ":q" | ":quit" => break,
                ":h" | ":help" => print_help(),
                ":history" => session.print_history(),
                ":clear" => session.clear(),
                _ => out(&format!("Unknown command: {}. Type :h for help.", input)),
            }
            continue;
        }

        session.eval(input);
    }
}

enum Kind { TopLevel, Body, Expr }

/// `classify`'s top-level-declaration prefix check. Converted to a data
/// table + linear scan (same technique as `lookup_keyword_info` in
/// lsp_hover_definition.rs) — cyclomatic complexity counts each `||` branch, so a flat
/// table genuinely lowers it rather than just moving it around.
fn is_top_level_prefix(t: &str) -> bool {
    const PREFIXES: &[&str] = &["fn ", "effect fn ", "type ", "mod type ", "import "];
    PREFIXES.iter().any(|p| t.starts_with(p))
}

/// `classify`'s statement-prefix check (`let`/`var`/`for`). Same table
/// technique as `is_top_level_prefix`.
fn is_body_prefix(t: &str) -> bool {
    const PREFIXES: &[&str] = &["let ", "var ", "for "];
    PREFIXES.iter().any(|p| t.starts_with(p))
}

/// `classify`'s assignment-detection scan: `ident = expr` (not `==` or
/// `=>`). Extracted verbatim.
fn looks_like_assignment(t: &str) -> bool {
    let bytes = t.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.') {
        i += 1;
    }
    if i == 0 {
        return false;
    }
    while i < bytes.len() && bytes[i] == b' ' { i += 1; }
    i < bytes.len() && bytes[i] == b'='
        && bytes.get(i + 1) != Some(&b'=')
        && bytes.get(i + 1) != Some(&b'>')
}

fn classify(input: &str) -> Kind {
    let t = input.trim_start();
    if is_top_level_prefix(t) {
        return Kind::TopLevel;
    }
    if is_body_prefix(t) || looks_like_assignment(t) {
        return Kind::Body;
    }
    Kind::Expr
}

struct Session {
    top: Vec<String>,
    body: Vec<String>,
    history: Vec<String>,
    repl_dir: PathBuf,
    build_dir: PathBuf,
}

impl Session {
    fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let repl_dir = PathBuf::from(&home).join(".almide/repl");
        let build_dir = repl_dir.join("build");
        std::fs::create_dir_all(&build_dir).ok();
        Self { top: vec![], body: vec![], history: vec![], repl_dir, build_dir }
    }

    fn eval(&mut self, input: &str) {
        self.history.push(input.to_string());
        match classify(input) {
            Kind::TopLevel => self.eval_top(input),
            Kind::Body => self.eval_body(input),
            Kind::Expr => self.eval_expr(input),
        }
    }

    fn eval_top(&mut self, input: &str) {
        let mut new_top = self.top.clone();
        new_top.push(input.to_string());
        let source = build_program(&new_top, &self.body, None);
        if self.compile_quiet(&source).is_ok() {
            self.top.push(input.to_string());
        }
    }

    fn eval_body(&mut self, input: &str) {
        let mut new_body = self.body.clone();
        new_body.push(input.to_string());
        let source = build_program(&self.top, &new_body, None);
        if self.compile_quiet(&source).is_ok() {
            self.body.push(input.to_string());
        }
    }

    fn eval_expr(&mut self, input: &str) {
        let source = build_program(&self.top, &self.body, Some(input));
        // #1490: rustc-free fast path FIRST — the session program runs on
        // the embedded wasm host (the same leg `almide run --target wasm`
        // uses), so the common REPL line answers in milliseconds with no
        // cargo in the loop. A shape the leg cannot lower falls back to
        // the rustc path silently; both paths print the language's own
        // `${expr}` rendering, so the answer is path-independent.
        match self.run_wasm_fast(&source) {
            Some(Ok(result)) => {
                let result = result.trim();
                if !result.is_empty() {
                    out(result);
                }
                return;
            }
            Some(Err(runtime_err)) => {
                // The program RAN and failed (abort, error propagation):
                // that verdict is real on either path — report, no fallback.
                if !runtime_err.is_empty() {
                    err(runtime_err.trim());
                }
                return;
            }
            None => {} // wall / any wasm-path refusal: the rustc path decides
        }
        match self.compile_and_run(&source) {
            Ok(result) => {
                let result = result.trim();
                if !result.is_empty() {
                    out(&format!("{}", result));
                }
            }
            Err(_) => {} // errors already printed by compiler / cargo
        }
    }

    /// The wasm fast path, as a SUBPROCESS of this same binary (`almide
    /// run <session> --target wasm`): the wall/refusal chatter of a
    /// declined shape stays captured instead of leaking into the session.
    /// `None` = the leg declined (fall back); `Some(Ok)` = stdout;
    /// `Some(Err)` = the program ran and failed (a real verdict).
    fn run_wasm_fast(&self, source: &str) -> Option<Result<String, String>> {
        let path = self.source_path();
        std::fs::write(&path, source).ok()?;
        let exe = std::env::current_exe().ok()?;
        let out = std::process::Command::new(exe)
            .args(["run", path.to_str()?, "--target", "wasm"])
            .output()
            .ok()?;
        let stderr = String::from_utf8_lossy(&out.stderr);
        if out.status.success() {
            return Some(Ok(String::from_utf8_lossy(&out.stdout).to_string()));
        }
        // The leg names its refusals ("wall: …", "error: …" at emit); a
        // RUNTIME failure prints the program's own abort ("Error: …").
        // Only the latter is a verdict; everything else falls back.
        if stderr.starts_with("Error: ") {
            return Some(Err(stderr.to_string()));
        }
        None
    }

    fn compile(&self, source: &str) -> Result<String, String> {
        let path = self.source_path();
        std::fs::write(&path, source).map_err(|e| e.to_string())?;
        let path_str = path.to_str().ok_or_else(|| format!("REPL source path is not valid UTF-8: {}", path.display()))?;
        crate::try_compile(path_str, false)
    }

    fn compile_quiet(&self, source: &str) -> Result<String, String> {
        crate::SUPPRESS_WARNINGS.store(true, std::sync::atomic::Ordering::Relaxed);
        let result = self.compile(source);
        crate::SUPPRESS_WARNINGS.store(false, std::sync::atomic::Ordering::Relaxed);
        result
    }

    fn compile_and_run(&self, source: &str) -> Result<String, String> {
        let rust_code = self.compile_quiet(source)?;
        // NOTE: this used to patch the emitted print to Debug format
        // ({:?}) "so List, records etc. work" — but `${expr}` interpolation
        // renders those natively on both targets now, and the patch made
        // the rustc path's answer DIFFER from the wasm fast path's (a
        // String answered `"hi"` here and `hi` there). One rendering — the
        // language's own — on both paths.
        let bin = super::cargo_build_generated(&rust_code, &self.build_dir, false)?;
        let output = std::process::Command::new(&bin)
            .output()
            .map_err(|e| format!("execution failed: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                err(&format!("{}", stderr.trim()));
            }
            return Err("runtime error".into());
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn source_path(&self) -> PathBuf {
        // Per-process: two concurrent REPLs (parallel tests, two shells)
        // sharing one fixed file clobbered each other's session between
        // the write and the subprocess read — the fast path then ran the
        // OTHER session's program and answered nothing.
        self.repl_dir.join(format!("repl-{}.almd", std::process::id()))
    }

    fn print_history(&self) {
        for (i, h) in self.history.iter().enumerate() {
            out(&format!("{:>3}  {}", i + 1, h));
        }
    }

    fn clear(&mut self) {
        self.top.clear();
        self.body.clear();
        out(&format!("Session cleared."));
    }
}

fn build_program(top: &[String], body: &[String], expr: Option<&str>) -> String {
    let mut s = String::new();
    s.push_str("import io\n");
    for decl in top {
        s.push_str(decl);
        s.push('\n');
    }
    s.push_str("\neffect fn main() -> Unit = {\n");
    for line in body {
        s.push_str("  ");
        s.push_str(line);
        s.push('\n');
    }
    if let Some(e) = expr {
        s.push_str("  let __r = ");
        s.push_str(e);
        s.push('\n');
        s.push_str("  io.print(\"${__r}\\n\")\n");
    }
    s.push_str("  io.print(\"\")\n");
    s.push_str("}\n");
    s
}

fn print_help() {
    out(&format!("Commands:"));
    out(&format!("  :q, :quit    Exit"));
    out(&format!("  :h, :help    Show this help"));
    out(&format!("  :history     Show evaluation history"));
    out(&format!("  :clear       Clear session state"));
    out("");
    out(&format!("Examples:"));
    out(&format!("  >>> 1 + 2"));
    out(&format!("  3"));
    out(&format!("  >>> let name = \"world\""));
    out(&format!("  >>> \"Hello, \" + name"));
    out(&format!("  Hello, world"));
}
